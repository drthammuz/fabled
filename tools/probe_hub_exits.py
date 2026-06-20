#!/usr/bin/env python3
"""
Probe hub branch exits (L2/L3/L4) on maps with branch_levels.

Checks:
  - branch_levels + layout export present
  - hub floor -1 west face has open gate frame on module boundary
  - hub floor pit + west-corridor floor cutouts (mesh walkable at drops)
  - L2/L3/L4 destination rooms have walkable floor mesh
  - hub uses distinct group_id from branch modules (not merged into GID_L2)

Usage:
    python tools/probe_hub_exits.py userinput/maps/level_stretch.json
"""
from __future__ import annotations

import json
import math
import sys
from pathlib import Path
from typing import Dict, List, Optional, Set, Tuple

sys.path.insert(0, str(Path(__file__).parent))
import gen_maps as gm  # noqa: E402
import probe_map_geometry as probe  # noqa: E402
import generate_kenney_catalog as kcat  # noqa: E402

LAYOUT_PATH = Path("userinput/kenney_layout.json")
MOD_H = 4.5
PIT_DROP_HALF = 2.05
GID_L2, GID_L3, GID_L4 = 92, 93, 94
HUB_EXIT_STEMS = frozenset({"gate", "gate-door"})


def load_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def pieces_for_gid(pieces: List[dict], gid: int) -> List[dict]:
    return [p for p in pieces if int(p.get("group_id", 0)) == gid]


def module_local_pieces(pieces: List[dict], mcx: float, mcz: float) -> List[gm.PlacedPiece]:
    out: List[gm.PlacedPiece] = []
    for p in pieces:
        if not gm.in_module_bounds(p, mcx, mcz):
            continue
        out.append(
            gm.pp(
                p["stem"],
                float(p["x"]) - mcx,
                float(p["z"]) - mcz,
                float(p.get("yaw", 0)),
            )
        )
        out[-1].floor_level = int(p.get("floor_level", 0))
    return out


def mesh_floor_at(x: float, z: float, tris: List, band_y: Tuple[float, float]) -> bool:
    hits = kcat.vertical_ray_hits(x, z, tris)
    return any(band_y[0] <= h <= band_y[1] for h in hits)


def floor_band(floor_level: int) -> Tuple[float, float]:
    y = floor_level * MOD_H
    return (y + kcat.FLOOR_BAND[0], y + kcat.FLOOR_BAND[1])


def hub_floor_cut_tris(tris: List, local_tx: float, local_tz: float) -> List:
    """Apply hub-floor pit cutout in module-local coordinates."""
    top_y = gm.HUB_FLOOR_LEVEL * MOD_H + 0.45
    return probe.filter_pit_floor_tris_at(tris, local_tx, local_tz, top_y)


def check_map(path: Path) -> List[str]:
    errors: List[str] = []
    doc = load_json(path)
    branches = doc.get("branch_levels") or {}
    if not branches:
        errors.append("map missing branch_levels")
        return errors

    ex = doc.get("extraction_xz")
    if not ex:
        errors.append("map missing extraction_xz")
        return errors
    exx, exz = float(ex[0]), float(ex[1])

    slot = gm.slot_from_world(exx, exz)
    mcx_hub, mcz_hub = gm.module_center(*slot)
    wc, wr = gm.neighbor_slot(*slot, "W")
    mcx_w, mcz_w = gm.module_center(wc, wr)

    pieces = doc.get("pieces", [])

    # --- group ids ---
    hub_floor_rooms = [
        p for p in pieces
        if p.get("stem") == gm.HUB_ROOM_STEM
        and int(p.get("floor_level", 0)) == gm.HUB_FLOOR_LEVEL
        and abs(float(p["x"]) - mcx_hub) < 0.1
        and abs(float(p["z"]) - mcz_hub) < 0.1
    ]
    if not hub_floor_rooms:
        errors.append("hub missing room-large at floor -1 on extraction module")
    else:
        hub_gid = int(hub_floor_rooms[0].get("group_id", 0))
        if hub_gid == GID_L2:
            errors.append(
                f"hub floor -1 uses group_id {hub_gid} (same as L2) — hub will be culled with branches"
            )
        if hub_gid in (GID_L3, GID_L4):
            errors.append(f"hub floor -1 uses branch group_id {hub_gid}")

    for label, gid in [("L2", GID_L2), ("L3", GID_L3), ("L4", GID_L4)]:
        if not pieces_for_gid(pieces, gid):
            errors.append(f"missing pieces with group_id {gid} ({label})")

    # --- hub west opening (open gate frame on module boundary) ---
    west_gate_x = mcx_hub - 10.0
    west_gates = [
        p for p in pieces
        if p.get("stem") in HUB_EXIT_STEMS
        and int(p.get("floor_level", 0)) == gm.HUB_FLOOR_LEVEL
        and abs(float(p["x"]) - west_gate_x) < 0.1
        and abs(float(p["z"]) - mcz_hub) < 0.1
    ]
    if not west_gates:
        errors.append(
            f"hub floor missing open gate west exit at ({west_gate_x}, {mcz_hub})"
        )
    if len(west_gates) > 1:
        errors.append(
            f"hub west exit has {len(west_gates)} stacked gates — use a single open frame"
        )

    # --- no closed hatch pieces blocking hub drops ---
    hub_holes = [
        p for p in pieces
        if p.get("stem") == gm.EXTRACTION_HOLE_STEM
        and int(p.get("floor_level", 0)) == gm.HUB_FLOOR_LEVEL
    ]
    if hub_holes:
        errors.append(
            f"hub floor has {len(hub_holes)} template-floor-hole piece(s) — "
            "use mesh cutouts for open drops instead"
        )

    # --- stairs in L2 module east antechamber (floor -2) ---
    stairs_x = mcx_w + 6.0
    depth_floor = gm.DEPTH_FLOOR_LEVEL
    stairs = [
        p for p in pieces
        if p.get("stem") == "stairs"
        and int(p.get("floor_level", p.get("floor", 0))) == depth_floor
        and abs(float(p["x"]) - stairs_x) < 0.1
        and abs(float(p.get("z", 0)) - mcz_w) < 0.1
    ]
    if not stairs:
        errors.append(
            f"L2 module missing stairs at east antechamber ({stairs_x}, {mcz_w}) floor {depth_floor}"
        )
    elif abs(float(stairs[0].get("yaw", 0)) - gm.PI2) > 0.05:
        errors.append(
            f"L2 stairs yaw {stairs[0].get('yaw')} — expected PI/2 ({gm.PI2})"
        )

    west_local = module_local_pieces(
        [
            p for p in pieces
            if int(p.get("floor_level", p.get("floor", 0))) == gm.HUB_FLOOR_LEVEL
            and gm.in_module_bounds(p, mcx_w, mcz_w)
            and p.get("stem") == gm.HUB_ROOM_STEM
        ],
        mcx_w,
        mcz_w,
    )
    west_tris = probe.build_module_tris(west_local)
    hub_band = floor_band(gm.HUB_FLOOR_LEVEL)
    top_y = gm.HUB_FLOOR_LEVEL * MOD_H + 0.45
    # Stairs shaft opening in module-local coords (east antechamber tile).
    west_tris = probe.filter_pit_floor_tris_at(west_tris, 6.0, 0.0, top_y)
    if mesh_floor_at(6.0, 0.0, west_tris, hub_band):
        errors.append(
            f"hub floor still solid above L2 stairs ({stairs_x}, {mcz_w}) — cannot walk down"
        )
    # Door threshold should stay walkable after east-link wall cutout.
    if not mesh_floor_at(8.0, 0.0, west_tris, hub_band):
        errors.append(
            f"hub floor missing at west-module east door threshold ({mcx_w + 8}, {mcz_w})"
        )

    hub_local = module_local_pieces(
        [
            p for p in pieces
            if int(p.get("floor_level", 0)) == gm.HUB_FLOOR_LEVEL
            and gm.in_module_bounds(p, mcx_hub, mcz_hub)
        ],
        mcx_hub,
        mcz_hub,
    )

    # --- mesh walkability at destinations ---
    for key, spec in branches.items():
        fl = int(spec["floor"])
        bx, bz = float(spec["x"]), float(spec["z"])
        band = floor_band(fl)
        local = module_local_pieces(
            [p for p in pieces if int(p.get("floor_level", 0)) == fl],
            bx,
            bz,
        )
        tris = probe.build_module_tris(local)
        if not mesh_floor_at(0.0, 0.0, tris, band):
            errors.append(f"L{key} destination ({bx}, {bz}) floor {fl}: no walkable mesh at centre")

    # --- L2/L3 finished content (props beyond bare shell) ---
    for label, gid in [("L2", GID_L2), ("L3", GID_L3)]:
        gid_pieces = pieces_for_gid(pieces, gid)
        shells = [p for p in gid_pieces if p.get("stem") == gm.HUB_ROOM_STEM]
        props = [p for p in gid_pieces if p.get("stem") not in (gm.HUB_ROOM_STEM, "template-wall")]
        if not shells:
            errors.append(f"L{label} missing branch room shell")
        if len(props) < 2:
            errors.append(f"L{label} needs dressing props (found {len(props)})")

    # --- hub corridor + open pit drops (mesh cutouts) ---
    hub_tris = hub_floor_cut_tris(probe.build_module_tris(hub_local), 0.0, 0.0)
    hub_tris = probe.filter_pit_floor_tris_at(
        hub_tris, -8.0, 0.0, gm.HUB_FLOOR_LEVEL * MOD_H + 0.45,
    )
    hub_band = floor_band(gm.HUB_FLOOR_LEVEL)
    mid_x = mcx_hub - 8.0
    if not mesh_floor_at(3.0, 0.0, hub_tris, hub_band):
        errors.append(
            f"hub floor rim not walkable near pit ({exx + 3}, {exz}) after cutout"
        )
    mid_x = mcx_hub - 8.0
    if not mesh_floor_at(-4.0, 0.0, hub_tris, hub_band):
        errors.append(
            f"no hub-floor mesh on west corridor ({mid_x + 4}, {mcz_hub}) — cannot reach exits"
        )
    # Pit shaft + west corridor columns should have no floor after cutout.
    if mesh_floor_at(0.0, 0.0, hub_tris, hub_band):
        errors.append(
            f"hub floor still solid at pit centre ({exx}, {exz}) — L3 drop blocked"
        )
    if mesh_floor_at(mid_x - mcx_hub, 0.0, hub_tris, hub_band):
        errors.append(
            f"hub floor still solid at west corridor ({mid_x}, {mcz_hub}) — L4 drop blocked"
        )

    return errors


def check_layout(map_path: Path) -> List[str]:
    errors: List[str] = []
    if not LAYOUT_PATH.exists():
        errors.append(f"missing {LAYOUT_PATH}")
        return errors
    layout = load_json(LAYOUT_PATH)
    map_doc = load_json(map_path)
    if not layout.get("branch_levels"):
        errors.append("kenney_layout.json missing branch_levels export")
    if layout.get("branch_levels") != map_doc.get("branch_levels"):
        errors.append("branch_levels mismatch between map and kenney_layout.json")
    return errors


def main() -> None:
    path = Path(sys.argv[1] if len(sys.argv) > 1 else "userinput/maps/level_stretch.json")
    print(f"Probing hub exits: {path.name}\n")

    map_err = check_map(path)
    layout_err = check_layout(path)

    print("=== hub branch exits (map JSON) ===")
    if map_err:
        print(f"FAIL ({len(map_err)}):")
        for e in map_err:
            print(f"  · {e}")
    else:
        print("PASS: open west gate, L2/L3/L4 geometry, hub floor drops")

    print("\n=== runtime layout ===")
    if layout_err:
        print(f"FAIL ({len(layout_err)}):")
        for e in layout_err:
            print(f"  · {e}")
    else:
        print("PASS: branch_levels exported to kenney_layout.json")

    sys.exit(1 if map_err or layout_err else 0)


if __name__ == "__main__":
    main()
