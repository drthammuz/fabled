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
    return ex - 8.0, ez


def hub_stairs_opening(ex: float, ez: float) -> Tuple[float, float]:
    wx = ex - HUB_MODULE_SPAN
    return wx + 6.0, ez


def in_hub_drop_column(x: float, z: float, tx: float, tz: float) -> bool:
    return abs(x - tx) <= FLOOR_TILE_HALF and abs(z - tz) <= FLOOR_TILE_HALF


def in_extraction_drop_zone(x: float, z: float, ex: float, ez: float) -> bool:
    return abs(x - ex) < PIT_DROP_HALF and abs(z - ez) < PIT_DROP_HALF


def hide_extraction_hatch(stem: str, floor: int, px: float, pz: float, ex: float, ez: float) -> bool:
    if floor == HUB_FLOOR and stem in ("template-floor-hole", "template-floor-layer-hole"):
        west = hub_west_drop(ex, ez)
        stair = hub_stairs_opening(ex, ez)
        if (
            in_extraction_drop_zone(px, pz, ex, ez)
            or in_hub_drop_column(px, pz, west[0], west[1])
            or in_hub_drop_column(px, pz, stair[0], stair[1])
        ):
            return True
    if floor == HUB_FLOOR and stem in (
        "template-floor",
        "template-floor-layer",
        "template-floor-big",
    ):
        west = hub_west_drop(ex, ez)
        stair = hub_stairs_opening(ex, ez)
        if (
            in_hub_drop_column(px, pz, ex, ez)
            or in_hub_drop_column(px, pz, west[0], west[1])
            or in_hub_drop_column(px, pz, stair[0], stair[1])
        ):
            return True
    return (
        stem in ("template-floor-hole", "template-floor-layer-hole")
        and floor == 0
        and abs(px - ex) < 0.5
        and abs(pz - ez) < 0.5
    )


def skip_physics_collider(stem: str, floor: int, px: float, pz: float, ex: float, ez: float) -> bool:
    if stem in ("template-floor-hole", "template-floor-layer-hole"):
        if floor < 0:
            return True
        if floor == 0 and abs(px - ex) < 0.5 and abs(pz - ez) < 0.5:
            return True
    if floor == HUB_FLOOR:
        if stem in ("gate", "gate-opening", "gate-lasers"):
            return True
        west = hub_west_drop(ex, ez)
        stair = hub_stairs_opening(ex, ez)
        if stem.startswith("template-wall"):
            wall_x_west = ex - 10.0
            wall_x_east = ex - HUB_MODULE_SPAN + 10.0
            if abs(px - wall_x_west) < 1.6 and abs(pz - ez) < 2.05:
                return True
            if abs(px - wall_x_east) < 1.6 and abs(pz - ez) < 2.05:
                return True
        if stem in ("template-floor", "template-floor-layer", "template-floor-big"):
            if (
                in_hub_drop_column(px, pz, ex, ez)
                or in_hub_drop_column(px, pz, west[0], west[1])
                or in_hub_drop_column(px, pz, stair[0], stair[1])
            ):
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


def mesh_cutout_holes(stem: str, floor: int, px: float, pz: float, yaw: float, ex: float, ez: float) -> List[Tuple[float, float]]:
    if stem not in ROOM_SHELLS:
        return []
    holes: List[Tuple[float, float]] = []
    if floor == 0 and piece_overlaps_tile(px, pz, stem, yaw, ex, ez):
        holes.append((ex, ez))
    if floor == HUB_FLOOR:
        if piece_overlaps_tile(px, pz, stem, yaw, ex, ez):
            holes.append((ex, ez))
        west = hub_west_drop(ex, ez)
        if piece_overlaps_tile(px, pz, stem, yaw, west[0], west[1]):
            holes.append(west)
        stair = hub_stairs_opening(ex, ez)
        if piece_overlaps_tile(px, pz, stem, yaw, stair[0], stair[1]):
            holes.append(stair)
    return holes[:3]


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


def apply_piece_cutouts(tris: List[Tri], stem: str, floor: int, px: float, pz: float, yaw: float, ex: float, ez: float) -> List[Tri]:
    if stem not in ROOM_SHELLS:
        return tris
    plane_y = floor * MOD_H
    out = tris
    for hole in mesh_cutout_holes(stem, floor, px, pz, yaw, ex, ez):
        out = filter_open_hole_tris(out, hole[0], hole[1], plane_y)
    if floor == 0 and piece_overlaps_tile(px, pz, stem, yaw, ex, ez):
        out = filter_shaft_tris(out, ex, ez)
    return out


def build_world_tris(
    pieces: List[dict],
    floor: int,
    ex: float,
    ez: float,
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
        if visual and hide_extraction_hatch(stem, fl, px, pz, ex, ez):
            continue
        if not visual:
            if not piece_collides(stem):
                continue
            if skip_physics_collider(stem, fl, px, pz, ex, ez):
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
        piece_tris = apply_piece_cutouts(piece_tris, stem, fl, px, pz, yaw, ex, ez)
        out.extend(piece_tris)
    return out


def floor_band(floor: int) -> Tuple[float, float]:
    y = floor * MOD_H
    return y + probe.FLOOR_BAND[0], y + probe.FLOOR_BAND[1]


def mesh_solid_at(cx: float, cz: float, tris: List[Tri], floor: int) -> bool:
    band = floor_band(floor)
    hits = kcat.vertical_ray_hits(cx, cz, tris)
    return any(band[0] <= h <= band[1] for h in hits)


def patched_floor_mask(layout: dict, ex: float, ez: float) -> Dict[int, Optional[List[bool]]]:
    floors = layout.get("floors", {})
    out: Dict[int, Optional[List[bool]]] = {}
    cell = float(layout.get("grid_unit_m", CELL))
    for key, mask in floors.items():
        fl = int(key)
        cells = list(mask.get("cells", []))
        cx_cells = int(mask.get("cells_x", 0))
        cz_cells = int(mask.get("cells_z", 0))
        if not cells:
            out[fl] = None
            continue
        x0 = -((layout.get("modules_x", 3) * 5 * cell) / 2.0)
        z0 = -((layout.get("modules_z", 3) * 5 * cell) / 2.0)

        def cut_at(wx: float, wz: float) -> None:
            ix = int(round((wx - x0) / cell - 0.5))
            iz = int(round((wz - z0) / cell - 0.5))
            if 0 <= ix < cx_cells and 0 <= iz < cz_cells:
                cells[iz * cx_cells + ix] = False

        if fl == 0:
            cut_at(ex, ez)
        if fl == HUB_FLOOR:
            west = hub_west_drop(ex, ez)
            stair = hub_stairs_opening(ex, ez)
            cut_at(ex, ez)
            cut_at(west[0], west[1])
            cut_at(stair[0], stair[1])
        out[fl] = cells
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
    cell = float(layout.get("grid_unit_m", CELL))
    cx_cells = int(layout["floors"][str(floor)]["cells_x"])
    cz_cells = int(layout["floors"][str(floor)]["cells_z"])
    x0 = -((layout.get("modules_x", 3) * 5 * cell) / 2.0)
    z0 = -((layout.get("modules_z", 3) * 5 * cell) / 2.0)
    ix = int(round((cx - x0) / cell - 0.5))
    iz = int(round((cz - z0) / cell - 0.5))
    if ix < 0 or iz < 0 or ix >= cx_cells or iz >= cz_cells:
        return True
    return not cells[iz * cx_cells + ix]


def mesh_covers_cell(layout: dict, cx: float, cz: float, floor: int, ex: float, ez: float) -> bool:
    if floor < 0:
        west = hub_west_drop(ex, ez)
        stair = hub_stairs_opening(ex, ez)
        if floor == HUB_FLOOR and (
            in_hub_drop_column(cx, cz, ex, ez)
            or in_hub_drop_column(cx, cz, west[0], west[1])
            or in_hub_drop_column(cx, cz, stair[0], stair[1])
        ):
            return True
        for p in layout.get("pieces", []):
            fl = int(p.get("floor", p.get("floor_level", 0)))
            if fl != floor:
                continue
            stem = p["stem"]
            if skip_physics_collider(stem, fl, float(p["x"]), float(p["z"]), ex, ez):
                continue
            if stem not in ROOM_SHELLS:
                continue
            if piece_contains_xz(p, cx, cz):
                return True
        return False
    for p in layout.get("pieces", []):
        fl = int(p.get("floor", p.get("floor_level", 0)))
        if fl != floor:
            continue
        if skip_physics_collider(
            p["stem"], fl, float(p["x"]), float(p["z"]), ex, ez
        ):
            continue
        if not piece_collides(p["stem"]):
            continue
        if piece_contains_xz(p, cx, cz):
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
    if mesh_solid_at(cx, cz, phys_tris, floor):
        return True
    if mask_open_at(masks, layout, cx, cz, floor):
        return False
    if floor == 0 and in_hub_drop_column(cx, cz, ex, ez):
        return False
    if floor == HUB_FLOOR:
        west = hub_west_drop(ex, ez)
        stair = hub_stairs_opening(ex, ez)
        if (
            in_hub_drop_column(cx, cz, ex, ez)
            or in_hub_drop_column(cx, cz, west[0], west[1])
            or in_hub_drop_column(cx, cz, stair[0], stair[1])
        ):
            return False
    if mesh_covers_cell(layout, cx, cz, floor, ex, ez):
        return False
    return True


def audit_layout(path: Path) -> Tuple[List[str], List[str]]:
    layout = load_layout(path)
    ex_list = layout.get("extraction_xz")
    if not ex_list:
        return ["missing extraction_xz"], []
    ex, ez = float(ex_list[0]), float(ex_list[1])
    pieces = layout.get("pieces", [])
    masks = patched_floor_mask(layout, ex, ez)

    # Hub + west module band (world tiles on 4 m grid centres).
    xs = list(range(2, 29, 4))  # 2,6,10,...,26 — covers west module through hub east rim
    zs = [16, 20, 24]

    mismatches: List[str] = []
    rows: List[str] = []

    for floor in (0, HUB_FLOOR):
        vis_tris = build_world_tris(pieces, floor, ex, ez, visual=True)
        phys_tris = build_world_tris(pieces, floor, ex, ez, visual=False)
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

    # Expected states for reported bug tiles (authoritative pass/fail).
    expectations = [
        ("F0 pit trap", 0, ex, ez, False),
        ("F-1 hub pit", HUB_FLOOR, ex, ez, False),
        ("F-1 west drop", HUB_FLOOR, ex - 8.0, ez, False),
        ("F-1 stairs opening", HUB_FLOOR, hub_stairs_opening(ex, ez)[0], ez, False),
        ("F-1 door threshold", HUB_FLOOR, ex - HUB_MODULE_SPAN + 8.0, ez, True),
        ("F-1 hub rim E", HUB_FLOOR, ex + 2.0, ez, True),
    ]
    for label, floor, cx, cz, want_solid in expectations:
        vis_tris = build_world_tris(pieces, floor, ex, ez, visual=True)
        phys_tris = build_world_tris(pieces, floor, ex, ez, visual=False)
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
