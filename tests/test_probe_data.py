"""S-2.6 — `@probe.data` decorator + fluent `Tester` builder.

Pure decoration tests (no Postgres). Verifies that the decorator
collects assertions through the fluent API and produces a Rust-side
`ProbePlan` of the right shape. End-to-end execution lands in S-2.7.
"""

import pytest

from ematix_probe import probe, source


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
