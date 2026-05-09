# Learnings — ematix-probe

Append-only log of findings, surprises, and rules-of-thumb worth
remembering. Add an entry any time you'd want a future contributor (or
future-you) to know something that isn't obvious from the code.

Format: one entry per finding, dated, tagged. Don't edit old entries —
correct them with a new entry that supersedes.

Tags: `process`, `tooling`, `architecture`, `perf`, `tdd`, `drift`,
`rust`, `python`, `pyo3`, `ci`.

---

## 2026-05-06 — Project kickoff `process`

Decisions made before any code:
- TDD is non-negotiable for this project (no test → no implementation
  commit).
- 1-week sprints, retro at end of every sprint, learnings logged here.
- PI-1 is 10 sprints targeting v0.1 on PyPI.
- Process docs ([PROCESS.md](PROCESS.md), [PI_PLAN.md](PI_PLAN.md),
  per-sprint files, this log) are kept honest in the same PR as code
  changes that affect them.

Why this matters: ematix-flow shipped without an explicit cadence and
the late phases lost track of which decisions were intentional vs.
accidental. Doing the planning + retro work up front for ematix-probe
is the lesson learned.

## 2026-05-06 — Gate `pyo3/extension-module` behind a feature flag `pyo3` `tooling`

In Phase 0 the workspace `pyo3` dep enabled `extension-module` directly,
which broke `cargo test --workspace`: cargo builds the `_core` cdylib's
test binary as a normal executable, but `extension-module` tells pyo3
*not* to link libpython (the host process is supposed to provide it).
No host = linker error on `__Py_Dealloc`, `__Py_NoneStruct`, etc.

Fix: define `extension-module` as a *crate feature* on `ematix-probe-py`
that forwards to `pyo3/extension-module`, and have maturin enable it via
`[tool.maturin] features = ["ematix-probe-py/extension-module"]`. Plain
`cargo test` doesn't enable the feature → tests link against a real
libpython → green.

Apply this pattern to every future PyO3-bound crate.

## 2026-05-06 — pyo3 0.28 API churn: `allow_threads` → `detach`, `Clone` + `pyclass` needs explicit opt-in `pyo3` `rust`

Two friction points crossing the Python boundary in S-2.6 / S-2.7:

1. `Python::allow_threads(...)` was renamed to `Python::detach(...)`
   in pyo3 0.28. Same semantics (release the GIL while a closure
   runs blocking work like `runtime.block_on`), new name.

2. `#[pyclass]` types that derive `Clone` no longer auto-derive
   `FromPyObject`. Without an opt-in, you get a deprecation warning;
   with `-D warnings` in CI, that's a build failure. Fix:
   `#[pyclass(..., from_py_object)]` for types that need to round-
   trip through Python args (e.g. `Vec<PyAssertion>` in
   `ProbePlan::new`).

When upgrading pyo3 in this repo: grep for `allow_threads` and for
`#[pyclass(...)]` blocks paired with `#[derive(Clone)]` and add the
explicit opt-ins.

## 2026-05-06 — Postgres infers parameter types from the LHS column `pyo3` `tooling` `rust`

S-2.5 (`Between` assertion) with f64 bounds and an INT column hit:
`error serializing parameter 0`. The query `WHERE col < $1` made
Postgres infer `$1`'s type from `col` (INT). tokio-postgres, told
to send an f64, refused to coerce.

Fix: explicit `$1::float8` cast on the placeholder. Postgres now
sees `$1` as FLOAT8 and applies its own implicit cast on the LHS
column. As a bonus, this lets Between work on INT, BIGINT, NUMERIC,
or DOUBLE PRECISION columns without per-type SQL.

Pattern: when a parameterized SQL fragment compares a typed-Rust
value against a column whose Postgres type may differ, cast the
*placeholder*, not the column — e.g. `$1::float8`, `$1::text`. Keep
the column reference clean so indexes still apply.

## 2026-05-06 — Dev-only deps still trigger `cargo audit` advisories `rust` `tooling` `ci`

Adding `testcontainers-modules` as a `[dev-dependencies]` pulled in
`tokio-tar 0.3.1` (RUSTSEC-2025-0111, file-smuggling via PAX
headers, no fix available) and `rustls-pemfile 2.2.0` (unmaintained
warning, not blocking).

`cargo audit` doesn't distinguish dev-deps from runtime deps —
anything in `Cargo.lock` is fair game. So even a strictly dev-only
test infra triggers CI-fail-by-default.

Pattern for accepting these: edit `.cargo/audit.toml` `ignore = [...]`
and document the (a) why-can't-fix and (b) risk-assessment in the
inline comment AND in SECURITY.md's "Known accepted advisories"
table. **Don't suppress without that paper trail** — quarterly
re-audit cycles depend on it.

## 2026-05-06 — Sprint 1 retro: a 1-week sprint can close in 1 day `process`

Phase 0 was scoped for a 1-week sprint and shipped same-day. Two
implications:

1. **PI-1 dates are now loose.** The 10-sprint, 10-week PI-1 plan
   assumed 1 phase ≈ 1 sprint ≈ 1 week. Phase 0 broke that. We're
   not re-baselining yet — one data point isn't enough — but Sprint
   2 (Phase 1a, real implementation work) is the velocity test.
2. **Mid-sprint scope expansion is OK if explicit.** During Sprint 1,
   user requested mirroring ematix-flow's full CICD (release.yml,
   SECURITY.md, audit configs, bandit/pip-audit) before continuing.
   Wasn't in S-1.1..S-1.6. We did it anyway because the request was
   explicit. Logged as authorized scope expansion in the retro, not
   silent drift. Future rule: **if scope expands mid-sprint, add a
   story to the sprint file before doing the work** (even
   retroactively in the same PR).

## 2026-05-09 — AWS S3 SDKs fall through to IMDS when env-var creds aren't visible on the builder thread `rust` `tooling` `tdd`

S-7.4 — first cut of the LocalStack S3 test:

```rust
std::env::set_var("AWS_ACCESS_KEY_ID", "test");
std::env::set_var("AWS_SECRET_ACCESS_KEY", "test");
let store = AmazonS3Builder::new()
    .with_endpoint(&endpoint)
    // ... (no .with_access_key_id / .with_secret_access_key)
    .build()?;
```

Result: 30-second hang followed by:

```
RetryError { uri: http://169.254.169.254/latest/api/token,
             retries: 10, source: Connect("Host is down") }
```

The AmazonS3 client's credential resolver chain went:
1. Env vars (visible to *this* thread? maybe — varies by test
   runtime + reqwest's connection-pool thread)
2. AWS credentials file (none in CI)
3. IMDS / EC2 metadata service (`169.254.169.254`) — 30s
   timeout per attempt × 10 retries

Fix: pass creds explicitly to the builder.

```rust
let store = AmazonS3Builder::new()
    .with_endpoint(&endpoint)
    .with_access_key_id("test")
    .with_secret_access_key("test")
    .build()?;
```

Pattern: when you *know* the creds at builder time and you're
talking to a non-real-AWS endpoint (LocalStack, MinIO, R2),
**always pass creds explicitly**. Env-var setup is for production
+ from-env() builders; it's needlessly fragile in tests.

Bonus: the same trap bites `boto3` in the Python quickstart, but
boto3's `client(..., aws_access_key_id=, aws_secret_access_key=)`
named args sidestep the SDK chain entirely. Use those over
env vars in test/demo code too.

## 2026-05-08 — `object_store` over `aws-sdk-s3` for "treat it like a byte source" workloads `architecture` `rust` `tooling`

S-6.1 chose `object_store` (Apache crate, ~10 transitive deps,
single-trait abstraction) over `aws-sdk-s3` (~30 transitive deps,
AWS-specific) for the S3 Parquet adapter.

Two wins beyond dep weight:

1. **AmazonS3 + LocalFileSystem implement the same trait.** Tests
   point a `LocalFileSystem` store at a tempdir holding a
   parquet file and exercise the *exact* code path the production
   AmazonS3 store would. No LocalStack needed for adapter-level
   testing; LocalStack is reserved for end-to-end "looks like real
   S3" coverage.

2. **Test seam for free.** The constructor split is:
   - `S3ParquetAdapter::open(bucket, key, region, endpoint_url)` —
     production; builds AmazonS3 internally.
   - `S3ParquetAdapter::from_object_store(store, key)` —
     accepts any `Arc<dyn ObjectStore>`; tests + advanced users
     who need custom auth.

   Same constructor pattern works for any object-store-backed
   adapter we add later (GCS, Azure, etc).

Pattern: when adding a "fetch object from somewhere" adapter,
reach for `object_store` first. Drop down to a vendor-specific
SDK (aws-sdk-s3, gcp_auth, etc) only when you need
service-specific features (bucket policies, presigned URLs,
multipart writes) that `object_store` doesn't expose.

## 2026-05-08 — Inner `pub mod` declarations must come AFTER the module's `//!` doc comment `rust`

S-6.4 first cut had:

```rust
pub mod scheduler;

//! Engine-side load-probe types: ...
```

The compiler rejects the inner doc comment because items have
already appeared in the module. Inner doc comments
(`//!` / `/*! ... */`) must precede every item in their module.

Fix: put `pub mod` declarations *after* the module-level `//!`
comment. Module structure:

```rust
//! ...module docs...

pub mod child;

use whatever;

// ...rest of module...
```

Trivial but burned a compile cycle; documenting so the next
"add a submodule to a doc-commented module" attempt remembers.

## 2026-05-08 — Terminal-at-build Acc variants store raw verdict, not pre-baked `AssertionResult` `architecture` `rust`

In S-5.3 (SchemaMatch) the schema check is fully decided at
acc-build time — there's no per-batch state to accumulate. The
first cut tried to store the result as
`SchemaMatch { result: AssertionResult }` and just round-trip it
at finalize. That broke immediately: `AssertionResult` carries
`assertion_index`, which isn't known until finalize iterates with
`.enumerate()`.

Fix: store `(verdict, message)` as raw fields in the Acc variant;
finalize wraps with the index it's given.

Pattern for any future terminal-at-build Acc variant: store the
*decision* (verdict + message), not the *report* (which needs an
index). The index is the engine's responsibility, not the
accumulator's.

Trivial in retrospect, but it's the kind of plumbing decision
that's worth writing down because the next "this assertion has
no per-batch state" variant will hit the same temptation.

## 2026-05-08 — DuckDB `:memory:` is per-connection, not per-process `rust` `tooling`

Building `DuckDbAdapter` in S-4.5, the first design opened a fresh
`Connection` inside every `execute` (matched the Postgres adapter
pattern, where every execute pulls from a shared pool of
short-lived clients). Tests against `:memory:` failed immediately
with `Catalog Error: Table with name users does not exist`, even
though `execute_setup` had just created it.

Cause: each `:memory:` `Connection::open` is a *new* in-memory
database. The setup created the table in DB #1; the execute looked
in DB #2. File-backed databases would behave the same way for
in-memory state (DuckDB doesn't have a connection pool with shared
cache like SQLite's `mode=memory&cache=shared`).

Fix: hold one `Arc<Mutex<Connection>>` for the adapter's lifetime;
all `execute_setup` + `execute` calls lock and reuse it. Per-call
serialization on the mutex is fine for v0.1 (data probes are not
the hot path).

Pattern: when an in-process embedded DB has a `:memory:` mode,
**check whether it's per-connection or per-process before designing
the adapter's connection lifecycle.** The right shape is usually a
long-lived single connection (or an explicit shared-cache
mechanism if the lib supports it), not the pool-of-fresh-clients
pattern that fits networked servers.

## 2026-05-08 — `cargo deny` runs over the full lock, so any bump can surface old licenses `rust` `ci` `tooling`

Adding `duckdb` in S-4.5 caused `cargo deny check licenses` to
fail on **two licenses that had nothing to do with duckdb**:

- `CC0-1.0` (`tiny-keccak` via the duckdb dep chain — fair game,
  this WAS new)
- `CDLA-Permissive-2.0` (`webpki-roots` via `bollard` via
  `testcontainers` — already in the closure since Sprint 2)

Why CDLA fired now: adding duckdb caused Cargo.lock churn, which
re-resolved transitives, and `webpki-roots` got bumped to a
version that re-declared its license SPDX field with the new
identifier. The license hadn't changed; the declaration had.

Pattern: when a Sprint introduces a new dep, **run `cargo deny
check licenses` before assuming the only new licenses to allow are
the ones you can trace to your new direct dep.** Walk the failure
list; some entries will be old transitives that just got
re-declared. Both are still safe to add — both `CC0-1.0` and
`CDLA-Permissive-2.0` are permissive — but the diagnosis matters
for the audit trail in `deny.toml`'s comments.

## 2026-05-06 — Pin the test Postgres version; PG 14 changed `EXTRACT` return type `tdd` `tooling` `rust`

The freshness adapter (`SELECT EXTRACT(EPOCH FROM (now() - MAX(<col>))) FROM <t>`)
worked in `cargo test` but panicked in the S-3.7 Python example with
`error deserializing column 0`. Cause: `EXTRACT` returns `numeric` in
PG 14+, `double precision` in PG <14. tokio-postgres can deserialize
`double precision` to `f64` natively but needs the `rust_decimal`
feature to handle `numeric`.

The Rust integration tests didn't catch it because
`testcontainers-modules` defaults to `postgres:11-alpine` (still does
in 0.11.6). The Python e2e ran against `postgres:16-alpine` — same
schema, different result shape.

Fix: cast the SQL expression explicitly with `::double precision`.
No-op on older Postgres, correct on newer.

Two transferable lessons:
1. **When integration tests use a service image, pin it to the
   version you support.** Defaults shift silently. We should pick a
   minimum supported PG version (likely 14, since that's what most
   modern envs run) and pin testcontainers-modules to that tag.
2. **Cast at the SQL boundary, not in Rust.** Whenever a Postgres
   expression's result type may differ across server versions, cast
   to a stable type in the SQL itself rather than picking up a
   feature-gated decoder on the Rust side.

## 2026-05-06 — pyo3 `Option<T>` parameters need `#[pyo3(signature = ...)]` defaults `pyo3` `rust`

`assertion_row_count(low: Option<i64>, high: Option<i64>)` originally
required both args to be passed from Python. Adding
`#[pyo3(signature = (low=None, high=None))]` made them optional with
explicit `None` defaults, matching the Python `at_least=`/`at_most=`
keyword-only style the builder needed.

Pattern: `Option<T>` arg in Rust ≠ optional arg in Python signature.
Always pair with `#[pyo3(signature = ...)]` when you want the Python
caller to be able to omit it.

## 2026-05-06 — `set -e` does not catch failures behind a pipe `tooling` `ci`

Ran the Phase 0 gate sweep as `cargo test --workspace 2>&1 | tail -20`
and saw "ALL GREEN" even though the test phase had a linker error. The
last command in the pipeline is `tail`, so its zero exit overrode
cargo's non-zero — `set -e` only checks the *final* exit status.

Fix: don't pipe failure-sensitive commands through `tail` in CI-style
sweeps, OR use `set -o pipefail` to propagate the leftmost non-zero
exit. CI scripts in this repo use neither pipe nor `tail`; humans
running locally should remember that "looks green" ≠ "is green" when
output is filtered.
