"""Source factories for `@probe.data`.

A `Source` describes *where* probe data comes from — for v0.1
that's only Postgres. DuckDB / Parquet (local + S3) factories
land in Phase 2 / Phase 3.
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class Source:
    """A data probe source. Build via the factory functions in
    this module — do not construct directly."""

    kind: str
    url: str


def postgres(url: str) -> Source:
    """Postgres data source.

    `url` must be a libpq-style connection string starting with
    `postgres://` or `postgresql://`. To pull a URL from an
    environment variable, do that explicitly:

        source.postgres(os.environ["DATABASE_URL"])

    No env-var indirection is built into this factory — it keeps
    the probe declaration explicit about where credentials live.
    """
    if not (url.startswith("postgres://") or url.startswith("postgresql://")):
        raise ValueError(
            "postgres() expects a connection URL starting with "
            f"postgres:// or postgresql://, got: {url!r}"
        )
    return Source(kind="postgres", url=url)
