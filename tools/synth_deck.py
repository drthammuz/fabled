#!/usr/bin/env python3
"""Synth ``floor-*`` deck blocks — transition landing only (y=0, top @ 1.2 m).

See docs/synth-transition-architecture.md.  Not a global zone-wide floor replacer.
"""
from __future__ import annotations

from typing import Callable, List, Optional, Set, Tuple

import transition_entrances as te

Cell = Tuple[int, int]
DELTA = te.DELTA
CELL = te.CELL
KIT = "factions/synth"
SCALE = 4.0
DECK_RISE = 1.2
BARRIER_MIN_Y = 3.6

FLOOR_INTERIOR = "floor-panel"
FLOOR_EDGE = "floor-panel-straight"
FLOOR_END = "floor-panel-end"
FLOOR_CORNER = "floor-panel-corner"
FLOOR_CORNER_ALT = "floor-corner"
STRUCT_BARRIER = "structure-barrier"


def _exterior_sides(
    cell: Cell,
    walkable: Set[Cell],
    zone_lookup: Callable[[Cell], Optional[str]],
    zone: str,
    deck_cells: Set[Cell],
) -> List[str]:
    out: List[str] = []
    for side, (dx, dz) in DELTA.items():
        nb = (cell[0] + dx, cell[1] + dz)
        if nb not in walkable or nb not in deck_cells:
            if nb not in deck_cells:
                out.append(side)
            continue
        if zone_lookup(nb) != zone:
            out.append(side)
    return out


def pick_deck_stem(
    cell: Cell,
    walkable: Set[Cell],
    zone_lookup: Callable[[Cell], Optional[str]],
    zone: str,
    deck_cells: Set[Cell],
    *,
    deck_y: float = DECK_RISE,
) -> str:
    if deck_y >= BARRIER_MIN_Y - 0.01:
        return STRUCT_BARRIER
    return "floor"


def deck_piece(stem: str, x: float, z: float, zone: str) -> dict:
    return {
        "stem": stem,
        "x": x, "z": z, "yaw": 0.0,
        "y": 0.0,
        "floor_level": 0,
        "scale": SCALE,
        "kit": KIT,
        "zone": zone,
        "tags": ["synth_deck", "transition_deck"],
        "role": "deck",
    }


def strip_template_floors_at(
    pieces: List[dict],
    gx: int,
    gz: int,
    cells: Set[Cell],
) -> None:
    """Remove ground ``template-floor`` where transition deck GLBs replace them."""
    if not cells:
        return
    kept: List[dict] = []
    for p in pieces:
        if p.get("ceiling"):
            kept.append(p)
            continue
        stem = p.get("stem", "")
        if not stem.startswith("template-floor") or p.get("ceiling"):
            kept.append(p)
            continue
        ix = int(round((p["x"] / CELL) + gx / 2 - 0.5))
        iz = int(round((p["z"] / CELL) + gz / 2 - 0.5))
        if (ix, iz) in cells and int(p.get("floor_level", 0)) == 0:
            continue
        kept.append(p)
    pieces.clear()
    pieces.extend(kept)
