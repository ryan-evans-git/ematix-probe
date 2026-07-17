"""S-9.3 — `ematix-flow` integration shim.

`probe_from_table(table_cls, source=...)` introspects an
ematix-flow-style declarative table class and produces a
`DataProbe` with sensible default assertions:
- `not_null` on every non-nullable column.
- `unique` on each primary-key column.

Per PRD §6.2: "the generic core has zero ematix-flow dependency."
The shim duck-types on a small protocol (`__tablename__`,
`__schema__`, iterable `columns`) so any caller-supplied class can
participate, including ematix-flow's `ManagedTable`.
"""

from __future__ import annotations

from dataclasses import dataclass

from ematix_probe import source
from ematix_probe.flow import probe_from_table
from ematix_probe.probe import DataProbe


@dataclass(frozen=True)
class _Col:
    """Test fixture mimicking the duck-typed column protocol the
    shim consumes: a `.name`, a `.nullable` flag, and a
    `.primary_key` flag. ematix-flow's real Column type just needs
    to expose the same three attribute names."""

    name: str
    nullable: bool = True
    primary_key: bool = False


class _FlowTable:
    __tablename__ = "users"
    __schema__ = "public"
    columns = (
        _Col("id", nullable=False, primary_key=True),
        _Col("email", nullable=False),
        _Col("name", nullable=True),
    )


def test_returns_a_data_probe():
    src = source.postgres("postgres://localhost/x")
    p = probe_from_table(_FlowTable, source=src)
    assert isinstance(p, DataProbe)
    assert p.table == "users"
    assert p.schema == "public"


def test_auto_generates_not_null_on_non_nullable_columns():
    src = source.postgres("postgres://localhost/x")
    p = probe_from_table(_FlowTable, source=src)
    names = p.assertion_names()
    assert "id.not_null" in names, names
    assert "email.not_null" in names, names
    # Nullable column gets no not_null.
    assert "name.not_null" not in names, names


def test_auto_generates_unique_on_primary_key():
    src = source.postgres("postgres://localhost/x")
    p = probe_from_table(_FlowTable, source=src)
    names = p.assertion_names()
    assert "id.unique" in names, names
    # Non-PK columns shouldn't get unique even if they're not-null.
    assert "email.unique" not in names, names


def test_composite_primary_key_emits_one_joint_unique():
    # A multi-column PK is jointly unique — the shim must emit ONE
    # composite unique_group, NOT a per-column unique for each PK
    # column (which would hard-fail valid data).
    class _OrderLine:
        __tablename__ = "order_lines"
        __schema__ = "public"
        columns = (
            _Col("order_id", nullable=False, primary_key=True),
            _Col("line_no", nullable=False, primary_key=True),
            _Col("qty", nullable=False),
        )

    p = probe_from_table(_OrderLine, source=source.postgres("postgres://localhost/x"))
    names = p.assertion_names()
    assert "unique_group(order_id, line_no)" in names, names
    # No per-column uniques on the individual key columns.
    assert "order_id.unique" not in names, names
    assert "line_no.unique" not in names, names


def test_unique_constraints_emit_composite_unique():
    # Declared composite natural keys (__unique_constraints__) are
    # checked too — previously they were silently never asserted.
    class _User:
        __tablename__ = "users"
        __schema__ = "public"
        __unique_constraints__ = (("tenant_id", "email"),)
        columns = (
            _Col("id", nullable=False, primary_key=True),
            _Col("tenant_id", nullable=False),
            _Col("email", nullable=False),
        )

    p = probe_from_table(_User, source=source.postgres("postgres://localhost/x"))
    names = p.assertion_names()
    assert "id.unique" in names, names  # single-col PK unchanged
    assert "unique_group(tenant_id, email)" in names, names


def test_schemaless_table_passes_schema_none():
    class _NoSchema:
        __tablename__ = "events"
        columns = (_Col("id", nullable=False, primary_key=True),)

    p = probe_from_table(_NoSchema, source=source.postgres("postgres://localhost/x"))
    assert p.schema is None
    assert p.table == "events"


def test_extra_assertions_can_be_layered_via_callable_extension():
    # Per PRD §6.2: callers should still be able to layer extra
    # assertions on top of the auto-derived ones. The shim
    # accepts an optional `extend` callable that gets the same
    # `Tester` the auto-derived block populated.
    src = source.postgres("postgres://localhost/x")
    p = probe_from_table(
        _FlowTable,
        source=src,
        extend=lambda t: t.column("email").regex(r".+@.+\..+"),
    )
    names = p.assertion_names()
    assert "id.not_null" in names
    assert "email.regex" in names
