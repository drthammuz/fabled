#!/usr/bin/env python3
"""Normalize a synth GLB after mirror/edit for Bevy (external colormap, known-good material block).

Usage:
  python tools/repair_synth_glb.py assets/models/factions/synth/stairs-small-edge-r.glb
"""
from __future__ import annotations

import sys

from repair_priesthood_glb import repair

SYNTH = "assets/models/factions/synth"
DEFAULT_URI = "Textures/colormap.png"


def main() -> None:
    if len(sys.argv) < 2:
        print(__doc__)
        raise SystemExit(1)
    # Reuse priesthood repair logic — same Kenney external-colormap pattern.
    import repair_priesthood_glb as rp

    rp.PRIESTHOOD = SYNTH
    rp.TEMPLATE = f"{SYNTH}/floor.glb"
    rp.DEFAULT_URI = DEFAULT_URI
    for p in sys.argv[1:]:
        repair(p, uri=DEFAULT_URI)


if __name__ == "__main__":
    main()
