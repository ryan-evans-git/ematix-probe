"""End-to-end ematix-probe quickstart.

Boots a source (Postgres testcontainer / in-process DuckDB /
local Parquet file), seeds a `users` table that intentionally
violates several quality rules, declares a `@probe.data` covering
all 7 v0.1 assertion types, runs it, and writes JUnit XML + JSON
reports to ``out/``.

Run it from the repo root::

    python examples/quickstart/run.py                # postgres (default)
    python examples/quickstart/run.py --source duckdb
    python examples/quickstart/run.py --source parquet

Source-specific requirements:
- postgres: Docker running locally (testcontainer)
- duckdb:   no external requirements
- parquet:  no external requirements (DuckDB writes the file)

Plus the project's dev extras (``pip install -e '.[dev]'`` or
``maturin develop``).

Exits 0 even on a Fail verdict — this is a demo, not a CI gate.
The reports in ``out/`` show what a downstream CI runner would
consume.
"""

from __future__ import annotations

import argparse
import sys
import tempfile
from pathlib import Path

from ematix_probe import probe, source
from ematix_probe._core import duckdb_setup

OUT_DIR = Path(__file__).parent / "out"

# Postgres flavor (uses TIMESTAMPTZ + SERIAL).
POSTGRES_SEED = """
DROP TABLE IF EXISTS users;
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    email TEXT,
    user_id INT,
    age INT,
    country TEXT,
    updated_at TIMESTAMPTZ DEFAULT now()
);
INSERT INTO users (email, user_id, age, country, updated_at) VALUES
    ('alice@example.com', 1, 30, 'US', now()),
    ('bob@example.com',   2, 45, 'CA', now()),
    ('carol@example.com', 3, 22, 'US', now()),
    -- intentional violations so the report has something to show:
    (NULL,                4, 28, 'US', now()),    -- not_null fail
    ('duplicate-id',      1, 33, 'GB', now()),    -- unique + enum + regex fail
    ('eve@example.com',   6, 200, 'US', now());   -- between fail
"""

# DuckDB flavor — same shape, DuckDB-specific column types.
DUCKDB_SEED = """
CREATE TABLE users (
    id BIGINT,
    email VARCHAR,
    user_id BIGINT,
    age BIGINT,
    country VARCHAR,
    updated_at TIMESTAMP DEFAULT now()
);
INSERT INTO users (id, email, user_id, age, country, updated_at) VALUES
    (1, 'alice@example.com', 1, 30,  'US', now()),
    (2, 'bob@example.com',   2, 45,  'CA', now()),
    (3, 'carol@example.com', 3, 22,  'US', now()),
    (4, NULL,                4, 28,  'US', now()),
    (5, 'duplicate-id',      1, 33,  'GB', now()),
    (6, 'eve@example.com',   6, 200, 'US', now());
"""


def _build_probe(s):
    @probe.data(source=s, table="users")
    def user_quality(t):
        # Column-level checks
        t.column("email").not_null()
        t.column("email").regex(r".+@.+\..+")
        t.column("user_id").unique()
        t.column("age").between(0, 120)
        t.column("country").is_in(["US", "CA", "MX"])
        # Table-level checks
        t.row_count(at_least=1, at_most=1_000_000)
        t.freshness("updated_at", within="24h")

    return user_quality


def _run_postgres():
    import psycopg2
    from testcontainers.postgres import PostgresContainer

    print("Starting Postgres testcontainer...", flush=True)
    with PostgresContainer("postgres:16-alpine") as pg:
        url = pg.get_connection_url().replace("+psycopg2", "")
        print(f"  -> {url}", flush=True)
        with psycopg2.connect(url) as conn:
            conn.autocommit = True
            with conn.cursor() as cur:
                cur.execute(POSTGRES_SEED)
        print("Seeded `users` (6 rows, several intentional violations).", flush=True)
        return _build_probe(source.postgres(url)).run()


def _run_duckdb():
    # Use a tempfile so the DB is gone after the demo. ":memory:"
    # also works but persisting to disk lets you poke at it
    # afterwards if you want.
    with tempfile.TemporaryDirectory() as tmp:
        db_path = str(Path(tmp) / "quickstart.duckdb")
        print(f"Seeding in-process DuckDB at {db_path}...", flush=True)
        duckdb_setup(db_path, DUCKDB_SEED)
        print("Seeded `users` (6 rows, several intentional violations).", flush=True)
        return _build_probe(source.duckdb(db_path)).run()


def _run_parquet():
    # DuckDB writes the parquet file; the probe reads it.
    with tempfile.TemporaryDirectory() as tmp:
        parquet_path = str(Path(tmp) / "users.parquet")
        seed_db = str(Path(tmp) / "seed.duckdb")
        print(f"Writing seed Parquet to {parquet_path} via DuckDB...", flush=True)
        duckdb_setup(
            seed_db,
            DUCKDB_SEED + f"\nCOPY users TO '{parquet_path}' (FORMAT PARQUET);",
        )
        print("Wrote `users.parquet` (6 rows, several intentional violations).", flush=True)
        return _build_probe(source.parquet(parquet_path)).run()


def _run_s3():
    # LocalStack-backed S3 demo. DuckDB writes a parquet file
    # locally, then we boto3-upload it into a LocalStack bucket.
    import os

    import boto3
    from testcontainers.localstack import LocalStackContainer

    bucket = "ematix-quickstart"
    key = "users.parquet"

    # The Rust S3 adapter reads credentials via the standard AWS
    # resolver chain (env vars first). LocalStack accepts any
    # creds, but they have to be set so the chain doesn't fall
    # through to IMDS lookup.
    os.environ.setdefault("AWS_ACCESS_KEY_ID", "test")
    os.environ.setdefault("AWS_SECRET_ACCESS_KEY", "test")

    print("Starting LocalStack (S3 only) testcontainer...", flush=True)
    with LocalStackContainer(image="localstack/localstack:3.0").with_services("s3") as ls:
        endpoint = ls.get_url()
        print(f"  -> {endpoint}", flush=True)
        s3 = boto3.client(
            "s3",
            endpoint_url=endpoint,
            region_name="us-east-1",
            aws_access_key_id="test",
            aws_secret_access_key="test",
        )
        s3.create_bucket(Bucket=bucket)

        with tempfile.TemporaryDirectory() as tmp:
            parquet_path = str(Path(tmp) / "users.parquet")
            seed_db = str(Path(tmp) / "seed.duckdb")
            print("Writing seed Parquet via DuckDB...", flush=True)
            duckdb_setup(
                seed_db,
                DUCKDB_SEED + f"\nCOPY users TO '{parquet_path}' (FORMAT PARQUET);",
            )
            print(f"Uploading to s3://{bucket}/{key} ...", flush=True)
            s3.upload_file(parquet_path, bucket, key)

        print("Uploaded; running probe.", flush=True)
        return _build_probe(
            source.s3_parquet(
                bucket=bucket,
                key=key,
                region="us-east-1",
                endpoint_url=endpoint,
            )
        ).run()


_SOURCES = {
    "postgres": _run_postgres,
    "duckdb": _run_duckdb,
    "parquet": _run_parquet,
    "s3": _run_s3,
}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument(
        "--source",
        choices=sorted(_SOURCES),
        default="postgres",
        help="Backend the probe runs against. (default: postgres)",
    )
    args = parser.parse_args(argv)

    OUT_DIR.mkdir(exist_ok=True)
    print(f"Source: {args.source}", flush=True)
    print("Running probe `user_quality`...", flush=True)
    report = _SOURCES[args.source]()

    junit_path = OUT_DIR / "junit.xml"
    json_path = OUT_DIR / "report.json"
    report.write_junit(junit_path)
    report.write_json(json_path)

    print()
    print(f"Verdict: {report.verdict.upper()}")
    print(f"Assertions: {len(report.assertions)}")
    for a in report.assertions:
        marker = {"pass": "PASS", "fail": "FAIL", "error": " ERR"}[a.verdict]
        line = f"  [{marker}] {a.name or f'assertion_{a.assertion_index}'}"
        if a.message:
            line += f"  -- {a.message}"
        print(line)
    print()
    cwd = Path.cwd()
    junit_disp = junit_path.relative_to(cwd) if junit_path.is_relative_to(cwd) else junit_path
    json_disp = json_path.relative_to(cwd) if json_path.is_relative_to(cwd) else json_path
    print(f"JUnit XML: {junit_disp}")
    print(f"JSON:      {json_disp}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
