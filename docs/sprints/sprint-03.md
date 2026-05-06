# Sprint 3 — Phase 1b: data probe report surface + extended assertions

Dates: 2026-05-07 → 2026-05-13
PI: PI-1
Phase: Phase 1b
Status: **closed** — all 8 stories shipped on `phase-1b`

## Goal

Round out the data-probe MVP: extend the assertion vocabulary with
four more checks (`regex`, `enum`, `row_count`, `freshness`), and
ship the first machine-readable report formats (JUnit XML for CI,
JSON for downstream tooling). End of sprint: a single end-to-end
example runs against `ematix-flow` Postgres + emits JUnit / JSON
that drops into a CI runner unmodified.

## Stories

Each story RED → GREEN → REFACTOR per [PROCESS.md §5](../PROCESS.md).

- [x] **S-3.1 — `regex` assertion**
  - RED: integration test seeds rows where one `email` doesn't
    match `r".+@.+\..+"`. Expects `Verdict::Fail`.
  - GREEN: Pushdown SQL `SELECT count(*) FROM <t> WHERE <col> !~ $1`
    (Postgres `~`/`!~` regex operators). Add
    `Assertion::Regex { column, pattern: String }`.
  - REFACTOR: Decide if the regex SQL warrants a shared helper.

- [x] **S-3.2 — `enum` assertion**
  - RED: rows with one country code outside `{"US", "CA", ...}`
    → Fail.
  - GREEN: Pushdown
    `SELECT count(*) FROM <t> WHERE <col> NOT IN ($1, $2, ...)`.
    Add `Assertion::Enum { column, allowed: Vec<String> }`.
  - REFACTOR: Parameter-binding helper if multiple assertions
    end up needing variable-arity binding.

- [x] **S-3.3 — `row_count` (table-level) assertion**
  - RED: empty table fails `at_least(1)`; oversized table fails
    `at_most(1_000)`.
  - GREEN: Pushdown
    `SELECT count(*) FROM <t>`. Add
    `Assertion::RowCount { low: Option<i64>, high: Option<i64> }`
    where `None` = unbounded.
  - REFACTOR: Promote `low`/`high` shape to a shared `Range` if
    `Between` and `RowCount` overlap (probably not — RowCount is
    integer, Between is f64).

- [x] **S-3.4 — `freshness` (table-level) assertion**
  - RED: table whose `MAX(updated_at)` is 48h old → fails
    `within("24h")`.
  - GREEN: Pushdown
    `SELECT now() - MAX(<col>) FROM <t>`. Compare returned
    interval against the threshold. Add
    `Assertion::Freshness { column, within_seconds: i64 }`.
    Python side parses the duration string ("24h", "6h", "30m")
    on the way in.
  - REFACTOR: Split duration parsing into `python/.../duration.py`
    if reused.

- [x] **S-3.5 — JUnit XML report**
  - RED: Python test runs a probe + writes JUnit; asserts the
    XML parses with `xml.etree.ElementTree`, has one `<testsuite>`
    per probe, one `<testcase>` per assertion, `<failure>` on
    Fail, `<error>` on Error.
  - GREEN: Add `RunReport.write_junit(path)` on the Python side
    (pure Python — assemble from `report.assertions`). No Rust
    work; the report shape lives entirely in Python for v0.1.
  - REFACTOR: Pull the writer into a `report.py` module if the
    JSON writer (S-3.6) shares logic.

- [x] **S-3.6 — JSON report**
  - RED: probe runs + `report.write_json(path)` produces a file
    that `json.load`s into a stable schema (probe name, verdict,
    timestamps, per-assertion results with messages).
  - GREEN: Pure-Python writer. Schema documented in docstring.
  - REFACTOR: If the schema settles, consider exposing it as
    `ematix_probe.report.JsonReport` for type-checking
    consumers.

- [x] **S-3.7 — End-to-end example**
  - Update [examples/](../../examples/) (creating it if needed)
    with a runnable script + README that:
    1. Boots a Postgres testcontainer
    2. Loads sample data
    3. Runs a `@probe.data` covering all 7 assertions
    4. Writes JUnit + JSON reports to `out/`
    5. Includes screenshot of the JUnit report rendered by
       a CI viewer
  - The PRD §15 example becomes runnable.

- [x] **S-3.8 — CHANGELOG / sprint / learnings**

## Definition of Done

- [x] All Sprint 3 tests green in CI
- [x] All Phase 0 + Phase 1a gates still green
- [x] CI workflow green on `phase-1b` branch
- [ ] PR `phase-1b` → `main` opened and merged
- [x] CHANGELOG entry under `## [Unreleased]` for Phase 1b
- [x] PRD §6.1 example covers all 7 assertions and still type-checks
- [x] Retro filled below

## Out of scope (deferred)

- `regex` flavors beyond Postgres' POSIX (`~`); ECMAScript-style
  patterns deferred to Phase 1c if needed.
- `freshness` on streaming sources (only static tables in v0.1).
- Custom row predicates (`t.row(lambda r: ...)`); deferred to v0.2.

## Risks

1. **Postgres regex syntax differences from Python regex.** Users
   write `r".+@.+\..+"` and expect Python re semantics. Postgres
   POSIX is close-but-not-identical. Mitigation: document the
   subset that works, warn loudly on differences.
2. **Duration parsing surface.** Accept `"24h"`, `"6h30m"`, etc.?
   v0.1 keeps it minimal: `<int><unit>` where unit ∈ `s/m/h/d`.
3. **JUnit XML quirks.** Different CI runners parse JUnit
   differently (Jenkins vs. GitHub vs. GitLab). Mitigation:
   target the GitHub Actions JUnit reporter format.

## Retro (filled at sprint close)

### Kept
- Strict RED → GREEN pattern with no wildcard `_ =>` arms in RED
  commits. Each new `Assertion::*` variant deliberately broke the
  exhaustive match and the GREEN commit closed it. Same approach
  as Sprint 2; no friction.
- Story-level commits (one RED, one GREEN per story) keep the diff
  surface tight enough to review at the commit level. 13 commits
  on `phase-1b`, all atomic.
- Bundling the JUnit and JSON writers under a shared `report.py`
  module after S-3.5 paid off in S-3.6: the wrapper dataclass +
  `to_dict` reused everything, no duplicated traversal logic.

### Improved
- Anchored `Edit` calls on lines *inside* the `impl PostgresAdapter`
  block, not on the `/// Combine ...` doc that lives below the
  closing `}`. Cost a re-do twice in S-3.1/S-3.2. Saved as a
  feedback memory + caught first-try in S-3.3 / S-3.4.
- Picked the Python wrapper for report metadata over augmenting
  `core::RunSummary`. Result: zero pyo3 churn for S-3.5/3.6,
  faster iteration on the Python report shape.

### Dropped
- All four "REFACTOR: maybe extract a helper" notes from the
  story specs. Each `run_<assertion>` method ended up small and
  self-contained — no shared abstraction was justified. Same
  precedent as Sprint 2 (only S-2.1 had a real REFACTOR commit).
- `JsonReport` typed-class for type-checking consumers (S-3.6
  REFACTOR suggestion). The dict-shape is fine; revisit when an
  external consumer asks.
- The CI screenshot in the S-3.7 example. The JUnit XML is
  reporter-agnostic; pasting a screenshot into the README would
  bloat the repo with no compensating signal. README explains
  the `dorny/test-reporter` wiring instead.

### Learned
- See LEARNINGS entry: testcontainers-modules' default Postgres
  image is `postgres:11-alpine`, not the latest. The freshness
  query worked there but panicked on PG 14+ because
  `EXTRACT(EPOCH FROM ...)` returns `numeric` from PG 14 onward.
  Pin the test image to the version range you intend to support.
- See LEARNINGS entry: e2e tests through testcontainers' Ryuk
  reaper occasionally fail to map the reaper's port 8080.
  `TESTCONTAINERS_RYUK_DISABLED=true` is a viable local
  workaround; CI should not need it (each job is a fresh runner).

### Drift?
- None. All 8 stories landed within scope. Out-of-scope items
  (compound durations like `6h30m`, ECMAScript regex flavors,
  streaming freshness) stayed deferred per the original sprint
  plan.
