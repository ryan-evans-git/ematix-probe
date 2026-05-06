# Security

## Reporting a vulnerability

Email **ryanevans23@gmail.com** with a description of the issue and
how to reproduce it. Please don't open a public issue. We aim to
acknowledge within 72 hours and ship a fix or coordinated
disclosure plan within 14 days for high-severity findings.

## Automated scanning in CI

Every push and pull request runs the following gates; the release
workflow gates wheel builds on the same `ci.yml` run for the same
SHA, so a tag push can't bypass them on the way to PyPI:

| Tool | What it scans | Fails CI on |
|---|---|---|
| `cargo fmt --all -- --check` | Rust formatting | Any drift |
| `cargo clippy --workspace --all-targets -- -D warnings` | Rust lints | Any warning |
| `cargo audit` | `Cargo.lock` vs RustSec DB | Any non-ignored advisory |
| `cargo test --workspace --all-targets` | Rust unit + integration tests | Any failure |
| `ruff check python tests` | Python lints (E, F, W, I, UP, B, SIM) | Any finding |
| `bandit -r python -ll -c pyproject.toml` | Python security lints (medium+) | Any medium / high finding |
| `pip-audit --skip-editable` | Python deps vs PyPI advisory DB | Any advisory |
| `pytest` | Python tests | Any failure |

`release.yml` declares `await-ci` — wheel builds only proceed once
`ci.yml` reaches `success` on the same SHA. So no wheel reaches PyPI
without all checks passing.

## Known accepted advisories

Each entry below is suppressed in CI either via `.cargo/audit.toml`
(Rust) or `pyproject.toml` `[tool.bandit]` (Python). Re-evaluate
quarterly or on any upstream upgrade that might unblock a fix.

### Rust — `.cargo/audit.toml`

*None as of v0.1 Phase 0.* Add entries with rationale in the
`.cargo/audit.toml` `[advisories] ignore` array as transitive
advisories surface.

### Python — `pyproject.toml [tool.bandit]`

*None as of v0.1 Phase 0.* Add entries with rationale to the bandit
config block as findings surface that need suppression.

## Threat model

`ematix-probe` is a testing-automation framework — the operator (a
developer at your company) defines probes that read from configured
sources (Postgres, DuckDB, Parquet, S3) and drive synthetic load at
configured targets (HTTP services, Postgres). The trust boundaries
are:

1. **The operator's probe code is trusted.** A malicious
   `@probe.data` or `@probe.load` decorator function could
   trivially execute arbitrary code (return a SQL string with
   side effects, evaluate to `__import__('os').system('…')`, etc.).
   Same model as pytest, dbt, or Locust.
2. **Probe targets are *partially* trusted.** Data probes read
   rows / Arrow batches from data sources and assert on them; load
   probes send synthetic traffic to HTTP / SQL targets. The probe
   does not parse responses as code. Run history persistence, when
   enabled, writes opaque JSON / OTel histograms to the configured
   Postgres store.
3. **Configuration TOML / connection registry are trusted.**
   They're written by the operator. Inline credentials in TOML
   are a smell; the recommended pattern is env-var indirection.
4. **Logged secrets are not trusted to anyone.** Connection URLs,
   bearer tokens, and other credentials are redacted in `repr()`,
   `Display`, and any tracing / logging path.

The framework is **not designed** for:

- Multi-tenant probe hosting (one operator → many untrusted users
  defining probes).
- Direct exposure of probe-definition endpoints to the internet.
- Authenticated penetration testing of third-party systems without
  the operator's prior authorization.

If you need any of those, run `ematix-probe` inside an isolation
boundary (separate process, container, k8s namespace) per tenant.
