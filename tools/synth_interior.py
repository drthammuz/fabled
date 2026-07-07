#!/usr/bin/env python3
"""Synth interior placement catalog + room-aware furnishing (editor + procgen)."""

from __future__ import annotations

import json
import math
import random
import sys
from dataclasses import dataclass, field
from pathlib import Path
from collections.abc import Callable
from typing import Iterable

WorldAt = Callable[[tuple[int, int]], tuple[float, float]]

ROOT = Path(__file__).resolve().parents[1]
CATALOG_PATH = ROOT / "assets" / "models" / "factions" / "synth" / "placement_catalog.json"
SCALE = 4.0
KIT = "factions/synth"
CELL = 4.0
HALF_PI = math.pi / 2
GAP_M = 0.35
PLACE_GAP = GAP_M * 2.0 + 0.1

CATALOG = json.loads(CATALOG_PATH.read_text(encoding="utf-8"))
if isinstance(CATALOG, dict) and "stems" in CATALOG and "wall_face_offset_m" not in CATALOG:
    # Compatibility with new probe_faction_catalog format (stems only + metadata).
    # These were top-level in the old synth-specific probe.
    CATALOG["wall_face_offset_m"] = 2.0
    CATALOG["wall_half_thickness_m"] = 0.6
    CATALOG["deck_y"] = CATALOG.get("deck_y", 1.2)
WALL_FACE = CATALOG.get("wall_face_offset_m", 2.0)
WALL_T = CATALOG.get("wall_half_thickness_m", 0.6)

CellIx = tuple[int, int]
CellW = tuple[float, float]


@dataclass(frozen=True)
class BBox:
    x0: float
    x1: float
    z0: float
    z1: float

    def padded(self, m: float) -> BBox:
        return BBox(self.x0 - m, self.x1 + m, self.z0 - m, self.z1 + m)

    def overlaps(self, other: BBox) -> bool:
        return self.x0 < other.x1 and self.x1 > other.x0 and self.z0 < other.z1 and self.z1 > other.z0


def bbox_penetrates(a: BBox, b: BBox, eps: float = 1e-3) -> bool:
    """True when ``a`` has interior overlap with ``b`` (touching faces do not count)."""
    return (
        a.x0 < b.x1 - eps
        and a.x1 > b.x0 + eps
        and a.z0 < b.z1 - eps
        and a.z1 > b.z0 + eps
    )


@dataclass
class RoomInfo:
    room_id: int
    cells_ix: set[CellIx] = field(default_factory=set)
    cells_w: set[CellW] = field(default_factory=set)
    role: str = "lab"
    area: int = 0
    corridor_mouths: int = 0
    centre_w: tuple[float, float] = (0.0, 0.0)


def stem_info(stem: str) -> dict:
    try:
        return CATALOG["stems"][stem]
    except KeyError as e:
        raise KeyError(f"unknown stem {stem!r} — run tools/probe_synth_catalog.py") from e


def bounds_scaled(stem: str, scale: float = SCALE) -> dict[str, float]:
    b = stem_info(stem)["bounds_scale1"]
    return {k: b[k] * scale for k in ("x0", "x1", "y0", "y1", "z0", "z1")}


def quantize_yaw(yaw: float) -> float:
    q = round(yaw / HALF_PI) * HALF_PI
    if abs(q) < 1e-4 or abs(q - 2 * math.pi) < 1e-4:
        return 0.0
    return q


def face_yaw(stem: str, dx: float, dz: float) -> float:
    if abs(dx) < 1e-4 and abs(dz) < 1e-4:
        return 0.0
    base = math.atan2(dx, dz)
    if stem.startswith("chair"):
        return quantize_yaw(base)
    front = stem_info(stem).get("front", "+z")
    if front == "-z":
        base += math.pi
    return quantize_yaw(base)


def look_at(stem: str, x: float, z: float, tx: float, tz: float) -> float:
    return face_yaw(stem, tx - x, tz - z)


def world_front(stem: str, yaw: float) -> tuple[float, float]:
    if stem.startswith("chair"):
        sign = 1.0
    else:
        sign = -1.0 if stem_info(stem).get("front") == "-z" else 1.0
    lx, lz = 0.0, sign
    wx = lx * math.cos(yaw) + lz * math.sin(yaw)
    wz = -lx * math.sin(yaw) + lz * math.cos(yaw)
    mag = math.hypot(wx, wz) or 1.0
    return wx / mag, wz / mag


def world_bbox(stem: str, x: float, z: float, yaw: float, scale: float = SCALE) -> BBox:
    b = bounds_scaled(stem, scale)
    corners = (
        (b["x0"], b["z0"]),
        (b["x0"], b["z1"]),
        (b["x1"], b["z0"]),
        (b["x1"], b["z1"]),
    )
    cos_y = math.cos(yaw)
    sin_y = math.sin(yaw)
    wx: list[float] = []
    wz: list[float] = []
    for lx, lz in corners:
        wx.append(x + lx * cos_y + lz * sin_y)
        wz.append(z + -lx * sin_y + lz * cos_y)
    return BBox(min(wx), max(wx), min(wz), max(wz))


def nudge_prop_to_room(
    p: dict, west: float, east: float, south: float, north: float
) -> dict:
    """Shift prop so its probed bbox stays inside room clear faces (+PLACE_GAP)."""
    bb = world_bbox(p["stem"], p["x"], p["z"], p["yaw"], p["scale"])
    dx = dz = 0.0
    x_lo = room_clear_face("west", west) + PLACE_GAP
    x_hi = room_clear_face("east", east) - PLACE_GAP
    z_lo = room_clear_face("south", south) + PLACE_GAP
    z_hi = room_clear_face("north", north) - PLACE_GAP
    if bb.x0 < x_lo:
        dx = x_lo - bb.x0
    elif bb.x1 > x_hi:
        dx = x_hi - bb.x1
    if bb.z0 < z_lo:
        dz = z_lo - bb.z0
    elif bb.z1 > z_hi:
        dz = z_hi - bb.z1
    if dx or dz:
        p = {**p, "x": round(p["x"] + dx, 4), "z": round(p["z"] + dz, 4)}
    return p


def wall_inner(face: str, floor_center: float) -> float:
    if face == "west":
        return floor_center - WALL_FACE
    if face == "east":
        return floor_center + WALL_FACE
    if face == "north":
        return floor_center + WALL_FACE
    return floor_center - WALL_FACE


def room_clear_face(wall: str, floor_center: float) -> float:
    line = wall_inner(wall, floor_center)
    if wall == "east":
        return line - WALL_T
    if wall == "west":
        return line + WALL_T
    if wall == "south":
        return line + WALL_T
    return line - WALL_T


def _world_corner_offsets(stem: str, yaw: float, scale: float = SCALE) -> list[tuple[float, float]]:
    b = bounds_scaled(stem, scale)
    cos_y = math.cos(yaw)
    sin_y = math.sin(yaw)
    out: list[tuple[float, float]] = []
    for lx in (b["x0"], b["x1"]):
        for lz in (b["z0"], b["z1"]):
            wx = lx * cos_y + lz * sin_y
            wz = -lx * sin_y + lz * cos_y
            out.append((wx, wz))
    return out


def prop(
    stem: str,
    x: float,
    z: float,
    *,
    yaw: float | None = None,
    look: tuple[float, float] | None = None,
    y: float | None = None,
    scale: float = SCALE,
    tags: list[str] | None = None,
    role: str = "prop",
) -> dict:
    if y is None:
        deck = CATALOG.get("deck_y", 1.2)
        y = 0.0 if stem_info(stem).get("deck_y") == "substrate_block" else deck
    if yaw is None:
        yaw = look_at(stem, x, z, look[0], look[1]) if look else 0.0
    piece = {
        "stem": stem,
        "x": round(x, 4),
        "z": round(z, 4),
        "yaw": yaw,
        "floor_level": 0,
        "scale": scale,
        "kit": KIT,
        "y": y,
        "role": role,
    }
    if tags:
        piece["tags"] = tags
    return piece


def structure_piece(stem: str, x: float, z: float, yaw: float = 0.0) -> dict:
    y = 0.0 if stem.startswith("floor") else CATALOG.get("deck_y", 1.2)
    role = "floor" if stem.startswith("floor") else "wall"
    return {
        "stem": stem,
        "x": x,
        "z": z,
        "yaw": yaw,
        "floor_level": 0,
        "scale": SCALE,
        "kit": KIT,
        "y": y,
        "role": role,
    }


def wall_yaw(dx: int, dz: int) -> float:
    """Yaw for a wall on the outward face of a floor cell (matches editor + procgen)."""
    if dz < 0:
        return 0.0
    if dz > 0:
        return math.pi
    if dx > 0:
        return HALF_PI
    return 3 * HALF_PI  # west = 270°, not 3/4 π


def bed_origin_at_wall(
    wall_face: str, floor_x: float, floor_z: float, yaw: float, stem: str = "bed-single"
) -> dict:
    anchor = stem_info(stem).get("back_anchor_local_m", 2.0)
    if wall_face == "west":
        pillow_x = wall_inner("west", floor_x)
        ox = pillow_x + anchor * math.sin(yaw)
        oz = floor_z + anchor * math.cos(yaw)
    elif wall_face == "east":
        pillow_x = wall_inner("east", floor_x)
        ox = pillow_x + anchor * math.sin(yaw)
        oz = floor_z + anchor * math.cos(yaw)
    elif wall_face == "south":
        pillow_z = wall_inner("south", floor_z)
        ox = floor_x + anchor * math.sin(yaw)
        oz = pillow_z + anchor * math.cos(yaw)
    else:
        pillow_z = wall_inner("north", floor_z)
        ox = floor_x + anchor * math.sin(yaw)
        oz = pillow_z + anchor * math.cos(yaw)
    return prop(stem, ox, oz, yaw=yaw)


def wall_solid_bbox(w: dict) -> BBox:
    """Solid volume of a wall piece (yaw-aware)."""
    stem = w["stem"]
    x, z, yaw, scale = w["x"], w["z"], w["yaw"], w.get("scale", SCALE)
    b = bounds_scaled(stem, scale)
    cos_y = math.cos(yaw)
    sin_y = math.sin(yaw)
    wx: list[float] = []
    wz: list[float] = []
    for lx in (b["x0"], b["x1"]):
        for lz in (b["z0"], b["z1"]):
            wx.append(x + lx * cos_y + lz * sin_y)
            wz.append(z + -lx * sin_y + lz * cos_y)
    return BBox(min(wx), max(wx), min(wz), max(wz))


def flush_back_to_wall(
    stem: str,
    wall: str,
    x: float,
    yaw: float,
    *,
    z: float = 0.0,
    scale: float = SCALE,
) -> dict:
    """Place so probed mesh back sits on the room-side clear face (inset by wall half-thickness)."""
    offsets = _world_corner_offsets(stem, yaw, scale)
    if wall == "south" and abs(yaw) < 0.01:
        target = room_clear_face("south", z)
        oz = target - min(wz for _, wz in offsets)
        return prop(stem, x, oz, yaw=yaw)
    if wall == "north" and abs(yaw - math.pi) < 0.01:
        target = room_clear_face("north", z)
        oz = target - max(wz for _, wz in offsets)
        return prop(stem, x, oz, yaw=yaw)
    if wall == "east" and abs(yaw + HALF_PI) < 0.01:
        target = room_clear_face("east", x)
        ox = target - max(wx for wx, _ in offsets)
        return prop(stem, ox, z, yaw=yaw)
    if wall == "west" and abs(yaw - HALF_PI) < 0.01:
        target = room_clear_face("west", x)
        ox = target - min(wx for wx, _ in offsets)
        return prop(stem, ox, z, yaw=yaw)
    raise ValueError(f"flush_back_to_wall unsupported: {stem} {wall} yaw={yaw}")


def get_desk_front_clearance(desk_stem: str) -> float:
    """Return explicit min safety offset using the probe's visual features + suggested_front_clearance_m.
    This ensures chairs are placed far enough from desks/computers/tables based on catalogued visual+physical data,
    preventing bbox overlaps that the fixed PLACE_GAP alone sometimes misses.
    """
    try:
        vis = stem_info(desk_stem).get("visual", {})
        suggested = float(vis.get("suggested_front_clearance_m", 0.0))
        # explicit minimum safety: never smaller than suggested, and at least the normal PLACE_GAP
        return max(PLACE_GAP, suggested + 0.05)
    except Exception:
        return PLACE_GAP


def chair_before_desk(chair_stem: str, desk: dict, desk_stem: str | None = None) -> dict:
    """Seat the chair in FRONT of the desk along its facing axis (not always north).

    Desk yaw is always axis-aligned (0, pi, +/-pi/2). A north-only assumption put the
    chair behind/beside desks on side walls — sometimes onto a reserved (stair) cell.

    Now incorporates catalog visual `suggested_front_clearance_m` for safety offset (fix for overlaps in sweeps).
    """
    desk_stem = desk_stem or desk["stem"]
    yaw = desk["yaw"]
    fx = round(math.sin(yaw))
    fz = round(math.cos(yaw))
    desk_bb = world_bbox(desk_stem, desk["x"], desk["z"], yaw, desk["scale"])
    clear_gap = get_desk_front_clearance(desk_stem)
    if fz != 0:  # desk faces north/south
        face_z = desk_bb.z1 if fz > 0 else desk_bb.z0
        chair_yaw = look_at(chair_stem, desk["x"], face_z + fz * clear_gap, desk["x"], desk["z"])
        offs = _world_corner_offsets(chair_stem, chair_yaw, desk["scale"])
        if fz > 0:
            cz = face_z + clear_gap - min(wz for _, wz in offs)
        else:
            cz = face_z - clear_gap - max(wz for _, wz in offs)
        return prop(chair_stem, desk["x"], cz, yaw=chair_yaw)
    # desk faces east/west
    face_x = desk_bb.x1 if fx > 0 else desk_bb.x0
    chair_yaw = look_at(chair_stem, face_x + fx * clear_gap, desk["z"], desk["x"], desk["z"])
    offs = _world_corner_offsets(chair_stem, chair_yaw, desk["scale"])
    if fx > 0:
        cx = face_x + clear_gap - min(wx for wx, _ in offs)
    else:
        cx = face_x - clear_gap - max(wx for wx, _ in offs)
    return prop(chair_stem, cx, desk["z"], yaw=chair_yaw)


def chairs_at_east_table(chair_stem: str, table: dict) -> list[dict]:
    tx, tz = table["x"], table["z"]
    scale = table["scale"]
    table_bb = world_bbox(table["stem"], tx, tz, table["yaw"], scale)
    z0, z1 = table_bb.z0, table_bb.z1

    clear_gap = get_desk_front_clearance(table.get("stem", "table"))
    north_z = z1 + 1.5
    yaw_n = look_at(chair_stem, tx, north_z, tx, tz)
    off_n = _world_corner_offsets(chair_stem, yaw_n, scale)
    north_z = z1 + clear_gap - min(wz for _, wz in off_n)

    south_z = z0 - 1.5
    yaw_s = look_at(chair_stem, tx, south_z, tx, tz)
    off_s = _world_corner_offsets(chair_stem, yaw_s, scale)
    south_z = z0 - clear_gap - max(wz for _, wz in off_s)

    return [
        prop(chair_stem, tx, north_z, yaw=yaw_n),
        prop(chair_stem, tx, south_z, yaw=yaw_s),
    ]


def cells_rect(ix0: int, iz0: int, ix1: int, iz1: int) -> set[CellIx]:
    return {(ix, iz) for ix in range(ix0, ix1 + 1) for iz in range(iz0, iz1 + 1)}


def ix_to_world(cell: CellIx) -> CellW:
    return cell[0] * CELL, cell[1] * CELL


def _cell_world(cell: CellIx, world_at: WorldAt | None = None) -> CellW:
    return world_at(cell) if world_at else ix_to_world(cell)


def analyze_synth_zone(
    walkable: set[CellIx],
    zone_lookup: Callable[[CellIx], str | None],
    deck_cells: set[CellIx],
    corridor_cells: set[CellIx],
    *,
    transition_cells: set[CellIx] | None = None,
    synth_zones: frozenset[str] = frozenset({"prev", "next"}),
) -> tuple[set[CellIx], set[CellIx], dict[int, set[CellIx]]]:
    """Derive floor grid, corridor mask, and room components inside a synth zone.

    Used by live procgen (``gen_freeform``) — dressing vignettes use hand-authored
    floor plans instead.
    """
    synth_cells = {c for c in walkable if zone_lookup(c) in synth_zones}
    if not synth_cells:
        return set(), set(), {}
    floor_ix = synth_cells
    transition = (transition_cells or set()) & synth_cells
    corridor_ix = (synth_cells & corridor_cells) | (deck_cells & synth_cells) | transition
    # One-cell buffer around transitions/deck — no beds or heavy props in the foyer band.
    for c in list(transition):
        ix, iz = c
        for dx, dz in ((0, -1), (0, 1), (1, 0), (-1, 0)):
            nb = (ix + dx, iz + dz)
            if nb in synth_cells:
                corridor_ix.add(nb)
    candidates = synth_cells - corridor_ix
    rooms: dict[int, set[CellIx]] = {}
    visited: set[CellIx] = set()
    rid = 0
    for start in sorted(candidates):
        if start in visited:
            continue
        stack = [start]
        component: set[CellIx] = set()
        while stack:
            c = stack.pop()
            if c in visited or c not in candidates:
                continue
            visited.add(c)
            component.add(c)
            for dx, dz in ((0, -1), (0, 1), (1, 0), (-1, 0)):
                stack.append((c[0] + dx, c[1] + dz))
        if component:
            rooms[rid] = component
            rid += 1
    # Corridor segments mis-parsed as rooms (1-wide hall, alcoves between corridor arms).
    for rid, cells in list(rooms.items()):
        mouths = count_corridor_mouths(cells, corridor_ix)
        if mouths >= 2 or (len(cells) == 1 and mouths >= 1):
            corridor_ix |= cells
            del rooms[rid]
    return floor_ix, corridor_ix, rooms


def _world_to_cell(x: float, z: float, gx: int, gz: int) -> CellIx:
    return (
        int(round(x / CELL + gx / 2 - 0.5)),
        int(round(z / CELL + gz / 2 - 0.5)),
    )


def _prop_anchor_cell(p: dict, cell_at: Callable[[float, float], CellIx] | None) -> CellIx | None:
    if cell_at is None:
        return None
    return cell_at(float(p["x"]), float(p["z"]))


def _bbox_footprint_cells(
    bb: BBox,
    cell_at: Callable[[float, float], CellIx],
    *,
    step: float = CELL / 2,
) -> set[CellIx]:
    """Grid samples covering a prop bbox (catches corridor spill from large pieces)."""
    cells: set[CellIx] = set()
    x = bb.x0
    while x <= bb.x1 + 1e-6:
        z = bb.z0
        while z <= bb.z1 + 1e-6:
            cells.add(cell_at(x, z))
            z += step
        x += step
    return cells


def _prop_allowed(
    p: dict,
    info: RoomInfo,
    corridor_ix: set[CellIx],
    transition_ix: set[CellIx],
    synth_zones: frozenset[str],
    zone_lookup: Callable[[CellIx], str | None],
    cell_at: Callable[[float, float], CellIx],
    blocked: set[CellIx] | None = None,
) -> bool:
    blocked = blocked or set()
    cell = cell_at(float(p["x"]), float(p["z"]))
    if cell not in info.cells_ix:
        return False
    if cell in corridor_ix or cell in transition_ix or cell in blocked:
        return False
    if zone_lookup(cell) not in synth_zones:
        return False
    if p["stem"].startswith(("bed-single", "bed-double")):
        if touches_transition(info.cells_ix, transition_ix):
            return False
    bb = world_bbox(p["stem"], p["x"], p["z"], p["yaw"], p.get("scale", SCALE))
    footprint = _bbox_footprint_cells(bb, cell_at)
    for fc in footprint:
        if fc in corridor_ix or fc in transition_ix or fc in blocked:
            return False
        if fc not in info.cells_ix:
            return False
        zc = zone_lookup(fc)
        if zc is not None and zc not in synth_zones:
            return False
    return True


def furnish_procgen_zone(
    pieces: list[dict],
    floor_ix: set[CellIx],
    corridor_ix: set[CellIx],
    room_infos: list[RoomInfo],
    rng: random.Random,
    *,
    world_at: WorldAt | None = None,
    zone: str | None = None,
    walkable: set[CellIx] | None = None,
    transition_ix: set[CellIx] | None = None,
    zone_lookup: Callable[[CellIx], str | None] | None = None,
    cell_at: Callable[[float, float], CellIx] | None = None,
    synth_zones: frozenset[str] = frozenset({"prev", "next"}),
) -> int:
    """Room-first props + perimeter balconies + command mezzanine for procgen maps."""
    trans = transition_ix or set()
    n = 0

    # Reserve the mezzanine footprint (deck + stair cell) so ground furniture never
    # lands on a cell that becomes elevated deck (props would float 1.2 m below it).
    command = next((i for i in room_infos if i.role == "command"), None)
    mezz_blocked: set[CellIx] = set()
    if command is not None:
        mplan = mezzanine_plan(command, corridor_ix)
        if mplan is not None:
            mezz_blocked = set(mplan["deck_cells"]) | {
                (mplan["stair_col"], mplan["stairs"][0][1])
            }
            # Ensure clear passage around the stair in the low area so players can reach the stairs
            # (addresses blocking in the only low row)
            srow = mplan.get("stair_row") or (mplan["stairs"][0][1] if mplan.get("stairs") else 0)
            scol = mplan["stair_col"]
            for dx in (-1, 0, 1):
                for dz in (-1, 0, 1):
                    nc = (scol + dx, srow + dz)
                    if nc in command.cells_ix:
                        mezz_blocked.add(nc)

    for info in room_infos:
        if info.role == "corridor":
            continue
        blocked = mezz_blocked if info.role == "command" else set()
        furnish_ix = info.cells_ix - blocked
        if not furnish_ix:
            continue
        cells_w = {_cell_world(c, world_at) for c in furnish_ix}
        props = furnish_room(
            info.role, cells_w, furnish_ix, corridor_ix, floor_ix, rng,
            zone_lookup=zone_lookup, transition_ix=trans, synth_zones=synth_zones,
            cell_at=cell_at,
        )
        for p in props:
            if cell_at and zone_lookup and not _prop_allowed(
                p, info, corridor_ix, trans, synth_zones, zone_lookup, cell_at,
                blocked=blocked,
            ):
                continue
            p["tags"] = ["synth_prop", "synth_interior", info.role, f"room_{info.room_id}"]
            p["role"] = "prop"
            if zone:
                p["zone"] = zone
            # Beds must sit ON the deck with an explicit Y so playtest sync cannot
            # collapse them to substrate height when multiple pieces share a cell column.
            if p["stem"].startswith(("bed-single", "bed-double")):
                p["y"] = round(DECK_Y, 4)
            else:
                p.pop("y", None)  # ``_apply_zone_elevation`` sets deck height
            pieces.append(p)
            n += 1

    # Mezzanine before balconies so elevated deck exists; balconies last + pruned to match rules.
    pieces[:] = add_command_mezzanine(
        pieces, room_infos, world_at=world_at, zone=zone, corridor_ix=corridor_ix,
    )
    pieces[:] = apply_perimeter_balconies(
        pieces, floor_ix, corridor_ix, rng, world_at=world_at, walkable=walkable, zone=zone,
        zone_lookup=zone_lookup, cell_at=cell_at, transition_cells=trans,
    )
    pieces[:] = prune_invalid_balconies(
        pieces, floor_ix, corridor_ix, world_at=world_at, walkable=walkable, zone_lookup=zone_lookup,
        cell_at=cell_at,
    )
    return n


def world_bounds(cells_w: Iterable[CellW]) -> tuple[float, float, float, float]:
    xs = [c[0] for c in cells_w]
    zs = [c[1] for c in cells_w]
    return min(xs), max(xs), min(zs), max(zs)


def count_corridor_mouths(room_ix: set[CellIx], corridor_ix: set[CellIx]) -> int:
    mouths = 0
    for ix, iz in room_ix:
        for dx, dz in ((0, -1), (0, 1), (1, 0), (-1, 0)):
            if (ix + dx, iz + dz) in corridor_ix:
                mouths += 1
    return mouths


def has_open_core(room_ix: set[CellIx], min_side: int = 3) -> bool:
    if len(room_ix) < min_side * min_side:
        return False
    ixs = [c[0] for c in room_ix]
    izs = [c[1] for c in room_ix]
    for ix in range(min(ixs) + 1, max(ixs)):
        for iz in range(min(izs) + 1, max(izs)):
            block = cells_rect(ix - 1, iz - 1, ix + 1, iz + 1)
            if block <= room_ix:
                return True
    return False


def room_dimensions(room_ix: set[CellIx]) -> tuple[int, int, int]:
    ixs = [c[0] for c in room_ix]
    izs = [c[1] for c in room_ix]
    w = max(ixs) - min(ixs) + 1
    h = max(izs) - min(izs) + 1
    return len(room_ix), w, h


def touches_transition(room_ix: set[CellIx], transition_ix: set[CellIx]) -> bool:
    if not transition_ix:
        return False
    for ix, iz in room_ix:
        for dx, dz in ((0, -1), (0, 1), (1, 0), (-1, 0)):
            if (ix + dx, iz + dz) in transition_ix:
                return True
    return False


def opens_to_default(
    cells_ix: set[CellIx],
    zone_lookup: Callable[[CellIx], str | None],
    synth_zones: frozenset[str] = frozenset({"prev", "next"}),
) -> set[str]:
    """Room faces that open onto non-synth walkable zones (industrial substrate seam)."""
    faces: set[str] = set()
    for ix, iz in cells_ix:
        if (ix, iz - 1) not in cells_ix:
            z = zone_lookup((ix, iz - 1))
            if z is not None and z not in synth_zones:
                faces.add("south")
        if (ix, iz + 1) not in cells_ix:
            z = zone_lookup((ix, iz + 1))
            if z is not None and z not in synth_zones:
                faces.add("north")
        if (ix - 1, iz) not in cells_ix:
            z = zone_lookup((ix - 1, iz))
            if z is not None and z not in synth_zones:
                faces.add("west")
        if (ix + 1, iz) not in cells_ix:
            z = zone_lookup((ix + 1, iz))
            if z is not None and z not in synth_zones:
                faces.add("east")
    return faces


def valid_quarters_room(
    room_ix: set[CellIx],
    corridor_ix: set[CellIx],
    transition_ix: set[CellIx],
) -> bool:
    """Beds only in real rooms: not corridor, not transition-adjacent, single mouth, min 2×2."""
    if room_ix & corridor_ix:
        return False
    if touches_transition(room_ix, transition_ix):
        return False
    if count_corridor_mouths(room_ix, corridor_ix) != 1:
        return False
    area, w, h = room_dimensions(room_ix)
    return area >= 4 and w >= 2 and h >= 2


def _pick_interior_wall(
    cells_w: set[CellW],
    cells_ix: set[CellIx],
    corridor_ix: set[CellIx],
    floor_ix: set[CellIx],
    zone_lookup: Callable[[CellIx], str | None] | None,
    synth_zones: frozenset[str],
    *,
    transition_ix: set[CellIx] | None = None,
    cell_at: Callable[[float, float], CellIx] | None = None,
    test_stem: str = "computer-system",
) -> str | None:
    """Wall face where a desk cluster fits entirely inside the room (bbox-checked)."""
    trans = transition_ix or set()
    corridor = opens_to_corridor(cells_ix, corridor_ix)
    for face in ("south", "west", "north", "east"):
        if face in corridor:
            continue
        test = _test_desk_on_wall(face, cells_w, cells_ix, floor_ix, test_stem)
        if test is None:
            continue
        if cell_at is None or zone_lookup is None:
            return face
        if _prop_fits_room(test, cells_ix, corridor_ix, trans, synth_zones, zone_lookup, cell_at):
            return face
    return None


def classify_room(
    room_ix: set[CellIx],
    corridor_ix: set[CellIx],
    transition_ix: set[CellIx] | None = None,
) -> str:
    area, w, h = room_dimensions(room_ix)
    mouths = count_corridor_mouths(room_ix, corridor_ix)
    trans = transition_ix or set()
    if touches_transition(room_ix, trans):
        if area <= 6:
            return "storage"
    if mouths <= 1 and area <= 4:
        return "storage"
    if valid_quarters_room(room_ix, corridor_ix, trans):
        return "quarters"
    if area >= 16 and w >= 4 and h >= 3 and has_open_core(room_ix):
        return "mess"
    return "lab"


def assign_roles(
    rooms: dict[int, set[CellIx]],
    corridor_ix: set[CellIx],
    transition_ix: set[CellIx] | None = None,
) -> dict[int, str]:
    roles: dict[int, str] = {}
    for rid, cells in rooms.items():
        roles[rid] = classify_room(cells, corridor_ix, transition_ix)
    if not rooms:
        return roles
    largest = max(rooms, key=lambda r: len(rooms[r]))
    roles[largest] = "command"
    return roles


def build_shell(floor_ix: set[CellIx], corridor_ix: set[CellIx], rooms: dict[int, set[CellIx]]) -> list[dict]:
    """Floor + walls. One wall piece per edge (deduped); room↔corridor stays open."""
    zone_of: dict[CellIx, str] = {c: "corridor" for c in corridor_ix}
    for rid, cells in rooms.items():
        for c in cells:
            zone_of[c] = f"room_{rid}"

    pieces: list[dict] = []
    for ix, iz in sorted(floor_ix):
        x, z = ix_to_world((ix, iz))
        pieces.append(structure_piece("floor", x, z))

    # Canonical undirected edge — emit at most one wall per shared face.
    walled_edges: set[tuple[CellIx, CellIx]] = set()

    for ix, iz in sorted(floor_ix):
        x, z = ix_to_world((ix, iz))
        zone_a = zone_of[(ix, iz)]
        for dx, dz in ((0, -1), (0, 1), (1, 0), (-1, 0)):
            nx, nz = ix + dx, iz + dz
            a, b = (ix, iz), (nx, nz)
            edge = (a, b) if a < b else (b, a)
            if edge in walled_edges:
                continue

            if (nx, nz) not in floor_ix:
                pieces.append(structure_piece("wall", x + dx * 2.0, z + dz * 2.0, wall_yaw(dx, dz)))
                walled_edges.add(edge)
                continue

            zone_b = zone_of[(nx, nz)]
            if zone_a == zone_b:
                continue
            if zone_a.startswith("room_") and zone_b.startswith("room_"):
                pieces.append(structure_piece("wall", x + dx * 2.0, z + dz * 2.0, wall_yaw(dx, dz)))
                walled_edges.add(edge)
    return pieces


def _room_span(cells_w: set[CellW]) -> tuple[float, float, float, float, float, float]:
    west, east, south, north = world_bounds(cells_w)
    mid_x = (west + east) / 2.0
    mid_z = (south + north) / 2.0
    return west, east, south, north, mid_x, mid_z


def _south_wall_xs(cells_w: set[CellW], south: float) -> list[float]:
    return sorted(x for x, z in cells_w if z == south)


def opens_to_corridor(cells_ix: set[CellIx], corridor_ix: set[CellIx]) -> set[str]:
    """Room faces that open onto corridor (not a solid wall)."""
    faces: set[str] = set()
    for ix, iz in cells_ix:
        if (ix, iz - 1) in corridor_ix:
            faces.add("south")
        if (ix, iz + 1) in corridor_ix:
            faces.add("north")
        if (ix - 1, iz) in corridor_ix:
            faces.add("west")
        if (ix + 1, iz) in corridor_ix:
            faces.add("east")
    return faces


def _room_span_from_ix(cells_ix: set[CellIx]) -> tuple[float, float, float, float, float, float]:
    cells_w = {ix_to_world(c) for c in cells_ix}
    return _room_span(cells_w)


def _south_desk_xs(
    cells_ix: set[CellIx], floor_ix: set[CellIx], stem: str
) -> list[float]:
    """South-row columns where ``stem`` fits — skips room-room partition edge cells."""
    west, east, south, _, _, _ = _room_span_from_ix(cells_ix)
    south_iz = min(iz for _, iz in cells_ix)
    lo = room_clear_face("west", west) + PLACE_GAP
    hi = room_clear_face("east", east) - PLACE_GAP
    ok: list[float] = []
    for ix, iz in sorted(cells_ix):
        if iz != south_iz:
            continue
        nb_w, nb_e = (ix - 1, iz), (ix + 1, iz)
        if nb_w not in cells_ix and nb_w in floor_ix:
            continue
        if nb_e not in cells_ix and nb_e in floor_ix:
            continue
        x = ix * CELL
        desk = flush_back_to_wall(stem, "south", x, 0.0, z=south)
        bb = world_bbox(stem, desk["x"], desk["z"], 0.0)
        if bb.x0 >= lo and bb.x1 <= hi:
            ok.append(x)
    return ok


def _lateral_clear_xs(cells_w: set[CellW], south: float, stem: str) -> list[float]:
    """Deprecated wrapper — prefer ``_south_desk_xs`` with ``cells_ix``."""
    cells_ix = {(int(round(x / CELL)), int(round(z / CELL))) for x, z in cells_w}
    floor_ix = cells_ix
    return _south_desk_xs(cells_ix, floor_ix, stem)


def _prop_fits_room(
    p: dict,
    cells_ix: set[CellIx],
    corridor_ix: set[CellIx],
    transition_ix: set[CellIx],
    synth_zones: frozenset[str],
    zone_lookup: Callable[[CellIx], str | None],
    cell_at: Callable[[float, float], CellIx],
) -> bool:
    """True when a prop anchor + probed bbox stay inside this synth room."""
    cell = cell_at(float(p["x"]), float(p["z"]))
    if cell not in cells_ix or cell in corridor_ix or cell in transition_ix:
        return False
    if zone_lookup(cell) not in synth_zones:
        return False
    bb = world_bbox(p["stem"], p["x"], p["z"], p["yaw"], p.get("scale", SCALE))
    for fc in _bbox_footprint_cells(bb, cell_at):
        if fc in corridor_ix or fc in transition_ix or fc not in cells_ix:
            return False
        zc = zone_lookup(fc)
        if zc is not None and zc not in synth_zones:
            return False
    return True


def _bed_fits_room(
    p: dict,
    cells_ix: set[CellIx],
    corridor_ix: set[CellIx],
    transition_ix: set[CellIx],
    synth_zones: frozenset[str],
    zone_lookup: Callable[[CellIx], str | None],
    cell_at: Callable[[float, float], CellIx],
) -> bool:
    return _prop_fits_room(
        p, cells_ix, corridor_ix, transition_ix, synth_zones, zone_lookup, cell_at
    )


def _test_desk_on_wall(
    wall: str,
    cells_w: set[CellW],
    cells_ix: set[CellIx],
    floor_ix: set[CellIx],
    stem: str = "computer-system",
) -> dict | None:
    west, east, south, north, mid_x, mid_z = _room_span(cells_w)
    try:
        if wall == "south":
            xs = _south_desk_xs(cells_ix, floor_ix, stem)
            if not xs:
                return None
            return flush_back_to_wall(stem, "south", xs[len(xs) // 2], 0.0, z=south)
        yaw = {"west": HALF_PI, "east": -HALF_PI, "north": math.pi, "south": 0.0}[wall]
        anchor = {"west": west, "east": east, "north": north, "south": south}[wall]
        x = anchor if wall in ("west", "east") else mid_x
        z = mid_z if wall in ("west", "east") else south
        return flush_back_to_wall(stem, wall, x, yaw, z=z)
    except ValueError:
        return None


def setup_quarters(
    cells_w: set[CellW],
    cells_ix: set[CellIx],
    corridor_ix: set[CellIx],
    rng: random.Random,
    *,
    zone_lookup: Callable[[CellIx], str | None] | None = None,
    transition_ix: set[CellIx] | None = None,
    synth_zones: frozenset[str] = frozenset({"prev", "next"}),
    cell_at: Callable[[float, float], CellIx] | None = None,
) -> list[dict]:
    """Bunk-verified bed pattern — matches ``bunk_furnished_c`` when the room has an interior corner."""
    trans = transition_ix or set()
    if not valid_quarters_room(cells_ix, corridor_ix, trans):
        return []
    west, east, south, north, _, _ = _room_span(cells_w)
    area = len(cells_w)
    corridor = opens_to_corridor(cells_ix, corridor_ix)
    default = opens_to_default(cells_ix, zone_lookup, synth_zones) if zone_lookup else set()
    west_bed_z = south + CELL
    out: list[dict] = []

    def keep(bed: dict) -> bool:
        if cell_at is None or zone_lookup is None:
            return True
        return _bed_fits_room(bed, cells_ix, corridor_ix, trans, synth_zones, zone_lookup, cell_at)

    bunk: list[dict] = []
    if "south" not in corridor and "south" not in default:
        bunk.append(bed_origin_at_wall("south", west, south, 0.0))
    if "west" not in corridor and "west" not in default:
        bunk.append(bed_origin_at_wall("west", west, west_bed_z, HALF_PI))
    if area >= 8 and "south" not in corridor and "west" not in corridor:
        if "south" not in default and "west" not in default:
            bunk = [
                bed_origin_at_wall("west", west, west_bed_z, HALF_PI),
                bed_origin_at_wall("south", west, south, 0.0),
            ]
    for bed in bunk:
        if keep(bed):
            out.append(bed)

    if not out:
        # Exterior rooms: pick any wall where the bunk bed bbox stays inside the room.
        mid_x = (west + east) / 2.0
        mid_z = (south + north) / 2.0
        for face, x, z, yaw in (
            ("south", west, south, 0.0),
            ("west", west, west_bed_z, HALF_PI),
            ("north", west, north, math.pi),
            ("east", east, mid_z, -HALF_PI),
        ):
            if face in corridor:
                continue
            bed = bed_origin_at_wall(face, x, z, yaw)
            if keep(bed):
                out.append(bed)
                break

    if area >= 4 and not (area <= 6 and "south" not in corridor):
        corner_stem = rng.choice(["container", "container-flat"])
        if "south" not in corridor and "south" not in default:
            c = flush_back_to_wall(corner_stem, "east", east, -HALF_PI, z=south)
            out.append(nudge_prop_to_room(c, west, east, south, north))
        elif "east" not in corridor:
            c = flush_back_to_wall(corner_stem, "east", east, -HALF_PI, z=north)
            out.append(nudge_prop_to_room(c, west, east, south, north))
    return out


def setup_lab(
    cells_w: set[CellW],
    cells_ix: set[CellIx],
    floor_ix: set[CellIx],
    rng: random.Random,
    *,
    zone_lookup: Callable[[CellIx], str | None] | None = None,
    corridor_ix: set[CellIx] | None = None,
    synth_zones: frozenset[str] = frozenset({"prev", "next"}),
    transition_ix: set[CellIx] | None = None,
    cell_at: Callable[[float, float], CellIx] | None = None,
) -> list[dict]:
    _, east, south, north, _, _ = _room_span(cells_w)
    depth = north - south
    wall = _pick_interior_wall(
        cells_w, cells_ix, corridor_ix or set(), floor_ix, zone_lookup, synth_zones,
        transition_ix=transition_ix, cell_at=cell_at,
    )
    out: list[dict] = []
    desk_stems = ["computer-screen", "computer-system", "computer-wide"]
    chair_stems = ["chair", "chair-armrest-headrest"]
    if wall is None:
        west, east, south, north, mid_x, mid_z = _room_span(cells_w)
        desk = prop("computer-system", mid_x, mid_z, look=(mid_x, mid_z + CELL * 0.35))
        out.extend([desk, chair_before_desk(rng.choice(chair_stems), desk, desk["stem"])])
    elif wall == "south":
        xs_l = _south_desk_xs(cells_ix, floor_ix, desk_stems[0])
        xs_r = _south_desk_xs(cells_ix, floor_ix, desk_stems[1])
        xs = [x for x in xs_l if x in xs_r] or xs_l or xs_r
        if len(xs) >= 2 and xs[-1] - xs[0] >= CELL * 2:
            desk_l = flush_back_to_wall(desk_stems[0], "south", xs[0], 0.0, z=south)
            desk_r = flush_back_to_wall(desk_stems[1], "south", xs[-1], 0.0, z=south)
            out.extend(
                [
                    desk_l,
                    desk_r,
                    chair_before_desk(chair_stems[0], desk_l, desk_stems[0]),
                    chair_before_desk(chair_stems[1], desk_r, desk_stems[1]),
                ]
            )
        elif xs:
            x = xs[len(xs) // 2]
            stem = rng.choice(desk_stems)
            desk = flush_back_to_wall(stem, "south", x, 0.0, z=south)
            out.extend([desk, chair_before_desk(rng.choice(chair_stems), desk, stem)])
    else:
        west, east, south, north, mid_x, mid_z = _room_span(cells_w)
        yaw = {"west": HALF_PI, "east": -HALF_PI, "north": math.pi, "south": 0.0}[wall]
        stem = rng.choice(desk_stems)
        if wall in ("north", "south"):
            desk = flush_back_to_wall(stem, wall, mid_x, yaw, z=north if wall == "north" else south)
        else:
            desk = flush_back_to_wall(stem, wall, west if wall == "west" else east, yaw, z=mid_z)
        out.extend([desk, chair_before_desk(rng.choice(chair_stems), desk, stem)])
    if depth >= CELL * 3 and wall != "east":
        c = flush_back_to_wall("container-tall", "east", east, -HALF_PI, z=north)
        out.append(nudge_prop_to_room(c, *(_room_span(cells_w)[:4])))
    # extra filler to avoid mostly empty labs
    if len(out) < 2 and len(cells_w) > 4:
        try:
            filler = prop("container", mid_x, mid_z)
            out.append(nudge_prop_to_room(filler, *(_room_span(cells_w)[:4])))
        except:
            pass
    return out


def setup_office(
    cells_w: set[CellW],
    cells_ix: set[CellIx],
    floor_ix: set[CellIx],
    _: random.Random,
    *,
    zone_lookup: Callable[[CellIx], str | None] | None = None,
    corridor_ix: set[CellIx] | None = None,
    synth_zones: frozenset[str] = frozenset({"prev", "next"}),
    transition_ix: set[CellIx] | None = None,
    cell_at: Callable[[float, float], CellIx] | None = None,
) -> list[dict]:
    west, east, south, north, mid_x, mid_z = _room_span(cells_w)
    depth = north - south
    wall = _pick_interior_wall(
        cells_w, cells_ix, corridor_ix or set(), floor_ix, zone_lookup, synth_zones,
        transition_ix=transition_ix, cell_at=cell_at,
    )
    if wall is None:
        corridor = opens_to_corridor(cells_ix, corridor_ix or set())
        look_z = mid_z + CELL if "south" in corridor else mid_z - CELL if "north" in corridor else mid_z + CELL * 0.35
        look_x = mid_x + CELL if "west" in corridor else mid_x - CELL if "east" in corridor else mid_x
        desk = prop(
            "computer-system",
            mid_x,
            mid_z - CELL * 0.1 if "south" not in corridor else mid_z + CELL * 0.1,
            look=(look_x, look_z),
        )
        return [desk, chair_before_desk("chair-armrest-headrest", desk)]
    if wall == "south":
        xs = _south_desk_xs(cells_ix, floor_ix, "computer-system")
        desk_x = xs[len(xs) // 2] if xs else mid_x
        desk = flush_back_to_wall("computer-system", "south", desk_x, 0.0, z=south)
    else:
        yaw = {"west": HALF_PI, "east": -HALF_PI, "north": math.pi, "south": 0.0}[wall]
        if wall in ("north", "south"):
            desk = flush_back_to_wall("computer-system", wall, mid_x, yaw, z=north if wall == "north" else south)
        else:
            desk = flush_back_to_wall("computer-system", wall, west if wall == "west" else east, yaw, z=mid_z)
    out = [
        desk,
        chair_before_desk("chair-armrest-headrest", desk),
    ]
    if depth >= CELL * 4 and wall != "east":
        c = flush_back_to_wall("container-tall", "east", east, -HALF_PI, z=north)
        out.append(nudge_prop_to_room(c, west, east, south, north))
    elif depth >= CELL * 3 and wall != "west":
        c = flush_back_to_wall("container", "west", west, HALF_PI, z=south)
        out.append(nudge_prop_to_room(c, west, east, south, north))
    return out


def setup_storage(cells_w: set[CellW], rng: random.Random) -> list[dict]:
    west, east, south, north, _, _ = _room_span(cells_w)
    stem = rng.choice(["container", "container-tall", "container-wide"])
    corner = rng.choice(["ne", "nw", "se", "sw"])
    if corner == "ne":
        c = flush_back_to_wall(stem, "east", east, -HALF_PI, z=north)
    elif corner == "nw":
        c = flush_back_to_wall(stem, "west", west, HALF_PI, z=north)
    elif corner == "se":
        c = flush_back_to_wall(stem, "east", east, -HALF_PI, z=south)
    else:
        c = flush_back_to_wall(stem, "west", west, HALF_PI, z=south)
    out = [nudge_prop_to_room(c, west, east, south, north)]
    # second for less empty storage rooms
    if len(cells_w) > 5:
        stem2 = rng.choice(["container-flat", "container"])
        c2 = flush_back_to_wall(stem2, "east", east, -HALF_PI, z=south)
        out.append(nudge_prop_to_room(c2, west, east, south, north))
    return out


def setup_mess(cells_w: set[CellW], rng: random.Random) -> list[dict]:
    west, east, south, north, mid_x, mid_z = _room_span(cells_w)
    if len(cells_w) >= 6:
        table = flush_back_to_wall("table", "east", east, -HALF_PI, z=mid_z)
        table = nudge_prop_to_room(table, west, east, south, north)
        return [table, *chairs_at_east_table(rng.choice(["chair", "chair-cushion"]), table)]
    table = flush_back_to_wall("table-inset", "south", mid_x, 0.0, z=south)
    return [table, chair_before_desk("chair", table, "table-inset")]


def furnish_room(
    role: str,
    cells_w: set[CellW],
    cells_ix: set[CellIx],
    corridor_ix: set[CellIx],
    floor_ix: set[CellIx],
    rng: random.Random,
    *,
    zone_lookup: Callable[[CellIx], str | None] | None = None,
    transition_ix: set[CellIx] | None = None,
    synth_zones: frozenset[str] = frozenset({"prev", "next"}),
    cell_at: Callable[[float, float], CellIx] | None = None,
) -> list[dict]:
    trans = transition_ix or set()
    if role == "quarters":
        if not valid_quarters_room(cells_ix, corridor_ix, trans):
            return []
        return setup_quarters(
            cells_w, cells_ix, corridor_ix, rng,
            zone_lookup=zone_lookup, transition_ix=trans, synth_zones=synth_zones,
            cell_at=cell_at,
        )
    if role == "lab":
        return setup_lab(
            cells_w, cells_ix, floor_ix, rng,
            zone_lookup=zone_lookup, corridor_ix=corridor_ix, synth_zones=synth_zones,
            transition_ix=trans, cell_at=cell_at,
        )
    if role == "command":
        if len(cells_w) >= 20:
            return setup_mess(cells_w, rng)
        return setup_office(
            cells_w, cells_ix, floor_ix, rng,
            zone_lookup=zone_lookup, corridor_ix=corridor_ix, synth_zones=synth_zones,
            transition_ix=trans, cell_at=cell_at,
        )
    if role == "storage":
        return setup_storage(cells_w, rng)
    if role == "mess":
        return setup_mess(cells_w, rng)
    if not props and cells_w:
        # fallback to ensure rooms have at least something
        west, east, south, north, mid_x, mid_z = _room_span(cells_w)
        try:
            p = prop("container", mid_x, mid_z)
            props = [nudge_prop_to_room(p, west, east, south, north)]
        except:
            props = []
    return props


def validate_props(pieces: list[dict], name: str, quiet: bool = False) -> list[str]:
    errors: list[str] = []
    props: list[dict] = []
    for p in pieces:
        stem = p.get("stem", "")
        if stem.startswith(("floor", "wall")):
            continue
        if p.get("role") in ("floor", "wall", "stairs", "deck"):
            continue
        if p.get("role") == "prop":
            props.append(p)
            continue
        try:
            if stem_info(stem)["class"] not in ("structure",):
                props.append(p)
        except KeyError:
            if p.get("role") == "prop":
                props.append(p)

    boxes = [(p, world_bbox(p["stem"], p["x"], p["z"], p["yaw"], p["scale"])) for p in props]
    for i, (a, bb_a) in enumerate(boxes):
        tags_a = a.get("tags") or []
        if "synth_balcony" in tags_a or "synth_mezz" in tags_a:
            continue
        for b, bb_b in boxes[i + 1 :]:
            tags_b = b.get("tags") or []
            if "synth_balcony" in tags_b or "synth_mezz" in tags_b:
                continue
            if bb_a.padded(GAP_M).overlaps(bb_b.padded(GAP_M)):
                errors.append(
                    f"overlap: {a['stem']}@({a['x']},{a['z']}) ↔ {b['stem']}@({b['x']},{b['z']})"
                )

    for p in props:
        if not p["stem"].startswith("chair"):
            continue
        targets = [
            q
            for q in props
            if q is not p and q["stem"].startswith(("computer", "table"))
        ]
        if not targets:
            continue
        tgt = min(targets, key=lambda q: (q["x"] - p["x"]) ** 2 + (q["z"] - p["z"]) ** 2)
        fx, fz = world_front(p["stem"], p["yaw"])
        dx, dz = tgt["x"] - p["x"], tgt["z"] - p["z"]
        mag = math.hypot(dx, dz) or 1.0
        dot = fx * (dx / mag) + fz * (dz / mag)
        if dot < 0.85:
            errors.append(
                f"chair faces away from {tgt['stem']}: ({p['x']},{p['z']}) yaw={p['yaw']:.2f} dot={dot:.2f}"
            )

    walls = [
        p
        for p in pieces
        if (p.get("stem") == "wall" or p.get("role") == "wall")
        and not str(p.get("stem", "")).startswith("balcony")
    ]
    wall_boxes = [wall_solid_bbox(w) for w in walls]
    for p in props:
        tags = p.get("tags") or []
        if "synth_balcony" in tags or "synth_mezz" in tags:
            continue
        if float(p.get("y", DECK_Y)) > DECK_Y + 0.5:
            continue
        if p["stem"].startswith(("bed-single", "bed-double")):
            continue
        bb = world_bbox(p["stem"], p["x"], p["z"], p["yaw"], p["scale"])
        for i, wb in enumerate(wall_boxes):
            if bbox_penetrates(bb, wb):
                w = walls[i]
                errors.append(
                    f"wall clip: {p['stem']}@({p['x']},{p['z']}) ↔ wall@({w['x']},{w['z']})"
                )
                break

    if errors and not quiet:
        print(f"VALIDATION FAILED {name}:", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
    return errors


def generate_rating_floor_plan(seed: int = 42) -> tuple[set[CellIx], set[CellIx], dict[int, set[CellIx]]]:
    """Curated ~20-room wing + corridor network for interior rating."""
    rng = random.Random(seed)
    corridor: set[CellIx] = set()
    corridor |= cells_rect(-20, 0, 20, 1)
    corridor |= cells_rect(-20, -1, 20, -1)
    corridor |= cells_rect(-20, 2, 20, 2)

    room_specs: list[tuple[int, int, int, int]] = [
        # North wing (open onto corridor row iz=2)
        (-20, 3, -18, 5),
        (-17, 3, -14, 5),
        (-13, 3, -11, 4),
        (-10, 3, -7, 6),
        (-6, 3, -4, 4),
        (-3, 3, 0, 5),
        (1, 3, 4, 6),
        (5, 3, 8, 4),
        (9, 3, 12, 6),
        (13, 3, 16, 5),
        (17, 3, 19, 4),
        # South wing (open onto corridor row iz=-1)
        (-20, -5, -18, -2),
        (-17, -6, -14, -2),
        (-13, -4, -10, -2),
        (-9, -7, -6, -2),
        (-5, -5, -2, -2),
        (1, -6, 4, -2),
        (5, -4, 8, -2),
        (9, -8, 13, -2),
        (14, -5, 18, -2),
        (-20, -9, -16, -6),
        (0, -10, 3, -7),
    ]

    rooms: dict[int, set[CellIx]] = {}
    occupied: set[CellIx] = set(corridor)
    rid = 0
    for ix0, iz0, ix1, iz1 in room_specs:
        cells = cells_rect(ix0, iz0, ix1, iz1)
        if cells & occupied:
            continue
        if count_corridor_mouths(cells, corridor) == 0:
            continue
        rooms[rid] = cells
        occupied |= cells
        rid += 1

    floor_ix = occupied
    _ = rng
    return floor_ix, corridor, rooms


def build_room_infos(
    rooms: dict[int, set[CellIx]], corridor_ix: set[CellIx], roles: dict[int, str]
) -> list[RoomInfo]:
    infos: list[RoomInfo] = []
    for rid, cells_ix in sorted(rooms.items()):
        cells_w = {ix_to_world(c) for c in cells_ix}
        west, east, south, north = world_bounds(cells_w)
        infos.append(
            RoomInfo(
                room_id=rid,
                cells_ix=cells_ix,
                cells_w=cells_w,
                role=roles[rid],
                area=len(cells_ix),
                corridor_mouths=count_corridor_mouths(cells_ix, corridor_ix),
                centre_w=((west + east) / 2, (south + north) / 2),
            )
        )
    return infos


def furnish_floor_plan(
    seed: int = 42,
) -> tuple[list[dict], list[RoomInfo], set[CellIx], set[CellIx]]:
    floor_ix, corridor_ix, rooms = generate_rating_floor_plan(seed)
    roles = assign_roles(rooms, corridor_ix)
    room_infos = build_room_infos(rooms, corridor_ix, roles)
    pieces = build_shell(floor_ix, corridor_ix, rooms)
    rng = random.Random(seed ^ 0xA5A5)

    for info in room_infos:
        if info.role == "corridor":
            continue
        props = furnish_room(info.role, info.cells_w, info.cells_ix, corridor_ix, floor_ix, rng)
        for p in props:
            p["tags"] = ["synth_prop", "synth_interior", info.role, f"room_{info.room_id}"]
            p["role"] = "prop"
        pieces.extend(props)

    return pieces, room_infos, floor_ix, corridor_ix


DECK_Y = CATALOG.get("deck_y", 1.2)
FLIGHT_RISE = DECK_Y  # one synth flight = deck block height


def generate_showcase_floor_plan(
    seed: int = 42,
) -> tuple[set[CellIx], set[CellIx], dict[int, set[CellIx]]]:
    """Compact furnished wing — varied room sizes, exterior void for balconies."""
    _ = random.Random(seed)
    corridor: set[CellIx] = set()
    corridor |= cells_rect(-10, 0, 10, 0)
    corridor |= cells_rect(-10, -1, 10, -1)
    corridor |= cells_rect(-10, 1, 10, 1)
    corridor |= cells_rect(-10, 2, 10, 2)

    room_specs: list[tuple[int, int, int, int]] = [
        # North wing (opens onto corridor row iz=2)
        (-10, 3, -8, 5),   # quarters
        (-7, 3, -4, 6),    # lab
        (-3, 3, 2, 7),     # command hall (large)
        (3, 3, 6, 5),      # lab
        (7, 3, 10, 6),     # mess bay
        # South wing (opens onto corridor row iz=-1)
        (-10, -4, -7, -2),  # storage dead-end
        (-6, -5, -3, -2),   # lab
        (0, -4, 3, -2),     # quarters
        (4, -5, 8, -2),     # mess
        (9, -4, 10, -2),    # storage nook
    ]

    rooms: dict[int, set[CellIx]] = {}
    occupied: set[CellIx] = set(corridor)
    rid = 0
    for ix0, iz0, ix1, iz1 in room_specs:
        cells = cells_rect(ix0, iz0, ix1, iz1)
        if cells & occupied:
            continue
        if count_corridor_mouths(cells, corridor) == 0:
            continue
        rooms[rid] = cells
        occupied |= cells
        rid += 1

    return occupied, corridor, rooms


def _balcony_y(stem: str) -> float:
    if stem.startswith("balcony-floor"):
        return DECK_Y - 0.6  # DeckTopFlush @ scale 4 → walkable top 1.2 m
    return DECK_Y


# Balcony geometry (probed @ scale 4): balcony-floor-center is 4 m wide x 2.8 m deep.
# Outward unit per exterior face; the edge a face's tile spans runs perpendicular to it.
OUTWARD: dict[str, tuple[float, float]] = {
    "n": (0.0, -1.0),
    "s": (0.0, 1.0),
    "e": (1.0, 0.0),
    "w": (-1.0, 0.0),
}
FACE_STEPS: dict[str, tuple[int, int]] = {
    "n": (0, -1),
    "s": (0, 1),
    "e": (1, 0),
    "w": (-1, 0),
}
# VISUAL ORIENTATION (probed from the GLBs, not just bbox):
#   balcony-floor-center has a RAISED LIP on its +z edge (verts y=0.15 @ z[0.15,0.35];
#   walkable surface y=0.10 on the -z side). The lip must face OUTWARD over the drop, so
#   the tile yaw points local +z toward the cell's outward direction (face_yaw, front=+z).
#   Using a single yaw for n+s (or e+w) is the "correct in 2 dirs, wrong in 2" bug.
FLOOR_EDGE_YAW: dict[str, float] = {
    "n": math.pi,       # outward -z
    "s": 0.0,           # outward +z (lip already faces +z)
    "e": HALF_PI,       # outward +x
    "w": 3 * HALF_PI,   # outward -x
}
#   balcony-floor-corner's recessed walkable area opens toward (+x,-z); its lip wraps the
#   -x and +z edges, so the lip-corner faces local (-x,+z). Each convex corner needs a
#   distinct yaw so that lip-corner points along the outward diagonal.
CORNER_FLOOR_YAW: dict[frozenset, float] = {
    frozenset({"s", "w"}): 0.0,        # outward (-x,+z)
    frozenset({"s", "e"}): HALF_PI,    # outward (+x,+z)
    frozenset({"n", "e"}): math.pi,    # outward (+x,-z)
    frozenset({"n", "w"}): 3 * HALF_PI,  # outward (-x,-z)
}
FLOOR_HALF = CELL / 2.0  # 2.0 m (room floor cell half-width)


def _balcony_geometry() -> tuple[float, float, float]:
    """(half_depth, floor_centre_offset, rail_offset) for balcony tiles @ current scale."""
    half_depth = bounds_scaled("balcony-floor-center")["z1"]  # 1.4 m
    offset = FLOOR_HALF + half_depth  # 3.4 m: inner edge meets building floor edge
    rail_offset = FLOOR_HALF + 2.0 * half_depth  # 4.8 m: outer (open) edge of the ledge
    return half_depth, offset, rail_offset


def substrate_floor_cells(
    pieces: list[dict],
    zone_lookup: Callable[[CellIx], str | None],
    *,
    cell_at: Callable[[float, float], CellIx],
) -> set[CellIx]:
    """Industrial substrate cells that actually carry a floor GLB at y≈0 (not just walkable grid)."""
    out: set[CellIx] = set()
    for p in pieces:
        if int(p.get("floor_level", 0)) != 0 or p.get("ceiling"):
            continue
        stem = str(p.get("stem", ""))
        role = p.get("role", "")
        if role not in ("floor", "deck") and not stem.startswith(
            ("floor", "template-floor", "corridor", "room")
        ):
            continue
        if float(p.get("y", 0.0)) > 0.05:
            continue
        c = cell_at(float(p["x"]), float(p["z"]))
        if zone_lookup(c) == "default":
            out.add(c)
    return out


def _room_exterior_faces(
    floor_ix: set[CellIx],
    corridor_ix: set[CellIx],
    walkable: set[CellIx] | None = None,
    zone_lookup: Callable[[CellIx], str | None] | None = None,
    substrate_cells: set[CellIx] | None = None,
    transition_cells: set[CellIx] | None = None,
) -> list[tuple[int, int, str]]:
    """(ix, iz, face) for room-cell faces that may get a balcony.

    Balconies require the far-side cell to be a **real industrial map tile** outside this
    synth footprint — never void, never another synth cell. Same intent as perimeter
    windows (shuttered windows may face void; balconies may not).
    """
    if walkable is None:
        return []
    faces: list[tuple[int, int, str]] = []
    for ix, iz in sorted(floor_ix):
        if (ix, iz) in corridor_ix:
            continue
        for face, (sx, sz) in FACE_STEPS.items():
            nb = (ix + sx, iz + sz)
            if nb in floor_ix or nb not in walkable:
                continue
            if zone_lookup is not None and zone_lookup(nb) != "default":
                continue
            if substrate_cells is not None and nb not in substrate_cells:
                continue
            # Avoid placing balcony on or next to transition cells (stairs/doors) so rails don't block access
            if transition_cells is not None:
                trans = transition_cells
                if (ix, iz) in trans:
                    continue
                is_near_trans = any( (ix + adx, iz + adz) in trans for adx, adz in ((0,0),(1,0),(-1,0),(0,1),(0,-1)) )
                if is_near_trans:
                    continue
            # Require at least 2 cells of depth inward so balcony isn't crammed against opposite wall
            # (addresses "balcony facing a wall, needs 2 tiles between windows and next wall")
            id_x, id_z = -sx, -sz
            depth_ok = True
            for d in (1, 2):
                cx, cz = ix + d * id_x, iz + d * id_z
                if (cx, cz) not in floor_ix:
                    depth_ok = False
                    break
            if not depth_ok:
                continue
            faces.append((ix, iz, face))
    return faces


def expected_balcony_floors(
    floor_ix: set[CellIx],
    corridor_ix: set[CellIx],
    *,
    world_at: WorldAt | None = None,
    walkable: set[CellIx] | None = None,
    zone_lookup: Callable[[CellIx], str | None] | None = None,
    substrate_cells: set[CellIx] | None = None,
    transition_cells: set[CellIx] | None = None,
) -> dict[tuple[float, float], tuple[str, float]]:
    """Authoritative balcony floor layout: {(x, z): (stem, yaw)}.

    Single source of truth shared by the generator and the verifier so a passing
    audit guarantees the on-disk JSON matches this exact placement.
    """
    _, offset, _ = _balcony_geometry()
    faces = _room_exterior_faces(
        floor_ix, corridor_ix, walkable, zone_lookup, substrate_cells, transition_cells,
    )
    out: dict[tuple[float, float], tuple[str, float]] = {}
    cell_faces: dict[CellIx, set[str]] = {}
    for ix, iz, face in faces:
        cell_faces.setdefault((ix, iz), set()).add(face)
        cx, cz = _cell_world((ix, iz), world_at)
        ox, oz = OUTWARD[face]
        key = (round(cx + ox * offset, 1), round(cz + oz * offset, 1))
        out.setdefault(key, ("balcony-floor-center", FLOOR_EDGE_YAW[face]))
    for (ix, iz), fset in cell_faces.items():
        cx, cz = _cell_world((ix, iz), world_at)
        for fa, fb in (("n", "e"), ("n", "w"), ("s", "e"), ("s", "w")):
            if fa in fset and fb in fset:
                oax, oaz = OUTWARD[fa]
                obx, obz = OUTWARD[fb]
                key = (round(cx + (oax + obx) * offset, 1), round(cz + (oaz + obz) * offset, 1))
                out.setdefault(key, ("balcony-floor-corner", CORNER_FLOOR_YAW[frozenset({fa, fb})]))
    return out


def _plan_rail_run(length: float) -> list[tuple[float, str]]:
    """Tile a straight run of ``length`` m: list of (centre_offset_from_start, stem).

    Short stubs (< 3.4 m, e.g. a convex corner's 2.8 m overhang edge) take one 2 m
    ``rail-narrow``; longer runs take evenly-spaced 4 m ``rail`` pieces. Pieces may
    overlap slightly along the run (a continuous fence) but never overshoot the corner.
    """
    if length < 3.4:
        return [(length / 2.0, "rail-narrow")]
    n = max(1, round(length / 4.0))
    return [(length * (i + 0.5) / n, "rail") for i in range(n)]


def expected_balcony_rails(
    floor_ix: set[CellIx],
    corridor_ix: set[CellIx],
    *,
    world_at: WorldAt | None = None,
    walkable: set[CellIx] | None = None,
    zone_lookup: Callable[[CellIx], str | None] | None = None,
    substrate_cells: set[CellIx] | None = None,
    transition_cells: set[CellIx] | None = None,
) -> list[tuple[float, float, float, str]]:
    """Authoritative balcony rail layout: list of (x, z, yaw, stem).

    Rails follow the OUTER boundary of the *union* of ledge tiles. Tracing the union
    boundary (instead of one long rail per exterior face) means perpendicular runs meet
    at a butt joint at every corner — convex and concave — instead of two long rails
    overshooting and crossing (the inner-corner bug). Single source of truth for the
    generator and verifier.
    """
    ledge = expected_balcony_floors(
        floor_ix,
        corridor_ix,
        world_at=world_at,
        walkable=walkable,
        zone_lookup=zone_lookup,
        substrate_cells=substrate_cells,
        transition_cells=transition_cells,
    )
    if not ledge:
        return []
    res = 0.2
    rail_half = bounds_scaled("rail")["z1"]

    ledge_cells: set[tuple[int, int]] = set()
    for (bx, bz), (stem, yaw) in ledge.items():
        ex = bounds_scaled(stem)["x1"]
        ez = bounds_scaled(stem)["z1"]
        if abs(math.cos(yaw)) < 0.5:  # tile rotated 90° → width runs along z
            ex, ez = ez, ex
        gx0, gx1 = round((bx - ex) / res), round((bx + ex) / res)
        gz0, gz1 = round((bz - ez) / res), round((bz + ez) / res)
        for gx in range(gx0, gx1):
            for gz in range(gz0, gz1):
                ledge_cells.add((gx, gz))
    build_cells: set[tuple[int, int]] = set()
    for cell in floor_ix:
        cx, cz = _cell_world(cell, world_at)
        gx0, gx1 = round((cx - FLOOR_HALF) / res), round((cx + FLOOR_HALF) / res)
        gz0, gz1 = round((cz - FLOOR_HALF) / res), round((cz + FLOOR_HALF) / res)
        for gx in range(gx0, gx1):
            for gz in range(gz0, gz1):
                build_cells.add((gx, gz))

    def is_open(cell: tuple[int, int]) -> bool:
        return cell not in ledge_cells and cell not in build_cells

    # Outward boundary unit edges, grouped by (line, inward-sign); inward sign insets the rail.
    h_runs: dict[tuple[int, int], set[int]] = {}  # (z_line, sign) -> {gx}
    v_runs: dict[tuple[int, int], set[int]] = {}  # (x_line, sign) -> {gz}
    for gx, gz in ledge_cells:
        if is_open((gx, gz + 1)):
            h_runs.setdefault((gz + 1, -1), set()).add(gx)
        if is_open((gx, gz - 1)):
            h_runs.setdefault((gz, 1), set()).add(gx)
        if is_open((gx + 1, gz)):
            v_runs.setdefault((gx + 1, -1), set()).add(gz)
        if is_open((gx - 1, gz)):
            v_runs.setdefault((gx, 1), set()).add(gz)

    def merge(idxs: set[int]) -> list[tuple[int, int]]:
        s = sorted(idxs)
        runs: list[tuple[int, int]] = []
        a = b = s[0]
        for v in s[1:]:
            if v == b + 1:
                b = v
            else:
                runs.append((a, b))
                a = b = v
        runs.append((a, b))
        return runs

    rails: list[tuple[float, float, float, str]] = []
    for (z_line, sign), idxs in h_runs.items():
        z_w = z_line * res + sign * rail_half
        for a, b in merge(idxs):
            start = a * res
            length = (b - a + 1) * res
            for off, stem in _plan_rail_run(length):
                rails.append((round(start + off, 4), round(z_w, 4), 0.0, stem))
    for (x_line, sign), idxs in v_runs.items():
        x_w = x_line * res + sign * rail_half
        for a, b in merge(idxs):
            start = a * res
            length = (b - a + 1) * res
            for off, stem in _plan_rail_run(length):
                rails.append((round(x_w, 4), round(start + off, 4), HALF_PI, stem))
    return rails


def _room_footprint_mid(floor_ix: set[CellIx], corridor_ix: set[CellIx]) -> tuple[float, float]:
    room = [(ix, iz) for ix, iz in floor_ix if (ix, iz) not in corridor_ix]
    if not room:
        return 0.0, 0.0
    xs = [ix * CELL for ix, _ in room]
    zs = [iz * CELL for _, iz in room]
    return (min(xs) + max(xs)) / 2.0, (min(zs) + max(zs)) / 2.0


def _balcony_yaw(
    room_dirs: set[str], bx: float, bz: float, mid_x: float, mid_z: float
) -> tuple[str, float]:
    """Stem + yaw from hand-authored ``bunk.json`` exterior L-shape."""
    if len(room_dirs) >= 2:
        key = frozenset(room_dirs)
        yaw = {
            frozenset({"n", "e"}): HALF_PI,
            frozenset({"s", "e"}): math.pi,
            frozenset({"s", "w"}): 3 * HALF_PI,
            frozenset({"n", "w"}): 0.0,
        }.get(key, HALF_PI)
        return "balcony-floor-corner", yaw
    (d,) = tuple(room_dirs)
    if d in ("w", "e"):
        return "balcony-floor-center", (0.0 if bz >= mid_z else math.pi)
    return "balcony-floor-center", (HALF_PI if bx >= mid_x else 3 * HALF_PI)


def prune_invalid_balconies(
    pieces: list[dict],
    floor_ix: set[CellIx],
    corridor_ix: set[CellIx],
    *,
    world_at: WorldAt | None = None,
    walkable: set[CellIx] | None = None,
    zone_lookup: Callable[[CellIx], str | None] | None = None,
    cell_at: Callable[[float, float], CellIx] | None = None,
) -> list[dict]:
    """Drop balcony pieces that do not match the authoritative layout (stale / void-facing)."""
    substrate = (
        substrate_floor_cells(pieces, zone_lookup, cell_at=cell_at)
        if zone_lookup is not None and cell_at is not None
        else None
    )
    expected_f = expected_balcony_floors(
        floor_ix,
        corridor_ix,
        world_at=world_at,
        walkable=walkable,
        zone_lookup=zone_lookup,
        substrate_cells=substrate,
    )
    expected_r = {
        (round(x, 4), round(z, 4), round(yaw, 4), stem)
        for x, z, yaw, stem in expected_balcony_rails(
            floor_ix,
            corridor_ix,
            world_at=world_at,
            walkable=walkable,
            zone_lookup=zone_lookup,
            substrate_cells=substrate,
        )
    }
    out: list[dict] = []
    for p in pieces:
        tags = p.get("tags") or []
        if "synth_balcony" not in tags:
            out.append(p)
            continue
        stem = str(p.get("stem", ""))
        if stem.startswith("balcony-floor"):
            key = (round(p["x"], 1), round(p["z"], 1))
            if key in expected_f:
                out.append(p)
            continue
        if "balcony_rail" in tags or stem in ("rail", "rail-narrow"):
            key = (round(p["x"], 4), round(p["z"], 4), round(float(p.get("yaw", 0.0)), 4), stem)
            if key in expected_r:
                out.append(p)
            continue
        out.append(p)
    return out


def apply_perimeter_balconies(
    pieces: list[dict],
    floor_ix: set[CellIx],
    corridor_ix: set[CellIx],
    rng: random.Random,
    *,
    rate: float = 1.0,
    world_at: WorldAt | None = None,
    walkable: set[CellIx] | None = None,
    zone: str | None = None,
    zone_lookup: Callable[[CellIx], str | None] | None = None,
    cell_at: Callable[[float, float], CellIx] | None = None,
    transition_cells: set[CellIx] | None = None,
) -> list[dict]:
    """Balcony ledge abutting every exterior room edge: floor tiles + corners + outer rail.

    The ledge inner edge meets the building floor edge (touches the building), tiles are
    4 m wide on 4 m centres (touch each other), and the open outer edge gets a rail.
    """
    _ = rate
    substrate = (
        substrate_floor_cells(pieces, zone_lookup, cell_at=cell_at)
        if zone_lookup is not None and cell_at is not None
        else None
    )
    faces = _room_exterior_faces(
        floor_ix, corridor_ix, walkable, zone_lookup, substrate, transition_cells,
    )
    if not faces:
        return list(pieces)

    # Drop exterior walls so each balcony edge reads as an open ledge.
    drop_walls: set[tuple[float, float, float]] = set()
    for ix, iz, face in faces:
        cx, cz = _cell_world((ix, iz), world_at)
        sx, sz = FACE_STEPS[face]
        drop_walls.add(
            (round(cx + sx * FLOOR_HALF, 1), round(cz + sz * FLOOR_HALF, 1), round(wall_yaw(sx, sz), 4))
        )
    out: list[dict] = []
    for p in pieces:
        if p.get("stem") == "wall" and p.get("role") == "wall":
            key = (round(p["x"], 1), round(p["z"], 1), round(p["yaw"], 4))
            if key in drop_walls:
                continue
        out.append(p)

    deck_y = DECK_Y - bounds_scaled("balcony-floor-center")["y1"]  # origin so top == DECK_Y
    for (bx, bz), (stem, yaw) in sorted(
        expected_balcony_floors(
            floor_ix,
            corridor_ix,
            world_at=world_at,
            walkable=walkable,
            zone_lookup=zone_lookup,
            substrate_cells=substrate,
            transition_cells=transition_cells,
        ).items()
    ):
        piece = {
            "stem": stem,
            "x": bx,
            "z": bz,
            "yaw": yaw,
            "floor_level": 0,
            "scale": SCALE,
            "kit": KIT,
            "y": deck_y,
            "role": "wall",
            "tags": ["synth_balcony", "balcony_floor"],
        }
        if zone:
            piece["zone"] = zone
        out.append(piece)

    # Rails trace the outer boundary of the ledge union (see expected_balcony_rails):
    # perpendicular runs butt-join at corners instead of crossing.
    for rx, rz, ryaw, rstem in expected_balcony_rails(
        floor_ix,
        corridor_ix,
        world_at=world_at,
        walkable=walkable,
        zone_lookup=zone_lookup,
        substrate_cells=substrate,
    ):
        rail = {
            "stem": rstem,
            "x": rx,
            "z": rz,
            "yaw": ryaw,
            "floor_level": 0,
            "scale": SCALE,
            "kit": KIT,
            "y": DECK_Y,
            "role": "prop",
            "tags": ["synth_balcony", "balcony_rail"],
        }
        if zone:
            rail["zone"] = zone
        out.append(rail)
    return out


MEZZ_STAIR_STEM = "stairs-small-center"
WALL_YAW_INTO = {"south": 0.0, "north": math.pi, "east": -HALF_PI, "west": HALF_PI}


def _mezz_stair_travel(stair_iz: int, deck_izs: list[int]) -> str:
    """Compass direction from stair cell toward the deck (``transition_entrances.DELTA`` keys)."""
    deck_center = sum(deck_izs) / len(deck_izs)
    if stair_iz < deck_center - 1e-3:
        return "S"
    if stair_iz > deck_center + 1e-3:
        return "N"
    return "S"


def _mezz_stair_yaw(stair_iz: int, deck_izs: list[int]) -> float:
    import transition_entrances as te

    travel = _mezz_stair_travel(stair_iz, deck_izs)
    return te._stairs_yaw(travel, ascending=True)


def _mezz_parapet(stair_iz: int, deck_izs: list[int]) -> tuple[str, int]:
    """Outer deck row + wall face for loft props (parapet away from the stair approach)."""
    deck_center = sum(deck_izs) / len(deck_izs)
    if stair_iz < deck_center:
        return "north", max(deck_izs)
    return "south", min(deck_izs)


def _deck_adjacent_corridor(deck_cells: list[CellIx], corridor_ix: set[CellIx]) -> bool:
    for ix, iz in deck_cells:
        for dx, dz in ((0, -1), (0, 1), (1, 0), (-1, 0)):
            if (ix + dx, iz + dz) in corridor_ix:
                return True
    return False


def _pick_mezz_stair_col(
    cells: set[CellIx],
    stair_row: int,
    open_row: int,
    corridor_faces: set[str],
) -> int | None:
    """Stair column flush against a side wall, preferring the wall away from the corridor.

    Restricted to columns present in BOTH the stair row and the deck row it lands on so
    the flight always connects ground -> deck.
    """
    cols = sorted(
        {ix for ix, iz in cells if iz == stair_row}
        & {ix for ix, iz in cells if iz == open_row}
    )
    if not cols:
        return None
    prefer_east = "west" in corridor_faces and "east" not in corridor_faces
    if prefer_east:
        return max(cols)
    return min(cols)  # default + corridor-on-east -> hug the west wall


def _mezz_stair_piece(x: float, z: float, yaw: float, base_y: float) -> dict:
    return {
        "stem": MEZZ_STAIR_STEM,
        "x": round(x, 4),
        "z": round(z, 4),
        "yaw": yaw,
        "y": round(base_y, 4),
        "floor_level": 0,
        "scale": SCALE,
        "kit": KIT,
        "role": "stairs",
        "tags": ["synth_mezz", "indoor_stairs"],
    }


def mezzanine_plan(
    command: "RoomInfo",
    corridor_ix: set[CellIx] | None = None,
) -> dict | None:
    """Geometry for the command-hall mezzanine (shared by generator + verifier).

    Single-flight loft: one ``stairs-small-center`` cell + a 2-row deck at the end of
    the hall farthest from corridor mouths. Returns None when the deck would touch a
    corridor or the room is too small.
    """
    corridor_ix = corridor_ix or set()
    cells = command.cells_ix
    ixs = [c[0] for c in cells]
    izs = [c[1] for c in cells]
    ix0, ix1 = min(ixs), max(ixs)
    iz0, iz1 = min(izs), max(izs)
    rows = iz1 - iz0 + 1
    cols = ix1 - ix0 + 1
    if rows < 3 or cols < 3:
        return None

    flights = 1
    deck_top = DECK_Y * (flights + 1)
    deck_origin = deck_top - DECK_Y
    corridor_faces = opens_to_corridor(cells, corridor_ix)

    # (touch, deck_rows, stair_row, open_row, open_sign, stair_col, deck_cells)
    candidates: list[tuple[int, set[int], int, int, int, int, list[CellIx]]] = []
    for at_north in (True, False):
        if at_north:
            # Deck hugs the north end; you step on at its south (room-facing) edge.
            deck_rows = {iz1 - 1, iz1}
            stair_row = iz1 - 2
            open_row = iz1 - 1
            open_sign = -1
        else:
            deck_rows = {iz0, iz0 + 1}
            stair_row = iz0 + 2
            open_row = iz0 + 1
            open_sign = 1
        if stair_row < iz0 or stair_row > iz1:
            continue
        deck_cells = [(ix, iz) for ix, iz in cells if iz in deck_rows]
        if len(deck_cells) < 2:
            continue
        stair_col = _pick_mezz_stair_col(cells, stair_row, open_row, corridor_faces)
        if stair_col is None:
            continue
        touch = _deck_adjacent_corridor(deck_cells, corridor_ix)
        if (stair_col, stair_row) in corridor_ix:
            touch += 10
        for dx, dz in ((0, -1), (0, 1), (1, 0), (-1, 0)):
            if (stair_col + dx, stair_row + dz) in corridor_ix:
                touch += 5
        candidates.append((touch, deck_rows, stair_row, open_row, open_sign, stair_col, deck_cells))

    if not candidates:
        return None
    candidates.sort(key=lambda t: t[0])
    touch, deck_rows, stair_row, open_row, open_sign, stair_col, deck_cells = candidates[0]
    if touch > 0:
        return None

    deck_back_row = min(deck_rows)
    deck_izs = sorted({iz for _, iz in deck_cells})
    stair_iz = stair_row
    parapet_wall, parapet_row = _mezz_parapet(stair_iz, deck_izs)
    rail_z = open_row * CELL + open_sign * FLOOR_HALF
    stairs = [(stair_col, stair_row, DECK_Y)]
    return {
        "flights": flights,
        "deck_top": deck_top,
        "deck_origin": deck_origin,
        "deck_cells": deck_cells,
        "stairs": stairs,
        "stair_col": stair_col,
        "rail_z": rail_z,
        "deck_rows": sorted(deck_rows),
        "deck_back_row": deck_back_row,
        "open_row": open_row,
        "open_sign": open_sign,
        "parapet_wall": parapet_wall,
        "parapet_row": parapet_row,
        "stair_row": stair_row,
        "stair_travel": _mezz_stair_travel(stair_iz, deck_izs),
        "ix0": ix0,
        "ix1": ix1,
        "iz0": iz0,
    }


def _mezz_upper_walls(
    command: RoomInfo,
    plan: dict,
    *,
    world_at: WorldAt | None,
    zone: str | None,
    corridor_ix: set[CellIx],
) -> list[dict]:
    """Second wall tier on the command room exterior shell.

    Stacked FLUSH on top of the base wall tier (zone deck 1.2 + 4 m wall =
    5.2), never overlapping it: the old deck_top (2.4) start left 2.8 m of
    coplanar double wall — z-fighting ("when you use double walls you should
    not overlap them; use full height with the 2 walls")."""
    out: list[dict] = []
    y = DECK_Y + 4.0
    cells = command.cells_ix
    for ix, iz in sorted(cells):
        x, z = _cell_world((ix, iz), world_at)
        for dx, dz in ((0, -1), (0, 1), (1, 0), (-1, 0)):
            nb = (ix + dx, iz + dz)
            if nb in cells:
                continue
            if nb in corridor_ix:
                continue
            wx = x + dx * 2.0
            wz = z + dz * 2.0
            wall = structure_piece("wall", wx, wz, wall_yaw(dx, dz))
            wall["y"] = y
            wall["tags"] = ["synth_mezz", "mezz_wall_upper"]
            if zone:
                wall["zone"] = zone
            out.append(wall)
    return out


def _mezz_loft_props(
    plan: dict,
    *,
    world_at: WorldAt | None,
    zone: str | None,
) -> list[dict]:
    """Console + storage on the outer deck row, flush to the parapet wall."""
    wall = plan["parapet_wall"]
    back_row = plan["parapet_row"]
    yaw = WALL_YAW_INTO[wall]
    back = sorted(c for c in plan["deck_cells"] if c[1] == back_row)
    if len(back) < 2:
        return []
    # Nudge against the FULL 2-row deck footprint, not the single parapet row —
    # a degenerate single-row span collapses the clamp and buries props in the wall.
    deck_w = [_cell_world(c, world_at) for c in plan["deck_cells"]]
    west = min(c[0] for c in deck_w)
    east = max(c[0] for c in deck_w)
    south = min(c[1] for c in deck_w)
    north = max(c[1] for c in deck_w)
    edge_z = _cell_world((back[0][0], back_row), world_at)[1]
    cx = _cell_world(back[len(back) // 2 - 1], world_at)[0]
    bx = _cell_world(back[len(back) // 2], world_at)[0]
    deck_top = plan["deck_top"]
    console = flush_back_to_wall("computer-screen", wall, cx, yaw, z=edge_z)
    console["y"] = deck_top
    console["tags"] = ["synth_mezz", "loft_ops"]
    console = nudge_prop_to_room(console, west, east, south, north)
    console["y"] = deck_top
    # Symmetric ``container`` (asymmetric ``container-tall`` pokes through the parapet).
    barrel = flush_back_to_wall("container", wall, bx, yaw, z=edge_z)
    barrel["y"] = deck_top
    barrel["tags"] = ["synth_mezz", "loft_storage"]
    barrel = nudge_prop_to_room(barrel, west, east, south, north)
    barrel["y"] = deck_top
    if zone:
        console["zone"] = zone
        barrel["zone"] = zone
    return [barrel, console]


def add_command_mezzanine(
    pieces: list[dict],
    room_infos: list[RoomInfo],
    *,
    world_at: WorldAt | None = None,
    zone: str | None = None,
    corridor_ix: set[CellIx] | None = None,
) -> list[dict]:
    """Elevated loft at the command hall end farthest from corridors — short stairs only."""
    command = next((i for i in room_infos if i.role == "command"), None)
    if command is None:
        return list(pieces)
    corridor_ix = corridor_ix or set()
    plan = mezzanine_plan(command, corridor_ix)
    if plan is None:
        return list(pieces)

    out = list(pieces)
    for ix, iz in plan["deck_cells"]:
        x, z = _cell_world((ix, iz), world_at)
        deck = structure_piece("floor", x, z)
        deck["y"] = plan["deck_origin"]
        deck["group_id"] = 50000 + (ix << 8) + iz
        deck["tags"] = ["synth_mezz", "mezz_floor"]
        if zone:
            deck["zone"] = zone
        out.append(deck)

    for ix, iz, base_y in plan["stairs"]:
        import transition_entrances as te
        import faction_profiles as fp

        cx, cz = _cell_world((ix, iz), world_at)
        deck_izs = sorted({diz for _, diz in plan["deck_cells"]})
        travel = _mezz_stair_travel(iz, deck_izs)
        tdx, tdz = te.DELTA[travel]
        # The short stair GLB is ~1.2 m deep, not a full cell: align its HIGH edge
        # with the deck-cell boundary (like _place_stair_at_cell does at zone seams)
        # instead of floating at the cell centre with a gap to the raised floor.
        high_ext, _ = fp.stair_ramp_footprint_m(MEZZ_STAIR_STEM, SCALE, KIT)
        sx = cx + tdx * (CELL * 0.5 - high_ext + te.STAIRS_SEAM_OVERLAP_M)
        sz = cz + tdz * (CELL * 0.5 - high_ext + te.STAIRS_SEAM_OVERLAP_M)
        yaw = _mezz_stair_yaw(iz, deck_izs)
        stair = _mezz_stair_piece(sx, sz, yaw, base_y)
        if zone:
            stair["zone"] = zone
        out.append(stair)

    out.extend(_mezz_upper_walls(command, plan, world_at=world_at, zone=zone, corridor_ix=corridor_ix))

    # Rail the open, room-facing deck edge (gap where the stair lands), not the
    # exterior wall side which already carries the upper wall tier.
    open_row = plan["open_row"]
    deck_set = set(plan["deck_cells"])
    _, rail_z = _cell_world((plan["stair_col"], open_row), world_at)
    # Inset by the rail's half thickness so it sits fully ON the deck tile
    # (centred on the edge leaves half the rail hanging over the open side).
    mezz_rail_half = bounds_scaled("rail")["z1"]
    rail_z = round(rail_z + plan["open_sign"] * (FLOOR_HALF - mezz_rail_half), 4)
    for ix in range(plan["ix0"], plan["ix1"] + 1):
        if ix == plan["stair_col"]:
            continue
        if (ix, open_row) not in deck_set:
            continue
        rx, _ = _cell_world((ix, open_row), world_at)
        rail = {
            "stem": "rail",
            "x": round(rx, 4),
            "z": rail_z,
            "yaw": 0.0,
            "floor_level": 0,
            "scale": SCALE,
            "kit": KIT,
            "y": plan["deck_top"],
            "role": "prop",
            "tags": ["synth_mezz", "mezz_rail"],
        }
        if zone:
            rail["zone"] = zone
        out.append(rail)

    out.extend(_mezz_loft_props(plan, world_at=world_at, zone=zone))
    return out


def furnish_showcase_plan(
    seed: int = 42,
) -> tuple[list[dict], list[RoomInfo], set[CellIx], set[CellIx]]:
    """Showcase wing: room roles + perimeter balconies + command mezzanine."""
    floor_ix, corridor_ix, rooms = generate_showcase_floor_plan(seed)
    roles = assign_roles(rooms, corridor_ix)
    room_infos = build_room_infos(rooms, corridor_ix, roles)
    pieces = build_shell(floor_ix, corridor_ix, rooms)
    rng = random.Random(seed ^ 0xA5A5)

    for info in room_infos:
        props = furnish_room(info.role, info.cells_w, info.cells_ix, corridor_ix, floor_ix, rng)
        for p in props:
            p["tags"] = ["synth_prop", "synth_interior", info.role, f"room_{info.room_id}"]
            p["role"] = "prop"
        pieces.extend(props)

    pieces = apply_perimeter_balconies(pieces, floor_ix, corridor_ix, rng)
    pieces = add_command_mezzanine(pieces, room_infos, corridor_ix=corridor_ix)
    return pieces, room_infos, floor_ix, corridor_ix


# --------------------------------------------------------------------------------------
# Quality scoring + metrics for automatic evaluation sweeps (see Automation Plan)
# These turn "looks good to human" into CPU-measurable numbers so we can sweep seeds,
# try param variants, and adapt (pick better densities, gaps, pairing rules) without
# asking for eyes on every trial.
# --------------------------------------------------------------------------------------

def count_workstation_pairs(pieces: list[dict], *, gap: float = PLACE_GAP) -> int:
    """Count computer-like props that have a correctly-oriented chair 'in front' within reasonable distance.
    This captures the desired relational cataloguing: screen + chair pair, yaw-correct, space-checked.
    """
    computers = [p for p in pieces if p.get("stem", "").startswith(("computer", "table-display"))]
    chairs = [p for p in pieces if p.get("stem", "").startswith("chair")]
    paired = 0
    for desk in computers:
        desk_bb = world_bbox(desk["stem"], desk["x"], desk["z"], desk["yaw"], desk.get("scale", SCALE))
        fx = round(math.sin(desk["yaw"]))
        fz = round(math.cos(desk["yaw"]))
        for ch in chairs:
            ch_bb = world_bbox(ch["stem"], ch["x"], ch["z"], ch["yaw"], ch.get("scale", SCALE))
            # Same rough column or row as desk front
            if fx != 0:
                if abs(ch["z"] - desk["z"]) > 2.5: continue
                expected_dir = 1 if fx > 0 else -1
                if abs(ch["x"] - (desk["x"] + expected_dir * (desk_bb.x1 - desk_bb.x0 + gap))) > 1.5: continue
            else:
                if abs(ch["x"] - desk["x"]) > 2.5: continue
                expected_dir = 1 if fz > 0 else -1
                if abs(ch["z"] - (desk["z"] + expected_dir * (desk_bb.z1 - desk_bb.z0 + gap))) > 1.5: continue
            # Yaw should face the desk (chair front toward desk)
            if abs((ch["yaw"] - desk["yaw"]) % (2*math.pi) - math.pi) > 0.6: continue
            paired += 1
            break
    return paired


def estimate_free_walk_fraction(pieces: list[dict], floor_cells: set, *, margin: float = 0.6) -> float:
    """Rough % of floor cells that remain reasonably clear of prop bboxes (player can walk).
    Used to detect 'swamped' layouts. Higher is better; target not to drop too low.
    """
    if not floor_cells:
        return 1.0
    occupied = set()
    for p in pieces:
        if p.get("role") not in ("prop", None) or "synth_mezz" in (p.get("tags") or []):
            continue
        try:
            bb = world_bbox(p["stem"], p["x"], p["z"], p["yaw"], p.get("scale", SCALE))
        except Exception:
            continue
        # Mark nearby cells
        cx, cz = p["x"], p["z"]
        for dx in (-1, 0, 1):
            for dz in (-1, 0, 1):
                occupied.add((round(cx/4) + dx, round(cz/4) + dz))
    free = len([c for c in floor_cells if c not in occupied])
    return free / max(1, len(floor_cells))


def _count_prop_overlaps(pieces: list[dict]) -> int:
    """Lightweight overlap detector (modeled on validate_props) for use in sweep scoring.
    Does not print or raise — just counts so sweep can apply heavy penalty and learn.
    """
    props: list[dict] = []
    for p in pieces:
        stem = p.get("stem", "")
        if stem.startswith(("floor", "wall")):
            continue
        if p.get("role") in ("floor", "wall", "stairs", "deck"):
            continue
        if p.get("role") == "prop":
            props.append(p)
            continue
        try:
            if stem_info(stem)["class"] not in ("structure",):
                props.append(p)
        except KeyError:
            if p.get("role") == "prop":
                props.append(p)

    boxes = [(p, world_bbox(p["stem"], p["x"], p["z"], p["yaw"], p.get("scale", SCALE))) for p in props]
    count = 0
    for i, (a, bb_a) in enumerate(boxes):
        tags_a = a.get("tags") or []
        if "synth_balcony" in tags_a or "synth_mezz" in tags_a:
            continue
        for b, bb_b in boxes[i + 1 :]:
            tags_b = b.get("tags") or []
            if "synth_balcony" in tags_b or "synth_mezz" in tags_b:
                continue
            if bb_a.padded(GAP_M).overlaps(bb_b.padded(GAP_M)):
                count += 1
    return count


def compute_quality_metrics(pieces: list[dict], room_infos: list[RoomInfo] | None = None,
                            floor_ix: set | None = None) -> dict:
    """Composite numbers for the self-evaluation loop. Used across seeds to score and adapt.
    Overlaps now produce heavy negative penalty so the search "sees" bad (too-small) gap choices.
    """
    pairs = count_workstation_pairs(pieces)
    free_frac = estimate_free_walk_fraction(pieces, floor_ix or set()) if floor_ix else 0.5
    n_props = sum(1 for p in pieces if p.get("role") == "prop")
    n_beds = sum(1 for p in pieces if p.get("stem", "").startswith("bed"))
    n_comps = sum(1 for p in pieces if p.get("stem", "").startswith("computer"))
    overlap_cnt = _count_prop_overlaps(pieces)
    # count filled rooms (non-empty) to encourage less "mostly empty"
    room_prop_counts = {}
    for p in pieces:
        if p.get("role") != "prop": continue
        for tag in p.get("tags", []):
            if tag.startswith("room_"):
                room_prop_counts[tag] = room_prop_counts.get(tag, 0) + 1
                break
    n_filled = sum(1 for c in room_prop_counts.values() if c > 0)
    # Simple composite (weights can be tuned by the sweep or human policy)
    score = (pairs * 2.0) + (free_frac * 10) + (n_props * 0.1) + (n_filled * 1.5) - (max(0, n_props - 25) * 0.5)
    if overlap_cnt > 0:
        score -= 500 * overlap_cnt   # heavy penalty as specified; lets optimizer learn to avoid overlaps
    return {
        "workstation_pairs": pairs,
        "free_walk_fraction": round(free_frac, 3),
        "num_props": n_props,
        "num_beds": n_beds,
        "num_computers": n_comps,
        "overlap_count": overlap_cnt,
        "num_filled_rooms": n_filled,
        "composite_score": round(score, 2),
    }
