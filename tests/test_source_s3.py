"""S-6.2 — Python `source.s3_parquet` builder + dispatch.

Builder-level tests only; live S3 round-trip is exercised by
the Rust S3ParquetAdapter tests against `LocalFileSystem` and
manual against LocalStack/AWS. Adding LocalStack to the Python
test surface waits on a follow-up sprint.
"""

from __future__ import annotations

import pytest
from ematix_probe import probe, source


class TestS3ParquetSource:
    def test_basic(self):
        s = source.s3_parquet(bucket="my-bucket", key="data/users.parquet", region="us-east-1")
        assert s.kind == "s3_parquet"
        assert s.s3_bucket == "my-bucket"
        assert s.s3_key == "data/users.parquet"
        assert s.s3_region == "us-east-1"
        assert s.s3_endpoint is None

    def test_with_endpoint_for_localstack(self):
        s = source.s3_parquet(
            bucket="b",
            key="k",
            region="us-east-1",
            endpoint_url="http://localhost:4566",
        )
        assert s.s3_endpoint == "http://localhost:4566"

    def test_rejects_empty_bucket(self):
        with pytest.raises(ValueError, match="bucket"):
            source.s3_parquet(bucket="", key="k", region="us-east-1")

    def test_rejects_empty_key(self):
        with pytest.raises(ValueError, match="key"):
            source.s3_parquet(bucket="b", key="", region="us-east-1")

    def test_rejects_empty_region(self):
        with pytest.raises(ValueError, match="region"):
            source.s3_parquet(bucket="b", key="k", region="")


class TestS3DispatchUnreachable:
    """If a probe is built against an s3_parquet source, run() must
    dispatch to the right pyo3 entry point (not crash with
    NotImplementedError or fall through to another adapter). Hard
    to verify without a live S3, so we just check the dispatch
    *attempts* the s3 path by catching the ConnectionError that an
    invalid bucket produces."""

    def test_run_attempts_s3_dispatch(self):
        s3_src = source.s3_parquet(
            bucket="ematix-probe-test-nonexistent-xyz-9999",
            key="missing.parquet",
            region="us-east-1",
            # Point at localhost so we get a fast connection refused
            # instead of a real-S3 timeout.
            endpoint_url="http://127.0.0.1:1",
        )

        @probe.data(source=s3_src, table="users")
        def quality(t):
            t.column("id").not_null()

        # Should raise something other than NotImplementedError —
        # NotImplementedError would mean dispatch isn't wired.
        with pytest.raises(Exception) as exc_info:
            quality.run()
        assert not isinstance(exc_info.value, NotImplementedError), (
            f"s3_parquet dispatch isn't wired: {exc_info.value!r}"
        )
