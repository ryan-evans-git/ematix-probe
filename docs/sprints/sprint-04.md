# Sprint 4 — Phase 2: scan path + DuckDB / local Parquet adapters

Dates: 2026-05-14 → 2026-05-20
PI: PI-1
Phase: Phase 2
Status: **planned** *(opens once PR for `phase-1b` from Sprint 3 merges)*

## Goal

Move the data probe off pushdown-only execution and add a real
*scan path*: pull Arrow batches from a source, evaluate the
existing v0.1 assertion vocabulary against the batches in Rust,
and ship two new adapters that are scan-only — DuckDB and local
Parquet. The Postgres adapter keeps its pushdown fast path; the
scan path becomes the fallback for sources without SQL.

Per [PI_PLAN.md](../PI_PLAN.md):

> Phase 2 — Data probe scan path — Arrow batches in Rust;
> DuckDB + local Parquet adapters

End of sprint:
- A `Scanner` trait that returns Arrow `RecordBatch` streams.
- All 7 assertions evaluable against an Arrow stream.
- `DuckDbAdapter` (in-process; embedded DuckDB) running the same
  `ProbePlan` end-to-end.
- `ParquetAdapter` reading a local file and producing an
  identical verdict to the same data in Postgres.
- The S-3.7 quickstart gains a `--source duckdb` / `--source
  parquet` flag.

## Stories

Each story RED → GREEN → REFACTOR per [PROCESS.md §5](../PROCESS.md).
Stories sketched in outline; flesh out at sprint kickoff.

- [ ] **S-4.1 — `Scanner` trait + Arrow batch type alias**
- [ ] **S-4.2 — Scan-path evaluator: `not_null` / `unique` / `between`**
- [ ] **S-4.3 — Scan-path evaluator: `regex` / `enum`**
- [ ] **S-4.4 — Scan-path evaluator: `row_count` / `freshness`**
- [ ] **S-4.5 — `DuckDbAdapter` (embedded; uses `duckdb` crate)**
- [ ] **S-4.6 — `ParquetAdapter` (local file; uses `arrow` + `parquet`)**
- [ ] **S-4.7 — Python `source.duckdb(path)` + `source.parquet(path)`**
- [ ] **S-4.8 — Quickstart `--source` flag + CHANGELOG / sprint close**

## Definition of Done

- [ ] All Sprint 4 tests green in CI
- [ ] All Phase 0 / 1a / 1b gates still green
- [ ] CI workflow green on the sprint branch
- [ ] PR opened and merged into `main`
- [ ] CHANGELOG entry under `## [Unreleased]` for Phase 2
- [ ] Quickstart runs end-to-end against all 3 source kinds
- [ ] Retro filled below

## Out of scope (deferred)

- S3 Parquet (Sprint 5 / Phase 3).
- Distribution assertions — `percentile_between`,
  `cardinality_between`, `schema_match` (Sprint 5).
- DuckDB extensions (httpfs, postgres_scanner, etc.); v0.1 only
  covers in-process query against local data.
- Arrow streaming → JSON / JUnit roundtrip beyond what Phase 1b
  already exposes.

## Risks

1. **Scan-path duplication.** Each assertion now has two impls
   (pushdown SQL + Arrow scan). Drift between them creates
   user-visible inconsistency. Mitigation: a shared property
   test that runs the same `ProbePlan` against Postgres and
   Parquet over identical seed data and asserts equal verdicts.
2. **`duckdb` crate build cost.** It vendors a large C++ DuckDB.
   Could double `cargo build` time on cold caches. Mitigation:
   hide behind a feature flag if the regression is severe;
   evaluate after S-4.5.
3. **Arrow + tokio interaction.** `arrow::record_batch` streams
   are usually sync iterators. Decide early whether the scan
   path is async (matches `DataAdapter::execute`) or sync (with
   a `spawn_blocking` boundary).

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
