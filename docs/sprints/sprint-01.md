# Sprint 1 — Workspace skeleton + green CI

Dates: 2026-05-06 → 2026-05-12
PI: PI-1
Phase: Phase 0
Status: **active**

## Goal

Stand up the Rust workspace, Python package, maturin build, and CI
green on first push — under TDD. End of sprint: `cargo test`, `pytest`,
`cargo clippy --all-targets`, `ruff check`, and `cargo fmt --check` all
pass on a fresh checkout.

## Stories

- [x] **S-1.1 — Rust workspace + `ematix-probe-core::version()`**
  - RED: `crates/ematix-probe-core/src/lib.rs` test
    `version_returns_dev_string` asserts `version() == "0.1.0-dev"`,
    fails to compile (no `version` symbol yet).
  - GREEN: implement `pub fn version() -> &'static str`.
  - REFACTOR: none (one line).

- [x] **S-1.2 — `ematix-probe` CLI binary with `--version`**
  - RED: `crates/ematix-probe-cli/tests/cli_version.rs` integration test
    invokes the built binary with `--version`, asserts stdout starts
    with `ematix-probe 0.1.0-dev`. Fails (binary doesn't exist).
  - GREEN: clap-based CLI in `crates/ematix-probe-cli/src/main.rs` that
    delegates the version string to `ematix_probe_core::version()`.
  - REFACTOR: none.

- [x] **S-1.3 — PyO3 binding crate exposing `__version__`**
  - RED: `tests/test_smoke.py::test_version_matches_core` does
    `import ematix_probe; assert ematix_probe.__version__ == "0.1.0-dev"`.
    Fails — module doesn't exist yet.
  - GREEN: `crates/ematix-probe-py/src/lib.rs` declares a `pyo3` module
    that calls `ematix_probe_core::version()` and exposes it as
    `__version__`. `python/ematix_probe/__init__.py` re-exports it.
    `pyproject.toml` configures maturin to build the package as
    `ematix_probe`.
  - REFACTOR: none.

- [x] **S-1.4 — Lints + formatters clean**
  - RED: `cargo clippy --all-targets -- -D warnings` and `ruff check .`
    in CI fail on an intentionally-bad commit (proves the gates work),
    then revert.
  - GREEN: zero warnings on the real tree.

- [x] **S-1.5 — GitHub Actions CI workflow green on first push**
  - RED: workflow file references the four jobs but the placeholder
    jobs `exit 1`. Push, see CI red. (Optional — we may skip the
    deliberate-red push and just verify locally.)
  - GREEN: real jobs — `cargo test --workspace`, `cargo clippy
    --all-targets -- -D warnings`, `cargo fmt --check`, `maturin develop`
    + `pytest`, `ruff check .`. All green on a clean push to a feature
    branch.

- [x] **S-1.6 — Repo hygiene**
  - `.gitignore` (Rust target dir, Python `__pycache__`, `.venv`,
    maturin build artifacts, OS noise).
  - `README.md` — one-paragraph description + status badge placeholder.
  - Initial commit on `main`; Phase 0 work lands on a `phase-0` branch
    via PR.

## Definition of Done

- [x] `cargo test --workspace` green (3 tests: core x2 + cli x1)
- [x] `cargo clippy --all-targets -- -D warnings` green
- [x] `cargo fmt --check` green
- [x] `maturin develop` succeeds in a clean venv
- [x] `pytest` green (2 tests)
- [x] `ruff check .` green
- [ ] CI workflow green on `phase-0` branch *(verified after first push)*
- [ ] PR opened against `main`, merged *(after first push)*
- [x] PRD / PI plan updated for any scope drift *(no drift)*
- [ ] Retro section below filled in *(end-of-sprint, not end-of-Phase-0)*

## Retro (filled at sprint close)

### Kept
-

### Improved
-

### Dropped
-

### Learned
-

### Drift?
-
