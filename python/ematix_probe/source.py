"""Source factories for `@probe.data`.

A `Source` describes *where* probe data comes from. Phase 1a
shipped Postgres; Phase 2 (Sprint 4) adds in-process DuckDB and
local Parquet via the engine's scan path. S3 Parquet lands in
Phase 3.
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


def duckdb(path: str) -> Source:
    """In-process DuckDB data source.

    `path` is the DuckDB database file path. Pass `":memory:"`
    for a transient in-memory database (the data only lives for
    the lifetime of the `DataProbe`).

    DuckDB runs in-process — no daemon, no network — so unlike
    `postgres()` there are no credentials to manage; the path is
    the whole connection string.
    """
    if not path:
        raise ValueError("duckdb() expects a non-empty path or ':memory:'")
    return Source(kind="duckdb", url=path)


def parquet(path: str) -> Source:
    """Local Parquet file data source.

    `path` is the filesystem path to a `.parquet` file. The
    Parquet file *is* the table — `table=` on `@probe.data` is
    informational only for parquet sources.

    S3 / object-store Parquet lands in Phase 3; explicitly reject
    `s3://` URLs here so users don't silently fall into a
    file-not-found error.
    """
    if not path:
        raise ValueError("parquet() expects a non-empty file path")
    if path.startswith("s3://"):
        raise ValueError(
            "parquet() does not yet accept s3:// URLs — that lands "
            "in Phase 3. Pass a local filesystem path for now."
        )
    return Source(kind="parquet", url=path)
