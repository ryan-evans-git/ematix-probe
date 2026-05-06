# Learnings — ematix-probe

Append-only log of findings, surprises, and rules-of-thumb worth
remembering. Add an entry any time you'd want a future contributor (or
future-you) to know something that isn't obvious from the code.

Format: one entry per finding, dated, tagged. Don't edit old entries —
correct them with a new entry that supersedes.

Tags: `process`, `tooling`, `architecture`, `perf`, `tdd`, `drift`,
`rust`, `python`, `pyo3`, `ci`.

---

## 2026-05-06 — Project kickoff `process`

Decisions made before any code:
- TDD is non-negotiable for this project (no test → no implementation
  commit).
- 1-week sprints, retro at end of every sprint, learnings logged here.
- PI-1 is 10 sprints targeting v0.1 on PyPI.
- Process docs ([PROCESS.md](PROCESS.md), [PI_PLAN.md](PI_PLAN.md),
  per-sprint files, this log) are kept honest in the same PR as code
  changes that affect them.

Why this matters: ematix-flow shipped without an explicit cadence and
the late phases lost track of which decisions were intentional vs.
accidental. Doing the planning + retro work up front for ematix-probe
is the lesson learned.

## 2026-05-06 — Gate `pyo3/extension-module` behind a feature flag `pyo3` `tooling`

In Phase 0 the workspace `pyo3` dep enabled `extension-module` directly,
which broke `cargo test --workspace`: cargo builds the `_core` cdylib's
test binary as a normal executable, but `extension-module` tells pyo3
*not* to link libpython (the host process is supposed to provide it).
No host = linker error on `__Py_Dealloc`, `__Py_NoneStruct`, etc.

Fix: define `extension-module` as a *crate feature* on `ematix-probe-py`
that forwards to `pyo3/extension-module`, and have maturin enable it via
`[tool.maturin] features = ["ematix-probe-py/extension-module"]`. Plain
`cargo test` doesn't enable the feature → tests link against a real
libpython → green.

Apply this pattern to every future PyO3-bound crate.

## 2026-05-06 — Dev-only deps still trigger `cargo audit` advisories `rust` `tooling` `ci`

Adding `testcontainers-modules` as a `[dev-dependencies]` pulled in
`tokio-tar 0.3.1` (RUSTSEC-2025-0111, file-smuggling via PAX
headers, no fix available) and `rustls-pemfile 2.2.0` (unmaintained
warning, not blocking).

`cargo audit` doesn't distinguish dev-deps from runtime deps —
anything in `Cargo.lock` is fair game. So even a strictly dev-only
test infra triggers CI-fail-by-default.

Pattern for accepting these: edit `.cargo/audit.toml` `ignore = [...]`
and document the (a) why-can't-fix and (b) risk-assessment in the
inline comment AND in SECURITY.md's "Known accepted advisories"
table. **Don't suppress without that paper trail** — quarterly
re-audit cycles depend on it.

## 2026-05-06 — Sprint 1 retro: a 1-week sprint can close in 1 day `process`

Phase 0 was scoped for a 1-week sprint and shipped same-day. Two
implications:

1. **PI-1 dates are now loose.** The 10-sprint, 10-week PI-1 plan
   assumed 1 phase ≈ 1 sprint ≈ 1 week. Phase 0 broke that. We're
   not re-baselining yet — one data point isn't enough — but Sprint
   2 (Phase 1a, real implementation work) is the velocity test.
2. **Mid-sprint scope expansion is OK if explicit.** During Sprint 1,
   user requested mirroring ematix-flow's full CICD (release.yml,
   SECURITY.md, audit configs, bandit/pip-audit) before continuing.
   Wasn't in S-1.1..S-1.6. We did it anyway because the request was
   explicit. Logged as authorized scope expansion in the retro, not
   silent drift. Future rule: **if scope expands mid-sprint, add a
   story to the sprint file before doing the work** (even
   retroactively in the same PR).

## 2026-05-06 — `set -e` does not catch failures behind a pipe `tooling` `ci`

Ran the Phase 0 gate sweep as `cargo test --workspace 2>&1 | tail -20`
and saw "ALL GREEN" even though the test phase had a linker error. The
last command in the pipeline is `tail`, so its zero exit overrode
cargo's non-zero — `set -e` only checks the *final* exit status.

Fix: don't pipe failure-sensitive commands through `tail` in CI-style
sweeps, OR use `set -o pipefail` to propagate the leftmost non-zero
exit. CI scripts in this repo use neither pipe nor `tail`; humans
running locally should remember that "looks green" ≠ "is green" when
output is filtered.
