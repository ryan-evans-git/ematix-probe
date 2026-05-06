# Sprint 2 — Phase 1a: Postgres data probe MVP

Dates: 2026-05-06 → 2026-05-13
PI: PI-1
Phase: Phase 1a
Status: **active**

## Goal

Land the smallest end-to-end data probe: a `@probe.data` decorator in
Python that runs three pushdown-SQL assertions (`not_null`, `unique`,
`between`) against a Postgres table and returns a `pass` / `fail`
verdict to the caller. No reports yet (Phase 1b adds JUnit + JSON);
no `regex` / `enum` / `row_count` / `freshness` (Phase 1b too).

## Stories

Each story RED → GREEN → REFACTOR per [PROCESS.md §5](../PROCESS.md).

- [ ] **S-2.1 — `ProbePlan` + `Verdict` types in core**
  - RED: `crates/ematix-probe-core/src/lib.rs` test
    `empty_plan_evaluates_pass` builds a `ProbePlan` with no
    assertions, runs it through a stub adapter, asserts the
    `Verdict` is `Pass`. Fails to compile (no types).
  - GREEN: define `Verdict { Pass, Fail, Error }`,
    `Assertion` enum, `ProbePlan { table, assertions: Vec<Assertion> }`,
    `DataAdapter` trait with one method (`execute(plan)
    -> Result<RunSummary>`).
  - REFACTOR: split into `engine::data` + `adapters::data` modules
    matching PRD §8.1.

- [ ] **S-2.2 — Postgres adapter (`tokio-postgres` + `deadpool-postgres`)**
  - RED: integration test in `crates/ematix-probe-core/tests/postgres_adapter.rs`
    that uses `testcontainers` to spin up Postgres, opens a pooled
    connection through the adapter, runs `SELECT 1`. Fails (no
    adapter exists).
  - GREEN: `adapters::data::postgres::PostgresAdapter` impls
    `DataAdapter`. Uses `deadpool-postgres` for pooling; opens with
    a connection string parsed via `tokio_postgres::Config::from_str`.
  - REFACTOR: factor URL parsing into `source::postgres(url)` in
    Phase 1b when the Python factory lands.

- [ ] **S-2.3 — `not_null` assertion (pushdown SQL)**
  - RED: integration test asserts that a table with one NULL row in
    column `email` produces `Verdict::Fail` with a meaningful
    `AssertionResult` (column = "email", failed_rows = 1, total = N).
    Fails (assertion not implemented).
  - GREEN: SQL builder emits
    `SELECT count(*) FROM "schema"."table" WHERE "col" IS NULL`;
    fail when count > 0.
  - REFACTOR: identifier-quoting helper (Postgres uses `"`).

- [ ] **S-2.4 — `unique` assertion (pushdown SQL)**
  - RED: integration test with two rows sharing the same value in a
    `customer_id` column → `Verdict::Fail`. Test against a clean
    table → `Verdict::Pass`.
  - GREEN: SQL:
    `SELECT count(*) FROM (SELECT "col", count(*) c FROM "schema"."table" GROUP BY "col" HAVING count(*) > 1) d`.
  - REFACTOR: assertion-result struct shared with S-2.3.

- [ ] **S-2.5 — `between` assertion (numeric inclusive range)**
  - RED: integration test with row outside `[0, 120]` in `age`
    column → `Verdict::Fail`. Test with all-in-range rows →
    `Verdict::Pass`.
  - GREEN: SQL:
    `SELECT count(*) FROM "schema"."table" WHERE "col" < $1 OR "col" > $2`,
    parameterized with low + high. Inclusive bounds for v0.1; exclusive
    variants in Phase 1b.
  - REFACTOR: extract a common `count_violations_sql` helper if
    natural; resist if it's a premature abstraction.

- [ ] **S-2.6 — Python `@probe.data` decorator + fluent builder**
  - RED: pytest in `tests/test_probe_data.py` builds:
    ```python
    @probe.data(source=source.postgres("DATABASE_URL"), table="users")
    def quality(t):
        t.column("email").not_null()
        t.column("user_id").unique()
        t.column("age").between(0, 120)
    ```
    and asserts that calling `quality.plan()` returns a `ProbePlan`
    with three assertions. Fails — no decorator exists.
  - GREEN: pyo3-bound `ProbePlan` / `Tester` / `ColumnRef` types in
    `crates/ematix-probe-py/src/lib.rs`; Python decorator + fluent
    builder in `python/ematix_probe/probe.py` and `source.py`.
  - REFACTOR: type stubs (`.pyi`) deferred to Phase 1b.

- [ ] **S-2.7 — End-to-end pytest using testcontainers**
  - RED: `tests/test_e2e_postgres.py` uses
    `testcontainers[postgres]` to spin Postgres, seeds a small
    table, runs the `@probe.data`-decorated probe, asserts the
    returned `RunReport.verdict == "pass"`. Fails — wiring not
    complete.
  - GREEN: glue Python decorator → ProbePlan → core engine →
    Postgres adapter → AssertionResult → RunReport → Python.
  - REFACTOR: split unit-only fast tests from
    Postgres-requiring integration tests via pytest markers.

- [ ] **S-2.8 — Update CHANGELOG, sprint, learnings**
  - Add Phase 1a entry to [CHANGELOG.md](../../CHANGELOG.md).
  - Update [LEARNINGS.md](../LEARNINGS.md) with anything surprising
    about pyo3 cross-language type design or pushdown-SQL edge cases.

## Definition of Done

- [ ] All Sprint 2 stories' tests green in CI
- [ ] `cargo test --workspace --all-targets` green (incl. integration tests)
- [ ] All Phase 0 gates still green (fmt, clippy, audit, ruff, bandit,
      pip-audit, pytest)
- [ ] CI workflow green on `phase-1a` branch
- [ ] PR `phase-1a` → `main` opened and merged
- [ ] CHANGELOG.md updated
- [ ] PRD §6.1 example still type-checks against the actual decorator surface
      (manual review — no automated check yet)
- [ ] Retro filled in below

## Out of scope (deferred to Phase 1b / later)

- `regex`, `enum`, `row_count`, `freshness` assertions
- JUnit XML / JSON report generation
- `@probe.flow_table` integration shim
- DuckDB / Parquet adapters
- Async probe functions (PRD §6 says supported in v0.1, but the
  pyo3-asyncio wiring lands in the pytest-plugin sprint)
- `--history postgres://...` run history persistence

## Risks

1. **`testcontainers` flakiness in CI** — Docker pull on a clean
   GitHub runner adds 30-60s. Mitigation: cache the pulled image,
   accept the cost on first run.
2. **`tokio-postgres` async runtime in PyO3** — calling async Rust
   from sync Python via pyo3 needs a runtime. Mitigation: spawn a
   tokio runtime on first use, share via `OnceCell`. Same pattern
   ematix-flow uses; copy from there.
3. **SQL identifier quoting edge cases** (mixed-case schemas, reserved
   words). Mitigation: always quote with `"`, escape embedded `"` by
   doubling. Test with a column named `"select"`.

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
