"""pytest plugin: discover ematix-probe `DataProbe` instances at
module top level and surface them as pytest collection items.

Wired through the `pytest11` entry point in pyproject.toml so a
`pip install ematix-probe` activates the plugin automatically;
users can also load it explicitly via `pytest_plugins =
["ematix_probe.pytest_plugin"]` in a conftest, which is what the
plugin's own tests do.

S-9.1 — scaffolding only: one pytest item per probe, runtest
calls `.run()` and raises AssertionError if the overall verdict
is not pass. S-9.2 splits this into one item per assertion for
finer-grained reporting.
"""

from __future__ import annotations

import pytest

from ematix_probe.probe import DataProbe


def pytest_pycollect_makeitem(collector, name, obj):
    """Per-attribute collection hook. Pytest calls this for each
    module-level attribute it considers; returning a non-None value
    short-circuits the default collection (which would skip
    `DataProbe` since it isn't a function or `Test*` class)."""
    if isinstance(obj, DataProbe):
        return DataProbeItem.from_parent(collector, name=name, probe=obj)
    return None


class DataProbeItem(pytest.Item):
    """A pytest test node wrapping a `DataProbe`. Runs the probe
    and asserts the overall verdict is pass.

    Per-assertion fan-out lands in S-9.2 — until then, a probe
    with N assertions still surfaces as one pytest test, and the
    failure message lists every non-pass assertion so the report
    is actionable."""

    def __init__(self, *, probe: DataProbe, **kwargs) -> None:
        super().__init__(**kwargs)
        self._probe = probe

    def runtest(self) -> None:
        report = self._probe.run()
        if report.verdict == "pass":
            return
        # Build a single message that names each non-pass assertion
        # so the pytest output has enough to act on without forcing
        # the user to dig through a separate report file.
        details = "; ".join(
            f"{a.name or f'assertion_{a.assertion_index}'}: "
            f"{a.message or a.verdict}"
            for a in report.assertions
            if a.verdict != "pass"
        )
        raise AssertionError(f"{self.name} {report.verdict}: {details}")

    def reportinfo(self):
        # Pytest displays this in the verbose progress line.
        return self.path, 0, f"data_probe::{self.name}"
