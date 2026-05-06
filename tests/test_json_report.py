"""S-3.6 — JSON report writer.

The Python ``RunReport.write_json(path)`` produces a JSON file
parseable with ``json.load`` into a stable schema:

    {
      "probe_name": str,
      "table": str,
      "schema": str | null,
      "verdict": "pass" | "fail" | "error",
      "started_at": ISO-8601 str,
      "finished_at": ISO-8601 str,
      "duration_seconds": float,
      "assertions": [
        {
          "assertion_index": int,
          "name": str,
          "verdict": "pass" | "fail" | "error",
          "message": str | null
        },
        ...
      ]
    }
"""

from __future__ import annotations

import json
from datetime import datetime, timezone
from pathlib import Path

from ematix_probe.report import AssertionResult, RunReport


def _make_report(
    *,
    probe_name: str = "customer_quality",
    table: str = "users",
    schema: str | None = None,
    verdict: str = "fail",
    assertions: list[AssertionResult] | None = None,
) -> RunReport:
    started = datetime(2026, 5, 6, 12, 0, 0, tzinfo=timezone.utc)
    finished = datetime(2026, 5, 6, 12, 0, 1, 250000, tzinfo=timezone.utc)
    return RunReport(
        probe_name=probe_name,
        table=table,
        schema=schema,
        verdict=verdict,
        assertions=assertions
        or [
            AssertionResult(
                assertion_index=0,
                name="email.not_null",
                verdict="pass",
                message=None,
            ),
            AssertionResult(
                assertion_index=1,
                name="email.regex",
                verdict="fail",
                message="column \"email\" has 1 row(s) not matching pattern",
            ),
        ],
        started_at=started,
        finished_at=finished,
    )


class TestWriteJson:
    def test_writes_parseable_json(self, tmp_path: Path) -> None:
        report = _make_report()
        out = tmp_path / "report.json"
        report.write_json(out)

        data = json.loads(out.read_text())
        assert isinstance(data, dict)

    def test_top_level_fields_present(self, tmp_path: Path) -> None:
        report = _make_report(schema="analytics")
        out = tmp_path / "report.json"
        report.write_json(out)
        data = json.loads(out.read_text())

        assert data["probe_name"] == "customer_quality"
        assert data["table"] == "users"
        assert data["schema"] == "analytics"
        assert data["verdict"] == "fail"
        assert data["started_at"] == "2026-05-06T12:00:00+00:00"
        assert data["finished_at"] == "2026-05-06T12:00:01.250000+00:00"
        assert data["duration_seconds"] == 1.25

    def test_schema_is_null_when_missing(self, tmp_path: Path) -> None:
        report = _make_report(schema=None)
        out = tmp_path / "report.json"
        report.write_json(out)
        data = json.loads(out.read_text())
        assert data["schema"] is None

    def test_assertions_array_shape(self, tmp_path: Path) -> None:
        report = _make_report()
        out = tmp_path / "report.json"
        report.write_json(out)
        data = json.loads(out.read_text())

        assert isinstance(data["assertions"], list)
        assert len(data["assertions"]) == 2

        first = data["assertions"][0]
        assert first["assertion_index"] == 0
        assert first["name"] == "email.not_null"
        assert first["verdict"] == "pass"
        assert first["message"] is None

        second = data["assertions"][1]
        assert second["assertion_index"] == 1
        assert second["name"] == "email.regex"
        assert second["verdict"] == "fail"
        assert "row(s) not matching" in second["message"]

    def test_assertion_with_no_name_falls_back_to_index(self, tmp_path: Path) -> None:
        report = _make_report(
            assertions=[
                AssertionResult(
                    assertion_index=0,
                    name=None,
                    verdict="pass",
                    message=None,
                ),
            ],
        )
        out = tmp_path / "report.json"
        report.write_json(out)
        data = json.loads(out.read_text())
        # Same fallback contract as JUnit: "assertion_<index>".
        assert data["assertions"][0]["name"] == "assertion_0"

    def test_accepts_str_path(self, tmp_path: Path) -> None:
        report = _make_report()
        out_str = str(tmp_path / "report.json")
        report.write_json(out_str)
        json.loads(Path(out_str).read_text())
