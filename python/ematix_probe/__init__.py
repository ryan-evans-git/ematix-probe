"""ematix-probe — declarative testing automation on a Rust core.

Public surface (Phase 1a):
- `__version__` — package version
- `probe` — decorators (`probe.data`)
- `source` — source factories (`source.postgres(url)`)

Probe execution surface, pytest plugin, and ematix-flow integration
shim land in subsequent phases per docs/PI_PLAN.md.
"""

from ematix_probe import probe, source
from ematix_probe._core import __version__

__all__ = ["__version__", "probe", "source"]
