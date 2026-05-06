# Changelog

All notable changes to `ematix-probe` are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
