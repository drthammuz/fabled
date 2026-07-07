#!/usr/bin/env python3
"""
Unified faction interior generation framework.

Data-driven room furnishing for all factions using per-faction placement_catalog.json
(architecture / faction-folder items) and prop_catalog.json (curated dressing from
the faction's prop-kit: factory/, retro_fantasy/, etc.).

Faction distinctions (IMPORTANT — do not blend these):
  industrial   → factory props: machines, conveyors, screens, pipes, robot-arms, hoppers
  priesthood   → retro_fantasy props: columns, barrels, crates, bricks, ladders, fences
  necropolis   → graveyard props already in factions/necropolis/: graves, altars, coffins, candles
  urban/outlaw → retro_urban props already in factions/urban/: barriers, dumpsters, pallets, scaffolding
  synth        → delegated to synth_interior.py (computers, beds, containers, chairs)

Scale rules (kit-unit calibration — see probe_faction_catalog.KIT_BASE_SCALE):
  All Kenney kits here are 1-unit normalized (wall stems == 1.0 unit == one 4 m
  cell), so props take x4 — the same factor as the user-approved synth interior.
  The factory kit is authored chunkier (machine = 1.3 units) and takes x2.
  Every stem is then clamped by room height / cell footprint; the probed value
  lives in the catalog as per-stem "auto_scale" and is resolved via stem_scale().

Bench / seating yaw convention (Kenney front = +local Z @ yaw=0):
  south wall → yaw=0     (faces north = +z into room)
  north wall → yaw=π     (faces south = -z into room)
  west wall  → yaw=π/2   (faces east = +x into room)   ← was WRONG in prior code
  east wall  → yaw=3π/2  (faces west = -x into room)   ← was WRONG in prior code
"""

from __future__ import annotations

import json
import math
import random
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, Iterable, Optional, Set, Tuple, Dict, List

try:
    import synth_interior as si
    _HAS_SYNTH = True
except Exception:
    _HAS_SYNTH = False

ROOT = Path(__file__).resolve().parent.parent
FACTIONS_DIR = ROOT / "assets" / "models" / "factions"

# ---------------------------------------------------------------------------
# Globals — set per-faction call via set_faction_context()
# ---------------------------------------------------------------------------

CURRENT_FACTION = "synth"
CURRENT_KIT = "factions/synth"         # kit path for architecture items in this faction's folder
CURRENT_SCALE = 4.0                     # scale for architecture pieces
CURRENT_GAP_M = 0.35
CURRENT_PLACE_GAP = CURRENT_GAP_M * 2.0 + 0.1
CURRENT_CATALOG: Dict = {}             # probed catalog from factions/<id>/ folder
# Wall inner-surface protrusion past the cell-face line (probed; necropolis
# brick walls stick 0.6 m into the room — props must snap to the SURFACE).
CURRENT_WALL_OFFSET = 0.0

CURRENT_PROP_CATALOG: Dict = {}        # probed catalog from prop kit (factory/, retro_fantasy/)
CURRENT_PROP_KIT = ""                  # kit string for prop pieces (e.g. "factory")
CURRENT_PROP_SCALE = 1.0              # scale for prop pieces

FACTION_ID_MAP = {
    "outlaw": "urban",
    "urban": "urban",
    "industrial_default": "industrial",
    "industrial": "industrial",
}

FACTION_SCALES = {
    "synth": 4.0,
    "outlaw": 4.0,
    "urban": 4.0,
    "necropolis": 3.0,
    "priesthood": 1.0,
    "industrial": 1.0,
    "industrial_default": 1.0,
}

# Prop kit folders per faction (also declared in probe_faction_catalog.py)
PROP_KIT_MAP = {
    "industrial": "factory",
    "industrial_default": "factory",
    "priesthood": "retro_fantasy",
}

# Fallback base prop scale when a catalog carries no prop_scale/auto_scale.
# (Kit-unit calibration: 1-unit kits -> x4, factory -> x2.)
PROP_SCALE_DEFAULT = {
    "industrial": 2.0,
    "industrial_default": 2.0,
    "priesthood": 4.8,  # matches probe KIT_BASE_SCALE["retro_fantasy"]
    "necropolis": 3.0,
    "urban": 4.0,
    "outlaw": 4.0,
    "synth": 4.0,
}

# Clamps mirrored from probe_faction_catalog (used when auto_scale is absent).
MAX_PROP_HEIGHT_M = 3.4
MAX_PROP_FOOTPRINT_M = 3.6

# For priesthood columns specifically, scale=4 makes them imposing stone pillars.
PRIESTHOOD_STEM_SCALES = {
    "column": 4.0,
    "column-damaged": 4.0,
    "column-paint": 4.0,
    "column-wood": 4.0,
    "structure-pole": 4.0,
    "structure-wall": 2.0,
    "fence-wood": 2.0,
    "fence": 2.0,
}


def set_faction_context(faction: str, sweep_params: Optional["SweepParams"] = None):
    """Configure globals for a faction. Loads both placement_catalog and prop_catalog."""
    global CURRENT_FACTION, CURRENT_KIT, CURRENT_CATALOG, CURRENT_SCALE
    global CURRENT_GAP_M, CURRENT_PLACE_GAP, CURRENT_WALL_OFFSET
    global CURRENT_PROP_CATALOG, CURRENT_PROP_KIT, CURRENT_PROP_SCALE

    fid = FACTION_ID_MAP.get(faction, faction)
    CURRENT_FACTION = fid
    CURRENT_KIT = f"factions/{fid}"
    CURRENT_SCALE = FACTION_SCALES.get(fid, 1.0)
    CURRENT_CATALOG = {}
    CURRENT_PROP_CATALOG = {}
    CURRENT_PROP_KIT = CURRENT_KIT
    CURRENT_PROP_SCALE = PROP_SCALE_DEFAULT.get(fid, 1.0)

    cat_path = FACTIONS_DIR / fid / "placement_catalog.json"
    if cat_path.exists():
        try:
            CURRENT_CATALOG = json.loads(cat_path.read_text(encoding="utf-8"))
        except Exception:
            CURRENT_CATALOG = {}
    CURRENT_WALL_OFFSET = float(CURRENT_CATALOG.get("wall_inner_offset_m") or 0.0)

    # Load prop catalog from prop kit (factory/, retro_fantasy/, etc.)
    prop_cat_path = FACTIONS_DIR / fid / "prop_catalog.json"
    if prop_cat_path.exists():
        try:
            pc = json.loads(prop_cat_path.read_text(encoding="utf-8"))
            CURRENT_PROP_CATALOG = pc
            CURRENT_PROP_KIT = pc.get("prop_kit", CURRENT_KIT)
            CURRENT_PROP_SCALE = float(pc.get("prop_scale", CURRENT_PROP_SCALE))
        except Exception:
            pass

    if sweep_params:
        CURRENT_GAP_M = 0.35 * getattr(sweep_params, "gap_multiplier", 1.0)
    else:
        CURRENT_GAP_M = 0.35
    CURRENT_PLACE_GAP = CURRENT_GAP_M * 2.0 + 0.1


# ---------------------------------------------------------------------------
# Geometry primitives
# ---------------------------------------------------------------------------

@dataclass(frozen=True)
class BBox:
    x0: float
    x1: float
    z0: float
    z1: float

    def padded(self, m: float) -> "BBox":
        return BBox(self.x0 - m, self.x1 + m, self.z0 - m, self.z1 + m)

    def overlaps(self, other: "BBox") -> bool:
        return self.x0 < other.x1 and self.x1 > other.x0 and self.z0 < other.z1 and self.z1 > other.z0


def _catalog_bounds(stem: str, catalog: Dict) -> dict:
    """Look up bounds from catalog, fall back to small default."""
    entry = catalog.get("stems", {}).get(stem, {})
    return entry.get("bounds_scale1", {"x0": -0.5, "x1": 0.5, "z0": -0.5, "z1": 0.5})


def world_bbox(stem: str, x: float, z: float, yaw: float,
               scale: float = None, catalog: Optional[dict] = None) -> BBox:
    """Rotation-aware world bounding box for a placed prop."""
    if scale is None:
        scale = CURRENT_SCALE
    # Check both catalogs; prop catalog takes precedence for prop items
    if catalog is None:
        bounds = _catalog_bounds(stem, CURRENT_PROP_CATALOG)
        if bounds == {"x0": -0.5, "x1": 0.5, "z0": -0.5, "z1": 0.5}:
            bounds = _catalog_bounds(stem, CURRENT_CATALOG)
    else:
        bounds = _catalog_bounds(stem, catalog)
    corners = [
        (bounds["x0"] * scale, bounds["z0"] * scale),
        (bounds["x0"] * scale, bounds["z1"] * scale),
        (bounds["x1"] * scale, bounds["z0"] * scale),
        (bounds["x1"] * scale, bounds["z1"] * scale),
    ]
    cos_y, sin_y = math.cos(yaw), math.sin(yaw)
    wx_list, wz_list = [], []
    for lx, lz in corners:
        wx_list.append(x + lx * cos_y + lz * sin_y)
        wz_list.append(z - lx * sin_y + lz * cos_y)
    return BBox(min(wx_list), max(wx_list), min(wz_list), max(wz_list))


# ---------------------------------------------------------------------------
# Per-stem scale resolution (data-driven from probed catalogs)
# ---------------------------------------------------------------------------

def stem_scale(stem: str) -> float:
    """Effective placement scale for a prop stem.

    Prefers the probed per-stem ``auto_scale`` (kit base scale clamped by room
    height / cell footprint); falls back to computing the same clamp from the
    catalog bounds. Priesthood structural stems keep their explicit overrides
    (columns stay full 4 m floor-to-ceiling)."""
    if CURRENT_FACTION == "priesthood" and stem in PRIESTHOOD_STEM_SCALES:
        return PRIESTHOOD_STEM_SCALES[stem]
    fallback_base = PROP_SCALE_DEFAULT.get(CURRENT_FACTION, 1.0)
    for cat, base in ((CURRENT_PROP_CATALOG, CURRENT_PROP_SCALE),
                      (CURRENT_CATALOG, fallback_base)):
        entry = cat.get("stems", {}).get(stem)
        if not entry:
            continue
        auto = entry.get("auto_scale")
        if auto:
            return float(auto)
        h = float(entry.get("height_m", 0.0))
        footprint = 2.0 * max(float(entry.get("half_x_m", 0.5)),
                              float(entry.get("half_z_m", 0.5)))
        s = base
        if h > 1e-6:
            s = min(s, MAX_PROP_HEIGHT_M / h)
        if footprint > 1e-6:
            s = min(s, MAX_PROP_FOOTPRINT_M / footprint)
        return round(max(0.25, s), 3)
    return fallback_base


def item_scale(stem: str, p: dict) -> float:
    """Scale actually applied to a placed piece (piece value wins)."""
    return float(p.get("scale") or stem_scale(stem))


# ---------------------------------------------------------------------------
# Piece constructors
# ---------------------------------------------------------------------------

def prop(stem: str, x: float, z: float, *,
         yaw: float = 0.0, scale: float = None, tags: Optional[List[str]] = None,
         role: str = "prop") -> dict:
    """Piece from the faction's own folder (architecture kit items)."""
    if scale is None:
        scale = stem_scale(stem)
    return {
        "stem": stem, "x": round(x, 4), "z": round(z, 4),
        "yaw": yaw, "floor_level": 0, "scale": scale,
        "kit": CURRENT_KIT, "y": 0.05, "role": role,
        **({"tags": tags} if tags else {}),
    }


def prop_piece(stem: str, x: float, z: float, *,
               yaw: float = 0.0, scale: float = None) -> dict:
    """Piece from the faction's prop kit (factory/, retro_fantasy/, etc.).
    Scale defaults to the probed per-stem value (see stem_scale). Cross-kit
    extras (e.g. space-station skip/rocks) carry their own probed prop_kit."""
    if scale is None:
        scale = stem_scale(stem)
    entry = CURRENT_PROP_CATALOG.get("stems", {}).get(stem) or {}
    kit = entry.get("prop_kit") or CURRENT_PROP_KIT
    return {
        "stem": stem, "x": round(x, 4), "z": round(z, 4),
        "yaw": yaw, "floor_level": 0, "scale": scale,
        "kit": kit, "y": 0.05, "role": "prop",
    }


def _prop_bbox(p: dict) -> BBox:
    """BBox for a piece, checking both catalogs."""
    return world_bbox(p["stem"], p["x"], p["z"], p.get("yaw", 0.0), p.get("scale", CURRENT_PROP_SCALE))


# ---------------------------------------------------------------------------
# Room geometry helpers
# ---------------------------------------------------------------------------

CellIx = Tuple[int, int]
CellW = Tuple[float, float]

CELL = 4.0
# Room bounds returned by _room_span are the INNER usable area: cell centres
# extended by half a cell minus a small wall clearance. (Cell centres alone
# wasted a 2 m band all around and made wall-snapping impossible.)
ROOM_INNER_MARGIN = CELL / 2.0 - 0.3


@dataclass
class RoomInfo:
    room_id: int
    cells_ix: Set[CellIx] = field(default_factory=set)
    cells_w: Set[CellW] = field(default_factory=set)
    role: str = "storage"
    area: int = 0
    corridor_mouths: int = 0


# Per-room context set by furnish_faction_interior before each setup call:
#   open_sides:  sides ("north"...) where the room has ANY open face (corridor
#                mouth or zone seam) — wall-snapping avoids these sides.
#   mouth_rects: keep-clear BBoxes over corridor mouths and door bands so no
#                prop ever blocks a corridor opening or a door.
CURRENT_ROOM_CTX: Dict = {"open_sides": set(), "mouth_rects": []}


def _room_span(cells_w: Set[CellW]) -> Tuple[float, float, float, float, float, float]:
    xs = [c[0] for c in cells_w]
    zs = [c[1] for c in cells_w]
    if not xs:
        return 0, 0, 0, 0, 0, 0
    m = ROOM_INNER_MARGIN
    return (min(xs) - m, max(xs) + m, min(zs) - m, max(zs) + m,
            sum(xs) / len(xs), sum(zs) / len(zs))


def count_corridor_mouths(room_ix: Set[CellIx], corridor_ix: Set[CellIx]) -> int:
    return sum(
        1 for ix, iz in room_ix
        for dx, dz in ((0, -1), (0, 1), (1, 0), (-1, 0))
        if (ix + dx, iz + dz) in corridor_ix
    )


def room_dimensions(room_ix: Set[CellIx]) -> Tuple[int, int, int]:
    ixs = [c[0] for c in room_ix]
    izs = [c[1] for c in room_ix]
    if not ixs:
        return 0, 0, 0
    return len(room_ix), max(ixs) - min(ixs) + 1, max(izs) - min(izs) + 1


def opens_to_corridor(cells_ix: Set[CellIx], corridor_ix: Set[CellIx]) -> Set[str]:
    faces: Set[str] = set()
    for ix, iz in cells_ix:
        if (ix, iz - 1) in corridor_ix: faces.add("south")
        if (ix, iz + 1) in corridor_ix: faces.add("north")
        if (ix - 1, iz) in corridor_ix: faces.add("west")
        if (ix + 1, iz) in corridor_ix: faces.add("east")
    return faces


def nudge_prop_to_room(p: dict, west: float, east: float, south: float, north: float,
                       gap: float = None) -> dict:
    if gap is None:
        gap = CURRENT_PLACE_GAP * 0.5
    bb = _prop_bbox(p)
    dx = dz = 0.0
    if bb.x0 < west + gap:   dx = (west + gap) - bb.x0
    elif bb.x1 > east - gap:  dx = (east - gap) - bb.x1
    if bb.z0 < south + gap:  dz = (south + gap) - bb.z0
    elif bb.z1 > north - gap: dz = (north - gap) - bb.z1
    if dx or dz:
        p = {**p, "x": round(p["x"] + dx, 4), "z": round(p["z"] + dz, 4)}
    return p


# ---------------------------------------------------------------------------
# Wall alignment (FIXED yaw convention)
# ---------------------------------------------------------------------------

def _wall_yaw_and_offset(side: str, item: dict, wall_coord: float,
                          is_west_or_south: bool, scale: float = 1.0) -> Tuple[float, float]:
    """Return (yaw, coordinate_snapped_to_wall) for a piece against the given wall side.

    Convention (Kenney front = +local Z @ yaw=0):
      south wall: yaw=0    (face north = +z into room)
      north wall: yaw=π    (face south = -z into room)
      west wall:  yaw=π/2  (face east = +x into room)
      east wall:  yaw=3π/2 (face west = -x into room)

    Depth offset uses half_z_m (the front-to-back extent at yaw=0), because after
    rotation the depth always maps to the wall-normal direction.
    """
    depth = max(item.get("half_z_m", 0.3) * scale, 0.15)
    gap = 0.12  # small air gap between prop back and wall face
    yaw = _facing_yaw(side, item)
    if side == "south":
        return yaw, wall_coord + depth + gap
    elif side == "north":
        return yaw, wall_coord - depth - gap
    elif side == "west":
        return yaw, wall_coord + depth + gap
    else:  # east
        return yaw, wall_coord - depth - gap


def _maybe_align_to_wall(stem: str, p: dict,
                          west: float, east: float, south: float, north: float,
                          item: dict) -> dict:
    """Snap seating and storage props against the nearest wall with correct facing.

    Seating → faces INTO room (back against wall).
    Storage (pallets, crates, barrels, boxes) → also snapped with back to wall,
    facing into room so the contents are visible.
    """
    purpose = (item.get("purpose") or "").lower()
    usage = (item.get("usage") or "").lower()
    sl = stem.lower()
    is_seating = "seating" in purpose or "bench" in sl
    is_storage = ("storage" in purpose or "pallet" in sl or "barrel" in sl
                  or "crate" in sl or "box" in sl or "hopper" in sl)
    if not (is_seating or is_storage or "against wall" in usage):
        return p
    x, z = p["x"], p["z"]
    dists = sorted([("west", x - west), ("east", east - x),
                    ("south", z - south), ("north", north - z)],
                   key=lambda t: t[1])
    # Never snap against an open side (corridor mouth / zone seam): the "wall"
    # there may be a doorway. Fall back to the nearest closed side.
    open_sides = CURRENT_ROOM_CTX.get("open_sides", set())
    closed = [d for d in dists if d[0] not in open_sides]
    side = (closed[0] if closed else dists[0])[0]
    wall_coord = _wall_face(side, west, east, south, north)
    yaw, coord = _wall_yaw_and_offset(side, item, wall_coord, side in ("west", "south"),
                                      scale=item_scale(stem, p))
    p = dict(p)
    p["yaw"] = yaw % (2 * math.pi)
    if side in ("west", "east"):
        p["x"] = round(coord, 4)
    else:
        p["z"] = round(coord, 4)
    return p


# ---------------------------------------------------------------------------
# Overlap-safe batch placer
# ---------------------------------------------------------------------------

class _RoomPlacer:
    """Manages placed bboxes for a room to prevent overlaps."""

    def __init__(self, west: float, east: float, south: float, north: float,
                 gap: float = None):
        self.west, self.east, self.south, self.north = west, east, south, north
        self.gap = gap if gap is not None else CURRENT_PLACE_GAP
        # Corridor mouths and door bands are pre-seeded as phantom boxes so no
        # prop can ever block an opening (they are not emitted as pieces).
        self.bbs: List[BBox] = list(CURRENT_ROOM_CTX.get("mouth_rects", []))
        self.out: List[dict] = []

    def try_add(self, p: dict, extra_gap: float = 0.0, nudge: bool = True) -> bool:
        if nudge:
            p = nudge_prop_to_room(p, self.west, self.east, self.south, self.north)
        bb = _prop_bbox(p)
        pad = self.gap + extra_gap
        if any(bb.padded(pad).overlaps(ob) for ob in self.bbs):
            return False
        self.bbs.append(bb)
        self.out.append(p)
        return True

    def add_unchecked(self, p: dict):
        p = nudge_prop_to_room(p, self.west, self.east, self.south, self.north)
        self.bbs.append(_prop_bbox(p))
        self.out.append(p)

    def add_fixed(self, p: dict):
        """Exact placement: no room-nudge (deliberately composed pieces —
        pipe/conveyor segments, row members — must keep their spacing)."""
        self.bbs.append(_prop_bbox(p))
        self.out.append(p)

    def pieces(self) -> List[dict]:
        return self.out


# ---------------------------------------------------------------------------
# Room role classifiers
# ---------------------------------------------------------------------------

def classify_priesthood_room(room_ix: Set[CellIx], corridor_ix: Set[CellIx]) -> str:
    area, w, h = room_dimensions(room_ix)
    mouths = count_corridor_mouths(room_ix, corridor_ix)
    if area >= 6 and w >= 2 and h >= 2 and mouths <= 2:
        return "chapel"
    if area <= 8 and mouths == 1:
        return "ruins"
    return "storage"


def classify_necropolis_room(room_ix: Set[CellIx], corridor_ix: Set[CellIx]) -> str:
    area, w, h = room_dimensions(room_ix)
    if area >= 8 and (w >= 2 or h >= 4):
        return "crypt"
    if area >= 5:
        return "chapel"
    return "debris"


def classify_urban_room(room_ix: Set[CellIx], corridor_ix: Set[CellIx]) -> str:
    area, w, h = room_dimensions(room_ix)
    mouths = count_corridor_mouths(room_ix, corridor_ix)
    if area >= 6 and mouths <= 2:
        return "breakroom" if (abs(w - h) < 2) else "storage"
    return "storage"


def classify_industrial_room(room_ix: Set[CellIx], corridor_ix: Set[CellIx]) -> str:
    area, w, h = room_dimensions(room_ix)
    if area >= 4:
        return "factory_floor"
    return "utility"


FACTION_CLASSIFIERS = {
    "priesthood": classify_priesthood_room,
    "necropolis": classify_necropolis_room,
    "urban": classify_urban_room,
    "industrial_default": classify_industrial_room,
    "industrial": classify_industrial_room,
}


def assign_faction_roles(rooms: Dict[int, Set[CellIx]],
                         corridor_ix: Set[CellIx], faction: str) -> Dict[int, str]:
    classifier = FACTION_CLASSIFIERS.get(faction, lambda r, c: "storage")
    roles = {rid: classifier(cells, corridor_ix) for rid, cells in rooms.items()}
    if rooms:
        largest = max(rooms, key=lambda r: len(rooms[r]))
        overrides = {
            "priesthood": "chapel",
            "necropolis": "crypt",
            "industrial": "factory_floor",
            "industrial_default": "factory_floor",
        }
        if faction in overrides:
            roles[largest] = overrides[faction]
    return roles


# ---------------------------------------------------------------------------
# Faction-specific room setup functions
# ---------------------------------------------------------------------------

# ── INDUSTRIAL ──────────────────────────────────────────────────────────────

# Probed factory pipe facts @ scale 1 (see pipe port probe): tube runs along
# local X, ports at the bbox X faces, tube centre at y=0.5, diameter 1.0 unit.
# pipe-large-long is exactly 2 units (= one 4 m cell at scale 2), so consecutive
# segments at cell-centre spacing connect port-to-port with no seam.
# pipe-large-junction is the same 2-unit straight PLUS a vertical branch whose
# top sits at y=2.056 units — at scale 2 that is ~4.11 m, i.e. it meets the
# roof. A floor run with one junction reads as a trunk line teeing into the roof.
_PIPE_SCALE = 2.0
_PIPE_RADIUS_M = 0.5 * _PIPE_SCALE
_PIPE_SEG_M = 2.0 * _PIPE_SCALE       # pipe-large-long length at scale 2
_ROOM_CLEAR_H = 4.0
_HALL_EXTRA_H = 4.25                  # extra wall tier of double-height halls
                                      # (gen_freeform.WALL_TIER_H — keep in sync)

# Catwalk pieces (factory kit, probed): deck plane at local y=0, rails ±z at
# ±0.6277, understructure to −0.147; stairs span 2 units along local x rising
# +x from 0 to 1.4. One shared scale keeps stairs-top == deck height.
_CW_SCALE = 2.0
_CW_DECK_H = 1.4 * _CW_SCALE          # walking height of the deck (2.8 m)
_CW_HALF_W = 0.6277 * _CW_SCALE       # deck+rail half width
_CW_SEG = 1.0 * _CW_SCALE             # catwalk-straight length along the walk axis
_CW_STAIR_LEN = 2.0 * _CW_SCALE       # stairs footprint along the walk axis
_CW_STAIR_HIGH = 1.5 * _CW_SCALE      # stairs origin → high edge
_CW_BASE_Y = 0.05                     # prop_piece default floor lift


def _closed_sides() -> List[str]:
    open_sides = CURRENT_ROOM_CTX.get("open_sides", set())
    return [s for s in ("south", "north", "west", "east") if s not in open_sides]


def _wall_face(side: str, west: float, east: float, south: float, north: float) -> float:
    """World coordinate of the wall's inner SURFACE on the given side.

    Span bounds ±0.3 give the cell-face line; CURRENT_WALL_OFFSET shifts into
    the room for kits whose wall slab protrudes past the face (necropolis
    brick: +0.6 m) so props lean on the visible surface, not the face line."""
    off = CURRENT_WALL_OFFSET
    return {"west": west - 0.3 + off, "east": east + 0.3 - off,
            "south": south - 0.3 + off, "north": north + 0.3 - off}[side]


_INTO_ROOM_YAW = {"south": 0.0, "north": math.pi,
                  "west": math.pi / 2, "east": 3 * math.pi / 2}


def _front_flip(item: dict) -> float:
    """π when the probed front axis is -z (screens, benches whose bulk —
    mount / backrest — sits at +z), so 'facing into the room' is honoured."""
    return math.pi if (item or {}).get("front") == "-z" else 0.0


def _facing_yaw(side: str, item: dict) -> float:
    return (_INTO_ROOM_YAW[side] + _front_flip(item)) % (2 * math.pi)


def _snap_to_side(stem: str, along: float, side: str,
                  west: float, east: float, south: float, north: float,
                  item: dict, *, gap: float = 0.12) -> dict:
    """Prop with its back against `side`, facing into the room, scale-aware."""
    scale = stem_scale(stem)
    depth = max(item.get("half_z_m", 0.3) * scale, 0.15)
    face = _wall_face(side, west, east, south, north)
    yaw = _facing_yaw(side, item)
    if side in ("west", "east"):
        x = face + depth + gap if side == "west" else face - depth - gap
        return prop_piece(stem, x, along, yaw=yaw)
    z = face + depth + gap if side == "south" else face - depth - gap
    return prop_piece(stem, along, z, yaw=yaw)


# For factions without a separate prop kit (necropolis, urban), prop_piece
# resolves to the faction folder anyway — same behaviour, keep one impl.
_snap_to_side_faction = _snap_to_side


def _emit_pipe_run(pl: "_RoomPlacer", side: str,
                   west: float, east: float, south: float, north: float,
                   rng, cat_stems: dict) -> int:
    """Wall-hugging pipe trunk spanning the full room, ends embedded in the
    perpendicular walls. Falls back to a roof-level run when the floor route
    would block an opening. Returns number of pieces placed."""
    if "pipe-large-long" not in cat_stems:
        return 0
    # Pipe material family per run: weathered metal or glass (same footprint —
    # the kit shares stem geometry between `pipe-large-*` and `pipe-glass-large-*`).
    family = "pipe-large"
    if "pipe-glass-large-long" in cat_stems and rng.random() < 0.4:
        family = "pipe-glass-large"
    horiz = side in ("south", "north")
    lo = _wall_face("west" if horiz else "south", west, east, south, north)
    hi = _wall_face("east" if horiz else "north", west, east, south, north)
    span = hi - lo
    n = int(round(span / _PIPE_SEG_M))
    if n < 1 or abs(span - n * _PIPE_SEG_M) > 0.25:
        return 0
    face = _wall_face(side, west, east, south, north)
    # Tube centre offset: radius from the wall face (slightly embedded).
    off = face + (_PIPE_RADIUS_M - 0.05) * (1 if side in ("south", "west") else -1)
    yaw = 0.0 if horiz else math.pi / 2
    # One special mid-run segment: junction (drops a stub) or an inline valve.
    special = None
    if n >= 3 and rng.random() < 0.7:
        specials = [s for s in (f"{family}-junction", f"{family}-valve") if s in cat_stems]
        if specials:
            special = rng.choice(specials)
    j_index = rng.randrange(1, n - 1) if special else -1

    def seg_pieces(y: Optional[float]) -> List[dict]:
        segs = []
        for i in range(n):
            a = lo + _PIPE_SEG_M * (i + 0.5)
            stem = special if (i == j_index and y is None) else f"{family}-long"
            if stem and stem.endswith("-valve"):
                # Valve tube is 1 kit unit vs the 2-unit long segment: pair it
                # with a 1-unit plain pipe so the slot stays gap-free.
                pair = [family, stem] if rng.random() < 0.5 else [stem, family]
                sub = [(a - 0.5 * _PIPE_SCALE, pair[0]),
                       (a + 0.5 * _PIPE_SCALE, pair[1])]
            else:
                sub = [(a, stem)]
            for sa, sstem in sub:
                x, z = (sa, off) if horiz else (off, sa)
                p = prop_piece(sstem, x, z, yaw=yaw, scale=_PIPE_SCALE)
                if y is not None:
                    p["y"] = y
                segs.append(p)
        return segs

    # Floor run first; if any segment hits a keep-clear box, roof run instead.
    floor_segs = seg_pieces(None)
    if all(not _prop_bbox(p).padded(0.05).overlaps(ob) for p in floor_segs for ob in pl.bbs):
        for p in floor_segs:
            pl.add_fixed(p)
        return len(floor_segs)
    roof_y = _ROOM_CLEAR_H - 2.0 * _PIPE_RADIUS_M - 0.05
    if CURRENT_ROOM_CTX.get("tall"):
        # Double-height hall: lift the roof run a full tier so it never crosses
        # the catwalk deck (2.8 m) embedded in the perpendicular walls.
        roof_y += _HALL_EXTRA_H
    roof_segs = seg_pieces(roof_y)
    # A pipe must END embedded in a solid wall, never in a doorway or corridor
    # mouth — reject the run if either END segment touches a keep-clear rect
    # (mid-run crossings above openings are fine).
    rects = CURRENT_ROOM_CTX.get("mouth_rects", [])
    for p in (roof_segs[0], roof_segs[-1]):
        if any(_prop_bbox(p).overlaps(r) for r in rects):
            return 0
    for p in roof_segs:
        # Roof runs block nothing at floor level — bypass overlap checks and
        # do NOT register their bbox (floor props may sit beneath).
        pl.out.append(p)
    return n


def _emit_catwalk(pl: "_RoomPlacer", side: str,
                  west: float, east: float, south: float, north: float,
                  rng, cat_stems: dict) -> bool:
    """Elevated catwalk tier along a closed wall of a double-height hall.

    Straight deck run at 2.8 m hugging the wall, far end embedded in the
    perpendicular wall, one catwalk-stairs flight down to the floor at the
    other end (walkable — pieces bake trimesh colliders server-side). The
    stairs footprint + landing register as floor obstacles; the deck itself is
    overhead and blocks nothing at floor level. Returns False when neither
    stair end keeps clear of corridor mouths."""
    if "catwalk-straight" not in cat_stems or "catwalk-stairs" not in cat_stems:
        return False
    horiz = side in ("south", "north")
    lo = _wall_face("west" if horiz else "south", west, east, south, north)
    hi = _wall_face("east" if horiz else "north", west, east, south, north)
    span = hi - lo
    # Leave ≥1.6 m walk-off gap past the stairs' low edge so you never step off
    # the last step straight into the perpendicular wall.
    n = int((span - _CW_STAIR_LEN - 1.6) // _CW_SEG)
    if n < 2:
        return False
    face = _wall_face(side, west, east, south, north)
    sign = 1 if side in ("south", "west") else -1
    off = face + sign * (_CW_HALF_W - 0.06)   # wall-side rail kisses the wall face
    yaw_run = 0.0 if horiz else math.pi / 2
    deck_y = _CW_BASE_Y + _CW_DECK_H

    for stair_at_hi in rng.sample([True, False], 2):
        if stair_at_hi:
            deck_end = lo + n * _CW_SEG       # deck grows from the lo wall
            s_dir = 1.0                       # stairs descend toward +axis
        else:
            deck_end = hi - n * _CW_SEG
            s_dir = -1.0
        # Stairs ascend toward local +x; local +x must point back at the deck
        # (world −axis·s_dir). Bevy yaw: +x→−z at π/2, +x→−x at π.
        if horiz:
            yaw_st = math.pi if s_dir > 0 else 0.0
        else:
            yaw_st = math.pi / 2 if s_dir > 0 else 3 * math.pi / 2
        s_origin = deck_end + s_dir * _CW_STAIR_HIGH
        sx, sz = (s_origin, off) if horiz else (off, s_origin)
        stair = prop_piece("catwalk-stairs", sx, sz, yaw=yaw_st, scale=_CW_SCALE)
        s_bb = _prop_bbox(stair)
        # Landing keep-clear past the low edge (deck_end + 4·s_dir).
        land_lo = deck_end + s_dir * _CW_STAIR_LEN
        land_hi = land_lo + s_dir * 1.6
        a0, a1 = min(land_lo, land_hi), max(land_lo, land_hi)
        land_bb = (BBox(a0, a1, off - 1.3, off + 1.3) if horiz
                   else BBox(off - 1.3, off + 1.3, a0, a1))
        if any(s_bb.padded(pl.gap).overlaps(ob) or land_bb.overlaps(ob)
               for ob in pl.bbs):
            continue
        decks = []
        for i in range(n):
            a = (lo if stair_at_hi else hi) + s_dir * _CW_SEG * (i + 0.5)
            x, z = (a, off) if horiz else (off, a)
            p = prop_piece("catwalk-straight", x, z, yaw=yaw_run, scale=_CW_SCALE)
            p["y"] = round(deck_y, 3)
            decks.append(p)
        # Wall-embedded far end must not hover over a doorway near the corner.
        rects = CURRENT_ROOM_CTX.get("mouth_rects", [])
        if any(_prop_bbox(decks[0]).overlaps(r) for r in rects):
            continue
        pl.bbs.append(s_bb)
        pl.bbs.append(land_bb)
        pl.out.append(stair)
        for p in decks:
            pl.out.append(p)   # overhead: no floor-level bbox
        return True
    return False


_TRUNK_SEG = 2.0                      # pipe-large-long = 2 units at scale 1
_TRUNK_Y = 2.6                        # tube spans 2.6–3.6 m: rests on machine
                                      # tops (2.6) and passes through the
                                      # machine-connection-pipe housing (top 3.4)


def _emit_bank_trunk(pl: "_RoomPlacer", side: str,
                     west: float, east: float, south: float, north: float,
                     rng, cat_stems: dict) -> int:
    """Overhead pipe trunk above a machine bank: wall-to-wall along the bank's
    wall over the machine centre line, both ends dying into the perpendicular
    walls (pipes never end in air or over a doorway). Makes the bank read as
    one fed processing line."""
    if "pipe-large-long" not in cat_stems:
        return 0
    horiz = side in ("south", "north")
    lo = _wall_face("west" if horiz else "south", west, east, south, north)
    hi = _wall_face("east" if horiz else "north", west, east, south, north)
    span = hi - lo
    n = int(round(span / _TRUNK_SEG))
    if n < 2 or abs(span - n * _TRUNK_SEG) > 0.25:
        return 0
    face = _wall_face(side, west, east, south, north)
    # Machine centres sit at face + half_z(1.5) + snap gap — run right above them.
    off = face + 1.62 * (1 if side in ("south", "west") else -1)
    yaw = 0.0 if horiz else math.pi / 2
    segs: List[dict] = []
    for i in range(n):
        a = lo + _TRUNK_SEG * (i + 0.5)
        x, z = (a, off) if horiz else (off, a)
        p = prop_piece("pipe-large-long", x, z, yaw=yaw, scale=1.0)
        p["y"] = _TRUNK_Y
        segs.append(p)
    rects = CURRENT_ROOM_CTX.get("mouth_rects", [])
    for p in (segs[0], segs[-1]):
        if any(_prop_bbox(p).overlaps(r) for r in rects):
            return 0
    for p in segs:
        pl.out.append(p)   # overhead: floor props may sit beneath
    return n


def setup_industrial_factory_floor(cells_w, cells_ix, rng):
    """Factory floor: pipe trunk line + machine row + conveyor line + screens.

    Factory kit at scale 2 (kit-unit calibration); every offset derives from
    probed bounds so items sit flush against walls and lines stay connected.
    """
    west, east, south, north, mid_x, mid_z = _room_span(cells_w)
    room_w = east - west
    room_h = north - south
    pl = _RoomPlacer(west, east, south, north)
    cat_stems = CURRENT_PROP_CATALOG.get("stems", {})
    closed = _closed_sides()
    rng.shuffle(closed)

    # ⓪ Double-height hall: catwalk tier FIRST along the longest closed wall.
    # That wall then drops out of the rotation — banks/screens/pipes there
    # would clip the deck overhead (machine-connection-pipe tops at 3.4 m).
    if CURRENT_ROOM_CTX.get("tall") and closed:
        cw_side = max(closed,
                      key=lambda s: room_w if s in ("south", "north") else room_h)
        if _emit_catwalk(pl, cw_side, west, east, south, north, rng, cat_stems):
            closed = [s for s in closed if s != cw_side]

    # ① Pipe trunk along a closed wall (or roof), wall-to-wall. Try each
    # closed side until one has solid walls at both ends (no doorway ends).
    for side in closed:
        if _emit_pipe_run(pl, side, west, east, south, north, rng, cat_stems):
            break

    # ② Machine BANK against another closed wall — the kit's machines are
    # modules designed to butt together into one processing line, not
    # scattered singles. Pieces are laid flush edge-to-edge, centred on the
    # wall, checked only against PRE-existing boxes (they touch each other
    # by design) and added as a unit.
    machine_stems = [s for s in ("machine", "machine-window", "machine-fortified",
                                 "machine-connection-pipe") if s in cat_stems] or ["machine"]
    if len(closed) > 1:
        side = closed[1]
        span_lo, span_hi = (west, east) if side in ("south", "north") else (south, north)
        span = span_hi - span_lo
        def _stem_w(stem: str) -> float:
            item = cat_stems.get(stem, {})
            return 2.0 * float(item.get("half_x_m", 0.5)) * stem_scale(stem)

        # A processing line reads as ONE fed chain: a tall feed hopper at one
        # end (width reserved up front) + a guaranteed pipe-port module.
        hop = next((s for s in ("hopper-high-square", "hopper-high-round")
                    if s in cat_stems), None)
        if hop and rng.random() >= 0.75:
            hop = None
        budget = span - 1.6 - (_stem_w(hop) if hop else 0.0)
        bank: List[Tuple[str, float]] = []
        while len(bank) < 4:
            stem = rng.choice(machine_stems)
            w = _stem_w(stem)
            if sum(bw for _, bw in bank) + w > budget:
                break
            bank.append((stem, w))
        if len(bank) >= 2 and "machine-connection-pipe" in cat_stems \
                and all(s != "machine-connection-pipe" for s, _ in bank):
            i = rng.randrange(len(bank))
            w = _stem_w("machine-connection-pipe")
            rest = sum(bw for j, (_, bw) in enumerate(bank) if j != i)
            if rest + w <= budget:
                bank[i] = ("machine-connection-pipe", w)
        if len(bank) >= 2 and hop:
            bank.insert(rng.choice([0, len(bank)]), (hop, _stem_w(hop)))
        if len(bank) >= 2:
            total = sum(bw for _, bw in bank)
            mid = (span_lo + span_hi) / 2.0
            a = mid - total / 2.0
            pieces_bank = []
            prior_bbs = list(pl.bbs)
            ok_bank = True
            for stem, w in bank:
                item = cat_stems.get(stem, {})
                p = _snap_to_side(stem, a + w / 2.0, side,
                                  west, east, south, north, item)
                bb = _prop_bbox(p)
                if any(bb.padded(0.02).overlaps(ob) for ob in prior_bbs):
                    ok_bank = False
                    break
                pieces_bank.append(p)
                a += w
            if ok_bank:
                for p in pieces_bank:
                    pl.add_fixed(p)
                # Overhead trunk ties the line together via the pipe-port module.
                if any(s == "machine-connection-pipe" for s, _ in bank):
                    _emit_bank_trunk(pl, side, west, east, south, north,
                                     rng, cat_stems)

    # ③ Conveyor line through the open floor, segments connected end-to-end.
    conv_stems = [s for s in ("conveyor", "conveyor-stripe", "conveyor-sides")
                  if s in cat_stems]
    if conv_stems and max(room_w, room_h) >= 6.0:
        conv = rng.choice(conv_stems)
        c_scale = stem_scale(conv)
        seg = 1.0 * c_scale  # conveyor is 1 unit long at scale 1
        horiz = room_w >= room_h
        span = (room_w if horiz else room_h) - 2.4
        n_conv = max(2, min(6, int(span / seg)))
        total = n_conv * seg
        start = (mid_x - total / 2 if horiz else mid_z - total / 2) + seg / 2
        # Offset the line off-centre so the room centre stays walkable.
        lat = (mid_z + rng.choice([-1.2, 1.2])) if horiz else (mid_x + rng.choice([-1.2, 1.2]))
        yaw = math.pi / 2 if horiz else 0.0  # belt travel along the line
        line: List[dict] = []
        ok = True
        for i in range(n_conv):
            a = start + i * seg
            x, z = (a, lat) if horiz else (lat, a)
            p = prop_piece(conv, x, z, yaw=yaw)
            bb = _prop_bbox(p)
            if any(bb.padded(0.05).overlaps(ob) for ob in pl.bbs):
                ok = False
                break
            line.append(p)
        if ok and len(line) >= 2:
            # Terminate the belt with a machine at each end (feeder/receiver)
            # so conveyor lines never start or end in thin air. If an end is
            # blocked (wall/opening), give up one belt segment and retry so
            # the machine takes the freed slot.
            end_stems = [s for s in ("machine-connection-pipe", "machine",
                                     "machine-window", "hopper-square")
                         if s in cat_stems]
            machines: List[dict] = []
            lo_i, hi_i = 0, len(line) - 1
            for sign in (-1.0, 1.0) if end_stems else ():
                for _attempt in range(2):
                    if hi_i - lo_i + 1 < 2:
                        break
                    edge = line[lo_i] if sign < 0 else line[hi_i]
                    stem = rng.choice(end_stems)
                    item = cat_stems.get(stem, {})
                    s = stem_scale(stem)
                    half_depth = float(item.get("half_z_m", 0.5)) * s
                    a = (edge["x"] if horiz else edge["z"]) + sign * (seg / 2 + half_depth + 0.05)
                    x, z = (a, lat) if horiz else (lat, a)
                    # Front faces the belt (toward -sign along the line axis).
                    base_yaw = (-sign * math.pi / 2) if horiz else (math.pi if sign > 0 else 0.0)
                    mp = prop_piece(stem, x, z, yaw=base_yaw + _front_flip(item))
                    bb = _prop_bbox(mp)
                    inside = (bb.x0 >= west - 0.05 and bb.x1 <= east + 0.05
                              and bb.z0 >= south - 0.05 and bb.z1 <= north + 0.05)
                    # Flush against the belt by design: tiny pad (the standard
                    # try_add gap would always collide with the belt itself).
                    if inside and not any(bb.padded(0.02).overlaps(ob) for ob in pl.bbs):
                        machines.append(mp)
                        break
                    if sign < 0:
                        lo_i += 1
                    else:
                        hi_i -= 1
            line = line[lo_i:hi_i + 1]
            if len(line) >= 2:
                for p in line:
                    pl.add_fixed(p)
                for mp in machines:
                    pl.add_fixed(mp)

    # ④ Screens on a third closed wall (info displays facing the room).
    screen_stems = [s for s in ("screen-wide", "screen-flat", "screen-panel-wide",
                                "screen-panel-flat") if s in cat_stems]
    if screen_stems and len(closed) > 2:
        side = closed[2]
        span_lo, span_hi = (west, east) if side in ("south", "north") else (south, north)
        n_scr = max(1, min(2, int((span_hi - span_lo) / 5.0)))
        step = (span_hi - span_lo) / (n_scr + 1)
        for i in range(n_scr):
            stem = rng.choice(screen_stems)
            pl.try_add(_snap_to_side(stem, span_lo + step * (i + 1), side,
                                     west, east, south, north, cat_stems.get(stem, {})))

    # ⑤ Storage corner: hoppers + boxes + skips/rubble by a closed-wall corner.
    store_stems = [s for s in ("hopper-round", "hopper-square", "box-large", "box-wide",
                               "skip", "skip-rocks", "rocks")
                   if s in cat_stems]
    if store_stems and closed:
        cx = west + 1.2 if "west" in closed else east - 1.2
        cz = south + 1.2 if "south" in closed else north - 1.2
        for i in range(rng.randint(1, 3)):
            stem = rng.choice(store_stems)
            s = stem_scale(stem)
            jx = cx + rng.uniform(-0.4, 1.4) * s * 0.5
            jz = cz + rng.uniform(-0.4, 1.4) * s * 0.5
            pl.try_add(_loose(prop_piece(stem, jx, jz,
                                         yaw=rng.choice([0, math.pi / 2, math.pi, 3 * math.pi / 2]))))

    # ⑥ Accent machinery: robot arm / scanner near the machine row.
    accent = [s for s in ("robot-arm-a", "robot-arm-b", "scanner-high", "piston-round")
              if s in cat_stems]
    if accent and rng.random() < 0.8:
        stem = rng.choice(accent)
        pl.try_add(_loose(prop_piece(stem,
                                     rng.uniform(west + 1.5, east - 1.5),
                                     rng.uniform(south + 1.5, north - 1.5),
                                     yaw=rng.choice([0, math.pi / 2, math.pi, 3 * math.pi / 2]))))

    # ⑦ Warning sign accent near the machine row.
    if "warning-orange" in cat_stems and rng.random() < 0.6:
        pl.try_add(_loose(prop_piece("warning-orange",
                                     mid_x + rng.uniform(-2.0, 2.0),
                                     mid_z + rng.uniform(-2.0, 2.0), yaw=0.0)))

    return pl.pieces()


def setup_industrial_utility(cells_w, cells_ix, rng):
    """Small industrial utility room: boxes, hoppers, pistons against walls."""
    west, east, south, north, mid_x, mid_z = _room_span(cells_w)
    pl = _RoomPlacer(west, east, south, north)
    cat_stems = CURRENT_PROP_CATALOG.get("stems", {})
    stems = [s for s in ("box-large", "box-small", "hopper-round", "hopper-square",
                         "piston-round", "cog-a", "cog-b") if s in cat_stems] \
        or ["box-large", "box-small"]
    closed = _closed_sides()
    for _ in range(rng.randint(2, 4)):
        stem = rng.choice(stems)
        item = cat_stems.get(stem, {})
        if closed and rng.random() < 0.75:
            side = rng.choice(closed)
            span_lo, span_hi = (west, east) if side in ("south", "north") else (south, north)
            p = _snap_to_side(stem, rng.uniform(span_lo + 1.0, span_hi - 1.0), side,
                              west, east, south, north, item)
        else:
            p = _loose(prop_piece(stem, rng.uniform(west + 1.0, east - 1.0),
                                  rng.uniform(south + 1.0, north - 1.0),
                                  yaw=rng.uniform(0, 2 * math.pi)))
        pl.try_add(p)
    return pl.pieces()


# ── PRIESTHOOD ──────────────────────────────────────────────────────────────

def setup_priesthood_chapel(cells_w, cells_ix, rng):
    """Stone chapel: columns flanking the room, barrels/crates to sides.

    No random doors/pillars from the dungeon architecture set.
    Uses retro_fantasy props (columns at scale=4, barrels/crates at scale=1).
    """
    west, east, south, north, mid_x, mid_z = _room_span(cells_w)
    room_w = east - west
    room_h = north - south
    pl = _RoomPlacer(west, east, south, north)

    prop_stems = CURRENT_PROP_CATALOG.get("stems", {})

    # ① Columns: place 2–4 near corners, at scale=4 (imposing stone pillars)
    col_stems = [s for s in ["column", "column-damaged", "column-paint", "column-wood"]
                 if s in prop_stems]
    if col_stems:
        col_offsets = [
            (west + 0.6, south + 0.6),
            (east - 0.6, south + 0.6),
        ]
        if room_w > 6 and room_h > 6:
            col_offsets += [(west + 0.6, north - 0.6), (east - 0.6, north - 0.6)]
        for cx, cz in col_offsets:
            pl.try_add(prop_piece(rng.choice(col_stems), cx, cz,
                                  yaw=rng.choice([0, math.pi / 2, math.pi, 3 * math.pi / 2]),
                                  scale=4.0))

    # ② Barrels along a closed wall (storage feel), scale-aware spacing.
    closed = _closed_sides()
    rng.shuffle(closed)
    barrel_stems = [s for s in ["barrels", "detail-barrel"] if s in prop_stems]
    if barrel_stems and closed:
        side = closed[0]
        span_lo, span_hi = (west, east) if side in ("south", "north") else (south, north)
        a = span_lo + 1.2
        for i in range(rng.randint(1, 3)):
            bstem = rng.choice(barrel_stems)
            item = prop_stems.get(bstem, {})
            width = 2.0 * max(item.get("half_x_m", 0.3), 0.15) * stem_scale(bstem)
            a += width * 0.5
            if a > span_hi - 1.0:
                break
            pl.try_add(_snap_to_side(bstem, a, side, west, east, south, north, item))
            a += width * 0.5 + 0.3

    # ③ Crates against another closed wall
    crate_stems = [s for s in ["detail-crate", "detail-crate-small", "detail-crate-ropes"]
                   if s in prop_stems]
    if crate_stems and len(closed) > 1:
        side = closed[1]
        span_lo, span_hi = (west, east) if side in ("south", "north") else (south, north)
        for i in range(rng.randint(1, 2)):
            cstem = rng.choice(crate_stems)
            item = prop_stems.get(cstem, {})
            pl.try_add(_snap_to_side(cstem, rng.uniform(span_lo + 1.2, span_hi - 1.2),
                                     side, west, east, south, north, item))

    # ④ Optional: fallen/damaged column as ruins accent (half-height stub)
    if "column-damaged" in prop_stems and rng.random() < 0.5:
        rx = mid_x + rng.uniform(-1.5, 1.5)
        rz = mid_z + rng.uniform(-1.5, 1.5)
        pl.try_add(_loose(prop_piece("column-damaged", rx, rz,
                                     yaw=rng.uniform(0, 2 * math.pi),
                                     scale=round(stem_scale("column-damaged") * 0.5, 2))))

    # ⑤ Ladder against a closed wall if room is tall
    if "ladder" in prop_stems and room_h > 5.0 and rng.random() < 0.6 and closed:
        side = closed[-1]
        span_lo, span_hi = (west, east) if side in ("south", "north") else (south, north)
        pl.try_add(_snap_to_side("ladder", rng.uniform(span_lo + 0.8, span_hi - 0.8),
                                 side, west, east, south, north,
                                 prop_stems.get("ladder", {}), gap=0.02))

    return pl.pieces()


def setup_priesthood_ruins(cells_w, cells_ix, rng):
    """Ruins room: fallen structural elements, scattered bricks, barrels."""
    west, east, south, north, mid_x, mid_z = _room_span(cells_w)
    pl = _RoomPlacer(west, east, south, north)
    prop_stems = CURRENT_PROP_CATALOG.get("stems", {})

    # Scattered bricks on floor
    if "bricks" in prop_stems:
        for _ in range(rng.randint(2, 4)):
            rx = rng.uniform(west + 0.5, east - 0.5)
            rz = rng.uniform(south + 0.5, north - 0.5)
            pl.try_add(_loose(prop_piece("bricks", rx, rz, yaw=rng.uniform(0, 2 * math.pi))))

    # Fallen column fragment (half-height broken stub)
    col_stems = [s for s in ["column-damaged", "column-wood"] if s in prop_stems]
    if col_stems:
        cstem = rng.choice(col_stems)
        rx = rng.choice([west + 0.8, east - 0.8, mid_x])
        rz = rng.choice([south + 0.8, north - 0.8, mid_z])
        pl.try_add(_loose(prop_piece(cstem, rx, rz,
                                     yaw=rng.uniform(0, 2 * math.pi),
                                     scale=round(stem_scale(cstem) * 0.5, 2))))

    # Structural wall remnant along one edge (PRIESTHOOD_STEM_SCALES: 2.0)
    if "structure-wall" in prop_stems:
        sx = rng.choice([west + 0.9, east - 0.9])
        sz = rng.uniform(south + 1.0, north - 1.0)
        pl.try_add(prop_piece("structure-wall", sx, sz,
                              yaw=math.pi / 2 if sx < mid_x else 3 * math.pi / 2))

    # Barrel for variety
    barrel_stems = [s for s in ["barrels", "detail-barrel"] if s in prop_stems]
    if barrel_stems and rng.random() < 0.5:
        rx = rng.uniform(west + 0.5, east - 0.5)
        rz = rng.uniform(south + 0.5, north - 0.5)
        pl.try_add(_loose(prop_piece(rng.choice(barrel_stems), rx, rz,
                                     yaw=rng.uniform(0, 2 * math.pi))))

    return pl.pieces()


def setup_priesthood_storage(cells_w, cells_ix, rng):
    """Stone storeroom: barrels and crates stacked against walls."""
    west, east, south, north, mid_x, mid_z = _room_span(cells_w)
    pl = _RoomPlacer(west, east, south, north)
    prop_stems = CURRENT_PROP_CATALOG.get("stems", {})

    storage_stems = [s for s in ["barrels", "detail-barrel", "detail-crate",
                                  "detail-crate-small", "detail-crate-ropes",
                                  "pulley-crate"]
                     if s in prop_stems]
    if not storage_stems:
        storage_stems = ["detail-crate"]

    closed = _closed_sides()
    for _ in range(rng.randint(2, 5)):
        sstem = rng.choice(storage_stems)
        item = prop_stems.get(sstem, {})
        if closed:
            side = rng.choice(closed)
            span_lo, span_hi = (west, east) if side in ("south", "north") else (south, north)
            p = _snap_to_side(sstem, rng.uniform(span_lo + 0.8, span_hi - 0.8),
                              side, west, east, south, north, item)
        else:
            p = prop_piece(sstem, rng.uniform(west + 0.8, east - 0.8),
                           rng.uniform(south + 0.8, north - 0.8),
                           yaw=rng.uniform(0, 2 * math.pi))
        pl.try_add(p)

    # Optional ladder against a closed wall
    if "ladder" in prop_stems and rng.random() < 0.4 and closed:
        side = rng.choice(closed)
        span_lo, span_hi = (west, east) if side in ("south", "north") else (south, north)
        pl.try_add(_snap_to_side("ladder", rng.uniform(span_lo + 0.8, span_hi - 0.8),
                                 side, west, east, south, north,
                                 prop_stems.get("ladder", {}), gap=0.02))

    return pl.pieces()


# ── NECROPOLIS ──────────────────────────────────────────────────────────────

def setup_necropolis_crypt(cells_w, cells_ix, rng):
    """Grave rows along closed walls — modelled on synth bed-row logic.

    Layout per grave slot (probed dims × stem_scale): gravestone at the wall,
    grave slab in front of it pointing into the room, occasional candle at the
    foot end. Rows are tight (graveyards ARE tight) but never cover corridor
    mouths / door bands (mouth rects are seeded in the placer)."""
    west, east, south, north, mid_x, mid_z = _room_span(cells_w)
    room_w = east - west
    room_h = north - south
    cat = CURRENT_CATALOG.get("stems", {})
    pl = _RoomPlacer(west, east, south, north, gap=0.15)

    g_scale = stem_scale("grave")
    g_info = cat.get("grave", {})
    g_hx = max(g_info.get("half_x_m", 0.36), 0.2) * g_scale   # half width (m)
    g_hz = max(g_info.get("half_z_m", 0.62), 0.3) * g_scale   # half length (m)

    gs_stems = [s for s in ["gravestone-cross", "gravestone-bevel",
                            "gravestone-round", "gravestone-wide"] if s in cat]
    stone_stem = rng.choice(gs_stems) if gs_stems else None
    stone_scale = stem_scale(stone_stem) if stone_stem else 1.0
    stone_hz = (max(cat.get(stone_stem, {}).get("half_z_m", 0.125), 0.05) * stone_scale
                if stone_stem else 0.0)

    g_step = g_hx * 2.0 + 0.35                     # spacing along the wall
    # grave centre distance from wall face: stone leans on the wall, slab in front
    g_inset = 2.0 * stone_hz + 0.1 + g_hz + 0.1
    candle_scale = stem_scale("candle") if "candle" in cat else 1.0

    closed = _closed_sides()
    long_sides = (["west", "east"] if room_h >= room_w else ["south", "north"])
    row_sides = [s for s in long_sides if s in closed] or closed[:1]
    # Two facing rows need walkway between them; drop to one row in shallow rooms
    depth = room_w if long_sides == ["west", "east"] else room_h
    if len(row_sides) > 1 and depth < 2.0 * (g_inset + g_hz) + 1.6:
        row_sides = row_sides[:1]

    def place_row(side: str):
        face = _wall_face(side, west, east, south, north)
        sgn = 1 if side in ("west", "south") else -1
        span_lo, span_hi = (south, north) if side in ("west", "east") else (west, east)
        span = span_hi - span_lo - 1.0
        n_graves = max(1, min(6, int(span / g_step)))
        start = span_lo + 0.5 + (span - n_graves * g_step) / 2.0 + g_step / 2.0
        yaw = {"west": math.pi / 2, "east": 3 * math.pi / 2,
               "south": 0.0, "north": math.pi}[side]
        for i in range(n_graves):
            a = start + i * g_step
            g_c = face + sgn * g_inset
            s_c = face + sgn * (stone_hz + 0.05)
            if side in ("west", "east"):
                gx, gz, sx, sz = g_c, a, s_c, a
            else:
                gx, gz, sx, sz = a, g_c, a, s_c
            if not pl.try_add(prop("grave", gx, gz, yaw=yaw), extra_gap=-0.1, nudge=False):
                continue
            if stone_stem:
                pl.add_fixed(prop(stone_stem, sx, sz,
                                  yaw=_facing_yaw(side, cat.get(stone_stem, {}))))
            if "candle" in cat and rng.random() < 0.45:
                # candle at the foot end, just past the slab, into the room
                c_c = face + sgn * (g_inset + g_hz + 0.3 * candle_scale)
                cx, cz = (c_c, a) if side in ("west", "east") else (a, c_c)
                pl.try_add(prop("candle", cx, cz, yaw=0.0), extra_gap=-0.1, nudge=False)

    for side in row_sides:
        place_row(side)

    # Mourning bench against a remaining closed wall, facing the graves
    bench_sides = [s for s in closed if s not in row_sides]
    if "bench" in cat and bench_sides:
        side = bench_sides[0]
        span_lo, span_hi = (west, east) if side in ("south", "north") else (south, north)
        b_info = cat["bench"]
        scale = stem_scale("bench")
        depth_b = max(b_info.get("half_z_m", 0.23) * scale, 0.15)
        face = _wall_face(side, west, east, south, north)
        coord = face + (depth_b + 0.12) * (1 if side in ("west", "south") else -1)
        mid_along = (span_lo + span_hi) / 2.0
        if side in ("west", "east"):
            b = prop("bench", coord, mid_along, yaw=_facing_yaw(side, b_info))
        else:
            b = prop("bench", mid_along, coord, yaw=_facing_yaw(side, b_info))
        pl.try_add(b)

    return pl.pieces()


def setup_necropolis_chapel(cells_w, cells_ix, rng):
    """Altar centred, candles flanking, a few coffins along walls."""
    west, east, south, north, mid_x, mid_z = _room_span(cells_w)
    pl = _RoomPlacer(west, east, south, north, gap=0.2)
    cat = CURRENT_CATALOG.get("stems", {})

    # Altar at centre (focal piece — the one allowed centre item)
    alt_stem = "altar-stone" if "altar-stone" in cat else ("altar-wood" if "altar-wood" in cat else None)
    if alt_stem:
        a_scale = stem_scale(alt_stem)
        a_hx = max(cat.get(alt_stem, {}).get("half_x_m", 0.5), 0.2) * a_scale
        a_hz = max(cat.get(alt_stem, {}).get("half_z_m", 0.33), 0.2) * a_scale
        pl.add_unchecked(prop(alt_stem, mid_x, mid_z, yaw=0.0))
        # Candles flanking altar (offset from the altar's actual width)
        if "candle" in cat:
            c_off = a_hx + 0.3 * stem_scale("candle") + 0.15
            for dx in (-c_off, c_off):
                pl.try_add(prop("candle", mid_x + dx, mid_z, yaw=0.0), extra_gap=-0.1)
        if "candle-multiple" in cat:
            pl.try_add(prop("candle-multiple", mid_x,
                            mid_z - (a_hz + 0.6), yaw=0.0), extra_gap=-0.1)

    # Coffins along closed walls
    if "coffin" in cat:
        closed = _closed_sides()
        for _ in range(rng.randint(1, 3)):
            if not closed:
                break
            side = rng.choice(closed)
            span_lo, span_hi = (west, east) if side in ("south", "north") else (south, north)
            pl.try_add(_snap_to_side("coffin", rng.uniform(span_lo + 1.2, span_hi - 1.2),
                                     side, west, east, south, north, cat.get("coffin", {})))

    # Bench south of the altar, facing it (front = +z at yaw 0)
    if "bench" in cat and alt_stem:
        b_scale = stem_scale("bench")
        b_off = a_hz + 1.0 + b_scale * 0.25
        pl.try_add(prop("bench", mid_x, mid_z - b_off,
                        yaw=_front_flip(cat.get("bench", {}))))

    return pl.pieces()


def setup_necropolis_debris(cells_w, cells_ix, rng):
    """Small ruined area: scattered gravestones, debris, rocks."""
    west, east, south, north, _, _ = _room_span(cells_w)
    pl = _RoomPlacer(west, east, south, north)
    cat = CURRENT_CATALOG.get("stems", {})
    debris_stems = [s for s in ["gravestone-broken", "debris", "rocks", "gravestone-debris",
                                 "bench-damaged", "gravestone-round", "cross-wood"]
                    if s in cat]
    if not debris_stems:
        debris_stems = ["rocks"]
    for _ in range(rng.randint(2, 4)):
        rx = rng.uniform(west + 0.8, east - 0.8)
        rz = rng.uniform(south + 0.8, north - 0.8)
        pl.try_add(_loose(prop(rng.choice(debris_stems), rx, rz,
                               yaw=rng.uniform(0, 2 * math.pi))))
    return pl.pieces()


# ── URBAN / OUTLAW ──────────────────────────────────────────────────────────

def setup_urban_storage(cells_w, cells_ix, rng):
    """Urban storage: pallets + barriers + cables at scale=1.

    Pallets: 1m×1m at scale=1, stacked against walls.
    Barriers: 1.7m×0.5m at scale=1, near wall.
    """
    west, east, south, north, mid_x, mid_z = _room_span(cells_w)
    pl = _RoomPlacer(west, east, south, north)
    cat = CURRENT_CATALOG.get("stems", {})

    storage_stems = [s for s in ["pallet", "pallet-small", "detail-block",
                                  "detail-dumpster-open", "detail-dumpster-closed"]
                     if s in cat]
    if not storage_stems:
        storage_stems = ["detail-block"]

    closed = _closed_sides()
    for _ in range(rng.randint(2, 4)):
        sstem = rng.choice(storage_stems)
        item = cat.get(sstem, {})
        if closed:
            side = rng.choice(closed)
            span_lo, span_hi = (west, east) if side in ("south", "north") else (south, north)
            p = _snap_to_side_faction(sstem, rng.uniform(span_lo + 1.0, span_hi - 1.0),
                                      side, west, east, south, north, item)
        else:
            p = _loose(prop(sstem, rng.uniform(west + 1.0, east - 1.0),
                            rng.uniform(south + 1.0, north - 1.0),
                            yaw=rng.uniform(0, 2 * math.pi)))
        pl.try_add(p)

    # Cable coils along walls
    cable_stems = [s for s in ["detail-cables-type-a", "detail-cables-type-b"] if s in cat]
    if cable_stems and rng.random() < 0.6:
        pl.try_add(_loose(prop(rng.choice(cable_stems),
                               rng.uniform(west + 0.8, east - 0.8),
                               rng.uniform(south + 0.8, north - 0.8),
                               yaw=rng.uniform(0, 2 * math.pi))))

    return pl.pieces()


def setup_urban_breakroom(cells_w, cells_ix, rng):
    """Urban breakroom: bench at wall, planks, barriers. All scale=1."""
    west, east, south, north, mid_x, mid_z = _room_span(cells_w)
    pl = _RoomPlacer(west, east, south, north)
    cat = CURRENT_CATALOG.get("stems", {})

    # Bench with its back against a closed wall, facing the room
    closed = _closed_sides()
    if "detail-bench" in cat and closed:
        b_info = cat["detail-bench"]
        side = rng.choice(closed)
        span_lo, span_hi = (west, east) if side in ("south", "north") else (south, north)
        pl.try_add(_snap_to_side_faction(
            "detail-bench", rng.uniform(span_lo + 1.2, span_hi - 1.2),
            side, west, east, south, north, b_info))

    # Scatter: bricks, planks, barriers
    scatter_stems = [s for s in ["detail-bricks-type-a", "detail-bricks-type-b",
                                  "planks", "detail-barrier-type-a", "detail-barrier-type-b",
                                  "detail-block"]
                     if s in cat]
    for _ in range(rng.randint(2, 4)):
        if not scatter_stems:
            break
        rx = rng.uniform(west + 0.8, east - 0.8)
        rz = rng.uniform(south + 0.8, north - 0.8)
        pl.try_add(_loose(prop(rng.choice(scatter_stems), rx, rz,
                               yaw=rng.uniform(0, 2 * math.pi))))

    return pl.pieces()


# ── ROLE → SETUP MAP ────────────────────────────────────────────────────────

ROLE_SETUPS = {
    "industrial": {
        "factory_floor": setup_industrial_factory_floor,
        "utility":       setup_industrial_utility,
        "storage":       setup_industrial_utility,    # reuse for small rooms
        "breakroom":     setup_industrial_factory_floor,
    },
    "industrial_default": {
        "factory_floor": setup_industrial_factory_floor,
        "utility":       setup_industrial_utility,
        "storage":       setup_industrial_utility,
        "breakroom":     setup_industrial_factory_floor,
    },
    "priesthood": {
        "chapel":  setup_priesthood_chapel,
        "ruins":   setup_priesthood_ruins,
        "storage": setup_priesthood_storage,
    },
    "necropolis": {
        "crypt":   setup_necropolis_crypt,
        "chapel":  setup_necropolis_chapel,
        "debris":  setup_necropolis_debris,
        "storage": setup_necropolis_debris,
    },
    "urban": {
        "storage":   setup_urban_storage,
        "breakroom": setup_urban_breakroom,
    },
    "outlaw": {
        "storage":   setup_urban_storage,
        "breakroom": setup_urban_breakroom,
    },
    "synth": {
        # Synth delegates to synth_interior; these are generic fallbacks only
        "storage":   setup_urban_storage,
        "breakroom": setup_urban_breakroom,
    },
}


# ---------------------------------------------------------------------------
# Stem list helpers
# ---------------------------------------------------------------------------

def _get_faction_decor_stems(faction: str) -> List[str]:
    """Return prop stems for scatter/extra pass, from prop_catalog if available."""
    fid = FACTION_ID_MAP.get(faction, faction)

    # Use prop catalog when available (industrial, priesthood)
    prop_cat_stems = CURRENT_PROP_CATALOG.get("stems", {})
    if prop_cat_stems:
        avoid = {"wall", "floor", "stairs", "corridor", "room", "gate", "template"}
        candidates = [s for s in prop_cat_stems
                      if not any(a in s for a in avoid)]
        if candidates:
            return candidates[:16]

    # Faction-folder props (necropolis, urban already have good sets)
    if fid == "necropolis":
        cat = CURRENT_CATALOG.get("stems", {})
        props = [s for s in cat
                 if any(k in s for k in ["grave", "candle", "coffin", "altar",
                                          "bench", "debris", "rock", "cross",
                                          "urn", "lantern", "fence"])]
        return props[:16] or ["grave", "candle", "bench", "rocks"]

    if fid in ("urban", "outlaw"):
        cat = CURRENT_CATALOG.get("stems", {})
        props = [s for s in cat
                 if any(k in s for k in ["pallet", "barrier", "bench", "cable",
                                          "dumpster", "block", "brick", "plank",
                                          "scaffold", "detail"])]
        return props[:16] or ["detail-block", "pallet", "detail-bench"]

    if fid == "synth":
        cat = CURRENT_CATALOG.get("stems", {})
        props = [s for s in cat
                 if any(k in s for k in ["computer", "chair", "table", "container",
                                          "display", "bed"])]
        return props[:16] or ["container", "table"]

    return ["rocks"]


# ---------------------------------------------------------------------------
# Resolve overlaps (post-pass nudge)
# ---------------------------------------------------------------------------

def resolve_overlaps(props_list: List[dict], gap: float = None) -> List[dict]:
    """Iterative nudge to separate overlapping pieces. Call after a room is fully placed.

    Only 'loose' scatter pieces may move. Deliberately composed pieces —
    pipe/conveyor runs, grave rows, wall-snapped items — are immovable: the
    placer already guaranteed their spacing, and nudging them destroys the
    end-to-end alignment (this bug scattered the first pipe runs)."""
    if len(props_list) < 2:
        return props_list
    if gap is None:
        gap = CURRENT_PLACE_GAP * 0.5
    rng_local = random.Random(42)

    def movable(p: dict) -> bool:
        return "loose" in (p.get("tags") or [])

    for _ in range(8):
        changed = False
        for i in range(len(props_list)):
            for j in range(i + 1, len(props_list)):
                p1, p2 = props_list[i], props_list[j]
                m1, m2 = movable(p1), movable(p2)
                if not (m1 or m2):
                    continue
                bb1 = _prop_bbox(p1)
                bb2 = _prop_bbox(p2)
                if bb1.padded(gap).overlaps(bb2.padded(gap)):
                    dx = (p1["x"] - p2["x"]) or (0.2 if rng_local.random() < 0.5 else -0.2)
                    dz = (p1["z"] - p2["z"]) or (0.2 if rng_local.random() < 0.5 else -0.2)
                    d = math.hypot(dx, dz) or 1.0
                    nd = 0.3 if (m1 and m2) else 0.5
                    if m1:
                        p1["x"] = round(p1["x"] + dx / d * nd, 3)
                        p1["z"] = round(p1["z"] + dz / d * nd, 3)
                    if m2:
                        p2["x"] = round(p2["x"] - dx / d * nd, 3)
                        p2["z"] = round(p2["z"] - dz / d * nd, 3)
                    changed = True
        if not changed:
            break
    return props_list


def _loose(p: dict) -> dict:
    """Mark a piece as free scatter — resolve_overlaps may nudge it."""
    p["tags"] = list(set(p.get("tags") or []) | {"loose"})
    return p


# ---------------------------------------------------------------------------
# Main entry point
# ---------------------------------------------------------------------------

def furnish_faction_interior(
    pieces: List[dict],
    gx: int,
    gz: int,
    walkable: Set[CellIx],
    zone_lookup: Callable[[CellIx], Optional[str]],
    deck_cells: Set[CellIx],
    seed: int,
    corridor_cells: Optional[Set[CellIx]] = None,
    faction: Optional[str] = None,
    sweep_params: Optional["SweepParams"] = None,
    world_at: Optional[Callable[[CellIx], Tuple[float, float]]] = None,
    global_walkable: Optional[Set[CellIx]] = None,
    door_positions: Optional[List[Tuple[float, float]]] = None,
    tall_cells: Optional[Set[CellIx]] = None,
) -> int:
    if faction is None:
        faction = CURRENT_FACTION
    set_faction_context(faction, sweep_params)

    corridor_ix: Set[CellIx] = corridor_cells or set()
    all_walkable: Set[CellIx] = global_walkable or walkable
    door_positions = door_positions or []

    # Build 4-connected room components (exclude corridors)
    from collections import deque
    rooms: Dict[int, Set[CellIx]] = {}
    visited: Set[CellIx] = set()
    rid = 0
    for c in sorted(walkable):
        if c in visited or c in corridor_ix:
            continue
        comp: Set[CellIx] = set()
        q = deque([c])
        while q:
            cur = q.popleft()
            if cur in visited or cur in corridor_ix:
                continue
            visited.add(cur)
            comp.add(cur)
            for dx, dz in ((0, 1), (0, -1), (1, 0), (-1, 0)):
                nb = (cur[0] + dx, cur[1] + dz)
                if nb in walkable and nb not in visited:
                    q.append(nb)
        if len(comp) >= 2:
            rooms[rid] = comp
            rid += 1

    if not rooms:
        return 0

    roles = assign_faction_roles(rooms, corridor_ix, CURRENT_FACTION)
    rng = random.Random(seed ^ 0xFACADE)

    def _world_at(c: CellIx) -> Tuple[float, float]:
        if world_at:
            return world_at(c)
        return float(c[0]) * 4.0, float(c[1]) * 4.0

    SIDE_DELTA = {"north": (0, 1), "south": (0, -1), "east": (1, 0), "west": (-1, 0)}

    n = 0
    for room_id, cells_ix in rooms.items():
        role = roles.get(room_id, "storage")
        cells_w = {_world_at(c) for c in cells_ix}
        west, east, south, north, mid_x, mid_z = _room_span(cells_w)

        # Room context: open faces (corridor mouths / zone seams) become
        # keep-clear boxes; sides with any opening are excluded from
        # wall-snapping. NOTE grid +z maps to "north" here in world terms.
        open_sides: Set[str] = set()
        mouth_rects: List[BBox] = []
        for c in cells_ix:
            wx, wz = _world_at(c)
            for side, (dx, dz) in SIDE_DELTA.items():
                nb = (c[0] + dx, c[1] + dz)
                if nb in cells_ix:
                    continue
                if nb in all_walkable:  # open face: corridor mouth or zone seam
                    open_sides.add(side)
                    if side in ("east", "west"):
                        fx = wx + dx * CELL / 2.0
                        mouth_rects.append(BBox(fx - 1.6, fx + 1.6, wz - 2.0, wz + 2.0))
                    else:
                        fz = wz + dz * CELL / 2.0
                        mouth_rects.append(BBox(wx - 2.0, wx + 2.0, fz - 1.6, fz + 1.6))
        # Door bands (transition / hidden doors): keep 2 m clear around them.
        for (dx_w, dz_w) in door_positions:
            if west - 2.5 <= dx_w <= east + 2.5 and south - 2.5 <= dz_w <= north + 2.5:
                mouth_rects.append(BBox(dx_w - 2.0, dx_w + 2.0, dz_w - 2.0, dz_w + 2.0))

        CURRENT_ROOM_CTX["open_sides"] = open_sides
        CURRENT_ROOM_CTX["mouth_rects"] = mouth_rects
        # Double-height hall (gen_freeform raised its shell): catwalk-eligible.
        CURRENT_ROOM_CTX["tall"] = bool(tall_cells) and cells_ix <= tall_cells

        faction_setups = ROLE_SETUPS.get(CURRENT_FACTION, {})
        setup_fn = faction_setups.get(role)
        room_props: List[dict] = []

        if setup_fn:
            room_props = setup_fn(cells_w, cells_ix, rng)

        # Urban/outlaw: blocker barriers at corridor mouth entrances
        if CURRENT_FACTION in ("urban", "outlaw") and corridor_ix:
            mouths = opens_to_corridor(cells_ix, corridor_ix)
            cat = CURRENT_CATALOG.get("stems", {})
            bstem = "detail-barrier-type-a" if "detail-barrier-type-a" in cat else None
            if bstem and mouths:
                face = list(mouths)[0]
                bitem = cat.get(bstem, {})
                if face == "south":
                    bx, bz, byaw = mid_x, south + 0.5, 0.0
                elif face == "north":
                    bx, bz, byaw = mid_x, north - 0.5, math.pi
                elif face == "west":
                    bx, bz, byaw = west + 0.5, mid_z, math.pi / 2
                else:
                    bx, bz, byaw = east - 0.5, mid_z, 3 * math.pi / 2
                bp = prop(bstem, bx, bz, yaw=byaw)
                bp["tags"] = [f"{CURRENT_FACTION}_blocker"]
                room_props.append(bp)

        # Resolve overlaps within this room
        if len(room_props) > 1:
            resolve_overlaps(room_props)

        # Emit pieces
        faction_tag = f"{CURRENT_FACTION}_{role}"
        room_tag = f"room_{room_id}"
        for p in room_props:
            p["tags"] = list(set(p.get("tags") or [])) + [faction_tag, room_tag]
            if "kit" not in p or not p["kit"]:
                p["kit"] = CURRENT_KIT
            pieces.append(p)
            n += 1

    return n


# ---------------------------------------------------------------------------
# Backward compat shim for synth delegation
# ---------------------------------------------------------------------------

def furnish_synth_interior(*args, **kwargs):
    if _HAS_SYNTH:
        return si.furnish_synth_interior(*args, **kwargs)
    return furnish_faction_interior(*args, **kwargs, faction="synth")


# ---------------------------------------------------------------------------
# SweepParams + load helper
# ---------------------------------------------------------------------------

@dataclass
class SweepParams:
    gap_multiplier: float = 1.0
    pairing_aggressiveness: float = 1.0
    quarters_area_max: int = 6
    chapel_bench_spacing: float = 2.5
    crypt_row_count: int = 3


def load_sweep_params(faction: str) -> SweepParams:
    path = ROOT / "userinput" / "synth_dressing" / f"sweep_best_params_{faction}.json"
    if path.exists():
        try:
            data = json.loads(path.read_text())
            best = data.get("best", {}).get("params", {})
            return SweepParams(
                gap_multiplier=float(best.get("gap_multiplier", 1.0)),
                pairing_aggressiveness=float(best.get("pairing_aggressiveness", 1.0)),
            )
        except Exception:
            pass
    return SweepParams()


# Helpers referenced by gen_freeform but not changed in behaviour
def _get_catalog_item(stem: str) -> Dict:
    return CURRENT_CATALOG.get("stems", {}).get(stem, {})


def flush_back_to_wall(stem, wall, x, yaw, *, z=0.0, scale=None):
    """Legacy helper kept for callers outside this module."""
    if scale is None:
        scale = CURRENT_SCALE
    item = _get_catalog_item(stem)
    depth = max(item.get("half_z_m", CURRENT_PLACE_GAP), CURRENT_PLACE_GAP)
    if wall == "south":   return prop(stem, x, z - depth, yaw=yaw, scale=scale)
    if wall == "north":   return prop(stem, x, z + depth, yaw=yaw, scale=scale)
    if wall == "east":    return prop(stem, x + depth, z, yaw=yaw, scale=scale)
    return prop(stem, x - depth, z, yaw=yaw, scale=scale)
