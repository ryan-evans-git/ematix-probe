"""S-2.6 — `@probe.data` decorator + fluent `Tester` builder.

Pure decoration tests (no Postgres). Verifies that the decorator
collects assertions through the fluent API and produces a Rust-side
`ProbePlan` of the right shape. End-to-end execution lands in S-2.7.
"""

import pytest
from ematix_probe import probe, source
from ematix_probe.probe import _assertion_name, _AssertionSpec, _to_rust
from ematix_probe.source import Source


def test_decorator_builds_plan_with_three_assertions():
    @probe.data(
        source=source.postgres("postgres://user:pass@localhost:5432/db"),
        table="users",
    )
    def quality(t):
        t.column("email").not_null()
        t.column("user_id").unique()
        t.column("age").between(0, 120)

    plan = quality.plan()
    assert plan.table == "users"
    assert plan.schema is None
    assert len(plan) == 3
    assert plan.assertion_count() == 3


def test_decorator_carries_table_and_schema():
    @probe.data(
        source=source.postgres("postgres://localhost/db"),
        table="dim_customers",
        schema="analytics",
    )
    def empty(t):
        pass

    plan = empty.plan()
    assert plan.table == "dim_customers"
    assert plan.schema == "analytics"
    assert len(plan) == 0


def test_chained_calls_on_same_column():
    @probe.data(source=source.postgres("postgres://localhost/db"), table="users")
    def quality(t):
        t.column("email").not_null().unique()

    plan = quality.plan()
    assert len(plan) == 2


def test_source_postgres_rejects_non_url_string():
    with pytest.raises(ValueError, match="connection URL"):
        source.postgres("DATABASE_URL")


def test_source_postgres_accepts_postgresql_scheme():
    s = source.postgres("postgresql://localhost/db")
    assert s.kind == "postgres"
    assert s.url.startswith("postgresql://")


def test_decorator_preserves_callable_function():
    """Until S-2.7 wires execution, the decorator exposes the
    original function so users can introspect it (e.g. for
    debugging). The probe object itself is NOT yet directly
    callable — that's S-2.7."""

    @probe.data(source=source.postgres("postgres://localhost/db"), table="t")
    def my_probe(t):
        t.column("x").not_null()

    # The plan-building phase has run already; the decorator returns
    # an object, not the raw function. We just need the original
    # name reachable for diagnostics.
    assert my_probe.__name__ == "my_probe"


def test_regex_assertion_translates_through_to_plan():
    @probe.data(source=source.postgres("postgres://localhost/db"), table="users")
    def quality(t):
        t.column("email").regex(r"^[^@]+@[^@]+$")

    plan = quality.plan()
    assert len(plan) == 1


def test_is_in_assertion_translates_through_to_plan():
    @probe.data(source=source.postgres("postgres://localhost/db"), table="orders")
    def quality(t):
        t.column("status").is_in(["new", "shipped", "cancelled"])

    plan = quality.plan()
    assert len(plan) == 1


def test_row_count_with_at_least_only():
    @probe.data(source=source.postgres("postgres://localhost/db"), table="t")
    def quality(t):
        t.row_count(at_least=10)

    assert len(quality.plan()) == 1


def test_row_count_with_at_most_only():
    @probe.data(source=source.postgres("postgres://localhost/db"), table="t")
    def quality(t):
        t.row_count(at_most=1000)

    assert len(quality.plan()) == 1


def test_row_count_rejects_no_bounds():
    with pytest.raises(ValueError, match="at_least"):

        @probe.data(source=source.postgres("postgres://localhost/db"), table="t")
        def quality(t):
            t.row_count()


def test_freshness_assertion_translates_through_to_plan():
    @probe.data(source=source.postgres("postgres://localhost/db"), table="events")
    def quality(t):
        t.freshness("ts", within="24h")

    plan = quality.plan()
    assert len(plan) == 1


def test_data_probe_exposes_source_table_and_schema():
    src = source.postgres("postgres://localhost/db")

    @probe.data(source=src, table="dim_customers", schema="analytics")
    def quality(t):
        pass

    assert quality.source is src
    assert quality.table == "dim_customers"
    assert quality.schema == "analytics"


def test_assertion_name_formats_each_kind_correctly():
    """`_assertion_name` is the label that lands in the run report.
    Column-level checks render as "<col>.<kind>"; freshness gets a
    function-call form; unknown / table-level kinds fall through to
    the bare kind. Tested directly because the freshness branch is
    only reachable when `.run()` translates a freshness spec back
    into its label."""
    assert (
        _assertion_name(_AssertionSpec(kind="not_null", column="email"))
        == "email.not_null"
    )
    assert (
        _assertion_name(_AssertionSpec(kind="freshness", column="ts"))
        == "freshness(ts)"
    )
    assert _assertion_name(_AssertionSpec(kind="row_count")) == "row_count"


def test_to_rust_raises_on_unknown_kind():
    """`_to_rust` fans out a Python-side spec to the matching pyo3
    factory. An unknown kind would only appear if `_AssertionSpec`
    grew a new variant without `_to_rust` catching up — guarded by
    a defensive AssertionError so the bug surfaces at decoration
    time rather than producing an inscrutable Rust panic later."""
    bogus = _AssertionSpec(kind="totally_made_up", column="x")
    with pytest.raises(AssertionError, match="unknown assertion kind"):
        _to_rust(bogus)


def test_data_probe_run_rejects_unknown_source_kinds():
    """`.run()` must raise NotImplementedError rather than silently
    no-op when pointed at a source kind no adapter can handle.
    Phase 2 added duckdb + parquet alongside postgres; future
    kinds (e.g. s3 Parquet in Phase 3) must still error explicitly
    until they are wired up."""
    unknown = Source(kind="s3-parquet", url="s3://bucket/x.parquet")

    @probe.data(source=unknown, table="t")
    def quality(t):
        t.column("x").not_null()

    with pytest.raises(NotImplementedError, match="s3-parquet"):
        quality.run()
