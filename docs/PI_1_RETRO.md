# PI-1 retrospective

PI-1 closed: **2026-05-09**, all 10 sprints shipped (Sprints 1–10
landed across the planned cadence). The PyPI release itself is
deferred to a manual run of [`docs/RELEASE.md`](RELEASE.md) since
the upload step needs PyPI / GH credentials we can't drive from
inside the loop.

## Sprint-by-sprint summary

| Sprint | Phase | Closed | Headline |
|---|---|---|---|
| 1 | 0 | 2026-05-06 | Workspace skeleton + green CI. |
| 2 | 1a | 2026-05-06 | Postgres adapter + pushdown SQL for `not_null` / `unique` / `between`. |
| 3 | 1b | (mid-PI) | `regex` / `enum` / `row_count` / `freshness`; JUnit + JSON reports; first end-to-end example. |
| 4 | 2 | (mid-PI) | Scan-path: Arrow batches + DuckDB + local Parquet adapters. |
| 5 | 3 | (mid-PI) | S3 Parquet + distribution assertions (`percentile_between`, `cardinality_between`, `schema_match`). |
| 6 | 4a | (mid-PI) | Load probe HTTP MVP — constant-rate scheduler + `p99_under` / `error_rate_below`. |
| 7 | 4b | (mid-PI) | `throughput_above` / `status_code_in` / warmup; httpbin-style end-to-end example; LocalStack S3 + cross-engine consistency. |
| 8 | 5 | (mid-PI) | Load probe VU mode + Postgres SQL adapter + `LoadProfile` evaluator unification. |
| 9 | 6 + 7 | 2026-05-09 | pytest plugin (per-assertion fan-out) + ematix-flow shim + opt-in sqlite run history; async PyO3 deferred to v0.2. |
| 10 | 8 + 9 | 2026-05-09 | Python CLI (run / list / explain / doctor + `--run-history-db`); README rewrite mirroring ematix-flow; `docs/RELEASE.md` runbook. |

## Cross-sprint patterns that worked

### TDD as the load-bearing process rule

Every story RED → GREEN → REFACTOR per
[PROCESS.md §5](PROCESS.md#5-the-development-loop). The discipline
paid off in three concrete ways:

1. **Refactors stayed safe.** Sprint 8's `LoadProfile` trait
   refactor + Sprint 9's `DataProbeCollector` rewrite both
   landed without breakage because the test suite caught
   regressions at the per-test level, not the integration
   level.
2. **Sprint estimates were accurate.** Stories that "felt
   small" but had RED tests showed up as small commits. Stories
   where the RED test took half a day signaled real complexity
   to revisit — Sprint 5's S3 adapter deferral came from a
   RED-step honesty check, not a "let's punt" gut call.
3. **Drift was visible.** Two process slips this PI (Sprint 7
   `--amend`, Sprint 8 `git add -A` swept the lock file) got
   logged as drift in their respective retros instead of
   compounding silently.

### Sprint-end retros + LEARNINGS as separate forms

Per-sprint retros captured what worked / what to keep doing /
what drifted. LEARNINGS.md captured the *one-line lessons* worth
remembering across PIs (PG14 EXTRACT cast quirk, AWS S3 SDK IMDS
fall-through, tokio-postgres bind-type strictness, pytest11 +
coverage-init ordering, trait-over-generic decision rationale,
async PyO3 deferral). Different audience for each — retros for
"what should I do differently next sprint", LEARNINGS for "what
should the NEXT contributor know about why this code is the way
it is".

### Documented deferrals over speculative implementation

Three deferrals to v0.2 + one deferral to "manual user step":

1. Async PyO3 (Sprint 9 / S-9.5) — `pyo3-asyncio` API churn.
   PI risk #4 escape hatch taken on purpose.
2. Drift detection — explicitly v0.2 per PRD §3.
3. Distributed load generation — explicitly v0.2 per PRD §3.
4. Actual PyPI upload — user-only, runbook documented.

Each is dated, signed, and rationaled. Future contributors can
re-litigate, but they have to argue against a specific
documented decision rather than a missed feature.

## Cross-sprint patterns to improve

### CI flakiness on load-bearing tests

`tests/test_source_s3.py::TestS3DispatchUnreachable::test_run_attempts_s3_dispatch`
silently passed-when-it-should-fail in Sprint 9 because a
synthesized pytester test file monkeypatched `DataProbe.run`
at module level and leaked into the in-process pytester run.
Caught + fixed within the sprint, but the failure mode (test
that *should* raise stops raising) is exactly the kind that's
hard to spot in a green-suite scan. **PI-2 follow-up:** add a
"flaky tests inventory" doc + a CI annotation pattern for
known-order-dependent tests.

### CI iteration cost on plugin / coverage interactions

Sprint 9's pytest-plugin work took **3 CI iterations** to land
green (plugin double-registration → coverage measurement →
lazy-import → workflow-runner switch). Each iteration cost ~5
min of CI wait. The root cause was a structural pytest11 ↔
coverage interaction nobody documents clearly. **PI-2
follow-up:** consider a tiny "ci-canary" workflow that runs the
exact CI invocation against a feature branch before pushing the
real change, so the iteration loop is local instead of CI-bound.

### Sprint sizing drift in the closing sprint

Sprint 10 collapsed S-10.5 into S-10.1 and S-10.7+S-10.8 into
one runbook story. The work was the right shape; the *story
boundaries* drifted. Worth flagging: in PI-2, either re-scope
stories at sprint kickoff to match natural commit boundaries,
or accept that closing-sprint stories are different in
character from mid-PI stories.

## Numbers

- **Sprints completed:** 10/10 in the planned cadence.
- **Stories completed:** ~70 across all sprints (estimate;
  some collapsed mid-sprint as in S-10.5).
- **CI iterations to green per sprint:** 1–3, median 1.
- **Test count at PI close:**
  - Rust: ~150+ tests across the workspace
    (`cargo test --workspace`).
  - Python: 106 tests, 98% line + branch coverage across
    `python/ematix_probe/`.
- **Documented LEARNINGS entries:** 8 across the PI.
- **PRs opened in PI-1:** ~12 (one per sprint + a few
  documentation / CI follow-ups).

## What to do differently in PI-2

1. **Spike technical risks earlier.** Async PyO3 deferred to v0.2
   because Sprint 9 was its first real exercise. Earlier spike
   (in Sprint 4 or 5, when Phase 4 work had budget) would have
   given enough signal to commit or defer with more time to
   adjust the rest of the PI.
2. **CI canary loop.** See "CI iteration cost" above.
3. **Test-isolation explicit invariants.** Document which
   tests rely on global state (DB connections, monkeypatches,
   working directories) so future tests don't quietly contend.
4. **Schedule a benchmarks sprint.** BENCHMARKS.md has been a
   "deferred to follow-up" item across multiple sprints. PI-2
   should give it a dedicated sprint with measurable
   performance gates per PRD §11.

## v0.1 readiness checklist

- [x] Data probes against Postgres / DuckDB / Parquet (local + S3).
- [x] Load probes against HTTP / Postgres SQL with constant-rate +
      virtual-user schedulers.
- [x] All assertion variants from PRD §6 (data) + §6.5–6.6 (load,
      Rust API only).
- [x] JUnit + JSON reports.
- [x] pytest plugin with per-assertion fan-out.
- [x] ematix-flow integration shim (zero hard dep).
- [x] Opt-in sqlite run history.
- [x] CLI (run / list / explain / doctor + `--run-history-db`).
- [x] README mirroring ematix-flow style.
- [x] Release runbook (`docs/RELEASE.md`).
- [ ] PyPI upload itself — manual via the runbook.
- [ ] BENCHMARKS.md numbers — deferred follow-up.
