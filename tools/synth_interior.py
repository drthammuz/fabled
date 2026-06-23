#!/usr/bin/env python3
"""Synth interior placement catalog + room-aware furnishing (editor + procgen)."""

from __future__ import annotations

import json
import math
import random
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable

ROOT = Path(__file__).resolve().parents[1]
CATALOG_PATH = ROOT / "assets" / "models" / "factions" / "synth" / "placement_catalog.json"
SCALE = 4.0
KIT = "factions/synth"
CELL = 4.0
HALF_PI = math.pi / 2
GAP_M = 0.35
PLACE_GAP = GAP_M * 2.0 + 0.1

CATALOG = json.loads(CATALOG_PATH.read_text(encoding="utf-8"))
WALL_FACE = CATALOG["wall_face_offset_m"]
WALL_T = CATALOG["wall_half_thickness_m"]

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


def chair_before_desk(chair_stem: str, desk: dict, desk_stem: str | None = None) -> dict:
    desk_stem = desk_stem or desk["stem"]
    desk_bb = world_bbox(desk_stem, desk["x"], desk["z"], desk["yaw"], desk["scale"])
    cz = desk_bb.z1 + 1.5
    yaw = look_at(chair_stem, desk["x"], cz, desk["x"], desk["z"])
    chair_offs = _world_corner_offsets(chair_stem, yaw, desk["scale"])
    cz = desk_bb.z1 + PLACE_GAP - min(wz for _, wz in chair_offs)
    return prop(chair_stem, desk["x"], cz, yaw=yaw)


def chairs_at_east_table(chair_stem: str, table: dict) -> list[dict]:
    tx, tz = table["x"], table["z"]
    scale = table["scale"]
    table_bb = world_bbox(table["stem"], tx, tz, table["yaw"], scale)
    z0, z1 = table_bb.z0, table_bb.z1

    north_z = z1 + 1.5
    yaw_n = look_at(chair_stem, tx, north_z, tx, tz)
    off_n = _world_corner_offsets(chair_stem, yaw_n, scale)
    north_z = z1 + PLACE_GAP - min(wz for _, wz in off_n)

    south_z = z0 - 1.5
    yaw_s = look_at(chair_stem, tx, south_z, tx, tz)
    off_s = _world_corner_offsets(chair_stem, yaw_s, scale)
    south_z = z0 - PLACE_GAP - max(wz for _, wz in off_s)

    return [
        prop(chair_stem, tx, north_z, yaw=yaw_n),
        prop(chair_stem, tx, south_z, yaw=yaw_s),
    ]


def cells_rect(ix0: int, iz0: int, ix1: int, iz1: int) -> set[CellIx]:
    return {(ix, iz) for ix in range(ix0, ix1 + 1) for iz in range(iz0, iz1 + 1)}


def ix_to_world(cell: CellIx) -> CellW:
    return cell[0] * CELL, cell[1] * CELL


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


def classify_room(room_ix: set[CellIx], corridor_ix: set[CellIx]) -> str:
    area, w, h = room_dimensions(room_ix)
    mouths = count_corridor_mouths(room_ix, corridor_ix)
    if mouths <= 1 and area <= 4:
        return "storage"
    if area <= 6 and w <= 3 and h <= 3:
        return "quarters"
    if area >= 16 and w >= 4 and h >= 3 and has_open_core(room_ix):
        return "mess"
    return "lab"


def assign_roles(rooms: dict[int, set[CellIx]], corridor_ix: set[CellIx]) -> dict[int, str]:
    roles: dict[int, str] = {}
    for rid, cells in rooms.items():
        roles[rid] = classify_room(cells, corridor_ix)
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


def setup_quarters(
    cells_w: set[CellW], cells_ix: set[CellIx], corridor_ix: set[CellIx], rng: random.Random
) -> list[dict]:
    """Bunk-verified bed pattern — matches ``bunk_furnished_c`` offsets exactly."""
    west, east, south, north, _, _ = _room_span(cells_w)
    area = len(cells_w)
    corridor = opens_to_corridor(cells_ix, corridor_ix)
    west_bed_z = south + CELL  # bunk west bed at z=-2 when south=-6
    out: list[dict] = []

    if "south" not in corridor:
        out.append(bed_origin_at_wall("south", west, south, 0.0))
    else:
        out.append(bed_origin_at_wall("west", west, west_bed_z, HALF_PI))

    if area >= 8 and "south" not in corridor and "west" not in corridor:
        out.append(bed_origin_at_wall("west", west, west_bed_z, HALF_PI))
        out.append(bed_origin_at_wall("south", west, south, 0.0))

    if area >= 4 and not (area <= 6 and "south" not in corridor):
        corner_stem = rng.choice(["container", "container-flat"])
        if "south" not in corridor:
            c = flush_back_to_wall(corner_stem, "east", east, -HALF_PI, z=south)
            out.append(nudge_prop_to_room(c, west, east, south, north))
        else:
            c = flush_back_to_wall(corner_stem, "east", east, -HALF_PI, z=north)
            out.append(nudge_prop_to_room(c, west, east, south, north))
    return out


def setup_lab(
    cells_w: set[CellW], cells_ix: set[CellIx], floor_ix: set[CellIx], rng: random.Random
) -> list[dict]:
    _, east, south, north, _, _ = _room_span(cells_w)
    depth = north - south
    out: list[dict] = []
    desk_stems = ["computer-screen", "computer-system", "computer-wide"]
    chair_stems = ["chair", "chair-armrest-headrest"]
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
    if depth >= CELL * 4:
        c = flush_back_to_wall("container-tall", "east", east, -HALF_PI, z=north)
        out.append(nudge_prop_to_room(c, *(_room_span(cells_w)[:4])))
    return out


def setup_office(
    cells_w: set[CellW], cells_ix: set[CellIx], floor_ix: set[CellIx], _: random.Random
) -> list[dict]:
    west, east, south, north, mid_x, _ = _room_span(cells_w)
    depth = north - south
    xs = _south_desk_xs(cells_ix, floor_ix, "computer-system")
    desk_x = xs[len(xs) // 2] if xs else mid_x
    desk = flush_back_to_wall("computer-system", "south", desk_x, 0.0, z=south)
    out = [
        desk,
        chair_before_desk("chair-armrest-headrest", desk),
    ]
    if depth >= CELL * 4:
        c = flush_back_to_wall("container-tall", "east", east, -HALF_PI, z=north)
        out.append(nudge_prop_to_room(c, west, east, south, north))
    elif depth >= CELL * 3:
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
    return [nudge_prop_to_room(c, west, east, south, north)]


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
) -> list[dict]:
    if role == "quarters":
        return setup_quarters(cells_w, cells_ix, corridor_ix, rng)
    if role == "lab":
        return setup_lab(cells_w, cells_ix, floor_ix, rng)
    if role == "command":
        if len(cells_w) >= 20:
            return setup_mess(cells_w, rng)
        return setup_office(cells_w, cells_ix, floor_ix, rng)
    if role == "storage":
        return setup_storage(cells_w, rng)
    if role == "mess":
        return setup_mess(cells_w, rng)
    return []


def validate_props(pieces: list[dict], name: str) -> list[str]:
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

    if errors:
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


def _room_exterior_faces(
    floor_ix: set[CellIx], corridor_ix: set[CellIx]
) -> list[tuple[int, int, str]]:
    """(ix, iz, face) for every room-cell face that borders a void cell."""
    faces: list[tuple[int, int, str]] = []
    for ix, iz in sorted(floor_ix):
        if (ix, iz) in corridor_ix:
            continue
        for face, (sx, sz) in FACE_STEPS.items():
            if (ix + sx, iz + sz) not in floor_ix:
                faces.append((ix, iz, face))
    return faces


def expected_balcony_floors(
    floor_ix: set[CellIx], corridor_ix: set[CellIx]
) -> dict[tuple[float, float], tuple[str, float]]:
    """Authoritative balcony floor layout: {(x, z): (stem, yaw)}.

    Single source of truth shared by the generator and the verifier so a passing
    audit guarantees the on-disk JSON matches this exact placement.
    """
    _, offset, _ = _balcony_geometry()
    faces = _room_exterior_faces(floor_ix, corridor_ix)
    out: dict[tuple[float, float], tuple[str, float]] = {}
    cell_faces: dict[CellIx, set[str]] = {}
    for ix, iz, face in faces:
        cell_faces.setdefault((ix, iz), set()).add(face)
        cx, cz = ix_to_world((ix, iz))
        ox, oz = OUTWARD[face]
        key = (round(cx + ox * offset, 1), round(cz + oz * offset, 1))
        out.setdefault(key, ("balcony-floor-center", FLOOR_EDGE_YAW[face]))
    for (ix, iz), fset in cell_faces.items():
        cx, cz = ix_to_world((ix, iz))
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
    floor_ix: set[CellIx], corridor_ix: set[CellIx]
) -> list[tuple[float, float, float, str]]:
    """Authoritative balcony rail layout: list of (x, z, yaw, stem).

    Rails follow the OUTER boundary of the *union* of ledge tiles. Tracing the union
    boundary (instead of one long rail per exterior face) means perpendicular runs meet
    at a butt joint at every corner — convex and concave — instead of two long rails
    overshooting and crossing (the inner-corner bug). Single source of truth for the
    generator and verifier.
    """
    ledge = expected_balcony_floors(floor_ix, corridor_ix)
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
        cx, cz = ix_to_world(cell)
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


def apply_perimeter_balconies(
    pieces: list[dict],
    floor_ix: set[CellIx],
    corridor_ix: set[CellIx],
    rng: random.Random,
    *,
    rate: float = 1.0,
) -> list[dict]:
    """Balcony ledge abutting every exterior room edge: floor tiles + corners + outer rail.

    The ledge inner edge meets the building floor edge (touches the building), tiles are
    4 m wide on 4 m centres (touch each other), and the open outer edge gets a rail.
    """
    _ = rate
    faces = _room_exterior_faces(floor_ix, corridor_ix)
    if not faces:
        return pieces

    # Drop exterior walls so each balcony edge reads as an open ledge.
    drop_walls: set[tuple[float, float, float]] = set()
    for ix, iz, face in faces:
        cx, cz = ix_to_world((ix, iz))
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
    for (bx, bz), (stem, yaw) in sorted(expected_balcony_floors(floor_ix, corridor_ix).items()):
        out.append(
            {
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
        )

    # Rails trace the outer boundary of the ledge union (see expected_balcony_rails):
    # perpendicular runs butt-join at corners instead of crossing.
    for rx, rz, ryaw, rstem in expected_balcony_rails(floor_ix, corridor_ix):
        out.append(
            {
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
        )
    return out


def _stair_piece(x: float, z: float, yaw: float, base_y: float) -> dict:
    return {
        "stem": "stairs",
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


def mezzanine_plan(command: "RoomInfo") -> dict | None:
    """Geometry for the command-hall mezzanine (shared by generator + verifier).

    Returns dict with: flights, deck_top, deck_origin, deck_cells [(ix,iz)],
    stairs [(ix,iz,base_y)], stair_col, rail_z, deck_rows. None if it won't fit.

    Dressing ground surface is DECK_Y (1.2). Each flight rises one DECK_Y and advances
    one cell toward -z; the top flight's high edge lands flush with the deck south edge.
    """
    cells = command.cells_ix
    ixs = [c[0] for c in cells]
    izs = [c[1] for c in cells]
    ix0, ix1 = min(ixs), max(ixs)
    iz0, iz1 = min(izs), max(izs)
    rows = iz1 - iz0 + 1
    cols = ix1 - ix0 + 1
    if rows < 4 or cols < 3:
        return None
    flights = max(1, min(2, rows - 2))  # deck takes 2 rows; stairs take `flights` rows
    deck_top = DECK_Y * (flights + 1)  # walkable surface of the deck (world Y)
    deck_origin = deck_top - DECK_Y  # floor block origin (block is DECK_Y tall)
    stair_col = ix0 + 1
    deck_rows = {iz0, iz0 + 1}

    deck_cells = [(ix, iz) for ix, iz in cells if iz in deck_rows]
    # Top flight at row iz0+2 (high edge meets deck south edge); each lower flight +1 row, -1 level.
    stairs = [(stair_col, iz0 + 2 + j, DECK_Y * (flights - j)) for j in range(flights)]
    rail_z = (iz0 + 1) * CELL + FLOOR_HALF  # deck south edge
    return {
        "flights": flights,
        "deck_top": deck_top,
        "deck_origin": deck_origin,
        "deck_cells": deck_cells,
        "stairs": stairs,
        "stair_col": stair_col,
        "rail_z": rail_z,
        "deck_rows": sorted(deck_rows),
        "ix0": ix0,
        "ix1": ix1,
        "iz0": iz0,
    }


def add_command_mezzanine(
    pieces: list[dict],
    room_infos: list[RoomInfo],
) -> list[dict]:
    """Elevated deck at the command hall's -z end, reached by a real advancing staircase."""
    command = next((i for i in room_infos if i.role == "command"), None)
    if command is None:
        return pieces
    plan = mezzanine_plan(command)
    if plan is None:
        return pieces

    out = list(pieces)
    for ix, iz in plan["deck_cells"]:
        x, z = ix_to_world((ix, iz))
        deck = structure_piece("floor", x, z)
        deck["y"] = plan["deck_origin"]
        deck["tags"] = ["synth_mezz", "mezz_floor"]
        out.append(deck)

    for ix, iz, base_y in plan["stairs"]:
        sx, sz = ix_to_world((ix, iz))
        out.append(_stair_piece(sx, sz, 0.0, base_y))  # yaw 0: climbs toward -z (the deck)
        # Fill the column under an elevated flight with floor blocks so it reads as a
        # solid staircase instead of a flight floating in mid-air.
        fill = DECK_Y
        while fill < base_y - 1e-3:
            block = structure_piece("floor", sx, sz)
            block["y"] = round(fill, 4)
            block["tags"] = ["synth_mezz", "mezz_stair_fill"]
            out.append(block)
            fill += DECK_Y

    # Rail along the deck's open south edge, except the stair column (the opening).
    for ix in range(plan["ix0"], plan["ix1"] + 1):
        if ix == plan["stair_col"]:
            continue
        rx = ix * CELL
        out.append(
            {
                "stem": "rail",
                "x": round(rx, 4),
                "z": round(plan["rail_z"], 4),
                "yaw": 0.0,
                "floor_level": 0,
                "scale": SCALE,
                "kit": KIT,
                "y": plan["deck_top"],
                "role": "prop",
                "tags": ["synth_mezz", "mezz_rail"],
            }
        )

    # Barrel + console on the back row of the deck (away from the stair column / edge).
    back = sorted([c for c in plan["deck_cells"] if c[1] == plan["iz0"] and c[0] != plan["stair_col"]])
    if len(back) >= 2:
        bx, bz = ix_to_world(back[len(back) // 2])
        cx, cz = ix_to_world(back[len(back) // 2 - 1])
        out.append(
            prop("container-tall", bx, bz, yaw=0.0, y=plan["deck_top"], tags=["synth_mezz", "loft_storage"])
        )
        out.append(
            prop("computer-screen", cx, cz, yaw=0.0, y=plan["deck_top"], tags=["synth_mezz", "loft_ops"])
        )
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
    pieces = add_command_mezzanine(pieces, room_infos)
    return pieces, room_infos, floor_ix, corridor_ix
