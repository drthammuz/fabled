#!/usr/bin/env python3
"""
Per-tile visual vs physical audit for hub / extraction / stairs region.

Mirrors editor G playtest (patched kenney_layout.json):
  - visual: placed GLB meshes after playtest cutouts, hatch pieces hidden
  - physical: colliding meshes after cutouts + floor-cell cuboids from mask

Usage:
    python tools/probe_hub_tile_audit.py
    python tools/probe_hub_tile_audit.py userinput/kenney_layout.json
"""
from __future__ import annotations

import json
import math
import sys
from pathlib import Path
from typing import Dict, List, Optional, Set, Tuple

sys.path.insert(0, str(Path(__file__).parent))
import probe_map_geometry as probe  # noqa: E402

CELL = 4.0
MOD_H = 4.5
FLOOR_TILE_HALF = CELL * 0.5 - 0.02
PIT_DROP_HALF = 1.25
HUB_FLOOR = -1
HUB_MODULE_SPAN = 20.0
ROOM_SHELLS = frozenset(
    {"room-large", "room-large-variation", "room-wide", "room-wide-variation"}
)
DEFAULT_LAYOUT = Path("userinput/kenney_layout.json")

import generate_kenney_catalog as kcat  # noqa: E402


def load_layout(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def hub_west_drop(ex: float, ez: float) -> Tuple[float, float]:
    # SW corner (ex-8, ez+8): off the centre row to the gate / stairs.
    return ex - 8.0, ez + 8.0


def hub_stairs_opening(ex: float, ez: float) -> Tuple[float, float]:
    wx = ex - HUB_MODULE_SPAN
    return wx + 4.0, ez


def hub_stairs_opening_cells(ex: float, ez: float):
    """Both hub-floor cells above the L2 stairs (1x2 piece, yaw +90deg): stair-top
    at wx+4 and the cell one step west at wx. Door threshold wx+8 stays solid."""
    wx = ex - HUB_MODULE_SPAN
    return [(wx + 4.0, ez), (wx, ez)]


def hub_l3_drop(ex: float, ez: float) -> Tuple[float, float]:
    """L3 pit drop on the hub floor — a separate tile north of the (ex,ez) landing."""
    return ex, ez - 8.0


def carves_floor(stem: str) -> bool:
    return (
        stem in ROOM_SHELLS
        or stem.startswith("corridor")
        or (stem.startswith("template-floor") and "hole" not in stem)
    )


def floor_prop_on_hole(stem: str, px: float, pz: float, masks, layout, floor: int) -> bool:
    """Suppress only *solid* floor tiles that overlap a hole. The template-floor-hole
    *frame* is intentional decoration: keep it rendered (its collider is skipped in
    skip_physics_collider so the hole stays open)."""
    if not stem.startswith("template-floor"):
        return False
    if "hole" in stem:
        return False
    return mask_open_at(masks, layout, px, pz, floor)


def hide_extraction_hatch(stem, floor, px, pz, masks, layout) -> bool:
    return floor_prop_on_hole(stem, px, pz, masks, layout, floor)


def skip_physics_collider(stem, floor, px, pz, ex, ez, masks, layout) -> bool:
    # Hole frame: the raised rim collides (open centre); never skip it. Mirrors
    # kenney_skip_piece_collider.
    if stem in ("template-floor-hole", "template-floor-layer-hole"):
        return False
    if floor_prop_on_hole(stem, px, pz, masks, layout, floor):
        return True
    if floor == HUB_FLOOR:
        if stem in ("gate", "gate-opening", "gate-lasers"):
            return True
        if stem.startswith("template-wall"):
            wall_x_west = ex - 10.0
            wall_x_east = ex - HUB_MODULE_SPAN + 10.0
            if abs(px - wall_x_west) < 1.6 and abs(pz - ez) < 2.05:
                return True
            if abs(px - wall_x_east) < 1.6 and abs(pz - ez) < 2.05:
                return True
    return False


def piece_collides(stem: str) -> bool:
    cat = probe.load_catalog().get(stem, {})
    return cat.get("collide_default", True)


def piece_grid_size(stem: str) -> Tuple[float, float]:
    cat = probe.load_catalog().get(stem, {})
    gu = cat.get("grid_units") or {}
    nx = float(gu.get("x", cat.get("footprint_m", {}).get("x", CELL) / CELL))
    nz = float(gu.get("z", cat.get("footprint_m", {}).get("z", CELL) / CELL))
    return nx, nz


def piece_overlaps_tile(px: float, pz: float, stem: str, yaw: float, tx: float, tz: float) -> bool:
    nx, nz = piece_grid_size(stem)
    steps = probe.yaw_steps(yaw)
    for _ in range(steps % 4):
        nx, nz = nz, nx
    half_w = nx * CELL * 0.5
    half_d = nz * CELL * 0.5
    tile = CELL * 0.5
    lx, lz = px - tx, pz - tz
    return (
        (lx - half_w) < tile
        and (lx + half_w) > -tile
        and (lz - half_d) < tile
        and (lz + half_d) > -tile
    )


def mesh_cutout_holes(stem, floor, px, pz, yaw, masks, layout) -> List[Tuple[float, float]]:
    """World XZ of every footprint cell whose mask cell is a hole (single source)."""
    if not carves_floor(stem):
        return []
    nx, nz = piece_grid_size(stem)
    steps = probe.yaw_steps(yaw)
    for _ in range(steps % 4):
        nx, nz = nz, nx
    sw_x = px - nx * CELL * 0.5
    sw_z = pz - nz * CELL * 0.5
    holes: List[Tuple[float, float]] = []
    for j in range(int(round(nz))):
        for i in range(int(round(nx))):
            cx = sw_x + (i + 0.5) * CELL
            cz = sw_z + (j + 0.5) * CELL
            if mask_open_at(masks, layout, cx, cz, floor):
                holes.append((cx, cz))
    return holes


def filter_open_hole_tris(tris: List[Tri], tx: float, tz: float, floor_plane_y: float) -> List[Tri]:
    out: List[Tri] = []
    half = FLOOR_TILE_HALF
    for tri in tris:
        cx = sum(v[0] for v in tri) / 3.0
        cy = max(v[1] for v in tri)
        cz = sum(v[2] for v in tri) / 3.0
        if (
            cy < floor_plane_y + 1.35
            and abs(cx - tx) <= half
            and abs(cz - tz) <= half
        ):
            continue
        out.append(tri)
    return out


def filter_shaft_tris(tris: List[Tri], ex: float, ez: float) -> List[Tri]:
    shaft_bottom = HUB_FLOOR * MOD_H - 0.35
    out: List[Tri] = []
    for tri in tris:
        cx = sum(v[0] for v in tri) / 3.0
        cy = min(v[1] for v in tri)
        cz = sum(v[2] for v in tri) / 3.0
        if (
            cy > shaft_bottom
            and abs(cx - ex) < FLOOR_TILE_HALF
            and abs(cz - ez) < FLOOR_TILE_HALF
        ):
            continue
        out.append(tri)
    return out


def apply_piece_cutouts(tris, stem, floor, px, pz, yaw, ex, ez, masks, layout):
    if not carves_floor(stem):
        return tris
    plane_y = floor * MOD_H
    out = tris
    for hole in mesh_cutout_holes(stem, floor, px, pz, yaw, masks, layout):
        out = filter_open_hole_tris(out, hole[0], hole[1], plane_y)
    if (
        stem in ROOM_SHELLS
        and floor == 0
        and mask_open_at(masks, layout, ex, ez, 0)
        and piece_overlaps_tile(px, pz, stem, yaw, ex, ez)
    ):
        out = filter_shaft_tris(out, ex, ez)
    return out


def build_world_tris(
    pieces: List[dict],
    floor: int,
    ex: float,
    ez: float,
    masks,
    layout,
    *,
    visual: bool,
) -> List[Tri]:
    out: List[Tri] = []
    for p in pieces:
        fl = int(p.get("floor", p.get("floor_level", 0)))
        if fl != floor:
            continue
        stem = p["stem"]
        px, pz = float(p["x"]), float(p["z"])
        yaw = float(p.get("yaw", 0.0))
        if visual and hide_extraction_hatch(stem, fl, px, pz, masks, layout):
            continue
        if not visual:
            if not piece_collides(stem):
                continue
            if skip_physics_collider(stem, fl, px, pz, ex, ez, masks, layout):
                continue
        local = probe.load_piece_tris(stem)
        if not local:
            continue
        steps = probe.yaw_steps(yaw)
        py = fl * MOD_H
        piece_tris: List[Tri] = []
        for tri in local:
            piece_tris.append(
                tuple(probe.transform_vertex(v, px, pz, py, steps) for v in tri)  # type: ignore[misc]
            )
        piece_tris = apply_piece_cutouts(piece_tris, stem, fl, px, pz, yaw, ex, ez, masks, layout)
        out.extend(piece_tris)
    return out


def floor_band(floor: int) -> Tuple[float, float]:
    y = floor * MOD_H
    return y + probe.FLOOR_BAND[0], y + probe.FLOOR_BAND[1]


def mesh_solid_at(cx: float, cz: float, tris: List[Tri], floor: int) -> bool:
    band = floor_band(floor)
    hits = kcat.vertical_ray_hits(cx, cz, tris)
    return any(band[0] <= h <= band[1] for h in hits)


def grid_dims(layout: dict) -> Tuple[int, int, float, float, float]:
    cell = float(layout.get("grid_unit_m", CELL))
    cx_cells = int(layout.get("modules_x", 3)) * 5
    cz_cells = int(layout.get("modules_z", 3)) * 5
    x0 = -(cx_cells * cell) / 2.0
    z0 = -(cz_cells * cell) / 2.0
    return cx_cells, cz_cells, x0, z0, cell


def coverage_mask(layout: dict, floor: int) -> List[bool]:
    """Sub-floor mask from floor-piece coverage (mirrors subfloor_mask_from_coverage)."""
    cx_cells, cz_cells, x0, z0, cell = grid_dims(layout)
    cells = [False] * (cx_cells * cz_cells)
    for p in layout.get("pieces", []):
        if int(p.get("floor", p.get("floor_level", 0))) != floor:
            continue
        if not carves_floor(p["stem"]):
            continue
        nx, nz = piece_grid_size(p["stem"])
        steps = probe.yaw_steps(float(p.get("yaw", 0.0)))
        for _ in range(steps % 4):
            nx, nz = nz, nx
        sw_x = float(p["x"]) - nx * cell * 0.5
        sw_z = float(p["z"]) - nz * cell * 0.5
        for j in range(int(round(nz))):
            for i in range(int(round(nx))):
                cxw = sw_x + (i + 0.5) * cell
                czw = sw_z + (j + 0.5) * cell
                ix = int((cxw - x0) // cell)
                iz = int((czw - z0) // cell)
                if 0 <= ix < cx_cells and 0 <= iz < cz_cells:
                    cells[iz * cx_cells + ix] = True
    return cells


def patched_floor_mask(layout: dict, ex: float, ez: float) -> Dict[int, Optional[List[bool]]]:
    cx_cells, cz_cells, x0, z0, cell = grid_dims(layout)
    floors = layout.get("floors", {})
    out: Dict[int, Optional[List[bool]]] = {}

    # Floor 0: solid ground (file mask if present, else filled).
    file0 = floors.get("0")
    if file0 and file0.get("cells"):
        out[0] = list(file0["cells"])
    else:
        out[0] = [True] * (cx_cells * cz_cells)
    # Sub-floors: derive from coverage (sub-floors aren't authored as masks).
    out[HUB_FLOOR] = coverage_mask(layout, HUB_FLOOR)
    out[-2] = coverage_mask(layout, -2)

    def make_set(fl: int):
        def set_at(wx: float, wz: float, on: bool) -> None:
            ix = int(round((wx - x0) / cell - 0.5))
            iz = int(round((wz - z0) / cell - 0.5))
            if 0 <= ix < cx_cells and 0 <= iz < cz_cells:
                out[fl][iz * cx_cells + ix] = on
        return set_at

    set0 = make_set(0)
    set0(ex, ez, False)  # extraction trap

    seth = make_set(HUB_FLOOR)
    west = hub_west_drop(ex, ez)
    stair_cells = hub_stairs_opening_cells(ex, ez)
    l3 = hub_l3_drop(ex, ez)
    seth(ex, ez, True)  # landing forced solid
    seth(west[0], west[1], False)
    for s in stair_cells:  # stairs span two cells
        seth(s[0], s[1], False)
    seth(l3[0], l3[1], False)

    setd = make_set(-2)
    for s in stair_cells:
        setd(s[0], s[1], False)
    return out


def mask_open_at(
    masks: Dict[int, Optional[List[bool]]],
    layout: dict,
    cx: float,
    cz: float,
    floor: int,
) -> bool:
    cells = masks.get(floor)
    if cells is None:
        return False
    cx_cells, cz_cells, x0, z0, cell = grid_dims(layout)
    ix = int(round((cx - x0) / cell - 0.5))
    iz = int(round((cz - z0) / cell - 0.5))
    if ix < 0 or iz < 0 or ix >= cx_cells or iz >= cz_cells:
        return True
    return not cells[iz * cx_cells + ix]


def uses_floor_cell_collider(stem: str, floor: int, ceiling: bool = False) -> bool:
    """Mask cuboid instead of trimesh for walkable template-floor tiles."""
    if stem != "template-floor":
        return False
    if ceiling:
        return False
    # floor 1+ template-floor is a roof slab (matches is_ceiling_slab).
    return floor <= 0


def mesh_covers_cell(layout: dict, cx: float, cz: float, floor: int, ex: float, ez: float, masks) -> bool:
    if floor < 0:
        # Below ground, any floor-bearing piece (room shell or template-floor tile) counts.
        for p in layout.get("pieces", []):
            fl = int(p.get("floor", p.get("floor_level", 0)))
            if fl != floor:
                continue
            stem = p["stem"]
            if skip_physics_collider(stem, fl, float(p["x"]), float(p["z"]), ex, ez, masks, layout):
                continue
            if uses_floor_cell_collider(stem, fl, bool(p.get("ceiling", False))):
                continue
            if not carves_floor(stem):
                continue
            if piece_contains_xz(p, cx, cz):
                return True
        return False
    for p in layout.get("pieces", []):
        fl = int(p.get("floor", p.get("floor_level", 0)))
        if fl != floor:
            continue
        stem = p["stem"]
        if skip_physics_collider(stem, fl, float(p["x"]), float(p["z"]), ex, ez, masks, layout):
            continue
        if uses_floor_cell_collider(stem, fl, bool(p.get("ceiling", False))):
            continue
        # Only walkable-surface pieces suppress the mask cuboid (not walls).
        if stem.startswith("corridor"):
            if piece_contains_xz(p, cx, cz):
                return True
            continue
        if carves_floor(stem) and piece_contains_xz(p, cx, cz):
            return True
    return False


def piece_contains_xz(p: dict, cx: float, cz: float) -> bool:
    stem = p["stem"]
    nx, nz = piece_grid_size(stem)
    yaw = float(p.get("yaw", 0.0))
    steps = probe.yaw_steps(yaw)
    for _ in range(steps % 4):
        nx, nz = nz, nx
    half_w = nx * CELL * 0.5
    half_d = nz * CELL * 0.5
    px, pz = float(p["x"]), float(p["z"])
    return abs(px - cx) <= half_w + 0.05 and abs(pz - cz) <= half_d + 0.05


def physical_solid_at(
    cx: float,
    cz: float,
    floor: int,
    layout: dict,
    phys_tris: List[Tri],
    masks: Dict[int, Optional[List[bool]]],
    ex: float,
    ez: float,
) -> bool:
    # Single source: a mask hole is never solid; otherwise a cuboid or room trimesh fills it.
    if mask_open_at(masks, layout, cx, cz, floor):
        return False
    if mesh_solid_at(cx, cz, phys_tris, floor):
        return True
    # Mask-solid cells with no room mesh get a floor-cell cuboid spawned by the server.
    return True


def audit_layout(path: Path) -> Tuple[List[str], List[str]]:
    layout = load_layout(path)
    ex_list = layout.get("extraction_xz")
    if not ex_list:
        return ["missing extraction_xz"], []
    ex, ez = float(ex_list[0]), float(ex_list[1])
    pieces = layout.get("pieces", [])
    masks = patched_floor_mask(layout, ex, ez)

    # Hub + west module band, sampled at true 4 m cell centres (boundaries are ambiguous).
    xs = list(range(-8, 29, 4))  # -8,-4,0,...,28 — west module through hub east rim
    zs = [12, 16, 20, 24]

    mismatches: List[str] = []
    rows: List[str] = []

    for floor in (0, HUB_FLOOR):
        vis_tris = build_world_tris(pieces, floor, ex, ez, masks, layout, visual=True)
        phys_tris = build_world_tris(pieces, floor, ex, ez, masks, layout, visual=False)
        for cx in xs:
            for cz in zs:
                vis = mesh_solid_at(cx, cz, vis_tris, floor)
                phys = physical_solid_at(cx, cz, floor, layout, phys_tris, masks, ex, ez)
                tag = "SOLID" if vis else "OPEN "
                ptag = "SOLID" if phys else "OPEN "
                match = vis == phys
                mark = "OK" if match else "MISMATCH"
                rows.append(
                    f"  F{floor:+2d} ({cx:5.1f},{cz:5.1f})  visual={tag}  physical={ptag}  {mark}"
                )

    # Authoritative expectations for the locked hub design.
    l3 = hub_l3_drop(ex, ez)
    west = hub_west_drop(ex, ez)
    stair_cells = hub_stairs_opening_cells(ex, ez)
    expectations = [
        ("F0 extraction trap", 0, ex, ez, False),
        ("F-1 hub landing (SOLID)", HUB_FLOOR, ex, ez, True),
        ("F-1 west drop (SW corner)", HUB_FLOOR, west[0], west[1], False),
        # The cell on the centre row to the gate must now be SOLID (path is clear).
        ("F-1 gate path (SOLID)", HUB_FLOOR, ex - 8.0, ez, True),
        ("F-1 stairs opening top", HUB_FLOOR, stair_cells[0][0], stair_cells[0][1], False),
        ("F-1 stairs opening west", HUB_FLOOR, stair_cells[1][0], stair_cells[1][1], False),
        ("F-1 L3 pit drop", HUB_FLOOR, l3[0], l3[1], False),
        ("F-1 door threshold", HUB_FLOOR, ex - HUB_MODULE_SPAN + 8.0, ez, True),
    ]
    for label, floor, cx, cz, want_solid in expectations:
        vis_tris = build_world_tris(pieces, floor, ex, ez, masks, layout, visual=True)
        phys_tris = build_world_tris(pieces, floor, ex, ez, masks, layout, visual=False)
        vis = mesh_solid_at(cx, cz, vis_tris, floor)
        phys = physical_solid_at(cx, cz, floor, layout, phys_tris, masks, ex, ez)
        ok = vis == want_solid and phys == want_solid
        rows.append(
            f"  EXPECT {label:22s} ({cx:5.1f},{cz:5.1f})  "
            f"want={'SOLID' if want_solid else 'OPEN '}  "
            f"vis={'SOLID' if vis else 'OPEN '}  phys={'SOLID' if phys else 'OPEN '}  "
            f"{'PASS' if ok else 'FAIL'}"
        )
        if not ok:
            mismatches.append(
                f"{label}: want {'solid' if want_solid else 'open'}, "
                f"got visual={'solid' if vis else 'open'} physical={'solid' if phys else 'open'}"
            )
        if vis != phys:
            mismatches.append(
                f"{label}: visual/physical disagree at ({cx},{cz})"
            )

    return mismatches, rows


def main() -> None:
    path = Path(sys.argv[1] if len(sys.argv) > 1 else DEFAULT_LAYOUT)
    print(f"Hub tile audit: {path}\n")
    mismatches, rows = audit_layout(path)
    print("=== tile grid (visual vs physical) ===")
    for row in rows:
        print(row)
    print()
    if mismatches:
        print(f"FAIL ({len(mismatches)} mismatch(es)):")
        for m in mismatches:
            print(f"  · {m}")
        sys.exit(1)
    print("PASS: visual and physical agree on all audited tiles")
    sys.exit(0)


if __name__ == "__main__":
    main()
