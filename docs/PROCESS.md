# ematix-probe — Engineering process

This document defines how we plan, execute, review, and learn on this
project. It is **load-bearing**: PRs that change the cadence update this
file in the same commit.

Status: **active** as of 2026-05-06.

---

## 1. Cadence

| Layer | Length | Output |
|---|---|---|
| **PI (Program Increment)** | ~10 weeks | A shippable release. PI-1 = v0.1 on PyPI. |
| **Sprint** | 1 week (Mon–Fri) | A merged PR per phase or sub-phase. |
| **Day** | n/a | TDD loop. Tests first, always. |

Why these sizes: the project is solo + AI-assisted, so PRs land fast
and 2-week sprints would over-batch. PIs stay long enough to actually
ship something users care about.

## 2. PI planning

Each PI starts with a **PI plan** ([PI_PLAN.md](PI_PLAN.md)) containing:

- PI goal — one sentence, e.g. *"Ship ematix-probe v0.1 to PyPI."*
- Sprint breakdown — phase → sprint mapping
- Risks — what could blow this up, with mitigations
- Out-of-scope — what we explicitly will not do this PI

The PI plan is updated mid-PI when scope drifts. Every drift gets an
entry in [LEARNINGS.md](LEARNINGS.md) so we don't lose the *why*.

## 3. Sprints

Each sprint is one markdown file under `docs/sprints/sprint-NN.md` with
this structure (template at the bottom of this doc):

- **Goal** — one sentence
- **Stories** — checklist of work, each with TDD tasks (RED → GREEN →
  REFACTOR)
- **Definition of Done** — explicit, testable criteria
- **Retro** — filled in on the last day of the sprint

The sprint file is committed at sprint start (before any code), updated
as stories complete (check off boxes in the same PR that lands the
work), and finalized with the retro before the next sprint opens.

## 4. Retros

At the end of every sprint, before opening the next sprint file, fill in
the retro section with four questions:

1. **Kept** — what worked, do more of
2. **Improved** — what was OK but could be sharper
3. **Dropped** — what wasted time, stop doing
4. **Learned** — surprises, gotchas, things future-you should know

Action items from retros either:
- become a sprint story in the next sprint, **or**
- become a one-line entry in [LEARNINGS.md](LEARNINGS.md) if they're
  more rule-of-thumb than concrete work.

A retro with zero entries is a smell — it usually means we didn't look
honestly. Force at least one item per question, even if minor.

## 5. TDD discipline

The non-negotiables:

1. **RED first.** No implementation commit lands before a failing test
   for it. Sprint stories explicitly list the test before the
   implementation.
2. **Smallest passing implementation.** Once the test is red, write the
   simplest thing that turns it green. Refactor *after* green, not
   before.
3. **One green test per commit, ideally.** Bigger commits are fine when
   tests are coupled, but we avoid "huge feature + ten tests" merges —
   they hide regressions.
4. **No skipped or `#[ignore]`d tests merged to `main`** without a
   tracked story to fix them.

Cross-language note: a Rust feature exposed to Python needs tests on
*both* sides — Rust unit tests for the engine logic, Python tests for
the binding surface. Round-trip tests live under `tests/`.

## 6. Documentation that travels with the code

These files are kept honest sprint by sprint:

| File | Purpose | Updated when |
|---|---|---|
| [PRD.md](PRD.md) | Locked v0.1 scope | PRD scope changes — opens a discussion, not silent edit |
| [PI_PLAN.md](PI_PLAN.md) | PI-N goals + sprint map | Sprint completion or scope drift |
| `sprints/sprint-NN.md` | Per-sprint plan + retro | Live during the sprint |
| [LEARNINGS.md](LEARNINGS.md) | Append-only log | Any time we learn something we'd want a future contributor to know |
| `ROADMAP.md` *(future)* | Post-v0.1 horizon | Each PI close |

A change to source code that invalidates any of the above must update
that doc in the same PR. CI may eventually enforce this; for now it's
on us.

## 7. Drift tracking

"Drift" = anywhere reality has moved away from the plan without an
explicit decision. Examples:
- A sprint story slipped two sprints; the PI plan still says it's
  in sprint 2.
- A v0.1 non-goal got implemented anyway because it "fell out for free."
- The decorator surface in code diverged from §6 of the PRD.

Each retro asks **"any drift?"** explicitly. Detected drift becomes
either:
- a doc update in the same PR (preferred), or
- a tracked story to reconcile next sprint.

Silent drift is the failure mode this whole process exists to prevent.

## 8. Sprint file template

```markdown
# Sprint NN — <one-line title>

Dates: YYYY-MM-DD → YYYY-MM-DD
PI: PI-N
Status: planned | active | closed

## Goal

<one sentence>

## Stories

- [ ] **S-NN.1** — <story title>
  - RED: <which test, in which file>
  - GREEN: <minimal implementation>
  - REFACTOR: <optional cleanup>
- [ ] **S-NN.2** — ...

## Definition of Done

- [ ] All sprint tests green in CI
- [ ] PRD / PI plan updated for any scope change
- [ ] Retro filled in below

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
```
