# Quickstart — all 7 assertions, JUnit + JSON reports

End-to-end demo of `@probe.data`. Boots a backend (Postgres
container, in-process DuckDB, or local Parquet), seeds a `users`
table with intentional violations, runs a probe covering every
v0.1 assertion, and writes `out/junit.xml` + `out/report.json`.

## Run it

From the repo root:

```bash
# One-time setup
python -m venv .venv && source .venv/bin/activate
pip install -e '.[dev]'
maturin develop --manifest-path crates/ematix-probe-py/Cargo.toml

# Demo (default = postgres; needs Docker)
python examples/quickstart/run.py

# Or any of the Phase 2 scan-path backends — no Docker needed:
python examples/quickstart/run.py --source duckdb
python examples/quickstart/run.py --source parquet
```

`postgres` requires Docker running locally; `duckdb` and
`parquet` are in-process and need nothing beyond the project's
dev extras.

## What the probe checks

| Assertion         | Builder                                | Verifies                                  |
| ----------------- | -------------------------------------- | ----------------------------------------- |
| `not_null`        | `t.column("email").not_null()`         | column has zero NULL rows                 |
| `regex`           | `t.column("email").regex(r".+@.+\..+")`| every non-NULL value matches POSIX regex  |
| `unique`          | `t.column("user_id").unique()`         | no duplicate values                       |
| `between`         | `t.column("age").between(0, 120)`      | every value in `[low, high]`              |
| `enum`            | `t.column("country").is_in([...])`     | every value in the allowed set            |
| `row_count`       | `t.row_count(at_least=1, at_most=1e6)` | table size in `[at_least, at_most]`       |
| `freshness`       | `t.freshness("updated_at", within="24h")` | `MAX(col)` is within the duration      |

Duration strings: `<int><unit>` where `unit ∈ {s, m, h, d}`.

## Expected output

```text
Verdict: FAIL
Assertions: 7
  [FAIL] email.not_null  -- column "email" has 1 NULL row(s); expected 0
  [FAIL] email.regex     -- column "email" has 1 row(s) not matching pattern ...
  [FAIL] user_id.unique  -- column "user_id" has 1 value(s) appearing more than once
  [FAIL] age.between     -- column "age" has 1 row(s) outside [0, 120]
  [FAIL] country.enum    -- column "country" has 1 row(s) outside allowed set (3 value(s))
  [PASS] row_count
  [PASS] freshness(updated_at)

JUnit XML: examples/quickstart/out/junit.xml
JSON:      examples/quickstart/out/report.json
```

The seed data deliberately violates 5 of the 7 assertions so the
JUnit output exercises the `<failure>` rendering path.

## Wiring into CI (GitHub Actions)

```yaml
- run: python examples/quickstart/run.py
- uses: dorny/test-reporter@v1
  if: success() || failure()
  with:
    name: data-quality
    path: examples/quickstart/out/junit.xml
    reporter: java-junit
```

The JUnit shape targets the GitHub Actions JUnit reporter
(sprint-03 risk #3); Jenkins and GitLab parse the same elements.
