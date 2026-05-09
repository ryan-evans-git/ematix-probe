# PI Plan — ematix-probe

Process: see [PROCESS.md](PROCESS.md). Cadence is 1-week sprints, retro
on the last day, PI close on the last sprint of the increment.

---

## PI-1 — v0.1 to PyPI

Dates: 2026-05-06 → 2026-07-15 (10 weeks, 10 sprints)
Status: **active** (Sprint 3 in flight; Sprints 1 & 2 closed
same-day. Schedule running ~9 weeks ahead — re-baseline after
Sprint 4 with one more data point.)

### PI goal

Ship `ematix-probe` v0.1 to PyPI: data probes (Postgres + DuckDB +
Parquet local/S3) and load probes (HTTP + Postgres SQL), pytest
plugin, `ematix-flow` integration shim, opt-in run history persistence.

Success = `pip install ematix-probe` works for an external user, the
end-to-end example in [PRD.md §15](PRD.md) runs green, and BENCHMARKS.md
reports real numbers.

### Sprint breakdown

The PRD's phases (§13) map to sprints roughly 1:1, with two phases
(Phase 1 data MVP, Phase 4 load MVP) budgeted for two sprints because
they introduce the most new infrastructure.

| Sprint | Phase | Goal | Status |
|---|---|---|---|
| **1** | Phase 0 | Workspace skeleton + green CI | **closed 2026-05-06** ([retro](sprints/sprint-01.md#retro-closed-2026-05-06)) |
| **2** | Phase 1a | Data probe MVP — Postgres adapter + pushdown SQL for `not_null` / `unique` / `between` | **closed 2026-05-06** ([retro](sprints/sprint-02.md#retro-closed-2026-05-06)) |
| **3** (this sprint) | Phase 1b | Data probe MVP — `regex` / `enum` / `row_count` / `freshness`; JUnit + JSON reports; first end-to-end example | active |
| **4** | Phase 2 | Data probe scan path — Arrow batches in Rust; DuckDB + local Parquet adapters | planned |
| **5** | Phase 3 | Data probe S3 + distribution assertions (`percentile_between`, `cardinality_between`, `schema_match`) | planned |
| **6** | Phase 4a | Load probe HTTP MVP — constant-rate scheduler, OTel ExponentialHistogram, `p99_under` / `error_rate_below` | planned |
| **7** | Phase 4b | Load probe HTTP polish — `throughput_above` / `status_code_in` / warmup; httpbin-style end-to-end example | planned |
| **8** | Phase 5 | Load probe VU mode + Postgres SQL adapter; query parameterization | **closed 2026-05-09** ([retro](sprints/sprint-08.md#retro-filled-at-sprint-close)) |
| **9** | Phase 6 + Phase 7 | pytest plugin + `ematix-flow` integration shim + opt-in run history persistence | planned |
| **10** | Phase 8 + Phase 9 | `explain` / `doctor` polish + docs + v0.1 PyPI release | planned |

A sprint that overruns drops scope to the next sprint, not pushes the
PI date. PI-1 length is fixed; what changes is what fits.

### Risks (and mitigations)

1. **PyO3 + maturin build matrix on multiple Pythons + multiple OS** —
   ematix-flow already has a working CI for this; copy that workflow
   verbatim in Sprint 1, don't reinvent.
2. **OTel ExponentialHistogram crate maturity in Rust** — the
   `opentelemetry` crate's exp-histogram support is recent. If it isn't
   ready, fall back to `hdrhistogram` internally and re-encode at
   serialization time. Decide in Sprint 6, not earlier.
3. **S3 Parquet performance** — IO-bound, not CPU-bound. Sprint 5
   includes a benchmark gate (a 100M-row scan must finish under 30s on
   M3 Pro per PRD §11) so we catch regressions before they ship.
4. **Async PyO3 binding complexity** — `pyo3-asyncio` API has churned.
   Sprint 9 (pytest plugin) is the first sprint that exercises async
   end-to-end; if it slips, async support drops to v0.2 and §6
   downgrades to "sync only."
5. **Solo + AI-assisted scope creep** — easy to keep saying yes. The
   v0.1 non-goals in PRD §3 are the firewall; any merge that touches
   those is a scope drift and gets flagged in retro.

### Out of scope this PI

Same as PRD §3 non-goals. Repeating the load-bearing ones:
- Distributed load generation
- Browser / UI / mobile testing
- gRPC / WebSocket / GraphQL / message queues
- Drift detection / baseline comparison (v0.2 layers on top of run
  history that ships in this PI)
- Backends beyond Postgres / DuckDB / Parquet (data) and HTTP / Postgres
  (load)

### Mid-PI checkpoints

- **End of Sprint 3** — first end-to-end data-probe example must run
  green. If not, drop scan-path scope (Sprint 4) to make Sprint 5 cover
  more.
- **End of Sprint 7** — first end-to-end load-probe example must run
  green. If not, drop VU mode (Sprint 8) to v0.2.
- **End of Sprint 10** — `pip install ematix-probe` from TestPyPI must
  work for a fresh-machine user. If not, PyPI release slips to PI-2;
  the PI ships as a tagged GitHub release instead.
