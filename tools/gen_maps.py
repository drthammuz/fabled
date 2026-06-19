#!/usr/bin/env python3
"""
Fabled – procedural 5×5 Kenney map generator.

Two-phase generation (similar in spirit to strat_planned in gen_modules.py):

  1. High-level design – paint which module slots connect (spawn and end in
     opposite corners; spanning connected graph over all 25 slots; target ~2.25
     average connections per module).

  2. Module placement – pick real modules from a pool whose boundary openings
     match the painted graph, including tile alignment on the shared 5-cell
     face.  Modules may be rotated; unused exits are closed with template-wall.
     Backtracks when a placement dead-ends.

Usage:
    python tools/gen_maps.py
    python tools/gen_maps.py --pool space_rooms --seed 42 --attempts 50
    python tools/gen_maps.py --pool-batch 10 --seed 1

Output:
    userinput/maps/gen_map_<timestamp>.json
    userinput/maps/pool/map_NNN.json  (with --pool-batch)
"""

from __future__ import annotations

import argparse
import json
import math
import random
import sys
import time
from collections import deque
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Set, Tuple

# Reuse module constants / grid logic from the module generator.
sys.path.insert(0, str(Path(__file__).parent))
import gen_modules as gm  # noqa: E402

PI = gm.PI
PI2 = gm.PI2
PI32 = gm.PI32
CELLS = gm.CELLS
CELL_M = gm.CELL_M
MODULE_M = gm.MODULE_M
SIDES = gm.SIDES
OPPOSITE = gm.OPPOSITE
DELTA = gm.DELTA
PIECE_DB = gm.PIECE_DB
CellGrid = gm.CellGrid
PlacedPiece = gm.PlacedPiece
pp = gm.pp
cell_cx = gm.cell_cx
cell_cz = gm.cell_cz
rotated_faces = gm.rotated_faces
rotated_dims = gm.rotated_dims

import probe_map_geometry as probe  # noqa: E402

_CATALOG: Optional[Dict] = None


def get_catalog() -> Dict:
    global _CATALOG
    if _CATALOG is None:
        _CATALOG = probe.load_catalog()
    return _CATALOG


def mesh_exits(pieces: List[PlacedPiece]) -> Dict[str, Set[int]]:
    """Open border tiles from merged GLB mesh (authoritative)."""
    return probe.border_exits_for_pieces(pieces)


def module_exits_from_data(data: dict, pieces: List[PlacedPiece]) -> Dict[str, Set[int]]:
    """Use saved border_exits when present; otherwise probe mesh."""
    stored = probe.border_exits_from_json(data)
    if stored is not None:
        return stored
    return mesh_exits(pieces)


TARGET_AVG_DEGREE = 2.25
MAP_MODULES = 5
MAP_DIR = Path("userinput/maps")
POOL_DIR = MAP_DIR / "pool"
LAYOUT_PATH = Path("userinput/kenney_layout.json")
EXTRACTION_HOLE_STEM = "template-floor-hole"
HUB_EXIT_GATE = "gate"
EXTRACTION_ROOM_STEM = "room-large"
# Hub sits one MOD_H below the stretch floor (see shared::level::MOD_H).
HUB_FLOOR_LEVEL = -1
DEPTH_FLOOR_LEVEL = -2
HUB_ROOM_STEM = "room-large"
# Branch level group ids (distinct from stretch modules 1–9).
GID_L2 = 92
GID_L3 = 93
GID_L4 = 94
# Shared-face openings must land on the same of the 5 boundary tiles.
# Kenney room-large / most generated corridors use the centre tile (index 2).
CENTER_TILE = 2
# Grid-aligned stair opening offset from west module centre (cell ix=3 → +4 m).
# Must be a multiple of CELL_M to land exactly on a 4 m cell centre.
STAIR_CELL_DX = 4.0
# Hub floor-1 hole offsets from hub module centre (must be multiples of CELL_M).
# West drop sits in the SW corner (ix=0, iz=4) so it is OFF the centre row that the
# player walks along from the landing to the west gate / stairs.
WEST_DROP_DX = -8.0   # west drop tile (ix=0)
WEST_DROP_DZ = 8.0    # west drop tile (iz=4) — off the gate path
L3_DROP_DZ  = -8.0    # L3 pit drop tile (iz=0)


# ─── floor transition rules (mirrors `shared/kenney_transitions.rs`) ─────────
#
# Module-local tile grid: NW corner = (0, 0), ix increases east, iz increases south.
#   stair up (bottom)->(top) from floor a to b:
#     lowest step on `bottom`, run ends on `top`; omit template-floor on BOTH tiles
#     on BOTH floor bands (no double texture under/over the stair mesh).
#   trap down (tile) from floor a to b:
#     omit floor on `a` only; landing on `b` at the same tile stays solid.


@dataclass(frozen=True)
class ModuleTile:
    ix: int
    iz: int

    def offset_m(self) -> Tuple[float, float]:
        """Metres from module centre (matches gen_modules.cell_cx/cz)."""
        return cell_cx(self.ix), cell_cz(self.iz)


@dataclass(frozen=True)
class StairUp:
    bottom: ModuleTile
    top: ModuleTile
    from_floor: int
    to_floor: int

    def module_holes(self) -> List[Tuple[float, float]]:
        """Offsets from module centre where template-floor must be absent."""
        return [self.bottom.offset_m(), self.top.offset_m()]


@dataclass(frozen=True)
class TrapDown:
    tile: ModuleTile
    from_floor: int
    to_floor: int

    def module_hole(self) -> Tuple[float, float]:
        return self.tile.offset_m()


# Hub L2: (2,2)->(3,2) west module, depth -2 up to hub -1.
HUB_L2_STAIR = StairUp(ModuleTile(2, 2), ModuleTile(3, 2), DEPTH_FLOOR_LEVEL, HUB_FLOOR_LEVEL)


# ─── coordinate helpers ──────────────────────────────────────────────────────

def world_to_cell(x: float, z: float) -> Tuple[int, int]:
    return round((x + 8.0) / 4.0), round((z + 8.0) / 4.0)


def module_center(col: int, row: int) -> Tuple[float, float]:
    """World centre of module slot (col, row) on the map grid."""
    wx0 = -(MAP_MODULES * CELLS * CELL_M) * 0.5
    wz0 = wx0
    return wx0 + col * MODULE_M + MODULE_M * 0.5, wz0 + row * MODULE_M + MODULE_M * 0.5


def neighbor_slot(col: int, row: int, side: str) -> Tuple[int, int]:
    dx, dz = DELTA[side]
    return col + dx, row + dz


def in_map(col: int, row: int) -> bool:
    return 0 <= col < MAP_MODULES and 0 <= row < MAP_MODULES


# ─── module analysis ─────────────────────────────────────────────────────────
# Boundary openings are derived directly from placed pieces (not a walkability
# replay).  This matches what you see in the editor.

WALL_YAW = {'N': PI, 'S': 0.0, 'E': PI2, 'W': PI32}
_ROOM_PREFIXES = ('room-', 'stairs-')


def _is_room_piece(stem: str) -> bool:
    return stem.startswith(_ROOM_PREFIXES)


def _piece_footprint(pdef: gm.PieceDef, ix: int, iz: int, yaw: float) -> Tuple[int, int, int, int]:
    """Return (ox, oz, nx, nz) cell footprint from piece centre cell."""
    nx, nz = rotated_dims(pdef.nx, pdef.nz, yaw)
    return ix - nx // 2, iz - nz // 2, nx, nz


def _add_boundary(
    out: Dict[str, Set[int]],
    side: str,
    idx: int,
) -> None:
    if 0 <= idx < CELLS:
        out[side].add(idx)


_CENTER_ONLY_ROOMS = frozenset({'room-large', 'room-large-variation'})


def _compute_boundary_openings(
    pieces: List[PlacedPiece],
    full_room_faces: bool,
) -> Dict[str, Set[int]]:
    """
    Derive boundary openings from piece stems + positions.

    full_room_faces=False (legacy model): every room GLB is a single centre tile.
    full_room_faces=True (honest): room-small / room-corner / room-wide expose
    every footprint cell along each open side — matching what you see in-editor.
  room-large keeps a single centre tile (wall_X() at module edge centres).
    """
    out: Dict[str, Set[int]] = {s: set() for s in SIDES}

    for piece in pieces:
        if piece.stem == 'template-wall':
            continue
        pdef = PIECE_DB.get(piece.stem)
        if not pdef or not pdef.is_structural:
            continue

        ix, iz = world_to_cell(piece.x, piece.z)
        ox, oz, nx, nz = _piece_footprint(pdef, ix, iz, piece.yaw)
        opens = rotated_faces(pdef.open_local, piece.yaw)

        if nx == 1 and nz == 1:
            if 'N' in opens and iz == 0:
                _add_boundary(out, 'N', ix)
            if 'S' in opens and iz == CELLS - 1:
                _add_boundary(out, 'S', ix)
            if 'E' in opens and ix == CELLS - 1:
                _add_boundary(out, 'E', iz)
            if 'W' in opens and ix == 0:
                _add_boundary(out, 'W', iz)
            continue

        use_full_edge = (
            full_room_faces
            and _is_room_piece(piece.stem)
            and piece.stem not in _CENTER_ONLY_ROOMS
        )

        if _is_room_piece(piece.stem) and not use_full_edge:
            if 'N' in opens:
                _add_boundary(out, 'N', ox + nx // 2)
            if 'S' in opens:
                _add_boundary(out, 'S', ox + nx // 2)
            if 'E' in opens:
                _add_boundary(out, 'E', oz + nz // 2)
            if 'W' in opens:
                _add_boundary(out, 'W', oz + nz // 2)
            continue

        # Wide corridors + full-face room GLBs.
        if 'N' in opens:
            for dix in range(nx):
                bx = ox + dix
                if oz == 0:
                    _add_boundary(out, 'N', bx)
        if 'S' in opens:
            for dix in range(nx):
                bx = ox + dix
                if oz + nz - 1 == CELLS - 1:
                    _add_boundary(out, 'S', bx)
        if 'E' in opens:
            for diz in range(nz):
                bz = oz + diz
                if ox + nx - 1 == CELLS - 1:
                    _add_boundary(out, 'E', bz)
        if 'W' in opens:
            for diz in range(nz):
                bz = oz + diz
                if ox == 0:
                    _add_boundary(out, 'W', bz)

    for piece in pieces:
        if piece.stem != 'template-wall':
            continue
        if abs(piece.z + 10.0) < 0.6:
            idx = round((piece.x + 8.0) / 4.0)
            out['N'].discard(idx)
        elif abs(piece.z - 10.0) < 0.6:
            idx = round((piece.x + 8.0) / 4.0)
            out['S'].discard(idx)
        elif abs(piece.x - 10.0) < 0.6:
            idx = round((piece.z + 8.0) / 4.0)
            out['E'].discard(idx)
        elif abs(piece.x + 10.0) < 0.6:
            idx = round((piece.z + 8.0) / 4.0)
            out['W'].discard(idx)

    return out


def boundary_openings(pieces: List[PlacedPiece]) -> Dict[str, Set[int]]:
    """Legacy centre-tile model (underestimates room-small openings)."""
    return _compute_boundary_openings(pieces, full_room_faces=False)


def honest_boundary_openings(pieces: List[PlacedPiece]) -> Dict[str, Set[int]]:
    """What the editor actually shows."""
    return _compute_boundary_openings(pieces, full_room_faces=True)


def open_sides_from_exits(exits: Dict[str, Set[int]]) -> Set[str]:
    return {s for s, tiles in exits.items() if tiles}


def wall_for_opening(side: str, tile: int) -> PlacedPiece:
    if side == 'N':
        return pp('template-wall', cell_cx(tile), -10.0, WALL_YAW['N'])
    if side == 'S':
        return pp('template-wall', cell_cx(tile),  10.0, WALL_YAW['S'])
    if side == 'E':
        return pp('template-wall',  10.0, cell_cz(tile), WALL_YAW['E'])
    return pp('template-wall', -10.0, cell_cz(tile), WALL_YAW['W'])


def close_for_placement(
    pieces: List[PlacedPiece],
    required: Set[str],
    keep_tile: Optional[Dict[str, int]] = None,
) -> List[PlacedPiece]:
    """Wall honest openings except one kept tile per required side."""
    keep_tile = keep_tile or {}
    out = list(pieces)
    opens = honest_boundary_openings(out)
    for side in SIDES:
        if side in required:
            allowed = {keep_tile.get(side, CENTER_TILE)}
        else:
            allowed = set()
        for tile in sorted(opens.get(side, set()) - allowed):
            out.append(wall_for_opening(side, tile))
    return out


# Legacy grid replay kept only for walkable-centre spawn fallback.
def stamp_piece(grid: CellGrid, piece: PlacedPiece) -> None:
    pdef = PIECE_DB.get(piece.stem)
    if not pdef or not pdef.is_structural:
        return

    ix, iz = world_to_cell(piece.x, piece.z)
    nx, nz = rotated_dims(pdef.nx, pdef.nz, piece.yaw)
    ox, oz = ix - nx // 2, iz - nz // 2
    opens = rotated_faces(pdef.open_local, piece.yaw)

    if nx == 1 and nz == 1:
        cell = grid.c(ix, iz)
        cell.walkable = True
        if len(opens) == 2:
            a, b = tuple(opens)
            grid.fill_corridor_cell(ix, iz, a, b)
        else:
            for face in opens:
                cell.open_faces.add(face)
    else:
        grid.fill_room(ox, oz, pdef.nx, pdef.nz, set(opens), piece.yaw)


def apply_wall_closures(grid: CellGrid, pieces: List[PlacedPiece]) -> None:
    """Discard open faces blocked by template-wall pieces."""
    for p in pieces:
        if p.stem != 'template-wall':
            continue
        ix, iz = world_to_cell(p.x, p.z)
        if abs(p.z + 10.0) < 0.6 and iz == 0:
            grid.c(ix, 0).open_faces.discard('N')
        elif abs(p.z - 10.0) < 0.6 and iz == CELLS - 1:
            grid.c(ix, CELLS - 1).open_faces.discard('S')
        elif abs(p.x - 10.0) < 0.6 and ix == CELLS - 1:
            grid.c(CELLS - 1, iz).open_faces.discard('E')
        elif abs(p.x + 10.0) < 0.6 and ix == 0:
            grid.c(0, iz).open_faces.discard('W')


def pieces_from_json(raw: list) -> List[PlacedPiece]:
    out: List[PlacedPiece] = []
    for p in raw:
        out.append(PlacedPiece(
            stem=p['stem'], x=float(p['x']), z=float(p['z']),
            yaw=float(p['yaw']), floor_level=int(p.get('floor_level', 0)),
            scale=float(p.get('scale', 1.0)),
        ))
    return out


def replay_pieces_to_grid(pieces: List[PlacedPiece]) -> CellGrid:
    grid = CellGrid()
    for p in pieces:
        if PIECE_DB.get(p.stem, gm.PieceDef('', 0, 0, gm._fs(), False)).is_structural:
            stamp_piece(grid, p)
    apply_wall_closures(grid, pieces)
    return grid


def exits_by_side(grid: CellGrid) -> Dict[str, Set[int]]:
    """Deprecated — prefer boundary_openings(pieces)."""
    result: Dict[str, Set[int]] = {s: set() for s in SIDES}
    for ix, iz, side in grid.entrances():
        if side in ('N', 'S'):
            result[side].add(ix)
        else:
            result[side].add(iz)
    return result


def build_variants(path: Path, data: dict, side_hint: Optional[Set[str]] = None) -> List[ModuleVariant]:
    """
    One variant per module, saved orientation only.

    The map editor places modules by translating piece offsets — it does not
    rotate the group.  Pre-rotating piece (x, z) in Python does not match how
    per-piece yaw is applied in-engine, so we never spin modules here.
    """
    base_pieces = pieces_from_json(data.get('pieces', []))
    name = data.get('name', path.stem)
    fm = data.get('floor_mask', {})
    floor_mask = list(fm.get('cells', [True] * (CELLS * CELLS)))
    exits = module_exits_from_data(data, base_pieces)
    detected = open_sides_from_exits(exits)
    open_sides = side_hint if side_hint else detected
    if not open_sides:
        return []
    grid = replay_pieces_to_grid(base_pieces)
    return [ModuleVariant(path, name, 0, base_pieces, grid, open_sides, exits, floor_mask)]


@dataclass
class ModuleVariant:
    path: Path
    name: str
    rotation: int          # always 0 (see build_variants)
    pieces: List[PlacedPiece]
    grid: CellGrid
    open_sides: Set[str]
    exits: Dict[str, Set[int]]
    floor_mask: List[bool]


# ─── phase 1: high-level connectivity paint ──────────────────────────────────

Slot = Tuple[int, int]
Side = str


@dataclass
class HighLevelDesign:
    spawn: Slot
    end: Slot
    # slot -> set of sides that must connect to a neighbour
    connections: Dict[Slot, Set[Side]]
    edges: Set[Tuple[Slot, Slot]]

    def degree(self, slot: Slot) -> int:
        return len(self.connections.get(slot, set()))

    def avg_degree(self) -> float:
        if not self.connections:
            return 0.0
        return sum(len(v) for v in self.connections.values()) / len(self.connections)

    def ascii(self) -> str:
        # row 0 = north (−Z), printed top-to-bottom so it matches the editor view.
        lines = ['    N', 'W   E']
        for row in range(MAP_MODULES):
            row_chars = []
            for col in range(MAP_MODULES):
                slot = (col, row)
                if slot == self.spawn:
                    ch = 'S'
                elif slot == self.end:
                    ch = 'E'
                else:
                    ch = str(self.degree(slot))
                row_chars.append(ch)
            lines.append(' '.join(row_chars))
        lines.append('    S')
        return '\n'.join(lines)


def _add_edge(connections: Dict[Slot, Set[Side]], a: Slot, b: Slot) -> None:
    ac, ar = a
    bc, br = b
    dc, dr = bc - ac, br - ar
    if (dc, dr) == (0, -1):
        connections[a].add('N')
        connections[b].add('S')
    elif (dc, dr) == (0, 1):
        connections[a].add('S')
        connections[b].add('N')
    elif (dc, dr) == (1, 0):
        connections[a].add('E')
        connections[b].add('W')
    elif (dc, dr) == (-1, 0):
        connections[a].add('W')
        connections[b].add('E')


def design_high_level(rng: random.Random) -> HighLevelDesign:
    """
    Build a connected graph on all module slots.

    Spawn and end sit in opposite corners.  Start from a random spanning tree,
    then add chords until the average degree approaches TARGET_AVG_DEGREE.
    """
    last = MAP_MODULES - 1
    corners = [
        ((0, 0), (last, last)),
        ((last, 0), (0, last)),
        ((0, last), (last, 0)),
        ((last, last), (0, 0)),
    ]
    spawn, end = rng.choice(corners)

    all_slots: List[Slot] = [(c, r) for r in range(MAP_MODULES) for c in range(MAP_MODULES)]
    connections: Dict[Slot, Set[Side]] = {s: set() for s in all_slots}
    edges: Set[Tuple[Slot, Slot]] = set()

    # Randomised spanning tree via shuffled BFS expansion.
    filled: Set[Slot] = {spawn}
    frontier = [spawn]
    rng.shuffle(frontier)

    while len(filled) < len(all_slots):
        if not frontier:
            # Pick a filled cell and grow toward nearest unfilled.
            src = rng.choice(list(filled))
            frontier = [src]
        cur = frontier.pop()
        nbrs = []
        for side in SIDES:
            nc, nr = neighbor_slot(cur[0], cur[1], side)
            if in_map(nc, nr) and (nc, nr) not in filled:
                nbrs.append(((nc, nr), side))
        if not nbrs:
            continue
        rng.shuffle(nbrs)
        nxt, side = nbrs[0]
        filled.add(nxt)
        frontier.append(nxt)
        edge = tuple(sorted((cur, nxt)))
        edges.add(edge)  # type: ignore[arg-type]
        _add_edge(connections, cur, nxt)

    # Ensure end is on the tree (always true for spanning tree) and has ≥1 link.
    assert end in filled

    # Add extra adjacency edges to approach target average degree.
    target_edges = int(round(len(all_slots) * TARGET_AVG_DEGREE / 2))
    adjacency: List[Tuple[Slot, Slot]] = []
    for c in range(MAP_MODULES):
        for r in range(MAP_MODULES):
            for side in ('E', 'S'):
                a = (c, r)
                b = neighbor_slot(c, r, side)
                if in_map(*b):
                    adjacency.append(tuple(sorted((a, b))))  # type: ignore[arg-type]
    rng.shuffle(adjacency)

    for edge in adjacency:
        if len(edges) >= target_edges:
            break
        if edge in edges:
            continue
        a, b = edge
        da, db = len(connections[a]), len(connections[b])
        if da >= 4 or db >= 4:
            continue
        # Prefer adding to slots that are still below 3 connections.
        if da >= 3 and db >= 3 and rng.random() < 0.35:
            continue
        edges.add(edge)
        _add_edge(connections, a, b)

    return HighLevelDesign(spawn=spawn, end=end, connections=connections, edges=edges)


# ─── phase 2: module placement ───────────────────────────────────────────────

@dataclass
class PlacedModule:
    col: int
    row: int
    variant: ModuleVariant
    pieces: List[PlacedPiece]
    grid: CellGrid
    floor_mask: List[bool]


@dataclass
class PlacementState:
    design: HighLevelDesign
    pool: List[ModuleVariant]
    rng: random.Random
    placed: Dict[Slot, PlacedModule] = field(default_factory=dict)

    def required_sides(self, slot: Slot) -> Set[Side]:
        return set(self.design.connections.get(slot, set()))

    def neighbor_tile_constraints(self, slot: Slot) -> Dict[Side, int]:
        """Tile index each side must keep open to match a placed neighbour."""
        constraints: Dict[Side, int] = {}
        col, row = slot
        for side in SIDES:
            nc, nr = neighbor_slot(col, row, side)
            nslot = (nc, nr)
            if nslot not in self.placed:
                continue
            if side not in self.design.connections.get(slot, set()):
                continue
            opp = OPPOSITE[side]
            nopens = mesh_exits(self.placed[nslot].pieces).get(opp, set())
            if not nopens:
                continue
            if len(nopens) > 1:
                # Prefer centre when neighbour has multiple (should not happen after close).
                constraints[side] = CENTER_TILE if CENTER_TILE in nopens else min(nopens)
            else:
                constraints[side] = next(iter(nopens))
        return constraints

    def compatible_variants(self, slot: Slot) -> List[ModuleVariant]:
        required = self.required_sides(slot)
        tile_req = self.neighbor_tile_constraints(slot)
        out: List[ModuleVariant] = []
        for var in self.pool:
            if not required.issubset(var.open_sides):
                continue
            ok = True
            for side in required:
                tile = tile_req.get(side, CENTER_TILE)
                if tile not in var.exits.get(side, set()):
                    ok = False
                    break
            if ok:
                out.append(var)
        return out

    def close_unused_exits(
        self, var: ModuleVariant, required: Set[Side], tile_req: Dict[Side, int],
    ) -> List[PlacedPiece]:
        return probe.close_for_placement_mesh(
            var.pieces, required, tile_req,
        )

    def _verify_slot(
        self, slot: Slot, pieces: List[PlacedPiece],
        required: Set[Side], tile_req: Dict[Side, int],
    ) -> List[str]:
        errs: List[str] = []
        opens = mesh_exits(pieces)
        for side in required:
            want = {tile_req.get(side, CENTER_TILE)}
            if opens.get(side, set()) != want:
                errs.append(f"{slot} {side} want open {want} mesh has {opens.get(side, set())}")
        for side in SIDES:
            if side in required:
                continue
            if opens.get(side):
                errs.append(f"{slot} mesh leak {side} tiles {opens[side]}")
        return errs

    def placement_order(self) -> List[Slot]:
        sc, sr = self.design.spawn
        dist: Dict[Slot, int] = {(sc, sr): 0}
        q = deque([(sc, sr)])
        while q:
            c, r = q.popleft()
            for side in SIDES:
                nc, nr = neighbor_slot(c, r, side)
                if not in_map(nc, nr):
                    continue
                nxt = (nc, nr)
                if nxt in dist:
                    continue
                dist[nxt] = dist[(c, r)] + 1
                q.append(nxt)
        return sorted(dist.keys(), key=lambda s: (dist[s], s[1], s[0]))

    def try_place_all(self) -> bool:
        order = self.placement_order()
        return self._place_from(0, order)

    def _place_from(self, idx: int, order: List[Slot]) -> bool:
        if idx >= len(order):
            return True

        slot = order[idx]
        if slot in self.placed:
            return self._place_from(idx + 1, order)

        required = self.required_sides(slot)
        tile_req = self.neighbor_tile_constraints(slot)
        candidates = self.compatible_variants(slot)
        self.rng.shuffle(candidates)

        for var in candidates:
            pieces = self.close_unused_exits(var, required, tile_req)
            slot_errs = self._verify_slot(slot, pieces, required, tile_req)
            if slot_errs:
                continue
            grid = replay_pieces_to_grid(pieces)
            col, row = slot
            floor_mask = var.floor_mask
            self.placed[slot] = PlacedModule(col, row, var, pieces, grid, floor_mask)
            if self._place_from(idx + 1, order):
                return True
            del self.placed[slot]

        return False


# ─── map output ──────────────────────────────────────────────────────────────

def compute_spawn(design: HighLevelDesign, placed: Dict[Slot, PlacedModule]) -> List[float]:
    """Spawn at the centre of the centre walkable cell in the start module."""
    sc, sr = design.spawn
    cx, cz = module_center(sc, sr)
    pm = placed.get(design.spawn)
    if pm is None:
        return [cx, cz]
    # Prefer the module centre cell (2, 2) when walkable.
    if pm.grid.c(CENTER_TILE, CENTER_TILE).walkable:
        return [cx, cz]
    # Fall back to any walkable cell nearest module centre.
    best = (cx, cz)
    best_d = float('inf')
    for iz in range(CELLS):
        for ix in range(CELLS):
            if not pm.grid.c(ix, iz).walkable:
                continue
            px, pz = cell_cx(ix), cell_cz(iz)
            d = (px - 0.0) ** 2 + (pz - 0.0) ** 2
            if d < best_d:
                best_d = d
                best = (cx + px, cz + pz)
    return [best[0], best[1]]


def piece_footprint_covers_tile(
    p: dict, mcx: float, mcz: float, tile_ix: int, tile_iz: int,
) -> bool:
    lx = p['x'] - mcx
    lz = p['z'] - mcz
    pdef = PIECE_DB.get(p['stem'])
    if not pdef:
        return piece_origin_on_tile(p, mcx, mcz, tile_ix, tile_iz)
    ix, iz = world_to_cell(lx, lz)
    ox, oz, nx, nz = _piece_footprint(pdef, ix, iz, p['yaw'])
    return ox <= tile_ix < ox + nx and oz <= tile_iz < oz + nz


def piece_origin_on_tile(
    p: dict, mcx: float, mcz: float, tile_ix: int, tile_iz: int,
) -> bool:
    wx = mcx + cell_cx(tile_ix)
    wz = mcz + cell_cz(tile_iz)
    return abs(p['x'] - wx) < 0.15 and abs(p['z'] - wz) < 0.15


def in_module_bounds(p: dict, mcx: float, mcz: float) -> bool:
    half = MODULE_M * 0.5 + 0.1
    return abs(p['x'] - mcx) <= half and abs(p['z'] - mcz) <= half


def end_module_tile_constraints(
    design: HighLevelDesign,
    placed: Dict[Slot, PlacedModule],
    slot: Slot,
) -> Dict[str, int]:
    """Tile index each side must keep open to match a placed neighbour."""
    constraints: Dict[str, int] = {}
    col, row = slot
    for side in SIDES:
        nc, nr = neighbor_slot(col, row, side)
        nslot = (nc, nr)
        if nslot not in placed:
            continue
        if side not in design.connections.get(slot, set()):
            continue
        opp = OPPOSITE[side]
        nopens = mesh_exits(placed[nslot].pieces).get(opp, set())
        if not nopens:
            continue
        if len(nopens) > 1:
            constraints[side] = CENTER_TILE if CENTER_TILE in nopens else min(nopens)
        else:
            constraints[side] = next(iter(nopens))
    return constraints


def build_tiled_floor_room(
    mcx: float,
    mcz: float,
    floor_level: int,
    required: Set[str],
    group_id: int,
    tile_req: Optional[Dict[str, int]] = None,
    floor_holes: Optional[List[Tuple[float, float]]] = None,
    frame_holes: Optional[List[Tuple[float, float]]] = None,
) -> List[dict]:
    """Room built from single-cell template-floor tiles instead of a room-large shell.

    A full perimeter ring of single-cell template-wall pieces is placed (all 5
    boundary tiles per side), leaving the required door tile open on each
    required side.  This replaces the room-large shell, which previously supplied
    both the floor and the perimeter walls — close_for_placement_mesh only walls
    the shell's *door openings*, so without the full ring the rooms had no walls.

    Each cell in the 5×5 footprint gets a template-floor except cells listed in
    floor_holes, which are (dx, dz) offsets from the module centre.  Absent
    tiles == absent floor == absent physics — no carving or mask patching needed.
    """
    keep = tile_req or {}
    out: List[dict] = []
    # Full perimeter wall ring — one template-wall per boundary tile per side,
    # minus the kept door tile on each required side.
    for side in SIDES:
        door_tile = keep.get(side, CENTER_TILE) if side in required else None
        for tile in range(CELLS):
            if tile == door_tile:
                continue
            w = wall_for_opening(side, tile)
            out.append({
                "stem": w.stem,
                "x": mcx + w.x,
                "z": mcz + w.z,
                "yaw": w.yaw,
                "floor_level": floor_level,
                "scale": 1.0,
                "group_id": group_id,
            })
    # Per-cell floor tiles — skip hole cells.
    hole_cells: Set[Tuple[int, int]] = set()
    for dx, dz in (floor_holes or []):
        ix = CENTER_TILE + round(dx / CELL_M)
        iz = CENTER_TILE + round(dz / CELL_M)
        hole_cells.add((ix, iz))
    for iz in range(CELLS):
        for ix in range(CELLS):
            if (ix, iz) in hole_cells:
                continue
            out.append({
                "stem": "template-floor",
                "x": mcx + cell_cx(ix),
                "z": mcz + cell_cz(iz),
                "yaw": 0.0,
                "floor_level": floor_level,
                "scale": 1.0,
                "group_id": group_id,
            })
    # Decorative hole frame (raised rim, open centre) over drop holes. Its collider is
    # skipped (floor < 0 / extraction tile) so the hole stays physically open; this is
    # purely the visual ring the player sees around the pit.
    for dx, dz in (frame_holes or []):
        out.append({
            "stem": EXTRACTION_HOLE_STEM,
            "x": mcx + cell_cx(CENTER_TILE + round(dx / CELL_M)),
            "z": mcz + cell_cz(CENTER_TILE + round(dz / CELL_M)),
            "yaw": 0.0,
            "floor_level": floor_level,
            "scale": 1.0,
            "group_id": group_id,
        })
    return out


def build_end_extraction_room(
    mcx: float,
    mcz: float,
    required: Set[str],
    tile_req: Dict[str, int],
    group_id: int,
) -> List[dict]:
    """Finish module: tiled floor with walls closed except painted connections.

    The trap cell (module centre) is intentionally skipped; the caller places
    template-floor-hole there instead.
    """
    trap_hole = [(0.0, 0.0)]   # centre tile = extraction trap
    return build_tiled_floor_room(mcx, mcz, 0, required, group_id, tile_req, trap_hole)


def apply_extraction_and_hub(
    design: HighLevelDesign,
    placed: Dict[Slot, PlacedModule],
    floor0: List[bool],
    pieces_out: List[dict],
    cells_total: int,
) -> Tuple[List[float], List[bool]]:
    """
    Replace the finish module with a room-large extraction room, cut a single
    centre-tile floor hole, and spawn the hub one floor below.
    """
    ec, er = design.end
    end_slot = (ec, er)
    mcx, mcz = module_center(ec, er)
    hole_x = mcx + cell_cx(CENTER_TILE)
    hole_z = mcz + cell_cz(CENTER_TILE)
    extraction = [hole_x, hole_z]

    end_gid = max(
        (int(p['group_id']) for p in pieces_out if in_module_bounds(p, mcx, mcz) and p.get('group_id')),
        default=1,
    )

    pieces_out[:] = [
        p for p in pieces_out
        if p.get('floor_level', 0) != 0 or not in_module_bounds(p, mcx, mcz)
    ]

    required = set(design.connections.get(end_slot, set()))
    tile_req = end_module_tile_constraints(design, placed, end_slot)
    pieces_out.extend(build_end_extraction_room(mcx, mcz, required, tile_req, end_gid))

    pieces_out.append({
        "stem": EXTRACTION_HOLE_STEM,
        "x": hole_x,
        "z": hole_z,
        "yaw": 0.0,
        "floor_level": 0,
        "scale": 1.0,
        "group_id": end_gid,
    })

    for iz in range(CELLS):
        for ix in range(CELLS):
            mix = ec * CELLS + ix
            miz = er * CELLS + iz
            floor0[miz * cells_total + mix] = not (ix == CENTER_TILE and iz == CENTER_TILE)

    hub_floor = [False] * (cells_total * cells_total)
    for iz in range(CELLS):
        for ix in range(CELLS):
            mix = ec * CELLS + ix
            miz = er * CELLS + iz
            hub_floor[miz * cells_total + mix] = True

    # Initial hub room (fully solid — holes added by apply_hub_exits / apply_hub_branches).
    pieces_out.extend(
        build_tiled_floor_room(mcx, mcz, HUB_FLOOR_LEVEL, set(), end_gid)
    )

    return extraction, hub_floor


def set_slot_floor_mask(
    mask: List[bool],
    slot: Tuple[int, int],
    cells_total: int,
    value: bool = True,
) -> None:
    col, row = slot
    for iz in range(CELLS):
        for ix in range(CELLS):
            mix = col * CELLS + ix
            miz = row * CELLS + iz
            mask[miz * cells_total + mix] = value


def build_branch_room(
    mcx: float,
    mcz: float,
    floor_level: int,
    required: Set[str],
    group_id: int,
    tile_req: Optional[Dict[str, int]] = None,
    floor_holes: Optional[List[Tuple[float, float]]] = None,
    frame_holes: Optional[List[Tuple[float, float]]] = None,
) -> List[dict]:
    """Single-module branch room using per-cell floor tiles instead of a room shell."""
    return build_tiled_floor_room(
        mcx, mcz, floor_level, required, group_id, tile_req, floor_holes, frame_holes
    )


BRANCH_GIDS = frozenset({GID_L2, GID_L3, GID_L4})


def strip_module_floor_pieces(
    pieces_out: List[dict],
    mcx: float,
    mcz: float,
    floor_levels: Set[int],
) -> None:
    pieces_out[:] = [
        p for p in pieces_out
        if int(p.get("floor_level", 0)) not in floor_levels
        or not in_module_bounds(p, mcx, mcz)
    ]


def append_branch_props(
    pieces_out: List[dict],
    mcx: float,
    mcz: float,
    floor_level: int,
    group_id: int,
) -> None:
    """Light dressing so branch rooms read as finished destinations."""
    props = [
        ("template-floor-detail-a", 4.0, 4.0, 0.0),
        ("cables", -4.0, 0.0, PI2),
        ("template-detail", -4.0, -4.0, PI),
    ]
    for stem, lx, lz, yaw in props:
        pieces_out.append({
            "stem": stem,
            "x": mcx + lx,
            "z": mcz + lz,
            "yaw": yaw,
            "floor_level": floor_level,
            "scale": 1.0,
            "group_id": group_id,
        })


def world_x_to_map_ix(x: float) -> int:
    wx0 = -(MAP_MODULES * CELLS * CELL_M) * 0.5
    return int(round((x - wx0) / CELL_M - 0.5))


def cut_floor_cell(mask: List[bool], cells_total: int, x: float, z: float) -> None:
    ix = world_x_to_map_ix(x)
    iz = world_x_to_map_ix(z)
    if 0 <= ix < cells_total and 0 <= iz < cells_total:
        mask[iz * cells_total + ix] = False


def apply_hub_exits(
    end_slot: Tuple[int, int],
    extraction: List[float],
    pieces_out: List[dict],
    floors: Dict[str, dict],
    cells_total: int,
) -> Dict[str, dict]:
    """
    Prepare the hub (floor -1) with three exit anchors for streaming child maps.

    No child-map geometry is embedded — only the hub shell, west gate, and drop
    holes that candidates attach to at runtime.
    """
    ec, er = end_slot
    hub_slot = (ec, er)
    wc, wr = neighbor_slot(ec, er, 'W')
    if not in_map(wc, wr):
        return {}

    mcx_hub, mcz_hub = module_center(*hub_slot)
    mcx_w, mcz_w = module_center(wc, wr)
    hole_x, hole_z = extraction[0], extraction[1]
    west_hole_x = hole_x + WEST_DROP_DX
    west_hole_z = hole_z + WEST_DROP_DZ

    hub_gid = max(
        (
            int(p['group_id'])
            for p in pieces_out
            if p.get('group_id') is not None
            and in_module_bounds(p, mcx_hub, mcz_hub)
            and int(p.get('floor_level', 0)) == 0
        ),
        default=9,
    )

    # Strip any prior hub floor pieces before rebuilding with tiled floors.
    strip_module_floor_pieces(pieces_out, mcx_hub, mcz_hub, {HUB_FLOOR_LEVEL})

    # Rebuild hub with west opening toward the branch corridor.
    # Holes: west drop (SW corner, off the gate path) and L3 pit drop (iz=0);
    # landing tile is solid. Both drop holes get a visible template-floor-hole frame.
    hub_holes = [(WEST_DROP_DX, WEST_DROP_DZ), (0.0, L3_DROP_DZ)]
    pieces_out.extend(
        build_branch_room(
            mcx_hub, mcz_hub, HUB_FLOOR_LEVEL, {'W'}, hub_gid,
            {'W': CENTER_TILE}, hub_holes, frame_holes=hub_holes,
        )
    )
    pieces_out.append({
        "stem": HUB_EXIT_GATE,
        "x": mcx_hub - 10.0,
        "z": mcz_hub,
        "yaw": PI32,
        "floor_level": HUB_FLOOR_LEVEL,
        "scale": 1.0,
        "group_id": hub_gid,
    })

    hub_key = str(HUB_FLOOR_LEVEL)
    if hub_key not in floors:
        floors[hub_key] = {
            "cells_x": cells_total,
            "cells_z": cells_total,
            "cells": [False] * (cells_total * cells_total),
        }
    hub_mask = floors[hub_key]["cells"]
    set_slot_floor_mask(hub_mask, hub_slot, cells_total)
    set_slot_floor_mask(hub_mask, (wc, wr), cells_total)

    # Cut hole cells in mask to match absent floor tiles.
    # Landing tile (hole_x, hole_z) is kept SOLID — do NOT cut it.
    # Stairs span two cells (stair-top at +4 and one cell west at +0).
    holes = [
        (west_hole_x, west_hole_z),
        (mcx_hub, mcz_hub + L3_DROP_DZ),
        (mcx_w + STAIR_CELL_DX, mcz_w),
        (mcx_w, mcz_w),
    ]
    for hx, hz in holes:
        cut_floor_cell(hub_mask, cells_total, hx, hz)

    return {
        "2": {
            "x": mcx_w + 6.0,
            "z": mcz_w,
            "floor": HUB_FLOOR_LEVEL,
            "label": "West gate",
            "kind": "walk",
        },
        "3": {
            "x": hole_x,
            "z": hole_z,
            "floor": HUB_FLOOR_LEVEL,
            "label": "Centre pit",
            "kind": "drop",
        },
        "4": {
            "x": west_hole_x,
            "z": west_hole_z,
            "floor": HUB_FLOOR_LEVEL,
            "label": "West drop",
            "kind": "drop",
        },
    }


def apply_hub_branches(
    end_slot: Tuple[int, int],
    pieces_out: List[dict],
    floors: Dict[str, dict],
    cells_total: int,
) -> None:
    """
    Add hub branch levels L2–L4 (embedded, preloaded with L1):

      L3 — floor -2 under the pit (hole drop from hub)
      L4 — floor -2 west of hub (door + drop)
      L2 — floor -1 west of hub (door + stairs up + drop; same band as hub)
    """
    ec, er = end_slot
    hub_slot = (ec, er)
    wc, wr = neighbor_slot(ec, er, 'W')
    if not in_map(wc, wr):
        return

    mcx_hub, mcz_hub = module_center(*hub_slot)
    mcx_w, mcz_w = module_center(wc, wr)

    # Preserve the extraction module group for the hub shell (must differ from L2/L3/L4).
    hub_gid = max(
        (
            int(p['group_id'])
            for p in pieces_out
            if p.get('group_id') is not None
            and in_module_bounds(p, mcx_hub, mcz_hub)
            and int(p.get('floor_level', 0)) == 0
        ),
        default=9,
    )

    # Idempotent: strip prior branch placement before rebuilding.
    pieces_out[:] = [
        p for p in pieces_out if int(p.get("group_id", 0)) not in BRANCH_GIDS
    ]
    # Strip BOTH the hub band (-1) AND the depth band (-2) under the hub: L3 is now a
    # tiled room, so any leftover room-large shell at -2 here would be an invisible
    # double collider overlapping the tiled floor/walls.
    strip_module_floor_pieces(
        pieces_out, mcx_hub, mcz_hub, {HUB_FLOOR_LEVEL, DEPTH_FLOOR_LEVEL},
    )
    strip_module_floor_pieces(
        pieces_out, mcx_w, mcz_w, {HUB_FLOOR_LEVEL, DEPTH_FLOOR_LEVEL},
    )

    hole_x = mcx_hub + cell_cx(CENTER_TILE)
    hole_z = mcz_hub + cell_cz(CENTER_TILE)

    # Rebuild hub with a west opening toward the branch corridor.
    # Holes: west drop (SW corner, off the gate path) and L3 pit drop (iz=0);
    # landing tile at centre is solid. Both drop holes get a template-floor-hole frame.
    hub_holes = [(WEST_DROP_DX, WEST_DROP_DZ), (0.0, L3_DROP_DZ)]
    pieces_out.extend(
        build_branch_room(
            mcx_hub, mcz_hub, HUB_FLOOR_LEVEL, {'W'}, hub_gid,
            {'W': CENTER_TILE}, hub_holes, frame_holes=hub_holes,
        )
    )
    # Open west exit frame (no closed door panel).
    pieces_out.append({
        "stem": HUB_EXIT_GATE,
        "x": mcx_hub - 10.0,
        "z": mcz_hub,
        "yaw": PI32,
        "floor_level": HUB_FLOOR_LEVEL,
        "scale": 1.0,
        "group_id": hub_gid,
    })

    # L2 — stairs antechamber (floor -1, west module east opening).
    # StairUp (2,2)->(3,2): omit floor on both footprint tiles (see HUB_L2_STAIR).
    stair_holes = HUB_L2_STAIR.module_holes()
    pieces_out.extend(
        build_branch_room(
            mcx_w, mcz_w, HUB_FLOOR_LEVEL, {'E'}, GID_L2, {'E': CENTER_TILE}, stair_holes,
        )
    )
    append_branch_props(pieces_out, mcx_w, mcz_w, HUB_FLOOR_LEVEL, GID_L2)
    # Stairs piece (no floor contribution — single mesh, floor -2).
    pieces_out.append({
        "stem": "stairs",
        "x": mcx_w + STAIR_CELL_DX,
        "z": mcz_w,
        "yaw": PI2,
        "floor_level": DEPTH_FLOOR_LEVEL,
        "scale": 1.0,
        "group_id": GID_L2,
    })

    # L3 — pit drop target (floor -2, under extraction). All cells solid.
    pieces_out.extend(
        build_branch_room(mcx_hub, mcz_hub, DEPTH_FLOOR_LEVEL, set(), GID_L3),
    )
    append_branch_props(pieces_out, mcx_hub, mcz_hub, DEPTH_FLOOR_LEVEL, GID_L3)

    # L4 — west drop target (floor -2). Same stair footprint: no floor under stairs.
    pieces_out.extend(
        build_branch_room(
            mcx_w, mcz_w, DEPTH_FLOOR_LEVEL, {'E'}, GID_L4, {'E': CENTER_TILE},
            stair_holes,
        )
    )

    hub_key = str(HUB_FLOOR_LEVEL)
    depth_key = str(DEPTH_FLOOR_LEVEL)
    if hub_key not in floors:
        floors[hub_key] = {
            "cells_x": cells_total,
            "cells_z": cells_total,
            "cells": [False] * (cells_total * cells_total),
        }
    if depth_key not in floors:
        floors[depth_key] = {
            "cells_x": cells_total,
            "cells_z": cells_total,
            "cells": [False] * (cells_total * cells_total),
        }

    hub_mask = floors[hub_key]["cells"]
    depth_mask = floors[depth_key]["cells"]
    set_slot_floor_mask(hub_mask, hub_slot, cells_total)
    set_slot_floor_mask(hub_mask, (wc, wr), cells_total)
    set_slot_floor_mask(depth_mask, hub_slot, cells_total)
    set_slot_floor_mask(depth_mask, (wc, wr), cells_total)
    # Cut mask cells that match absent floor tiles (holes).
    cut_floor_cell(hub_mask, cells_total, mcx_hub + WEST_DROP_DX, mcz_hub + WEST_DROP_DZ)
    cut_floor_cell(hub_mask, cells_total, mcx_hub, mcz_hub + L3_DROP_DZ)
    # StairUp footprint on hub band and depth band (no template-floor under treads).
    for dx, dz in HUB_L2_STAIR.module_holes():
        cut_floor_cell(hub_mask, cells_total, mcx_w + dx, mcz_w + dz)
        cut_floor_cell(depth_mask, cells_total, mcx_w + dx, mcz_w + dz)


def apply_hub_exits_to_doc(doc: dict) -> bool:
    """Patch an existing map JSON with hub exit anchors (no embedded child maps)."""
    ex = doc.get("extraction_xz")
    if not ex:
        return False
    end_slot = slot_from_world(float(ex[0]), float(ex[1]))
    cells_total = int(doc.get("modules_x", MAP_MODULES)) * CELLS
    floors = doc.setdefault("floors", {})
    extraction = [float(ex[0]), float(ex[1])]
    doc["hub_exits"] = apply_hub_exits(end_slot, extraction, doc.setdefault("pieces", []), floors, cells_total)
    doc.pop("branch_levels", None)
    return True


def apply_hub_branches_to_doc(doc: dict) -> bool:
    """Legacy alias — patches hub exit anchors only."""
    return apply_hub_exits_to_doc(doc)


def export_branch_map_snippets(doc: dict) -> None:
    """Write per-branch JSON snippets for inspection (not separate runtime loads)."""
    branches = doc.get("branch_levels") or {}
    if not branches:
        return
    out_dir = MAP_DIR / "branches"
    out_dir.mkdir(parents=True, exist_ok=True)
    gids = {"2": GID_L2, "3": GID_L3, "4": GID_L4}
    for key, spec in branches.items():
        gid = gids.get(key)
        if gid is None:
            continue
        fl = int(spec["floor"])
        pieces = [
            p for p in doc.get("pieces", [])
            if int(p.get("group_id", 0)) == gid and int(p.get("floor_level", 0)) == fl
        ]
        snippet = {
            "label": spec.get("label"),
            "floor": fl,
            "centre_xz": [spec["x"], spec["z"]],
            "group_id": gid,
            "pieces": pieces,
        }
        path = out_dir / f"level_l{key}.json"
        path.write_text(json.dumps(snippet, indent=2) + "\n", encoding="utf-8")


def export_kenney_layout(doc: dict) -> None:
    """Write the runtime playtest layout consumed by the game client/server."""
    floors: Dict[str, dict] = {}
    for key, mask in doc.get('floors', {}).items():
        floors[str(key)] = mask

    layout = {
        "grid_unit_m": CELL_M,
        "modules_x": doc["modules_x"],
        "modules_z": doc["modules_z"],
        "floors": floors,
        "pieces": [
            {
                "stem": p["stem"],
                "x": p["x"],
                "z": p["z"],
                "yaw": p["yaw"],
                "floor": p.get("floor_level", 0),
                "scale": p.get("scale", 1.0),
                **({"group_id": p["group_id"]} if p.get("group_id") is not None else {}),
            }
            for p in doc["pieces"]
        ],
        "spawn_xz": doc.get("spawn_xz"),
        "extraction_xz": doc.get("extraction_xz"),
        "hub_exits": doc.get("hub_exits"),
        "branch_levels": doc.get("branch_levels"),
    }
    LAYOUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    LAYOUT_PATH.write_text(json.dumps(layout, indent=2) + "\n", encoding='utf-8')


def build_map_json(
    name: str,
    design: HighLevelDesign,
    placed: Dict[Slot, PlacedModule],
) -> dict:
    cells_total = MAP_MODULES * CELLS
    floor = [False] * (cells_total * cells_total)
    pieces_out: List[dict] = []
    group_id = 1

    for (col, row), pm in sorted(placed.items()):
        mcx, mcz = module_center(col, row)
        for p in pm.pieces:
            pieces_out.append({
                "stem": p.stem,
                "x": p.x + mcx,
                "z": p.z + mcz,
                "yaw": p.yaw,
                "floor_level": p.floor_level,
                "scale": p.scale,
                "group_id": group_id,
            })
        for iz in range(CELLS):
            for ix in range(CELLS):
                if not pm.floor_mask[iz * CELLS + ix]:
                    continue
                mix = col * CELLS + ix
                miz = row * CELLS + iz
                floor[miz * cells_total + mix] = True
        group_id += 1

    spawn = compute_spawn(design, placed)
    extraction, hub_floor = apply_extraction_and_hub(
        design, placed, floor, pieces_out, cells_total,
    )
    floors: Dict[str, dict] = {
        "0": {
            "cells_x": cells_total,
            "cells_z": cells_total,
            "cells": floor,
        },
        str(HUB_FLOOR_LEVEL): {
            "cells_x": cells_total,
            "cells_z": cells_total,
            "cells": hub_floor,
        },
    }
    hub_exits = apply_hub_exits(design.end, extraction, pieces_out, floors, cells_total)
    # Embed L2/L3/L4 branch content (strips and rebuilds hub, adds west-module rooms).
    apply_hub_branches(design.end, pieces_out, floors, cells_total)
    module_exits: Dict[str, Dict[str, List[int]]] = {}
    for slot, sides in design.connections.items():
        exp = probe.expected_border_openings(sides)
        module_exits[f"{slot[0]},{slot[1]}"] = {
            s: sorted(tiles) for s, tiles in exp.items() if tiles
        }
    return {
        "version": 1,
        "name": name,
        "modules_x": MAP_MODULES,
        "modules_z": MAP_MODULES,
        "floors": floors,
        "pieces": pieces_out,
        "spawn_xz": spawn,
        "extraction_xz": extraction,
        "hub_exits": hub_exits,
        "module_exits": module_exits,
    }


def audit_map(
    design: HighLevelDesign,
    placed: Dict[Slot, PlacedModule],
) -> List[str]:
    """
    Verify every painted connection has matching centre-tile openings on both
    modules, and every unrequired face is fully walled.
    """
    errors: List[str] = []

    for (col, row), pm in placed.items():
        slot = (col, row)
        required = design.connections.get(slot, set())
        opens = mesh_exits(pm.pieces)

        for side in required:
            if CENTER_TILE not in opens.get(side, set()):
                errors.append(
                    f"{slot} needs {side} connection but centre tile closed "
                    f"(open tiles: {opens.get(side, set())})"
                )

        for side in SIDES:
            if side in required:
                continue
            if opens.get(side):
                errors.append(f"{slot} leak: {side} open at tiles {opens[side]} (not in design)")

        # Map outer border must be sealed.
        if row == 0 and opens.get('N'):
            errors.append(f"{slot} leak: N faces void at tiles {opens['N']}")
        if row == MAP_MODULES - 1 and opens.get('S'):
            errors.append(f"{slot} leak: S faces void at tiles {opens['S']}")
        if col == 0 and opens.get('W'):
            errors.append(f"{slot} leak: W faces void at tiles {opens['W']}")
        if col == MAP_MODULES - 1 and opens.get('E'):
            errors.append(f"{slot} leak: E faces void at tiles {opens['E']}")

    for (col, row), pm in placed.items():
        for side in design.connections.get((col, row), set()):
            nc, nr = neighbor_slot(col, row, side)
            nslot = (nc, nr)
            if nslot not in placed:
                errors.append(f"{(col,row)} connects {side} but neighbour {nslot} missing")
                continue
            opp = OPPOSITE[side]
            a = mesh_exits(pm.pieces).get(side, set())
            b = mesh_exits(placed[nslot].pieces).get(opp, set())
            shared = a & b
            if not shared:
                errors.append(f"no shared tile {(col,row)}.{side} {a} <-> {nslot}.{opp} {b}")
            elif a != shared or b != shared:
                errors.append(
                    f"extra tiles {(col,row)}.{side} {a} <-> {nslot}.{opp} {b} (shared {shared})"
                )

    return errors


def validate_map(design: HighLevelDesign, placed: Dict[Slot, PlacedModule]) -> Tuple[bool, str]:
    if len(placed) != MAP_MODULES * MAP_MODULES:
        return False, f"only {len(placed)}/{MAP_MODULES * MAP_MODULES} modules placed"

    # Module-level connectivity BFS from spawn.
    seen: Set[Slot] = {design.spawn}
    q = deque([design.spawn])
    while q:
        slot = q.popleft()
        col, row = slot
        for side in design.connections.get(slot, set()):
            nc, nr = neighbor_slot(col, row, side)
            if not in_map(nc, nr):
                continue
            nxt = (nc, nr)
            if nxt in seen:
                continue
            seen.add(nxt)
            q.append(nxt)

    if len(seen) != MAP_MODULES * MAP_MODULES:
        return False, f"module graph reachability {len(seen)}/{MAP_MODULES * MAP_MODULES}"

    avg_deg = design.avg_degree()
    if abs(avg_deg - TARGET_AVG_DEGREE) > 0.6:
        return False, f"avg degree {avg_deg:.2f} far from target {TARGET_AVG_DEGREE}"

    errors = audit_map(design, placed)
    if errors:
        return False, f"{len(errors)} opening issues (first: {errors[0]})"

    return True, f"ok  avg_degree={avg_deg:.2f}  openings verified"


# ─── pool loading ────────────────────────────────────────────────────────────

def slot_from_world(x: float, z: float) -> Slot:
    """Assign a world position to the nearest module slot centre."""
    best: Optional[Slot] = None
    best_d = float('inf')
    for col in range(MAP_MODULES):
        for row in range(MAP_MODULES):
            cx, cz = module_center(col, row)
            d = (x - cx) ** 2 + (z - cz) ** 2
            if d < best_d:
                best_d = d
                best = (col, row)
    assert best is not None
    return best


def audit_map_file(path: Path) -> None:
    """Audit a saved map JSON (uses group_id when present, else nearest slot)."""
    data = json.loads(path.read_text(encoding='utf-8'))
    pieces_raw = data.get('pieces', [])
    by_gid: Dict[int, List[dict]] = {}
    for p in pieces_raw:
        gid = int(p.get('group_id', 0))
        by_gid.setdefault(gid, []).append(p)

    by_slot: Dict[Slot, List[PlacedPiece]] = {}
    for gid, group in by_gid.items():
        if not group:
            continue
        # Slot = nearest centre to group centroid.
        ax = sum(float(p['x']) for p in group) / len(group)
        az = sum(float(p['z']) for p in group) / len(group)
        slot = slot_from_world(ax, az)
        mcx, mcz = module_center(*slot)
        local = [
            PlacedPiece(
                stem=p['stem'], x=float(p['x']) - mcx, z=float(p['z']) - mcz,
                yaw=float(p['yaw']), floor_level=int(p.get('floor_level', 0)),
                scale=float(p.get('scale', 1.0)),
            )
            for p in group
        ]
        by_slot[slot] = local

    print(f"Auditing {path} — {len(by_slot)} module slots with pieces")
    errors: List[str] = []

    for slot in sorted(by_slot):
        opens = mesh_exits(by_slot[slot])
        for side, tiles in opens.items():
            if not tiles:
                continue
            col, row = slot
            nc, nr = neighbor_slot(col, row, side)
            if not in_map(nc, nr):
                errors.append(f"{slot} leak: {side} open {tiles} (map edge)")
                continue
            nopens = mesh_exits(by_slot.get((nc, nr), []))
            opp = OPPOSITE[side]
            ntiles = nopens.get(opp, set())
            shared = tiles & ntiles
            if not shared:
                errors.append(
                    f"MISMATCH {slot}.{side} tiles {tiles} <-> {(nc,nr)}.{opp} tiles {ntiles}"
                )
            elif CENTER_TILE in tiles | ntiles and CENTER_TILE not in shared:
                errors.append(
                    f"OFF-CENTRE {slot}.{side} {tiles} <-> {(nc,nr)}.{opp} {ntiles}"
                )

    if not errors:
        print("  No opening mismatches or leaks detected.")
    else:
        print(f"  {len(errors)} issue(s):")
        for e in errors:
            print(f"    · {e}")


def load_pool(pool_name: str, min_score: float, corridors_only: bool = True) -> List[ModuleVariant]:
    pool_dir = Path("userinput/modules") / pool_name
    index_path = pool_dir / "gen_index.json"
    allowed: Optional[Set[str]] = None
    side_hints: Dict[str, Set[str]] = {}
    if index_path.exists():
        meta = json.loads(index_path.read_text(encoding='utf-8'))
        allowed = {e['name'] for e in meta if e.get('score', 0) >= min_score}
        for e in meta:
            if e.get('score', 0) >= min_score and e.get('sides'):
                side_hints[e['name']] = set(e['sides'])

    variants: List[ModuleVariant] = []
    for path in sorted(pool_dir.glob("*.json")):
        if path.name in ('gen_index.json',):
            continue
        try:
            data = json.loads(path.read_text(encoding='utf-8'))
        except (json.JSONDecodeError, OSError):
            continue
        name = data.get('name', path.stem)
        if allowed is not None and name not in allowed:
            continue
        stems = {p.get('stem', '') for p in data.get('pieces', [])}
        if corridors_only and any(s.startswith('room-') for s in stems):
            continue
        hint = side_hints.get(name)
        variants.extend(build_variants(path, data, hint))

    return variants


def export_pool_index(entries: List[dict]) -> None:
    """Write pool manifest consumed by the game runtime."""
    POOL_DIR.mkdir(parents=True, exist_ok=True)
    manifest = {
        "version": 1,
        "modules": MAP_MODULES,
        "grid_unit_m": CELL_M,
        "start_id": entries[0]["id"] if entries else None,
        "maps": entries,
    }
    (POOL_DIR / "index.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


def try_generate_map(
    rng: random.Random,
    pool: List[ModuleVariant],
    max_attempts: int,
) -> Optional[Tuple[HighLevelDesign, Dict[Slot, PlacedModule], dict]]:
    """Try up to `max_attempts` designs; return (design, placed, doc) on success."""
    for attempt in range(1, max_attempts + 1):
        design = design_high_level(rng)
        state = PlacementState(design=design, pool=pool, rng=rng)
        if not state.try_place_all():
            continue
        ok, _msg = validate_map(design, state.placed)
        if not ok:
            continue
        doc = build_map_json(f"gen_{attempt}", design, state.placed)
        return design, state.placed, doc
    return None


def generate_pool_batch(
    count: int,
    rng: random.Random,
    pool: List[ModuleVariant],
    attempts_per_map: int,
) -> List[dict]:
    """Generate `count` full maps into userinput/maps/pool/."""
    POOL_DIR.mkdir(parents=True, exist_ok=True)
    entries: List[dict] = []
    for i in range(1, count + 1):
        map_id = f"map_{i:03d}"
        print(f"Generating {map_id}…")
        result = try_generate_map(rng, pool, attempts_per_map)
        if result is None:
            print(f"  FAILED to generate {map_id} in {attempts_per_map} attempts")
            continue
        design, _placed, doc = result
        doc["name"] = map_id
        doc["pool_id"] = map_id
        rel = f"pool/{map_id}.json"
        out_path = MAP_DIR / rel
        out_path.write_text(json.dumps(doc, indent=2) + "\n", encoding="utf-8")
        entries.append({
            "id": map_id,
            "path": rel.replace("\\", "/"),
            "spawn_xz": doc["spawn_xz"],
            "extraction_xz": doc.get("extraction_xz"),
            "hub_exits": doc.get("hub_exits"),
            "modules_x": doc["modules_x"],
            "modules_z": doc["modules_z"],
        })
        print(f"  ok  spawn={design.spawn}  end={design.end}  pieces={len(doc['pieces'])}")
    export_pool_index(entries)
    if entries:
        export_kenney_layout(json.loads((MAP_DIR / entries[0]["path"]).read_text(encoding="utf-8")))
    return entries


# ─── main ────────────────────────────────────────────────────────────────────

def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--pool', default='space_rooms',
                    help='Module pool (default: space_rooms — verified room-large modules)')
    ap.add_argument('--min-score', type=float, default=0.85,
                    help='Minimum module score from gen_index.json (default: 0.85)')
    ap.add_argument('--seed', type=int, default=None)
    ap.add_argument('--pool-batch', type=int, default=None, metavar='N',
                    help='Generate N full 5×5 maps into userinput/maps/pool/')
    ap.add_argument('--attempts', type=int, default=50,
                    help='Max high-level designs to try per map (default: 50)')
    ap.add_argument('--out', default=None, help='Output map path')
    ap.add_argument('--corridors-only', action='store_true',
                    help='Exclude room-* GLBs (only for generated pool; limits junction variety)')
    ap.add_argument('--probe', action='store_true',
                    help='After generation, run catalog mesh probe on output')
    ap.add_argument('--audit', default=None, metavar='MAP.json',
                    help='Audit an existing map (re-derive slots from piece positions)')
    ap.add_argument('--add-branches', default=None, metavar='MAP.json',
                    help='Add embedded L2/L3/L4 branch levels to an existing map and export layout')
    args = ap.parse_args()

    if args.audit:
        audit_map_file(Path(args.audit))
        return

    if args.add_branches:
        path = Path(args.add_branches)
        doc = json.loads(path.read_text(encoding='utf-8'))
        if not apply_hub_branches_to_doc(doc):
            print(f"No extraction_xz in {path} — cannot place branch levels.")
            sys.exit(1)
        path.write_text(json.dumps(doc, indent=2) + "\n", encoding='utf-8')
        export_kenney_layout(doc)
        export_branch_map_snippets(doc)
        print(f"Patched {path} with hub exit anchors")
        print(f"  Exported {LAYOUT_PATH}")
        return

    rng = random.Random(args.seed)
    pool = load_pool(args.pool, args.min_score, corridors_only=args.corridors_only)
    if not pool:
        print(f"No modules found in userinput/modules/{args.pool} (min_score={args.min_score})")
        sys.exit(1)

    print(f"Loaded {len(pool)} module variants from pool '{args.pool}'")

    if args.pool_batch:
        entries = generate_pool_batch(args.pool_batch, rng, pool, args.attempts)
        if not entries:
            print("Pool batch generation failed — no maps produced.")
            sys.exit(1)
        print(f"Wrote {len(entries)} maps to {POOL_DIR.resolve()}")
        print(f"  manifest: {(POOL_DIR / 'index.json').resolve()}")
        print(f"  playtest layout: {LAYOUT_PATH.resolve()} (first pool map)")
        return

    MAP_DIR.mkdir(parents=True, exist_ok=True)
    t0 = time.time()

    for attempt in range(1, args.attempts + 1):
        design = design_high_level(rng)
        state = PlacementState(design=design, pool=pool, rng=rng)

        if not state.try_place_all():
            print(f"  attempt {attempt}: placement failed  "
                  f"(spawn={design.spawn} end={design.end} avg_deg={design.avg_degree():.2f})")
            continue

        ok, msg = validate_map(design, state.placed)
        if not ok:
            issues = audit_map(design, state.placed)
            print(f"  attempt {attempt}: validation failed – {msg}")
            for line in issues[:12]:
                print(f"    · {line}")
            if len(issues) > 12:
                print(f"    · … and {len(issues) - 12} more")
            continue

        ts = time.strftime("%m%d%H%M%S")
        out_path = Path(args.out) if args.out else MAP_DIR / f"gen_map_{ts}.json"
        name = out_path.stem
        doc = build_map_json(name, design, state.placed)
        out_path.write_text(json.dumps(doc, indent=2), encoding='utf-8')
        export_kenney_layout(doc)

        elapsed = time.time() - t0
        print(f"Generated map in {elapsed:.1f}s (attempt {attempt})")
        print(f"  spawn={design.spawn}  end={design.end}  {msg}")
        print(f"  high-level paint (S=spawn E=end, digit=connection count):\n{design.ascii()}")
        print(f"  wrote {out_path.resolve()}")
        print(f"  exported {LAYOUT_PATH.resolve()}")
        print(f"  pieces={len(doc['pieces'])}  spawn_xz={doc['spawn_xz']}"
              f"  extraction_xz={doc['extraction_xz']}")
        if args.probe:
            issues = probe.probe_map(out_path, verbose=False)
            if issues:
                print(f"  probe: {len(issues)} mismatch(es) — run probe_map_geometry.py -v")
                for line in issues[:8]:
                    print(f"    · {line}")
            else:
                print("  probe: OK (mesh borders match expected + adjacent)")
        return

    print(f"Failed to generate a valid map in {args.attempts} attempts.")
    sys.exit(1)


if __name__ == '__main__':
    main()
