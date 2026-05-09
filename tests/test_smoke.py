"""Phase 0 smoke tests — proves the maturin build wired the Rust core
into the Python package and the version string round-trips."""

import ematix_probe


def test_version_attribute_exists():
    assert hasattr(ematix_probe, "__version__")


def test_version_matches_core():
    # Version is sourced from Cargo at build time. Round-trip
    # check: it's non-empty and looks like a sane SemVer-ish
    # string. Hard-coding the literal here would break every
    # release commit's CI.
    v = ematix_probe.__version__
    assert isinstance(v, str) and v
    assert v[0].isdigit(), f"version should start with a digit; got {v!r}"
