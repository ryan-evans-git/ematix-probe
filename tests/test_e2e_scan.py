"""S-4.7 — end-to-end Python `DataProbe.run()` against the
scan-path adapters (DuckDB in-memory, local Parquet).

Mirrors `tests/test_e2e_postgres.py` but no testcontainer needed
— DuckDB is in-process and Parquet seeding goes through DuckDB's
COPY TO statement, so neither test depends on Docker.
"""

from __future__ import annotations

from pathlib import Path

from ematix_probe import probe, source
from ematix_probe._core import duckdb_setup


def _seed_duckdb(url: str, sql: str) -> None:
    """Run a DDL/DML batch via the pyo3 helper."""
    duckdb_setup(url, sql)


def test_duckdb_probe_passes_when_data_is_clean(tmp_path: Path) -> None:
    db = str(tmp_path / "clean.duckdb")
    _seed_duckdb(
        db,
        """
        CREATE TABLE users (
            id BIGINT,
            email VARCHAR,
            age DOUBLE
        );
        INSERT INTO users VALUES
            (1, 'a@x.com', 25.0),
            (2, 'b@y.org', 40.0),
            (3, 'c@z.io',  33.0);
        """,
    )

    @probe.data(source=source.duckdb(db), table="users")
    def quality(t):
        t.column("email").not_null()
        t.column("id").unique()
        t.column("age").between(0, 120)

    report = quality.run()
    assert report.verdict == "pass", f"unexpected: {report!r}"
    assert len(report.assertions) == 3


def test_duckdb_probe_fails_when_data_is_dirty(tmp_path: Path) -> None:
    db = str(tmp_path / "dirty.duckdb")
    _seed_duckdb(
        db,
        """
        CREATE TABLE users (id BIGINT, email VARCHAR, age DOUBLE);
        INSERT INTO users VALUES
            (1, 'a@x.com',  25.0),
            (2, NULL,       40.0),
            (1, 'c@z.io',  200.0);
        """,
    )

    @probe.data(source=source.duckdb(db), table="users")
    def quality(t):
        t.column("email").not_null()
        t.column("id").unique()
        t.column("age").between(0, 120)

    report = quality.run()
    assert report.verdict == "fail"
    assert all(a.verdict == "fail" for a in report.assertions)


def test_duckdb_composite_unique_passes_on_valid_composite_key(tmp_path: Path) -> None:
    # order_id repeats across lines and line_no repeats across orders —
    # each column is NON-unique — but the (order_id, line_no) tuple is
    # unique. A per-column unique would wrongly fail; the composite must
    # pass.
    db = str(tmp_path / "ok.duckdb")
    _seed_duckdb(
        db,
        """
        CREATE TABLE order_lines (order_id BIGINT, line_no BIGINT, qty BIGINT);
        INSERT INTO order_lines VALUES
            (1, 1, 5), (1, 2, 3),
            (2, 1, 9), (2, 2, 1);
        """,
    )

    @probe.data(source=source.duckdb(db), table="order_lines")
    def quality(t):
        t.unique(["order_id", "line_no"])

    report = quality.run()
    assert report.verdict == "pass", f"unexpected: {report!r}"
    assert len(report.assertions) == 1


def test_duckdb_composite_unique_fails_on_duplicate_combination(tmp_path: Path) -> None:
    db = str(tmp_path / "dup.duckdb")
    _seed_duckdb(
        db,
        """
        CREATE TABLE order_lines (order_id BIGINT, line_no BIGINT, qty BIGINT);
        INSERT INTO order_lines VALUES
            (1, 1, 5), (1, 2, 3),
            (1, 1, 7);   -- (1,1) duplicated
        """,
    )

    @probe.data(source=source.duckdb(db), table="order_lines")
    def quality(t):
        t.unique(["order_id", "line_no"])

    report = quality.run()
    assert report.verdict == "fail"
    assert report.assertions[0].verdict == "fail"


def test_duckdb_composite_unique_mixed_types(tmp_path: Path) -> None:
    # Composite key over an Int64 + a Utf8 column.
    db = str(tmp_path / "mixed.duckdb")
    _seed_duckdb(
        db,
        """
        CREATE TABLE memberships (tenant_id BIGINT, email VARCHAR);
        INSERT INTO memberships VALUES
            (1, 'a@x.com'), (1, 'b@x.com'), (2, 'a@x.com');
        """,
    )

    @probe.data(source=source.duckdb(db), table="memberships")
    def quality(t):
        t.unique(["tenant_id", "email"])

    report = quality.run()
    assert report.verdict == "pass", f"unexpected: {report!r}"


def test_parquet_probe_runs_against_seeded_file(tmp_path: Path) -> None:
    # Seed a parquet file by having DuckDB write it via COPY TO.
    db = str(tmp_path / "seed.duckdb")
    parquet = str(tmp_path / "users.parquet")
    _seed_duckdb(
        db,
        f"""
        CREATE TABLE users (id BIGINT, email VARCHAR, age DOUBLE);
        INSERT INTO users VALUES
            (1, 'a@x.com', 25.0),
            (2, 'b@y.org', 40.0);
        COPY users TO '{parquet}' (FORMAT PARQUET);
        """,
    )

    @probe.data(source=source.parquet(parquet), table="users")
    def quality(t):
        t.column("email").not_null()
        t.column("id").unique()
        t.column("age").between(0, 120)

    report = quality.run()
    assert report.verdict == "pass", f"unexpected: {report!r}"
