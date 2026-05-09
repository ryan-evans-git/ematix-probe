# ematix-probe

**Declarative testing automation: data probes + load probes, on a Rust core.**

`ematix-probe` is a Python testing-automation library, with a Rust core,
that lets you assert on the *shape* of data sitting in a target (Postgres,
DuckDB, Parquet on S3) and on the *behavior* of a service under synthetic
traffic (HTTP, Postgres SQL) — using one declarative API and one CLI.

> Status: **Phase 5 closed** (Sprint 8, PI-1) — v0.1 not yet released.
>
> What works today: data probes against Postgres / DuckDB / local
> Parquet / S3 Parquet (`not_null`, `unique`, `between`, `regex`,
> `enum`, `row_count`, `freshness`, `percentile_between`,
> `cardinality_between`, `schema_match`); load probes against HTTP
> and Postgres SQL with constant-rate or virtual-user scheduling
> (`p99_under`, `error_rate_below`, `throughput_above`,
> `status_code_in`). Pytest plugin + PyPI release land in
> Sprints 9–10.

## Documents

- [Product Requirements](docs/PRD.md) — locked v0.1 scope
- [Engineering process](docs/PROCESS.md) — TDD + sprint cadence + retros
- [PI plan](docs/PI_PLAN.md) — current PI, sprint map, risks
- [Sprints](docs/sprints/) — per-sprint plans + retros
- [Learnings](docs/LEARNINGS.md) — append-only log

Sibling project: [ematix-flow](https://github.com/ryan-evans-git/ematix-flow).

## License

Apache-2.0.
