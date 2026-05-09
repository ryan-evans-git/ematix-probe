"""`ematix-probe` command-line interface.

Wired through the `[project.scripts]` entry in pyproject.toml so
`pip install ematix-probe` produces a console-script.

Subcommands (PRD §7):
- `run` — discover and execute probes from a Python file.
- `list` — discover probes without running them.
- `explain` — print the compiled plan for one probe.
- `doctor` — environment health check.

The CLI deliberately lives in Python (not the Rust binary at
`crates/ematix-probe-cli`). Probe discovery lives where probes
live — Python. The Rust binary stays as workspace scaffolding.
"""

from __future__ import annotations

import argparse
import importlib.util
import sys
from collections.abc import Sequence
from pathlib import Path

from .probe import DataProbe
from .report import RunReport


def main(argv: Sequence[str] | None = None) -> int:
    parser = _build_parser()
    args = parser.parse_args(argv)
    if not getattr(args, "func", None):
        parser.print_help()
        return 0
    return args.func(args)


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="ematix-probe",
        description="Declarative testing automation: data probes + load probes.",
    )
    sub = parser.add_subparsers(dest="cmd", metavar="<command>")

    run = sub.add_parser("run", help="Run all probes in a Python file.")
    run.add_argument("path", help="Path to a .py file containing @probe.* decorators.")
    run.add_argument(
        "--run-history-db",
        metavar="PATH",
        help="Append one row per probe execution to this sqlite file.",
    )
    run.set_defaults(func=_cmd_run)

    lst = sub.add_parser("list", help="List probes in a Python file (no execution).")
    lst.add_argument("path")
    lst.set_defaults(func=_cmd_list)

    explain = sub.add_parser(
        "explain", help="Print the compiled plan for one probe."
    )
    explain.add_argument("path")
    explain.add_argument("probe", help="Probe attribute name (matches `def name`).")
    explain.set_defaults(func=_cmd_explain)

    doctor = sub.add_parser(
        "doctor", help="Environment health check (imports, _core extension, adapters)."
    )
    doctor.set_defaults(func=_cmd_doctor)

    return parser


def _import_probe_file(path_str: str):
    path = Path(path_str)
    if not path.is_file():
        raise FileNotFoundError(f"no such file: {path}")
    # Use a unique module name so re-running doesn't hit a stale
    # cached module from a previous invocation.
    mod_name = f"_ematix_probe_user_{abs(hash(str(path.resolve())))}"
    spec = importlib.util.spec_from_file_location(mod_name, path)
    if spec is None or spec.loader is None:
        raise ImportError(f"cannot load module from {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[mod_name] = module
    spec.loader.exec_module(module)
    return module


def _discover_probes(module) -> list[tuple[str, DataProbe]]:
    out: list[tuple[str, DataProbe]] = []
    for name in dir(module):
        if name.startswith("_"):
            continue
        obj = getattr(module, name)
        if isinstance(obj, DataProbe):
            out.append((name, obj))
    return out


def _cmd_run(args) -> int:
    try:
        module = _import_probe_file(args.path)
    except (FileNotFoundError, ImportError) as e:
        print(f"error: {e}", file=sys.stderr)
        return 2

    probes = _discover_probes(module)
    if not probes:
        print(f"0 probes found in {args.path}")
        return 0

    history = None
    if args.run_history_db:
        from .run_history import RunHistory

        history = RunHistory(args.run_history_db)

    any_fail = False
    for name, p in probes:
        report = p.run()
        if history is not None:
            history.record(report)
        _print_report(name, report)
        if report.verdict != "pass":
            any_fail = True
    return 1 if any_fail else 0


def _cmd_list(args) -> int:
    try:
        module = _import_probe_file(args.path)
    except (FileNotFoundError, ImportError) as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
    probes = _discover_probes(module)
    if not probes:
        print(f"0 probes found in {args.path}")
        return 0
    for name, p in probes:
        schema = f"{p.schema}." if p.schema else ""
        print(f"{name}  ({schema}{p.table})  [{len(p.assertion_names())} assertions]")
    return 0


def _cmd_explain(args) -> int:
    try:
        module = _import_probe_file(args.path)
    except (FileNotFoundError, ImportError) as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
    probes = dict(_discover_probes(module))
    if args.probe not in probes:
        print(f"error: probe {args.probe!r} not found in {args.path}", file=sys.stderr)
        print(f"  available: {', '.join(probes) or '(none)'}", file=sys.stderr)
        return 2
    p = probes[args.probe]
    schema = f"{p.schema}." if p.schema else ""
    print(f"probe: {args.probe}")
    print(f"  table: {schema}{p.table}")
    print(f"  source: {p.source.kind}")
    print("  assertions:")
    for name in p.assertion_names():
        print(f"    - {name}")
    return 0


def _cmd_doctor(_args) -> int:
    # Each check prints a one-line status; exit 0 only if every
    # check passes. Keep the surface minimal — this is a sanity
    # ping, not a full diagnostic.
    ok = True
    try:
        import ematix_probe  # noqa: F401

        print("[OK]   ematix_probe importable")
    except Exception as e:
        print(f"[FAIL] ematix_probe importable: {e}")
        ok = False
    try:
        from ematix_probe import _core  # noqa: F401

        print("[OK]   _core extension loaded")
    except Exception as e:
        print(f"[FAIL] _core extension loaded: {e}")
        ok = False
    for name in ("postgres", "duckdb", "parquet", "s3_parquet"):
        try:
            from ematix_probe._core import (
                run_duckdb_probe,  # noqa: F401
                run_parquet_probe,  # noqa: F401
                run_postgres_probe,  # noqa: F401
                run_s3_parquet_probe,  # noqa: F401
            )

            print(f"[OK]   adapter dispatch: {name}")
        except Exception as e:
            print(f"[FAIL] adapter dispatch: {name}: {e}")
            ok = False
            break
    return 0 if ok else 1


def _print_report(name: str, report: RunReport) -> None:
    marker = {"pass": "[PASS]", "fail": "[FAIL]", "error": "[ ERR]"}[report.verdict]
    print(f"{marker} {name}  ({report.table})")
    if report.verdict == "pass":
        return
    for a in report.assertions:
        if a.verdict == "pass":
            continue
        label = a.name or f"assertion_{a.assertion_index}"
        print(f"         {label}: {a.verdict} -- {a.message or 'no message'}")


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
