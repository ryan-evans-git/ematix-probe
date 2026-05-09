"""S-9.4 — SQLite run-history persistence (opt-in).

Gives users a single sqlite file they can point pytest / CLI runs
at to accumulate one row per probe execution. Schema is the
substrate for v0.2 drift detection (per PRD §3 non-goals);
keeping it minimal so additive columns later don't break existing
files.
"""

from __future__ import annotations

import sqlite3
from datetime import datetime, timezone
from pathlib import Path

from ematix_probe.report import AssertionResult, RunReport
from ematix_probe.run_history import RunHistory


def _report(verdict: str = "pass") -> RunReport:
    now = datetime(2026, 5, 9, 12, 0, 0, tzinfo=timezone.utc)
    return RunReport(
        probe_name="customer_dim_quality",
        table="dim_customers",
        schema="public",
        verdict=verdict,
        assertions=[
            AssertionResult(0, "pass", None, "id.not_null"),
            AssertionResult(1, verdict, "5 violations" if verdict == "fail" else None,
                            "email.not_null"),
        ],
        started_at=now,
        finished_at=now,
    )


def test_init_creates_schema(tmp_path: Path):
    db = tmp_path / "history.sqlite"
    RunHistory(db)
    # Two tables: runs + assertions, joined on run_id.
    with sqlite3.connect(db) as conn:
        rows = {
            r[0]
            for r in conn.execute(
                "SELECT name FROM sqlite_master WHERE type='table'"
            )
        }
    assert "runs" in rows
    assert "assertions" in rows


def test_record_persists_one_row_per_run(tmp_path: Path):
    db = tmp_path / "history.sqlite"
    h = RunHistory(db)
    h.record(_report())
    h.record(_report(verdict="fail"))
    with sqlite3.connect(db) as conn:
        run_count = conn.execute("SELECT COUNT(*) FROM runs").fetchone()[0]
    assert run_count == 2


def test_record_persists_one_row_per_assertion(tmp_path: Path):
    db = tmp_path / "history.sqlite"
    h = RunHistory(db)
    h.record(_report())  # 2 assertions
    h.record(_report())  # 2 more
    with sqlite3.connect(db) as conn:
        assertion_count = conn.execute("SELECT COUNT(*) FROM assertions").fetchone()[0]
    assert assertion_count == 4


def test_recorded_row_carries_probe_metadata_and_verdict(tmp_path: Path):
    db = tmp_path / "history.sqlite"
    h = RunHistory(db)
    h.record(_report(verdict="fail"))
    with sqlite3.connect(db) as conn:
        run = conn.execute(
            "SELECT probe_name, table_name, schema_name, verdict, started_at "
            "FROM runs"
        ).fetchone()
    name, table, schema, verdict, started_at = run
    assert name == "customer_dim_quality"
    assert table == "dim_customers"
    assert schema == "public"
    assert verdict == "fail"
    # ISO 8601 with TZ — sortable, no ambiguity.
    assert started_at == "2026-05-09T12:00:00+00:00"


def test_assertion_rows_link_back_to_run(tmp_path: Path):
    db = tmp_path / "history.sqlite"
    h = RunHistory(db)
    h.record(_report(verdict="fail"))
    with sqlite3.connect(db) as conn:
        rows = list(
            conn.execute(
                "SELECT a.assertion_index, a.verdict, a.message, a.name "
                "FROM assertions a JOIN runs r ON a.run_id = r.id "
                "ORDER BY a.assertion_index"
            )
        )
    assert rows == [
        (0, "pass", None, "id.not_null"),
        (1, "fail", "5 violations", "email.not_null"),
    ]


def test_repeated_init_is_idempotent(tmp_path: Path):
    db = tmp_path / "history.sqlite"
    h = RunHistory(db)
    h.record(_report())
    # Reopening must not wipe existing rows.
    h2 = RunHistory(db)
    h2.record(_report())
    with sqlite3.connect(db) as conn:
        assert conn.execute("SELECT COUNT(*) FROM runs").fetchone()[0] == 2
