# Sprint 5 — Phase 3: S3 Parquet + distribution assertions

Dates: 2026-05-21 → 2026-05-27
PI: PI-1
Phase: Phase 3
Status: **planned** *(opens once PR for `phase-2` from Sprint 4 merges)*

## Goal

Take the scan path off-disk and add a richer assertion vocabulary
focused on data-distribution properties. Two themes:

1. **S3 Parquet** — the same `ParquetAdapter` shape from Sprint 4,
   but reading objects from object storage instead of a local
   path. First step toward Phase 4's load-test target list (the
   Parquet ingest job in `ematix-flow` lives on S3).
2. **Distribution assertions** — `percentile_between`,
   `cardinality_between`, `schema_match`. These are scan-path-
   only in v0.1 (no clean Postgres pushdown for percentile
   without `percentile_cont` + window) — drives a sharper line
   between "every adapter must implement this" and "scan-path
   covers this".

Per [PI_PLAN.md](../PI_PLAN.md):

> Phase 3 — Data probe S3 + distribution assertions
> (`percentile_between`, `cardinality_between`, `schema_match`)

End of sprint:
- `S3ParquetAdapter` reading a Parquet object given a bucket +
  key + region (LocalStack in tests, real S3 in CI is overkill
  for v0.1 — local-file Parquet adapter from S-4.6 covers the
  read-from-disk path, S3 just changes the byte source).
- `Assertion::PercentileBetween { column, p, low, high }`,
  `Assertion::CardinalityBetween { column, low, high }`,
  `Assertion::SchemaMatch { fields: Vec<(String, ArrowDataType)> }`
  with scan-path implementations.
- Cross-engine consistency property test (sprint-04 retro
  callout): same `ProbePlan` over identical seed data in
  Postgres + DuckDB + Parquet must produce equal `Verdict`s.
- Quickstart gains a `--source s3` mode (LocalStack-backed for
  the demo).

## Stories

Each story RED → GREEN → REFACTOR per [PROCESS.md §5](../PROCESS.md).
Stories sketched in outline; flesh out at sprint kickoff.

- [ ] **S-5.1 — `Assertion::PercentileBetween`** (scan-path eval)
- [ ] **S-5.2 — `Assertion::CardinalityBetween`** (scan-path eval)
- [ ] **S-5.3 — `Assertion::SchemaMatch`** (scan-path eval)
- [ ] **S-5.4 — Streaming Parquet scanner** (drop the eager Vec
       collect from S-4.6 in favor of a per-row-group iterator)
- [ ] **S-5.5 — `S3ParquetAdapter`** (local-file Parquet adapter
       refactored to take a `Read + Seek` source; S3 is a
       `tokio` + `aws-sdk-s3` GET wrapped to satisfy that)
- [ ] **S-5.6 — Python `source.s3_parquet(bucket, key, region=)`**
- [ ] **S-5.7 — Cross-engine consistency property test**
- [ ] **S-5.8 — Quickstart `--source s3` (LocalStack) + sprint
       close**

## Definition of Done

- [ ] All Sprint 5 tests green in CI
- [ ] All Phase 0 / 1a / 1b / 2 gates still green
- [ ] CI workflow green on the sprint branch
- [ ] PR opened and merged into `main`
- [ ] CHANGELOG entry under `## [Unreleased]` for Phase 3
- [ ] Quickstart runs end-to-end against `s3` mode (LocalStack)
- [ ] Cross-engine property test green on all 4 source kinds
- [ ] Retro filled below

## Out of scope (deferred)

- Real S3 in CI (LocalStack is fine for v0.1).
- Adapter-side pushdown for any of the three new assertions
  (Postgres `percentile_cont` is reasonable but waits until
  there's a real ask).
- Parquet *writes* (only reads in v0.1 — flow already owns the
  write path).
- Schema *evolution* matching (`SchemaMatch` is a strict-equality
  check on field names + Arrow data types in v0.1).

## Risks

1. **Streaming Parquet → memory profile.** The per-row-group
   iterator loses the "eager-collect simplicity" of S-4.6.
   Borrow lifetimes between `ParquetRecordBatchReader` and
   `Statement` were the reason we avoided this in Sprint 4. If
   the borrow story doesn't hold up, fall back to per-execute
   read-into-Vec and accept the memory hit on S3 too.
2. **`aws-sdk-s3` cold build cost.** Adds another large transitive
   tree. Already-permissive licenses, but build time may
   regress; measure after S-5.5 and decide if a feature flag is
   worth it.
3. **Distribution assertion semantics on NULLs.** `percentile_
   between` over a column that's mostly-NULL is degenerate.
   Spec: NULLs excluded from the percentile computation; if the
   column has fewer than 2 non-NULL values, return `Error` with
   a "not enough data" message. Document at evaluator time.

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
