# Sprint 1 — Workspace skeleton + green CI

Dates: 2026-05-06 → 2026-05-06 *(closed early — Phase 0 shipped same-day)*
PI: PI-1
Phase: Phase 0
Status: **closed**

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
- [x] CI workflow green on `phase-0` branch *(5/5 checks green on PR #1)*
- [x] PR opened against `main`, merged *(PR #1 merged 2026-05-06)*
- [x] PRD / PI plan updated for any scope drift *(see drift note in retro)*
- [x] Retro section below filled in

## Retro (closed 2026-05-06)

### Kept
- **TDD rhythm worked.** Every story went RED (test compiled-but-failed
  or asserted the wrong thing) → GREEN (smallest passing impl) → small
  commit. Three pyo3 binding bugs were caught by the test runner before
  they could land in `main`. Keep this discipline.
- **Mirroring ematix-flow's CICD verbatim was the right call.** Saved
  hours of reinventing wheel-build matrix decisions, audit-toml
  rationale comments, the `await-ci` shell-loop guard pattern, and the
  PyO3 macOS link-flag quirks. Strategy for future sprints: *check
  ematix-flow first*.
- **Process docs live alongside code.** PRD, PI plan, sprint, and
  learnings all updated in the same PR as the code. Seeing the whole
  picture in one diff was high-signal.

### Improved
- **CWD anchoring in shell commands.** Claude Code's session anchor is
  `ematix-flow`, but our work is in `ematix-probe`. One verification
  sweep accidentally compiled ematix-flow's huge workspace before I
  caught it (1m25s wasted). Going forward: always lead Bash commands
  with `cd /Users/ryanevans/RustroverProjects/ematix-probe && ...` for
  ematix-probe work.
- **Pipe-masked failures.** `cargo test ... 2>&1 | tail -20` made
  `set -e` blind to a real linker error. Logged in
  [LEARNINGS.md](../LEARNINGS.md). For CI gate sweeps: don't pipe
  through `tail`, OR use `set -o pipefail`.

### Dropped
- **Reflex of "tail the noisy build output."** It hides failures.
  Stop doing this for status-checking sweeps.
- **Putting placeholder pyo3 setups together that haven't been
  end-to-end-tested.** The first attempt enabled `extension-module` at
  the workspace level and broke `cargo test --workspace` — should have
  copied ematix-flow's per-crate feature pattern from the start.

### Learned
- Two technical learnings already in [LEARNINGS.md](../LEARNINGS.md):
  pyo3 `extension-module` feature gating, and `set -e` + pipe.
- **Process learning:** Phase 0 was scoped for 1 week but shipped in
  1 day. PI plan dates are now accurate-but-loose. We'll re-baseline
  PI-1 dates after Sprint 2 finishes — get one more data point on
  actual sprint velocity.

### Drift?
- **In-sprint scope expansion (approved, not drift):** The user asked
  mid-sprint to mirror ematix-flow's full CICD (release.yml,
  SECURITY.md, .cargo/audit.toml, bandit, pip-audit, CHANGELOG.md)
  before continuing. This wasn't in S-1.1..S-1.6 originally. Decision
  was explicit ("model the CICD process and requirements after what
  is used for ematix-flow"), so it's an authorized scope expansion,
  not silent drift. Going forward: if a similar request lands
  mid-sprint, log it as an explicit story in the sprint file before
  starting work, even retroactively.
- **No PRD drift, no PI plan drift.** Phase 0 stories all match what
  shipped.
