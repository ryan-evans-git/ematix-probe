# Sprint 5 — Phase 3: S3 Parquet + distribution assertions

Dates: 2026-05-21 → 2026-05-27
PI: PI-1
Phase: Phase 3
Status: **closed** — 5/8 stories shipped on `phase-3`; S3 work deferred to Sprint 6 (see retro below)

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

- [x] **S-5.1 — `Assertion::PercentileBetween`** (scan-path eval)
- [x] **S-5.2 — `Assertion::CardinalityBetween`** (scan-path eval)
- [x] **S-5.3 — `Assertion::SchemaMatch`** (scan-path eval)
- [x] **S-5.4 — Streaming Parquet scanner** (drop the eager Vec
       collect from S-4.6 in favor of a per-row-group iterator)
- [ ] **S-5.5 — `S3ParquetAdapter`** (local-file Parquet adapter
       refactored to take a `Read + Seek` source; S3 is a
       `tokio` + `aws-sdk-s3` GET wrapped to satisfy that)
- [ ] **S-5.6 — Python `source.s3_parquet(bucket, key, region=)`**
- [x] **S-5.7 — Cross-engine consistency property test**
- [x] **S-5.8 — Sprint close (CHANGELOG / retro / learnings /
       sprint-06 stub).** The `--source s3` half deferred with
       S-5.5 / S-5.6.

## Definition of Done

- [x] All Sprint 5 tests green in CI
- [x] All Phase 0 / 1a / 1b / 2 gates still green
- [x] CI workflow green on the sprint branch
- [ ] PR opened and merged into `main`
- [x] CHANGELOG entry under `## [Unreleased]` for Phase 3
- [ ] Quickstart runs end-to-end against `s3` mode (LocalStack)
- [x] Cross-engine property test green on all 3 implemented source kinds (postgres + duckdb + parquet; s3 deferred to S6)
- [x] Retro filled below

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
- The "scan-path Acc enum" pattern from Sprint 4 absorbed the
  three new distribution assertions cleanly: each was an Acc
  variant + build/update/finalize arm, no engine refactor.
- For PercentileBetween + CardinalityBetween, the sprint plan
  said "scan-path-only — Postgres returns Error". That contract
  surfaced as a positive — the property test (S-5.7) now
  *asserts* Postgres errors on the scan-only assertions, locking
  the v0.1 split in.
- Sprint-04 retro called out "no cross-engine consistency test
  yet" as Dropped. Sprint 5 closed it with S-5.7. Worth carrying
  forward: every time we add a multi-adapter assertion, the
  property test should grow with it.

### Improved
- Extracted two helpers (`is_supported_numeric`,
  `collect_numeric_into`) when adding PercentileBetween — both
  now back `between` and `percentile_between`. Recognized the
  shape during S-5.1 GREEN rather than after, so the dedupe
  happened at the right time.
- For SchemaMatch, decided NOT to check nullability in v0.1.
  Documented at acc-build with a one-liner instead of leaving
  the absence implicit. Saves a future "should this be a bug?"
  conversation.

### Dropped
- **S-5.5 (S3ParquetAdapter), S-5.6 (Python source.s3_parquet),
  and the `--source s3` half of S-5.8.** Two reasonable
  implementation paths surfaced — `aws-sdk-s3` download-to-
  tempfile vs. streaming via `object_store` +
  `ParquetObjectReader` — and committing to either inside Sprint
  5 would have meant either skipping the property test or
  shipping a half-built S3 path. Pushed to Sprint 6, where it
  can lead the sprint and the right approach can be picked
  carefully.
- Per-bound validation on PercentileBetween's `low`/`high`. The
  spec rejects `p` out of `[0, 1]` but does NOT validate `low <=
  high` — a degenerate range just always-fails. Symmetrical with
  Between (which also doesn't pre-validate). Flagged in case a
  future ask wants stricter input checking.

### Learned
- See LEARNINGS entry: per-acc-variant initialization that
  composes `Acc::SomeError { ... }.with_check(...)` reads nicely
  but doesn't compose with stored `AssertionResult` until
  `assertion_index` is known at finalize. Solved by storing
  `(verdict, message)` pre-computed in the Acc variant and
  wrapping at finalize. Generalizable: terminal Acc variants
  decided at build time should store their decision as raw
  fields, not a pre-baked `AssertionResult`.
- Sprint sizing: 8 stories was too many when one of them (S-5.5)
  has high architectural ambiguity. Sprint plans should keep at
  most one "research-needed" story per sprint, or front-load the
  research as its own story.

### Drift?
- Yes — the S3 deferral shifts work from Sprint 5 → Sprint 6.
  Documented openly in the closed sprint file + the new sprint
  file rather than silently moving the goalposts. Sprint 6 (now
  open) absorbs S-5.5 + S-5.6 as its lead stories, plus the
  originally-planned Phase 4a load-probe MVP. May need to spill
  to Sprint 7 if both don't fit; tracked in sprint-06.
