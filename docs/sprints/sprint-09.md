# Sprint 9 — Phase 6 + Phase 7: pytest plugin + ematix-flow shim + run history

Dates: TBD (opens once PR for `phase-6` from Sprint 8 merges)
PI: PI-1
Phase: Phase 6 + Phase 7
Status: **planned**

## Goal

Take the v0.1 engine to where end-users live: a pytest plugin that
turns `ematix-probe` plans into normal pytest tests, an
`ematix-flow` integration shim so flow runs can drive probes, and
opt-in run-history persistence so trends are queryable across runs.

Per [PI_PLAN.md](../PI_PLAN.md):

> Sprint 9 — Phase 6 + Phase 7: pytest plugin +
> `ematix-flow` integration shim + opt-in run history persistence.

End of sprint:

- `pytest-ematix-probe` (or in-tree plugin module) — discovers
  `*.probe.yaml` (or python factory functions) and runs each
  probe as one pytest test, with assertions surfacing as
  per-test pass/fail.
- `ematix-flow` adapter (call site shape TBD per
  `ematix-flow`'s plugin contract) — surfaces probe verdicts
  alongside flow steps.
- Run history: opt-in `--run-history-db <sqlite-path>` writing
  one row per probe execution. Schema doubles as the substrate
  for v0.2 drift detection (per PRD §3 non-goals).
- Async support decision (PRD §6 / PI risk #4): commit to
  `pyo3-asyncio` if the API is ready, else freeze v0.1 to sync
  and document.

## Stories

Sketched only — flesh out at sprint kickoff.

- [ ] **S-9.1** — pytest plugin scaffold: install entry point,
       discovers a YAML / Python probe spec, runs it.
- [ ] **S-9.2** — Per-assertion test reporting (one pytest test
       node per `LoadAssertion` / `Assertion`, not per plan).
- [ ] **S-9.3** — `ematix-flow` integration shim (likely a small
       wrapper module + a flow-step entry point).
- [ ] **S-9.4** — SQLite run-history schema + `--run-history-db`
       flag on the CLI; one row per probe execution.
- [ ] **S-9.5** — Async PyO3 binding decision + spike (or
       documented v0.2 deferral).
- [ ] **S-9.6** — Sprint close (CHANGELOG / retro / sprint-10
       stub for Phase 8 + 9: explain / doctor polish + PyPI
       release prep).

## Definition of Done

- [ ] All Sprint 9 tests green in CI
- [ ] All prior-phase gates still green
- [ ] `pytest -p ematix_probe` (or equivalent) runs a probe in a
       fresh repo and reports per-assertion pass/fail
- [ ] Run-history sqlite file populated by an end-to-end run
- [ ] CHANGELOG entry under `## [Unreleased]` for Phase 6 + 7
- [ ] Retro filled below

## Out of scope (deferred)

- Drift detection / baseline comparison — explicitly v0.2 per
  PRD §3.
- IDE plugin / VSCode integration — out of v0.1.
- Multi-process distributed run history — single-file SQLite
  only in v0.1.
- pytest-xdist parallel execution support — v0.2 if requested.

## Risks

1. **`pyo3-asyncio` API churn** (PI risk #4). Mitigation:
   spike S-9.5 first, decide on commit vs sync-only fallback by
   end of week 1.
2. **Plugin discovery surface area.** YAML vs Python factories
   vs both — decide based on what `ematix-flow` already
   consumes; don't invent new conventions.
3. **Run-history schema lock-in.** Whatever ships becomes the
   substrate for v0.2 drift detection. Mitigation: keep the
   schema minimal (timestamp, plan_id, assertion_index,
   verdict, message); add columns later, never rename.

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
