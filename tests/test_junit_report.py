"""S-3.5 — JUnit XML report writer.

The Python ``RunReport`` exposes ``write_junit(path)`` that drops a
GitHub-Actions-compatible JUnit file:
- one ``<testsuite>`` per probe
- one ``<testcase>`` per assertion
- ``<failure>`` child on Fail, ``<error>`` child on Error,
  no children on Pass

Tests construct ``RunReport`` instances directly (bypassing
Postgres) so the writer can be unit-tested without testcontainers.
"""

from __future__ import annotations

import xml.etree.ElementTree as ET
from datetime import datetime, timezone
from pathlib import Path

import pytest
from ematix_probe.report import AssertionResult, RunReport


def _make_report(
    *,
    probe_name: str = "customer_quality",
    table: str = "users",
    schema: str | None = None,
    verdict: str = "fail",
    assertions: list[AssertionResult] | None = None,
) -> RunReport:
    started = datetime(2026, 5, 6, 12, 0, 0, tzinfo=timezone.utc)
    finished = datetime(2026, 5, 6, 12, 0, 1, 250000, tzinfo=timezone.utc)
    return RunReport(
        probe_name=probe_name,
        table=table,
        schema=schema,
        verdict=verdict,
        assertions=assertions
        or [
            AssertionResult(
                assertion_index=0,
                name="email.not_null",
                verdict="pass",
                message=None,
            ),
            AssertionResult(
                assertion_index=1,
                name="email.regex",
                verdict="fail",
                message="column \"email\" has 1 row(s) not matching pattern",
            ),
        ],
        started_at=started,
        finished_at=finished,
    )


class TestWriteJunit:
    def test_emits_parseable_xml_with_one_testsuite_per_probe(self, tmp_path: Path) -> None:
        report = _make_report()
        out = tmp_path / "junit.xml"
        report.write_junit(out)

        tree = ET.parse(out)
        root = tree.getroot()

        suites = root.findall(".//testsuite") if root.tag != "testsuite" else [root]
        assert len(suites) == 1, f"expected exactly one <testsuite>, got {len(suites)}"

    def test_one_testcase_per_assertion(self, tmp_path: Path) -> None:
        report = _make_report()
        out = tmp_path / "junit.xml"
        report.write_junit(out)

        tree = ET.parse(out)
        cases = tree.findall(".//testcase")
        assert len(cases) == 2

    def test_failure_element_on_fail(self, tmp_path: Path) -> None:
        report = _make_report()
        out = tmp_path / "junit.xml"
        report.write_junit(out)

        tree = ET.parse(out)
        cases = tree.findall(".//testcase")
        # First case is "email.not_null" (pass) → no children.
        # Second case is "email.regex" (fail) → has <failure>.
        pass_case = next(c for c in cases if c.get("name") == "email.not_null")
        fail_case = next(c for c in cases if c.get("name") == "email.regex")

        assert pass_case.find("failure") is None
        assert pass_case.find("error") is None

        failure = fail_case.find("failure")
        assert failure is not None, "Fail assertion should have <failure> child"
        # The Postgres adapter's failure message should land in the
        # <failure> body so a CI viewer surfaces the diagnostic.
        assert failure.text and "row(s) not matching" in failure.text

    def test_error_element_on_error(self, tmp_path: Path) -> None:
        report = _make_report(
            verdict="error",
            assertions=[
                AssertionResult(
                    assertion_index=0,
                    name="email.regex",
                    verdict="error",
                    message="connection refused",
                ),
            ],
        )
        out = tmp_path / "junit.xml"
        report.write_junit(out)

        tree = ET.parse(out)
        case = tree.find(".//testcase")
        assert case is not None
        error = case.find("error")
        assert error is not None, "Error assertion should have <error> child"
        assert error.text and "connection refused" in error.text

    def test_testsuite_carries_counts_and_name(self, tmp_path: Path) -> None:
        report = _make_report()
        out = tmp_path / "junit.xml"
        report.write_junit(out)

        tree = ET.parse(out)
        root = tree.getroot()
        suite = root if root.tag == "testsuite" else root.find(".//testsuite")
        assert suite is not None

        assert suite.get("name") == "customer_quality"
        assert suite.get("tests") == "2"
        assert suite.get("failures") == "1"
        assert suite.get("errors") == "0"

    def test_accepts_str_path(self, tmp_path: Path) -> None:
        report = _make_report()
        out_str = str(tmp_path / "junit.xml")
        report.write_junit(out_str)
        # Just verify the file exists + parses
        ET.parse(out_str)

    def test_assertion_with_no_name_falls_back_to_index(self, tmp_path: Path) -> None:
        report = _make_report(
            assertions=[
                AssertionResult(
                    assertion_index=0,
                    name=None,
                    verdict="pass",
                    message=None,
                ),
            ],
        )
        out = tmp_path / "junit.xml"
        report.write_junit(out)

        tree = ET.parse(out)
        case = tree.find(".//testcase")
        assert case is not None
        # Should have *some* name attribute; index-based fallback is fine.
        assert case.get("name")
