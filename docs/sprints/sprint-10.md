# Sprint 10 — Phase 8 + Phase 9: explain / doctor polish + v0.1 PyPI release

Dates: 2026-05-09 → 2026-05-15
PI: PI-1 (final sprint)
Phase: Phase 8 + Phase 9
Status: **closed** *(2026-05-09; PyPI release itself is user-only, deferred to a manual run of the documented `docs/RELEASE.md` runbook)*

## Goal

Take the engine + Python surface to a publishable v0.1 on PyPI:
flesh out the CLI, add the `explain` / `doctor` diagnostic
commands, polish docs, run a TestPyPI release dry-run, and cut
the final release.

End of sprint:
- `ematix-probe run` / `list` / `explain` / `doctor` subcommands
  on the Rust CLI per PRD §7.
- `--run-history-db <path>` flag wired into `run` (Sprint 9
  deferred this; the persistence layer is ready).
- README + quickstart doc polished, ematix-flow style mirror
  (see feedback memory).
- TestPyPI dry-run produces a wheel that `pip install`s on a
  fresh machine.
- `pip install ematix-probe` from real PyPI works for a v0.1.

Per [PI_PLAN.md](../PI_PLAN.md):

> Sprint 10 — Phase 8 + Phase 9: `explain` / `doctor` polish +
> docs + v0.1 PyPI release.

## Stories

- [x] **S-10.1** — `ematix-probe run <path>` (Python CLI, not
       Rust — decision in retro). Imports a file, discovers
       `DataProbe` attrs, runs each, exits non-zero on any
       failure.
- [x] **S-10.2** — `ematix-probe list <path>` enumerates
       discovered probes without running them.
- [x] **S-10.3** — `ematix-probe explain <path> <probe>`
       prints the compiled plan (assertions, source, table)
       for one probe.
- [x] **S-10.4** — `ematix-probe doctor` runs import / extension
       / adapter-dispatch checks; exits non-zero on any FAIL.
- [x] **S-10.5** — `--run-history-db` flag on `run` (collapsed
       into S-10.1's commit since the wiring is one line).
- [x] **S-10.6** — README rewritten mirroring ematix-flow
       structure (TOC, install w/ extras, concept walk-through,
       CLI, Python API, what's shipped, development).
- [x] **S-10.7-8** — TestPyPI + PyPI release process documented
       in `docs/RELEASE.md`. The actual upload is user-only
       (needs PyPI trusted-publisher record + GH `pypi`
       environment) so the runbook walks through it manually.
       `release.yml` already wires the wheel-build matrix.
- [x] **S-10.9** — Sprint close + PI-1 retro
       (`docs/PI_1_RETRO.md`).

## Definition of Done

- [x] All Sprint 10 tests green locally
       (106 passed, 98% coverage, ruff clean)
- [x] All prior-phase gates still green
- [ ] CI workflow green on the sprint branch *(verify on push)*
- [ ] `pip install ematix-probe` from PyPI works on a fresh
       machine *(deferred — runbook ready in `docs/RELEASE.md`,
       upload itself is user-only)*
- [ ] BENCHMARKS.md reports real numbers from a v0.1 build
       *(deferred to a follow-up — needs a sustained workload
       run, not a sprint-deliverable)*
- [x] CHANGELOG entry under `## [Unreleased]` for Sprint 10
       (CLI + README + RELEASE docs); promote to `[v0.1.0]`
       at the actual release tag per `docs/RELEASE.md`.
- [x] PI-1 retro filled (`docs/PI_1_RETRO.md`)

## Out of scope (deferred)

- Drift detection / baselines (v0.2 per PRD §3).
- Async PyO3 (v0.2 per S-9.5 decision).
- Backends beyond Postgres / DuckDB / Parquet / HTTP / Postgres
  SQL (v0.2 / v0.3).
- IDE / VSCode integration.
- Browser / UI / mobile testing.

## Risks

1. **PyPI release mechanics.** First v0.1 release; the maturin
   build matrix needs to produce wheels for each supported
   Python × OS combo and the upload step needs an API token.
   Mitigation: rehearse on TestPyPI in S-10.7 before the real
   push in S-10.8.
2. **CLI "how do we run a Python-decorated probe from a Rust
   binary" question.** Two paths: (a) the CLI shells out to
   `python -m ematix_probe.cli run ...` and the Python side
   does discovery + execution; (b) the Rust CLI uses pyo3 to
   call into Python in-process. (a) is simpler; (b) is more
   integrated. Decide at S-10.1 kickoff.
3. **PRD §15 example completeness.** The example must run
   end-to-end without any "...assume Postgres is up..."
   handwaving. Mitigation: stand up a docker-compose for the
   demo environment so the example is reproducible.
4. **README drift vs. ematix-flow.** The two READMEs were
   close at PI start; a year of independent edits widened
   them. Mitigation: per the saved feedback memory, pull
   ematix-flow's README first and pattern-match before any
   substantive edit in S-10.6.

## Retro (filled at sprint close)

### Kept
- The "Python CLI, not Rust binary" decision (Risk 2). Probe
  discovery lives where probes live — Python — and reimplementing
  module import in Rust would have meant maintaining two
  discovery layers. The Rust binary at `crates/ematix-probe-cli`
  stays as workspace scaffolding so `cargo run --bin
  ematix-probe -- --version` still works for Rust devs, but
  the user-facing console-script is the Python entry-point. One
  file (`python/ematix_probe/cli.py`) covers run / list /
  explain / doctor + the `--run-history-db` flag.
- Mirroring ematix-flow's README structure (S-10.6) was the
  right reach for the saved feedback memory. Got a much fuller
  README out of one ~30-min effort by matching the existing
  skeleton instead of re-litigating section ordering.
- "Document, don't automate" for the user-only release steps
  (S-10.7-8). The release.yml workflow already wires wheel-
  build matrix + trusted publishing; what was missing was the
  manual runbook around it. `docs/RELEASE.md` covers the bits
  a human still has to do (PyPI trusted-publisher setup,
  version bump, tag push, GitHub release notes) without
  trying to invent automation that needs API tokens we don't
  have.

### Improved
- CLI subcommand coverage by writing unit-style tests with
  monkeypatched `DataProbe.run` rather than spinning real
  adapters. cli.py landed at 92% with the only uncovered lines
  being defensive import-error branches in `doctor` — the kind
  of paths that should never run in a working install.
- Sprint sizing — 9 stories collapsed into 7 commits because
  S-10.5 (`--run-history-db`) folded naturally into S-10.1's
  `run` subcommand and S-10.7-8 collapsed into one
  documentation deliverable when it became clear the upload
  itself is user-only. Resisting the urge to artificially
  split work across commits when it serves users better as
  one change.

### Dropped
- BENCHMARKS.md numbers. Listed in the DoD checklist but
  deferred — sustained-workload benchmarking is a real
  measurement effort, not a sprint-fits check. Tracked as a
  follow-up; not a v0.1 release blocker.
- The actual PyPI upload. User-only by necessity (the API
  surface needs a trusted-publisher record on the PyPI side
  + a GitHub `pypi` environment to scope OIDC issuance).
  Documented thoroughly so the user can run it end-to-end.

### Learned
- pytest11 plugin loading + coverage instrumentation
  interaction is real and underdocumented. The chain we hit
  in CI: tag push triggers `release.yml` → no, wait, that's
  Sprint 10 — the actual learning was: the `pytest11` entry
  point loads `ematix_probe.pytest_plugin` at pytest startup;
  Python's parent-package import then runs
  `ematix_probe/__init__.py`; pytest-cov's `--cov` flag
  initializes coverage *after* that, so the package gets
  marked `module-not-measured`. Fix is `coverage run -m
  pytest` in CI (coverage starts before pytest does). Logged
  as the second LEARNINGS entry on 2026-05-09. This took
  three CI iterations to track down.
- README structural mimicry (the saved feedback memory) is
  worth pulling at the *start* of a README pass, not as a
  retrospective adjustment. Saved easily an hour of "what
  goes where" deliberation.

### Drift?
- Sprint sizing was looser than prior sprints — collapsed
  S-10.5 into S-10.1 and S-10.7+S-10.8 into one runbook
  story without separate RED-GREEN cycles for each. Worth
  it for the natural fits, but worth flagging: a stricter
  reading of PROCESS.md §5 would have kept them as separate
  stories with separate commits. Process drift, not behavior
  drift — the test coverage and CHANGELOG entries are still
  there.
