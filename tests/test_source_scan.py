"""S-4.7 — Python source factories for DuckDB + local Parquet.

Builder-level tests (no actual scan). End-to-end execution against
both backends is exercised by the S-4.8 quickstart.
"""

from __future__ import annotations

from pathlib import Path

import pytest
from ematix_probe import source


class TestDuckdbSource:
    def test_duckdb_in_memory(self):
        s = source.duckdb(":memory:")
        assert s.kind == "duckdb"
        assert s.url == ":memory:"

    def test_duckdb_file_path(self, tmp_path: Path):
        p = str(tmp_path / "warehouse.duckdb")
        s = source.duckdb(p)
        assert s.kind == "duckdb"
        assert s.url == p

    def test_duckdb_rejects_empty_path(self):
        with pytest.raises(ValueError, match="duckdb"):
            source.duckdb("")


class TestParquetSource:
    def test_parquet_local_path(self, tmp_path: Path):
        p = str(tmp_path / "events.parquet")
        s = source.parquet(p)
        assert s.kind == "parquet"
        assert s.url == p

    def test_parquet_rejects_empty_path(self):
        with pytest.raises(ValueError, match="parquet"):
            source.parquet("")

    def test_parquet_rejects_s3_url_in_phase_2(self):
        # S3 lands in Phase 3 — until then, reject loudly so users
        # don't silently pass an S3 URL and get a "no such file"
        # confusing error from the local file opener.
        with pytest.raises(ValueError, match="s3"):
            source.parquet("s3://bucket/key.parquet")
