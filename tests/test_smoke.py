"""Phase 0 smoke tests — proves the maturin build wired the Rust core
into the Python package and the version string round-trips."""

import ematix_probe


def test_version_attribute_exists():
    assert hasattr(ematix_probe, "__version__")


def test_version_matches_core():
    assert ematix_probe.__version__ == "0.1.0-dev"
