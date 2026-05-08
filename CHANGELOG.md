# Changelog

All notable changes to `ematix-probe` are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added — Phase 2 (Sprint 4, PI-1)

- **Scan path** (`engine::scan`): pull-based `Scanner` trait
  yielding Arrow `RecordBatch`es + a shared `evaluate(plan,
  scanner)` that runs every v0.1 assertion (`not_null`, `unique`,
  `between`, `regex`, `enum`, `row_count`, `freshness`) against
  the stream. Same `Verdict`/message contract as the Postgres
  pushdown adapter from Sprints 2–3.
  - Per-assertion accumulators built up-front from the scanner's
    schema, so missing-column and unsupported-type errors surface
    without scanning a row.
  - `unique` HashSets keyed on `i64` / `String`; `between` casts
    Int8/16/32/64 + UInt8/16/32/64 + Float32/64 to f64; `regex`
    via the `regex` crate (RE2-flavored, differs subtly from
    Postgres POSIX); `freshness` tracks MAX across batches in the
    column's native Arrow `TimeUnit` and compares against
    `SystemTime::now()` at finalize.
  - `reduce_verdict` lifted from `adapters::data::postgres`
    (`pub(crate)`) to `engine::data` (`pub`) so the scan path
    shares it.
- **`DuckDbAdapter`** (`adapters::data::duckdb`): in-process
  DuckDB via the `duckdb` crate (`bundled` feature). Holds one
  `Arc<Mutex<Connection>>` for the adapter's lifetime so
  `:memory:` databases persist across `execute_setup` +
  `execute`. All sync DB calls run inside
  `tokio::task::spawn_blocking`. Per-`execute`: `SELECT * FROM
  <qualified_table>` → eager-collect into `Vec<RecordBatch>` →
  scan-path evaluator.
- **`ParquetAdapter`** (`adapters::data::parquet`): local Parquet
  files via the `parquet` crate. Eager-loads via
  `ParquetRecordBatchReaderBuilder`. The Parquet file *is* the
  table — `ProbePlan::table` / `schema` are ignored.
- **Python source factories**: `source.duckdb(path)` and
  `source.parquet(path)` join `source.postgres(url)`.
  `source.parquet` rejects `s3://` URLs explicitly (Phase 3).
- **pyo3 entry points**: `run_duckdb_probe`, `run_parquet_probe`,
  and `duckdb_setup` (test/example seeding helper). `DataProbe.
  run()` now dispatches on `source.kind`.
- **Quickstart `--source` flag**: `python examples/quickstart/
  run.py --source {postgres,duckdb,parquet}`. The two scan-path
  backends need no Docker; the parquet variant has DuckDB write
  the file via `COPY TO`.
- **License allow-list**: added `CC0-1.0` (`tiny-keccak` via
  duckdb chain) and `CDLA-Permissive-2.0` (`webpki-roots` via
  testcontainers/bollard) to `deny.toml`.

### Test surface — Phase 2

- Rust: 24 new scan-path + adapter tests (3 trait basics + 6/7/10
  evaluator coverage + 4 DuckDB + 3 Parquet — 40 total
  non-Docker green; postgres tests skip locally without Docker
  but run on CI).
- Python: 9 new tests (6 source builders, 3 e2e duckdb/parquet —
  no Docker needed for any of them). 67 total Python tests green.

### Added — Phase 1b (Sprint 3, PI-1)

- **Extended assertion vocabulary**: `regex`, `enum`, `row_count`,
  `freshness`. Each pushes down a single `count(*)` (or `MAX(col)`
  for freshness) so the wire returns one row per check regardless
  of table size.
  - `Assertion::Regex { column, pattern }` — Postgres POSIX
    `<col> !~ $1`. NULL-safe: NULLs are not counted as violations
    (pair with `not_null` to forbid).
  - `Assertion::Enum { column, allowed }` — variable-arity
    `<col> NOT IN ($1, $2, ...)`. Empty `allowed` rejected as
    `AdapterError::Config`.
  - `Assertion::RowCount { low, high }` — `Option<i64>` bounds;
    both `None` rejected as `Config` (asserts nothing).
  - `Assertion::Freshness { column, within_seconds }` —
    `EXTRACT(EPOCH FROM (now() - MAX(<col>)))::double precision`.
    Empty table → `Fail` (no signal); negative `within_seconds`
    → `Config`. Cast keeps the result f64-deserializable across
    PG 11..PG 16+.
- **Python builders** for all 7 assertions:
  `t.column(...).regex(p)`, `.is_in([...])`, plus table-level
  `t.row_count(at_least=, at_most=)` and `t.freshness(col,
  within="24h")`. Duration grammar is `<int><unit>` where
  `unit ∈ {s, m, h, d}`; lives in `python/ematix_probe/duration.py`.
- **Machine-readable reports** (`python/ematix_probe/report.py`):
  - `RunReport.write_junit(path)` — GitHub-Actions-compatible
    JUnit XML; one `<testsuite>` per probe, one `<testcase>` per
    assertion, `<failure>` on Fail / `<error>` on Error.
  - `RunReport.write_json(path)` — stable JSON schema documented
    in `tests/test_json_report.py`.
  - `DataProbe.run()` now returns a Python `RunReport` wrapping
    the pyo3 result with probe name / table / schema / timestamps
    / per-assertion human names.
- **End-to-end example** (`examples/quickstart/`): runnable script
  + README that boots a Postgres testcontainer, seeds intentional
  violations, runs all 7 assertions, writes JUnit + JSON, and
  shows the GitHub Actions reporter wiring.
- **Test surface grows to 25 Rust + 41 Python tests** (was 16
  total at end of Phase 1a): +14 Rust integration tests for the
  new assertions; +25 Python tests across duration parser,
  freshness builder, JUnit writer, and JSON writer.

### Fixed — Phase 1b

- Freshness adapter panicked on PG 14+ because
  `EXTRACT(EPOCH FROM ...)` returns `numeric` there (was
  `double precision` on PG <14). Cast explicitly to
  `double precision` — no-op on older versions, correct on
  newer. Surfaced by the S-3.7 example running against
  `postgres:16-alpine` while the integration tests used
  `postgres:11-alpine` (testcontainers-modules' default).

### Added — Phase 1a (Sprint 2, PI-1)

- **Engine foundation** (`crates/ematix-probe-core`):
  `engine::data::{Verdict, Assertion, ProbePlan, AssertionResult, RunSummary}`
  + `adapters::data::{AdapterError, DataAdapter}`. Module tree
  matches the architecture target in PRD §8.1
  (`engine::{data,load}` + `adapters::{data,load}`).
- **`PostgresAdapter`** with eager `SELECT 1` connection
  validation, `tokio-postgres` + `deadpool-postgres` pool, and
  identifier-quoting helpers (`quote_ident`, `qualified_table`).
- **Pushdown SQL** for the v0.1 column-level assertion vocabulary:
  `not_null`, `unique`, `between` (inclusive range, NULL-safe,
  works on INT / BIGINT / NUMERIC / DOUBLE PRECISION via
  `$1::float8` placeholder cast).
- **Python `@probe.data` decorator + fluent `Tester` builder**
  (`python/ematix_probe/probe.py`, `source.py`):
  ```python
  @probe.data(source=source.postgres(url), table="users")
  def quality(t):
      t.column("email").not_null()
      t.column("user_id").unique()
      t.column("age").between(0, 120)
  report = quality.run()
  ```
- **PyO3 bindings** (`crates/ematix-probe-py`): `ProbePlan`,
  `Assertion`, `AssertionResult`, `RunReport` Python types +
  assertion factory functions + `run_postgres_probe(url, plan)`
  end-to-end entry point. GIL-released via `py.detach` while the
  tokio runtime drives the async adapter.
- **Test surface**: 16 tests across 4 levels —
  6 Rust unit tests (version + identifier-quoting helpers);
  1 Rust integration (empty-plan basics);
  4 Rust integration (Postgres assertion behavior, testcontainers);
  2 Rust integration (Postgres connect/validate);
  1 CLI integration;
  6 Python pytest (decorator + Source + chained calls);
  2 Python pytest (full e2e against Postgres testcontainer).

### Added — Phase 0 (Sprint 1, PI-1)

- Rust workspace skeleton (`ematix-probe-core`, `ematix-probe-cli`,
  `ematix-probe-py`) with maturin build via `pyproject.toml`.
- `ematix-probe-core::version()` returning `"0.1.0-dev"`.
- `ematix-probe` CLI binary (`--version` only; subcommands land in
  later phases per [PRD §7](docs/PRD.md)).
- PyO3 binding crate exposing `__version__` to the `ematix_probe`
  Python package.
- GitHub Actions CI workflow: `rust` (fmt + clippy + test),
  `audit-rust` (cargo-audit vs RustSec DB), and `python` matrix on
  py3.11 / 3.12 / 3.13 (maturin build + ruff + bandit + pip-audit
  + pytest).
- Release workflow (tag-gated, manylinux_2_28 + macOS aarch64
  wheels + sdist + PyPI trusted publisher).
- `SECURITY.md` (vuln reporting, automated CI gates, threat model).
- `.cargo/audit.toml` (advisory ignore list, currently empty).
- `.cargo/config.toml` (PyO3 macOS link flags).
- Project process docs: [PRD](docs/PRD.md), [PROCESS](docs/PROCESS.md)
  (TDD + sprint cadence), [PI plan](docs/PI_PLAN.md), per-sprint
  files under [docs/sprints/](docs/sprints/), [LEARNINGS](docs/LEARNINGS.md).

[Unreleased]: https://github.com/ryan-evans-git/ematix-probe/compare/...HEAD
