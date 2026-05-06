"""Python-side run report + machine-readable writers.

The pyo3 ``ematix_probe._core.RunReport`` carries only what the
Rust adapter knows: an overall verdict and a list of per-assertion
verdicts/messages. CI-facing report formats (JUnit XML, JSON) need
more — the probe's name, the table it ran against, when it ran,
and a human label per assertion.

This module wraps the pyo3 result with that presentation metadata.
``DataProbe.run`` is responsible for capturing the metadata at run
time and instantiating ``RunReport`` from the raw pyo3 result.

The wrapper is constructible directly so the writers can be
unit-tested without spinning up Postgres (see
``tests/test_junit_report.py`` and ``tests/test_json_report.py``).
"""

from __future__ import annotations

import xml.etree.ElementTree as ET
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Literal

Verdict = Literal["pass", "fail", "error"]


@dataclass(frozen=True)
class AssertionResult:
    """Per-assertion outcome enriched with a human-readable name.

    ``name`` is what report consumers see (e.g.
    ``"email.not_null"``). When ``None`` the writer falls back to
    ``f"assertion_{assertion_index}"`` so every <testcase> still
    has a stable identifier.
    """

    assertion_index: int
    verdict: Verdict
    message: str | None
    name: str | None = None


@dataclass(frozen=True)
class RunReport:
    probe_name: str
    table: str
    schema: str | None
    verdict: Verdict
    assertions: list[AssertionResult]
    started_at: datetime
    finished_at: datetime
    # Reserved for future use (e.g. adapter URL, plan hash); kept
    # in the constructor signature so adding metadata later doesn't
    # break the dataclass shape.
    metadata: dict[str, str] = field(default_factory=dict)

    @property
    def duration_seconds(self) -> float:
        return (self.finished_at - self.started_at).total_seconds()

    def write_junit(self, path: str | Path) -> None:
        """Serialize this report as JUnit XML at ``path``.

        Schema:
            <testsuites>
              <testsuite name=probe tests=N failures=F errors=E
                         time=DURATION timestamp=ISO>
                <testcase name=ASSERTION classname=PROBE.TABLE>
                  [<failure message=MSG>FULL_MSG</failure>]
                  [<error message=MSG>FULL_MSG</error>]
                </testcase>
                ...
              </testsuite>
            </testsuites>

        Targets the GitHub Actions JUnit reporter format
        (sprint-03 risk #3). Jenkins/GitLab parse the same shape.
        """
        failures = sum(1 for a in self.assertions if a.verdict == "fail")
        errors = sum(1 for a in self.assertions if a.verdict == "error")

        suites = ET.Element("testsuites")
        suite = ET.SubElement(
            suites,
            "testsuite",
            {
                "name": self.probe_name,
                "tests": str(len(self.assertions)),
                "failures": str(failures),
                "errors": str(errors),
                "time": f"{self.duration_seconds:.3f}",
                "timestamp": self.started_at.isoformat(),
            },
        )

        classname = (
            f"{self.schema}.{self.table}" if self.schema else self.table
        )
        for a in self.assertions:
            case_name = a.name or f"assertion_{a.assertion_index}"
            case = ET.SubElement(
                suite,
                "testcase",
                {"name": case_name, "classname": f"{self.probe_name}.{classname}"},
            )
            if a.verdict == "fail":
                child = ET.SubElement(
                    case,
                    "failure",
                    {"message": a.message or "assertion failed"},
                )
                child.text = a.message or "assertion failed"
            elif a.verdict == "error":
                child = ET.SubElement(
                    case,
                    "error",
                    {"message": a.message or "adapter error"},
                )
                child.text = a.message or "adapter error"

        tree = ET.ElementTree(suites)
        # ET.indent (Python 3.9+) makes the file diff-friendly; the
        # cost is one O(n) walk after assembly.
        ET.indent(tree, space="  ")
        tree.write(Path(path), encoding="utf-8", xml_declaration=True)
