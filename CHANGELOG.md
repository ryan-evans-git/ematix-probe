# Changelog

All notable changes to `ematix-probe` are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

(no changes since v0.1.3)

## [0.1.3] - 2026-07-17

### Added

- **Composite (multi-column) uniqueness — `Assertion::UniqueGroup`.**
  A new first-class assertion for keys that are unique *jointly*
  rather than per-column. Table-level Python API `t.unique([...])`;
  pushdown SQL on Postgres (`GROUP BY … HAVING count(*) > 1`) and a
  cross-batch `UniqueComposite` accumulator on the scan path
  (DuckDB / Parquet / S3), keyed on Int64/Utf8 tuple parts.
- **Correct composite-key handling in the ematix-flow shim.**
  `probe_from_table` now emits a single joint `unique` over a
  multi-column primary key (previously a per-column `unique` for
  each PK column, which hard-fails a valid composite key), and a
  composite `unique` per declared `__unique_constraints__` group
  (previously never checked).

### Changed

- **Dependency security bumps** clearing every non-ignored
  `cargo audit` / `pip-audit` advisory: pyo3 0.28 → 0.29
  (RUSTSEC-2026-0176/0177), object_store 0.13 → 0.14 (pulls
  quick-xml 0.41 — RUSTSEC-2026-0194/0195), postgres-protocol
  0.6.12, tokio-postgres 0.7.18, quinn-proto 0.11.16, anyhow
  1.0.103, and msgpack 1.2.1 (GHSA-6v7p-g79w-8964). No source
  changes were required.

## [0.1.2] - 2026-05-19

### Changed

- **README rewrite — marketable top.** Mirrors ematix-flow's
  marketable-README pattern: punchy tagline, code-snippet hook
  above the fold, "Why ematix-probe" bullets, refreshed status
  callout. Section content below (Sources, Data probes,
  Assertions, Load probes, pytest, ematix-flow, Run history,
  CLI, Python API, What's shipped, Development, License) is
  unchanged. The pre-rewrite README was the stub from S-10.6
  with a stale "Phase 7 closed / v0.1 lands in Sprint 10"
  status block.
- **`pyproject.toml` description tightened.** Shrunk from a
  330-char paragraph to a flow-style one-liner:
  *"Declarative data-quality + load probes for Python. Rust +
  tokio under the hood."* The longer form read more like an
  intro than a tagline on PyPI search results.
- **CI infrastructure mirrored to ematix-flow** (#17 + #18). mold
  linker (~5-10× faster Rust link phase), cargo-nextest under
  `cargo llvm-cov nextest` (parallel test execution, 88%
  region-coverage gate preserved), uv-based Python dep installs
  via `astral-sh/setup-uv@v4` (warm-cache hits make Python jobs
  drop from ~14 min to ~1 min). All three CI gates compound on
  re-runs.
- **Makefile expanded** with `test` / `test-rust` / `test-python` /
  `fmt` / `lint` / `security` targets so contributors can
  reproduce the CI gates locally (#17).
- **GitHub repo description + topics aligned** with the PyPI
  tagline; topics added for discoverability (`rust`, `python`,
  `pytest`, `postgres`, `duckdb`, `parquet`, `s3`,
  `data-quality`, `data-testing`, `load-testing`, `ematix`).

### Fixed

- **Release runbook drift** (#20). `docs/RELEASE.md` had three
  stale references: the Python matrix listed `{3.11, 3.12, 3.13}`
  (real matrix has been `3.11–3.14` since #15), the wheel count
  was "6 wheels per release" (now 8), and the pre-flight
  checklist still pointed at raw `cargo test` / `pytest` instead
  of the new `make test-rust` / `make test-python` targets. All
  three refreshed.
- **Stale coverage-rationale comments** (#19). After #17 swapped
  the test runner to `cargo llvm-cov nextest`, three comments in
  `ci.yml` still named `cargo test`. Refreshed to runner-agnostic
  "Rust test suite" phrasing.

### Security

- **idna 3.13 → 3.15** (CVE-2026-45409) in
  `requirements-dev.lock`. Dev-only transitive (requests →
  cachecontrol); no impact on shipped wheels.

### Notes

This is a README + metadata release. No public-API or runtime
behavior changes vs v0.1.1 — users installing from PyPI get
identical probe + CLI surfaces, just a clearer project page and
faster CI for contributors.

## [0.1.1] - 2026-05-09

### Fixed

- **Sdist now ships LICENSE + NOTICE.** v0.1.0's wheel uploads
  succeeded but the sdist was rejected by PyPI's metadata
  validator with `400 License-File LICENSE does not exist in
  distribution file`. Switched the `[project]` license
  declaration from the deprecated
  `license = { text = "Apache-2.0" }` form to PEP 639's
  `license = "Apache-2.0"` SPDX expression + explicit
  `license-files = ["LICENSE", "NOTICE"]` glob list. Maturin
  now packs both files into the sdist root and records the
  `License-File:` headers PyPI validates against.

### Changed

- Dropped the legacy `License :: OSI Approved :: Apache Software
  License` trove classifier — PEP 639 deprecates pairing it with
  the SPDX `license` field; the SPDX expression is canonical.

### Notes

v0.1.0 wheels published successfully and remain available on PyPI;
v0.1.1 is identical functionally and adds the sdist for users on
unsupported platforms (Windows / Intel Mac) who fall through the
wheel matrix.

## [0.1.0] - 2026-05-09

First public release. PI-1 complete: data probes (Postgres /
DuckDB / Parquet local + S3) and load probes (HTTP / Postgres SQL,
constant-rate + virtual-user schedulers), pytest plugin with
per-assertion test nodes, ematix-flow integration shim, opt-in
sqlite run history, and the `ematix-probe` Python CLI.

See [`docs/PI_1_RETRO.md`](docs/PI_1_RETRO.md) for the cross-PI
retrospective. Changes below are grouped by sprint as they shipped
during PI-1.

### Added — Phase 8 + Phase 9 (Sprint 10, PI-1 close)

- **Python CLI** (`ematix-probe` console-script via
  `[project.scripts]`):
  - `ematix-probe run <path>` — discover and execute every
    `@probe.data` instance in a Python file; exit non-zero on
    any non-pass verdict.
  - `ematix-probe run <path> --run-history-db <sqlite>` — append
    one row per probe to a sqlite history file.
  - `ematix-probe list <path>` — enumerate probes (name,
    schema.table, assertion count) without execution.
  - `ematix-probe explain <path> <probe>` — print the compiled
    plan (table, source, assertion names) for one probe.
  - `ematix-probe doctor` — environment health check (package
    importable, `_core` extension loaded, adapter dispatch
    discoverable).
  - The Rust binary at `crates/ematix-probe-cli` stays as
    workspace scaffolding but is no longer the user-facing
    entry point — probe discovery lives where probes live.
- **README rewrite** mirroring the sibling
  [ematix-flow](https://github.com/ryan-evans-git/ematix-flow)
  README structure: TOC, install w/ extras table, concept
  walk-through (sources → probes → assertions → load → pytest →
  flow shim → run history → CLI → API), what's shipped,
  development. Prior README was a four-line stub.
- **`docs/RELEASE.md`** — manual runbook for cutting a
  `vX.Y.Z` release: prereqs (PyPI trusted-publisher + GH
  `pypi` environment), version bump, CHANGELOG promotion,
  workflow_dispatch dry-run, tag push, post-publish verify,
  GitHub release. The `release.yml` workflow already wires the
  wheel-build matrix; this doc covers the steps a human still
  has to do.

### Test surface — Sprint 10

- Python: 15 new tests across CLI run / history (8) and CLI
  list / explain / doctor (7). cli.py at 92% line + branch
  coverage; package total at 98%.

### Added — Phase 6 + Phase 7 (Sprint 9, PI-1)

- **pytest plugin** (`ematix_probe.pytest_plugin`) — auto-loads
  via the `pytest11` entry point. Discovers `DataProbe`
  instances at module top level and surfaces each one as a
  `DataProbeCollector` that yields one pytest item per
  assertion. Per-assertion fan-out gives CI a red/green node
  per check rather than one per probe; `RunReport` is cached
  on the collector so `.run()` fires exactly once regardless
  of how many assertions a probe declares.
- **`ematix-flow` integration shim** (`ematix_probe.flow`) —
  `probe_from_table(table_cls, *, source, extend=None)` builds
  a `DataProbe` from any class implementing the duck-typed
  table protocol (`__tablename__`, optional `__schema__`,
  iterable `columns` with `.name / .nullable / .primary_key`).
  Auto-derives `not_null` on non-nullable columns and `unique`
  on PKs; `extend` lets callers layer extra assertions through
  the same fluent `Tester`. Zero hard dependency on
  `ematix-flow` per PRD §6.2.
- **Opt-in run-history persistence**
  (`ematix_probe.run_history.RunHistory(path)`) — stdlib-sqlite3
  persister with a two-table schema (`runs` + `assertions`)
  and PRAGMA `user_version` for future migrations. One row per
  probe execution + one per assertion, joined by `run_id`. No
  new dependency.
- **`DataProbe.assertion_names()`** — public accessor returning
  human labels in plan order. Used by the pytest plugin to
  name per-assertion items; also useful for any caller that
  wants to introspect a probe before running it.

### Decisions — Sprint 9

- **Async PyO3 deferred to v0.2.** v0.1 ships sync `def`
  probes only. PRD §6 / §6.6 / §16 updated; rationale in
  LEARNINGS (2026-05-09). PI risk #4 escape hatch taken
  deliberately to protect the rest of the sprint budget.
- **`--run-history-db` CLI flag deferred.** The Rust CLI is
  still a `--version` skeleton; flag wiring lands when the
  CLI grows the `run` / `list` / `explain` subcommands in
  Sprint 10. The `RunHistory` persistence layer itself shipped.

### Test surface — Sprint 9

- Python: 14 new tests across plugin scaffold (3),
  per-assertion reporting (3), flow shim (5), run history (6).
  All in-process pytester-based — no maturin reinstall
  required to validate the plugin entry-point flow at dev time.

### Added — Phase 5 (Sprint 8, PI-1)

- **VU (virtual-user) closed-model load** alongside the existing
  open-model constant-rate scheduler:
  - New `LoadMode` enum (`non_exhaustive`) with two variants:
    `ConstantRate { rps }` (open) and `VirtualUsers { count }`
    (closed). `LoadPlan.rps` is replaced by `LoadPlan.mode`.
  - `engine::load::scheduler::VuPool` — N concurrent workers
    each looping `request → wait → request` until the plan
    duration elapses. Per-tick `tick_index` assigned by an
    atomic counter; output sorted by `tick_index`.
  - `HttpLoadAdapter::collect_samples` dispatches on `plan.mode`
    — `ConstantRateScheduler` for open, `VuPool` for closed.
- **Postgres SQL load adapter**:
  - `engine::load::postgres::{PostgresTarget, LoadQuery,
    QueryParam, PgLoadPlan}` — DSN + parameterized SQL string
    + ordered typed bind values. No raw interpolation path:
    values can only enter through `LoadQuery::param`.
  - `adapters::load::postgres::PostgresLoadAdapter` —
    deadpool-postgres pool, `prepare` + `query` per tick,
    binds via `tokio_postgres::types::ToSql`. Same open/closed
    dispatch as the HTTP adapter; success maps to
    `status_code: Some(200)`, SQL errors to
    `status_code: None, error: Some(message)`.
- **One evaluator, both target types** — new `LoadProfile` trait
  exposing the four read-only fields the evaluator consumes
  (`duration / warmup / mode / assertions`). Both `LoadPlan` and
  `PgLoadPlan` implement it; `evaluate_load<P: LoadProfile>` is
  one entry point. All four `LoadAssertion` variants
  (P99Under / ErrorRateBelow / ThroughputAbove / StatusCodeIn)
  work against postgres samples with no extra code.
- **Postgres load demo**:
  `cargo run --example postgres_load_demo --package
  ematix-probe-core`. Spins a Postgres testcontainer, seeds a
  small `users` table, drives 10 VUs against
  `SELECT * FROM users WHERE id = $1::bigint` for 2s, evaluates
  all four assertions.

### Test surface — Sprint 8

- Rust: 14 new tests across the LoadMode refactor (2),
  VuPool (5), HTTP-adapter VU mode (1), Postgres target
  shape (5), Postgres adapter integration (3), and
  postgres-typed evaluator (5). All prior tests remained
  green through the `evaluate_load` generic refactor.
- Python: no new tests this sprint (load API still Rust-only —
  Python surface is Phase 7).

### Added — Phase 4b (Sprint 7, PI-1)

- **Two more `LoadAssertion` variants** rounding out the v0.1
  load-probe vocabulary:
  - `ThroughputAbove { threshold_rps }` — actual req/s
    (samples / wall-clock seconds) at or above the threshold.
    Counts every attempted request including connection
    failures (the "did the scheduler keep up?" assertion); pair
    with `ErrorRateBelow` for the success angle.
  - `StatusCodeIn { allowed: Vec<u16> }` — every sample's
    `status_code` must be in `allowed`. Connection failures
    count as violations. Failure message lists the first 5
    offenders with a "+N more" suffix.
- **`LoadPlan::warmup: Duration`** + sample-window filtering in
  `evaluate_load`. Samples with `tick_index <
  floor(warmup_secs * rps)` are dropped before any per-assertion
  evaluation. `ThroughputAbove` uses `(duration - warmup)` as
  its denominator. `warmup >= duration` rejected as Error
  per assertion.
- **LocalStack test scaffolding** (`testcontainers-modules`
  `localstack` feature) + new tests:
  - `s3_parquet_localstack.rs`: end-to-end S3ParquetAdapter
    against a LocalStack S3 container. Locks in the production
    AmazonS3 path (not just `LocalFileSystem`).
  - `cross_engine_consistency.rs`: extended to compare *4*
    engines (postgres + duckdb + parquet + s3_parquet) on the
    same seed bytes.
- **Quickstart `--source s3`**: spins LocalStack, creates a
  bucket, has DuckDB write parquet locally, boto3-uploads it,
  then probes via `source.s3_parquet`. Same Verdict shape as
  the other backends.
- **Rust load-probe example**:
  `cargo run --example load_probe_demo`. Spins an in-process
  httpbin-shaped responder + drives a 25-RPS / 2s plan
  exercising all 4 v0.1 load assertions. (Standalone Rust
  example because the load API doesn't have a Python surface
  yet — Phase 7 / Sprint 9 work.)
- **Python dev extras**: `testcontainers[postgres,localstack]`
  + `boto3>=1.34`. requirements-dev.lock regenerated.

### Test surface — Sprint 7

- Rust: 16 new tests + the cross-engine extension (1 LocalStack
  + 6 ThroughputAbove + 5 StatusCodeIn + 4 warmup + s3 added
  to the existing consistency test).
- Python: no new tests (the s3 quickstart is a demo, not a
  test); existing 64 still green.

### Added — Phase 3 closeout + Phase 4a (Sprint 6, PI-1)

#### Phase 3 closeout (carry-over from Sprint 5)

- **`S3ParquetAdapter`** (`adapters::data::s3_parquet`) — built
  on the `object_store` crate so the same trait object backs
  AWS S3, LocalStack, MinIO, R2, and a `LocalFileSystem` impl
  for tests.
  - `S3ParquetAdapter::open(bucket, key, region, endpoint_url)`
    for production. `endpoint_url` is the LocalStack/MinIO/R2
    knob.
  - `S3ParquetAdapter::from_object_store(store, key)` for tests
    (or for users who want custom object-store auth).
  - `execute` downloads the object to a tempfile and delegates
    to the existing `ParquetAdapter`. Trades S3 byte-range
    streaming for a one-line implementation that reuses the
    scan-path; streaming via `ParquetObjectReader` is the
    eventual destination.
- **Python `source.s3_parquet(bucket, key, region, endpoint_url)`**
  + pyo3 dispatch. `Source` dataclass extended with optional
  `s3_bucket` / `s3_key` / `s3_region` / `s3_endpoint` fields
  (explicit rather than packed into `url` query params).
  `parquet()` already rejected `s3://` URLs in Phase 2; that
  pointer is now real.

#### Phase 4a — Load probe HTTP MVP

- **New module `engine::load`**: parallel to `engine::data` for
  load tests.
  - `HttpTarget { method, url }` + `HttpTarget::get(url)`. v0.1
    only `GET`.
  - `LoadPlan { target, duration, rps, assertions }`.
  - `LoadAssertion::{ P99Under { metric, threshold_ms } |
    ErrorRateBelow { threshold } }` — `non_exhaustive`.
  - `Sample { tick_index, latency, status_code, error }`. Lives
    in `engine::load` so evaluators can consume it without
    crossing into `adapters::load`.
  - `evaluate_load(plan, &[Sample]) -> RunSummary` — same
    `Verdict` + `reduce_verdict` as `engine::data`, so callers
    see one consistent verdict-reduction story across data +
    load probes.
- **`engine::load::scheduler::ConstantRateScheduler`** — emits
  `Tick`s at a target RPS for a fixed duration. Tick #i
  scheduled at `start + i / rps`; lazy start so `new()` doesn't
  race the wall clock. `fired_at` is recorded so downstream can
  measure scheduler drift.
- **`adapters::load::http::HttpLoadAdapter`** — `reqwest` GETs
  driven by the scheduler; `tokio::spawn` per tick (k6-style
  open model) so the next tick can fire on schedule even if a
  prior request is still in flight. Per-request timeout =
  `plan.duration + 5s`.
- **P99Under evaluator**: nearest-rank method on a buffered
  Vec<f64>ms (mirrors `PercentileBetween` for data probes).
  Filters out connection-error samples; v0.1 only computes
  `metric == "latency_ms"` (other strings → Error).
- **ErrorRateBelow evaluator**: counts (connection failure ∪
  non-2xx status) / total samples; same definition as k6 /
  vegeta / locust. Threshold out of `[0, 1]` (incl NaN) →
  Error. Empty samples → Error.
- **`reqwest`** promoted from a transitive (via
  testcontainers/bollard) to a direct dep with the minimal
  `rustls-tls` feature set.

The load probe API is Rust-only in Phase 4a. Python wrapping +
pytest integration land in Phase 7 (Sprint 9).

### Test surface — Sprint 6

- Rust: 31 new tests across S3 (3) + load skeleton (4) +
  scheduler (5) + HTTP adapter (3) + P99 evaluator (5) +
  ErrorRate evaluator (6) + S3 coverage (already in S-6.1 file).
- Python: 6 new tests for `source.s3_parquet` + dispatch.

### Added — Phase 3 (Sprint 5, PI-1)

- **Distribution assertions** — three new assertion variants
  evaluable on the scan path (Postgres adapter returns
  `Verdict::Error` with a "scan-path only — use DuckDB / Parquet"
  message until a pushdown is justified):
  - `Assertion::PercentileBetween { column, p, low, high }` —
    `p ∈ [0.0, 1.0]`. Buffers non-NULL `f64`-cast values across
    batches, sorts at finalize, picks `values[floor(p * (n-1))]`
    (nearest-rank method). NULLs excluded; empty / all-NULL →
    `Error`. Memory: O(non_null_count); t-digest streaming
    deferred until a real workload pushes through enough rows
    to matter.
  - `Assertion::CardinalityBetween { column, low, high }` —
    same `Option<i64>` bound shape as `RowCount`. HashSet
    accumulation across batches; NULLs not counted (matches SQL
    `COUNT(DISTINCT col)`). Supported column types: `Int64`,
    `Utf8` (same set as `Unique`).
  - `Assertion::SchemaMatch { fields }` — strict equality on
    `(name, ArrowDataType)` tuples in order. Decided at
    acc-build (the scanner schema is known the moment it's
    opened), so empty-stream probes still produce a meaningful
    Verdict. Nullability not checked (DuckDB / Parquet readers
    often surface non-NULL columns as nullable).
- **Streaming Parquet scanner** (S-5.4 refactor): drops the
  S-4.6 eager `Vec<RecordBatch>` collect. The
  `ParquetRecordBatchReader` lives in the scanner behind
  `Arc<Mutex<Option<...>>>` so each `next_batch` does
  `spawn_blocking { reader.next() }`. Memory profile:
  O(row_group_size) instead of O(file).
- **Cross-engine consistency property test**
  (`tests/cross_engine_consistency.rs`): same seed data in
  Postgres + DuckDB + Parquet, same `ProbePlan`, must produce
  equal Verdicts. Two suites — 7-assertion "core" (all 3
  engines must agree) + 3-assertion "scan-only" (DuckDB +
  Parquet agree, Postgres Errors). Catches drift between
  pushdown SQL and scan-path evaluators before user-visible
  inconsistency ships.

### Deferred — Phase 3 punt

- **S3 Parquet adapter** (S-5.5 / S-5.6 / S-5.8 `--source s3`):
  pushed to Sprint 6. Two reasonable paths (download-to-tempfile
  with `aws-sdk-s3`, or streaming via `object_store` +
  `ParquetObjectReader`) and the Sprint 5 cut needed to ship
  before that decision matured. Tracked in sprint-05 retro and
  sprint-06.

### Added — Phase 2 (Sprint 4, PI-1)

- **Scan path** (`engine::scan`): pull-based `Scanner` trait
  yielding Arrow `RecordBatch`es + a shared `evaluate(plan,
  scanner)` that runs every v0.1 assertion (`not_null`, `unique`,
  `between`, `regex`, `enum`, `row_count`, `freshness`) against
  the stream. Same `Verdict`/message contract as the Postgres
  pushdown adapter from Sprints 2–3.
  - Per-assertion accumulators built up-front from the scanner's
    schema, so missing-column and unsupported-type errors surface
    without scanning a row.
  - `unique` HashSets keyed on `i64` / `String`; `between` casts
    Int8/16/32/64 + UInt8/16/32/64 + Float32/64 to f64; `regex`
    via the `regex` crate (RE2-flavored, differs subtly from
    Postgres POSIX); `freshness` tracks MAX across batches in the
    column's native Arrow `TimeUnit` and compares against
    `SystemTime::now()` at finalize.
  - `reduce_verdict` lifted from `adapters::data::postgres`
    (`pub(crate)`) to `engine::data` (`pub`) so the scan path
    shares it.
- **`DuckDbAdapter`** (`adapters::data::duckdb`): in-process
  DuckDB via the `duckdb` crate (`bundled` feature). Holds one
  `Arc<Mutex<Connection>>` for the adapter's lifetime so
  `:memory:` databases persist across `execute_setup` +
  `execute`. All sync DB calls run inside
  `tokio::task::spawn_blocking`. Per-`execute`: `SELECT * FROM
  <qualified_table>` → eager-collect into `Vec<RecordBatch>` →
  scan-path evaluator.
- **`ParquetAdapter`** (`adapters::data::parquet`): local Parquet
  files via the `parquet` crate. Eager-loads via
  `ParquetRecordBatchReaderBuilder`. The Parquet file *is* the
  table — `ProbePlan::table` / `schema` are ignored.
- **Python source factories**: `source.duckdb(path)` and
  `source.parquet(path)` join `source.postgres(url)`.
  `source.parquet` rejects `s3://` URLs explicitly (Phase 3).
- **pyo3 entry points**: `run_duckdb_probe`, `run_parquet_probe`,
  and `duckdb_setup` (test/example seeding helper). `DataProbe.
  run()` now dispatches on `source.kind`.
- **Quickstart `--source` flag**: `python examples/quickstart/
  run.py --source {postgres,duckdb,parquet}`. The two scan-path
  backends need no Docker; the parquet variant has DuckDB write
  the file via `COPY TO`.
- **License allow-list**: added `CC0-1.0` (`tiny-keccak` via
  duckdb chain) and `CDLA-Permissive-2.0` (`webpki-roots` via
  testcontainers/bollard) to `deny.toml`.

### Test surface — Phase 2

- Rust: 24 new scan-path + adapter tests (3 trait basics + 6/7/10
  evaluator coverage + 4 DuckDB + 3 Parquet — 40 total
  non-Docker green; postgres tests skip locally without Docker
  but run on CI).
- Python: 9 new tests (6 source builders, 3 e2e duckdb/parquet —
  no Docker needed for any of them). 67 total Python tests green.

### Added — Phase 1b (Sprint 3, PI-1)

- **Extended assertion vocabulary**: `regex`, `enum`, `row_count`,
  `freshness`. Each pushes down a single `count(*)` (or `MAX(col)`
  for freshness) so the wire returns one row per check regardless
  of table size.
  - `Assertion::Regex { column, pattern }` — Postgres POSIX
    `<col> !~ $1`. NULL-safe: NULLs are not counted as violations
    (pair with `not_null` to forbid).
  - `Assertion::Enum { column, allowed }` — variable-arity
    `<col> NOT IN ($1, $2, ...)`. Empty `allowed` rejected as
    `AdapterError::Config`.
  - `Assertion::RowCount { low, high }` — `Option<i64>` bounds;
    both `None` rejected as `Config` (asserts nothing).
  - `Assertion::Freshness { column, within_seconds }` —
    `EXTRACT(EPOCH FROM (now() - MAX(<col>)))::double precision`.
    Empty table → `Fail` (no signal); negative `within_seconds`
    → `Config`. Cast keeps the result f64-deserializable across
    PG 11..PG 16+.
- **Python builders** for all 7 assertions:
  `t.column(...).regex(p)`, `.is_in([...])`, plus table-level
  `t.row_count(at_least=, at_most=)` and `t.freshness(col,
  within="24h")`. Duration grammar is `<int><unit>` where
  `unit ∈ {s, m, h, d}`; lives in `python/ematix_probe/duration.py`.
- **Machine-readable reports** (`python/ematix_probe/report.py`):
  - `RunReport.write_junit(path)` — GitHub-Actions-compatible
    JUnit XML; one `<testsuite>` per probe, one `<testcase>` per
    assertion, `<failure>` on Fail / `<error>` on Error.
  - `RunReport.write_json(path)` — stable JSON schema documented
    in `tests/test_json_report.py`.
  - `DataProbe.run()` now returns a Python `RunReport` wrapping
    the pyo3 result with probe name / table / schema / timestamps
    / per-assertion human names.
- **End-to-end example** (`examples/quickstart/`): runnable script
  + README that boots a Postgres testcontainer, seeds intentional
  violations, runs all 7 assertions, writes JUnit + JSON, and
  shows the GitHub Actions reporter wiring.
- **Test surface grows to 25 Rust + 41 Python tests** (was 16
  total at end of Phase 1a): +14 Rust integration tests for the
  new assertions; +25 Python tests across duration parser,
  freshness builder, JUnit writer, and JSON writer.

### Fixed — Phase 1b

- Freshness adapter panicked on PG 14+ because
  `EXTRACT(EPOCH FROM ...)` returns `numeric` there (was
  `double precision` on PG <14). Cast explicitly to
  `double precision` — no-op on older versions, correct on
  newer. Surfaced by the S-3.7 example running against
  `postgres:16-alpine` while the integration tests used
  `postgres:11-alpine` (testcontainers-modules' default).

### Added — Phase 1a (Sprint 2, PI-1)

- **Engine foundation** (`crates/ematix-probe-core`):
  `engine::data::{Verdict, Assertion, ProbePlan, AssertionResult, RunSummary}`
  + `adapters::data::{AdapterError, DataAdapter}`. Module tree
  matches the architecture target in PRD §8.1
  (`engine::{data,load}` + `adapters::{data,load}`).
- **`PostgresAdapter`** with eager `SELECT 1` connection
  validation, `tokio-postgres` + `deadpool-postgres` pool, and
  identifier-quoting helpers (`quote_ident`, `qualified_table`).
- **Pushdown SQL** for the v0.1 column-level assertion vocabulary:
  `not_null`, `unique`, `between` (inclusive range, NULL-safe,
  works on INT / BIGINT / NUMERIC / DOUBLE PRECISION via
  `$1::float8` placeholder cast).
- **Python `@probe.data` decorator + fluent `Tester` builder**
  (`python/ematix_probe/probe.py`, `source.py`):
  ```python
  @probe.data(source=source.postgres(url), table="users")
  def quality(t):
      t.column("email").not_null()
      t.column("user_id").unique()
      t.column("age").between(0, 120)
  report = quality.run()
  ```
- **PyO3 bindings** (`crates/ematix-probe-py`): `ProbePlan`,
  `Assertion`, `AssertionResult`, `RunReport` Python types +
  assertion factory functions + `run_postgres_probe(url, plan)`
  end-to-end entry point. GIL-released via `py.detach` while the
  tokio runtime drives the async adapter.
- **Test surface**: 16 tests across 4 levels —
  6 Rust unit tests (version + identifier-quoting helpers);
  1 Rust integration (empty-plan basics);
  4 Rust integration (Postgres assertion behavior, testcontainers);
  2 Rust integration (Postgres connect/validate);
  1 CLI integration;
  6 Python pytest (decorator + Source + chained calls);
  2 Python pytest (full e2e against Postgres testcontainer).

### Added — Phase 0 (Sprint 1, PI-1)

- Rust workspace skeleton (`ematix-probe-core`, `ematix-probe-cli`,
  `ematix-probe-py`) with maturin build via `pyproject.toml`.
- `ematix-probe-core::version()` returning `"0.1.0-dev"`.
- `ematix-probe` CLI binary (`--version` only; subcommands land in
  later phases per [PRD §7](docs/PRD.md)).
- PyO3 binding crate exposing `__version__` to the `ematix_probe`
  Python package.
- GitHub Actions CI workflow: `rust` (fmt + clippy + test),
  `audit-rust` (cargo-audit vs RustSec DB), and `python` matrix on
  py3.11 / 3.12 / 3.13 (maturin build + ruff + bandit + pip-audit
  + pytest).
- Release workflow (tag-gated, manylinux_2_28 + macOS aarch64
  wheels + sdist + PyPI trusted publisher).
- `SECURITY.md` (vuln reporting, automated CI gates, threat model).
- `.cargo/audit.toml` (advisory ignore list, currently empty).
- `.cargo/config.toml` (PyO3 macOS link flags).
- Project process docs: [PRD](docs/PRD.md), [PROCESS](docs/PROCESS.md)
  (TDD + sprint cadence), [PI plan](docs/PI_PLAN.md), per-sprint
  files under [docs/sprints/](docs/sprints/), [LEARNINGS](docs/LEARNINGS.md).

[Unreleased]: https://github.com/ryan-evans-git/ematix-probe/compare/...HEAD
