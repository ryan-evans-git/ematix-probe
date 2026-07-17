"""ematix-flow integration shim.

Lets a caller hand an ematix-flow-style declarative table class to
ematix-probe and get a `DataProbe` with sensible default
assertions, then layer extra checks on top — implementing the
`@probe.flow_table` surface from PRD §6.2.

Per PRD: "the generic core has zero ematix-flow dependency." This
module duck-types on a small protocol rather than importing
anything from ematix-flow. Any class that exposes:

- `__tablename__: str` — required.
- `__schema__: str | None` — optional; defaults to `None`.
- `columns`: iterable of objects with `.name: str`,
  `.nullable: bool`, `.primary_key: bool`.

…can participate. ematix-flow's `ManagedTable` matches this shape
out of the box; SQLAlchemy-style declarative models match with a
trivial adapter.
"""

from __future__ import annotations

from collections.abc import Callable
from typing import Protocol, runtime_checkable

from .probe import DataProbe, Tester
from .source import Source


@runtime_checkable
class _ColumnLike(Protocol):
    name: str
    nullable: bool
    primary_key: bool


@runtime_checkable
class _TableLike(Protocol):
    __tablename__: str
    columns: tuple[_ColumnLike, ...]


def probe_from_table(
    table_cls: _TableLike,
    *,
    source: Source,
    extend: Callable[[Tester], None] | None = None,
) -> DataProbe:
    """Build a `DataProbe` from an ematix-flow-style declarative
    table class.

    Auto-derived assertions:
    - `not_null` on every column where `nullable is False`.
    - a single `unique` covering the primary key: one column when the
      PK is single-column, or one *composite* `unique` over the whole
      tuple when the PK spans several columns. (Emitting a per-column
      `unique` for each PK column — the previous behaviour — hard-fails
      a valid composite key, since its columns are only unique
      *jointly*, e.g. `order_id` repeats across order lines.)
    - a composite `unique` for every group in `__unique_constraints__`
      (declared natural keys), if the table exposes it.

    `extend` lets callers layer additional assertions on top using
    the same fluent `Tester` API the auto-derived block populates.
    Order: auto-derived first, then `extend(t)`.
    """
    table_name = table_cls.__tablename__
    schema_name = getattr(table_cls, "__schema__", None)
    columns = tuple(table_cls.columns)

    # Collect the key groups. All primary-key columns form ONE key (a
    # multi-column PK is jointly unique, not per-column); each declared
    # __unique_constraints__ group is another. De-dup while preserving
    # order so the same group isn't asserted twice.
    groups: list[tuple[str, ...]] = []
    pk = tuple(col.name for col in columns if col.primary_key)
    if pk:
        groups.append(pk)
    for uc in getattr(table_cls, "__unique_constraints__", ()) or ():
        g = tuple(uc)
        if g and g not in groups:
            groups.append(g)

    def _build(t: Tester) -> None:
        for col in columns:
            if not col.nullable:
                t.column(col.name).not_null()
        for group in groups:
            if len(group) == 1:
                t.column(group[0]).unique()
            else:
                t.unique(list(group))
        if extend is not None:
            extend(t)

    # Mirror the original table-class identity so reports / pytest
    # nodes name the probe sensibly. Using __qualname__ would be
    # nicer for nested classes but __name__ matches what
    # `@probe.data` would have used.
    _build.__name__ = table_cls.__name__
    return DataProbe(_build, source=source, table=table_name, schema=schema_name)
