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
from typing import Callable, Dict, List, Optional, Set, Tuple, Union

import faction_interior as fi

ROOT = Path(__file__).resolve().parent.parent

try:
    from pygltflib import GLTF2
    _HAS_PYGLTFLIB = True
except Exception:
    _HAS_PYGLTFLIB = False

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

# Cache for model Y bounds (min_y, max_y) per (kit, stem) to support proper wall alignment
_MODEL_Y_BOUNDS: Dict[Tuple[str, str], Tuple[float, float]] = {}

def _get_model_y_bounds(kit: str, stem: str) -> Tuple[float, float]:
    """Return (min_y, max_y) in model units for the given stem in the kit folder.
    Falls back to (0, 1) if can't load.
    """
    key = (kit, stem)
    if key in _MODEL_Y_BOUNDS:
        return _MODEL_Y_BOUNDS[key]
    if not _HAS_PYGLTFLIB:
        _MODEL_Y_BOUNDS[key] = (0.0, 1.0)
        return _MODEL_Y_BOUNDS[key]
    try:
        # kit like "factions/necropolis" -> folder necropolis
        kit_id = kit.split('/')[-1] if '/' in kit else kit
        path = ROOT / "assets" / "models" / "factions" / kit_id / f"{stem}.glb"
        if not path.exists():
            _MODEL_Y_BOUNDS[key] = (0.0, 1.0)
            return _MODEL_Y_BOUNDS[key]
        g = GLTF2().load(str(path))
        if not g.meshes or not g.meshes[0].primitives:
            _MODEL_Y_BOUNDS[key] = (0.0, 1.0)
            return _MODEL_Y_BOUNDS[key]
        pos_idx = g.meshes[0].primitives[0].attributes.POSITION
        acc = g.accessors[pos_idx]
        min_y, max_y = acc.min[1], acc.max[1]
        _MODEL_Y_BOUNDS[key] = (min_y, max_y)
        return min_y, max_y
    except Exception:
        _MODEL_Y_BOUNDS[key] = (0.0, 1.0)
        return _MODEL_Y_BOUNDS[key]

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
    # (world_x, world_z, yaw) of gate-door pieces sealing each hidden entrance.
    secret_doors: List[Tuple[float, float, float]] = field(default_factory=list)
    # Per-door geometry audit trail (parallel to secret_doors / hidden_rooms).
    secret_door_meta: List["SecretDoorMeta"] = field(default_factory=list)
    # Faction profile used when mix_mode=single; composition when transition.
    faction_profile_id: str = "industrial_default"
    composition: Optional["LevelComposition"] = None
    enemy_spawns: List[Tuple[float, float, float]] = field(default_factory=list)
    npc_spawns: List[Tuple[float, float, float]] = field(default_factory=list)
    # Per-enemy patrol waypoint loops, parallel to enemy_spawns ([] = wander).
    enemy_patrols: List[List[Tuple[float, float, float]]] = field(default_factory=list)


@dataclass(frozen=True)
class SecretDoorMeta:
    """Ground-truth entrance cells recorded when a hidden room is linked."""
    parent_room: int
    hidden_room: int
    room_cell: Cell          # parent boundary cell (carve ax, az)
    corridor_cell: Cell      # chosen link cell touching room_cell
    link_cells: frozenset    # full corridor link at placement time
    adjacent_link: Tuple[Cell, ...]  # all link cells touching room_cell


# ─── geometry ────────────────────────────────────────────────────────────────

def world_x(gx: int, ix: int) -> float:
    return -(gx * CELL) * 0.5 + ix * CELL + CELL * 0.5


def world_z(gz: int, iz: int) -> float:
    return -(gz * CELL) * 0.5 + iz * CELL + CELL * 0.5


# ─── generation ──────────────────────────────────────────────────────────────

def _jitter_room(rng: random.Random, room: Room, strength: float, rmin: int) -> Room:
    """Chop random edges off a rectangle — finishes organicness for room silhouettes."""
    if strength < 0.05:
        return room
    x0, z0, w, h = room.x0, room.z0, room.w, room.h
    n = int(strength * 4) + (1 if rng.random() < strength * 0.5 else 0)
    for _ in range(n):
        if w <= rmin and h <= rmin:
            break
        side = rng.randint(0, 3)
        if side == 0 and h > rmin:
            z0 += 1
            h -= 1
        elif side == 1 and h > rmin:
            h -= 1
        elif side == 2 and w > rmin:
            x0 += 1
            w -= 1
        elif side == 3 and w > rmin:
            w -= 1
    return Room(x0, z0, w, h)


def place_rooms(
    rng: random.Random, gx: int, gz: int, max_rooms: int,
    rmin: int, rmax: int, tries: int, organicness: float = 0.0,
    halls: int = 0,
) -> List[Room]:
    rooms: List[Room] = []
    # Factory-hall seeding: a few guaranteed-LARGE rooms placed first (before the
    # general fill claims the space), biased toward the middle of the grid so they
    # tend to land in the default (industrial) zone band. Halls stay rectangular
    # (no organicness jitter) — the double-height/catwalk treatment needs clean
    # perimeter walls.
    hall_min = max(rmin + 2, 5)
    hall_max = max(hall_min, rmax + 1)
    for _ in range(halls):
        for _try in range(80):
            w = rng.randint(hall_min, hall_max)
            h = rng.randint(hall_min, hall_max)
            if w + 2 >= gx or h + 2 >= gz:
                continue
            x_lo = max(1, int(gx * 0.2) - w // 2)
            x_hi = min(gx - w - 1, int(gx * 0.8) - w // 2)
            z_lo = max(1, int(gz * 0.2) - h // 2)
            z_hi = min(gz - h - 1, int(gz * 0.8) - h // 2)
            if x_lo > x_hi or z_lo > z_hi:
                continue
            cand = Room(rng.randint(x_lo, x_hi), rng.randint(z_lo, z_hi), w, h)
            if any(cand.overlaps(r) for r in rooms):
                continue
            rooms.append(cand)
            break
    for _ in range(tries):
        if len(rooms) >= max_rooms:
            break
        w = rng.randint(rmin, rmax)
        h = rng.randint(rmin, rmax)
        if w + 2 >= gx or h + 2 >= gz:
            continue
        x0 = rng.randint(1, gx - w - 1)
        z0 = rng.randint(1, gz - h - 1)
        cand = _jitter_room(rng, Room(x0, z0, w, h), organicness, rmin)
        if any(cand.overlaps(r) for r in rooms):
            continue
        rooms.append(cand)
    return rooms


def _piece(
    kit: Optional[str],
    *,
    stem: str,
    x: float,
    z: float,
    yaw: float,
    floor_level: int = 0,
    scale: float = 1.0,
    group_id: int = 1,
    ceiling: bool = False,
    underside: bool = False,
    zone: Optional[str] = None,
    role: Optional[str] = None,
) -> dict:
    p: dict = {
        "stem": stem,
        "x": x,
        "z": z,
        "yaw": yaw,
        "floor_level": floor_level,
        "scale": scale,
        "group_id": group_id,
    }
    if ceiling:
        p["ceiling"] = True
    if underside:
        p["underside"] = True
    # Per-zone architecture kit is resolved from the faction manifests by piece
    # ROLE (see build_zone_kits / _ACTIVE_ZONE_KITS). None / missing = default
    # space grammar. Ceilings are always space. Callers that emit custom faction
    # stems pass `role` explicitly (the stem can't be classified by prefix);
    # others fall back to inferring the role from the stem.
    lookup_role = role if role is not None else _piece_role(stem)
    kit = None if ceiling else _ACTIVE_ZONE_KITS.get(zone or "", {}).get(lookup_role)
    if kit:
        p["kit"] = kit
    if zone:
        p["zone"] = zone
    # Stamp the architectural role so floor/roof audits recognise faction-custom
    # stems (e.g. urban's "road-asphalt-center" floor) by role, not stem prefix.
    p["role"] = lookup_role
    return p


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


def _hidden_entrance_pose(
    parent: Room, cand: Room, link: Set[Cell], gx: int, gz: int,
) -> Tuple[float, float, float, Cell, Cell, Tuple[Cell, ...]]:
    """Wall-plane pose + the cells used to derive it (for probes).

    Returns (wx, wz, yaw, room_cell, corridor_cell, adjacent_link).
    """
    bx, bz = cand.nearest_cell(parent.cx, parent.cz)
    border = [
        c for c in parent.cells()
        if any((c[0] + dx, c[1] + dz) in link for dx, dz in DELTA.values())
    ]
    if border:
        ax, az = min(border, key=lambda c: (c[0] - bx) ** 2 + (c[1] - bz) ** 2)
    else:
        ax, az = parent.nearest_cell(cand.cx, cand.cz)
    adjacent = [
        (ax + dx, az + dz)
        for dx, dz in DELTA.values()
        if (ax + dx, az + dz) in link
    ]
    if adjacent:
        nx, nz = min(adjacent, key=lambda c: (c[0] - bx) ** 2 + (c[1] - bz) ** 2)
        dx, dz = nx - ax, nz - az
    else:
        nx, nz = ax, az
        ex, ez = bx - ax, bz - az
        dx, dz = (
            (1 if ex > 0 else -1, 0) if abs(ex) >= abs(ez)
            else (0, 1 if ez > 0 else -1)
        )
    yaw = 0.0 if dz != 0 else math.pi / 2
    return (
        world_x(gx, ax) + dx * CELL * 0.5,
        world_z(gz, az) + dz * CELL * 0.5,
        yaw,
        (ax, az),
        (nx, nz),
        tuple(adjacent),
    )


def place_hidden_rooms(
    rng: random.Random, rooms: List[Room], room_cells: Set[Cell],
    corridor_cells: Set[Cell], wide_cells: Set[Cell],
    gx: int, gz: int, room_min: int, prevalence: float,
) -> Tuple[List[int], List[Tuple[float, float, float]], List[SecretDoorMeta]]:
    """Append small single-entrance dead-end rooms.

    Returns (hidden_room_indices, secret_doors, secret_door_meta). Mutates
    `rooms`, `room_cells`, and `corridor_cells` in place.
    """
    n_target = round(prevalence * 4)
    if n_target <= 0:
        return [], [], []
    hidden: List[int] = []
    doors: List[Tuple[float, float, float]] = []
    meta: List[SecretDoorMeta] = []
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
        wx, wz, yaw, room_cell, corridor_cell, adjacent = _hidden_entrance_pose(
            rooms[parent], cand, link, gx, gz)
        doors.append((wx, wz, yaw))
        meta.append(SecretDoorMeta(
            parent_room=parent,
            hidden_room=len(rooms),  # index after append
            room_cell=room_cell,
            corridor_cell=corridor_cell,
            link_cells=frozenset(link),
            adjacent_link=adjacent,
        ))
        rooms.append(cand)
        hidden.append(len(rooms) - 1)
        room_cells |= cc
        corridor_cells |= link - room_cells
        occupied |= cc | link | halo
    return hidden, doors, meta


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
    faction_profile_id: str = "industrial_default",
    composition: Optional["LevelComposition"] = None,
    room_tries: int = 400,
    num_enemies: int = 5,
    num_npcs: int = 3,
    hall_rooms: int = 2,
) -> Optional[FreeformMap]:
    import level_composition as lc

    comp = (composition or lc.LevelComposition(mix_mode="single")).normalized()
    rng = random.Random(seed)
    gx = gz = cells
    rooms = place_rooms(rng, gx, gz, max_rooms, room_min, room_max, room_tries, organicness,
                        halls=hall_rooms)
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
    hidden_rooms, secret_doors, secret_door_meta = place_hidden_rooms(
        rng, rooms, room_cells, corridor_cells, wide_cells, gx, gz,
        room_min, hidden_area_prevalence)
    if hidden_rooms:
        walkable = room_cells | corridor_cells | wide_cells

    fm = FreeformMap(
        gx, gz, rooms, walkable, room_cells, corridor_cells, spawn_i, end_i,
        seed=seed if seed is not None else 0,
        wide_cells=wide_cells,
        hidden_rooms=hidden_rooms,
        secret_doors=secret_doors,
        secret_door_meta=secret_door_meta,
        faction_profile_id=faction_profile_id,
        composition=comp,
    )
    hub_rng = random.Random((fm.seed * 2654435761) & 0xFFFFFFFF)
    for _ in range(32):
        hub = build_hub(hub_rng, fm)
        if hub is not None:
            fm.hub = hub
            break
        hub_rng = random.Random(hub_rng.randint(0, 2**31 - 1))
    # Place agent spawns AFTER the hub is built: NPCs go to the hub + hidden
    # rooms only; enemies roam the walkable floor.
    _place_agent_spawns(fm, rng, num_enemies, num_npcs)
    if fm.hub is None:
        return None
    return fm


# ─── tile emission ───────────────────────────────────────────────────────────

def _world_to_cell(gx: int, gz: int, wx: float, wz: float) -> Cell:
    ix = int(round((wx + gx * CELL * 0.5 - CELL * 0.5) / CELL))
    iz = int(round((wz + gz * CELL * 0.5 - CELL * 0.5) / CELL))
    return (ix, iz)


def _face_cells(gx: int, gz: int, wx: float, wz: float) -> List[Cell]:
    """Cell(s) a piece sits over. Centred pieces → 1 cell; a wall on a cell **face**
    (half-cell offset) → both cells sharing that face, so elevation is robust to which
    side the float rounds to (a synth/default seam wall belongs to the synth side)."""
    fx = (wx + gx * CELL * 0.5 - CELL * 0.5) / CELL
    fz = (wz + gz * CELL * 0.5 - CELL * 0.5) / CELL
    ix, iz = round(fx), round(fz)
    cells = [(ix, iz)]
    if abs(fx - ix) > 0.25:  # wall on an E-W face (vertical grid line)
        cells = [(int(round(fx - 0.5)), iz), (int(round(fx + 0.5)), iz)]
    elif abs(fz - iz) > 0.25:  # wall on a N-S face (horizontal grid line)
        cells = [(ix, int(round(fz - 0.5))), (ix, int(round(fz + 0.5)))]
    return cells


# ---------------------------------------------------------------------------
# Faction-zone CATALOGUE: which architecture kit each composition zone uses for
# each piece ROLE. A transition map runs prev -> default -> next zones, mapped to
# faction looks. `None`/missing role = the default `space/` grammar (which then
# takes a per-zone TINT material in the client, e.g. pink for `prev`).
#
# Roles: "floor" (template-floor*), "wall" (template-wall*), "corridor"
# (corridor*). Ceilings are always space (handled in _piece). Swapping a faction
# to its own `factions/<name>/` folder = changing the kit string(s) here.
# Faction-zone kit resolution is DATA-DRIVEN from the faction asset manifests
# (assets/models/factions/<id>/faction.json, loaded via tools/faction_assets.py).
# `_ACTIVE_ZONE_KITS` maps composition zone ("prev"/"default"/"next") -> piece
# ROLE -> kit folder, and `_ACTIVE_FLOORLESS` is the set of kits whose
# corridor-corner has no floor mesh. Both are rebuilt per map by build_zone_kits()
# from the actual faction in each zone, so the look FOLLOWS the faction (reorder
# the factions and the visuals move with them). Empty default = pure space grammar.
_ACTIVE_ZONE_KITS: Dict[str, Dict[str, str]] = {}
_ACTIVE_FLOORLESS: Set[str] = set()
_ACTIVE_WALLS_ONLY_CORNERS: Set[str] = set()
# Per-zone emit-slot ("floor"/"wall"/"corner") overrides from faction manifests:
# stems (default = template-*/corridor-corner) and a uniform scale (default 1.0).
_ACTIVE_ZONE_STEMS: Dict[str, Dict[str, str]] = {}
_ACTIVE_ZONE_SCALE: Dict[str, Dict[str, float]] = {}
# Per-zone, per-slot calibration: yaw_offset (radians added to the placement yaw)
# and inset (world units pushing a wall out along its side normal).
_ACTIVE_ZONE_YAW: Dict[str, Dict[str, float]] = {}
_ACTIVE_ZONE_INSET: Dict[str, Dict[str, float]] = {}


def _piece_role(stem: str) -> str:
    """Architectural role of a piece for faction-zone kit lookup."""
    if stem.startswith("template-floor"):
        return "floor"
    if stem.startswith("template-wall"):
        return "wall"
    if stem.startswith("corridor"):
        return "corridor"
    return "other"


def build_zone_kits(comp) -> None:
    """Resolve each composition zone's faction to its per-role kit, from manifests.

    A faction WITH an asset manifest routes only its ``provides`` roles to its
    ``factions/<id>`` folder (unprovided roles fall back to the space grammar).
    A faction WITHOUT a manifest falls back to its whole ``building_system`` kit
    for every role (legacy behaviour, e.g. synth -> space_station). Sets the
    module-level ``_ACTIVE_ZONE_KITS`` / ``_ACTIVE_FLOORLESS`` used by _piece.
    """
    import faction_assets as fa
    import faction_profiles as fp

    assets = fa.load_all()
    zones = {
        "prev": comp.prev_faction,
        "default": comp.default_faction,
        "next": comp.next_faction,
    }
    zone_kits: Dict[str, Dict[str, str]] = {}
    zone_stems: Dict[str, Dict[str, str]] = {}
    zone_scale: Dict[str, Dict[str, float]] = {}
    zone_yaw: Dict[str, Dict[str, float]] = {}
    zone_inset: Dict[str, Dict[str, float]] = {}
    # emit slot -> manifest role
    SLOT_ROLE = (("floor", "floor"), ("wall", "wall"), ("corner", "corridor"))
    for zone, profile_id in zones.items():
        asset = fa.asset_for_profile(profile_id, assets)
        zone_stems[zone] = {}
        zone_scale[zone] = {}
        zone_yaw[zone] = {}
        zone_inset[zone] = {}
        if asset is not None:
            zone_kits[zone] = {
                role: asset.kit for role in asset.provides if role in fa.ROLES
            }
            for slot, role in SLOT_ROLE:
                if role in asset.provides:
                    rd = asset.roles.get(role, {})
                    st = asset.stem(slot)
                    if st:
                        zone_stems[zone][slot] = st
                    if asset.scale != 1.0:
                        zone_scale[zone][slot] = asset.scale
                    # Per-piece calibration vs the generator's edge placement:
                    # yaw_offset rotates the piece to match native facing; inset
                    # pushes a wall out along its side normal to correct an
                    # off-centre depth anchor (world units, post-scale).
                    if rd.get("yaw_offset"):
                        zone_yaw[zone][slot] = float(rd["yaw_offset"])
                    if rd.get("inset"):
                        zone_inset[zone][slot] = float(rd["inset"])
        else:
            kit = fp.architecture_kit(fp.load_profile(profile_id))
            zone_kits[zone] = {role: kit for role in fa.ROLES} if kit else {}
    global _ACTIVE_ZONE_KITS, _ACTIVE_FLOORLESS, _ACTIVE_WALLS_ONLY_CORNERS
    global _ACTIVE_ZONE_STEMS, _ACTIVE_ZONE_SCALE
    global _ACTIVE_ZONE_YAW, _ACTIVE_ZONE_INSET
    _ACTIVE_ZONE_KITS = zone_kits
    _ACTIVE_FLOORLESS = fa.floorless_corner_kits(assets)
    _ACTIVE_WALLS_ONLY_CORNERS = fa.walls_only_corner_kits(assets)
    _ACTIVE_ZONE_YAW = zone_yaw
    _ACTIVE_ZONE_INSET = zone_inset
    _ACTIVE_ZONE_STEMS = zone_stems
    _ACTIVE_ZONE_SCALE = zone_scale


def _corner_has_floor(stem: str, kit: Optional[str]) -> bool:
    """A corridor-corner provides floor geometry unless its kit's GLB is floorless."""
    return not (stem == "corridor-corner" and kit in _ACTIVE_FLOORLESS)


def _piece_floor_surface(p: dict) -> bool:
    """True when a piece provides floor geometry at its cell (incl. hole frames).

    Role-aware: faction floors use custom stems (e.g. urban "road-asphalt-center")
    so we classify by the stamped `role`, falling back to stem prefix for legacy /
    hub pieces that carry no role.
    """
    role = p.get("role")
    stem = p["stem"]
    kit = p.get("kit")
    if role == "wall":
        return False
    if role == "corridor":  # a corner piece
        return kit not in _ACTIVE_FLOORLESS
    if role == "floor" or role == "deck":
        return True
    if stem.startswith("floor") and kit in ("space_station", "factions/synth"):
        return True
    if stem.startswith("corridor"):
        return _corner_has_floor(stem, kit)
    if stem.startswith("template-floor"):
        return True
    return False


def _piece_solid_floor(p: dict) -> bool:
    """Walkable / ceiling slab — excludes trap-door hole frames (open vertical)."""
    if p["stem"] == "template-floor-hole":
        return False
    return _piece_floor_surface(p)


def _has_floor_surface_at(
    pieces: List[dict], floor: int, wx: float, wz: float, *, eps: float = 0.01
) -> bool:
    """Any floor GLB at this centre (incl. trap-door hole frames, excl. ceiling slabs)."""
    for p in pieces:
        if int(p.get("floor_level", 0)) != floor:
            continue
        if p.get("ceiling"):
            continue
        if not _piece_floor_surface(p):
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
        if not _piece_solid_floor(p):
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
    kit_lookup: Optional[KitLookup] = None,
    kit: Optional[str] = None,
    zone_lookup: Optional[ZoneLookup] = None,
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
        pieces.append(_piece(
            _cell_kit(kit_lookup, (ix, iz), kit),
            stem="template-floor", x=wx, z=wz, yaw=0.0,
            floor_level=roof_floor, ceiling=True,
            zone=_cell_zone(zone_lookup, (ix, iz)),
        ))


def _arrival_shaft_cell(fm: FreeformMap) -> Optional[Cell]:
    """The spawn-room centre cell whose floor-1 roof is left OPEN so a streamed
    child map can be dropped into from the hub exit directly above it. Returns
    None if there is no spawn room. Shared by the roof emitter and the audit so
    the intentionally-missing roof isn't flagged as an error."""
    if fm.spawn_room < len(fm.rooms):
        spawn = fm.rooms[fm.spawn_room]
        return (spawn.cx, spawn.cz)
    return None


def emit_all_roofs(
    pieces: List[dict],
    fm: FreeformMap,
    hub: Optional[Hub],
    gx: int,
    gz: int,
    holes0: Optional[Set[Cell]] = None,
    kit_lookup: Optional[KitLookup] = None,
    kit: Optional[str] = None,
    zone_lookup: Optional[ZoneLookup] = None,
) -> None:
    """Final roof pass — hub extension (−1→0), landings (−2→−1), main level (0→1).

    A ceiling (`template-floor`, ``ceiling: True``) is placed one level above **every**
    walkable floor-0 tile, including the extraction-trap tile (so the roof has no hole
    above the trapdoor — the trap drops you *down*, the roof is one level *up*). The
    "tile under a trap door" that must stay open is the landing cell *below* a hub trap;
    that is handled by ``skip_roof`` in the landing pass. Ceiling slabs are visual-only
    (no collider), so the player walks freely beneath them."""
    holes0 = holes0 or set()
    import level_composition as lc

    if hub:
        hub_extension = (hub.floor1 - hub.holes1) - fm.walkable
        emit_roofs(pieces, gx, gz, -1, hub_extension, kit_lookup=kit_lookup, kit=kit, zone_lookup=zone_lookup)
        for c in hub.holes1:
            emit_roofs(pieces, gx, gz, -1, {c}, kit_lookup=kit_lookup, kit=kit, zone_lookup=zone_lookup)
        for ex in hub.exits:
            emit_roofs(
                pieces, gx, gz, -2, ex.landing,
                skip_roof=ex.landing & hub.holes1,
                kit_lookup=kit_lookup, kit=kit, zone_lookup=zone_lookup,
            )
    skip_roof: Set[Cell] = set()
    comp = (fm.composition or lc.LevelComposition(mix_mode="single")).normalized()
    if comp.mix_mode == "transition":
        spine, _, _, _ = lc.plan_zones_for_map(fm)
        elev_fn = lc.make_elevation_lookup(fm.walkable, spine, comp)
        skip_roof = {c for c in fm.walkable if elev_fn(c) > 0}
    # Arrival shaft: NO roof over the spawn cell. Streamed child maps mount with
    # their spawn directly under the parent's hub exit hole — a colliding roof
    # there sits flush inside the parent's floor and plugs the drop.
    shaft = _arrival_shaft_cell(fm)
    if shaft is not None:
        skip_roof = skip_roof | {shaft}
    emit_roofs(
        pieces, gx, gz, 0, fm.walkable,
        skip_roof=skip_roof,
        kit_lookup=kit_lookup, kit=kit, zone_lookup=zone_lookup,
    )


def emit_floor_tiles(
    pieces: List[dict], gx: int, gz: int, floor: int,
    footprint: Set[Cell], holes: Set[Cell],
    kit: Optional[str] = None,
) -> None:
    """Tiled room/corridor on an arbitrary floor: template-floor per cell
    (template-floor-hole for trap cells), perimeter template-wall where the
    footprint borders empty space.  Holes stay inside the footprint so no wall
    rings them — they read as an open pit the player drops through."""
    for (ix, iz) in sorted(footprint):
        wx, wz = world_x(gx, ix), world_z(gz, iz)
        stem = "template-floor-hole" if (ix, iz) in holes else "template-floor"
        pieces.append(_piece(kit, stem=stem, x=wx, z=wz, yaw=0.0, floor_level=floor))
        for side, (dx, dz) in DELTA.items():
            if (ix + dx, iz + dz) not in footprint:
                pieces.append(_piece(
                    kit, stem="template-wall",
                    x=wx + dx * CELL * 0.5, z=wz + dz * CELL * 0.5,
                    yaw=WALL_YAW[side], floor_level=floor,
                ))


KitLookup = Callable[[Cell], Optional[str]]
ZoneLookup = Callable[[Cell], Optional[str]]


def _cell_kit(lookup: Optional[KitLookup], cell: Cell, fallback: Optional[str]) -> Optional[str]:
    if lookup is not None:
        return lookup(cell)
    return fallback


def _cell_zone(lookup: Optional[ZoneLookup], cell: Cell) -> Optional[str]:
    if lookup is not None:
        return lookup(cell)
    return None


def emit_pieces(
    fm: FreeformMap,
    holes0: Optional[Set[Cell]] = None,
    kit_lookup: Optional[KitLookup] = None,
    kit: Optional[str] = None,
    zone_lookup: Optional[ZoneLookup] = None,
) -> List[dict]:
    pieces: List[dict] = []
    holes0 = holes0 or set()

    def k(ix: int, iz: int) -> Optional[str]:
        return _cell_kit(kit_lookup, (ix, iz), kit)

    def z(ix: int, iz: int) -> Optional[str]:
        return _cell_zone(zone_lookup, (ix, iz))

    def add(stem: str, ix: int, iz: int, yaw: float) -> None:
        pieces.append(_piece(
            k(ix, iz), stem=stem,
            x=world_x(fm.gx, ix), z=world_z(fm.gz, iz), yaw=yaw,
            zone=z(ix, iz),
        ))

    def _slot_stem(ix: int, iz: int, slot: str, default: str) -> str:
        return _ACTIVE_ZONE_STEMS.get(z(ix, iz) or "", {}).get(slot, default)

    def _slot_scale(ix: int, iz: int, slot: str) -> float:
        return _ACTIVE_ZONE_SCALE.get(z(ix, iz) or "", {}).get(slot, 1.0)

    def _slot_yaw(ix: int, iz: int, slot: str) -> float:
        return _ACTIVE_ZONE_YAW.get(z(ix, iz) or "", {}).get(slot, 0.0)

    def _slot_inset(ix: int, iz: int, slot: str) -> float:
        return _ACTIVE_ZONE_INSET.get(z(ix, iz) or "", {}).get(slot, 0.0)

    def add_floor(ix: int, iz: int) -> None:
        pieces.append(_piece(
            None, stem=_slot_stem(ix, iz, "floor", "template-floor"),
            x=world_x(fm.gx, ix), z=world_z(fm.gz, iz), yaw=0.0,
            scale=_slot_scale(ix, iz, "floor"), zone=z(ix, iz), role="floor",
        ))

    def add_corner(ix: int, iz: int, yaw: float) -> None:
        pieces.append(_piece(
            None, stem=_slot_stem(ix, iz, "corner", "corridor-corner"),
            x=world_x(fm.gx, ix), z=world_z(fm.gz, iz),
            yaw=yaw + _slot_yaw(ix, iz, "corner"),
            scale=_slot_scale(ix, iz, "corner"), zone=z(ix, iz), role="corridor",
        ))

    def add_wall(ix: int, iz: int, side: str) -> None:
        dx, dz = DELTA[side]
        # inset pushes the wall out along the side normal to correct an off-centre
        # depth anchor (faction kits with non-zero roles.wall.inset).
        edge = CELL * 0.5 + _slot_inset(ix, iz, "wall")
        wall_stem = _slot_stem(ix, iz, "wall", "template-wall")
        wall_scale = _slot_scale(ix, iz, "wall")
        wall_yaw = WALL_YAW[side] + _slot_yaw(ix, iz, "wall")
        w = _piece(
            None, stem=wall_stem,
            x=world_x(fm.gx, ix) + dx * edge,
            z=world_z(fm.gz, iz) + dz * edge,
            yaw=wall_yaw,
            scale=wall_scale, zone=z(ix, iz), role="wall",
        )
        # Only for necropolis brick walls (user's edited taller stem or original short ones).
        # Align bottom to floor and force effective height to room height (4m) using scale_y.
        # Other factions use their models as-designed (full height at their scale).
        kit = w.get("kit") or ""
        stem = w.get("stem", "")
        if w.get("role") == "wall" and "necropolis" in kit.lower() and "brick" in stem.lower():
            if stem == "brick-wall2":
                # user's Blender-edited version with height doubled (approx 1.45 model units)
                model_h = 1.45
                miny = 0.0
            else:
                miny, maxy = _get_model_y_bounds(kit, stem)
                model_h = maxy - miny
            if model_h > 0:
                target_h = 4.0  # standard room height for one level
                # scale_y makes the Y dimension give exactly target_h (overrides xz scale for height only)
                scale_y = target_h / model_h
                w["scale_y"] = scale_y
                # shift y so that the model's min Y lands at floor level 0 after scaling
                w["y"] = - miny * scale_y
        pieces.append(w)

    for (ix, iz) in sorted(fm.walkable):
        if (ix, iz) in holes0:
            add("template-floor-hole", ix, iz, 0.0)  # floor-0 extraction trap
            continue
        faces = [s for s, (dx, dz) in DELTA.items() if (ix + dx, iz + dz) in fm.walkable]
        if (ix, iz) in fm.corridor_cells:
            faces_set = set(faces)
            fs = frozenset(faces)
            is_elbow = (
                len(faces) == 2
                and fs not in (frozenset({"N", "S"}), frozenset({"E", "W"}))
            )
            corner_kit = _ACTIVE_ZONE_KITS.get(z(ix, iz) or "", {}).get("corridor")
            walls_only = corner_kit in _ACTIVE_WALLS_ONLY_CORNERS
            if is_elbow and not walls_only:
                # If this cell's faction uses a FLOORLESS corner GLB (its floor mesh
                # was removed), lay a matching floor tile under it — that tile takes
                # the per-zone kit like every other floor, so the corner floor
                # matches the surrounding floors. Factions whose corner still
                # bundles a floor get no extra tile (would double-floor / z-fight).
                if corner_kit in _ACTIVE_FLOORLESS:
                    add_floor(ix, iz)
                add_corner(ix, iz, CORNER_YAW.get(fs, 0.0))
            else:
                add_floor(ix, iz)
                for side in ('N', 'S', 'E', 'W'):
                    if side not in faces_set:
                        add_wall(ix, iz, side)
        else:
            # Room cell: floor + perimeter walls where it borders non-walkable.
            # Corridor neighbours stay open (the doorway); corridor GLBs supply
            # their own side walls so we never wall a corridor cell here.
            add_floor(ix, iz)
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


def _apply_synth_exterior_floors(
    pieces: List[dict],
    gx: int,
    gz: int,
    walkable: Set[Cell],
    zone_lookup: ZoneLookup,
    comp: "LevelComposition",
    interior_cells: Set[Cell],
    deck_cells: Set[Cell],
) -> None:
    """Ground-level synth ``floor`` on exterior corridors (no 1.2 m zone uplift)."""
    import synth_transition as st

    if comp.mix_mode != "transition":
        return
    synth_zones = st.synth_zone_ids(comp)
    if not synth_zones:
        return
    skip = interior_cells | deck_cells
    for p in pieces:
        if p.get("role") != "floor" or p.get("stem") != "template-floor":
            continue
        if p.get("ceiling"):
            continue
        ix = int(round((p["x"] / CELL) + gx / 2 - 0.5))
        iz = int(round((p["z"] / CELL) + gz / 2 - 0.5))
        cell = (ix, iz)
        if cell not in walkable or cell in skip:
            continue
        zone = zone_lookup(cell)
        if zone not in synth_zones:
            continue
        p["stem"] = "floor"
        p["kit"] = "factions/synth"
        p["scale"] = 4.0
        # Pin to EXACTLY y=0 (top 1.2 m) to match deck pieces. Without this the piece
        # has no `y` and the client falls back to default_placement_y = -0.005, so synth
        # floor blocks sat 5 mm below the deck tiles → a visible seam "line" at every
        # deck/floor boundary ("2 tiles a pixel too high"). All synth surfaces flush now.
        p["y"] = 0.0
        p["tags"] = list(p.get("tags") or []) + ["synth_exterior_floor"]


def _ensure_synth_accessibility(
    pieces: List[dict],
    fm: "FreeformMap",
    spine: List[Cell],
    comp: "LevelComposition",
    zone_lookup: ZoneLookup,
    gx: int,
    gz: int,
    door_spec,
) -> int:
    """Guarantee every synth cell is reachable from spawn.

    The envelope pass walls synth↔default seams, which can seal off a whole synth
    region that the base graph reached through those open faces (D3/D4 — inaccessible
    areas / transition without an opening).  This pass BFS-walks from spawn honouring
    walls/doors/stairs + the 1.2 m elevation, and for each still-unreachable synth cell
    adjacent to a reached cell it opens a real entrance there (remove the seam wall;
    add a door, plus stairs if it crosses the level).  Repeats until everything reachable.
    """
    import level_composition as lc
    import synth_transition as st
    import transition_entrances as te

    DELTA = {"N": (0, -1), "S": (0, 1), "E": (1, 0), "W": (-1, 0)}
    elev = lc.make_elevation_lookup(fm.walkable, spine, comp)

    def facekey(c, s):
        dx, dz = DELTA[s]
        return (round(world_x(gx, c[0]) + dx * CELL * 0.5, 1),
                round(world_z(gz, c[1]) + dz * CELL * 0.5, 1))

    def rebuild_lookups():
        wall = {}
        door = set()
        stair = set()
        for p in pieces:
            if p.get("ceiling") or int(p.get("floor_level", 0)) != 0:
                continue
            r = p.get("role")
            if r == "wall":
                wall.setdefault((round(p["x"], 1), round(p["z"], 1)), []).append(p)
            elif r == "door":
                door.add((round(p["x"], 1), round(p["z"], 1)))
            elif r == "stairs":
                stair.add((int(round(p["x"] / CELL + gx / 2 - 0.5)),
                           int(round(p["z"] / CELL + gz / 2 - 0.5))))
        return wall, door, stair

    def reach(wall, door, stair):
        spawn = fm.rooms[fm.spawn_room]
        start = (spawn.cx, spawn.cz)
        seen = {start}
        stack = [start]
        while stack:
            c = stack.pop()
            for s, (dx, dz) in DELTA.items():
                nb = (c[0] + dx, c[1] + dz)
                if nb not in fm.walkable or nb in seen:
                    continue
                k = facekey(c, s)
                if k in wall and k not in door:
                    continue
                if abs(elev(c) - elev(nb)) > 0.5 and not (c in stair or nb in stair):
                    continue
                seen.add(nb)
                stack.append(nb)
        return seen

    synth = st.synth_zone_ids(comp)
    opened = 0
    for _ in range(400):  # bounded; each iteration opens >=1 entrance
        wall, door, stair = rebuild_lookups()
        seen = reach(wall, door, stair)
        frontier = None
        for c in sorted(fm.walkable):
            if c in seen:
                continue
            for s, (dx, dz) in DELTA.items():
                nb = (c[0] + dx, c[1] + dz)
                if nb in seen and nb in fm.walkable:
                    frontier = (c, nb, s)
                    break
            if frontier:
                break
        if frontier is None:
            break
        c, nb, s = frontier  # c = unreached cell, nb = reached neighbour
        k = facekey(c, s)
        if k in wall:  # remove the sealing wall(s)
            for w in wall[k]:
                if w in pieces:
                    pieces.remove(w)
        za, zb = zone_lookup(c), zone_lookup(nb)
        need_stair = abs(elev(c) - elev(nb)) > 0.5 and nb not in stair and c not in stair
        if za in synth and zb in synth:
            # same elevated building, internal opening — a door is enough.
            dx2, dy2, dyaw2 = te._cell_face_pose(gx, gz, c, s)
            pieces.append({
                "stem": door_spec.stem, "x": dx2, "z": dy2, "yaw": dyaw2,
                "y": st.FLIGHT_RISE, "floor_level": 0, "scale": door_spec.scale,
                "kit": door_spec.kit, "zone": za, "role": "door",
                "tags": ["synth_transition", "access_repair", "elevated_door"],
            })
        elif za in synth or zb in synth:
            # one side is the elevated building: door on the synth face, stairs on the
            # default cell (works both ways — climb up or descend into a default pocket).
            synth_cell, default_cell = (c, nb) if za in synth else (nb, c)
            pieces.extend(st.crossing_pieces(
                gx, gz, synth_cell, default_cell, zone_lookup(synth_cell),
                door_spec, need_stair=need_stair,
            ))
        # both default at same level: removing the wall is enough (no door needed).
        opened += 1
    return opened


def _attach_walls_to_doors(
    pieces: List[dict],
    fm: "FreeformMap",
    spine: List[Cell],
    comp: "LevelComposition",
    gx: int,
    gz: int,
    zone_lookup=None,
) -> int:
    """Give every floor-0 door a wall on each open flank so it reads as a real doorway,
    not a freestanding frame in open floor ("doorway with no connected walls").

    A door's flanks are the two face positions along its own wall line. If a flank is
    open (no wall, between two walkable cells) we add a jamb wall there — UNLESS doing
    so would cut the only connection between those two cells (reachability-safe). Walls
    inherit the door's yaw/Y so they sit flush.

    Jambs use the DOOR'S ZONE faction wall (a priesthood gate gets priesthood jambs);
    the synth kit wall is only the fallback for synth zones / unzoned doors."""
    import level_composition as lc
    import synth_transition as st

    synth_zones = st.synth_zone_ids(comp)

    DELTA = {"N": (0, -1), "S": (0, 1), "E": (1, 0), "W": (-1, 0)}
    elev = lc.make_elevation_lookup(fm.walkable, spine, comp)

    def fkey(c, s):
        dx, dz = DELTA[s]
        return (round(world_x(gx, c[0]) + dx * CELL * 0.5, 1),
                round(world_z(gz, c[1]) + dz * CELL * 0.5, 1))

    wall_faces = {
        (round(p["x"], 1), round(p["z"], 1))
        for p in pieces
        if p.get("role") == "wall" and int(p.get("floor_level", 0)) == 0
    }
    door_faces = {
        (round(p["x"], 1), round(p["z"], 1))
        for p in pieces
        if p.get("role") == "door" and int(p.get("floor_level", 0)) == 0
    }

    def connected(a: Cell, b: Cell, blocked: tuple) -> bool:
        """BFS a→b honouring walls/doors, with one extra blocked face ``blocked``."""
        seen = {a}
        stack = [a]
        while stack:
            c = stack.pop()
            if c == b:
                return True
            for s in DELTA:
                dx, dz = DELTA[s]
                nb = (c[0] + dx, c[1] + dz)
                if nb not in fm.walkable or nb in seen:
                    continue
                k = fkey(c, s)
                if k == blocked:
                    continue
                if k in wall_faces and k not in door_faces:
                    continue
                seen.add(nb)
                stack.append(nb)
        return False

    def flank_cells(fx: float, fz: float):
        gxf = fx / CELL + gx / 2 - 0.5
        gzf = fz / CELL + gz / 2 - 0.5
        if abs(gxf - round(gxf)) > 0.25:
            return (int(round(gxf - 0.5)), int(round(gzf))), (int(round(gxf + 0.5)), int(round(gzf)))
        return (int(round(gxf)), int(round(gzf - 0.5))), (int(round(gxf)), int(round(gzf + 0.5)))

    added = 0
    new: List[dict] = []
    for p in list(pieces):
        if p.get("role") != "door" or int(p.get("floor_level", 0)) != 0:
            continue
        x, z = p["x"], p["z"]
        gxf = x / CELL + gx / 2 - 0.5
        vertical = abs(gxf - round(gxf)) > 0.25
        flanks = [(x, z - CELL), (x, z + CELL)] if vertical else [(x - CELL, z), (x + CELL, z)]
        for fx, fz in flanks:
            key = (round(fx, 1), round(fz, 1))
            if key in wall_faces or key in door_faces:
                continue
            a, b = flank_cells(fx, fz)
            if a not in fm.walkable or b not in fm.walkable:
                continue  # against void — already a fine jamb
            if not connected(a, b, key):
                continue  # this flank is the only path between a,b — leave it open
            y = max(elev(a), elev(b))
            zone_id = p.get("zone")
            ground_zone = (
                zone_lookup is not None
                and zone_id in ("prev", "default", "next")
                and zone_id not in synth_zones
            )
            if ground_zone:
                # Ground-faction door: jamb with that faction's own wall.
                anchor = a if zone_lookup(a) == zone_id else b
                other = b if anchor == a else a
                dxc, dzc = other[0] - anchor[0], other[1] - anchor[1]
                side = {(0, -1): "N", (0, 1): "S", (1, 0): "E", (-1, 0): "W"}[(dxc, dzc)]
                jambs = _zone_wall_pieces_two_sided(
                    gx, gz, anchor, side, zone_id, ["zone_seam_wall", "door_jamb"],
                )
            else:
                jambs = [{
                    "stem": "wall", "x": fx, "z": fz, "yaw": p.get("yaw", 0.0),
                    "floor_level": 0, "scale": 4.0, "kit": "factions/synth",
                    "zone": zone_id, "role": "wall",
                    "tags": ["synth_transition", "door_jamb"],
                }]
            for w in jambs:
                if y > 0:
                    w["y"] = y
                new.append(w)
            wall_faces.add(key)
            added += 1
    pieces.extend(new)
    return added


def _apply_necropolis_wall_height(w: dict) -> None:
    """Necropolis brick walls: stretch Y so the wall spans floor-to-roof (4 m)."""
    kit = w.get("kit") or ""
    stem = w.get("stem", "")
    if w.get("role") != "wall" or "necropolis" not in kit.lower() or "brick" not in stem.lower():
        return
    if stem == "brick-wall2":
        # user's Blender-edited version with height doubled (approx 1.45 model units)
        model_h = 1.45
        miny = 0.0
    else:
        miny, maxy = _get_model_y_bounds(kit, stem)
        model_h = maxy - miny
    if model_h > 0:
        target_h = 4.0
        scale_y = target_h / model_h
        w["scale_y"] = scale_y
        w["y"] = -miny * scale_y


def _zone_wall_pieces_two_sided(
    gx: int, gz: int, cell: Cell, side: str, zone_id: Optional[str], tags: List[str],
) -> List[dict]:
    """Zone-faction wall seen from BOTH sides; urban wall-a-flat is a one-sided
    facade panel, so it gets a mirrored back-to-back copy."""
    w = _zone_wall_piece(gx, gz, cell[0], cell[1], side, zone_id)
    w["tags"] = list(tags)
    out = [w]
    if "urban" in (w.get("kit") or ""):
        back = dict(w)
        back["yaw"] = (w["yaw"] + math.pi) % (2 * math.pi)
        back["tags"] = list(tags) + ["seam_backside"]
        out.append(back)
    return out


def _zone_wall_piece(gx: int, gz: int, ix: int, iz: int, side: str, zone: Optional[str]) -> dict:
    """Wall at a cell face using the zone faction's wall stem/kit/scale/yaw/inset —
    the same construction as emit_pieces.add_wall, callable outside base emission."""
    dx, dz = DELTA[side]
    zkey = zone or ""
    edge = CELL * 0.5 + _ACTIVE_ZONE_INSET.get(zkey, {}).get("wall", 0.0)
    stem = _ACTIVE_ZONE_STEMS.get(zkey, {}).get("wall", "template-wall")
    scale = _ACTIVE_ZONE_SCALE.get(zkey, {}).get("wall", 1.0)
    yaw = WALL_YAW[side] + _ACTIVE_ZONE_YAW.get(zkey, {}).get("wall", 0.0)
    w = _piece(
        None, stem=stem,
        x=world_x(gx, ix) + dx * edge,
        z=world_z(gz, iz) + dz * edge,
        yaw=yaw, scale=scale, zone=zone, role="wall",
    )
    _apply_necropolis_wall_height(w)
    return w


# ─── industrial factory halls (double-height rooms with catwalk tiers) ───────

WALL_TIER_H = 4.25        # template-wall model height (probed, scale 1) — one wall tier
HALL_ROOF_LIFT = WALL_TIER_H  # hall roof sits exactly one extra wall tier higher
HALL_MIN_DIM = 4          # cells; anything smaller keeps the normal 1-tier shell
HALL_MAX_COUNT = 2


def _select_industrial_halls(fm: FreeformMap, zone_lookup, comp) -> Set[Cell]:
    """Cells of up to HALL_MAX_COUNT big fully-industrial rooms that get the
    double-height factory-hall treatment (tier-2 walls, raised roof, catwalk).

    Only rooms entirely inside the industrial zone qualify — the raised shell
    and the interior catwalk planner must agree on the same footprint, and the
    spawn/extraction/hidden rooms keep their special roof handling."""
    if comp.mix_mode == "transition":
        if "industrial" not in (comp.default_faction or ""):
            return set()
        def zone_ok(c: Cell) -> bool:
            return zone_lookup(c) == "default"
    else:
        if "industrial" not in (fm.faction_profile_id or ""):
            return set()
        def zone_ok(c: Cell) -> bool:
            return True
    hidden = set(fm.hidden_rooms)
    cands: List[Tuple[int, int, Set[Cell]]] = []
    for i, r in enumerate(fm.rooms):
        if i in (fm.spawn_room, fm.end_room) or i in hidden:
            continue
        if r.w < HALL_MIN_DIM or r.h < HALL_MIN_DIM:
            continue
        cells = set(r.cells())
        if fm.hub and fm.hub.trap0 in cells:
            continue
        if not all(zone_ok(c) for c in cells):
            continue
        cands.append((len(cells), i, cells))
    cands.sort(key=lambda t: (-t[0], t[1]))
    halls: Set[Cell] = set()
    for _, _, cells in cands[:HALL_MAX_COUNT]:
        halls |= cells
    return halls


def _emit_hall_upper_tier(
    pieces: List[dict], gx: int, gz: int, hall_cells: Set[Cell], zone_lookup,
) -> int:
    """Second FULL-height wall tier around double-height halls, stacked flush on
    top of the base tier (user rule: double walls never overlap — full height
    per tier). Open faces (corridor mouths) get the upper tier too: above the
    base wall line the neighbouring corridor roof is lower, so the face must be
    closed all the way up to the raised hall roof."""
    n = 0
    for (ix, iz) in sorted(hall_cells):
        for side, (dx, dz) in DELTA.items():
            if (ix + dx, iz + dz) in hall_cells:
                continue
            w = _zone_wall_piece(gx, gz, ix, iz, side, _cell_zone(zone_lookup, (ix, iz)))
            w["y"] = WALL_TIER_H
            w["tags"] = ["industrial_hall", "hall_upper_wall"]
            pieces.append(w)
            n += 1
    return n


def _raise_hall_roofs(pieces: List[dict], gx: int, gz: int, hall_cells: Set[Cell]) -> int:
    """Lift the floor-1 ceiling slabs over hall cells by one wall tier. The slab
    keeps floor_level 1 (validate() and the streaming shaft logic key on that);
    only its world y moves up."""
    base = 4.5 - 0.005  # default floor-1 slab y (MOD_H × 1 − ε, kenney_layout.rs)
    n = 0
    for p in pieces:
        if not p.get("ceiling") or int(p.get("floor_level", 0)) != 1:
            continue
        if _world_to_cell(gx, gz, float(p["x"]), float(p["z"])) in hall_cells:
            p["y"] = round(base + HALL_ROOF_LIFT, 4)
            n += 1
    return n


def _emit_zone_seam_walls(
    pieces: List[dict],
    fm: "FreeformMap",
    spine: List[Cell],
    comp: "LevelComposition",
    gx: int,
    gz: int,
    zone_lookup,
) -> int:
    """Close ground-faction zone seams with the faction's own walls.

    Non-elevated factions previously got ONE free-standing transition door at
    the prev/default (default/next) boundary and nothing else — 'a random door
    in the middle of a room'. Mirror the synth envelope rule: wall every seam
    face with the zone faction's wall stem, keep the transition door as the
    entrance, then BFS-repair reachability by converting a seam wall into an
    extra faction door wherever sealing isolated a region."""
    import faction_profiles as fp
    import level_composition as lc
    import transition_entrances as te

    if comp.mix_mode != "transition":
        return 0

    def facekey(c: Cell, side: str) -> Tuple[float, float]:
        dx, dz = DELTA[side]
        return (round(world_x(gx, c[0]) + dx * CELL * 0.5, 1),
                round(world_z(gz, c[1]) + dz * CELL * 0.5, 1))

    def snap_face(v: float, half: float) -> float:
        """Snap a coordinate to the nearest cell-face line (faces sit at
        (k - g/2) * CELL). Needed because some faction walls carry a placement
        inset (necropolis brick: 1.4) so the piece anchor is off the face."""
        return (round(v / CELL + half) - half) * CELL

    def wall_face_key(p: dict) -> Tuple[float, float]:
        x, z = float(p["x"]), float(p["z"])
        yaw = float(p.get("yaw", 0.0)) % math.pi
        if abs(yaw - math.pi / 2.0) < 0.3:  # wall spans z, normal along x
            x = snap_face(x, gx / 2.0)
        else:
            z = snap_face(z, gz / 2.0)
        return (round(x, 1), round(z, 1))

    # Ground zones only — synth seams are closed by emit_synth_envelope_walls.
    zones = []
    for zone_id, fac in (("prev", comp.prev_faction), ("next", comp.next_faction)):
        prof = fp.load_profile(fac)
        if fac != "synth" and lc.elevation_for_faction_profile(prof) <= 0:
            zones.append((zone_id, fac, prof))
    if not zones:
        return 0

    wall_keys = {wall_face_key(p) for p in pieces
                 if p.get("role") == "wall" and int(p.get("floor_level", 0)) == 0}
    door_keys = {(round(p["x"], 1), round(p["z"], 1)) for p in pieces
                 if p.get("role") == "door" and int(p.get("floor_level", 0)) == 0}

    def seam_wall_pieces(cell: Cell, side: str, zone_id: str, tags: List[str]) -> List[dict]:
        return _zone_wall_pieces_two_sided(gx, gz, cell, side, zone_id, tags)

    added = 0
    mine: Dict[Tuple[float, float], List[dict]] = {}
    zone_of_face: Dict[Tuple[float, float], Tuple[str, "fp.FactionProcgenProfile"]] = {}
    for zone_id, fac, prof in zones:
        for c in sorted(fm.walkable):
            if zone_lookup(c) != zone_id:
                continue
            for side, (dx, dz) in DELTA.items():
                nb = (c[0] + dx, c[1] + dz)
                if nb not in fm.walkable:
                    continue
                if zone_lookup(nb) != "default":
                    continue
                k = facekey(c, side)
                if k in wall_keys or k in door_keys:
                    continue
                # A corridor crossing the seam must never dead-end into a
                # blank wall: when the straight path continues on both sides
                # of the face through a corridor cell, gate it with a door.
                behind_nb = (nb[0] + dx, nb[1] + dz)
                behind_c = (c[0] - dx, c[1] - dz)
                corridor_crossing = (
                    (nb in fm.corridor_cells and behind_nb in fm.walkable)
                    or (c in fm.corridor_cells and behind_c in fm.walkable)
                )
                if corridor_crossing:
                    d_spec = te._resolve_door(prof)
                    dxw, dzw, dyaw = te._cell_face_pose(gx, gz, c, side)
                    pieces.append({
                        "stem": d_spec.stem, "x": dxw, "z": dzw, "yaw": dyaw,
                        "floor_level": 0, "scale": d_spec.scale,
                        "kit": d_spec.kit, "zone": zone_id, "role": "door",
                        "tags": ["transition_entrance", "zone_seam_wall",
                                 "corridor_seam_door"],
                    })
                    door_keys.add(k)
                    added += 1
                    continue
                ws = seam_wall_pieces(c, side, zone_id, ["zone_seam_wall"])
                pieces.extend(ws)
                wall_keys.add(k)
                mine[k] = ws
                zone_of_face[k] = (zone_id, prof)
                added += 1
    if not added:
        return added

    # --- Reachability repair -------------------------------------------------
    elev = lc.make_elevation_lookup(fm.walkable, spine, comp)

    def lookups():
        wall = set()
        door = set()
        stair = set()
        for p in pieces:
            if p.get("ceiling") or int(p.get("floor_level", 0)) != 0:
                continue
            r = p.get("role")
            if r == "wall":
                wall.add(wall_face_key(p))
            elif r == "door":
                door.add((round(p["x"], 1), round(p["z"], 1)))
            elif r == "stairs":
                stair.add((int(round(p["x"] / CELL + gx / 2 - 0.5)),
                           int(round(p["z"] / CELL + gz / 2 - 0.5))))
        return wall, door, stair

    spawn = fm.rooms[fm.spawn_room]
    start = (spawn.cx, spawn.cz)
    for _ in range(64):  # bounded; each iteration opens one seam door
        wall, door, stair = lookups()
        seen = {start}
        stack = [start]
        while stack:
            c = stack.pop()
            for side, (dx, dz) in DELTA.items():
                nb = (c[0] + dx, c[1] + dz)
                if nb not in fm.walkable or nb in seen:
                    continue
                k = facekey(c, side)
                if k in wall and k not in door:
                    continue
                if abs(elev(c) - elev(nb)) > 0.5 and not (c in stair or nb in stair):
                    continue
                seen.add(nb)
                stack.append(nb)
        # Find an unreached cell whose frontier face is one of OUR seam walls.
        frontier = None
        for c in sorted(fm.walkable):
            if c in seen:
                continue
            for side, (dx, dz) in DELTA.items():
                nb = (c[0] + dx, c[1] + dz)
                if nb not in seen or nb not in fm.walkable:
                    continue
                k = facekey(c, side)
                if k in mine:
                    frontier = (c, side, k)
                    break
            if frontier:
                break
        if frontier is None:
            break  # everything reachable, or blockage predates this pass
        c, side, k = frontier
        for w in mine.pop(k):
            if w in pieces:
                pieces.remove(w)
        zone_id, prof = zone_of_face[k]
        d_spec = te._resolve_door(prof)
        dx_w, dz_w, dyaw = te._cell_face_pose(gx, gz, c, side)
        pieces.append({
            "stem": d_spec.stem, "x": dx_w, "z": dz_w, "yaw": dyaw,
            "floor_level": 0, "scale": d_spec.scale, "kit": d_spec.kit,
            "zone": zone_id, "role": "door",
            "tags": ["transition_entrance", "zone_seam_wall", "access_repair"],
        })

    # --- Door jambs ----------------------------------------------------------
    # When the seam turns a corner AT the door cell, the flank position along
    # the door's own wall line is open floor and the door reads free-standing.
    # Add a faction wall on each open flank, reachability-safe.
    wall, door, stair = lookups()

    def connected(a: Cell, b: Cell, blocked: Tuple[float, float]) -> bool:
        seen = {a}
        stack = [a]
        while stack:
            c = stack.pop()
            if c == b:
                return True
            for side, (dx, dz) in DELTA.items():
                nb = (c[0] + dx, c[1] + dz)
                if nb not in fm.walkable or nb in seen:
                    continue
                k = facekey(c, side)
                if k == blocked:
                    continue
                if k in wall and k not in door:
                    continue
                if abs(elev(c) - elev(nb)) > 0.5 and not (c in stair or nb in stair):
                    continue
                seen.add(nb)
                stack.append(nb)
        return False

    zone_prof = {z: pr for z, _f, pr in zones}
    for p in list(pieces):
        if p.get("role") != "door" or int(p.get("floor_level", 0)) != 0:
            continue
        tags = p.get("tags") or []
        if "transition_entrance" not in tags or "elevated_door" in tags:
            continue
        zone_id = p.get("zone")
        if zone_id not in zone_prof:
            continue
        x, z = float(p["x"]), float(p["z"])
        gxf = x / CELL + gx / 2 - 0.5
        gzf = z / CELL + gz / 2 - 0.5
        vertical = abs(gxf - round(gxf)) > 0.25  # face between E/W neighbours
        if vertical:
            flanks = [((int(round(gxf - 0.5)), int(round(gzf)) + dzz), "E")
                      for dzz in (-1, 1)]
        else:
            flanks = [((int(round(gxf)) + dxx, int(round(gzf - 0.5))), "S")
                      for dxx in (-1, 1)]
        for cell_a, side in flanks:
            dxs, dzs = DELTA[side]
            cell_b = (cell_a[0] + dxs, cell_a[1] + dzs)
            k = facekey(cell_a, side)
            if k in wall or k in door:
                continue
            if cell_a not in fm.walkable or cell_b not in fm.walkable:
                continue  # void flank — base-gen wall already jambs it
            if connected(cell_a, cell_b, k):
                anchor = cell_a if zone_lookup(cell_a) == zone_id else \
                    (cell_b if zone_lookup(cell_b) == zone_id else cell_a)
                a_side = side if anchor == cell_a else OPP[side]
                y = max(elev(cell_a), elev(cell_b))
                for w in seam_wall_pieces(anchor, a_side, zone_id,
                                          ["zone_seam_wall", "door_jamb"]):
                    if y > 0:
                        w["y"] = y
                    pieces.append(w)
                wall.add(k)
            else:
                # Sole path — a wall would break reachability. Extend the gate
                # line with another faction door instead (passable, reads as a
                # wide gated threshold rather than a free-standing frame).
                d_spec = te._resolve_door(zone_prof[zone_id])
                dxw, dzw, dyaw = te._cell_face_pose(gx, gz, cell_a, side)
                pieces.append({
                    "stem": d_spec.stem, "x": dxw, "z": dzw, "yaw": dyaw,
                    "floor_level": 0, "scale": d_spec.scale, "kit": d_spec.kit,
                    "zone": zone_id, "role": "door",
                    "tags": ["transition_entrance", "zone_seam_wall", "door_jamb"],
                })
                door.add(k)
            added += 1
    return added


def _strip_walls_under_doors(pieces: List[dict], gx: int = 0, gz: int = 0) -> int:
    """Remove any floor-0 wall sharing a door's face (a door must be an OPENING, not
    lodged inside a wall — D2).  Doors win over walls.  Includes hidden doors, and
    matches walls by their face-snapped position too (inset walls, e.g. necropolis
    brick 1.4 m, sit off the face line)."""
    door_keys = {
        (round(p["x"], 1), round(p["z"], 1))
        for p in pieces
        if int(p.get("floor_level", 0)) == 0
        and (p.get("role") == "door" or "hidden_entrance" in (p.get("tags") or []))
    }
    if not door_keys:
        return 0

    def snapped(p: dict) -> Tuple[float, float]:
        x, z = float(p["x"]), float(p["z"])
        if not gx:
            return (round(x, 1), round(z, 1))
        yaw = float(p.get("yaw", 0.0)) % math.pi
        if abs(yaw - math.pi / 2.0) < 0.3:
            x = (round(x / CELL + gx / 2.0) - gx / 2.0) * CELL
        else:
            z = (round(z / CELL + gz / 2.0) - gz / 2.0) * CELL
        return (round(x, 1), round(z, 1))

    kept: List[dict] = []
    removed = 0
    for p in pieces:
        if (
            p.get("role") == "wall"
            and int(p.get("floor_level", 0)) == 0
            and ((round(p["x"], 1), round(p["z"], 1)) in door_keys
                 or snapped(p) in door_keys)
        ):
            removed += 1
            continue
        kept.append(p)
    if removed:
        pieces.clear()
        pieces.extend(kept)
    return removed


def _apply_zone_elevation(
    pieces: List[dict],
    gx: int,
    gz: int,
    fm: FreeformMap,
    spine: List[Cell],
    comp: "LevelComposition",
    *,
    interior_cells: Optional[Set[Cell]] = None,
) -> None:
    """Raise faction-deck pieces to ``elevation_rise`` (synth station height)."""
    import level_composition as lc

    if comp.mix_mode != "transition":
        return
    elev = lc.make_elevation_lookup(
        fm.walkable, spine, comp, interior_cells=interior_cells,
    )
    for p in pieces:
        if "synth_mezz" in (p.get("tags") or []):
            continue
        if "y" in p or p.get("ceiling"):
            continue
        # Deck uplift is a floor-0 concept only — never raise the underground hub
        # floors/walls (floor_level -1/-2) that happen to sit under a synth cell.
        if int(p.get("floor_level", 0)) != 0:
            continue
        if p.get("role") == "deck":
            continue
        if p.get("stem", "").startswith("floor") and p.get("kit") in ("space_station", "factions/synth"):
            continue
        # Walls sit on a cell FACE; a synth/default seam wall belongs to the synth
        # (elevated) side, so take the max elevation over both cells sharing the face.
        e = max((elev(c) for c in _face_cells(gx, gz, p["x"], p["z"])), default=0.0)
        if e > 0:
            p["y"] = e


def build_nav_grid(
    pieces: List[dict],
    fm: "FreeformMap",
    spine: List[Cell],
    comp: "LevelComposition",
    gx: int,
    gz: int,
) -> dict:
    """Bake the walkable-cell nav graph for server-side enemy pathfinding.

    Mirrors the generator's own reachability rules (walls block, doors pass,
    elevation steps need a stair cell) — the exact rules that guarantee PLAYER
    reachability, so an A* over this graph never routes through a wall or up a
    sheer 1.2 m ledge. ``open`` lists the passable faces (N/S/E/W) per cell."""
    import level_composition as lc

    DELTA = {"N": (0, -1), "S": (0, 1), "E": (1, 0), "W": (-1, 0)}
    elev = lc.make_elevation_lookup(fm.walkable, spine, comp)

    def snap_face(v: float, half: float) -> float:
        return (round(v / CELL + half) - half) * CELL

    wall_keys: Set[Tuple[float, float]] = set()
    door_keys: Set[Tuple[float, float]] = set()
    stair_cells: Set[Cell] = set()
    for p in pieces:
        if p.get("ceiling") or int(p.get("floor_level", 0)) != 0:
            continue
        r = p.get("role")
        tags = p.get("tags") or []
        if r == "door" or "hidden_entrance" in tags:
            door_keys.add((round(p["x"], 1), round(p["z"], 1)))
        elif r == "wall":
            # Hall tier-2 walls float a full wall height up — ground nav ignores
            # them (they'd otherwise wall off the hall's own corridor mouths).
            if float(p.get("y") or 0.0) >= 2.0:
                continue
            # Snap inset walls (necropolis brick 1.4) back to their face line.
            x, z = float(p["x"]), float(p["z"])
            yaw = float(p.get("yaw", 0.0)) % math.pi
            if abs(yaw - math.pi / 2.0) < 0.3:
                x = snap_face(x, gx / 2.0)
            else:
                z = snap_face(z, gz / 2.0)
            wall_keys.add((round(x, 1), round(z, 1)))
        elif r == "stairs":
            stair_cells.add((int(round(p["x"] / CELL + gx / 2 - 0.5)),
                             int(round(p["z"] / CELL + gz / 2 - 0.5))))

    def facekey(c: Cell, s: str) -> Tuple[float, float]:
        dx, dz = DELTA[s]
        return (round(world_x(gx, c[0]) + dx * CELL * 0.5, 1),
                round(world_z(gz, c[1]) + dz * CELL * 0.5, 1))

    cells = []
    for c in sorted(fm.walkable):
        open_sides = ""
        for s, (dx, dz) in DELTA.items():
            nb = (c[0] + dx, c[1] + dz)
            if nb not in fm.walkable:
                continue
            k = facekey(c, s)
            if k in wall_keys and k not in door_keys:
                continue
            if abs(elev(c) - elev(nb)) > 0.5 and not (c in stair_cells or nb in stair_cells):
                continue
            open_sides += s
        cells.append({
            "c": [c[0], c[1]],
            "y": round(float(elev(c)), 3),
            "open": open_sides,
            "stair": c in stair_cells,
        })
    return {"cell_m": CELL, "cells_x": gx, "cells_z": gz, "cells": cells}


_WALL_OFF_CACHE: Dict[str, float] = {}


def _faction_wall_inner_offset(faction_id: str) -> float:
    """``wall_inner_offset_m`` from the faction's placement catalog: how far
    the wall's inner surface protrudes INTO the room past the cell-face line
    (0.0 = wall plane sits on the face; necropolis/priesthood are 0.6)."""
    if faction_id in _WALL_OFF_CACHE:
        return _WALL_OFF_CACHE[faction_id]
    off = 0.0
    try:
        import faction_assets as fa
        asset = fa.asset_for_profile(faction_id)
        if asset is not None and "wall" in asset.provides:
            cat = ROOT / "assets" / "models" / asset.kit / "placement_catalog.json"
            if cat.is_file():
                off = float(json.loads(cat.read_text(encoding="utf-8"))
                            .get("wall_inner_offset_m") or 0.0)
    except Exception:
        off = 0.0
    _WALL_OFF_CACHE[faction_id] = off
    return off


# Mini-market GLB extents along local z (model units, probed 2026-07-07).
# Local +z faces the room centre (yaw_in), so the wall-side extent is the
# NEGATIVE z bound. Used to place each piece's back face flush against the
# faction wall's inner surface instead of a hardcoded offset (which buried
# the shelf inside 0.6 m-thick walls and left displays clipping them).
_MM_BACK = {"shelf-end": 0.40, "display-fruit": 0.30, "display-bread": 0.30,
            "cash-register": 0.40, "detail-awning-wide": 0.15,
            "detail-awning-small": 0.15}


def _dress_hub_and_hidden(pieces: List[dict], fm: "FreeformMap", gx: int, gz: int, rng,
                          hub_wall_off: float = 0.0,
                          wall_off_for_cell=None) -> None:
    """Market dressing: a stall cluster in the hub (the between-level shop room,
    cf. the r.e.p.o. re-stock shop) and a small shack in every hidden room
    (whose keeper NPC spawns at the room centre). Stalls use the Kenney
    mini-market kit; the retro-urban awning is the canopy, wall-mounted at
    2.3 m. Mini-market is a 1-unit kit but its props are chunky — x2.5 puts
    shelves/displays at human scale, the cash-register counter x2.2 (counter
    top ~1.3 m). All bases sit ON the floor (y = floor*4 + 0.01 — the old
    +0.05 read as hovering) and all backs sit flush against the wall's inner
    surface (W below), which varies per faction (wall_inner_offset_m)."""
    KIT = "retro_urban"
    MARKET = "mini_market"
    S = 4.0
    MS = 2.5
    REG = 2.2
    GAP = 0.04           # clearance so backs never z-fight the wall face
    HALF = CELL / 2.0    # cell centre → cell-face line
    DELTA = ((0, -1), (0, 1), (1, 0), (-1, 0))

    def prop(stem: str, x: float, z: float, yaw: float, floor: int, tag: str,
             kit: str = KIT, scale: float = S, y_lift: float = 0.0) -> dict:
        # ``y`` is an ABSOLUTE world height (PieceRecord.world_y override), so
        # sub-ground floors must bake in the 4 m module height: hub floor -1
        # surface sits at -4.0 (same convention as the keeper NPC spawns).
        return {"stem": stem, "x": round(x, 4), "z": round(z, 4), "yaw": round(yaw, 4),
                "floor_level": floor, "scale": scale, "kit": kit,
                "y": round(floor * 4.0 + 0.01 + y_lift, 3),
                "role": "prop", "tags": [tag]}

    hub = fm.hub
    if hub:
        wall_face = HALF - float(hub_wall_off)   # inner wall surface distance
        avoid: Set[Cell] = set(hub.holes1) | {hub.trap0} | {e.trap for e in hub.exits}
        floor1 = set(hub.floor1)
        cands = []
        for c in sorted(floor1):
            if any((c[0] + ax, c[1] + az) in avoid for ax in (-1, 0, 1) for az in (-1, 0, 1)):
                continue
            for dx, dz in DELTA:
                if (c[0] + dx, c[1] + dz) not in floor1:
                    # displays flank 1.8 m to each side: both perpendicular
                    # neighbours must be open floor so they never sit in a wall
                    if (c[0] + dz, c[1] - dx) in floor1 and (c[0] - dz, c[1] + dx) in floor1:
                        cands.append((c, dx, dz))
                    break
        # THREE trade stalls (plan4 rule: 3 station-tied NPCs per hub), spread
        # at least 3 cells apart; each stall's keeper NPC spawns behind the
        # counter (between counter and shelf).
        rng.shuffle(cands)
        placed: List[Tuple[Cell, int, int]] = []
        for c, dx, dz in cands:
            if len(placed) >= 3:
                break
            if any(abs(c[0] - p[0][0]) + abs(c[1] - p[0][1]) < 3 for p in placed):
                continue
            placed.append((c, dx, dz))
        keeper_spawns: List[Tuple[float, float, float]] = []
        # Outward distances (from cell centre toward the wall), back-flush:
        shelf_out = wall_face - _MM_BACK["shelf-end"] * MS - GAP
        disp_out = wall_face - _MM_BACK["display-fruit"] * MS - GAP
        awn_out = wall_face - _MM_BACK["detail-awning-wide"] * S - GAP
        reg_out = -1.35                                   # counter toward room
        keeper_out = (shelf_out + (reg_out + 0.40 * REG)) / 2.0
        for c, dx, dz in placed:
            wx, wz = world_x(gx, c[0]), world_z(gz, c[1])
            yaw_in = math.atan2(-dx, -dz)   # local +z toward room centre
            # Shop layout: counter between keeper and room, shelf behind the
            # keeper against the wall, displays flanking (perp = (dz,-dx)),
            # awning canopy wall-mounted overhead.
            pieces.append(prop("detail-awning-wide", wx + dx * awn_out, wz + dz * awn_out,
                               yaw_in, -1, "hub_market", y_lift=2.3))
            pieces.append(prop("cash-register", wx + dx * reg_out, wz + dz * reg_out,
                               yaw_in, -1, "hub_market", MARKET, REG))
            pieces.append(prop("shelf-end", wx + dx * shelf_out, wz + dz * shelf_out,
                               yaw_in, -1, "hub_market", MARKET, MS))
            pieces.append(prop("display-fruit", wx + dz * 1.8 + dx * disp_out,
                               wz - dx * 1.8 + dz * disp_out,
                               yaw_in, -1, "hub_market", MARKET, MS))
            pieces.append(prop("display-bread", wx - dz * 1.8 + dx * disp_out,
                               wz + dx * 1.8 + dz * disp_out,
                               yaw_in, -1, "hub_market", MARKET, MS))
            keeper_spawns.append((wx + dx * keeper_out, -4.0 + 0.15,
                                  wz + dz * keeper_out))  # hub floor -1 surface
        if keeper_spawns:
            fm.npc_spawns = list(getattr(fm, "npc_spawns", [])) + keeper_spawns  # type: ignore[attr-defined]

    for i in getattr(fm, "hidden_rooms", []):
        r = fm.rooms[i]
        cx, cz = r.cx, r.cz
        side = next(((dx, dz) for dx, dz in DELTA
                     if (cx + dx, cz + dz) not in fm.walkable), (0, -1))
        dx, dz = side
        wx, wz = world_x(gx, cx), world_z(gz, cz)
        yaw_in = math.atan2(-dx, -dz)
        off = (wall_off_for_cell((cx, cz)) if wall_off_for_cell else 0.0)
        w_face = HALF - float(off)
        awn_out = w_face - _MM_BACK["detail-awning-small"] * S - GAP
        disp_out = w_face - _MM_BACK["display-fruit"] * MS - GAP
        pieces.append(prop("detail-awning-small", wx + dx * awn_out, wz + dz * awn_out,
                           yaw_in, 0, "hidden_shack", y_lift=2.3))
        pieces.append(prop("display-fruit", wx + dx * disp_out + dz * 1.2,
                           wz + dz * disp_out - dx * 1.2, yaw_in, 0, "hidden_shack",
                           MARKET, MS))
        pieces.append(prop("shopping-cart", wx - dz * 1.4, wz + dx * 1.4,
                           yaw_in, 0, "hidden_shack", MARKET, MS))


def to_doc(fm: FreeformMap, name: str) -> dict:
    import faction_profiles as fp
    import level_composition as lc

    comp = (fm.composition or lc.LevelComposition(mix_mode="single")).normalized()
    # Resolve which faction (hence kit + floorless-corner) each zone uses, from
    # the faction manifests, BEFORE emitting any pieces.
    build_zone_kits(comp)
    spine, _, kit_lookup, zone_lookup = lc.plan_zones_for_map(fm)
    hub_kit = fp.architecture_kit(fp.load_profile(comp.default_faction))
    # Double-height industrial factory halls: chosen ONCE here so the shell
    # (upper wall tier + raised roof) and the interior catwalk planner agree.
    hall_cells = _select_industrial_halls(fm, zone_lookup, comp)

    hub = fm.hub
    gx, gz = fm.gx, fm.gz
    holes0 = {hub.trap0} if hub else set()

    mask0 = [False] * (gx * gz)
    for (ix, iz) in fm.walkable:
        mask0[iz * gx + ix] = (ix, iz) != (hub.trap0 if hub else None)

    import transition_entrances as te

    pieces = emit_pieces(fm, holes0, kit_lookup=kit_lookup, zone_lookup=zone_lookup)

    transition_rng = random.Random((fm.seed * 1597334677) & 0xFFFFFFFF)
    transition_pieces, transition_plans = te.emit_transition_pieces(
        gx, gz, spine, comp, fm.walkable, zone_lookup, transition_rng,
        existing_pieces=pieces,
    )
    pieces.extend(transition_pieces)

    import synth_transition as st

    interior_cells: Optional[Set[Cell]] = None
    deck_cells: Set[Cell] = set()
    for plan in transition_plans:
        deck_cells |= plan.deck_cells
    synth_zones = st.synth_zone_ids(comp)
    if transition_plans:
        # The synth zone is one elevated building: EVERY synth cell sits on the 1.2 m
        # deck (so every synth wall is at 1.2 m).  Earlier this was a 1-D BFS line, which
        # left branch cells — and their walls — at y=0.  Whole footprint now elevates.
        # Only zones whose faction IS synth — never a ground faction's prev/next zone.
        interior_cells = {c for c in fm.walkable if zone_lookup(c) in synth_zones}
        _apply_synth_exterior_floors(
            pieces, gx, gz, fm.walkable, zone_lookup, comp, set(), deck_cells,
        )
    # Close the synth footprint: wall every synth cell that borders a walkable
    # non-synth cell (base-gen only walls void faces, so synth/default seams leak).
    # Corridor crossings become door+stairs instead of a blocking wall.
    if transition_plans:
        synth_door = te._resolve_door(fp.load_profile("synth"))
        pieces.extend(st.emit_synth_envelope_walls(
            gx, gz, fm.walkable, zone_lookup, deck_cells, pieces,
            corridor_cells=fm.corridor_cells, door_spec=synth_door,
            synth_zones=synth_zones,
        ))
        # Guarantee every synth region is reachable from spawn (open a stair+door
        # entrance into any area the envelope walls sealed off).
        _ensure_synth_accessibility(
            pieces, fm, spine, comp, zone_lookup, gx, gz, synth_door,
        )
        # Give every door a wall jamb on open flanks (no freestanding doorframes).
        # Reachability-safe: never jambs the sole path between two cells.
        _attach_walls_to_doors(pieces, fm, spine, comp, gx, gz, zone_lookup)
    floors = {"0": {"cells_x": gx, "cells_z": gz, "cells": mask0}}
    hub_exits: Dict[str, dict] = {}

    if hub:
        # Exit trap cells are OPEN shafts: the drop continues from the exit
        # room straight into the next level's map, mounted 4 m below by the
        # runtime streamer (the server seals them until the child is ready).
        exit_traps = {ex.trap for ex in hub.exits}
        emit_floor_tiles(pieces, gx, gz, -1, hub.floor1, hub.holes1, kit=hub_kit)
        emit_floor_tiles(pieces, gx, gz, -2, hub.floor2, exit_traps, kit=hub_kit)
        floors["-1"] = _mask(gx, gz, hub.floor1, hub.holes1)
        floors["-2"] = _mask(gx, gz, hub.floor2, exit_traps)
        for i, ex in enumerate(hub.exits):
            hub_exits[str(i)] = {
                "x": world_x(gx, ex.trap[0]), "z": world_z(gz, ex.trap[1]),
                "floor": -2, "kind": ex.kind, "label": f"Next level {i + 1}",
            }

    def _cell_wall_off(cell: Cell) -> float:
        zone = lc.zone_for_cell(cell, spine, comp)
        fac = {"prev": comp.prev_faction, "next": comp.next_faction}.get(
            zone, comp.default_faction)
        return _faction_wall_inner_offset(fac)

    _dress_hub_and_hidden(
        pieces, fm, gx, gz, transition_rng,
        hub_wall_off=_faction_wall_inner_offset(comp.default_faction),
        wall_off_for_cell=_cell_wall_off,
    )

    for (wx, wz, yaw), meta in zip(fm.secret_doors, fm.secret_door_meta):
        door_cell = meta.corridor_cell
        zone = lc.zone_for_cell(door_cell, spine, comp)
        hd_prof = lc.hidden_door_profile(comp, zone)
        hd = hd_prof.hidden_door
        pieces.append({
            "stem": hd.stem,
            "x": wx, "z": wz, "yaw": yaw,
            "floor_level": 0, "scale": 1.0,
            "zone": zone,
            **hd.to_piece_extras(),
        })

    # Close ground-faction zone seams: faction walls flanking the transition
    # door (else the door free-stands in open floor). Synth zones handled above.
    _emit_zone_seam_walls(pieces, fm, spine, comp, gx, gz, zone_lookup)

    _strip_walls_under_doors(pieces, gx, gz)

    # Interior dressing: windows on the synth perimeter, banners inside (drop-in stem
    # swaps — still solid walls, integrity/reachability unchanged).
    if transition_plans:
        st.decorate_synth_walls(pieces, gx, gz, fm.walkable, zone_lookup, fm.seed,
                                synth_zones=synth_zones)
        st.furnish_synth_interior(
            pieces, gx, gz, fm.walkable, zone_lookup, deck_cells, fm.seed, fm.corridor_cells,
            synth_zones=synth_zones,
        )

    # Unified rich interior for ALL factions (synth + non-synth).
    # Uses the new faction_interior framework for data-driven room roles + clusters.
    # Synth still gets its special elevation/balcony/mezz logic via delegation if needed.
    import faction_interior as fi
    prev_f = comp.prev_faction
    next_f = comp.next_faction

    def _world_at(c):
        return world_x(gx, c[0]), world_z(gz, c[1])

    # Door positions (transition + hidden doors) so furnish keeps their bands clear.
    door_xz = [(float(p["x"]), float(p["z"])) for p in pieces
               if p.get("role") == "door"
               or "hidden_entrance" in (p.get("tags") or [])]
    # Spawn and extraction trap get the same keep-clear treatment.
    _spawn_room = fm.rooms[fm.spawn_room]
    door_xz.append((world_x(gx, _spawn_room.cx), world_z(gz, _spawn_room.cz)))
    if hub:
        door_xz.append((world_x(gx, hub.trap0[0]), world_z(gz, hub.trap0[1])))

    def _furnish_zone(zone_id: str, faction_id: str) -> None:
        """Furnish one composition zone with its faction's interior logic."""
        if faction_id == "synth":
            return  # handled by the dedicated synth path above
        zone_walkable = {c for c in fm.walkable if zone_lookup(c) == zone_id}
        zone_walkable -= deck_cells or set()
        if not zone_walkable:
            return
        try:
            fi.furnish_faction_interior(
                pieces, gx, gz, zone_walkable, zone_lookup, deck_cells or set(), fm.seed,
                corridor_cells=fm.corridor_cells or set(),
                faction=faction_id,
                sweep_params=fi.load_sweep_params(faction_id),
                world_at=_world_at,
                global_walkable=fm.walkable,
                door_positions=door_xz,
                tall_cells=hall_cells,
            )
        except Exception:
            pass  # grace for any missing stems

    _furnish_zone("prev", prev_f)
    _furnish_zone("next", next_f)  # zones are disjoint even when prev == next faction
    # The middle (default) zone is a faction too — usually industrial. It was
    # previously never furnished, so industrial only got props when it happened
    # to be first/last. Furnish it like any other zone.
    if comp.mix_mode == "transition":
        _furnish_zone("default", comp.default_faction)

    # Post-adjust: nudge any interior room props that landed too close to spawn (prevents spawn clip/fall)
    spawn_c = (fm.rooms[fm.spawn_room].cx, fm.rooms[fm.spawn_room].cz)
    spawn_wx = world_x(gx, spawn_c[0])
    spawn_wz = world_z(gz, spawn_c[1])
    for pp in pieces:
        if 'room_' not in str(pp.get('tags', [])):
            continue
        if abs(pp.get('x', 0) - spawn_wx) < 1.5 and abs(pp.get('z', 0) - spawn_wz) < 1.5:
            pp['x'] = round(pp['x'] + 2.5, 4)
            pp['z'] = round(pp['z'] + transition_rng.uniform(-1,1), 4)

    emit_all_roofs(pieces, fm, hub, gx, gz, holes0, kit_lookup=kit_lookup, zone_lookup=zone_lookup)

    if hall_cells:
        _emit_hall_upper_tier(pieces, gx, gz, hall_cells, zone_lookup)
        _raise_hall_roofs(pieces, gx, gz, hall_cells)

    _apply_zone_elevation(
        pieces, gx, gz, fm, spine, comp, interior_cells=interior_cells,
    )

    # Explicitly pin y=0 for ground-level faction floors and walls (outlaw/urban etc.)
    # to prevent any synth-style 1.2m lift or transition bugs.
    for p in pieces:
        if p.get("zone") in ("prev", "next"):
            kit = p.get("kit", "")
            is_ground_faction = any(k in kit for k in ("urban", "outlaw", "industrial", "priesthood"))
            if is_ground_faction and p.get("role") in ("floor", "wall"):
                p["y"] = 0.0

    spawn = fm.rooms[fm.spawn_room]
    spawn_cell = (spawn.cx, spawn.cz)
    elev_fn = lc.make_elevation_lookup(fm.walkable, spine, comp)
    spawn_y = elev_fn(spawn_cell)

    def _agent_spawn(x: float, y: float, z: float) -> List[float]:
        # Agents spawn ON their cell's floor (synth deck 1.2 m), not inside it.
        # Explicit non-surface y (hub NPCs at -3.85) is deliberate — keep it.
        if abs(y - 0.15) > 1e-6:
            return [float(x), float(y), float(z)]
        c = (int(round(x / CELL + gx / 2 - 0.5)), int(round(z / CELL + gz / 2 - 0.5)))
        return [float(x), float(elev_fn(c)) + 0.15, float(z)]

    building = comp.next_faction if comp.mix_mode == "transition" else fm.faction_profile_id
    return {
        "version": 1,
        "name": name,
        "faction_profile": fm.faction_profile_id,
        "level_composition": comp.to_doc(),
        "building_system": fp.load_profile(building).building_system,
        "hub_model": "freeform_v1",
        "modules_x": max(1, gx // 5),
        "modules_z": max(1, gz // 5),
        "floors": floors,
        "pieces": pieces,
        "spawn_xz": [world_x(gx, spawn.cx), world_z(gz, spawn.cz)],
        "spawn_y": spawn_y,
        "extraction_xz": [world_x(gx, hub.trap0[0]) if hub else 0.0,
                          world_z(gz, hub.trap0[1]) if hub else 0.0],
        "hub_exits": hub_exits,
        "spine_len": len(spine),
        "transition_entrances": len([
            p for p in pieces if "transition_entrance" in (p.get("tags") or [])
        ]),
        "enemy_spawns": [_agent_spawn(x, y, z) for (x, y, z) in getattr(fm, 'enemy_spawns', [])],
        "npc_spawns": [_agent_spawn(x, y, z) for (x, y, z) in getattr(fm, 'npc_spawns', [])],
        "enemy_patrols": [[_agent_spawn(x, y, z) for (x, y, z) in route]
                          for route in getattr(fm, 'enemy_patrols', [])],
        "nav": build_nav_grid(pieces, fm, spine, comp, gx, gz),
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

def _preview_error_summary(errs: List[str], *, max_len: int = 140) -> str:
    """One readable line for the editor Proc status (avoid semicolon mash)."""
    if not errs:
        return "Generation failed"
    head = errs[0]
    if len(errs) > 1:
        head = f"{head} (+{len(errs) - 1} more)"
    if len(head) > max_len:
        return head[: max_len - 1] + "…"
    return head


def audit_floor_overlaps(pieces: List[dict], *, eps: float = 0.01) -> List[str]:
    """Two solid floor tiles at the same floor + centre → z-fighting."""
    errs: List[str] = []
    seen: Dict[Tuple[int, float, float], str] = {}
    for p in pieces:
        if not _piece_solid_floor(p):
            continue
        fl = int(p.get("floor_level", 0))
        wx, wz = float(p["x"]), float(p["z"])
        wy = round(float(p.get("y", 0.0)), 2)
        key = (fl, round(wx / eps) * eps, round(wz / eps) * eps, wy)
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
    if doc is not None:
        import level_composition as lc

        pieces = doc.get("pieces", [])
        gx, gz = fm.gx, fm.gz
        comp = (fm.composition or lc.LevelComposition(mix_mode="single")).normalized()
        spine, _, _, _ = lc.plan_zones_for_map(fm)
        elev_fn = (
            lc.make_elevation_lookup(fm.walkable, spine, comp)
            if comp.mix_mode == "transition"
            else (lambda _c: 0.0)
        )
        # Every walkable floor-0 tile needs a ceiling at floor 1 except open
        # elevated decks and the arrival shaft (spawn cell, roof left open on
        # purpose so a streamed child map drops in from the hub above).
        shaft = _arrival_shaft_cell(fm)
        for c in fm.walkable:
            wx, wz = world_x(gx, c[0]), world_z(gz, c[1])
            if elev_fn(c) > 0 or c == shaft:
                continue
            has_ceil = any(
                p.get("ceiling")
                and int(p.get("floor_level", 0)) == 1
                and abs(float(p["x"]) - wx) < 0.01
                and abs(float(p["z"]) - wz) < 0.01
                for p in pieces
            )
            if not has_ceil:
                errs.append(f"walkable cell {c} missing roof (ceiling at floor 1)")
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


def _place_agent_spawns(fm: FreeformMap, rng: random.Random, num_enemies: int, num_npcs: int) -> None:
    """Pick walkable positions for agents + bake per-enemy patrol routes.

    Enemies never spawn in the STARTING faction zone (transition maps: the
    ``prev`` zone around the spawn room; single maps: the spawn room itself) —
    the entry area is safe ground, which also evens out difficulty between
    maps for balancing. Each enemy gets a 2-4 waypoint patrol loop of nearby
    same-zone cells (``fm.enemy_patrols``, parallel to ``fm.enemy_spawns``).
    """
    if not fm.walkable:
        return
    import level_composition as lc

    walk_list = list(fm.walkable)
    spawn_c = (fm.rooms[fm.spawn_room].cx, fm.rooms[fm.spawn_room].cz) if fm.spawn_room < len(fm.rooms) else walk_list[0]
    end_c = (fm.rooms[fm.end_room].cx, fm.rooms[fm.end_room].cz) if fm.end_room < len(fm.rooms) else walk_list[-1]

    try:
        _, _, _, zone_lookup = lc.plan_zones_for_map(fm)
    except Exception:
        zone_lookup = lambda _c: None  # noqa: E731
    start_zone = zone_lookup(spawn_c)
    spawn_room_cells: Set[Cell] = (
        set(fm.rooms[fm.spawn_room].cells()) if fm.spawn_room < len(fm.rooms) else set()
    )
    hidden_cells: Set[Cell] = set()
    for i in getattr(fm, "hidden_rooms", []):
        hidden_cells |= set(fm.rooms[i].cells())

    def in_start_faction(c: Cell) -> bool:
        if start_zone is not None:
            return zone_lookup(c) == start_zone
        return c in spawn_room_cells

    # Enemy candidates: outside the starting faction, never in hidden rooms
    # (those belong to friendly keepers). Fall back gracefully on tiny maps.
    enemy_cells = [c for c in walk_list
                   if not in_start_faction(c) and c not in hidden_cells]
    if len(enemy_cells) < max(1, num_enemies):
        enemy_cells = [c for c in walk_list
                       if c not in spawn_room_cells and c not in hidden_cells]
    if not enemy_cells:
        enemy_cells = walk_list

    # Use a small positive Y so they sit visibly on floor 0 even if map has no explicit spawn_y yet.
    # Real elevation will be improved later; for editor sliders this makes them findable.
    AGENT_Y = 0.15

    def world_for(c: Cell) -> Tuple[float, float, float]:
        ox = (rng.random() - 0.5) * 2.8
        oz = (rng.random() - 0.5) * 2.8
        return (world_x(fm.gx, c[0]) + ox, AGENT_Y, world_z(fm.gz, c[1]) + oz)

    taken: List[Tuple[float, float]] = []
    def too_close(wx: float, wz: float) -> bool:
        for tx, tz in taken:
            if (wx - tx) ** 2 + (wz - tz) ** 2 < 3.5 ** 2:  # tighter for testability
                return True
        # milder exclusion from spawn/extraction
        if (wx - world_x(fm.gx, spawn_c[0]))**2 + (wz - world_z(fm.gz, spawn_c[1]))**2 < 3.5**2:
            return True
        if (wx - world_x(fm.gx, end_c[0]))**2 + (wz - world_z(fm.gz, end_c[1]))**2 < 3.5**2:
            return True
        return False

    def pick(n: int, cells: List[Cell]) -> Tuple[List[Tuple[float, float, float]], List[Cell]]:
        picks: List[Tuple[float, float, float]] = []
        pick_cells: List[Cell] = []
        attempts = max(n * 20, len(cells))
        for _ in range(attempts):
            if len(picks) >= n:
                break
            c = rng.choice(cells)
            w = world_for(c)
            if too_close(w[0], w[2]):
                continue
            picks.append(w)
            pick_cells.append(c)
            taken.append((w[0], w[2]))
        # Fallback: if still short on tiny maps, force-place remaining at random walkables (may cluster)
        while len(picks) < n and cells:
            c = rng.choice(cells)
            w = world_for(c)
            picks.append(w)
            pick_cells.append(c)
            taken.append((w[0], w[2]))
        return picks[:n], pick_cells[:n]

    spawns, spawn_cells = pick(max(0, num_enemies), enemy_cells)
    fm.enemy_spawns = spawns  # type: ignore[attr-defined]

    def bake_patrol(cell: Cell) -> List[Tuple[float, float, float]]:
        """2-4 waypoint loop: BFS ring cells 2..7 steps out, same zone,
        never dipping into the starting faction or hidden rooms."""
        zone = zone_lookup(cell)
        seen: Dict[Cell, int] = {cell: 0}
        frontier = [cell]
        for depth in range(1, 8):
            nxt: List[Cell] = []
            for c in frontier:
                for dx, dz in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    nb = (c[0] + dx, c[1] + dz)
                    if (nb in fm.walkable and nb not in seen
                            and not in_start_faction(nb) and nb not in hidden_cells
                            and zone_lookup(nb) == zone):
                        seen[nb] = depth
                        nxt.append(nb)
            frontier = nxt
            if not frontier:
                break
        ring = [c for c, d in seen.items() if 2 <= d <= 7]
        rng.shuffle(ring)
        # Waypoints spaced at least 2 cells apart so the loop reads as a route.
        wps: List[Cell] = [cell]
        for c in ring:
            if len(wps) >= 2 + rng.randrange(3):
                break
            if all(abs(c[0] - w[0]) + abs(c[1] - w[1]) >= 2 for w in wps):
                wps.append(c)
        if len(wps) < 2:
            return []  # no room to patrol — enemy will idle-wander instead
        return [(world_x(fm.gx, c[0]), AGENT_Y, world_z(fm.gz, c[1])) for c in wps]

    fm.enemy_patrols = [bake_patrol(c) for c in spawn_cells]  # type: ignore[attr-defined]

    # NPCs live only in the HUB and in HIDDEN rooms (user rule) — never in the
    # regular map areas the enemies roam. Counts are placement RULES (plan4
    # 2026-07-07), not a knob (`num_npcs` is accepted but ignored): every
    # hidden room gets 1-2 keepers; the hub gets 1-2 free wanderers here plus
    # one keeper per market stall, appended by _dress_hub_and_hidden (which is
    # the code that knows where the stalls end up).
    npc_spawns: List[Tuple[float, float, float]] = []
    for i in getattr(fm, "hidden_rooms", []):
        r = fm.rooms[i]
        npc_spawns.append((world_x(fm.gx, r.cx), AGENT_Y, world_z(fm.gz, r.cz)))
        if rng.random() < 0.5:  # some hideouts keep a second pair of eyes
            npc_spawns.append((world_x(fm.gx, r.cx) + 1.2, AGENT_Y,
                               world_z(fm.gz, r.cz) - 0.9))
    if fm.hub:
        # Hub floor -1 surface sits at -MOD_H. Avoid every hole cell AND its
        # neighbours (drop shafts to -2, extraction landing) so a wandering
        # NPC can't stroll off an edge.
        avoid: Set[Cell] = set(fm.hub.holes1) | {fm.hub.trap0}
        avoid |= {e.trap for e in fm.hub.exits}
        avoid |= {(c[0] + dx, c[1] + dz)
                  for c in list(avoid) for dx in (-1, 0, 1) for dz in (-1, 0, 1)}
        hub_cells = [c for c in fm.hub.floor1 if c not in avoid]
        rng.shuffle(hub_cells)
        n_hub = 1 + rng.randrange(2)  # 1-2 mobile hub NPCs
        for c in hub_cells[:n_hub]:
            ox = (rng.random() - 0.5) * 2.0
            oz = (rng.random() - 0.5) * 2.0
            npc_spawns.append((world_x(fm.gx, c[0]) + ox, -4.0 + 0.15,
                               world_z(fm.gz, c[1]) + oz))
    fm.npc_spawns = npc_spawns  # type: ignore[attr-defined]


# ─── report / output ─────────────────────────────────────────────────────────

def build_report(fm: FreeformMap, doc: dict, seed: Optional[int], elapsed: float, path: str) -> dict:
    comp = doc.get("level_composition") or {}
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
        "level_composition": comp,
        "spine_len": doc.get("spine_len", 0),
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
    faction_profile_id: str = "industrial_default",
    mix_mode: str = "transition",
    prev_faction: str = "priesthood",
    next_faction: str = "industrial_default",
    default_faction: str = "industrial_default",
    prev_fraction: float = 0.15,
    default_fraction: float = 0.60,
    next_fraction: float = 0.35,
    num_enemies: int = 5,
    num_npcs: int = 3,
    hall_rooms: int = 2,
) -> dict:
    """Generate one free-form map, write it, export the layout, return a report.

    Shared by the CLI and by `gen_maps.generate_map_report` (editor Proc tab),
    so the editor preview produces free-form maps without a Rust rebuild.
    """
    import gen_maps  # reuse layout export
    import level_composition as lc

    composition = lc.LevelComposition(
        mix_mode=mix_mode,
        prev_faction=prev_faction,
        next_faction=next_faction,
        default_faction=default_faction,
        prev_fraction=prev_fraction,
        default_fraction=default_fraction,
        next_fraction=next_fraction,
    ).normalized()

    t0 = time.time()
    fm: Optional[FreeformMap] = None
    base_seed = seed if seed is not None else random.randint(0, 2**31 - 1)
    for k in range(max(1, attempts)):
        cand = generate_map(
            base_seed + k, cells=cells, max_rooms=max_rooms,
            room_min=room_min, room_max=room_max, loops=loops,
            organicness=organicness, corridor_width=corridor_width,
            hidden_area_prevalence=hidden_area_prevalence,
            faction_profile_id=faction_profile_id,
            composition=composition,
            num_enemies=num_enemies,
            num_npcs=num_npcs,
            hall_rooms=hall_rooms,
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
            "error": _preview_error_summary(overlap),
            "elapsed_s": round(time.time() - t0, 2),
        }
    geo = validate(fm, doc)
    if geo:
        return {
            "ok": False,
            "seed": seed,
            "error": _preview_error_summary(geo),
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
    ap.add_argument('--halls', type=int, default=2,
                    help='guaranteed-large factory-hall rooms seeded mid-grid')
    ap.add_argument('--loops', type=int, default=3)
    ap.add_argument('--organicness', type=float, default=0.0,
                    help='0=clean L corridors, 1=winding jogged routes')
    ap.add_argument('--corridor-width', type=float, default=1.0,
                    help='1.0=all 1-wide, 2.0=all 2-wide, 1.3=~30%% 2-wide')
    ap.add_argument('--hidden', type=float, default=0.0,
                    help='hidden-area prevalence 0-1 (dead-end secret rooms)')
    ap.add_argument('--faction-profile', default='industrial_default',
                    help='whole-map profile when --mix-mode single')
    ap.add_argument('--mix-mode', default='transition', choices=('single', 'transition'),
                    help='single=one profile; transition=start/middle/end zones')
    ap.add_argument('--prev-faction', default='priesthood')
    ap.add_argument('--next-faction', default='industrial_default')
    ap.add_argument('--default-faction', default='industrial_default')
    ap.add_argument('--prev-fraction', type=float, default=0.15)
    ap.add_argument('--default-fraction', type=float, default=0.60)
    ap.add_argument('--next-fraction', type=float, default=0.35)
    ap.add_argument('--num-enemies', type=int, default=5, help='Number of enemies')
    ap.add_argument('--num-npcs', type=int, default=3,
                    help='DEPRECATED, ignored: NPC counts are placement rules '
                         '(1-2 per hidden room, 3 stall keepers + 1-2 wanderers in the hub)')
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
        faction_profile_id=args.faction_profile,
        mix_mode=args.mix_mode,
        prev_faction=args.prev_faction,
        next_faction=args.next_faction,
        default_faction=args.default_faction,
        prev_fraction=args.prev_fraction,
        default_fraction=args.default_fraction,
        next_fraction=args.next_fraction,
        num_enemies=args.num_enemies,
        num_npcs=args.num_npcs,
        hall_rooms=args.halls,
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
