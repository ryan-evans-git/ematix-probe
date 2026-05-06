"""S-3.4 — Python-side freshness wiring.

Two layers under test:

1. The `parse_duration` helper that converts strings like ``"24h"``
   into an integer count of seconds.
2. The `Tester.freshness(column, within=...)` table-level builder
   that lands a ``freshness`` assertion into the compiled
   ``ProbePlan``.

Live Postgres execution of the freshness assertion is covered by
``crates/ematix-probe-core/tests/postgres_assertions.rs``; this
module stays unit-scoped (no DB).
"""

from __future__ import annotations

import pytest
from ematix_probe import probe, source
from ematix_probe.duration import parse_duration


class TestParseDuration:
    @pytest.mark.parametrize(
        "spec,expected_seconds",
        [
            ("30s", 30),
            ("30m", 30 * 60),
            ("24h", 24 * 60 * 60),
            ("7d", 7 * 24 * 60 * 60),
            ("0s", 0),
            ("  6h  ", 6 * 60 * 60),
        ],
    )
    def test_accepts_int_unit_form(self, spec: str, expected_seconds: int) -> None:
        assert parse_duration(spec) == expected_seconds

    @pytest.mark.parametrize(
        "spec",
        [
            "24",          # missing unit
            "h",           # missing number
            "24hr",        # multi-char unit
            "6h30m",       # compound (deferred to Phase 1c)
            "-1h",         # negative
            "1.5h",        # fractional
            "",
            " ",
        ],
    )
    def test_rejects_invalid_input(self, spec: str) -> None:
        with pytest.raises(ValueError, match="duration"):
            parse_duration(spec)

    def test_rejects_non_string(self) -> None:
        with pytest.raises(ValueError, match="must be a string"):
            parse_duration(86400)  # type: ignore[arg-type]


class TestFreshnessBuilder:
    def test_freshness_lands_assertion_in_plan(self) -> None:
        @probe.data(
            source=source.postgres("postgres://localhost/db"),
            table="events",
        )
        def quality(t):
            t.freshness("updated_at", within="24h")

        plan = quality.plan()
        assert len(plan) == 1

    def test_freshness_composes_with_column_assertions(self) -> None:
        @probe.data(
            source=source.postgres("postgres://localhost/db"),
            table="events",
        )
        def quality(t):
            t.column("event_id").not_null().unique()
            t.freshness("updated_at", within="6h")

        plan = quality.plan()
        assert len(plan) == 3

    def test_freshness_propagates_duration_parser_errors(self) -> None:
        with pytest.raises(ValueError, match="duration"):

            @probe.data(
                source=source.postgres("postgres://localhost/db"),
                table="events",
            )
            def _bad(t):
                t.freshness("updated_at", within="24hr")
