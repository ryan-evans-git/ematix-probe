"""S-9.2 — per-assertion test reporting.

S-9.1 surfaces one pytest item per probe; this story splits that
into one item per assertion so a probe with N assertions becomes
N pytest test nodes (parametrize-style). Side benefits: each
failing assertion shows up as its own red test node in CI, and
the run pivots from "the probe failed" to "this specific
assertion failed".

Stub strategy: `pytester.runpytest` runs in-process, so the inner
pytest run sees the same `ematix_probe.probe.DataProbe` object as
the outer process. We monkeypatch `DataProbe.run` from the *outer*
test (where pytest's `monkeypatch` fixture properly restores it on
teardown). Doing the monkeypatch inside the synthesized test file
would leak the fake into other suite tests because our custom
`DataProbeAssertionItem` doesn't run pytest fixtures.
"""

from __future__ import annotations

from datetime import datetime, timezone

from ematix_probe.probe import DataProbe
from ematix_probe.report import AssertionResult, RunReport

pytest_plugins = ["pytester"]


_CONFTEST = 'pytest_plugins = ["ematix_probe.pytest_plugin"]\n'


def _make_report(probe_name: str, verdict: str, results: list[AssertionResult]) -> RunReport:
    now = datetime.now(tz=timezone.utc)
    return RunReport(
        probe_name=probe_name,
        table="t",
        schema=None,
        verdict=verdict,
        assertions=results,
        started_at=now,
        finished_at=now,
    )


def test_one_pytest_node_per_assertion(pytester, monkeypatch):
    monkeypatch.setattr(
        DataProbe,
        "run",
        lambda self: _make_report(
            "multi_assertion_check",
            "pass",
            [
                AssertionResult(0, "pass", None, "id.not_null"),
                AssertionResult(1, "pass", None, "id.unique"),
                AssertionResult(2, "pass", None, "age.between"),
            ],
        ),
    )
    pytester.makeconftest(_CONFTEST)
    pytester.makepyfile(
        """
        from ematix_probe import probe, source

        @probe.data(
            source=source.postgres("postgres://localhost/x"),
            table="t",
        )
        def multi_assertion_check(t):
            t.column("id").not_null()
            t.column("id").unique()
            t.column("age").between(0, 120)
        """
    )
    result = pytester.runpytest("-v")
    result.assert_outcomes(passed=3)
    result.stdout.fnmatch_lines(["*multi_assertion_check*id.not_null*"])
    result.stdout.fnmatch_lines(["*multi_assertion_check*id.unique*"])
    result.stdout.fnmatch_lines(["*multi_assertion_check*age.between*"])


def test_failing_assertion_fails_just_its_node(pytester, monkeypatch):
    monkeypatch.setattr(
        DataProbe,
        "run",
        lambda self: _make_report(
            "mixed",
            "fail",
            [
                AssertionResult(0, "pass", None, "id.not_null"),
                AssertionResult(1, "fail", "5 null values found", "name.not_null"),
            ],
        ),
    )
    pytester.makeconftest(_CONFTEST)
    pytester.makepyfile(
        """
        from ematix_probe import probe, source

        @probe.data(
            source=source.postgres("postgres://localhost/x"),
            table="t",
        )
        def mixed(t):
            t.column("id").not_null()
            t.column("name").not_null()
        """
    )
    result = pytester.runpytest("-v")
    result.assert_outcomes(passed=1, failed=1)
    result.stdout.fnmatch_lines(["*5 null values found*"])


def test_run_is_called_only_once_per_probe(pytester, monkeypatch):
    # Per-assertion fan-out should not multiply DB / HTTP work.
    # The collector caches the RunReport so all N items read from
    # one execution.
    call_count = {"n": 0}

    def _counting(self):
        call_count["n"] += 1
        return _make_report(
            "counted",
            "pass",
            [
                AssertionResult(0, "pass", None, "id.not_null"),
                AssertionResult(1, "pass", None, "name.not_null"),
                AssertionResult(2, "pass", None, "age.between"),
            ],
        )

    monkeypatch.setattr(DataProbe, "run", _counting)
    pytester.makeconftest(_CONFTEST)
    pytester.makepyfile(
        """
        from ematix_probe import probe, source

        @probe.data(
            source=source.postgres("postgres://localhost/x"),
            table="t",
        )
        def counted(t):
            t.column("id").not_null()
            t.column("name").not_null()
            t.column("age").between(0, 120)
        """
    )
    result = pytester.runpytest("-v")
    result.assert_outcomes(passed=3)
    assert call_count["n"] == 1, (
        f"expected DataProbe.run() called once across the 3 per-assertion "
        f"items; got {call_count['n']}"
    )
