"""Unit-level coverage of `ematix_probe.pytest_plugin`.

The pytester-based tests in `test_pytest_plugin*.py` are
integration: they spin an inner pytest run and observe outcomes.
Coverage tooling doesn't always follow plugin code through that
path because plugins are loaded before instrumentation kicks in,
so the integration tests alone leave the plugin module under-
measured.

This module exercises the plugin's internals directly — the
collector hook, the cached run-once behavior, the per-assertion
item dispatch, the failure-formatting branch, and the exception
re-raise path. No pytester involved.
"""

from __future__ import annotations

from datetime import datetime, timezone
from types import SimpleNamespace
from unittest.mock import MagicMock

import pytest

from ematix_probe import probe, source
from ematix_probe.probe import DataProbe
from ematix_probe.pytest_plugin import (
    DataProbeAssertionItem,
    DataProbeCollector,
    pytest_pycollect_makeitem,
)
from ematix_probe.report import AssertionResult, RunReport


def _make_probe() -> DataProbe:
    @probe.data(
        source=source.postgres("postgres://localhost/x"),
        table="t",
    )
    def sample_probe(t):
        t.column("id").not_null()
        t.column("email").not_null()

    return sample_probe


def _report(verdict: str, results: list[AssertionResult]) -> RunReport:
    now = datetime.now(tz=timezone.utc)
    return RunReport(
        probe_name="sample_probe",
        table="t",
        schema=None,
        verdict=verdict,
        assertions=results,
        started_at=now,
        finished_at=now,
    )


def test_pytest_pycollect_makeitem_returns_collector_for_data_probe(monkeypatch):
    pr = _make_probe()
    parent = MagicMock()
    sentinel = object()
    monkeypatch.setattr(
        DataProbeCollector,
        "from_parent",
        classmethod(lambda cls, *a, **kw: sentinel),
    )
    out = pytest_pycollect_makeitem(parent, "sample_probe", pr)
    assert out is sentinel


def test_pytest_pycollect_makeitem_returns_none_for_other():
    parent = MagicMock()
    assert pytest_pycollect_makeitem(parent, "x", 42) is None
    assert pytest_pycollect_makeitem(parent, "f", lambda: None) is None
    assert pytest_pycollect_makeitem(parent, "s", "not a probe") is None


def test_collector_run_probe_caches_successful_report(monkeypatch):
    pr = _make_probe()
    calls = {"n": 0}

    def _counting(self):
        calls["n"] += 1
        return _report(
            "pass",
            [
                AssertionResult(0, "pass", None, "id.not_null"),
                AssertionResult(1, "pass", None, "email.not_null"),
            ],
        )

    monkeypatch.setattr(DataProbe, "run", _counting)

    coll = DataProbeCollector.__new__(DataProbeCollector)
    coll._probe = pr
    coll._cached = None

    r1 = coll.run_probe()
    r2 = coll.run_probe()
    assert r1 is r2
    assert calls["n"] == 1, f"expected 1 run, got {calls['n']}"


def test_collector_run_probe_caches_and_reraises_exceptions(monkeypatch):
    pr = _make_probe()
    calls = {"n": 0}

    def _boom(self):
        calls["n"] += 1
        raise RuntimeError("postgres unreachable")

    monkeypatch.setattr(DataProbe, "run", _boom)
    coll = DataProbeCollector.__new__(DataProbeCollector)
    coll._probe = pr
    coll._cached = None

    with pytest.raises(RuntimeError, match="postgres unreachable"):
        coll.run_probe()
    with pytest.raises(RuntimeError, match="postgres unreachable"):
        coll.run_probe()
    # Second call must not re-execute the failing probe — the
    # cached error is re-raised so all per-assertion items get a
    # consistent signal without re-incurring the underlying cost.
    assert calls["n"] == 1


def test_assertion_item_runtest_passes_for_pass_verdict(monkeypatch):
    pr = _make_probe()
    monkeypatch.setattr(
        DataProbe,
        "run",
        lambda self: _report(
            "pass",
            [
                AssertionResult(0, "pass", None, "id.not_null"),
                AssertionResult(1, "pass", None, "email.not_null"),
            ],
        ),
    )
    coll = DataProbeCollector.__new__(DataProbeCollector)
    coll._probe = pr
    coll._cached = None

    item = DataProbeAssertionItem.__new__(DataProbeAssertionItem)
    item.parent = coll
    item.name = "id.not_null"
    item._index = 0
    # Should not raise — verdict is pass.
    item.runtest()


def test_assertion_item_runtest_raises_for_fail_verdict(monkeypatch):
    pr = _make_probe()
    monkeypatch.setattr(
        DataProbe,
        "run",
        lambda self: _report(
            "fail",
            [
                AssertionResult(0, "fail", "5 nulls found", "id.not_null"),
                AssertionResult(1, "pass", None, "email.not_null"),
            ],
        ),
    )
    coll = DataProbeCollector.__new__(DataProbeCollector)
    coll._probe = pr
    coll._cached = None

    item = DataProbeAssertionItem.__new__(DataProbeAssertionItem)
    item.parent = coll
    item.name = "id.not_null"
    item._index = 0
    with pytest.raises(AssertionError, match="5 nulls found"):
        item.runtest()


def test_assertion_item_runtest_uses_index_fallback_when_name_missing(monkeypatch):
    pr = _make_probe()
    monkeypatch.setattr(
        DataProbe,
        "run",
        lambda self: _report(
            "fail",
            [AssertionResult(0, "fail", None, None)],
        ),
    )
    coll = DataProbeCollector.__new__(DataProbeCollector)
    coll._probe = pr
    coll._cached = None

    item = DataProbeAssertionItem.__new__(DataProbeAssertionItem)
    item.parent = coll
    item.name = "anonymous"
    item._index = 0
    with pytest.raises(AssertionError, match=r"assertion_0:"):
        item.runtest()


def test_assertion_item_reportinfo_includes_probe_and_assertion_names():
    item = DataProbeAssertionItem.__new__(DataProbeAssertionItem)
    item.parent = SimpleNamespace(name="my_probe")
    item.name = "id.not_null"
    item._index = 0
    item.path = "/tmp/some/path.py"
    path, lineno, label = item.reportinfo()
    assert path == "/tmp/some/path.py"
    assert lineno == 0
    assert "my_probe" in label
    assert "id.not_null" in label
