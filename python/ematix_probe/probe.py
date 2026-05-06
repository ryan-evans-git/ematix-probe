"""`@probe.data` decorator + fluent `Tester` builder.

Usage (per PRD §6.1):

    from ematix_probe import probe, source

    @probe.data(
        source=source.postgres("postgres://localhost/db"),
        table="dim_customers",
    )
    def customer_dim_quality(t):
        t.column("email").not_null()
        t.column("customer_id").unique()
        t.column("age").between(0, 120)

The decorator returns a `DataProbe` object. `quality.plan()` exposes
the compiled `ProbePlan`; running the probe end-to-end against a
Postgres adapter lands in S-2.7.
"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass

from ematix_probe._core import (
    ProbePlan,
    assertion_between,
    assertion_not_null,
    assertion_unique,
)

from .source import Source


@dataclass(frozen=True)
class _AssertionSpec:
    """Internal: a Python-side assertion record. Translated into a
    Rust `Assertion` when `DataProbe._build_plan` materializes the
    plan."""

    kind: str
    column: str
    low: float | None = None
    high: float | None = None


class _ColumnRef:
    """Builder returned by `Tester.column(name)`. Each fluent call
    appends one `_AssertionSpec` to the parent `Tester`. Returns
    `self` so chains like `.not_null().unique()` work."""

    __slots__ = ("_tester", "_name")

    def __init__(self, tester: Tester, name: str) -> None:
        self._tester = tester
        self._name = name

    def not_null(self) -> _ColumnRef:
        self._tester._add(_AssertionSpec(kind="not_null", column=self._name))
        return self

    def unique(self) -> _ColumnRef:
        self._tester._add(_AssertionSpec(kind="unique", column=self._name))
        return self

    def between(self, low: float, high: float) -> _ColumnRef:
        self._tester._add(
            _AssertionSpec(
                kind="between",
                column=self._name,
                low=float(low),
                high=float(high),
            )
        )
        return self


class Tester:
    """The `t` argument the decorated function receives. Yields
    `_ColumnRef` builders via `t.column(name)`. Currently column-
    only; table-level assertions (`row_count`, `freshness`) land in
    Phase 1b."""

    __slots__ = ("_specs",)

    def __init__(self) -> None:
        self._specs: list[_AssertionSpec] = []

    def column(self, name: str) -> _ColumnRef:
        return _ColumnRef(self, name)

    def _add(self, spec: _AssertionSpec) -> None:
        self._specs.append(spec)


class DataProbe:
    """The object `@probe.data` returns. Holds the plan + the
    source binding. Execution surface (`.run()`, `__call__`) lands
    in S-2.7."""

    def __init__(
        self,
        fn: Callable[[Tester], None],
        *,
        source: Source,
        table: str,
        schema: str | None,
    ) -> None:
        self._fn = fn
        self._source = source
        self._table = table
        self._schema = schema
        # Build eagerly so any malformed assertion / type error
        # surfaces at decoration time, not first-run time.
        self._plan = self._build_plan()

    # Preserve the original function's identity for diagnostics
    # (frame names in tracebacks, repr, etc.). functools.wraps
    # would clobber __dict__ with non-callable noise; this is
    # the minimal subset we want.
    @property
    def __name__(self) -> str:
        return self._fn.__name__

    def _build_plan(self) -> ProbePlan:
        tester = Tester()
        self._fn(tester)
        rust_assertions = [_to_rust(spec) for spec in tester._specs]
        return ProbePlan(self._schema, self._table, rust_assertions)

    def plan(self) -> ProbePlan:
        """The compiled Rust-side `ProbePlan`."""
        return self._plan

    @property
    def source(self) -> Source:
        return self._source

    @property
    def table(self) -> str:
        return self._table

    @property
    def schema(self) -> str | None:
        return self._schema


def _to_rust(spec: _AssertionSpec):
    """Convert a Python-side `_AssertionSpec` to a Rust `Assertion`
    via the pyo3 factory functions."""
    if spec.kind == "not_null":
        return assertion_not_null(spec.column)
    if spec.kind == "unique":
        return assertion_unique(spec.column)
    if spec.kind == "between":
        assert spec.low is not None and spec.high is not None
        return assertion_between(spec.column, spec.low, spec.high)
    # _AssertionSpec is internal — unknown kinds indicate a bug,
    # not a user error.
    raise AssertionError(f"unknown assertion kind: {spec.kind!r}")


def data(
    *,
    source: Source,
    table: str,
    schema: str | None = None,
) -> Callable[[Callable[[Tester], None]], DataProbe]:
    """`@probe.data(source=..., table=..., schema=None)` — declare
    a data probe. The decorated function receives a `Tester` and
    populates it with column-level assertions through the fluent
    builder.

    The decorator returns a `DataProbe` object (not the original
    function). Access the compiled plan via `.plan()`. Execution
    against the source adapter is wired up in S-2.7.
    """

    def decorator(fn: Callable[[Tester], None]) -> DataProbe:
        return DataProbe(fn, source=source, table=table, schema=schema)

    return decorator
