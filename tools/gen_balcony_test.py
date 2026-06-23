#!/usr/bin/env python3
"""Balcony piece test vignette — one of every balcony/rail piece at yaw 0 (reference row)
plus a minimal room run through the real generator so the assembled result is visible.

Load in the dressing shell: File -> Load -> balcony_test. Use it to confirm each piece's
default orientation against assets/models/factions/synth/placement_catalog.json `orientation`.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "userinput" / "synth_dressing"
sys.path.insert(0, str(ROOT / "tools"))
import synth_interior as si  # noqa: E402
import audit_synth_scene as audit  # noqa: E402

CELL = si.CELL
REFERENCE_PIECES = [
    "balcony-floor",
    "balcony-floor-center",
    "balcony-floor-corner",
    "balcony-rail",
    "rail",
    "rail-narrow",
]


def base_floor(ix: int, iz: int) -> dict:
    x, z = si.ix_to_world((ix, iz))
    p = si.structure_piece("floor", x, z)
    p["y"] = 0.0
    p["tags"] = ["test_base"]
    return p


def reference_piece(stem: str, ix: int) -> dict:
    x, z = si.ix_to_world((ix, 0))
    return {
        "stem": stem,
        "x": x,
        "z": z,
        "yaw": 0.0,
        "floor_level": 0,
        "scale": si.SCALE,
        "kit": si.KIT,
        "y": si._balcony_y(stem),
        "role": "prop",
        "tags": ["balcony_reference", stem],
    }


def yaw_piece(stem: str, ix: int, iz: int, yaw: float, tag: str) -> dict:
    x, z = si.ix_to_world((ix, iz))
    return {
        "stem": stem,
        "x": x,
        "z": z,
        "yaw": yaw,
        "floor_level": 0,
        "scale": si.SCALE,
        "kit": si.KIT,
        "y": si._balcony_y(stem),
        "role": "prop",
        "tags": ["balcony_reference", tag, stem],
    }


def main() -> None:
    pieces: list[dict] = []
    import math
    import random

    # --- Reference row: one of each piece at yaw 0, each on its own base tile. ---
    for col, stem in enumerate(REFERENCE_PIECES):
        ix = col * 2
        pieces.append(base_floor(ix, 0))
        pieces.append(reference_piece(stem, ix))

    # --- Orientation rows: the SAME piece rotated 0/90/180/270 so the lip is visible. ---
    # balcony-floor-center (lip on +z): the raised edge should rotate around the tile.
    for col, yaw in enumerate([0.0, math.pi / 2, math.pi, 3 * math.pi / 2]):
        ix = col * 2
        pieces.append(base_floor(ix, -3))
        pieces.append(yaw_piece("balcony-floor-center", ix, -3, yaw, "yaw_center"))
    # balcony-floor-corner (lip wraps -x/+z): lip-corner should point to each diagonal.
    for col, yaw in enumerate([0.0, math.pi / 2, math.pi, 3 * math.pi / 2]):
        ix = col * 2
        pieces.append(base_floor(ix, -6))
        pieces.append(yaw_piece("balcony-floor-corner", ix, -6, yaw, "yaw_corner"))

    # --- Assembled L-shaped room (3x3 minus one cell) → 1 concave corner + convex corners.
    #     This is the inner-corner case where long rails used to cross.
    room = {(ix, iz) for ix in range(0, 3) for iz in range(5, 8)} - {(2, 7)}
    shell = si.build_shell(room, set(), {0: room})
    assembled = si.apply_perimeter_balconies(shell, room, set(), random.Random(1))
    pieces.extend(assembled)

    # Floor mask: cover everything generously.
    cells_x, cells_z = 40, 40
    doc = {
        "version": 1,
        "name": "balcony_test",
        "vignette": "balcony_test",
        "seed": 0,
        "note": (
            "z=0 row: one of each piece at yaw 0. z=-3 row: balcony-floor-center at "
            "0/90/180/270 (watch the raised lip rotate). z=-6 row: balcony-floor-corner "
            "at the 4 diagonals (lip-corner points outward). +z block: an L-shaped room "
            "(3x3 minus a cell) through the real generator — verify rails butt-join at the "
            "concave corner instead of crossing."
        ),
        "floor_mask": {"cells_x": cells_x, "cells_z": cells_z, "cells": [True] * (cells_x * cells_z)},
        "pieces": pieces,
        "spawn_xz": [0.0, 0.0],
        "spawn_y": 1.2,
    }

    OUT.mkdir(parents=True, exist_ok=True)
    path = OUT / "balcony_test.json"
    path.write_text(json.dumps(doc, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {path} ({len(pieces)} pieces)")

    flags = [f for f in audit.audit_doc(doc, "balcony_test") if "balcony" not in f.lower() or "STACKED" in f]
    if flags:
        print("AUDIT flags (non-balcony):")
        for f in flags[:20]:
            print(f"  - {f}")
    else:
        print("AUDIT OK balcony_test.json")


if __name__ == "__main__":
    main()
