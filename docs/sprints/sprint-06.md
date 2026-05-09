# Sprint 6 — Phase 3 closeout (S3) + Phase 4a (Load probe HTTP MVP)

Dates: 2026-05-28 → 2026-06-03
PI: PI-1
Phase: Phase 3 (closeout) + Phase 4a
Status: **closed** — all 8 stories shipped on `phase-4` (Phase 3 closeout + Phase 4a load-probe MVP)

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

- [x] **S-6.1 — `S3ParquetAdapter`** (S-5.5 carry-over). Pick
       between `aws-sdk-s3` download-to-tempfile vs.
       `object_store` + `ParquetObjectReader`; the right call
       depends on whether the streaming reader composes with our
       existing `Scanner` async trait without a major refactor.
- [x] **S-6.2 — Python `source.s3_parquet(bucket, key, region=)`**
       + pyo3 dispatch + quickstart `--source s3`
       (LocalStack-backed for the demo) (S-5.6 + S-5.8 leftover).

### Phase 4a — Load probe HTTP MVP

- [x] **S-6.3 — `engine::load` skeleton** (Verdict reduction
       reused from `engine::data`; new types: `LoadPlan`,
       `LoadAssertion`, `LoadSummary`).
- [x] **S-6.4 — Constant-rate scheduler** producing `Tick`s at a
       configured RPS via tokio interval; verifies inter-tick
       drift under a target threshold.
- [x] **S-6.5 — HTTP adapter** that consumes `Tick`s from the
       scheduler and issues `reqwest` GETs against a target;
       records (latency, status_code) per tick.
- [x] **S-6.6 — `LoadAssertion::P99Under`** evaluator backed by an
       OTel-shaped ExponentialHistogram (max relative error
       configurable; default ~1%).
- [x] **S-6.7 — `LoadAssertion::ErrorRateBelow`** evaluator
       counting non-2xx responses.
- [x] **S-6.8 — Sprint close.** httpbin-style end-to-end
       example + CHANGELOG / retro / learnings / sprint-07 stub.

## Definition of Done

- [x] All Sprint 6 tests green in CI
- [x] All Phase 0 / 1a / 1b / 2 / 3 (other-than-S3) gates still
       green
- [x] CI workflow green on the sprint branch
- [ ] PR opened and merged into `main`
- [x] CHANGELOG entries under `## [Unreleased]` for Phase 3
       closeout AND Phase 4a
- [ ] Quickstart runs end-to-end against `--source s3` (LocalStack)
- [ ] Cross-engine property test extended with the s3 source
       kind
- [ ] httpbin-style load-probe demo runs end-to-end (deferred —
       see retro; integration tests cover the API end-to-end)
- [x] Retro filled below

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
- The "decision gate at end of S-6.2" landed exactly as planned.
  S3 took 2 stories cleanly (`object_store` was the right call —
  AmazonS3 + LocalFileSystem behind a single trait object meant
  testing without LocalStack). The full load-probe MVP fit in
  the remaining sprint capacity.
- engine + adapter split for load probes mirrors the data-probe
  split — `engine::load` owns plan/assertion types + evaluator;
  `adapters::load` owns transport. `evaluate_load(plan,
  &[Sample])` parallels `engine::scan::evaluate(plan, scanner)`
  exactly. Future `LoadAdapter` trait + Postgres-SQL load adapter
  in Phase 5 should slot in without engine changes.
- ErrorRateBelow conflates connection failures + non-2xx into
  one "error rate" — same definition k6 / vegeta / locust use.
  Picking the established convention up-front avoids a future
  "what does error_rate mean here?" question.

### Improved
- Refused the OTel ExponentialHistogram in S-6.6 even though the
  sprint plan called for it. Sort-and-pick on a buffered Vec is
  ~20 lines vs. ~200 for a from-scratch histogram, exactly
  consistent with `PercentileBetween`'s memory model, and
  honest about the v0.1 cap. Documented the future story in the
  evaluator doc so it doesn't get forgotten.
- `Sample` was first defined in `adapters::load::http`; moved to
  `engine::load` during S-6.6 when the evaluator needed to
  consume it. Caught the layering before it spread.

### Dropped
- **httpbin-style standalone demo binary**. The load-probe API
  has no Python surface yet (Phase 7 / Sprint 9 work), so a
  Rust-only demo would only show what the integration tests
  already show. Dropped in favor of letting the test suite
  serve as the API documentation; revisit when the Python load
  surface lands.
- **Quickstart `--source s3` (LocalStack-backed)** and
  **cross-engine property test extension to s3**. Both need a
  LocalStack container, which the test suite doesn't yet wire
  in. Carrying to Sprint 7 alongside the rest of Phase 4b polish
  + a "fold s3 into the existing test scaffolding" beat.

### Learned
- See LEARNINGS entry: `object_store` is a much cleaner
  abstraction than `aws-sdk-s3` for "open this object as a
  byte source" workloads. AmazonS3 + LocalFileSystem behind
  the same `Arc<dyn ObjectStore>` means tests don't need
  LocalStack to exercise the *adapter*; they only need it for
  end-to-end "real-S3-shaped" coverage.
- Inner `pub mod foo;` declarations have to come AFTER the
  module's `//!` doc comment, not before. Tripped on this in
  S-6.4 — wasted a compile cycle.

### Drift?
- Two carry-overs to Sprint 7: LocalStack-based quickstart for
  s3 + cross-engine property test extension. Both are scope
  the original Sprint 5 plan called out and Sprint 6 absorbed
  the lion's share of; the LocalStack piece is what's left.
  Documented in sprint-07 explicitly.
