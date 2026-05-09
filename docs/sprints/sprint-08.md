# Sprint 8 — Phase 5: Load probe VU mode + Postgres SQL load adapter

Dates: 2026-06-11 → 2026-06-17
PI: PI-1
Phase: Phase 5
Status: **closed** *(2026-05-09)*

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

- [x] **S-8.1 — `LoadMode` enum** + `LoadPlan.mode` field; existing
       constant-rate path becomes `LoadMode::ConstantRate { rps }`.
- [x] **S-8.2 — `VuPool` scheduler** producing `Tick`s from N
       concurrent workers in a closed loop.
- [x] **S-8.3 — `HttpLoadAdapter` dispatches on `LoadMode`** —
       pulls ticks from `ConstantRateScheduler` or `VuPool`
       depending on the plan.
- [x] **S-8.4 — Postgres target shape** (`PostgresTarget` /
       `LoadQuery` types: SQL string + parameter values).
- [x] **S-8.5 — `PostgresLoadAdapter`** — `tokio-postgres`
       prepared statements driven by the chosen scheduler.
- [x] **S-8.6 — Latency metric uniformity** — verify P99Under +
       ErrorRateBelow + ThroughputAbove + StatusCodeIn (or
       postgres-equivalent "status") all work against postgres
       samples. Implemented via `LoadProfile` trait so
       `evaluate_load` is generic over plan type.
- [x] **S-8.7 — `examples/postgres_load_demo.rs`** — 10 VUs
       against a postgres testcontainer with a parameterized
       `SELECT * FROM users WHERE id = $1`.
- [x] **S-8.8 — Sprint close** (CHANGELOG / retro / learnings /
       sprint-09 stub for Phase 6 + 7).

## Definition of Done

- [x] All Sprint 8 tests green in CI
- [x] All prior-phase gates still green
- [x] CI workflow green on the sprint branch *(verify on push)*
- [x] PR opened and merged into `main` *(this story)*
- [x] CHANGELOG entry under `## [Unreleased]` for Phase 5
- [x] `cargo run --example postgres_load_demo` runs end-to-end
- [x] HTTP demo (S-7.7) still passes after the LoadMode
       refactor
- [x] Retro filled below

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
- The trait-based unification in S-8.6 (`LoadProfile` + generic
  `evaluate_load<P: LoadProfile>`) was the lowest-blast-radius way
  to make one evaluator serve both target types. Every existing
  caller kept compiling because `LoadPlan` implements the trait;
  `PgLoadPlan` joins by adding the same four-line impl. Reusable
  pattern when you need polymorphism over plans/configs that share
  a small read-only surface.
- Extracting `request_to_sample` (S-8.3) and the analogous
  `run_query_to_sample` (S-8.5) — same per-tick "do work → measure
  → emit Sample" shape on both adapters. Made the open/closed
  dispatch a thin shell, which kept the adapter modules tiny.
- The `non_exhaustive` `LoadMode` enum from Sprint 8 lets us add
  future modes (ramping, stepped, spike) without breaking exhaustive
  matches at downstream call sites.

### Improved
- `LoadAssertion` deliberately stayed shared between HTTP and
  Postgres targets rather than forking — turns out P99Under,
  ErrorRateBelow, ThroughputAbove, StatusCodeIn all read cleanly
  against postgres samples (success → Some(200), error → None +
  message). Symmetry without an adapter-specific assertion DSL.
- `bind_owned` in `adapters::load::postgres` boxes each
  `QueryParam` into a `Box<dyn ToSql + Sync + Send>` so we can
  build the `&[&(dyn ToSql + Sync)]` slice tokio-postgres demands.
  Cleanest of the alternatives I considered (impl `ToSql` for
  the enum directly was much more code; per-tick match arms
  would mean variadic execution).

### Dropped
- Nothing intentional. Considered hoisting `(duration, mode,
  warmup, assertions)` into a shared `LoadProfile` *struct*
  embedded in both plan structs, but the trait was less
  invasive (no field-renaming, no breaking destructuring in
  test ctors).

### Learned
- `tokio_postgres::types::ToSql` is bind-type-strict: pass an
  `i64` (which `QueryParam::Int` becomes) and the server-side
  `$1` cast must be `bigint`, not `int` — otherwise tokio-postgres
  errors with "error serializing parameter 0". Documented in the
  postgres_load_demo comment so the next person reading the
  example doesn't trip over it.
- Examples in Cargo packages have access to `[dev-dependencies]`
  by default — useful to know, since `postgres_load_demo` needs
  `testcontainers-modules` (a dev-dep) but ships as an example,
  not a test. No Cargo.toml plumbing required.
- Generic `evaluate_load<P: LoadProfile>` does not need turbofish
  at call sites — Rust infers `P` from the `&plan` argument. Means
  the existing `evaluate_load(&plan, &samples)` call style works
  unchanged across the refactor, which is what you want from a
  "make it generic" change.

### Drift?
- One slip on S-8.4 GREEN — committed `.claude/scheduled_tasks.lock`
  (a Claude Code runtime artifact) into the repo because
  `git add -A` swept it up. Caught it within one commit, added
  to .gitignore, removed from the index in a follow-up `chore:`
  commit. Future rule: prefer staging specific paths over `-A`
  when the working tree has untracked runtime files.
