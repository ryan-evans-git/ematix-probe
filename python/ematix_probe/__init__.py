"""ematix-probe — declarative testing automation on a Rust core.

Phase 0 exposes only the version string. Probe decorators, fluent
builders, and the pytest plugin land in subsequent phases — see
docs/PRD.md and docs/PI_PLAN.md.
"""

from ematix_probe._core import __version__

__all__ = ["__version__"]
