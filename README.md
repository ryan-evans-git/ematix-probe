# ematix-probe

**A declarative Python framework for asserting on the shape of
your data and the behavior of your services. Rust + tokio under
the hood.**

> Status: **Phase 7 closed** (Sprint 9, PI-1) — v0.1 PyPI release
> lands in Sprint 10. All four surfaces below — data probes, load
> probes, pytest plugin, ematix-flow integration — are shipped.

ematix-probe lets you declare a target (a database table, a
parquet file, an HTTP endpoint, a SQL query) and the assertions
it must satisfy in Python; the framework runs the checks and
returns a structured verdict. Probes carry their own decorators
and fire from `ematix-probe run`, your pytest suite, or directly
from Python. The same primitives power data-quality checks
(Postgres, DuckDB, Parquet — local or S3), load tests (HTTP and
SQL with constant-rate or virtual-user schedulers), and an
opt-in run-history sqlite log so trends are queryable across
runs.

The rest of this README walks through how to use it, in the
order you'd reach for each feature.

---

## Table of contents

1. [Install](#install)
2. [Sources](#sources)
3. [Data probes](#data-probes)
4. [Assertions](#assertions)
5. [Load probes](#load-probes)
6. [pytest plugin](#pytest-plugin)
7. [ematix-flow integration](#ematix-flow-integration)
8. [Run history](#run-history)
9. [CLI](#cli)
10. [Python API](#python-api)
11. [What's shipped](#whats-shipped)
12. [Development](#development)
13. [License](#license)

---

## Install

```sh
pip install ematix-probe
```

The core install ships every adapter, the `ematix-probe` CLI
binary, and the pytest plugin (auto-loaded via the `pytest11`
entry point — no `pytest_plugins` wiring required).

### Optional extras

| Extra | What it adds | Install |
|---|---|---|
| `dev` | Test runner + linters + maturin + testcontainers (Postgres / LocalStack) for the local development workflow. | `pip install "ematix-probe[dev]"` |

The runtime surface (CLI, pytest plugin, every data + load
adapter) needs no extras. To build from source, see
[Development](#development) at the bottom.

---

## Sources

Sources are the first thing to set up. Every data probe
references a source by call-site; ematix-probe doesn't ship a
connection registry the way ematix-flow does — credentials live
in the URL or environment variables you pass in.

```python
from ematix_probe import source

postgres   = source.postgres("postgres://user:pass@host/db")
duckdb     = source.duckdb(":memory:")
parquet    = source.parquet("/path/to/file.parquet")
s3_parquet = source.s3_parquet(
    bucket="analytics",
    key="dim/customers.parquet",
    region="us-east-1",
    # endpoint_url= is optional — set it for LocalStack / MinIO.
)
```

Sources are inert factories — no connection is opened until the
probe runs.

---

## Data probes

A data probe declares a target table + the assertions it must
satisfy. The decorator returns a `DataProbe` object you can run
directly, collect via pytest, or list / explain through the CLI.

```python
from ematix_probe import probe, source

@probe.data(
    source=source.postgres("postgres://localhost/warehouse"),
    table="dim_customers",
    schema="public",
)
def customer_dim_quality(t):
    t.column("customer_id").not_null().unique()
    t.column("email").not_null().regex(r".+@.+\..+")
    t.column("status").is_in(["active", "churned", "trial"])
    t.column("age").between(0, 120)
    t.row_count(at_least=1_000, at_most=10_000_000)
    t.freshness("updated_at", within="24h")
```

Run it directly:

```python
report = customer_dim_quality.run()
print(report.verdict)              # "pass" | "fail" | "error"
for a in report.assertions:
    print(a.name, a.verdict, a.message)
```

Or write it to JUnit / JSON for CI:

```python
from ematix_probe.report import write_junit, write_json

write_junit([report], "build/probe-results.xml")
write_json([report], "build/probe-results.json")
```

The same probe can be picked up by pytest with no extra wiring —
see [pytest plugin](#pytest-plugin).

---

## Assertions

The assertion vocabulary is the same across every adapter; the
adapter chooses pushdown SQL vs. an Arrow scan internally.

| Assertion | Meaning |
|---|---|
| `t.column(c).not_null()` | Every value in `c` is non-NULL. |
| `t.column(c).unique()` | Every value in `c` is unique (NULLs allowed). |
| `t.column(c).between(low, high)` | Every value in `c` lies in `[low, high]` inclusive. |
| `t.column(c).regex(pattern)` | Every non-NULL value matches `pattern` (Postgres POSIX flavor on the SQL path; `regex` crate on the scan path). |
| `t.column(c).is_in([...])` | Every value is in the allowed set. |
| `t.row_count(at_least=, at_most=)` | Table row count falls in `[at_least, at_most]` (open ends supported). |
| `t.freshness(c, within="24h")` | The most recent value of `c` is no older than `within` (h / m / s / d). |
| `t.percentile_between(c, p=99, low=, high=)` | The pᵗʰ percentile of `c` lies in `[low, high]`. Scan-path only. |
| `t.cardinality_between(c, low=, high=)` | The count of distinct values in `c` lies in `[low, high]`. Scan-path only. |
| `t.schema_match({col: type, ...})` | The target's column types match the declared mapping. Scan-path only. |

Each assertion produces one `AssertionResult` with `verdict` ∈
`{"pass", "fail", "error"}` and an actionable message on
non-pass.

---

## Load probes

Load probes drive a target with synthetic traffic and assert on
the resulting samples. v0.1 ships HTTP and Postgres SQL targets
under either constant-rate (open-model) or virtual-user
(closed-model) schedulers. The Python surface is Rust-only in
v0.1 — Python decorators land in v0.2.

Drive the engine directly today:

```python
# Pseudocode mirroring the Rust API; full Python load surface ships in v0.2.
from ematix_probe import load
plan = load.http_plan(
    target=load.HttpTarget.get("https://api.example.com/health"),
    duration="60s",
    mode=load.ConstantRate(rps=100),
    warmup="10s",
    assertions=[
        load.p99_under("latency_ms", 200),
        load.error_rate_below(0.005),
        load.throughput_above(95),
        load.status_code_in([200, 304]),
    ],
)
```

Or use the Rust API directly via `cargo run --example
load_probe_demo` / `--example postgres_load_demo`.

---

## pytest plugin

`pip install ematix-probe` registers a `pytest11` plugin; pytest
auto-loads it. Any `@probe.data` instance at module top-level
becomes one pytest test node *per assertion*:

```python
# tests/test_warehouse_quality.py
from ematix_probe import probe, source

@probe.data(
    source=source.postgres("postgres://localhost/warehouse"),
    table="dim_customers",
)
def customer_dim_quality(t):
    t.column("customer_id").not_null()
    t.column("email").regex(r".+@.+\..+")
```

`pytest -v` reports:

```
tests/test_warehouse_quality.py::customer_dim_quality::customer_id.not_null PASSED
tests/test_warehouse_quality.py::customer_dim_quality::email.regex          FAILED
```

The probe runs once per pytest collection — assertion fan-out
caches the `RunReport` so N assertions don't multiply the
underlying database / HTTP work.

---

## ematix-flow integration

Sibling project [ematix-flow](https://github.com/ryan-evans-git/ematix-flow)
ships declarative table classes; ematix-probe consumes them
through a duck-typed shim:

```python
from ematix_probe.flow import probe_from_table
from ematix_probe import source

# CustomerDim is any class exposing __tablename__, optional
# __schema__, and an iterable `columns` with .name / .nullable /
# .primary_key — ematix-flow's ManagedTable matches out of the box.
quality = probe_from_table(
    CustomerDim,
    source=source.postgres("postgres://warehouse/db"),
    extend=lambda t: t.column("email").regex(r".+@.+\..+"),
)
```

Auto-derived: `not_null` on every non-nullable column + `unique`
on each primary key. `extend` lets you layer extras via the same
fluent API. ematix-probe has zero hard dependency on
ematix-flow — the protocol-typing means any conforming class
participates.

---

## Run history

Opt-in sqlite persistence. Pass `--run-history-db <path>` to
the CLI, or use the API directly:

```python
from ematix_probe.run_history import RunHistory

h = RunHistory("history.sqlite")
h.record(probe.run())
```

Schema is `runs` (one row per probe execution) + `assertions`
(one row per assertion result, joined by `run_id`), tagged with
`PRAGMA user_version = 1`. Designed as the substrate for v0.2
drift detection — additive columns only, no renames.

---

## CLI

```
ematix-probe run <path>           # discover + run probes; non-zero on fail
ematix-probe run <path> --run-history-db history.sqlite

ematix-probe list <path>          # enumerate probes, no execution
ematix-probe explain <path> <probe>   # print compiled plan for one probe
ematix-probe doctor               # environment health check
```

`<path>` points at any Python file containing `@probe.*`
decorators. The CLI imports the file, finds module-level
`DataProbe` attributes, runs each, and exits non-zero if any
verdict isn't `pass`.

---

## Python API

The package exposes:

- `probe.data(source=..., table=..., schema=None)` — data-probe decorator.
- `source.postgres / duckdb / parquet / s3_parquet` — source factories.
- `DataProbe.run()` — execute a probe, return a `RunReport`.
- `report.write_junit(reports, path)` / `report.write_json(reports, path)` — CI reports.
- `flow.probe_from_table(cls, source=, extend=)` — ematix-flow shim.
- `run_history.RunHistory(path)` — opt-in sqlite persistence.
- `pytest_plugin` — auto-loaded by pytest; not imported directly.

The Rust load-probe surface (`engine::load`,
`adapters::load::http`, `adapters::load::postgres`) is exposed
through the workspace's example crates today; the Python load
surface lands in v0.2.

---

## What's shipped

**Data probes:** Postgres, DuckDB, local Parquet, S3 Parquet.
Assertions: `not_null`, `unique`, `between`, `regex`, `enum`,
`row_count`, `freshness`, `percentile_between`,
`cardinality_between`, `schema_match`.

**Load probes (Rust API):** HTTP + Postgres SQL targets;
constant-rate (open-model) and virtual-user (closed-model)
schedulers. Assertions: `p99_under`, `error_rate_below`,
`throughput_above`, `status_code_in`. Sample-window warmup
filtering. Per-tick `Sample`s shared across HTTP and SQL paths
through one `evaluate_load` entry point.

**Reporting:** JUnit XML + JSON writers; pytest plugin with
per-assertion test nodes; opt-in sqlite run history.

**Out of v0.1 (planned for v0.2):** async PyO3 (`async def`
probe functions + `pyo3-asyncio` integration), drift detection,
distributed load generation, backends beyond the v0.1 set.

---

## Development

```sh
# Build the Rust workspace (core + CLI + Python extension crate)
cargo build --release

# Build + install the Python extension into a venv
python -m venv .venv && source .venv/bin/activate
pip install maturin
maturin develop --release

# Run tests
cargo test --workspace                    # default + integration (Docker)
pytest                                    # full Python suite
coverage run -m pytest && coverage report --fail-under=90
```

Process docs:

- [Product Requirements](docs/PRD.md) — locked v0.1 scope
- [Engineering process](docs/PROCESS.md) — TDD + sprint cadence + retros
- [PI plan](docs/PI_PLAN.md) — current PI, sprint map, risks
- [Sprints](docs/sprints/) — per-sprint plans + retros
- [Learnings](docs/LEARNINGS.md) — append-only log

Sibling project: [ematix-flow](https://github.com/ryan-evans-git/ematix-flow).

---

## License

Apache-2.0.
