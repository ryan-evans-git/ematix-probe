# ematix-probe

**Declarative testing automation: data probes + load probes, on a Rust core.**

`ematix-probe` is a Python testing-automation library, with a Rust core,
that lets you assert on the *shape* of data sitting in a target (Postgres,
DuckDB, Parquet on S3) and on the *behavior* of a service under synthetic
traffic (HTTP, Postgres SQL) — using one declarative API and one CLI.

> Status: **Phase 0** — workspace skeleton. v0.1 not yet released.

## Documents

- [Product Requirements](docs/PRD.md) — locked v0.1 scope
- [Engineering process](docs/PROCESS.md) — TDD + sprint cadence + retros
- [PI plan](docs/PI_PLAN.md) — current PI, sprint map, risks
- [Sprints](docs/sprints/) — per-sprint plans + retros
- [Learnings](docs/LEARNINGS.md) — append-only log

Sibling project: [ematix-flow](https://github.com/ryan-evans-git/ematix-flow).

## License

Apache-2.0.
