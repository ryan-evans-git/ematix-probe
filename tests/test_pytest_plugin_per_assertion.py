"""S-9.2 — per-assertion test reporting.

S-9.1 surfaces one pytest item per probe; this story splits that
into one item per assertion so a probe with N assertions becomes
N pytest test nodes (parametrize-style). Side benefits: each
failing assertion shows up as its own red test node in CI, and
the run pivots from "the probe failed" to "this specific
assertion failed".

Tests stub `DataProbe.run` so they don't need a live Postgres —
the plugin's contract is about *how* the report is mapped to
pytest items, not about adapter behavior.
"""

import pytest

pytest_plugins = ["pytester"]


def _conftest_loading_plugin() -> str:
    return 'pytest_plugins = ["ematix_probe.pytest_plugin"]\n'


def test_one_pytest_node_per_assertion(pytester):
    pytester.makeconftest(_conftest_loading_plugin())
    pytester.makepyfile(
        """
        from datetime import datetime, timezone

        from ematix_probe import probe, source
        from ematix_probe.probe import DataProbe
        from ematix_probe.report import AssertionResult, RunReport


        def _fake_run(self):
            return RunReport(
                probe_name="multi_assertion_check",
                table="t",
                schema=None,
                verdict="pass",
                assertions=[
                    AssertionResult(0, "pass", None, "id.not_null"),
                    AssertionResult(1, "pass", None, "id.unique"),
                    AssertionResult(2, "pass", None, "age.between"),
                ],
                started_at=datetime.now(tz=timezone.utc),
                finished_at=datetime.now(tz=timezone.utc),
            )


        DataProbe.run = _fake_run


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
    # Three pytest nodes: one per assertion.
    result.assert_outcomes(passed=3)
    # And each per-assertion node names the underlying assertion
    # in its node id so a CI report links the failure to the check.
    result.stdout.fnmatch_lines(["*multi_assertion_check*id.not_null*"])
    result.stdout.fnmatch_lines(["*multi_assertion_check*id.unique*"])
    result.stdout.fnmatch_lines(["*multi_assertion_check*age.between*"])


def test_failing_assertion_fails_just_its_node(pytester):
    pytester.makeconftest(_conftest_loading_plugin())
    pytester.makepyfile(
        """
        from datetime import datetime, timezone

        from ematix_probe import probe, source
        from ematix_probe.probe import DataProbe
        from ematix_probe.report import AssertionResult, RunReport


        def _fake_run(self):
            return RunReport(
                probe_name="mixed",
                table="t",
                schema=None,
                verdict="fail",
                assertions=[
                    AssertionResult(0, "pass", None, "id.not_null"),
                    AssertionResult(1, "fail", "5 null values found", "name.not_null"),
                ],
                started_at=datetime.now(tz=timezone.utc),
                finished_at=datetime.now(tz=timezone.utc),
            )


        DataProbe.run = _fake_run


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
    # One assertion passed, one failed — independent pytest nodes.
    result.assert_outcomes(passed=1, failed=1)
    # And the failure message must surface the assertion message
    # so the report has enough to act on.
    result.stdout.fnmatch_lines(["*5 null values found*"])


def test_run_is_called_only_once_per_probe(pytester):
    # Per-assertion fan-out should not multiply DB / HTTP work.
    # The collector caches the RunReport so all N items read from
    # one execution.
    pytester.makeconftest(_conftest_loading_plugin())
    pytester.makepyfile(
        """
        from datetime import datetime, timezone

        from ematix_probe import probe, source
        from ematix_probe.probe import DataProbe
        from ematix_probe.report import AssertionResult, RunReport


        _CALL_COUNT = {"n": 0}


        def _counting_run(self):
            _CALL_COUNT["n"] += 1
            return RunReport(
                probe_name="counted",
                table="t",
                schema=None,
                verdict="pass",
                assertions=[
                    AssertionResult(0, "pass", None, "id.not_null"),
                    AssertionResult(1, "pass", None, "name.not_null"),
                    AssertionResult(2, "pass", None, "age.between"),
                ],
                started_at=datetime.now(tz=timezone.utc),
                finished_at=datetime.now(tz=timezone.utc),
            )


        DataProbe.run = _counting_run


        @probe.data(
            source=source.postgres("postgres://localhost/x"),
            table="t",
        )
        def counted(t):
            t.column("id").not_null()
            t.column("name").not_null()
            t.column("age").between(0, 120)


        def test_one_run_for_three_assertions():
            assert _CALL_COUNT["n"] == 1, (
                f"expected DataProbe.run() called once across the 3 "
                f"per-assertion items; got {_CALL_COUNT['n']}"
            )
        """
    )
    result = pytester.runpytest("-v")
    # 3 per-assertion items + 1 sanity test = 4 passed; run() called once.
    result.assert_outcomes(passed=4)
