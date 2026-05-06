"""Duration string parsing for table-level assertions.

v0.1 surface (per sprint-03 risk #2): a non-negative integer
followed by a single unit character.

    s → seconds
    m → minutes
    h → hours
    d → days

Examples: ``"30s"``, ``"30m"``, ``"24h"``, ``"7d"``.

Compound forms like ``"6h30m"`` are *not* supported in v0.1; they
land in Phase 1c if real probes need them.
"""

from __future__ import annotations

import re

_DURATION_RE = re.compile(r"^\s*(\d+)\s*([smhd])\s*$")

_UNIT_SECONDS = {
    "s": 1,
    "m": 60,
    "h": 60 * 60,
    "d": 24 * 60 * 60,
}


def parse_duration(spec: str) -> int:
    """Parse a duration string into a non-negative integer count of
    seconds. Raises ``ValueError`` on any input outside the v0.1
    surface."""
    if not isinstance(spec, str):
        raise ValueError(
            f"duration must be a string like '24h', got {type(spec).__name__}"
        )
    match = _DURATION_RE.match(spec)
    if match is None:
        raise ValueError(
            f"invalid duration {spec!r}; expected '<int><unit>' "
            "with unit in {'s','m','h','d'} (e.g. '24h', '30m')"
        )
    n = int(match.group(1))
    unit = match.group(2)
    return n * _UNIT_SECONDS[unit]
