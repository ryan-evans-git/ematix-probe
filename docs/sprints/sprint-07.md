# Sprint 7 — Phase 4b: Load probe HTTP polish + S3 carry-over

Dates: 2026-06-04 → 2026-06-10
PI: PI-1
Phase: Phase 4b (+ a small Sprint-6 carry-over)
Status: **planned** *(opens once PR for `phase-4` from Sprint 6 merges)*

## Goal

Polish the load-probe HTTP MVP from Sprint 6 + close the
remaining LocalStack-based bits from Sprint 5/6:

1. Two more load assertions: `throughput_above` and
   `status_code_in`.
2. Warmup support — discard samples from a configurable warmup
   window so the first connection / DNS / TLS handshake doesn't
   skew p99.
3. The httpbin-style end-to-end example deferred from S-6.8 —
   now backed by a LocalStack-on-CI test container so the demo
   is reproducible.
4. Quickstart `--source s3` and the cross-engine property test
   extension to s3, both deferred from Sprint 6.

Per [PI_PLAN.md](../PI_PLAN.md):

> Phase 4b — Load probe HTTP polish — `throughput_above` /
> `status_code_in` / warmup; httpbin-style end-to-end example

End of sprint:
- `LoadAssertion::ThroughputAbove { threshold_rps }` and
  `LoadAssertion::StatusCodeIn { allowed: Vec<u16> }` evaluators.
- `LoadPlan::warmup: Duration` field; samples from
  `[start, start + warmup)` are dropped before evaluation.
- An `examples/load_demo/` Rust example (or extension to the
  Python quickstart once a Python load surface lands) running
  against an in-process server + a httpbin-shaped LocalStack
  target.
- Quickstart `--source s3` runs end-to-end against LocalStack.
- Cross-engine consistency property test extended to include
  `s3_parquet`.

## Stories

Each story RED → GREEN → REFACTOR per [PROCESS.md §5](../PROCESS.md).

- [ ] **S-7.1 — `LoadAssertion::ThroughputAbove`** evaluator —
       compares actual req/s (samples / wall-clock duration of
       the run) against `threshold_rps`.
- [ ] **S-7.2 — `LoadAssertion::StatusCodeIn`** evaluator —
       all samples must have `status_code` in `allowed`.
- [ ] **S-7.3 — `LoadPlan::warmup: Duration` + sample-window
       filtering** in `evaluate_load`.
- [ ] **S-7.4 — LocalStack test scaffolding** (testcontainers
       module for LocalStack S3 + helper to seed an object).
- [ ] **S-7.5 — Cross-engine property test** extended to `s3`.
- [ ] **S-7.6 — Quickstart `--source s3`** wired to use the
       LocalStack scaffolding.
- [ ] **S-7.7 — httpbin-shaped load-probe demo** (example
       binary or doc-test) hitting a LocalStack httpbin target
       + showing assertion output.
- [ ] **S-7.8 — Sprint close** (CHANGELOG / retro / learnings
       / sprint-08 stub for Phase 5).

## Definition of Done

- [ ] All Sprint 7 tests green in CI
- [ ] All prior-phase gates still green
- [ ] CI workflow green on the sprint branch
- [ ] PR opened and merged into `main`
- [ ] CHANGELOG entry under `## [Unreleased]` for Phase 4b
- [ ] httpbin-style demo runs end-to-end (LocalStack-backed)
- [ ] Quickstart `--source s3` runs end-to-end
- [ ] Cross-engine property test green on all 4 source kinds
       (postgres + duckdb + parquet + s3_parquet)
- [ ] Retro filled below

## Out of scope (deferred)

- Load-probe VU mode (closed-model load generation) — Sprint 8.
- Postgres SQL adapter for load probes — Sprint 8.
- Distributed load (multi-process / multi-host) — beyond v0.1.
- Streaming Parquet from S3 (`ParquetObjectReader`) — beyond
  v0.1.
- Latency assertions other than p99 (`p50_under`, `p95_under`)
  — wait for a real ask.

## Risks

1. **LocalStack startup cost on CI.** The S3 service alone
   starts in ~10s. Bundling httpbin too could push this to ~30s.
   Mitigation: scope the LocalStack container to a single tokio
   test that fans out (same trick as `cross_engine_consistency`).
2. **Throughput measurement on busy CI.** Real RPS on a
   contended runner won't hit `plan.rps` exactly. Same loose-
   tolerance pattern as the scheduler test (`±20%` rather than
   strict equality).
3. **Warmup interaction with short runs.** A 10ms warmup on a
   20ms run leaves only 10ms of measurable samples. Document
   the lower-bound; reject `warmup >= duration` at validation
   time.

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
