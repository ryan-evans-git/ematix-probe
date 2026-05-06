# Changelog

All notable changes to `ematix-probe` are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
