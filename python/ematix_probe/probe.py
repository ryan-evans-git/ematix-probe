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
from datetime import datetime, timezone

from ematix_probe._core import (
    ProbePlan,
    assertion_between,
    assertion_enum,
    assertion_freshness,
    assertion_not_null,
    assertion_regex,
    assertion_row_count,
    assertion_unique,
    run_duckdb_probe,
    run_parquet_probe,
    run_postgres_probe,
    run_s3_parquet_probe,
)

from .duration import parse_duration
from .report import AssertionResult, RunReport
from .source import Source


@dataclass(frozen=True)
class _AssertionSpec:
    """Internal: a Python-side assertion record. Translated into a
    Rust `Assertion` when `DataProbe._build_plan` materializes the
    plan."""

    kind: str
    # column is empty for table-level checks like row_count.
    column: str = ""
    low: float | None = None
    high: float | None = None
    within_seconds: int | None = None
    pattern: str | None = None
    allowed: tuple[str, ...] | None = None
    row_low: int | None = None
    row_high: int | None = None


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

    def regex(self, pattern: str) -> _ColumnRef:
        """Assert every non-NULL value matches the Postgres POSIX
        regex `pattern`. NULLs are not counted as violations — pair
        with `.not_null()` to forbid them."""
        self._tester._add(
            _AssertionSpec(kind="regex", column=self._name, pattern=pattern)
        )
        return self

    def is_in(self, allowed: list[str]) -> _ColumnRef:
        """Assert every value is one of `allowed`. Named `is_in` to
        avoid shadowing the Python keyword `in`."""
        self._tester._add(
            _AssertionSpec(
                kind="enum",
                column=self._name,
                allowed=tuple(allowed),
            )
        )
        return self


class Tester:
    """The `t` argument the decorated function receives. Yields
    `_ColumnRef` builders via `t.column(name)`. Table-level checks
    (`row_count`, `freshness`) live directly on the `Tester`."""

    __slots__ = ("_specs",)

    def __init__(self) -> None:
        self._specs: list[_AssertionSpec] = []

    def column(self, name: str) -> _ColumnRef:
        return _ColumnRef(self, name)

    def row_count(
        self,
        *,
        at_least: int | None = None,
        at_most: int | None = None,
    ) -> Tester:
        """Assert the table's row count is within bounds. At least
        one of `at_least` / `at_most` must be supplied — passing
        neither asserts nothing and is rejected as a programming
        error."""
        if at_least is None and at_most is None:
            raise ValueError(
                "row_count() requires at least one of at_least= or at_most="
            )
        self._add(
            _AssertionSpec(
                kind="row_count",
                row_low=at_least,
                row_high=at_most,
            )
        )
        return self

    def freshness(self, column: str, *, within: str) -> Tester:
        """Assert that the most recent value of `column` is no older
        than `within`. `within` is a duration string (`"24h"`,
        `"30m"`, …) parsed by `parse_duration`."""
        self._add(
            _AssertionSpec(
                kind="freshness",
                column=column,
                within_seconds=parse_duration(within),
            )
        )
        return self

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
        # Stash the specs so .run() can name each assertion in the
        # RunReport (the pyo3 result only carries indices).
        self._specs: list[_AssertionSpec] = list(tester._specs)
        rust_assertions = [_to_rust(spec) for spec in self._specs]
        return ProbePlan(self._schema, self._table, rust_assertions)

    def plan(self) -> ProbePlan:
        """The compiled Rust-side `ProbePlan`."""
        return self._plan

    def run(self) -> RunReport:
        """Execute the probe against the configured source. Sync;
        async support lands with the pytest plugin in Sprint 9.
        Only Postgres sources are wired in v0.1 Phase 1a."""
        started_at = datetime.now(tz=timezone.utc)
        kind = self._source.kind
        if kind == "postgres":
            raw = run_postgres_probe(self._source.url, self._plan)
        elif kind == "duckdb":
            raw = run_duckdb_probe(self._source.url, self._plan)
        elif kind == "parquet":
            raw = run_parquet_probe(self._source.url, self._plan)
        elif kind == "s3_parquet":
            raw = run_s3_parquet_probe(
                self._source.s3_bucket,
                self._source.s3_key,
                self._source.s3_region,
                self._source.s3_endpoint,
                self._plan,
            )
        else:
            # S3 Parquet + future kinds land in later phases; until
            # then, error explicitly rather than silently no-op.
            raise NotImplementedError(
                f"source kind {kind!r} is not yet supported by DataProbe.run()"
            )
        finished_at = datetime.now(tz=timezone.utc)

        assertions = [
            AssertionResult(
                assertion_index=r.assertion_index,
                verdict=r.verdict,
                message=r.message,
                name=_assertion_name(self._specs[r.assertion_index]),
            )
            for r in raw.assertions
        ]
        return RunReport(
            probe_name=self.__name__,
            table=self._table,
            schema=self._schema,
            verdict=raw.verdict,
            assertions=assertions,
            started_at=started_at,
            finished_at=finished_at,
        )

    def assertion_names(self) -> list[str]:
        """Human labels for every assertion in the probe, in plan
        order. Used by the pytest plugin (S-9.2) to fan a probe out
        into one pytest item per assertion. Names match what
        ``RunReport.assertions[i].name`` will carry, so tooling can
        join the two by index."""
        return [_assertion_name(spec) for spec in self._specs]

    @property
    def source(self) -> Source:
        return self._source

    @property
    def table(self) -> str:
        return self._table

    @property
    def schema(self) -> str | None:
        return self._schema


def _assertion_name(spec: _AssertionSpec) -> str:
    """Human label for the report. Column-level checks read as
    ``"<column>.<kind>"``; table-level reads as ``"<kind>(<column>)"``
    or just ``"<kind>"`` when there's no associated column."""
    if spec.kind in ("not_null", "unique", "between", "regex", "enum"):
        return f"{spec.column}.{spec.kind}"
    if spec.kind == "freshness":
        return f"freshness({spec.column})"
    return spec.kind


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
    if spec.kind == "regex":
        assert spec.pattern is not None
        return assertion_regex(spec.column, spec.pattern)
    if spec.kind == "enum":
        assert spec.allowed is not None
        return assertion_enum(spec.column, list(spec.allowed))
    if spec.kind == "row_count":
        return assertion_row_count(spec.row_low, spec.row_high)
    if spec.kind == "freshness":
        assert spec.within_seconds is not None
        return assertion_freshness(spec.column, spec.within_seconds)
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
