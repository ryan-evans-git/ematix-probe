"""S-10.2 / S-10.3 / S-10.4 — CLI list / explain / doctor.

Covers the three side subcommands so cli.py is fully exercised.
The contract:
- `list` prints one line per probe (name, table, assertion count)
  and never executes anything.
- `explain <probe>` prints the compiled plan for one probe by name;
  errors with a non-zero exit when the probe doesn't exist.
- `doctor` runs a small set of import / extension checks and
  exits non-zero if any fail.
"""

from __future__ import annotations

from pathlib import Path

from ematix_probe.cli import main


def _make_probe_file(tmp_path: Path, body: str) -> Path:
    f = tmp_path / "probes.py"
    f.write_text("from ematix_probe import probe, source\n" + body)
    return f


# ---- list ----------------------------------------------------------------


def test_list_prints_one_line_per_probe(tmp_path, capsys):
    f = _make_probe_file(
        tmp_path,
        '''
@probe.data(source=source.postgres("postgres://x"), table="users", schema="public")
def users_quality(t):
    t.column("id").not_null()
    t.column("email").not_null()

@probe.data(source=source.postgres("postgres://x"), table="orders")
def orders_quality(t):
    t.column("id").not_null()
''',
    )
    rc = main(["list", str(f)])
    assert rc == 0
    out = capsys.readouterr().out
    assert "users_quality" in out
    assert "public.users" in out
    assert "orders_quality" in out
    assert "2 assertions" in out
    assert "1 assertions" in out


def test_list_empty_file_reports_zero(tmp_path, capsys):
    f = tmp_path / "empty.py"
    f.write_text("X = 1\n")
    rc = main(["list", str(f)])
    assert rc == 0
    out = capsys.readouterr().out
    assert "0 probes" in out


def test_list_missing_file_errors(tmp_path):
    rc = main(["list", str(tmp_path / "nope.py")])
    assert rc == 2


# ---- explain -------------------------------------------------------------


def test_explain_prints_plan_for_named_probe(tmp_path, capsys):
    f = _make_probe_file(
        tmp_path,
        '''
@probe.data(source=source.postgres("postgres://x"), table="users", schema="public")
def users_quality(t):
    t.column("id").not_null()
    t.column("email").regex(r".+@.+")
''',
    )
    rc = main(["explain", str(f), "users_quality"])
    assert rc == 0
    out = capsys.readouterr().out
    assert "users_quality" in out
    assert "public.users" in out
    assert "postgres" in out
    assert "id.not_null" in out
    assert "email.regex" in out


def test_explain_missing_probe_errors_with_available_list(tmp_path, capsys):
    f = _make_probe_file(
        tmp_path,
        '''
@probe.data(source=source.postgres("postgres://x"), table="t")
def actual(t):
    t.column("id").not_null()
''',
    )
    rc = main(["explain", str(f), "typo"])
    assert rc == 2
    err = capsys.readouterr().err
    assert "typo" in err
    assert "actual" in err  # surfaces the available probe so users can fix the typo


def test_explain_missing_file_errors(tmp_path):
    rc = main(["explain", str(tmp_path / "nope.py"), "x"])
    assert rc == 2


# ---- doctor --------------------------------------------------------------


def test_doctor_passes_in_a_healthy_environment(capsys):
    rc = main(["doctor"])
    assert rc == 0
    out = capsys.readouterr().out
    assert "ematix_probe importable" in out
    assert "_core extension loaded" in out
    assert "adapter dispatch" in out
    # No FAIL lines on a healthy install.
    assert "[FAIL]" not in out
