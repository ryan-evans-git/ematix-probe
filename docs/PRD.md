# ematix-probe — Product Requirements Document (v0.1 draft)

Status: **approved** — v0.1 scope locked, Phase 0 unblocked. No code yet.
Owner: ryanevans23@gmail.com
Last updated: 2026-05-06

> **Process.** This project runs on TDD + 1-week sprints + retros + an
> append-only learnings log. See [PROCESS.md](PROCESS.md), the active
> [PI plan](PI_PLAN.md), and the current sprint under
> [docs/sprints/](sprints/).

> **Name — decided.** `ematix-probe`. PyPI distribution: `ematix-probe`.
> Python import: `ematix_probe`. CLI binary: `ematix-probe`.

---

## 1. Summary

`ematix-probe` is an open-source Python testing-automation library, with a
Rust core, that lets engineers declare two kinds of probes against a running
data system and have the library execute them, report results, and gate CI:

1. **Data probes** — declarative assertions about the *shape* of data sitting
   in a target (column null rate, uniqueness, ranges, regex, row count,
   freshness, distribution percentiles, schema match).
2. **Load probes** — declarative assertions about the *behavior* of a
   service under synthetic traffic (HTTP and SQL targets, constant-rate or
   VU-driven, with `p99_under` / `error_rate_below` / `throughput_above`
   assertions).

The product's center of gravity is **declarative assertions about a running
system at scale**. Rust earns its keep on both surfaces:

- Data probes scan billions of rows through Apache Arrow without per-row
  Python overhead — the bottleneck for Great Expectations on object-store
  data.
- Load probes drive real OS-thread concurrency without the GIL — the
  bottleneck for Locust.

`ematix-probe` is a **sibling** to `ematix-flow`: it works against any
supported target, but ships with first-class introspection of `ematix-flow`
`ManagedTable` definitions so verifying a flow pipeline is a one-decorator
job.

## 2. Goals

1. Replace hand-written data-quality checks (per-column SQL, ad-hoc Python
   row scans, dbt-test expectations spread across YAML) with one declarative
   `@probe.data` decorator that runs as either pushdown SQL or Arrow scan.
2. Replace single-node load tests written in Locust (Python, GIL-bound) or
   k6 (JavaScript only) with a Python-API load probe whose engine is Rust
   and which can saturate a service from one workstation.
3. Be ergonomic for Python engineers from line one — `pip install
   ematix-probe`, no external services to run a probe against Postgres or
   an HTTP endpoint.
4. Be CI-native: exit code reflects pass/fail, JUnit XML for any CI runner,
   JSON report for downstream tooling.
5. Stay small enough to read end-to-end. v0.1 is a focused library, not a
   platform.
6. Ship to PyPI as `ematix-probe` with one fully documented end-to-end
   example: an `ematix-flow` Postgres pipeline + a data probe asserting on
   the loaded table + a load probe hitting an API that reads from it.

## 3. Non-goals (v0.1)

- **Distributed load generation.** Single-node only. Multi-node coordinator
  is post-v0.1 (see ROADMAP).
- **Browser / UI automation.** Not a Playwright/Selenium competitor. No
  visual-regression, DOM, or CDP work.
- **gRPC, WebSocket, GraphQL, message-queue load.** HTTP/HTTPS and Postgres
  SQL only in v0.1.
- **Drift detection / regression vs. baseline.** v0.1 only stores run
  history (opt-in, see §5.2); each probe verdict is still pass/fail
  against absolute thresholds, not vs.-yesterday. Baseline comparison and
  regression alerting are layered on the persisted store in v0.2.
- **Statistical hypothesis testing** beyond percentile and distribution
  bounds (no t-tests, KS tests, etc.).
- **Mobile / device farms.**
- **Replacing pytest.** `ematix-probe` integrates *with* pytest as a plugin;
  it does not try to be a general-purpose unit-test runner.
- **Backends beyond Postgres / DuckDB / Parquet (local + S3) for data
  probes.** The adapter trait is designed so Snowflake / BigQuery /
  Delta / Iceberg can be added later without breaking the public API.

## 4. Personas

- **Primary — Analytics/data engineer** running an `ematix-flow` (or dbt,
  or hand-rolled) pipeline who currently writes one-off SQL queries to
  spot-check loaded tables. Wants a single decorator that asserts on
  null rates, freshness, and row counts and fails CI when something
  drifts.
- **Primary — Backend engineer** shipping an HTTP API that reads from a
  warehouse table. Currently uses k6 for load tests in JS and writes
  data-quality checks in Python — wants both in one declarative file
  that runs in CI.
- **Secondary — Platform engineer** standing up a "verify the warehouse is
  fast enough" probe that runs hourly against Postgres and pages on p99
  regression.
- **Secondary — Site-reliability engineer** running pre-deploy load
  smoke-tests against a staging environment and wanting an exit code, not
  a Grafana dashboard.

## 5. Core concepts

### 5.1 `Probe`

A probe is a single named, declarative assertion target. Two kinds in v0.1:
`@probe.data` and `@probe.load`. Both compile to a Rust `ProbePlan` that
the engine executes.

### 5.2 `Run`

A single execution of one or more probes, producing a `RunReport`. Runs
have an id, a wall-clock start/end, and a deterministic per-probe verdict
(`pass` / `fail` / `error`). **Run history persistence is an opt-in user
config in v0.1** — by default each run is ephemeral (terminal + JUnit +
JSON), but pointing `[run_history]` at a Postgres URL writes one row per
run plus per-probe and per-assertion detail tables, mirroring the
`_ematix_*` metadata schema `ematix-flow` uses. v0.2 layers
baseline/drift/regression on top of that store; v0.1 just stores.

### 5.3 Pushdown vs. scan (data probes only)

A data probe declares an assertion; the engine chooses execution:

- **Pushdown** — the assertion compiles to one SQL query and runs on the
  target (Postgres, DuckDB). Cheap, accurate, no data movement. Default
  for SQL targets.
- **Scan** — the engine reads Arrow batches from the target (Parquet on
  S3, DuckDB over Parquet) and applies the assertion in Rust. Necessary
  when the target isn't a SQL engine, or when the assertion can't be
  expressed cheaply in SQL (e.g. percentile-of-percentile on a 1B-row
  table where pushdown would OOM the warehouse).

The user does not pick — the engine does, with an explicit override
(`@probe.data(execution="scan")`) for the rare case it gets it wrong.

### 5.4 Load model (load probes only)

Two equally first-class generation modes:

- **Constant rate** — `s.rate("100 rps", duration="60s")`. k6/Vegeta
  semantics. The engine schedules requests off a leaky-bucket clock and
  back-pressures the user only by failing the probe if the target can't
  keep up.
- **Virtual users** — `s.users(50, ramp="30s")`. Locust semantics. N
  cooperative tasks loop through scenarios; throughput is whatever the
  target can absorb.

Both produce the same metrics surface (`p50/p95/p99/p999`, error rate,
throughput, latency histograms). Histograms serialize as **OpenTelemetry
ExponentialHistogram** (OTLP JSON encoding) so reports drop into any
OTel-aware backend (Tempo, Jaeger, Honeycomb, vendor APMs) without a
re-encoder. Internally the engine uses a base-2 exponential bucket
representation that's lossless under OTel merge.

### 5.5 Assertions

Data and load probes share an assertion DSL where it makes sense
(`between`, `at_least`, `at_most`, `equals`, `within`). Each surface adds
its own:

- Data: `not_null`, `unique`, `regex`, `enum`, `freshness`, `row_count`,
  `schema_match`, `percentile_between`, `cardinality_between`.
- Load: `p50/p95/p99/p999_under`, `error_rate_below`, `throughput_above`,
  `status_code_in`.

## 6. Public API (Python)

Decorated probe functions may be either `def` or `async def`. The Rust
engine drives both — a sync function runs in a worker thread; an async
function is awaited on the engine's tokio runtime via `pyo3-asyncio`.
This matters most for load probes whose scenarios issue auxiliary
async I/O (e.g. fetching a fresh JWT before each VU loop).

### 6.1 Data probe — fluent style

```python
from ematix_probe import probe, source

@probe.data(source=source.postgres("DATABASE_URL"), table="analytics.dim_customers")
def customer_dim_quality(t):
    t.row_count().between(1_000, 10_000_000)
    t.freshness("updated_at").within("24h")

    t.column("customer_id").not_null().unique()
    t.column("email").not_null().regex(r".+@.+\..+")
    t.column("age").between(0, 120)
    t.column("country").enum({"US", "CA", "MX", "GB", "DE", ...})
    t.column("ltv").percentile_between(p=99, low=0, high=100_000)

    t.schema_match({"customer_id": "int64", "email": "string", ...})
```

### 6.2 Data probe — `ematix-flow` integration

```python
from ematix_flow import ManagedTable
from ematix_probe import probe

class CustomerDim(ManagedTable):
    __tablename__ = "dim_customers"
    # ... ematix-flow column definitions

@probe.flow_table(CustomerDim)
def customer_dim_baseline(t):
    """Auto-generates schema_match + not_null on PK + freshness on watermark
    column, and lets you layer more assertions on top."""
    t.column("email").regex(r".+@.+\..+")
```

`@probe.flow_table` is a thin shim that introspects the `ManagedTable` and
calls `@probe.data` with sensible defaults derived from the declarative
schema. The generic core has zero `ematix-flow` dependency.

### 6.3 Load probe — HTTP, constant rate

```python
from ematix_probe import probe

@probe.load(target="https://api.example.com")
def customer_lookup_load(s):
    s.scenario("warm cache lookup")
    s.get("/v1/customers/{id}", ids=range(1, 10_000), headers={"Auth": "Bearer ..."})

    s.rate("100/s", duration="60s")
    s.warmup("10s")

    s.expect.p99_under("200ms")
    s.expect.p999_under("1s")
    s.expect.error_rate_below(0.005)
    s.expect.status_code_in({200, 304})
```

### 6.4 Load probe — SQL, virtual users

```python
@probe.load(target="postgres://warehouse")
def warehouse_query_load(s):
    s.query(
        "SELECT count(*) FROM fct_orders WHERE order_date BETWEEN $1 AND $2",
        params=[("2026-01-01", "2026-01-31"), ("2026-02-01", "2026-02-28"), ...],
    )
    s.users(50, ramp="30s", duration="5m")

    s.expect.p95_under("2s")
    s.expect.error_rate_below(0.0)
```

### 6.5 Load probe — async scenario

```python
import httpx
from ematix_probe import probe

@probe.load(target="https://api.example.com")
async def authenticated_lookup_load(s):
    async with httpx.AsyncClient() as client:
        token = (await client.post("/auth/token", json=...)).json()["jwt"]

    s.get("/v1/customers/{id}", ids=range(1, 10_000),
          headers={"Authorization": f"Bearer {token}"})
    s.users(50, ramp="30s", duration="2m")
    s.expect.p99_under("250ms")
```

Async is supported on both `@probe.data` and `@probe.load` — typically
useful for one-shot setup (fetching credentials, warming a cache) inline
with the probe declaration.

### 6.6 pytest plugin

```python
# tests/test_probes.py
import pytest
from ematix_probe.pytest_plugin import probe_runs

@probe_runs(customer_dim_quality)
def test_dim_passes(report):
    assert report.passed
```

A failing `@probe.*` decorator function under pytest fails the test
naturally; the explicit fixture above is for advanced cases (custom
reporting, per-test parameterization). Async probe functions integrate
with `pytest-asyncio` without extra wiring.

## 7. CLI surface

The CLI binary is **`ematix-probe`** — full namespaced name, no clash with
`linux-tools probe`, matches the PyPI/import name. Users who want a
shorter command can alias `ep=ematix-probe` themselves.

```text
ematix-probe run [PATH...]              # discover @probe.* in PATH, run, exit nonzero on fail
ematix-probe run --only data            # data probes only
ematix-probe run --only load
ematix-probe run --report junit.xml --report json:report.json
ematix-probe run --tag prod             # filter by @probe.data(tags=["prod"])
ematix-probe run --history postgres://... # opt-in run history persistence
ematix-probe list                       # show discovered probes, no execution
ematix-probe explain <probe_name>       # show resolved plan (pushdown SQL, generated load schedule)
ematix-probe doctor                     # validate config, target reachability, version skew
```

`ematix-probe run` is the workhorse. `ematix-probe explain` is the
debugging tool that makes the engine's choices transparent — what SQL got
generated, what the request schedule looks like, which adapter was
selected.

## 8. Architecture

Mirrors `ematix-flow` exactly so the two projects share infra patterns and
the user mental model.

```text
ematix-probe/
├── Cargo.toml                       # Rust workspace
├── crates/
│   ├── ematix-probe-core/           # engine, adapter trait, assertion DSL,
│   │                                # Arrow scan, SQL pushdown, HTTP/SQL load drivers
│   ├── ematix-probe-cli/            # `probe` binary
│   └── ematix-probe-py/             # PyO3 bindings — exposes Probe, Run, RunReport
├── python/
│   └── ematix_probe/                # decorators, fluent DSL, pytest plugin,
│                                    # ematix-flow integration shim
├── pyproject.toml                   # maturin build, packaged as ematix-probe
├── docs/
│   ├── PRD.md
│   ├── ROADMAP.md
│   ├── USER_GUIDE.md
│   └── BENCHMARKS.md
├── examples/
└── tests/
```

### 8.1 Rust core layout (`ematix-probe-core`)

- `adapters::data::{postgres, duckdb, parquet}` — implement `DataAdapter`
  trait: `scan_arrow(plan) -> Stream<RecordBatch>`, `pushdown_sql(plan)
  -> Result<RunSummary>`.
- `adapters::load::{http, postgres}` — implement `LoadAdapter` trait:
  `dispatch(request) -> Future<Response>`.
- `engine::data` — assertion → execution-mode chooser, reduces results
  into `Verdict`.
- `engine::load` — scheduler (constant-rate clock or VU pool), latency
  recorder (HDR histogram), reduce → `Verdict`.
- `report` — JUnit XML, JSON, terminal Rich-compatible.

### 8.2 Python layout (`ematix_probe`)

- `probe.data`, `probe.load`, `probe.flow_table` decorators.
- `Tester` (data) and `Scenario` (load) fluent builders that compile to the
  Rust `ProbePlan`.
- `source.postgres`, `source.duckdb`, `source.parquet` factories.
- `pytest_plugin` for native pytest integration.
- `ematix_flow_integration` — optional, only imported when `ematix-flow`
  is installed.

## 9. Backends supported in v0.1

### Data probe targets

| Target | Pushdown | Scan |
|---|---|---|
| Postgres | ✅ default | — |
| DuckDB (in-process or file) | ✅ default | ✅ for non-SQL-expressible assertions |
| Parquet (local FS) | — | ✅ default (via DuckDB or Arrow direct) |
| Parquet (S3) | — | ✅ default |

### Load probe targets

| Target | Mode |
|---|---|
| HTTP / HTTPS | constant-rate + VU |
| Postgres SQL | constant-rate + VU |

Everything else (Snowflake, BigQuery, Delta, Iceberg, gRPC, WebSocket) is
post-v0.1, behind the same adapter trait.

## 10. Reporting

Every `probe run` produces:

1. **Terminal output** — Rich-style summary table, one row per probe, color-
   coded verdict, per-assertion details on failure.
2. **JUnit XML** — `--report junit:path.xml`. Drops directly into GitHub
   Actions / Jenkins / GitLab CI test reporters.
3. **JSON report** — `--report json:path.json`. Stable schema. Includes
   per-probe verdict, per-assertion outcome, latency histograms (load),
   row counts and percentiles (data), and the resolved plan that ran.
4. **Exit code** — `0` if all probes pass, `1` if any probe fails, `2` if
   the run errored before assertions could be evaluated.

Optional in v0.1: Prometheus push-gateway target (`--report prometheus:url`)
for scheduled probes that should fan out into existing alerting.

**Run history persistence** is also opt-in: `--history postgres://...` (or
the `[run_history]` config table) writes one durable row per run plus
per-probe + per-assertion detail tables. Schema mirrors
`ematix-flow`'s `_ematix_*` metadata layout so the two systems can share
a metadata Postgres without colliding. Disabled by default — no infra
needed to use the tool.

## 11. Performance targets (v0.1, M3 Pro)

| Surface | Target |
|---|---|
| Data — pushdown probe (Postgres) | < 50 ms overhead per assertion vs. raw SQL |
| Data — scan probe (100M-row Parquet, S3, 10 cols, 5 column-level assertions) | < 30 s wall-clock |
| Load — HTTP constant-rate (single node, single target) | sustain 50,000 rps |
| Load — Postgres VU (50 users, simple query) | sustain 10,000 qps |
| Cold start | `probe run` discover → first assertion < 200 ms |

These are aspirational v0.1 ceilings; the BENCHMARKS.md doc will report the
real numbers, with method, the same way `ematix-flow` does.

## 12. Test plan

- **Rust unit tests** per crate. Floor for v0.1: 200+ on the core engine,
  covering each adapter, the assertion DSL, pushdown SQL generation, the
  rate scheduler, and the VU pool.
- **Rust integration tests** that spin up real Postgres + DuckDB + a local
  HTTP echo server and run end-to-end probes against them.
- **Python tests** for the decorator surface, the fluent DSL → ProbePlan
  compiler, the pytest plugin, and the `ematix-flow` integration shim.
- **Cross-language round-trip tests** — Python decorator → Rust ProbePlan
  → executed → RunReport → asserted in Python.
- **CI parity with `ematix-flow`** — same lints (clippy, ruff), same
  formatters (rustfmt, ruff), same coverage gates, same release flow.

## 13. Milestones

The `ematix-flow` "phase" cadence is the model: each phase is a vertical
slice that ships passing tests + docs + examples.

- **Phase 0 — workspace skeleton.** Rust workspace, `pyproject.toml` with
  maturin, empty `probe` CLI, CI green.
- **Phase 1 — data probe MVP, Postgres only.** Decorator → ProbePlan →
  pushdown SQL. Column-level assertions: `not_null`, `unique`, `regex`,
  `enum`, `between`. Table-level: `row_count`, `freshness`. JUnit XML +
  JSON reports. End-to-end example.
- **Phase 2 — data probe scan path.** Arrow batch scan in Rust. DuckDB +
  local Parquet adapters. Same assertion vocabulary.
- **Phase 3 — data probe S3 + distribution assertions.**
  `percentile_between`, `cardinality_between`, `schema_match`. S3 Parquet
  adapter.
- **Phase 4 — load probe HTTP MVP.** Constant-rate scheduler, latency
  histograms, `p99_under` / `error_rate_below` / `throughput_above` /
  `status_code_in`. End-to-end example against an httpbin-style target.
- **Phase 5 — load probe VU mode + Postgres adapter.** VU pool with
  ramp, Postgres SQL load adapter, query parameterization.
- **Phase 6 — pytest plugin + `ematix-flow` integration shim.**
- **Phase 7 — opt-in run history persistence.** Postgres adapter for the
  `_ematix_probe_*` schema, `--history postgres://...` flag,
  `[run_history]` config block, write-on-run with no read path yet (v0.2
  adds baselines/regression on top).
- **Phase 8 — `ematix-probe explain` + `ematix-probe doctor` + docs polish.**
- **Phase 9 — v0.1 PyPI release.**

Each phase is a PR, gated on TDD: failing tests committed first, then
implementation. (Per project convention.)

## 14. Decisions

1. **Name** → `ematix-probe`. PyPI: `ematix-probe`. Python import:
   `ematix_probe`. CLI binary: `ematix-probe` (no shorter alias shipped;
   users who want one can alias `ep=ematix-probe`).
2. **Async Python API** → supported in v0.1. Both `@probe.data` and
   `@probe.load` accept `def` and `async def`. The Rust engine drives
   sync functions on a worker thread and async functions on its tokio
   runtime via `pyo3-asyncio`. (See §6 preamble.)
3. **`probe.flow_table` location** → lives in `ematix_probe.ematix_flow`.
   The generic `ematix_probe` core has zero `ematix-flow` dependency; the
   integration shim is optional and only imported when `ematix-flow` is
   installed. (See §6.2 / §8.2.)
4. **CLI binary name** → `ematix-probe` (full namespaced name, matches
   PyPI / import name, no `linux-tools probe` clash). (See §7.)
5. **Histogram serialization** → OpenTelemetry ExponentialHistogram (OTLP
   JSON encoding). Drops directly into any OTel-aware backend without a
   re-encoder. (See §5.4.)
6. **Run history persistence** → user-configurable opt-in for v0.1.
   Default: ephemeral (terminal + JUnit + JSON). With `--history
   postgres://...` or `[run_history]` config: durable run/probe/assertion
   tables in a `_ematix_probe_*` schema that mirrors `ematix-flow`'s
   metadata layout. v0.2 adds baseline/drift on top. (See §5.2, §10,
   Phase 7.)

## 15. Appendix — full end-to-end example

```python
# probes/customer_pipeline.py

from ematix_probe import probe, source
from ematix_flow import ManagedTable, Column, Integer, String, Timestamp

class CustomerDim(ManagedTable):
    __tablename__ = "dim_customers"
    __schema__ = "analytics"
    customer_id: Column[Integer, "primary_key"]
    email:       Column[String]
    country:     Column[String]
    updated_at:  Column[Timestamp, "watermark"]


@probe.flow_table(CustomerDim)
def customer_dim_quality(t):
    t.freshness("updated_at").within("6h")
    t.row_count().at_least(1_000)
    t.column("email").regex(r".+@.+\..+")
    t.column("country").enum({"US", "CA", "MX", "GB"})


@probe.load(target="https://api.internal/customer")
def customer_lookup_load(s):
    s.get("/v1/customers/{id}", ids=range(1, 1_000))
    s.rate("200/s", duration="60s")
    s.warmup("5s")
    s.expect.p99_under("150ms")
    s.expect.error_rate_below(0.001)
```

```bash
$ ematix-probe run probes/ --report junit:probe-report.xml --report json:probe.json
ematix-probe 0.1.0 — 2 probes discovered

  ✓ customer_dim_quality                 4 assertions, 312 ms
  ✓ customer_lookup_load                 4 assertions, 60.2 s
                                         p99 = 132 ms, errors = 0.0004

PASS · 2/2 probes · 8/8 assertions · 60.5 s
```

CI sees exit `0`, the JUnit reporter renders both probes as test cases,
and the JSON report is the input to whatever downstream tooling exists.

---

**Approved 2026-05-06.** Decorator surface (§6) and milestone ordering
(§13) confirmed alongside §14 decisions. Phase 0 (workspace skeleton +
green CI under TDD) is unblocked.
