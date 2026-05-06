"""End-to-end ematix-probe quickstart.

Boots an ephemeral Postgres testcontainer, seeds a `users` table
that intentionally violates several quality rules, declares a
`@probe.data` covering all 7 v0.1 assertion types, runs it, and
writes JUnit XML + JSON reports to ``out/``.

Run it from the repo root::

    python examples/quickstart/run.py

Requirements: Docker running locally + the project's dev extras
installed (``pip install -e '.[dev]'`` or ``maturin develop``).

Exits 0 even on a Fail verdict — this is a demo, not a CI gate.
The reports in ``out/`` show what a downstream CI runner would
consume.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

import psycopg2
from ematix_probe import probe, source
from testcontainers.postgres import PostgresContainer

OUT_DIR = Path(__file__).parent / "out"

SEED_SQL = """
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
    ('duplicate-id',      1, 33, 'GB', now()),    -- unique fail
                                                  -- enum fail (GB ∉ {US,CA,MX})
                                                  -- regex fail (no @ in email)
    ('eve@example.com',   6, 200, 'US', now());   -- between fail
"""


def main() -> int:
    OUT_DIR.mkdir(exist_ok=True)

    print("Starting Postgres testcontainer...", flush=True)
    with PostgresContainer("postgres:16-alpine") as pg:
        url = pg.get_connection_url().replace("+psycopg2", "")
        print(f"  -> {url}", flush=True)

        with psycopg2.connect(url) as conn:
            conn.autocommit = True
            with conn.cursor() as cur:
                cur.execute(SEED_SQL)
        print("Seeded `users` (6 rows, several intentional violations).", flush=True)

        @probe.data(source=source.postgres(url), table="users")
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

        print("Running probe `user_quality`...", flush=True)
        report = user_quality.run()

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
        print(f"JUnit XML: {junit_path.relative_to(Path.cwd()) if junit_path.is_relative_to(Path.cwd()) else junit_path}")
        print(f"JSON:      {json_path.relative_to(Path.cwd()) if json_path.is_relative_to(Path.cwd()) else json_path}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
