# Sprint 4 — Phase 2: scan path + DuckDB / local Parquet adapters

Dates: 2026-05-14 → 2026-05-20
PI: PI-1
Phase: Phase 2
Status: **closed** — all 8 stories shipped on `phase-2`

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

- [x] **S-4.1 — `Scanner` trait + Arrow batch type alias**
- [x] **S-4.2 — Scan-path evaluator: `not_null` / `unique` / `between`**
- [x] **S-4.3 — Scan-path evaluator: `regex` / `enum`**
- [x] **S-4.4 — Scan-path evaluator: `row_count` / `freshness`**
- [x] **S-4.5 — `DuckDbAdapter` (embedded; uses `duckdb` crate)**
- [x] **S-4.6 — `ParquetAdapter` (local file; uses `arrow` + `parquet`)**
- [x] **S-4.7 — Python `source.duckdb(path)` + `source.parquet(path)`**
- [x] **S-4.8 — Quickstart `--source` flag + CHANGELOG / sprint close**

## Definition of Done

- [x] All Sprint 4 tests green in CI
- [x] All Phase 0 / 1a / 1b gates still green
- [x] CI workflow green on the sprint branch
- [ ] PR opened and merged into `main`
- [x] CHANGELOG entry under `## [Unreleased]` for Phase 2
- [x] Quickstart runs end-to-end against all 3 source kinds
- [x] Retro filled below

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
- Same RED → GREEN cadence with no `_ =>` wildcard arms in RED.
  For S-4.2..S-4.4, the wildcard arm in `Acc::build` was a
  *deliberate* TBD-Error (different intent: produce a Summary
  instead of panicking) — flagged with a comment so it doesn't
  read as a real fallback.
- The `evaluate(plan, scanner)` function gave both DuckDB and
  Parquet adapters a one-line execution path. Adding S3 Parquet
  in Sprint 5 should mostly be: open a Scanner, call evaluate.
- Per-execute connection isolation for Postgres + Parquet, but
  per-adapter long-lived Connection for DuckDB. The DuckDB
  decision came from the `:memory:` test failure on the very
  first run — a reminder to test the simplest possible thing
  *first* rather than designing for a phantom future use case.

### Improved
- Source builder validation: `parquet()` rejects `s3://`
  explicitly. Without that, users who tried it in Phase 2 would
  get a confusing "no such file" from the local opener; the
  early check turns the failure into "Phase 3" pointing.

### Dropped
- The "scan-path duplication" property test (sprint risk #1) was
  not built. Postgres + DuckDB + Parquet do exercise the same
  assertion contract through unit-level seed data, but a
  property test that runs *the same* `ProbePlan` over identical
  rows in all three engines would be stronger. Deferred —
  candidate for Sprint 5 / Phase 3 sweep.
- The build-time evaluation of risk #2 (DuckDB cold-build cost):
  measured ~1m 30s for first compile, then incremental rebuilds
  are subsecond. Acceptable. Not worth gating behind a feature
  flag yet.

### Learned
- See LEARNINGS entry: `:memory:` DuckDB connections are
  **per-connection** — opening a fresh one per execute creates a
  fresh database. The adapter has to hold one Connection for
  its lifetime if any state needs to persist.
- See LEARNINGS entry: `cargo deny check licenses` runs over the
  full Cargo.lock, so adding *any* dep can surface previously-
  unaccounted licenses from existing transitives. Two showed up
  in Sprint 4 (`CC0-1.0`, `CDLA-Permissive-2.0`) that had been
  in the closure since Sprint 2 but only failed CI now because
  Cargo.lock churned.

### Drift?
- None. Sprint scope held: 8/8 stories, no scope creep, no
  deferrals into the sprint that weren't in the original plan.
