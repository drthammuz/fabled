#!/usr/bin/env python3
"""
Place 2–4 known modules, print per-tile boundary diagrams, audit honestly,
write a small map JSON for visual check in the editor.

Usage:
    python tools/debug_map_placement.py
    python tools/debug_map_placement.py --layout ew   # 2 modules east-west
    python tools/debug_map_placement.py --layout four   # 2x2 space_rooms grid
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Dict, List, Optional, Set, Tuple

sys.path.insert(0, str(Path(__file__).parent))
import gen_maps as gm  # noqa: E402
import probe_map_geometry as probe  # noqa: E402

MAP_OUT = Path("userinput/maps/debug_placement.json")
SPACE = Path("userinput/modules/space_rooms")

Slot = Tuple[int, int]


def face_grid(opens: Dict[str, Set[int]], side: str) -> str:
    tiles = [str(i) if i in opens.get(side, set()) else "." for i in range(5)]
    return "".join(tiles)


def print_module_faces(slot: Slot, name: str, pieces: List[gm.PlacedPiece]) -> None:
    model = gm.boundary_openings(pieces)
    honest = gm.honest_boundary_openings(pieces)
    print(f"\n=== slot {slot}  {name} ===")
    print("  model N:", face_grid(model, "N"), "  honest:", face_grid(honest, "N"))
    print("  model S:", face_grid(model, "S"), "  honest:", face_grid(honest, "S"))
    print("  model W:", face_grid(model, "W"), "  honest:", face_grid(honest, "W"))
    print("  model E:", face_grid(model, "E"), "  honest:", face_grid(honest, "E"))
    if model != honest:
        print("  ** model vs honest MISMATCH — room GLB wider than centre tile **")


def build_two_ew() -> Tuple[Dict[Slot, List[gm.PlacedPiece]], Dict[Slot, Set[str]]]:
    """m01 (N,S,E) west  +  m05 hub east.  Centre-tile E<->W connection."""
    m01 = json.loads((SPACE / "m01_nse_junction.json").read_text())
    m05 = json.loads((SPACE / "m05_nsew_hub.json").read_text())
    p01 = gm.pieces_from_json(m01["pieces"])
    p05 = gm.pieces_from_json(m05["pieces"])
    # Close sides not needed for this 2-module strip.
    p01 = probe.close_for_placement_mesh(p01, {"E"}, {"E": gm.CENTER_TILE})
    p05 = probe.close_for_placement_mesh(p05, {"W"}, {"W": gm.CENTER_TILE})
    design = {
        (0, 0): {"E"},
        (1, 0): {"W"},
    }
    return {(0, 0): p01, (1, 0): p05}, design


def build_four_grid() -> Tuple[Dict[Slot, List[gm.PlacedPiece]], Dict[Slot, Set[str]]]:
    """2x2 from space_rooms corner pieces (hand-verified in gen_space_rooms.py)."""
    specs = {
        (0, 0): "m09_se_corner.json",   # S+E
        (1, 0): "m02_sew_junction.json",  # S+E+W
        (0, 1): "m06_nw_corner.json",   # N+W
        (1, 1): "m05_nsew_hub.json",    # all
    }
    placed: Dict[Slot, List[gm.PlacedPiece]] = {}
    design: Dict[Slot, Set[str]] = {
        (0, 0): {"E", "S"},
        (1, 0): {"W", "S"},
        (0, 1): {"E", "N"},
        (1, 1): {"W", "N"},
    }
    for slot, fname in specs.items():
        data = json.loads((SPACE / fname).read_text())
        req = design[slot]
        placed[slot] = probe.close_for_placement_mesh(
            gm.pieces_from_json(data["pieces"]), req,
            {s: gm.CENTER_TILE for s in req},
        )
    return placed, design


def module_exits_json(design: Dict[Slot, Set[str]]) -> Dict[str, Dict[str, List[int]]]:
    out: Dict[str, Dict[str, List[int]]] = {}
    for slot, sides in design.items():
        exp = probe.expected_border_openings(sides)
        out[f"{slot[0]},{slot[1]}"] = {s: sorted(tiles) for s, tiles in exp.items() if tiles}
    return out


def write_map(
    placed: Dict[Slot, List[gm.PlacedPiece]],
    spawn: Slot,
    out: Path,
    design: Optional[Dict[Slot, Set[str]]] = None,
) -> None:
    cells_total = 5 * gm.CELLS
    floor = [False] * (cells_total * cells_total)
    pieces_out: List[dict] = []
    gid = 1
    for (col, row), pieces in sorted(placed.items()):
        mcx, mcz = gm.module_center(col, row)
        for p in pieces:
            pieces_out.append({
                "stem": p.stem, "x": p.x + mcx, "z": p.z + mcz,
                "yaw": p.yaw, "floor_level": p.floor_level, "scale": p.scale,
                "group_id": gid,
            })
        for iz in range(gm.CELLS):
            for ix in range(gm.CELLS):
                mix = col * gm.CELLS + ix
                miz = row * gm.CELLS + iz
                floor[miz * cells_total + mix] = True
        gid += 1
    sc, sr = spawn
    cx, cz = gm.module_center(sc, sr)
    doc = {
        "version": 1,
        "name": out.stem,
        "modules_x": 5,
        "modules_z": 5,
        "floors": {"0": {"cells_x": cells_total, "cells_z": cells_total, "cells": floor}},
        "pieces": pieces_out,
        "spawn_xz": [cx, cz],
    }
    if design:
        doc["module_exits"] = module_exits_json(design)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(doc, indent=2), encoding="utf-8")


def audit_honest(
    placed: Dict[Slot, List[gm.PlacedPiece]],
    design: Dict[Slot, Set[str]],
) -> List[str]:
    errors: List[str] = []
    for slot, req in design.items():
        opens = gm.mesh_exits(placed[slot])
        for side in req:
            if gm.CENTER_TILE not in opens.get(side, set()):
                errors.append(f"{slot} missing centre {side} (open {opens.get(side)})")
        for side in gm.SIDES:
            if side in req:
                continue
            if opens.get(side):
                errors.append(f"{slot} catalog leak {side} tiles {opens[side]}")
    for slot, req in design.items():
        col, row = slot
        for side in req:
            nc, nr = gm.neighbor_slot(col, row, side)
            nslot = (nc, nr)
            if nslot not in placed:
                continue
            opp = gm.OPPOSITE[side]
            a = gm.mesh_exits(placed[slot]).get(side, set())
            b = gm.mesh_exits(placed[nslot]).get(opp, set())
            if a != b:
                errors.append(f"connection {slot}.{side} mismatch: {a} <-> {b}")
    return errors


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--layout", choices=["ew", "four"], default="ew")
    ap.add_argument("--out", default=str(MAP_OUT))
    args = ap.parse_args()

    if args.layout == "ew":
        placed, design = build_two_ew()
        names = {(0, 0): "m01", (1, 0): "m05"}
        spawn = (0, 0)
    else:
        placed, design = build_four_grid()
        names = {s: "?" for s in placed}
        spawn = (1, 1)

    print(f"Debug layout: {args.layout}  ({len(placed)} modules)")
    for slot, pieces in placed.items():
        print_module_faces(slot, names.get(slot, "?"), pieces)

    errors = audit_honest(placed, design)
    print(f"\n--- catalog audit: {len(errors)} issue(s) ---")
    for e in errors:
        print(f"  · {e}")

    out = Path(args.out)
    write_map(placed, spawn, out, design)
    print(f"\nWrote {out.resolve()}  spawn={spawn}")
    print("Open in editor: Map mode, load this file, check E<->W centre connection.")


if __name__ == "__main__":
    main()
