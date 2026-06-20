#!/usr/bin/env python3
"""
Fabled – free-form tile map generator (rooms + corridors, NO module grid).

This is the post-module generator: there is no 5×5 module slot structure and no
pre-authored / room-* GLBs.  Everything is built from 1×1 Kenney tiles on a flat
cell grid of arbitrary size:

  1. Place variable-size rectangular rooms (no overlap, 1-cell gap).
  2. Connect room centres with a minimum spanning tree + a few loop edges.
  3. Carve 1-wide L-shaped corridors between connected rooms (outside rooms).
  4. Emit tiles: rooms → `template-floor` + perimeter `template-wall`; corridors
     → rounded `corridor-*` pieces chosen by their open faces.
  5. Spawn / extraction = the two farthest-apart rooms.

Output doc matches what `gen_maps.export_kenney_layout` / the editor expect:
floors mask + pieces + spawn_xz + extraction_xz.  No hub / branch levels.

Usage:
    python tools/gen_freeform.py --seed 42
    python tools/gen_freeform.py --seed 42 --cells 40 --rooms 12
"""

from __future__ import annotations

import argparse
import json
import math
import random
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Set, Tuple

CELL = 4.0
PI = math.pi
PI2 = math.pi / 2.0
PI32 = 3.0 * math.pi / 2.0

Cell = Tuple[int, int]
DELTA: Dict[str, Cell] = {'N': (0, -1), 'S': (0, 1), 'E': (1, 0), 'W': (-1, 0)}
OPP = {'N': 'S', 'S': 'N', 'E': 'W', 'W': 'E'}

# Wall on a cell's `side` face, finished face pointing inward (verified yaws,
# mirrors gen_modules step-7 edge closing: N→PI, S→0, E→PI2, W→PI32).
WALL_YAW = {'N': PI, 'S': 0.0, 'E': PI2, 'W': PI32}

# Corridor piece yaws by open-face signature (mirrors gen_modules.strat_planned).
CORRIDOR_END_YAW = {'N': PI2, 'S': PI32, 'E': 0.0, 'W': PI}
CORNER_YAW = {
    frozenset({'N', 'W'}): 0.0,
    frozenset({'S', 'W'}): PI2,
    frozenset({'S', 'E'}): PI,
    frozenset({'N', 'E'}): PI32,
}
JUNC_YAW = {'S': 0.0, 'E': PI2, 'N': PI, 'W': PI32}

MAP_DIR = Path("userinput/maps")
LAYOUT_PATH = Path("userinput/kenney_layout.json")


@dataclass(frozen=True)
class Room:
    x0: int
    z0: int
    w: int
    h: int

    @property
    def cx(self) -> int:
        return self.x0 + self.w // 2

    @property
    def cz(self) -> int:
        return self.z0 + self.h // 2

    def cells(self) -> List[Cell]:
        return [(x, z) for z in range(self.z0, self.z0 + self.h)
                for x in range(self.x0, self.x0 + self.w)]

    def nearest_cell(self, tx: int, tz: int) -> Cell:
        """Room cell closest to an (external) target point — a boundary cell."""
        return (
            min(max(tx, self.x0), self.x0 + self.w - 1),
            min(max(tz, self.z0), self.z0 + self.h - 1),
        )

    def overlaps(self, other: "Room", gap: int = 1) -> bool:
        return not (
            self.x0 - gap >= other.x0 + other.w
            or other.x0 - gap >= self.x0 + self.w
            or self.z0 - gap >= other.z0 + other.h
            or other.z0 - gap >= self.z0 + self.h
        )


@dataclass
class HubExit:
    kind: str                 # 'trap' | 'doorway'
    corridor: List[Cell]      # floor -1 stub cells (empty for a direct trap)
    trap: Cell                # floor -1 cell that drops to the next level
    landing: Set[Cell]        # floor -2 landing room footprint


@dataclass
class Hub:
    trap0: Cell               # floor 0 extraction trap cell (drop into hub)
    floor1: Set[Cell]         # floor -1 footprint (hub room + doorway stubs)
    holes1: Set[Cell]         # floor -1 cells that are trap holes
    floor2: Set[Cell]         # floor -2 footprint (the two landing rooms)
    exits: List[HubExit]


@dataclass
class FreeformMap:
    gx: int
    gz: int
    rooms: List[Room]
    walkable: Set[Cell]
    room_cells: Set[Cell]
    corridor_cells: Set[Cell]
    spawn_room: int
    end_room: int
    seed: int = 0
    hub: Optional[Hub] = None
    # 2-wide corridor cells: walkable but emitted as room-style floor+walls (not
    # 1-wide corridor GLBs). Kept out of corridor_cells so emit_pieces walls them.
    wide_cells: Set[Cell] = field(default_factory=set)
    # Indices into `rooms` of single-entrance dead-end "secret" rooms.
    hidden_rooms: List[int] = field(default_factory=list)


# ─── geometry ────────────────────────────────────────────────────────────────

def world_x(gx: int, ix: int) -> float:
    return -(gx * CELL) * 0.5 + ix * CELL + CELL * 0.5


def world_z(gz: int, iz: int) -> float:
    return -(gz * CELL) * 0.5 + iz * CELL + CELL * 0.5


# ─── generation ──────────────────────────────────────────────────────────────

def place_rooms(
    rng: random.Random, gx: int, gz: int, max_rooms: int,
    rmin: int, rmax: int, tries: int,
) -> List[Room]:
    rooms: List[Room] = []
    for _ in range(tries):
        if len(rooms) >= max_rooms:
            break
        w = rng.randint(rmin, rmax)
        h = rng.randint(rmin, rmax)
        if w + 2 >= gx or h + 2 >= gz:
            continue
        x0 = rng.randint(1, gx - w - 1)
        z0 = rng.randint(1, gz - h - 1)
        cand = Room(x0, z0, w, h)
        if any(cand.overlaps(r) for r in rooms):
            continue
        rooms.append(cand)
    return rooms


def mst_edges(rooms: List[Room]) -> List[Tuple[int, int]]:
    """Prim's MST over room centres (Manhattan distance)."""
    n = len(rooms)
    if n <= 1:
        return []
    in_tree = {0}
    edges: List[Tuple[int, int]] = []
    while len(in_tree) < n:
        best: Optional[Tuple[int, int, int]] = None
        for a in in_tree:
            for b in range(n):
                if b in in_tree:
                    continue
                d = abs(rooms[a].cx - rooms[b].cx) + abs(rooms[a].cz - rooms[b].cz)
                if best is None or d < best[0]:
                    best = (d, a, b)
        assert best is not None
        _, a, b = best
        edges.append((a, b))
        in_tree.add(b)
    return edges


def loop_edges(
    rng: random.Random, rooms: List[Room], existing: Set[Tuple[int, int]], count: int,
) -> List[Tuple[int, int]]:
    n = len(rooms)
    cand: List[Tuple[int, int, int]] = []
    for a in range(n):
        for b in range(a + 1, n):
            if (a, b) in existing or (b, a) in existing:
                continue
            d = abs(rooms[a].cx - rooms[b].cx) + abs(rooms[a].cz - rooms[b].cz)
            cand.append((d, a, b))
    cand.sort()
    # Prefer short reconnects (nearby rooms) for natural-looking shortcuts.
    pool = cand[: max(count * 3, count)]
    rng.shuffle(pool)
    return [(a, b) for _, a, b in pool[:count]]


def _hline(path: Set[Cell], x0: int, x1: int, z: int, wide: bool = False) -> None:
    for x in range(min(x0, x1), max(x0, x1) + 1):
        path.add((x, z))
        if wide:  # widen a horizontal run perpendicular (in z)
            path.add((x, z + 1))


def _vline(path: Set[Cell], z0: int, z1: int, x: int, wide: bool = False) -> None:
    for z in range(min(z0, z1), max(z0, z1) + 1):
        path.add((x, z))
        if wide:  # widen a vertical run perpendicular (in x)
            path.add((x + 1, z))


def carve_corridor(
    rng: random.Random, a: Room, b: Room, room_cells: Set[Cell],
    gx: int, gz: int, organicness: float = 0.0, corridor_width: float = 1.0,
) -> Tuple[Set[Cell], bool]:
    """Path between the rooms' nearest-facing boundary cells; returns (cells, is_wide).

    Connecting the boundary cell each room presents toward the other (rather than
    centre-to-centre) keeps the corridor short and meets each room at a single
    perpendicular cell — a clean doorway instead of a long edge seam.

    `organicness` (0–1): at 0 the path is a clean 1-bend L; with probability
    `organicness` it becomes a 2-bend Z (jog through an intermediate offset).

    `corridor_width` (1.0–2.0): fraction of corridors that come out 2-wide. 1.0 =
    all 1-wide, 2.0 = all 2-wide, 1.3 = ~30% 2-wide. Wide corridors are emitted as
    room-style floor + perimeter walls (see emit_pieces), not 1-wide corridor GLBs.
    """
    ax, az = a.nearest_cell(b.cx, b.cz)
    bx, bz = b.nearest_cell(a.cx, a.cz)
    path: Set[Cell] = set()
    wide = corridor_width > 1.0 and rng.random() < (corridor_width - 1.0)

    jog = organicness > 0.0 and rng.random() < organicness
    if jog and abs(bx - ax) >= 2 and rng.random() < 0.5:
        # Horizontal-dominant Z: A → (mx,az) → (mx,bz) → B
        lo, hi = sorted((ax, bx))
        mx = rng.randint(lo + 1, hi - 1)
        _hline(path, ax, mx, az, wide)
        _vline(path, az, bz, mx, wide)
        _hline(path, mx, bx, bz, wide)
    elif jog and abs(bz - az) >= 2:
        # Vertical-dominant Z: A → (ax,mz) → (bx,mz) → B
        lo, hi = sorted((az, bz))
        mz = rng.randint(lo + 1, hi - 1)
        _vline(path, az, mz, ax, wide)
        _hline(path, ax, bx, mz, wide)
        _vline(path, mz, bz, bx, wide)
    elif rng.random() < 0.5:
        _hline(path, ax, bx, az, wide)
        _vline(path, az, bz, bx, wide)
    else:
        _vline(path, az, bz, ax, wide)
        _hline(path, ax, bx, bz, wide)
    cells = {
        (x, z) for (x, z) in path
        if 0 <= x < gx and 0 <= z < gz and (x, z) not in room_cells
    }
    return cells, wide


def place_hidden_rooms(
    rng: random.Random, rooms: List[Room], room_cells: Set[Cell],
    corridor_cells: Set[Cell], wide_cells: Set[Cell],
    gx: int, gz: int, room_min: int, prevalence: float,
) -> List[int]:
    """Append small single-entrance dead-end rooms; return their indices in `rooms`.

    Mutates `rooms` (append), `room_cells` and `corridor_cells` (in place). Each
    hidden room is isolated by a 1-cell halo from all existing walkable space, then
    linked back to its nearest room by ONE 1-wide corridor — a lone entrance, the
    site a future secret door will seal.
    """
    n_target = round(prevalence * 4)
    if n_target <= 0:
        return []
    hidden: List[int] = []
    occupied = room_cells | corridor_cells | wide_cells
    for _ in range(n_target * 10):
        if len(hidden) >= n_target:
            break
        w = rng.randint(room_min, room_min + 1)
        h = rng.randint(room_min, room_min + 1)
        if w + 2 >= gx or h + 2 >= gz:
            continue
        cand = Room(rng.randint(1, gx - w - 1), rng.randint(1, gz - h - 1), w, h)
        cc = set(cand.cells())
        halo = {(cx + dx, cz + dz) for (cx, cz) in cc
                for dx in (-1, 0, 1) for dz in (-1, 0, 1)}
        if halo & occupied:
            continue
        parent = min(
            range(len(rooms)),
            key=lambda i: (rooms[i].cx - cand.cx) ** 2 + (rooms[i].cz - cand.cz) ** 2)
        link, _ = carve_corridor(rng, rooms[parent], cand, room_cells, gx, gz, 0.0, 1.0)
        if not link:
            continue
        rooms.append(cand)
        hidden.append(len(rooms) - 1)
        room_cells |= cc
        corridor_cells |= link - room_cells
        occupied |= cc | link | halo
    return hidden


HUB_SIZE = 7        # hub room is HUB_SIZE×HUB_SIZE cells on floor -1
LANDING_SIZE = 3    # each next-level landing room on floor -2
DOORWAY_LEN = 2     # corridor cells from hub wall to the doorway's trap


def _rect(cx: int, cz: int, w: int, h: int, gx: int, gz: int) -> Set[Cell]:
    """w×h cell rect centred on (cx,cz), clamped fully inside the grid."""
    x0 = min(max(cx - w // 2, 0), gx - w)
    z0 = min(max(cz - h // 2, 0), gz - h)
    return {(x, z) for z in range(z0, z0 + h) for x in range(x0, x0 + w)}


def _landing_at(trap: Cell, gx: int, gz: int) -> Set[Cell]:
    return _rect(trap[0], trap[1], LANDING_SIZE, LANDING_SIZE, gx, gz)


def _landings_disjoint(a: Set[Cell], b: Set[Cell]) -> bool:
    return not (a & b)


def build_hub(rng: random.Random, fm: FreeformMap) -> Optional[Hub]:
    """Floor-0 extraction trap → hub room (-1) with 2 exits → landings (-2).

    Each exit is either a direct trap door in the hub floor, or a doorway opening
    onto a short corridor that ends in a trap door.  Both drop the player onto a
    landing room on floor -2 — the start of one of the two next levels (the
    player commits to one; there is no way back up).  No stairs / gate / west
    expansion (the old module hub model is gone).

    Returns None when two exits with disjoint landings cannot be placed.
    """
    gx, gz = fm.gx, fm.gz
    end = fm.rooms[fm.end_room]
    trap0 = (end.cx, end.cz)

    hub = _rect(trap0[0], trap0[1], HUB_SIZE, HUB_SIZE, gx, gz)
    hcx = sum(x for x, _ in hub) // len(hub)
    hcz = sum(z for _, z in hub) // len(hub)
    landing_cell = (hcx, hcz)

    floor1: Set[Cell] = set(hub)
    holes1: Set[Cell] = set()
    floor2: Set[Cell] = set()
    exits: List[HubExit] = []

    sides = ['N', 'S', 'E', 'W']
    rng.shuffle(sides)
    for side in sides:
        if len(exits) >= 2:
            break
        dx, dz = DELTA[side]
        kind = rng.choice(['trap', 'doorway'])
        corridor: List[Cell] = []
        if kind == 'doorway':
            corridor = [
                (hcx + dx * (3 + i), hcz + dz * (3 + i))
                for i in range(1, DOORWAY_LEN + 1)
            ]
            if not all(0 <= x < gx and 0 <= z < gz for x, z in corridor):
                kind = 'trap'
        if kind == 'trap':
            trap = (hcx + dx * 2, hcz + dz * 2)
            if trap == landing_cell or trap not in hub:
                continue
            corridor = []
        else:
            trap = corridor[-1]

        landing = _landing_at(trap, gx, gz)
        if not _landings_disjoint(landing, floor2):
            continue
        if trap in holes1:
            continue

        holes1.add(trap)
        floor1 |= set(corridor)
        floor2 |= landing
        exits.append(HubExit(kind, corridor, trap, landing))

    if len(exits) < 2:
        return None

    return Hub(trap0=trap0, floor1=floor1, holes1=holes1, floor2=floor2, exits=exits)


def generate_map(
    seed: Optional[int],
    *,
    cells: int = 25,
    max_rooms: int = 11,
    room_min: int = 3,
    room_max: int = 7,
    loops: int = 3,
    organicness: float = 0.0,
    corridor_width: float = 1.0,
    hidden_area_prevalence: float = 0.0,
    room_tries: int = 400,
) -> Optional[FreeformMap]:
    rng = random.Random(seed)
    gx = gz = cells
    rooms = place_rooms(rng, gx, gz, max_rooms, room_min, room_max, room_tries)
    if len(rooms) < 2:
        return None

    room_cells: Set[Cell] = set()
    for r in rooms:
        room_cells.update(r.cells())

    edges = mst_edges(rooms)
    eset = set(edges)
    edges += loop_edges(rng, rooms, eset, loops)

    corridor_cells: Set[Cell] = set()
    wide_cells: Set[Cell] = set()
    for a, b in edges:
        cells_set, is_wide = carve_corridor(
            rng, rooms[a], rooms[b], room_cells, gx, gz, organicness, corridor_width)
        if is_wide:
            wide_cells |= cells_set
        else:
            corridor_cells |= cells_set
    # A cell carved by both a wide and a narrow corridor reads as wide (room-style).
    corridor_cells -= room_cells | wide_cells
    wide_cells -= room_cells

    walkable = room_cells | corridor_cells | wide_cells

    # Spawn / extraction = farthest-apart room pair (Euclidean on centres).
    spawn_i, end_i, best = 0, 1, -1.0
    for i in range(len(rooms)):
        for j in range(i + 1, len(rooms)):
            d = (rooms[i].cx - rooms[j].cx) ** 2 + (rooms[i].cz - rooms[j].cz) ** 2
            if d > best:
                best, spawn_i, end_i = d, i, j

    # Hidden areas: small single-entrance dead-end rooms appended AFTER spawn/end
    # are chosen, so a secret can never become spawn or extraction. Reachable for
    # now (no seal); the runtime secret-door mechanic seals/opens them later.
    hidden_rooms = place_hidden_rooms(
        rng, rooms, room_cells, corridor_cells, wide_cells, gx, gz,
        room_min, hidden_area_prevalence)
    if hidden_rooms:
        walkable = room_cells | corridor_cells | wide_cells

    fm = FreeformMap(
        gx, gz, rooms, walkable, room_cells, corridor_cells, spawn_i, end_i,
        seed=seed if seed is not None else 0,
        wide_cells=wide_cells,
        hidden_rooms=hidden_rooms,
    )
    hub_rng = random.Random((fm.seed * 2654435761) & 0xFFFFFFFF)
    for _ in range(32):
        hub = build_hub(hub_rng, fm)
        if hub is not None:
            fm.hub = hub
            break
        hub_rng = random.Random(hub_rng.randint(0, 2**31 - 1))
    if fm.hub is None:
        return None
    return fm


# ─── tile emission ───────────────────────────────────────────────────────────

def _world_to_cell(gx: int, gz: int, wx: float, wz: float) -> Cell:
    ix = int(round((wx + gx * CELL * 0.5 - CELL * 0.5) / CELL))
    iz = int(round((wz + gz * CELL * 0.5 - CELL * 0.5) / CELL))
    return (ix, iz)


def _is_floor_surface(stem: str) -> bool:
    """True when a piece provides floor geometry at its cell (incl. hole frames)."""
    if stem.startswith("corridor"):
        return True
    if stem.startswith("template-floor"):
        return True
    return False


def _is_solid_floor(stem: str) -> bool:
    """Walkable / ceiling slab — excludes trap-door hole frames (open vertical)."""
    if stem.startswith("corridor"):
        return True
    if stem == "template-floor":
        return True
    return False


def _has_floor_surface_at(
    pieces: List[dict], floor: int, wx: float, wz: float, *, eps: float = 0.01
) -> bool:
    """Any floor GLB at this centre (incl. trap-door hole frames, excl. ceiling slabs)."""
    for p in pieces:
        if int(p.get("floor_level", 0)) != floor:
            continue
        if p.get("ceiling"):
            continue
        if not _is_floor_surface(p["stem"]):
            continue
        if abs(float(p["x"]) - wx) < eps and abs(float(p["z"]) - wz) < eps:
            return True
    return False


def _has_solid_floor_at(
    pieces: List[dict], floor: int, wx: float, wz: float, *, eps: float = 0.01
) -> bool:
    """0 px centre probe for an existing functional floor tile (not hole frames / ceilings)."""
    for p in pieces:
        if int(p.get("floor_level", 0)) != floor:
            continue
        if p.get("ceiling"):
            continue
        if not _is_solid_floor(p["stem"]):
            continue
        if abs(float(p["x"]) - wx) < eps and abs(float(p["z"]) - wz) < eps:
            return True
    return False


def emit_roofs(
    pieces: List[dict],
    gx: int,
    gz: int,
    floor: int,
    footprint: Set[Cell],
    *,
    skip_roof: Optional[Set[Cell]] = None,
    roof_offset: int = 1,
) -> None:
    """Normal `template-floor` one level up — rendered flipped as ceiling from below.

    Must run **last**, after all functional floors. Skips when a functional floor
    tile already occupies that centre (0 px probe). Trap-door *frames* on
    ``floor`` still get a ceiling slab on the level above (open pit below, roof
    above). Use ``skip_roof`` only where a ceiling would block a vertical drop
    (landing cells under a hub trap)."""
    roof_floor = floor + roof_offset
    skip_roof = skip_roof or set()
    for (ix, iz) in sorted(footprint):
        if (ix, iz) in skip_roof:
            continue
        wx, wz = world_x(gx, ix), world_z(gz, iz)
        if _has_floor_surface_at(pieces, roof_floor, wx, wz):
            continue
        pieces.append({
            "stem": "template-floor",
            "x": wx,
            "z": wz,
            "yaw": 0.0,
            "floor_level": roof_floor,
            "scale": 1.0,
            "group_id": 1,
            "ceiling": True,
        })


def emit_all_roofs(
    pieces: List[dict],
    fm: FreeformMap,
    hub: Optional[Hub],
    gx: int,
    gz: int,
    holes0: Optional[Set[Cell]] = None,
) -> None:
    """Final roof pass — hub extension (−1→0), landings (−2→−1), main level (0→1).

    A ceiling (`template-floor`, ``ceiling: True``) is placed one level above **every**
    walkable floor-0 tile, including the extraction-trap tile (so the roof has no hole
    above the trapdoor — the trap drops you *down*, the roof is one level *up*). The
    "tile under a trap door" that must stay open is the landing cell *below* a hub trap;
    that is handled by ``skip_roof`` in the landing pass. Ceiling slabs are visual-only
    (no collider), so the player walks freely beneath them."""
    holes0 = holes0 or set()
    if hub:
        hub_extension = (hub.floor1 - hub.holes1) - fm.walkable
        emit_roofs(pieces, gx, gz, -1, hub_extension)
        for c in hub.holes1:
            emit_roofs(pieces, gx, gz, -1, {c})
        for ex in hub.exits:
            emit_roofs(
                pieces, gx, gz, -2, ex.landing,
                skip_roof=ex.landing & hub.holes1,
            )
    # Main level: roof over every walkable floor-0 tile (trap included → no roof hole).
    emit_roofs(pieces, gx, gz, 0, fm.walkable)


def emit_floor_tiles(
    pieces: List[dict], gx: int, gz: int, floor: int,
    footprint: Set[Cell], holes: Set[Cell],
) -> None:
    """Tiled room/corridor on an arbitrary floor: template-floor per cell
    (template-floor-hole for trap cells), perimeter template-wall where the
    footprint borders empty space.  Holes stay inside the footprint so no wall
    rings them — they read as an open pit the player drops through."""
    for (ix, iz) in sorted(footprint):
        wx, wz = world_x(gx, ix), world_z(gz, iz)
        stem = "template-floor-hole" if (ix, iz) in holes else "template-floor"
        pieces.append({"stem": stem, "x": wx, "z": wz, "yaw": 0.0,
                       "floor_level": floor, "scale": 1.0, "group_id": 1})
        for side, (dx, dz) in DELTA.items():
            if (ix + dx, iz + dz) not in footprint:
                pieces.append({
                    "stem": "template-wall",
                    "x": wx + dx * CELL * 0.5, "z": wz + dz * CELL * 0.5,
                    "yaw": WALL_YAW[side], "floor_level": floor,
                    "scale": 1.0, "group_id": 1,
                })


def emit_pieces(fm: FreeformMap, holes0: Optional[Set[Cell]] = None) -> List[dict]:
    pieces: List[dict] = []
    holes0 = holes0 or set()

    def add(stem: str, ix: int, iz: int, yaw: float) -> None:
        pieces.append({
            "stem": stem,
            "x": world_x(fm.gx, ix),
            "z": world_z(fm.gz, iz),
            "yaw": yaw,
            "floor_level": 0,
            "scale": 1.0,
            "group_id": 1,
        })

    def add_wall(ix: int, iz: int, side: str) -> None:
        dx, dz = DELTA[side]
        pieces.append({
            "stem": "template-wall",
            "x": world_x(fm.gx, ix) + dx * CELL * 0.5,
            "z": world_z(fm.gz, iz) + dz * CELL * 0.5,
            "yaw": WALL_YAW[side],
            "floor_level": 0,
            "scale": 1.0,
            "group_id": 1,
        })

    for (ix, iz) in sorted(fm.walkable):
        if (ix, iz) in holes0:
            add("template-floor-hole", ix, iz, 0.0)  # floor-0 extraction trap
            continue
        faces = [s for s, (dx, dz) in DELTA.items() if (ix + dx, iz + dz) in fm.walkable]
        if (ix, iz) in fm.corridor_cells:
            n = len(faces)
            if n == 0:
                add("template-floor", ix, iz, 0.0)
            elif n == 1:
                add("corridor-end", ix, iz, CORRIDOR_END_YAW[faces[0]])
            elif n == 2:
                fs = frozenset(faces)
                if fs in (frozenset({'N', 'S'}), frozenset({'E', 'W'})):
                    add("corridor", ix, iz, PI32 if 'N' in fs else 0.0)
                else:
                    add("corridor-corner", ix, iz, CORNER_YAW.get(fs, 0.0))
            elif n == 3:
                missing = ({'N', 'S', 'E', 'W'} - set(faces)).pop()
                add("corridor-junction", ix, iz, JUNC_YAW[missing])
            else:
                add("corridor-intersection", ix, iz, 0.0)
        else:
            # Room cell: floor + perimeter walls where it borders non-walkable.
            # Corridor neighbours stay open (the doorway); corridor GLBs supply
            # their own side walls so we never wall a corridor cell here.
            add("template-floor", ix, iz, 0.0)
            for side, (dx, dz) in DELTA.items():
                if (ix + dx, iz + dz) not in fm.walkable:
                    add_wall(ix, iz, side)

    return pieces


def _mask(gx: int, gz: int, footprint: Set[Cell], holes: Set[Cell]) -> dict:
    cells = [False] * (gx * gz)
    for (ix, iz) in footprint:
        if (ix, iz) not in holes and 0 <= ix < gx and 0 <= iz < gz:
            cells[iz * gx + ix] = True
    return {"cells_x": gx, "cells_z": gz, "cells": cells}


def to_doc(fm: FreeformMap, name: str) -> dict:
    hub = fm.hub
    gx, gz = fm.gx, fm.gz
    holes0 = {hub.trap0} if hub else set()

    # Floor 0 — main level; the extraction trap cell is carved out of the mask.
    mask0 = [False] * (gx * gz)
    for (ix, iz) in fm.walkable:
        mask0[iz * gx + ix] = (ix, iz) != (hub.trap0 if hub else None)

    pieces = emit_pieces(fm, holes0)
    floors = {"0": {"cells_x": gx, "cells_z": gz, "cells": mask0}}
    hub_exits: Dict[str, dict] = {}

    if hub:
        emit_floor_tiles(pieces, gx, gz, -1, hub.floor1, hub.holes1)
        emit_floor_tiles(pieces, gx, gz, -2, hub.floor2, set())
        floors["-1"] = _mask(gx, gz, hub.floor1, hub.holes1)
        floors["-2"] = _mask(gx, gz, hub.floor2, set())
        for i, ex in enumerate(hub.exits):
            hub_exits[str(i)] = {
                "x": world_x(gx, ex.trap[0]), "z": world_z(gz, ex.trap[1]),
                "floor": -2, "kind": ex.kind, "label": f"Next level {i + 1}",
            }

    # Roofs last — normal template-floor one level up, only where empty.
    emit_all_roofs(pieces, fm, hub, gx, gz, holes0)

    spawn = fm.rooms[fm.spawn_room]
    return {
        "version": 1,
        "name": name,
        "hub_model": "freeform_v1",
        "modules_x": max(1, gx // 5),
        "modules_z": max(1, gz // 5),
        "floors": floors,
        "pieces": pieces,
        "spawn_xz": [world_x(gx, spawn.cx), world_z(gz, spawn.cz)],
        "extraction_xz": [world_x(gx, hub.trap0[0]) if hub else 0.0,
                          world_z(gz, hub.trap0[1]) if hub else 0.0],
        "hub_exits": hub_exits,
    }


def ascii_map(fm: FreeformMap) -> str:
    spawn = (fm.rooms[fm.spawn_room].cx, fm.rooms[fm.spawn_room].cz)
    end = (fm.rooms[fm.end_room].cx, fm.rooms[fm.end_room].cz)
    hidden_cells: Set[Cell] = set()
    for i in fm.hidden_rooms:
        hidden_cells |= set(fm.rooms[i].cells())
    lines = []
    for iz in range(fm.gz):
        row = []
        for ix in range(fm.gx):
            c = (ix, iz)
            if c == spawn:
                row.append('S')
            elif c == end:
                row.append('X')
            elif c in hidden_cells:
                row.append('h')  # hidden / secret dead-end room
            elif c in fm.room_cells:
                row.append('#')
            elif c in fm.corridor_cells:
                row.append('+')
            elif c in fm.wide_cells:
                row.append('=')  # 2-wide corridor (room-style floor)
            else:
                row.append('·')
        lines.append(''.join(row))
    return '\n'.join(lines)


# ─── validation ──────────────────────────────────────────────────────────────

def audit_floor_overlaps(pieces: List[dict], *, eps: float = 0.01) -> List[str]:
    """Two solid floor tiles at the same floor + centre → z-fighting."""
    errs: List[str] = []
    seen: Dict[Tuple[int, float, float], str] = {}
    for p in pieces:
        if not _is_solid_floor(p["stem"]):
            continue
        fl = int(p.get("floor_level", 0))
        wx, wz = float(p["x"]), float(p["z"])
        key = (fl, round(wx / eps) * eps, round(wz / eps) * eps)
        if key in seen:
            errs.append(
                f"duplicate floor at level {fl} ({wx:.1f},{wz:.1f}): "
                f"{seen[key]!r} + {p['stem']!r}"
            )
        else:
            seen[key] = p["stem"]
    return errs


def validate(fm: FreeformMap, doc: Optional[dict] = None) -> List[str]:
    errs: List[str] = []
    # All walkable cells reachable from spawn cell (flood fill, 4-connected).
    start = (fm.rooms[fm.spawn_room].cx, fm.rooms[fm.spawn_room].cz)
    seen = {start}
    stack = [start]
    while stack:
        x, z = stack.pop()
        for dx, dz in DELTA.values():
            nb = (x + dx, z + dz)
            if nb in fm.walkable and nb not in seen:
                seen.add(nb)
                stack.append(nb)
    if len(seen) != len(fm.walkable):
        errs.append(f"walkable reachability {len(seen)}/{len(fm.walkable)} (disconnected)")
    end = (fm.rooms[fm.end_room].cx, fm.rooms[fm.end_room].cz)
    if end not in seen:
        errs.append("extraction room not reachable from spawn")
    if fm.hub:
        if len(fm.hub.exits) < 2:
            errs.append("hub needs 2 exits")
        elif fm.hub.exits[0].landing & fm.hub.exits[1].landing:
            errs.append("hub floor -2 landings overlap")
    if doc is not None and fm.hub:
        pieces = doc.get("pieces", [])
        gx, gz = fm.gx, fm.gz
        for c in fm.hub.holes1:
            wx, wz = world_x(gx, c[0]), world_z(gz, c[1])
            has_f0 = _has_solid_floor_at(pieces, 0, wx, wz)
            has_ceil = any(
                p.get("ceiling")
                and int(p.get("floor_level", 0)) == 0
                and abs(float(p["x"]) - wx) < 0.01
                and abs(float(p["z"]) - wz) < 0.01
                for p in pieces
            )
            if not has_f0 and not has_ceil:
                errs.append(f"hub trap {c} missing ceiling at floor 0")
        # Every walkable floor-0 tile (trap included) must have a ceiling slab at floor 1
        # so the roof is continuous — no hole above the trapdoor.
        for c in fm.walkable:
            wx, wz = world_x(gx, c[0]), world_z(gz, c[1])
            has_ceil = any(
                p.get("ceiling")
                and int(p.get("floor_level", 0)) == 1
                and abs(float(p["x"]) - wx) < 0.01
                and abs(float(p["z"]) - wz) < 0.01
                for p in pieces
            )
            if not has_ceil:
                errs.append(f"walkable cell {c} missing roof (ceiling at floor 1)")
        for i, ex in enumerate(fm.hub.exits):
            trap_cells = ex.landing & fm.hub.holes1
            for c in ex.landing - trap_cells:
                wx, wz = world_x(gx, c[0]), world_z(gz, c[1])
                has_f1 = _has_solid_floor_at(pieces, -1, wx, wz)
                has_ceil = any(
                    p.get("ceiling")
                    and int(p.get("floor_level", 0)) == -1
                    and abs(float(p["x"]) - wx) < 0.01
                    and abs(float(p["z"]) - wz) < 0.01
                    for p in pieces
                )
                if has_f1 and has_ceil:
                    errs.append(
                        f"exit {i} landing cell {c} already has f−1 floor but got roof slab"
                    )
                elif not has_f1 and not has_ceil:
                    errs.append(f"exit {i} landing cell {c} missing roof at floor −1")
        hub_ext = (fm.hub.floor1 - fm.hub.holes1) - fm.walkable
        for c in fm.hub.floor1 & fm.walkable:
            wx, wz = world_x(gx, c[0]), world_z(gz, c[1])
            if any(
                p.get("ceiling")
                and int(p.get("floor_level", 0)) == 0
                and abs(float(p["x"]) - wx) < 0.01
                and abs(float(p["z"]) - wz) < 0.01
                for p in pieces
            ):
                errs.append(f"hub cell {c} under floor-0 walkable must not get ceiling slab")
            if any(
                p.get("underside")
                and abs(float(p["x"]) - wx) < 0.01
                and abs(float(p["z"]) - wz) < 0.01
                for p in pieces
            ):
                errs.append(f"hub cell {c} floor-0 tile must not be tagged underside")
        for c in hub_ext:
            wx, wz = world_x(gx, c[0]), world_z(gz, c[1])
            has_func = _has_solid_floor_at(pieces, 0, wx, wz)
            has_ceil = any(
                p.get("ceiling")
                and int(p.get("floor_level", 0)) == 0
                and abs(float(p["x"]) - wx) < 0.01
                and abs(float(p["z"]) - wz) < 0.01
                for p in pieces
            )
            if not has_func and not has_ceil:
                errs.append(f"hub extension cell {c} missing ceiling at floor 0")
            if has_func and has_ceil:
                errs.append(f"hub extension cell {c} duplicate floor+ceiling at floor 0")
    if doc is not None:
        errs.extend(audit_floor_overlaps(doc.get("pieces", [])))
    return errs


# ─── report / output ─────────────────────────────────────────────────────────

def build_report(fm: FreeformMap, doc: dict, seed: Optional[int], elapsed: float, path: str) -> dict:
    return {
        "ok": True,
        "path": path.replace("\\", "/"),
        "seed": seed,
        "spawn": [fm.rooms[fm.spawn_room].cx, fm.rooms[fm.spawn_room].cz],
        "end": [fm.rooms[fm.end_room].cx, fm.rooms[fm.end_room].cz],
        "avg_degree": 0.0,
        "rooms": len(fm.rooms),
        "pieces": len(doc.get("pieces", [])),
        "paint": ascii_map(fm),
        "paint_roles": ascii_map(fm),
        "elapsed_s": round(elapsed, 2),
    }


def run(
    *,
    seed: Optional[int],
    attempts: int,
    out_path: Path,
    export_layout: bool,
    name: str = "freeform",
    cells: int = 25,
    max_rooms: int = 11,
    room_min: int = 3,
    room_max: int = 7,
    loops: int = 3,
    organicness: float = 0.0,
    corridor_width: float = 1.0,
    hidden_area_prevalence: float = 0.0,
) -> dict:
    """Generate one free-form map, write it, export the layout, return a report.

    Shared by the CLI and by `gen_maps.generate_map_report` (editor Proc tab),
    so the editor preview produces free-form maps without a Rust rebuild.
    """
    import gen_maps  # reuse layout export

    t0 = time.time()
    fm: Optional[FreeformMap] = None
    base_seed = seed if seed is not None else random.randint(0, 2**31 - 1)
    for k in range(max(1, attempts)):
        cand = generate_map(
            base_seed + k, cells=cells, max_rooms=max_rooms,
            room_min=room_min, room_max=room_max, loops=loops,
            organicness=organicness, corridor_width=corridor_width,
            hidden_area_prevalence=hidden_area_prevalence,
        )
        if cand and not validate(cand):
            fm = cand
            break
    if fm is None:
        return {"ok": False, "seed": seed, "error": "no valid free-form map",
                "elapsed_s": round(time.time() - t0, 2)}

    doc = to_doc(fm, name)
    overlap = audit_floor_overlaps(doc.get("pieces", []))
    if overlap:
        return {
            "ok": False,
            "seed": seed,
            "error": "; ".join(overlap[:3]),
            "elapsed_s": round(time.time() - t0, 2),
        }
    geo = validate(fm, doc)
    if geo:
        return {
            "ok": False,
            "seed": seed,
            "error": "; ".join(geo[:3]),
            "elapsed_s": round(time.time() - t0, 2),
        }
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(doc, indent=2) + "\n", encoding="utf-8")
    if export_layout:
        gen_maps.export_kenney_layout(doc)
    return build_report(fm, doc, seed, time.time() - t0, str(out_path))


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--seed', type=int, default=None)
    ap.add_argument('--cells', type=int, default=25, help='Grid size in cells (square)')
    ap.add_argument('--rooms', type=int, default=11, help='Max rooms')
    ap.add_argument('--room-min', type=int, default=3)
    ap.add_argument('--room-max', type=int, default=7)
    ap.add_argument('--loops', type=int, default=3)
    ap.add_argument('--organicness', type=float, default=0.0,
                    help='0=clean L corridors, 1=winding jogged routes')
    ap.add_argument('--corridor-width', type=float, default=1.0,
                    help='1.0=all 1-wide, 2.0=all 2-wide, 1.3=~30%% 2-wide')
    ap.add_argument('--hidden', type=float, default=0.0,
                    help='hidden-area prevalence 0-1 (dead-end secret rooms)')
    ap.add_argument('--attempts', type=int, default=20, help='Generation retries')
    ap.add_argument('--out', default=None)
    ap.add_argument('--preview', action='store_true')
    ap.add_argument('--no-layout-export', action='store_true')
    ap.add_argument('--show', action='store_true', help='Print ASCII map')
    args = ap.parse_args()

    out_path = Path(args.out) if args.out else (
        MAP_DIR / "_editor_preview.json" if args.preview
        else MAP_DIR / f"gen_map_{time.strftime('%m%d%H%M%S')}.json"
    )
    report = run(
        seed=args.seed, attempts=args.attempts, out_path=out_path,
        export_layout=not args.no_layout_export,
        cells=args.cells, max_rooms=args.rooms, room_min=args.room_min,
        room_max=args.room_max, loops=args.loops, organicness=args.organicness,
        corridor_width=args.corridor_width, hidden_area_prevalence=args.hidden,
    )
    if args.preview:
        print(json.dumps(report))
        raise SystemExit(0 if report.get("ok") else 1)
    if not report.get("ok"):
        print(f"Failed: {report.get('error')}")
        raise SystemExit(1)
    print(f"Free-form map: {report['rooms']} rooms, {report['pieces']} pieces, "
          f"{report['elapsed_s']}s")
    print(f"  spawn={report['spawn']} extraction={report['end']} cells={args.cells}")
    print(f"  wrote {out_path}")
    print(f"  exported {LAYOUT_PATH}")
    if args.show:
        print(report['paint'])


if __name__ == '__main__':
    import sys
    sys.path.insert(0, str(Path(__file__).parent))
    main()
