"""Opt-in SQLite run-history persistence.

Single sqlite file with two tables (`runs` + `assertions`) — one
row per probe execution and one per assertion result. Designed as
the substrate for v0.2 drift-detection (per PRD §3 non-goals), so
the schema stays minimal and changes additively: never rename a
column, only add new ones with sensible defaults.

Usage::

    from ematix_probe.run_history import RunHistory
    h = RunHistory("history.sqlite")
    h.record(probe.run())

The Python sqlite3 driver is stdlib, so this module adds no new
dependency. Connections open per call (short-lived) — the
expected access pattern is a few inserts per pytest run, not a
hot OLTP loop.
"""

from __future__ import annotations

import sqlite3
from contextlib import contextmanager
from pathlib import Path

from .report import RunReport


class RunHistory:
    """Append-only persister for `RunReport`s.

    The schema is created on construction (idempotent — re-opening
    an existing file is non-destructive). PRAGMA `user_version`
    tracks schema generation so future migrations can detect
    files written by older versions.
    """

    SCHEMA_VERSION = 1

    def __init__(self, path: str | Path) -> None:
        self._path = Path(path)
        self._init_schema()

    @contextmanager
    def _connect(self):
        conn = sqlite3.connect(self._path)
        try:
            # Enforce FK so deleting a run drops its assertions.
            conn.execute("PRAGMA foreign_keys = ON")
            yield conn
        finally:
            conn.close()

    def _init_schema(self) -> None:
        with self._connect() as conn:
            conn.executescript(
                """
                CREATE TABLE IF NOT EXISTS runs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    probe_name TEXT NOT NULL,
                    table_name TEXT NOT NULL,
                    schema_name TEXT,
                    verdict TEXT NOT NULL,
                    started_at TEXT NOT NULL,
                    finished_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS assertions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    run_id INTEGER NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
                    assertion_index INTEGER NOT NULL,
                    verdict TEXT NOT NULL,
                    message TEXT,
                    name TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_assertions_run_id
                    ON assertions(run_id);
                """
            )
            conn.execute(f"PRAGMA user_version = {self.SCHEMA_VERSION}")
            conn.commit()

    def record(self, report: RunReport) -> int:
        """Persist one `RunReport`. Returns the new `runs.id` so
        callers can associate downstream telemetry with the row."""
        with self._connect() as conn:
            cur = conn.execute(
                "INSERT INTO runs (probe_name, table_name, schema_name, "
                "verdict, started_at, finished_at) VALUES (?, ?, ?, ?, ?, ?)",
                (
                    report.probe_name,
                    report.table,
                    report.schema,
                    report.verdict,
                    report.started_at.isoformat(),
                    report.finished_at.isoformat(),
                ),
            )
            run_id = cur.lastrowid
            conn.executemany(
                "INSERT INTO assertions (run_id, assertion_index, verdict, "
                "message, name) VALUES (?, ?, ?, ?, ?)",
                [
                    (run_id, a.assertion_index, a.verdict, a.message, a.name)
                    for a in report.assertions
                ],
            )
            conn.commit()
            assert run_id is not None  # AUTOINCREMENT always assigns
            return run_id
