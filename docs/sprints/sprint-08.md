# Sprint 8 — Phase 5: Load probe VU mode + Postgres SQL load adapter

Dates: 2026-06-11 → 2026-06-17
PI: PI-1
Phase: Phase 5
Status: **planned** *(opens once PR for `phase-5` from Sprint 7 merges)*

## Goal

Two extensions to the load probe:

1. **VU (virtual-user) mode** — closed-model load generation
   alongside the constant-rate open-model scheduler from
   Sprint 6. A VU pool of `N` concurrent workers each loops
   `request → wait → request`, so the achieved RPS depends on
   the target's response time. Standard for stress-testing a
   service that backs off under load.
2. **Postgres SQL load adapter** — drives a Postgres target
   with parameterized queries instead of HTTP requests. Same
   `LoadPlan` / `Sample` / `evaluate_load` surface; latency is
   query latency, "status_code" maps to success / SQL error.

Per [PI_PLAN.md](../PI_PLAN.md):

> Phase 5 — Load probe VU mode + Postgres SQL adapter;
> query parameterization

End of sprint:
- `engine::load::scheduler::VuPool` — N concurrent workers
  driving a closed-model loop.
- `LoadPlan` gains `mode: LoadMode { ConstantRate { rps } |
  VirtualUsers { count } }` (or equivalent) so a single plan
  can pick its scheduling discipline.
- `adapters::load::postgres::PostgresLoadAdapter` running
  parameterized queries via `tokio-postgres` prepared
  statements.
- A second example
  (`crates/ematix-probe-core/examples/postgres_load_demo.rs`)
  driving a postgres testcontainer with a 10-VU read-heavy
  workload.

## Stories

Each story RED → GREEN → REFACTOR per [PROCESS.md §5](../PROCESS.md).
Stories sketched in outline; flesh out at sprint kickoff.

- [ ] **S-8.1 — `LoadMode` enum** + `LoadPlan.mode` field; existing
       constant-rate path becomes `LoadMode::ConstantRate { rps }`.
- [ ] **S-8.2 — `VuPool` scheduler** producing `Tick`s from N
       concurrent workers in a closed loop.
- [ ] **S-8.3 — `HttpLoadAdapter` dispatches on `LoadMode`** —
       pulls ticks from `ConstantRateScheduler` or `VuPool`
       depending on the plan.
- [ ] **S-8.4 — Postgres target shape** (`PostgresTarget` /
       `LoadQuery` types: SQL string + parameter values).
- [ ] **S-8.5 — `PostgresLoadAdapter`** — `tokio-postgres`
       prepared statements driven by the chosen scheduler.
- [ ] **S-8.6 — Latency metric uniformity** — verify P99Under +
       ErrorRateBelow + ThroughputAbove + StatusCodeIn (or
       postgres-equivalent "status") all work against postgres
       samples.
- [ ] **S-8.7 — `examples/postgres_load_demo.rs`** — 10 VUs
       against a postgres testcontainer with a parameterized
       `SELECT * FROM users WHERE id = $1`.
- [ ] **S-8.8 — Sprint close** (CHANGELOG / retro / learnings /
       sprint-09 stub for Phase 6 + 7).

## Definition of Done

- [ ] All Sprint 8 tests green in CI
- [ ] All prior-phase gates still green
- [ ] CI workflow green on the sprint branch
- [ ] PR opened and merged into `main`
- [ ] CHANGELOG entry under `## [Unreleased]` for Phase 5
- [ ] `cargo run --example postgres_load_demo` runs end-to-end
- [ ] HTTP demo (S-7.7) still passes after the LoadMode
       refactor
- [ ] Retro filled below

## Out of scope (deferred)

- Distributed load (multi-process / multi-host) — beyond v0.1.
- VU ramp-up / step / spike profiles — constant N only in v0.1.
- Postgres `EXPLAIN ANALYZE` integration for query-plan
  assertions — out of scope; `LoadAssertion` stays statistics-
  only.
- Custom assertion message formatting — wait for real reporter
  needs.

## Risks

1. **`LoadMode` refactor blast radius.** Adding the mode field
   to `LoadPlan` touches every existing test ctor + the HTTP
   adapter. Mitigation: keep the migration mechanical, ride
   the test suite for confidence.
2. **VU-mode test flakiness.** Closed-model timing is
   inherently non-deterministic (you can't "wait for N requests
   in T time" when each request takes a variable time).
   Mitigation: assertions on N requests (no timing), or
   timeout-bounded "drove for ≤ N seconds, made between K and
   M requests".
3. **Postgres adapter SQL injection surface.** Per PRD:
   parameterized queries only. `LoadQuery::new` should reject
   raw string interpolation at the API level.

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
