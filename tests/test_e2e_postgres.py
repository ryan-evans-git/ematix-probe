"""S-2.7 — end-to-end Python probe execution against real Postgres.

Spins a Postgres testcontainer, seeds a small table, declares a probe
via the `@probe.data` decorator, runs it, and asserts on the
returned `RunReport`. Exercises the full path:
    Python decorator → pyo3 → core engine → PostgresAdapter → SQL
    → AssertionResult → pyo3 → Python.

Tests are tagged `e2e` (custom marker) so unit-only runs can skip
them via `-m "not e2e"`. CI runs everything.
"""

from __future__ import annotations

import psycopg2
import pytest
from ematix_probe import probe, source
from testcontainers.postgres import PostgresContainer


@pytest.fixture(scope="module")
def postgres_url() -> str:
    """Module-scoped: one container shared by all e2e tests in this
    file. Each test creates and drops its own table to stay isolated.
    Container is stopped automatically when the fixture is torn down.

    `get_connection_url()` returns a SQLAlchemy-flavored URL like
    `postgresql+psycopg2://...`; we strip the `+psycopg2` so it
    parses as a libpq URL inside the Rust adapter.
    """
    with PostgresContainer("postgres:16-alpine") as pg:
        url = pg.get_connection_url().replace("+psycopg2", "")
        yield url


def _seed(url: str, ddl: str) -> None:
    """Run a setup DDL/DML batch via psycopg2 (separate from the
    adapter's pool, for test isolation)."""
    with psycopg2.connect(url) as conn:
        conn.autocommit = True
        with conn.cursor() as cur:
            cur.execute(ddl)


def test_probe_runs_and_passes_when_data_is_clean(postgres_url: str) -> None:
    _seed(
        postgres_url,
        """
        DROP TABLE IF EXISTS users_clean;
        CREATE TABLE users_clean (
            id SERIAL PRIMARY KEY,
            email TEXT NOT NULL,
            user_id INT NOT NULL,
            age INT NOT NULL
        );
        INSERT INTO users_clean (email, user_id, age) VALUES
            ('a@x', 1, 25),
            ('b@y', 2, 40),
            ('c@z', 3, 33);
        """,
    )

    @probe.data(source=source.postgres(postgres_url), table="users_clean")
    def quality(t):
        t.column("email").not_null()
        t.column("user_id").unique()
        t.column("age").between(0, 120)

    report = quality.run()
    assert report.verdict == "pass", f"unexpected verdict: {report!r}"
    assert len(report.assertions) == 3
    assert all(a.verdict == "pass" for a in report.assertions)


def test_probe_runs_and_fails_when_data_is_dirty(postgres_url: str) -> None:
    _seed(
        postgres_url,
        """
        DROP TABLE IF EXISTS users_dirty;
        CREATE TABLE users_dirty (
            id SERIAL PRIMARY KEY,
            email TEXT,
            user_id INT,
            age INT
        );
        INSERT INTO users_dirty (email, user_id, age) VALUES
            ('a@x', 1, 25),
            (NULL, 2, 40),     -- NULL email → not_null fails
            ('b@y', 1, 33),    -- duplicate user_id → unique fails
            ('c@z', 3, 200);   -- 200 > 120 → between fails
        """,
    )

    @probe.data(source=source.postgres(postgres_url), table="users_dirty")
    def quality(t):
        t.column("email").not_null()
        t.column("user_id").unique()
        t.column("age").between(0, 120)

    report = quality.run()
    assert report.verdict == "fail"
    assert len(report.assertions) == 3
    failed_kinds = {
        # PRD §6.1 says assertions report enough detail to debug; for
        # v0.1 we surface the message string.
        a.message
        for a in report.assertions
        if a.verdict == "fail"
    }
    # All three assertions should fail; verify each had a meaningful
    # detail message (strings include column name).
    assert len(failed_kinds) == 3, f"expected 3 distinct fail messages, got: {failed_kinds!r}"
