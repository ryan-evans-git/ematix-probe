# Sprint 6 — Phase 3 closeout (S3) + Phase 4a (Load probe HTTP MVP)

Dates: 2026-05-28 → 2026-06-03
PI: PI-1
Phase: Phase 3 (closeout) + Phase 4a
Status: **planned** *(opens once PR for `phase-3` from Sprint 5 merges)*

## Goal

Two themes, both load-bearing for the v0.1 release:

1. **Close out Phase 3** by shipping the S3 Parquet adapter that
   slipped from Sprint 5 (S-5.5 / S-5.6 / quickstart `--source
   s3`). Lead the sprint with this so it lands before
   Phase 4a work starts.
2. **Open Phase 4a** — load probe HTTP MVP per PI plan:
   constant-rate scheduler, OTel ExponentialHistogram for
   latency, `p99_under` and `error_rate_below` assertions
   against an HTTP target.

End of sprint:
- `S3ParquetAdapter` reading objects from any S3-compatible
  endpoint; LocalStack-backed in tests.
- Quickstart `--source s3` runs end-to-end against LocalStack.
- `LoadProbe` (separate from `DataProbe`) decorator + a load-
  side `Tester` builder for HTTP targets.
- `engine::load::scheduler` constant-rate scheduler producing
  request "ticks" at a configured RPS.
- HTTP adapter using `reqwest` (already in our closure via
  `bollard`) that the scheduler drives.
- `Assertion::P99Under { metric: "latency_ms", threshold }` and
  `Assertion::ErrorRateBelow { threshold }` evaluated against
  an OTel ExponentialHistogram of latencies.

## Stories

Each story RED → GREEN → REFACTOR per [PROCESS.md §5](../PROCESS.md).

### Phase 3 closeout (carry-over from Sprint 5)

- [ ] **S-6.1 — `S3ParquetAdapter`** (S-5.5 carry-over). Pick
       between `aws-sdk-s3` download-to-tempfile vs.
       `object_store` + `ParquetObjectReader`; the right call
       depends on whether the streaming reader composes with our
       existing `Scanner` async trait without a major refactor.
- [ ] **S-6.2 — Python `source.s3_parquet(bucket, key, region=)`**
       + pyo3 dispatch + quickstart `--source s3`
       (LocalStack-backed for the demo) (S-5.6 + S-5.8 leftover).

### Phase 4a — Load probe HTTP MVP

- [ ] **S-6.3 — `engine::load` skeleton** (Verdict reduction
       reused from `engine::data`; new types: `LoadPlan`,
       `LoadAssertion`, `LoadSummary`).
- [ ] **S-6.4 — Constant-rate scheduler** producing `Tick`s at a
       configured RPS via tokio interval; verifies inter-tick
       drift under a target threshold.
- [ ] **S-6.5 — HTTP adapter** that consumes `Tick`s from the
       scheduler and issues `reqwest` GETs against a target;
       records (latency, status_code) per tick.
- [ ] **S-6.6 — `LoadAssertion::P99Under`** evaluator backed by an
       OTel-shaped ExponentialHistogram (max relative error
       configurable; default ~1%).
- [ ] **S-6.7 — `LoadAssertion::ErrorRateBelow`** evaluator
       counting non-2xx responses.
- [ ] **S-6.8 — Sprint close.** httpbin-style end-to-end
       example + CHANGELOG / retro / learnings / sprint-07 stub.

## Definition of Done

- [ ] All Sprint 6 tests green in CI
- [ ] All Phase 0 / 1a / 1b / 2 / 3 (other-than-S3) gates still
       green
- [ ] CI workflow green on the sprint branch
- [ ] PR opened and merged into `main`
- [ ] CHANGELOG entries under `## [Unreleased]` for Phase 3
       closeout AND Phase 4a
- [ ] Quickstart runs end-to-end against `--source s3` (LocalStack)
- [ ] Cross-engine property test extended with the s3 source
       kind
- [ ] httpbin-style load-probe demo runs end-to-end
- [ ] Retro filled below

## Out of scope (deferred)

- Real S3 in CI (LocalStack covers v0.1).
- Load-probe VU mode (Sprint 8 / Phase 5).
- Postgres SQL adapter for load probes (Sprint 8 / Phase 5).
- Distributed load (multi-process / multi-host load generation).
- Latency assertions beyond p99 (`p50_under`, `p95_under`, etc
  are trivial follow-ups but wait until there's a real ask).

## Risks

1. **Sprint scope.** Two sprint themes is the most this project
   has done in one sprint. If the S3 path takes more than ~2
   stories, the load-probe MVP scope may need to spill to
   Sprint 7 (which already has Phase 4b polish work). Decision
   gate: end of S-6.2; if it ate more than half the sprint,
   move S-6.7 to Sprint 7 and ship Phase 4a as
   `P99Under`-only.
2. **OTel ExponentialHistogram complexity.** Implementing one
   from scratch is non-trivial; using `opentelemetry-rust`'s
   existing impl drags a large dep. Mitigation: start with the
   crate's impl, evaluate dep weight after S-6.6.
3. **Load test flakiness.** Constant-rate scheduling on a busy
   CI runner won't be exact. Tests should assert ranges (within
   N% of target) not exact rates.

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
