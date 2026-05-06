# Changelog

All notable changes to `ematix-probe` are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
