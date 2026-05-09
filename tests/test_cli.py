"""S-10.x — Python CLI (`ematix-probe`) — `run` subcommand.

Spec (S-10.1):
- `ematix-probe run <path-to-py-file>` imports the file as a
  module, finds every module-level `DataProbe` attribute, and
  runs each one in sequence.
- Exit code 0 only if every probe verdict is `pass`.
- Per-probe verdict + per-failed-assertion messages go to stdout.

Tests stub `DataProbe.run` so the CLI logic is exercised without
needing a live adapter.
"""

from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path

import pytest

from ematix_probe.cli import main
from ematix_probe.probe import DataProbe
from ematix_probe.report import AssertionResult, RunReport


def _make_probe_file(tmp_path: Path, body: str) -> Path:
    f = tmp_path / "probes.py"
    f.write_text(
        "from ematix_probe import probe, source\n"
        f"{body}\n"
    )
    return f


def _report(name: str, verdict: str, results: list[AssertionResult]) -> RunReport:
    now = datetime.now(tz=timezone.utc)
    return RunReport(
        probe_name=name,
        table="t",
        schema=None,
        verdict=verdict,
        assertions=results,
        started_at=now,
        finished_at=now,
    )


def test_run_exits_zero_when_all_probes_pass(tmp_path, monkeypatch, capsys):
    monkeypatch.setattr(
        DataProbe,
        "run",
        lambda self: _report(
            self.__name__,
            "pass",
            [AssertionResult(0, "pass", None, "id.not_null")],
        ),
    )
    f = _make_probe_file(
        tmp_path,
        '''
@probe.data(source=source.postgres("postgres://x"), table="t")
def my_check(t):
    t.column("id").not_null()
''',
    )
    rc = main(["run", str(f)])
    assert rc == 0
    out = capsys.readouterr().out
    assert "my_check" in out
    assert "pass" in out.lower()


def test_run_exits_nonzero_on_any_fail(tmp_path, monkeypatch, capsys):
    def _per_probe(self):
        if self.__name__ == "passing":
            return _report("passing", "pass",
                           [AssertionResult(0, "pass", None, "id.not_null")])
        return _report("failing", "fail",
                       [AssertionResult(0, "fail", "5 nulls", "id.not_null")])

    monkeypatch.setattr(DataProbe, "run", _per_probe)
    f = _make_probe_file(
        tmp_path,
        '''
@probe.data(source=source.postgres("postgres://x"), table="t")
def passing(t):
    t.column("id").not_null()

@probe.data(source=source.postgres("postgres://x"), table="t")
def failing(t):
    t.column("id").not_null()
''',
    )
    rc = main(["run", str(f)])
    assert rc != 0
    out = capsys.readouterr().out
    assert "passing" in out
    assert "failing" in out
    assert "5 nulls" in out, "failure message must surface to user"


def test_run_missing_file_errors_cleanly(tmp_path):
    rc = main(["run", str(tmp_path / "no_such_file.py")])
    assert rc != 0


def test_run_file_with_no_probes_succeeds_quietly(tmp_path, capsys):
    f = tmp_path / "empty.py"
    f.write_text("X = 1\n")
    rc = main(["run", str(f)])
    assert rc == 0
    out = capsys.readouterr().out
    # Nothing to do is success — but the user should know.
    assert "no probes" in out.lower() or "0 probes" in out.lower()


def test_run_writes_run_history_when_flag_passed(tmp_path, monkeypatch):
    """S-10.5 acceptance — verify --run-history-db wires through."""
    import sqlite3

    monkeypatch.setattr(
        DataProbe,
        "run",
        lambda self: _report(
            self.__name__,
            "pass",
            [AssertionResult(0, "pass", None, "id.not_null")],
        ),
    )
    f = _make_probe_file(
        tmp_path,
        '''
@probe.data(source=source.postgres("postgres://x"), table="t")
def history_check(t):
    t.column("id").not_null()
''',
    )
    db = tmp_path / "history.sqlite"
    rc = main(["run", str(f), "--run-history-db", str(db)])
    assert rc == 0
    with sqlite3.connect(db) as conn:
        rows = conn.execute(
            "SELECT probe_name, verdict FROM runs"
        ).fetchall()
    assert rows == [("history_check", "pass")]


@pytest.mark.parametrize("argv", [["--help"], ["run", "--help"], []])
def test_help_paths_exit_cleanly(argv, capsys):
    # --help variants exit 0 via SystemExit; bare invocation exits
    # with usage. Any of these should not raise an unhandled
    # exception.
    try:
        main(argv)
    except SystemExit as e:
        # argparse exits 0 for --help, 2 for missing required.
        assert e.code in (0, 2)
