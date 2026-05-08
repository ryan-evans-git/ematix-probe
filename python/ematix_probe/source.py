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
    this module — do not construct directly.

    The `url` field carries different meanings per `kind`:
    - postgres: libpq connection URL
    - duckdb: filesystem path or `:memory:`
    - parquet: filesystem path
    - s3_parquet: unused; see `s3_*` fields instead

    The s3 fields are populated only for `kind == "s3_parquet"`;
    they're explicit (rather than packed into `url` as query
    params) so users + the pyo3 dispatch don't have to round-trip
    through string parsing.
    """

    kind: str
    url: str
    s3_bucket: str | None = None
    s3_key: str | None = None
    s3_region: str | None = None
    s3_endpoint: str | None = None


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


def s3_parquet(
    *,
    bucket: str,
    key: str,
    region: str,
    endpoint_url: str | None = None,
) -> Source:
    """S3 (or S3-compatible) Parquet data source.

    `bucket` + `key` identify the object; `region` is the AWS
    region for the bucket. Pass `endpoint_url` for non-AWS S3
    (LocalStack, MinIO, R2, …); leave it `None` for real AWS.

    Credentials follow the standard AWS resolver chain (env vars
    → credentials file → IAM role) — no explicit hand-off in
    v0.1, same posture as `postgres()`.
    """
    if not bucket:
        raise ValueError("s3_parquet() requires a non-empty bucket")
    if not key:
        raise ValueError("s3_parquet() requires a non-empty key")
    if not region:
        raise ValueError("s3_parquet() requires a non-empty region")
    return Source(
        kind="s3_parquet",
        url="",  # see s3_* fields
        s3_bucket=bucket,
        s3_key=key,
        s3_region=region,
        s3_endpoint=endpoint_url,
    )


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
