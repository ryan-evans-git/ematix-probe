"""pytest plugin: discover ematix-probe `DataProbe` instances at
module top level and surface them as pytest collection items.

Wired through the `pytest11` entry point in pyproject.toml so a
`pip install ematix-probe` activates the plugin automatically;
users can also load it explicitly via `pytest_plugins =
["ematix_probe.pytest_plugin"]` in a conftest, which is what the
plugin's own tests do.

Layout (S-9.2): each `@probe.data` becomes a `DataProbeCollector`
which yields one `DataProbeAssertionItem` per assertion in the
probe's plan. The collector caches the `RunReport` so the
underlying `probe.run()` is invoked exactly once even when a probe
has N assertions — fan-out must not multiply DB / HTTP work.
"""

from __future__ import annotations

import pytest

from ematix_probe.probe import DataProbe
from ematix_probe.report import RunReport


def pytest_pycollect_makeitem(collector, name, obj):
    """Per-attribute collection hook. Returns a `DataProbeCollector`
    for any module-level `DataProbe`, which then yields per-assertion
    items. Returning `None` lets pytest's default collection
    proceed for everything else."""
    if isinstance(obj, DataProbe):
        return DataProbeCollector.from_parent(collector, name=name, probe=obj)
    return None


class DataProbeCollector(pytest.Collector):
    """Pytest collector wrapping a single `DataProbe`. Yields one
    `DataProbeAssertionItem` per assertion. Holds the lazily-cached
    `RunReport` (or any exception raised by `.run()`) so each
    assertion item reads from a single execution."""

    def __init__(self, *, probe: DataProbe, **kwargs) -> None:
        super().__init__(**kwargs)
        self._probe = probe
        self._cached: tuple[str, RunReport | BaseException] | None = None

    def collect(self):
        for idx, name in enumerate(self._probe.assertion_names()):
            yield DataProbeAssertionItem.from_parent(
                self,
                name=name,
                index=idx,
            )

    def run_probe(self) -> RunReport:
        """Run the probe at most once and memoize the outcome. If
        `.run()` raised, the same exception is re-raised on every
        call so each assertion item gets a consistent error rather
        than only the first item failing and the rest being skipped
        with no signal."""
        if self._cached is None:
            try:
                report = self._probe.run()
            except BaseException as exc:  # noqa: BLE001 — re-raise verbatim
                self._cached = ("err", exc)
            else:
                self._cached = ("ok", report)
        kind, payload = self._cached
        if kind == "err":
            assert isinstance(payload, BaseException)
            raise payload
        assert isinstance(payload, RunReport)
        return payload


class DataProbeAssertionItem(pytest.Item):
    """One pytest test node per probe assertion. `runtest` reads the
    cached `RunReport` from the parent collector, finds this item's
    assertion result by index, and raises `AssertionError` if it
    didn't pass."""

    def __init__(self, *, index: int, **kwargs) -> None:
        super().__init__(**kwargs)
        self._index = index

    def runtest(self) -> None:
        report = self.parent.run_probe()
        result = next(
            a for a in report.assertions if a.assertion_index == self._index
        )
        if result.verdict == "pass":
            return
        label = result.name or f"assertion_{self._index}"
        raise AssertionError(
            f"{label}: {result.verdict} — {result.message or 'no message'}"
        )

    def reportinfo(self):
        return self.path, 0, f"data_probe::{self.parent.name}::{self.name}"
