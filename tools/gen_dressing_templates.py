#!/usr/bin/env python3
"""Generate empty synth dressing room shells (floor + walls only)."""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "userinput" / "synth_dressing"
SCALE = 4.0
KIT = "factions/synth"


def p(stem: str, x: float, z: float, yaw: float = 0.0, y: float | None = None) -> dict:
    if y is None:
        y = 0.0 if stem.startswith("floor") else 1.2
    return {
        "stem": stem,
        "x": x,
        "z": z,
        "yaw": yaw,
        "floor_level": 0,
        "scale": SCALE,
        "kit": KIT,
        "y": y,
    }


def filled_mask() -> dict:
    return {"cells_x": 40, "cells_z": 40, "cells": [True] * (40 * 40)}


def wall_yaw(dx: int, dz: int) -> float:
    """Yaw for a wall on the outward face of a floor cell (matches editor + procgen)."""
    import synth_interior as si

    return si.wall_yaw(dx, dz)


def add_perimeter_walls(floors: set[tuple[float, float]], pieces: list[dict]) -> None:
    """Walls sit on cell faces — CELL/2 (2 m) from floor centre, not on adjacent cell centres."""
    for x, z in sorted(floors):
        for dx, dz in ((0, -1), (0, 1), (1, 0), (-1, 0)):
            nx, nz = x + dx * 4.0, z + dz * 4.0
            if (nx, nz) in floors:
                continue
            pieces.append(p("wall", x + dx * 2.0, z + dz * 2.0, wall_yaw(dx, dz)))


def rect_shell(cx_cells: list[float], cz_cells: list[float]) -> list[dict]:
    """Closed rectangle of floor cells with perimeter walls."""
    floors = {(x, z) for x in cx_cells for z in cz_cells}
    pieces: list[dict] = []
    for x, z in sorted(floors):
        pieces.append(p("floor", x, z))
    add_perimeter_walls(floors, pieces)
    return pieces


def l_shell(
    main_x: list[float], main_z: list[float], wing_x: list[float], wing_z: list[float]
) -> list[dict]:
    floors = {(x, z) for x in main_x for z in main_z}
    floors |= {(x, z) for x in wing_x for z in wing_z}
    pieces: list[dict] = []
    for x, z in sorted(floors):
        pieces.append(p("floor", x, z))
    add_perimeter_walls(floors, pieces)
    return pieces


def corridor_with_alcove() -> list[dict]:
    # 2-wide × 6-long corridor (N-S) + 3×3 alcove on the west side at mid span.
    cx = [-4.0, 0.0]
    cz = [-10.0, -6.0, -2.0, 2.0, 6.0, 10.0]
    alcove_x = [-12.0, -8.0, -4.0]
    alcove_z = [-2.0, 2.0, 6.0]
    return l_shell(cx, cz, alcove_x, alcove_z)


TEMPLATES = {
    "template_bunk": {
        "vignette": "bunk",
        "note": "3×4 m cells (12×16 m) — two-bed alcove room",
        "pieces": rect_shell([-4.0, 0.0, 4.0], [-6.0, -2.0, 2.0, 6.0]),
    },
    "template_mess": {
        "vignette": "mess",
        "note": "5×3 cells (20×12 m) open mess hall",
        "pieces": rect_shell([-8.0, -4.0, 0.0, 4.0, 8.0], [-4.0, 0.0, 4.0]),
    },
    "template_lab": {
        "vignette": "lab",
        "note": "4×4 cells (16×16 m) square lab",
        "pieces": rect_shell([-6.0, -2.0, 2.0, 6.0], [-6.0, -2.0, 2.0, 6.0]),
    },
    "template_office": {
        "vignette": "command",
        "note": "3×3 cells (12×12 m) command / office",
        "pieces": rect_shell([-4.0, 0.0, 4.0], [-4.0, 0.0, 4.0]),
    },
    "template_corridor": {
        "vignette": "corridor_alcove",
        "note": "N-S corridor with west alcove (3×3)",
        "pieces": corridor_with_alcove(),
    },
}


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for name, spec in TEMPLATES.items():
        cx, cz = 0.0, 0.0
        doc = {
            "version": 1,
            "name": name,
            "vignette": spec["vignette"],
            "floor_mask": filled_mask(),
            "pieces": spec["pieces"],
            "spawn_xz": [cx, cz],
            "spawn_y": 1.2,
        }
        path = OUT / f"{name}.json"
        path.write_text(json.dumps(doc, indent=2) + "\n", encoding="utf-8")
        print(f"wrote {path} ({len(spec['pieces'])} pieces) — {spec['note']}")


if __name__ == "__main__":
    main()
