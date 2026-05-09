# Sprint 10 — Phase 8 + Phase 9: explain / doctor polish + v0.1 PyPI release

Dates: TBD (opens once PR for `phase-7` from Sprint 9 merges)
PI: PI-1 (final sprint)
Phase: Phase 8 + Phase 9
Status: **planned**

## Goal

Take the engine + Python surface to a publishable v0.1 on PyPI:
flesh out the CLI, add the `explain` / `doctor` diagnostic
commands, polish docs, run a TestPyPI release dry-run, and cut
the final release.

End of sprint:
- `ematix-probe run` / `list` / `explain` / `doctor` subcommands
  on the Rust CLI per PRD §7.
- `--run-history-db <path>` flag wired into `run` (Sprint 9
  deferred this; the persistence layer is ready).
- README + quickstart doc polished, ematix-flow style mirror
  (see feedback memory).
- TestPyPI dry-run produces a wheel that `pip install`s on a
  fresh machine.
- `pip install ematix-probe` from real PyPI works for a v0.1.

Per [PI_PLAN.md](../PI_PLAN.md):

> Sprint 10 — Phase 8 + Phase 9: `explain` / `doctor` polish +
> docs + v0.1 PyPI release.

## Stories

- [ ] **S-10.1** — `ematix-probe run <module-or-spec>` Rust CLI
       subcommand: invokes the Python entry point or the Rust
       engine directly (decision: how do we route to a Python
       `@probe.data` discovered in a module?).
- [ ] **S-10.2** — `ematix-probe list` enumerates discovered
       probes without running them.
- [ ] **S-10.3** — `ematix-probe explain <probe>` prints the
       compiled plan (assertions, source, table) so users can
       see what the decorator built.
- [ ] **S-10.4** — `ematix-probe doctor` runs the validation
       suite (Postgres reachable? DuckDB linked? S3 creds set?
       Python plugin importable?).
- [ ] **S-10.5** — `--run-history-db` flag on `run`, wired to
       `ematix_probe.run_history.RunHistory`.
- [ ] **S-10.6** — README + docs polish. Mirror ematix-flow
       style (per saved feedback memory) — fetch its README
       first and pattern-match.
- [ ] **S-10.7** — TestPyPI dry-run: build wheels via maturin
       for each Python × OS combo, upload to TestPyPI, install
       on a fresh machine, run the quickstart end-to-end.
- [ ] **S-10.8** — Final v0.1 release: tag, build, push to
       PyPI, GitHub release with the CHANGELOG diff.
- [ ] **S-10.9** — Sprint close + PI-1 retro (covers all 10
       sprints, not just this one).

## Definition of Done

- [ ] All Sprint 10 tests green in CI
- [ ] All prior-phase gates still green
- [ ] `pip install ematix-probe` from PyPI works on a fresh
       machine and the PRD §15 end-to-end example runs green
- [ ] BENCHMARKS.md reports real numbers from a v0.1 build
- [ ] CHANGELOG entry under `## [v0.1.0]` (the first non-
       Unreleased section)
- [ ] PI-1 retro filled (separate doc — covers cross-sprint
       patterns, what worked, what to do differently in PI-2)

## Out of scope (deferred)

- Drift detection / baselines (v0.2 per PRD §3).
- Async PyO3 (v0.2 per S-9.5 decision).
- Backends beyond Postgres / DuckDB / Parquet / HTTP / Postgres
  SQL (v0.2 / v0.3).
- IDE / VSCode integration.
- Browser / UI / mobile testing.

## Risks

1. **PyPI release mechanics.** First v0.1 release; the maturin
   build matrix needs to produce wheels for each supported
   Python × OS combo and the upload step needs an API token.
   Mitigation: rehearse on TestPyPI in S-10.7 before the real
   push in S-10.8.
2. **CLI "how do we run a Python-decorated probe from a Rust
   binary" question.** Two paths: (a) the CLI shells out to
   `python -m ematix_probe.cli run ...` and the Python side
   does discovery + execution; (b) the Rust CLI uses pyo3 to
   call into Python in-process. (a) is simpler; (b) is more
   integrated. Decide at S-10.1 kickoff.
3. **PRD §15 example completeness.** The example must run
   end-to-end without any "...assume Postgres is up..."
   handwaving. Mitigation: stand up a docker-compose for the
   demo environment so the example is reproducible.
4. **README drift vs. ematix-flow.** The two READMEs were
   close at PI start; a year of independent edits widened
   them. Mitigation: per the saved feedback memory, pull
   ematix-flow's README first and pattern-match before any
   substantive edit in S-10.6.

## Retro (filled at sprint close)

### Kept
-

### Improved
-

### Dropped
-

### Learned
-

### Drift?
-
