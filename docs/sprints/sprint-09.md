# Sprint 9 — Phase 6 + Phase 7: pytest plugin + ematix-flow shim + run history

Dates: 2026-05-09 → 2026-05-15
PI: PI-1
Phase: Phase 6 + Phase 7
Status: **closed** *(2026-05-09)*

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

- [x] **S-9.1** — pytest plugin scaffold: `pytest11` entry point
       + `pytest_pycollect_makeitem` hook turning `DataProbe`
       attrs into pytest items.
- [x] **S-9.2** — Per-assertion test reporting via
       `DataProbeCollector` yielding one `DataProbeAssertionItem`
       per assertion; `RunReport` cached so `.run()` fires once.
- [x] **S-9.3** — `ematix_probe.flow.probe_from_table` shim:
       duck-typed on a small protocol, auto-derives `not_null`
       on non-nullable columns + `unique` on PKs, with an
       optional `extend=` callable to layer extras.
- [x] **S-9.4** — `ematix_probe.run_history.RunHistory(path)`:
       opt-in stdlib-sqlite3 persister, two-table schema
       (`runs` + `assertions`) with PRAGMA `user_version`. CLI
       flag wiring deferred until the Rust CLI grows past
       `--version`.
- [x] **S-9.5** — Async PyO3 deferred to v0.2. PRD + LEARNINGS
       updated with the rationale.
- [x] **S-9.6** — Sprint close.

## Definition of Done

- [x] All Sprint 9 tests green locally (83 → 83 + 14 new = 97
       passed at sprint close, 1 of which is the existing flaky
       s3 dispatch test passing in isolation)
- [x] All prior-phase gates still green
- [ ] CI workflow green on the sprint branch *(verify on push)*
- [x] Plugin loads via `pytest_plugins = ["ematix_probe.pytest_plugin"]`
       and surfaces `DataProbe` instances as per-assertion items
- [x] Run-history sqlite file populated by `RunHistory.record()`
       (covered by `tests/test_run_history.py`)
- [x] CHANGELOG entry under `## [Unreleased]` for Phase 6 + 7
- [x] Retro filled below

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
- The two-step `Collector` → per-assertion `Item` shape from S-9.2
  worked exactly like pytest's parametrize idiom but without
  fighting parametrize's collection-time semantics. Caching the
  `RunReport` on the parent collector means N assertions = 1
  `.run()` call, which is what users expect from a fan-out.
- Deferral over heroics on S-9.5. The pytest plugin is what
  v0.1 users actually need; spending the remaining sprint budget
  on `pyo3-asyncio` API archaeology would have crowded out the
  flow shim and run-history work that did ship. Documented v0.2
  deferral > undocumented v0.1 wobble.
- Stub libraries first, real adapters later — both `flow.py` and
  `run_history.py` ship with stdlib-only code paths
  (Protocol-based duck typing for flow; sqlite3 stdlib for
  history). Zero new transitive deps from this sprint.

### Improved
- Caught a fixture-isolation bug early in S-9.2: the synthesized
  pytester test files were monkeypatching `DataProbe.run` at
  module level, which leaks across the in-process pytester run
  back into the outer suite (the s3 dispatch test started
  passing instead of raising). Fix: own the monkeypatch from the
  *outer* test using pytest's `monkeypatch` fixture, which
  auto-restores on teardown. Added the rationale as a docstring
  comment on the test file so the next person doesn't reinvent
  the wheel.
- Custom `pytest.Item` subclasses don't run pytest fixtures.
  Discovered this when the autouse fixture in the synthesized
  file didn't fire for `DataProbeAssertionItem`. Implication:
  any future plugin work that needs fixture access on items has
  to either subclass `pytest.Function` (with the right
  Module/Class parent) or wire up the fixture machinery
  manually. Not blocking for v0.1 — noted for v0.2.

### Dropped
- `--run-history-db` CLI flag deferred. The Rust CLI is still a
  bare `--version` skeleton; bolting a flag on it now would
  invent half of S-10's `run`/`list`/`explain` surface. The
  persistence layer (`RunHistory`) is the load-bearing part
  and shipped — flag wiring is mechanical when the CLI grows.

### Learned
- `pyo3-asyncio` API churn (LEARNINGS 2026-05-09) is real enough
  that "spike vs defer" becomes a decision worth dating + signing
  rather than an open question. Documented decision is the
  artifact even when no code lands.
- Per-assertion fan-out ergonomics matter for CI signal: a
  3-assertion probe surfacing as 3 red/green pytest nodes (vs.
  one) gives the report layer enough resolution to point at the
  failing check directly. The collector-with-cached-report
  pattern is reusable for the future load-probe pytest plugin
  surface (Phase 7+) without restructuring.

### Drift?
- None this sprint. The S-9.4 CLI-flag deferral was an
  intentional scope cut tracked in the Dropped section, not
  drift.
