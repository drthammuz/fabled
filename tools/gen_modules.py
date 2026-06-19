#!/usr/bin/env python3
"""
Fabled – overnight module generator for the Kenney Space Kit.

Runs generation strategies, scores every candidate with a BFS-based fitness
function, keeps only the best ones, and writes them to a pool directory.

Graceful exit options (checked every iteration):
  1. Type  q  + Enter   in this terminal
  2. Create file:  userinput/gen_stop.flag

Usage:
    python tools/gen_modules.py
    python tools/gen_modules.py --pool generated --hours 8 --target 400 --seed 42

Output:
    userinput/modules/<pool>/gen_XXXX_s<score>.json   – accepted modules
    userinput/modules/<pool>/gen_index.json            – index with scores+metadata
    userinput/modules/<pool>/gen_log.txt               – timestamped run log
"""

import argparse
import json
import math
import os
import random
import sys
import threading
import time
from collections import deque
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Set, Tuple

# ─── constants ────────────────────────────────────────────────────────────────

PI   = math.pi
PI2  = PI / 2
PI32 = 3 * PI / 2

CELLS      = 5       # cells per module side
CELL_M     = 4.0     # metres per cell
MODULE_M   = CELLS * CELL_M  # 20 m per module side
HALF_M     = MODULE_M / 2    # 10 m  (distance from centre to boundary)

SIDES      = ['N', 'S', 'E', 'W']
OPPOSITE   = {'N': 'S', 'S': 'N', 'E': 'W', 'W': 'E'}
DELTA      = {'N': (0, -1), 'S': (0, 1), 'E': (1, 0), 'W': (-1, 0)}


def cell_cx(ix: int) -> float:
    """World X of cell (ix) centre, in module-local coords."""
    return -8.0 + ix * 4.0


def cell_cz(iz: int) -> float:
    """World Z of cell (iz) centre."""
    return -8.0 + iz * 4.0


def gate_on_boundary(side: str, tile: int = 2) -> Tuple[float, float, float]:
    """Gate on the module outer wall plane (±10 m), not the door-tile centre (±8 m).

    Matches hub west exit placement in ``gen_maps.apply_hub_exits`` (``mcx - 10``).
    ``tile`` is the boundary door cell index (0–4) on that face — usually ``2`` (centre).
    """
    half_cell = CELL_M * 0.5
    if side == 'W':
        return (cell_cx(0) - half_cell, cell_cz(tile), PI32)
    if side == 'E':
        return (cell_cx(CELLS - 1) + half_cell, cell_cz(tile), PI32)
    if side == 'N':
        return (cell_cx(tile), cell_cz(0) - half_cell, 0.0)
    if side == 'S':
        return (cell_cx(tile), cell_cz(CELLS - 1) + half_cell, 0.0)
    raise ValueError(f"unknown side {side!r}")


# ─── piece catalogue ──────────────────────────────────────────────────────────

@dataclass(frozen=True)
class PieceDef:
    stem: str
    nx: int               # width in cells (at yaw=0)
    nz: int               # depth in cells (at yaw=0)
    open_local: frozenset  # open faces in LOCAL frame at yaw=0  {N,S,E,W}
    is_structural: bool   # True = has built-in floor+walls; False = trim/deco


def _fs(*sides): return frozenset(sides)


PIECE_DB: Dict[str, PieceDef] = {
    # ── full rooms ────────────────────────────────────────────────────────────
    'room-large':             PieceDef('room-large',             5, 5, _fs('N','S','E','W'), True),
    'room-large-variation':   PieceDef('room-large-variation',   5, 5, _fs('N','S','E','W'), True),
    'room-small':             PieceDef('room-small',             3, 3, _fs('N','S','E','W'), True),
    'room-small-variation':   PieceDef('room-small-variation',   3, 3, _fs('N','S','E','W'), True),
    'room-wide':              PieceDef('room-wide',              5, 3, _fs('N','S','E','W'), True),
    'room-wide-variation':    PieceDef('room-wide-variation',    5, 3, _fs('N','S','E','W'), True),
    'room-corner':            PieceDef('room-corner',            3, 3, _fs('S','E'),         True),
    # ── narrow corridors (1×1) ───────────────────────────────────────────────
    'corridor':               PieceDef('corridor',               1, 1, _fs('N','S'),         True),
    'corridor-corner':        PieceDef('corridor-corner',        1, 1, _fs('S','E'),         True),
    'corridor-junction':      PieceDef('corridor-junction',      1, 1, _fs('N','S','E'),     True),
    'corridor-intersection':  PieceDef('corridor-intersection',  1, 1, _fs('N','S','E','W'), True),
    'corridor-end':           PieceDef('corridor-end',           1, 1, _fs('N'),             True),
    'corridor-transition':    PieceDef('corridor-transition',    1, 1, _fs('N','S'),         True),
    # ── wide corridors (2×2) ─────────────────────────────────────────────────
    'corridor-wide':          PieceDef('corridor-wide',          2, 2, _fs('N','S'),         True),
    'corridor-wide-corner':   PieceDef('corridor-wide-corner',   2, 2, _fs('S','E'),         True),
    # ── stairs ───────────────────────────────────────────────────────────────
    'stairs-wide':            PieceDef('stairs-wide',            3, 3, _fs('N','S'),         True),
    # ── door frames (1×1, structural opening) ────────────────────────────────
    'gate':                   PieceDef('gate',                   1, 1, _fs('N','S'),         True),
    'gate-door-window':       PieceDef('gate-door-window',       1, 1, _fs('N','S'),         True),
    # ── decoration / trim (no walls) ─────────────────────────────────────────
    'template-floor':         PieceDef('template-floor',         1, 1, _fs(),                False),
    'template-floor-big':     PieceDef('template-floor-big',     2, 2, _fs(),                False),
    'template-floor-detail-a':PieceDef('template-floor-detail-a',1, 1, _fs(),                False),
    'template-wall':          PieceDef('template-wall',          1, 1, _fs(),                False),
    'template-wall-top':      PieceDef('template-wall-top',      1, 1, _fs(),                False),
    'template-wall-corner':   PieceDef('template-wall-corner',   1, 1, _fs(),                False),
    'template-wall-detail-a': PieceDef('template-wall-detail-a', 1, 1, _fs(),                False),
    'cables':                 PieceDef('cables',                 1, 1, _fs(),                False),
}


def rotated_faces(faces: frozenset, yaw_rad: float) -> frozenset:
    """Rotate a set of face names by yaw (CCW 90° each step)."""
    steps = round(yaw_rad / PI2) % 4
    cycle = ['N', 'W', 'S', 'E']   # each CCW 90° step: N→W, W→S …
    return frozenset(cycle[(cycle.index(f) + steps) % 4] for f in faces)


def rotated_dims(nx: int, nz: int, yaw_rad: float) -> Tuple[int, int]:
    """Swap nx/nz when rotation is 90° or 270°."""
    return (nz, nx) if round(yaw_rad / PI2) % 2 else (nx, nz)


# ─── cell grid (5×5) ─────────────────────────────────────────────────────────

@dataclass
class Cell:
    walkable: bool = False
    # Faces through which a player can pass (to adjacent cell or module exterior)
    open_faces: Set[str] = field(default_factory=set)


class CellGrid:
    def __init__(self):
        # cells[iz][ix]
        self.cells: List[List[Cell]] = [
            [Cell() for _ in range(CELLS)] for _ in range(CELLS)
        ]

    def c(self, ix: int, iz: int) -> Cell:
        return self.cells[iz][ix]

    def in_bounds(self, ix: int, iz: int) -> bool:
        return 0 <= ix < CELLS and 0 <= iz < CELLS

    # ── fill a rectangular zone as one open room ──────────────────────────────
    def fill_room(self, cell_ox: int, cell_oz: int, nx: int, nz: int,
                  entrance_faces: Set[str], yaw: float = 0.0):
        """
        Mark a rectangular block of cells as walkable with full internal
        connectivity.  entrance_faces are the external faces of the room
        that connect to the module exterior/other pieces.
        """
        actual_nx, actual_nz = rotated_dims(nx, nz, yaw)

        for diz in range(actual_nz):
            for dix in range(actual_nx):
                ix = cell_ox + dix
                iz = cell_oz + diz
                if not self.in_bounds(ix, iz):
                    continue
                cell = self.c(ix, iz)
                cell.walkable = True
                # internal connectivity: open all faces that lead to another
                # cell within this rectangle
                for face, (ddx, ddz) in DELTA.items():
                    nix, niz = dix + ddx, diz + ddz
                    if 0 <= nix < actual_nx and 0 <= niz < actual_nz:
                        cell.open_faces.add(face)

        # External-facing cells get their outward face opened for entrances
        # entrance_faces is in world/rotated frame
        face_to_boundary_cell = {
            'N': (cell_ox + actual_nx // 2, cell_oz),
            'S': (cell_ox + actual_nx // 2, cell_oz + actual_nz - 1),
            'E': (cell_ox + actual_nx - 1,  cell_oz + actual_nz // 2),
            'W': (cell_ox,                  cell_oz + actual_nz // 2),
        }
        for face in entrance_faces:
            ix, iz = face_to_boundary_cell.get(face, (-1, -1))
            if self.in_bounds(ix, iz):
                self.c(ix, iz).open_faces.add(face)

    # ── single-cell corridor piece ────────────────────────────────────────────
    def fill_corridor_cell(self, ix: int, iz: int, open_a: str, open_b: str):
        """Mark one cell as walkable with exactly two open faces."""
        if not self.in_bounds(ix, iz):
            return
        cell = self.c(ix, iz)
        cell.walkable = True
        cell.open_faces.update({open_a, open_b})

    # ── pathfinding ──────────────────────────────────────────────────────────
    def bfs_from(self, start_ix: int, start_iz: int) -> Set[Tuple[int, int]]:
        if not self.in_bounds(start_ix, start_iz):
            return set()
        visited: Set[Tuple[int, int]] = {(start_ix, start_iz)}
        q = deque([(start_ix, start_iz)])
        while q:
            ix, iz = q.popleft()
            cell = self.c(ix, iz)
            for face, (dx, dz) in DELTA.items():
                nix, niz = ix + dx, iz + dz
                if (nix, niz) in visited or not self.in_bounds(nix, niz):
                    continue
                ncell = self.c(nix, niz)
                if ncell.walkable and face in cell.open_faces and OPPOSITE[face] in ncell.open_faces:
                    visited.add((nix, niz))
                    q.append((nix, niz))
        return visited

    # ── derived metrics ───────────────────────────────────────────────────────
    def walkable_count(self) -> int:
        return sum(c.walkable for row in self.cells for c in row)

    def entrances(self) -> List[Tuple[int, int, str]]:
        """(ix, iz, face) for every boundary cell that opens outward."""
        result = []
        for ix in range(CELLS):
            if self.c(ix, 0).walkable and 'N' in self.c(ix, 0).open_faces:
                result.append((ix, 0, 'N'))
            if self.c(ix, CELLS-1).walkable and 'S' in self.c(ix, CELLS-1).open_faces:
                result.append((ix, CELLS-1, 'S'))
        for iz in range(CELLS):
            if self.c(0, iz).walkable and 'W' in self.c(0, iz).open_faces:
                result.append((0, iz, 'W'))
            if self.c(CELLS-1, iz).walkable and 'E' in self.c(CELLS-1, iz).open_faces:
                result.append((CELLS-1, iz, 'E'))
        return result


# ─── module data ─────────────────────────────────────────────────────────────

@dataclass
class PlacedPiece:
    stem:  str
    x:     float   # module-local centre X
    z:     float   # module-local centre Z
    yaw:   float = 0.0
    floor_level: int = 0
    scale: float = 1.0


@dataclass
class Module:
    name:     str = ''
    pool:     str = ''
    pieces:   List[PlacedPiece] = field(default_factory=list)
    grid:     CellGrid          = field(default_factory=CellGrid)
    score:    float = 0.0
    scores:   Dict[str, float]  = field(default_factory=dict)
    strategy: str = ''
    gen_idx:  int = 0

    def to_json(self) -> dict:
        doc = {
            "version": 1,
            "name":    self.name,
            "pool":    self.pool,
            "floor_mask": {"cells_x": 5, "cells_z": 5, "cells": [True]*25},
            "extra_floor_masks": {},
            "pieces": [
                {"stem": p.stem, "x": p.x, "z": p.z,
                 "yaw": p.yaw, "floor_level": p.floor_level, "scale": p.scale}
                for p in self.pieces
            ],
        }
        # Mesh-probed exits: what is actually walkable through each border face.
        try:
            import probe_map_geometry as mesh_probe  # noqa: WPS433
            exits = mesh_probe.border_exits_for_pieces(self.pieces)
            doc["border_exits"] = mesh_probe.border_exits_to_json(exits)
        except Exception:
            pass
        return doc

    def mesh_entrances(self) -> List[Tuple[int, int, str]]:
        """(ix or iz, unused, side) style entrances from mesh border_exits."""
        try:
            import probe_map_geometry as mesh_probe  # noqa: WPS433
            exits = mesh_probe.border_exits_for_pieces(self.pieces)
        except Exception:
            return self.grid.entrances()
        out: List[Tuple[int, int, str]] = []
        for side in SIDES:
            for tile in sorted(exits.get(side, set())):
                if side in ('N', 'S'):
                    iz = 0 if side == 'N' else CELLS - 1
                    out.append((tile, iz, side))
                else:
                    ix = 0 if side == 'W' else CELLS - 1
                    out.append((ix, tile, side))
        return out

    def meta(self) -> dict:
        ent = self.mesh_entrances()
        mesh_sides = sorted({f for _, _, f in ent})
        structural = sum(1 for p in self.pieces
                         if PIECE_DB.get(p.stem, PieceDef('',0,0,_fs(),False)).is_structural)
        return {
            "name":       self.name,
            "score":      round(self.score, 4),
            "breakdown":  {k: round(v, 4) for k, v in self.scores.items()},
            "strategy":   self.strategy,
            "gen_idx":    self.gen_idx,
            "entrances":  len(ent),
            "sides":      mesh_sides,
            "total_pieces":      len(self.pieces),
            "structural_pieces": structural,
        }


# ─── scoring ─────────────────────────────────────────────────────────────────

def score_module(mod: Module) -> float:
    grid = mod.grid
    entrances  = grid.entrances()
    n_ent      = len(entrances)
    n_walkable = grid.walkable_count()

    s = {}

    # 1. entrance count  – sweet spot 2-3; wide pieces create 2 cells per side
    #    so 4 can mean "2 proper sides" rather than "chaotic junction" → don't penalise
    ent_table = {0: 0.0, 1: 0.40, 2: 1.00, 3: 0.90, 4: 0.72, 5: 0.50, 6: 0.35}
    s['entrances'] = ent_table.get(n_ent, 0.20)

    # 2. accessibility – average fraction of walkable cells reachable per entrance
    if n_ent == 0 or n_walkable == 0:
        s['accessibility'] = 0.0
    else:
        fracs = []
        for (ix, iz, _) in entrances:
            reached = grid.bfs_from(ix, iz)
            fracs.append(len(reached) / n_walkable)
        s['accessibility'] = sum(fracs) / len(fracs)

    # 3. connectivity – every entrance can reach every other entrance
    if n_ent < 2:
        s['connectivity'] = 1.0
    else:
        ix0, iz0, _ = entrances[0]
        r0 = grid.bfs_from(ix0, iz0)
        all_reach = all((ix, iz) in r0 for ix, iz, _ in entrances[1:])
        s['connectivity'] = 1.0 if all_reach else 0.0

    # 4. safety – no boundary walkable cell has an outward face except at entrances
    entrance_set = {(ix, iz) for ix, iz, _ in entrances}
    unsafe = 0
    for ix in range(CELLS):
        if grid.c(ix, 0).walkable and 'N' in grid.c(ix, 0).open_faces \
                and (ix, 0) not in entrance_set:
            unsafe += 1
        if grid.c(ix, CELLS-1).walkable and 'S' in grid.c(ix, CELLS-1).open_faces \
                and (ix, CELLS-1) not in entrance_set:
            unsafe += 1
    for iz in range(CELLS):
        if grid.c(0, iz).walkable and 'W' in grid.c(0, iz).open_faces \
                and (0, iz) not in entrance_set:
            unsafe += 1
        if grid.c(CELLS-1, iz).walkable and 'E' in grid.c(CELLS-1, iz).open_faces \
                and (CELLS-1, iz) not in entrance_set:
            unsafe += 1
    s['safety'] = max(0.0, 1.0 - unsafe * 0.4)

    # 5. coverage – fraction of 25 cells that are walkable (0.3–0.85 is interesting)
    cov = n_walkable / 25.0
    if cov < 0.15:
        s['coverage'] = 0.2
    elif cov > 0.95:
        s['coverage'] = 0.6   # just a plain room-large
    else:
        s['coverage'] = min(1.0, cov / 0.6)

    # 6. variety – distinct structural piece types; partial credit for many same-type pieces
    struct_pieces = [p for p in mod.pieces
                     if PIECE_DB.get(p.stem, PieceDef('',0,0,_fs(),False)).is_structural]
    structural_stems = {p.stem for p in struct_pieces}
    n_uniq = len(structural_stems)
    if n_uniq >= 2:
        s['variety'] = min(1.0, (n_uniq - 1) / 3.0)
    elif len(struct_pieces) >= 3:
        # Multiple pieces of the same type (e.g. four_rooms) — give partial credit
        s['variety'] = 0.25
    else:
        s['variety'] = 0.0

    # 7. not-trivial – penalise "room-large + walls only" designs
    has_big_room   = any('room-large' in p.stem for p in mod.pieces)
    has_other_room = any(p.stem not in ('room-large','room-large-variation','template-wall',
                                        'template-floor','template-wall-detail-a')
                         and PIECE_DB.get(p.stem, PieceDef('',0,0,_fs(),False)).is_structural
                         for p in mod.pieces)
    if has_other_room:
        s['creativity'] = 1.0
    elif has_big_room:
        s['creativity'] = 0.35
    else:
        s['creativity'] = 0.70

    # 8. efficiency – prefer fewer pure-tile pieces relative to structural count
    n_tiles = sum(1 for p in mod.pieces
                  if not PIECE_DB.get(p.stem, PieceDef('',0,0,_fs(),False)).is_structural)
    n_struct = sum(1 for p in mod.pieces
                   if PIECE_DB.get(p.stem, PieceDef('',0,0,_fs(),False)).is_structural)
    ratio = n_tiles / max(n_struct + n_tiles, 1)
    s['efficiency'] = max(0.0, 1.0 - ratio * 0.6)

    WEIGHTS = {
        'entrances':    0.18,
        'accessibility':0.22,
        'connectivity': 0.18,
        'safety':       0.15,
        'coverage':     0.07,
        'variety':      0.08,
        'creativity':   0.08,
        'efficiency':   0.04,
    }
    total = sum(s[k] * WEIGHTS[k] for k in WEIGHTS)
    mod.score  = total
    mod.scores = s
    return total


# ─── piece helpers ───────────────────────────────────────────────────────────

def pp(stem: str, x: float, z: float, yaw: float = 0.0) -> PlacedPiece:
    return PlacedPiece(stem=stem, x=float(x), z=float(z), yaw=float(yaw))


# Closing walls at each module boundary opening
def close_N(): return pp('template-wall',  0.0, -10.0, PI)
def close_S(): return pp('template-wall',  0.0,  10.0, 0.0)
def close_E(): return pp('template-wall',  10.0,  0.0, PI2)
def close_W(): return pp('template-wall', -10.0,  0.0, PI32)

CLOSE = {'N': close_N, 'S': close_S, 'E': close_E, 'W': close_W}


# ─── generation strategies ────────────────────────────────────────────────────

def strat_room_large(rng: random.Random, open_sides: Set[str]) -> Tuple[List[PlacedPiece], CellGrid]:
    """5×5 room — clean, no clutter.  One piece, all entrances opened directly."""
    stem = rng.choice(['room-large', 'room-large-variation'])
    pieces = [pp(stem, 0.0, 0.0)]
    grid = CellGrid()
    grid.fill_room(0, 0, 5, 5, open_sides)
    return pieces, grid


def strat_corner_room(rng: random.Random, open_sides: Set[str]) -> Tuple[List[PlacedPiece], CellGrid]:
    """
    One room-corner piece placed at a module corner, giving a clean L-shaped room.
    Corridors bridge any entrances on sides the corner doesn't cover directly.
    """
    # (cell_ox, cell_oz, yaw, faces_the_room_opens_at_module_boundary)
    corner_configs = [
        (0, 0, PI,   frozenset({'N', 'W'})),   # top-left
        (2, 0, PI2,  frozenset({'N', 'E'})),   # top-right
        (2, 2, 0.0,  frozenset({'S', 'E'})),   # bottom-right
        (0, 2, PI32, frozenset({'S', 'W'})),   # bottom-left
    ]
    # Prefer configs whose native exits overlap with open_sides
    best = max(corner_configs, key=lambda c: len(c[3] & open_sides))
    ox, oz, yaw, room_exits = best

    stem = rng.choice(['room-corner'])
    pieces = [pp(stem, cell_cx(ox + 1), cell_cz(oz + 1), yaw)]

    grid = CellGrid()
    grid.fill_room(ox, oz, 3, 3, room_exits & open_sides)

    # For requested sides the room doesn't cover natively, add corridor(s)
    for side in open_sides - room_exits:
        dx, dz = DELTA[side]
        # Middle cell of the room's face on this side
        if side == 'N':
            rix, riz = ox + 1, oz
        elif side == 'S':
            rix, riz = ox + 1, oz + 2
        elif side == 'E':
            rix, riz = ox + 2, oz + 1
        else:
            rix, riz = ox, oz + 1

        grid.c(rix, riz).open_faces.add(side)
        yaw_c = PI32 if side in ('N', 'S') else 0.0
        cix, ciz = rix + dx, riz + dz
        while grid.in_bounds(cix, ciz) and not grid.c(cix, ciz).walkable:
            grid.fill_corridor_cell(cix, ciz, side, OPPOSITE[side])
            pieces.append(pp('corridor', cell_cx(cix), cell_cz(ciz), yaw_c))
            cix += dx
            ciz += dz

    return pieces, grid


def strat_two_corners(rng: random.Random, open_sides: Set[str]) -> Tuple[List[PlacedPiece], CellGrid]:
    """
    Two room-corner pieces at opposite module corners connected by a central
    corridor.  Creates an S- or Z-shaped architectural space.
    """
    # Diagonal pairs: top-left + bottom-right, or top-right + bottom-left
    diagonal_pairs = [
        ((0, 0, PI,   frozenset({'N', 'W'})),
         (2, 2, 0.0,  frozenset({'S', 'E'}))),
        ((2, 0, PI2,  frozenset({'N', 'E'})),
         (0, 2, PI32, frozenset({'S', 'W'}))),
    ]
    pair = rng.choice(diagonal_pairs)

    grid = CellGrid()
    pieces = []
    for ox, oz, yaw, exits in pair:
        stem = rng.choice(['room-corner'])
        pieces.append(pp(stem, cell_cx(ox + 1), cell_cz(oz + 1), yaw))
        grid.fill_room(ox, oz, 3, 3, exits & open_sides)

    # Connect both rooms through a central corridor cell at (2,2)
    grid.fill_corridor_cell(2, 2, 'N', 'S')
    grid.c(2, 2).open_faces.update({'E', 'W'})
    pieces.append(pp('corridor-intersection', cell_cx(2), cell_cz(2)))

    # Connect the two rooms to the centre
    for (ox, oz, yaw, _) in pair:
        mid_ix, mid_iz = ox + 1, oz + 1  # centre of 3x3
        for face, (fdx, fdz) in DELTA.items():
            nix, niz = mid_ix + fdx, mid_iz + fdz
            if grid.in_bounds(nix, niz) and grid.c(nix, niz).walkable:
                grid.c(mid_ix, mid_iz).open_faces.add(face)
                grid.c(nix, niz).open_faces.add(OPPOSITE[face])

    # Boundary entrances for any open_sides not yet reachable
    for side in open_sides:
        dx, dz = DELTA[side]
        if side == 'N':   bnd = [(ix, 0) for ix in range(CELLS)]
        elif side == 'S': bnd = [(ix, CELLS-1) for ix in range(CELLS)]
        elif side == 'E': bnd = [(CELLS-1, iz) for iz in range(CELLS)]
        else:             bnd = [(0, iz) for iz in range(CELLS)]
        for bix, biz in bnd:
            if grid.c(bix, biz).walkable:
                grid.c(bix, biz).open_faces.add(side)
                break
            # trace inward one cell to see if we can reach walkable
            wix, wiz = bix + (-dx), biz + (-dz)
            if grid.in_bounds(wix, wiz) and grid.c(wix, wiz).walkable:
                grid.fill_corridor_cell(bix, biz, side, OPPOSITE[side])
                yaw_c = PI32 if side in ('N', 'S') else 0.0
                pieces.append(pp('corridor', cell_cx(bix), cell_cz(biz), yaw_c))
                grid.c(bix, biz).open_faces.add(side)
                break

    return pieces, grid


def strat_alcove(rng: random.Random, open_sides: Set[str]) -> Tuple[List[PlacedPiece], CellGrid]:
    """
    A room-wide (5×3) as the main space with a room-small attached as an alcove
    on one end.  Creates asymmetric rooms with a clear main hall + side pocket.
    """
    grid = CellGrid()
    pieces = []

    # Place room-wide in one half (N or S)
    half = rng.choice(['north', 'south'])
    wide_stem = rng.choice(['room-wide', 'room-wide-variation'])

    if half == 'north':
        wox, woz = 0, 0        # cells (0-4, 0-2)
        alcove_oz = 2           # alcove sits at rows 2-4
    else:
        wox, woz = 0, 2        # cells (0-4, 2-4)
        alcove_oz = 0

    pieces.append(pp(wide_stem, cell_cx(2), cell_cz(woz + 1)))
    grid.fill_room(wox, woz, 5, 3, set())

    # Attach a 3×3 room-small as alcove on one side of the wide room
    alcove_ox = rng.choice([0, 2])   # left or right quadrant
    small_stem = rng.choice(['room-small', 'room-small-variation', 'room-corner'])
    pieces.append(pp(small_stem, cell_cx(alcove_ox + 1), cell_cz(alcove_oz + 1)))
    grid.fill_room(alcove_ox, alcove_oz, 3, 3, set())

    # Connect wide-room and alcove (they may share boundary cells)
    for iz in range(CELLS):
        for ix in range(CELLS):
            if not grid.c(ix, iz).walkable:
                continue
            for face, (dx, dz) in DELTA.items():
                nix, niz = ix + dx, iz + dz
                if grid.in_bounds(nix, niz) and grid.c(nix, niz).walkable:
                    grid.c(ix, iz).open_faces.add(face)

    # Add boundary entrances
    for side in open_sides:
        dx, dz = DELTA[side]
        if side == 'N':   bnd = [(ix, 0) for ix in range(CELLS)]
        elif side == 'S': bnd = [(ix, CELLS-1) for ix in range(CELLS)]
        elif side == 'E': bnd = [(CELLS-1, iz) for iz in range(CELLS)]
        else:             bnd = [(0, iz) for iz in range(CELLS)]
        for bix, biz in bnd:
            if grid.c(bix, biz).walkable:
                grid.c(bix, biz).open_faces.add(side)
                break

    return pieces, grid


def strat_room_small_center(rng: random.Random, open_sides: Set[str]) -> Tuple[List[PlacedPiece], CellGrid]:
    """
    3×3 room centred in the module (cells 1-3).
    Each desired side gets a 1-cell corridor bridging the room to the boundary.
    Non-desired openings of the room are sealed by the room-small's own walls
    (no extra piece needed for them).
    """
    stem = rng.choice(['room-small', 'room-small-variation'])
    # Centre of the 3×3 area (cells 1,1 → 3,3): cell-centre at ix=2, iz=2
    rx, rz = cell_cx(2), cell_cz(2)
    pieces = [pp(stem, rx, rz)]

    grid = CellGrid()
    # Room occupies cells (1,1) to (3,3) — sw corner is (1,1)
    grid.fill_room(1, 1, 3, 3, set())   # no boundary entrances yet

    # Connect internal room faces outward via single-cell corridor
    # Room-small opens at the centre cell of each face:
    #   N face → cell (2,1), needing corridor at (2,0)
    #   S face → cell (2,3), needing corridor at (2,4)
    #   E face → cell (3,2), needing corridor at (4,2)
    #   W face → cell (1,2), needing corridor at (0,2)
    room_face_cells = {'N': (2,1), 'S': (2,3), 'E': (3,2), 'W': (1,2)}
    corr_cells      = {'N': (2,0), 'S': (2,4), 'E': (4,2), 'W': (0,2)}

    for side in SIDES:
        room_ix, room_iz = room_face_cells[side]
        corr_ix, corr_iz = corr_cells[side]
        if side in open_sides:
            # Open the room's face outward
            grid.c(room_ix, room_iz).open_faces.add(side)
            # Add corridor cell connecting room face → module boundary
            grid.fill_corridor_cell(corr_ix, corr_iz, side, OPPOSITE[side])
            # Corridor piece orientation
            yaw = PI32 if side in ('N', 'S') else 0.0
            corr_stem = rng.choice(['corridor', 'gate']) if rng.random() < 0.25 else 'corridor'
            pieces.append(pp(corr_stem, cell_cx(corr_ix), cell_cz(corr_iz), yaw))
        # else: room face is sealed by room model (no action needed)

    return pieces, grid


def strat_room_small_corner(rng: random.Random, open_sides: Set[str]) -> Tuple[List[PlacedPiece], CellGrid]:
    """
    3×3 room-small/corner placed in one quadrant.
    Corridors run from the room's two "useful" openings toward the module boundary
    (one axis per corridor).  Other openings are sealed.
    """
    stem = rng.choice(['room-small', 'room-small-variation', 'room-corner'])
    # All valid SW cell positions for a 3×3 piece within a 5×5 grid
    positions = [(ox, oz) for oz in range(CELLS-2) for ox in range(CELLS-2)]
    cell_ox, cell_oz = rng.choice(positions)
    rx = cell_cx(cell_ox + 1)
    rz = cell_cz(cell_oz + 1)

    # Determine which sides of the room are adjacent to the module boundary
    on_boundary_N = (cell_oz == 0)
    on_boundary_S = (cell_oz + 2 == CELLS - 1)
    on_boundary_E = (cell_ox + 2 == CELLS - 1)
    on_boundary_W = (cell_ox == 0)

    # For room-corner (opens S+E at yaw=0), choose a yaw so the openings
    # point outward toward the module boundary.
    if stem == 'room-corner':
        # Pick yaw so the two open faces point toward closer sides
        yaw_choices = [0.0, PI2, PI, PI32]
        corner_opens = {
            0.0:  frozenset(['S','E']),
            PI2:  frozenset(['W','S']),
            PI:   frozenset(['N','W']),
            PI32: frozenset(['E','N']),
        }
        best_yaw = 0.0
        best_match = -1
        for y, faces in corner_opens.items():
            match = sum(1 for f in faces if f in open_sides)
            if match > best_match:
                best_match, best_yaw = match, y
        yaw = best_yaw
        room_open_faces = corner_opens[yaw]
    else:
        yaw = 0.0
        room_open_faces = _fs('N','S','E','W')

    pieces = [pp(stem, rx, rz, yaw)]
    grid   = CellGrid()

    # Fill room cells
    nx, nz = rotated_dims(3, 3, yaw)
    grid.fill_room(cell_ox, cell_oz, nx, nz, set())

    # Cell that opens on each face of the 3×3 block
    face_cells = {
        'N': (cell_ox + nx//2, cell_oz),
        'S': (cell_ox + nx//2, cell_oz + nz - 1),
        'E': (cell_ox + nx - 1, cell_oz + nz//2),
        'W': (cell_ox,          cell_oz + nz//2),
    }

    for side in SIDES:
        if side not in room_open_faces:
            continue
        fcix, fciz = face_cells[side]
        dx, dz = DELTA[side]
        tix, tiz = fcix + dx, fciz + dz  # cell beyond room face

        if side in open_sides:
            # Open the room face
            grid.c(fcix, fciz).open_faces.add(side)
            # If the next cell is still inside the module, add corridor(s) to boundary
            while grid.in_bounds(tix, tiz):
                grid.fill_corridor_cell(tix, tiz, side, OPPOSITE[side])
                yaw_c = PI32 if side in ('N','S') else 0.0
                pieces.append(pp('corridor', cell_cx(tix), cell_cz(tiz), yaw_c))
                tix += dx; tiz += dz
        # else: room face sealed by model

    return pieces, grid


def strat_two_rooms(rng: random.Random, open_sides: Set[str]) -> Tuple[List[PlacedPiece], CellGrid]:
    """Two 3×3 rooms in opposite quadrants, connected by a central corridor."""
    # Choose two opposite or diagonal quadrant SW corners
    quad_pairs = [((0,0),(2,2)), ((2,0),(0,2))]
    (ox1,oz1), (ox2,oz2) = rng.choice(quad_pairs)

    stems = rng.sample(['room-small','room-small-variation','room-corner'], k=2)
    # Centre of each 3×3 block
    cx1, cz1 = cell_cx(ox1+1), cell_cz(oz1+1)
    cx2, cz2 = cell_cx(ox2+1), cell_cz(oz2+1)

    pieces = [pp(stems[0], cx1, cz1), pp(stems[1], cx2, cz2)]
    grid   = CellGrid()
    grid.fill_room(ox1, oz1, 3, 3, set())
    grid.fill_room(ox2, oz2, 3, 3, set())

    # Connect the two rooms through the module centre
    # Use corridor-intersection at (2,2) and corridors along each axis
    c_stem = rng.choice(['corridor-intersection', 'corridor-junction'])
    pieces.append(pp(c_stem, cell_cx(2), cell_cz(2)))
    grid.fill_corridor_cell(2, 2, 'N','S')
    grid.c(2,2).open_faces.update({'E','W'})  # intersection opens all

    # Link corridor to room 1 (find shortest path)
    def link_to_room(rx, rz, rid, gfid):
        # rx,rz = room SW corner; gfid = face of corridor toward room
        pass  # simplified: just open facing cells

    # Connect room1 (ox1,oz1) SE corner to corridor
    # path from room1's interior toward (2,2)
    for path_ix, path_iz, fa, fb in [(2,1,'N','S'), (2,3,'N','S'), (1,2,'E','W'), (3,2,'E','W')]:
        if grid.in_bounds(path_ix, path_iz) and not grid.c(path_ix, path_iz).walkable:
            grid.fill_corridor_cell(path_ix, path_iz, fa, fb)
            yaw_c = PI32 if fa in ('N','S') else 0.0
            pieces.append(pp('corridor', cell_cx(path_ix), cell_cz(path_iz), yaw_c))

    # Connect room-faces to the corridor network
    r1_mid = (ox1+1, oz1+1)
    r2_mid = (ox2+1, oz2+1)
    for rmx, rmz in [r1_mid, r2_mid]:
        for face, (dx, dz) in DELTA.items():
            nix, niz = rmx+dx, rmz+dz
            if grid.in_bounds(nix, niz) and grid.c(nix, niz).walkable:
                grid.c(rmx, rmz).open_faces.add(face)
                if OPPOSITE[face] not in grid.c(nix, niz).open_faces:
                    grid.c(nix, niz).open_faces.add(OPPOSITE[face])

    # Add boundary entrances
    entry_cells = {'N':(2,0,'N','S'), 'S':(2,4,'S','N'), 'E':(4,2,'E','W'), 'W':(0,2,'W','E')}
    for side in open_sides:
        ix, iz, fo, fi = entry_cells[side]
        if not grid.c(ix,iz).walkable:
            grid.fill_corridor_cell(ix, iz, fo, fi)
            yaw_c = PI32 if side in ('N','S') else 0.0
            pieces.append(pp('corridor', cell_cx(ix), cell_cz(iz), yaw_c))
        else:
            grid.c(ix, iz).open_faces.add(fo)

    return pieces, grid


def strat_room_wide_half(rng: random.Random, open_sides: Set[str]) -> Tuple[List[PlacedPiece], CellGrid]:
    """
    5×3 wide room occupying the N or S half of the module.
    The "inner" opening of the room gets a row of corridors/floor connecting
    to the other boundary.
    """
    stem = rng.choice(['room-wide', 'room-wide-variation'])
    half = rng.choice(['north', 'south'])

    if half == 'north':
        cell_oz = 0        # room occupies rows 0-2
        inner_side = 'S'   # room's S face is interior, at row 2
        outer_side = 'N'   # room's N face is at the module boundary
    else:
        cell_oz = 2        # room occupies rows 2-4
        inner_side = 'N'
        outer_side = 'S'

    rx, rz = cell_cx(2), cell_cz(cell_oz + 1)   # centre of 5×3 block
    pieces  = [pp(stem, rx, rz)]
    grid    = CellGrid()
    grid.fill_room(0, cell_oz, 5, 3, set())

    # Open external faces (room's outer faces on the boundary)
    # N: cell (2, cell_oz) face N;  S: (2, cell_oz+2) face S
    # E: (4, cell_oz+1) face E;     W: (0, cell_oz+1) face W
    outer_face_cells = {
        'N': (2, cell_oz),
        'S': (2, cell_oz + 2),
        'E': (4, cell_oz + 1),
        'W': (0, cell_oz + 1),
    }
    for side, (fix, fiz) in outer_face_cells.items():
        if side == outer_side and side in open_sides:
            grid.c(fix, fiz).open_faces.add(side)
        elif side in ('E', 'W') and side in open_sides:
            grid.c(fix, fiz).open_faces.add(side)

    # Extend inner side with a row of corridors bridging to the far boundary
    inner_row = cell_oz + 2 if half == 'north' else cell_oz   # row at inner face
    inner_face_cell_ix = 2
    bridge_dir = 'S' if half == 'north' else 'N'
    opp_dir    = OPPOSITE[bridge_dir]

    # Open the room's inner face
    grid.c(inner_face_cell_ix, inner_row).open_faces.add(bridge_dir)

    target_side_open = bridge_dir in open_sides
    dix, diz = DELTA[bridge_dir]
    bix, biz = inner_face_cell_ix + dix, inner_row + diz
    while grid.in_bounds(bix, biz):
        grid.fill_corridor_cell(bix, biz, bridge_dir, opp_dir)
        if target_side_open:
            pieces.append(pp('corridor', cell_cx(bix), cell_cz(biz), 0.0))
        bix += dix; biz += diz

    # If the inner side wasn't requested, we still need to CLOSE the room's
    # inner face.  The room-wide model has an opening there (built-in doorway),
    # so we add a template-wall.
    if bridge_dir not in open_sides:
        wx = 0.0
        wz = -10.0 + (inner_row + 0.5) * CELL_M + (1 if half=='north' else -1) * 2
        pieces.append(close_N() if bridge_dir=='N' else close_S())

    # Close unused outer boundary faces
    for side in ['N','S','E','W']:
        if side not in open_sides and side != inner_side:
            pieces.append(CLOSE[side]())

    return pieces, grid


def strat_corridor_hub(rng: random.Random, open_sides: Set[str]) -> Tuple[List[PlacedPiece], CellGrid]:
    """
    Corridor-intersection at centre + arms extending to boundary.
    Optional small room at one or two arm ends.
    """
    pieces = [pp('corridor-intersection', cell_cx(2), cell_cz(2))]
    grid   = CellGrid()
    grid.fill_corridor_cell(2, 2, 'N', 'S')
    grid.c(2,2).open_faces.update({'E', 'W'})

    # Arms going in each direction
    for side in SIDES:
        dx, dz = DELTA[side]
        prev_ix, prev_iz = 2, 2
        ix, iz = 2 + dx, 2 + dz
        arm_pieces = []
        while grid.in_bounds(ix, iz):
            grid.fill_corridor_cell(ix, iz, side, OPPOSITE[side])
            yaw_c = PI32 if side in ('N','S') else 0.0
            arm_pieces.append((ix, iz, yaw_c))
            prev_ix, prev_iz = ix, iz
            ix += dx; iz += dz

        if side in open_sides:
            # Commit arm pieces
            for aix, aiz, ayaw in arm_pieces:
                pieces.append(pp('corridor', cell_cx(aix), cell_cz(aiz), ayaw))
            # Open boundary face
            if arm_pieces:
                last_ix, last_iz, _ = arm_pieces[-1]
                grid.c(last_ix, last_iz).open_faces.add(side)
            else:
                grid.c(2, 2).open_faces.add(side)
        else:
            # Remove arm from grid (arm cells remain non-walkable)
            for aix, aiz, _ in arm_pieces:
                grid.c(aix, aiz).walkable = False
                grid.c(aix, aiz).open_faces.clear()

    # Optionally add small rooms at arm ends
    end_with_room = [s for s in open_sides if rng.random() < 0.4]
    for side in end_with_room:
        dx, dz = DELTA[side]
        # The boundary cell in this direction is at distance 2 from centre
        # (cells 0 or 4 for N/S/E/W).  We can't fit a 3×3 room there.
        # Skip if not enough space.
        pass  # placeholder for future enhancement

    # Gate on the module boundary wall (between tiles), not one cell inward.
    for side in open_sides:
        if rng.random() < 0.35:
            gx, gz, gyaw = gate_on_boundary(side, 2)
            pieces.append(pp('gate', gx, gz, gyaw))

    return pieces, grid


def strat_free_placement(rng: random.Random, open_sides: Set[str]) -> Tuple[List[PlacedPiece], CellGrid]:
    """
    Randomly places one large and 0-2 small structural pieces at any valid
    non-overlapping positions, then routes corridors from their openings to the
    requested module boundary entrances.  Produces the most structural variety.
    """
    grid   = CellGrid()
    pieces = []

    # Choose a main piece
    main_choice = rng.choice([
        ('room-large', 5, 5),
        ('room-large-variation', 5, 5),
        ('room-small', 3, 3),
        ('room-small-variation', 3, 3),
        ('room-wide', 5, 3),
        ('room-wide-variation', 5, 3),
        ('room-corner', 3, 3),
    ])
    mstem, mnx, mnz = main_choice
    # Random position for main piece
    max_ox = CELLS - mnx
    max_oz = CELLS - mnz
    ox = rng.randint(0, max(0, max_ox))
    oz = rng.randint(0, max(0, max_oz))
    grid.fill_room(ox, oz, mnx, mnz, set())
    pieces.append(pp(mstem, cell_cx(ox + mnx//2), cell_cz(oz + mnz//2)))

    # Optionally place a second 3×3 piece in a non-overlapping area
    if rng.random() < 0.55:
        small_stem = rng.choice(['room-small', 'room-small-variation', 'room-corner'])
        positions = [
            (sox, soz)
            for soz in range(CELLS-2) for sox in range(CELLS-2)
            if not any(grid.c(sox+dix, soz+diz).walkable
                       for diz in range(3) for dix in range(3)
                       if grid.in_bounds(sox+dix, soz+diz))
        ]
        if positions:
            sox, soz = rng.choice(positions)
            grid.fill_room(sox, soz, 3, 3, set())
            pieces.append(pp(small_stem, cell_cx(sox+1), cell_cz(soz+1)))

    # Route corridors: for each open side, find the nearest walkable cell
    # and trace a corridor from it to the module boundary
    for side in open_sides:
        dx, dz = DELTA[side]
        bdx, bdz = -dx, -dz  # interior direction

        # Find boundary cells on this side
        if side == 'N': bnd_cells = [(ix, 0) for ix in range(CELLS)]
        elif side == 'S': bnd_cells = [(ix, CELLS-1) for ix in range(CELLS)]
        elif side == 'E': bnd_cells = [(CELLS-1, iz) for iz in range(CELLS)]
        else:             bnd_cells = [(0, iz) for iz in range(CELLS)]

        # Try each boundary cell; trace inward until we hit a walkable cell
        connected = False
        rng.shuffle(bnd_cells)
        for bix, biz in bnd_cells:
            # Trace from boundary inward
            path = []
            tix, tiz = bix, biz
            while grid.in_bounds(tix, tiz) and not grid.c(tix, tiz).walkable:
                path.append((tix, tiz))
                tix += bdx; tiz += bdz
            if grid.in_bounds(tix, tiz) and grid.c(tix, tiz).walkable and path:
                # Connect: open room face + add corridor cells
                grid.c(tix, tiz).open_faces.add(side)
                prev_open = side
                for cix, ciz in reversed(path):
                    grid.fill_corridor_cell(cix, ciz, side, OPPOSITE[side])
                    yaw_c = PI32 if side in ('N','S') else 0.0
                    pieces.append(pp('corridor', cell_cx(cix), cell_cz(ciz), yaw_c))
                # Open the boundary cell's outward face
                grid.c(path[0][0], path[0][1]).open_faces.add(side)
                connected = True
                break
        # If nothing could be connected, just open the nearest walkable boundary cell
        if not connected:
            for bix, biz in bnd_cells:
                if grid.in_bounds(bix, biz) and grid.c(bix, biz).walkable:
                    grid.c(bix, biz).open_faces.add(side)
                    break

    return pieces, grid


def add_deco(rng: random.Random, pieces: List[PlacedPiece], grid: CellGrid,
             n_min: int = 0, n_max: int = 4):
    """
    Add random decorative trim to walkable cells.
    Called after every strategy to increase fingerprint variety.

    Do **not** add ``template-floor-*`` here — room/corridor GLBs already include
    floor meshes; extra floor tiles z-fight and read as duplicate textures.
    """
    deco = [
        'template-wall-detail-a', 'cables',
    ]
    walkable_cells = [
        (ix, iz) for iz in range(CELLS) for ix in range(CELLS)
        if grid.c(ix, iz).walkable
    ]
    rng.shuffle(walkable_cells)
    for ix, iz in walkable_cells[:rng.randint(n_min, n_max)]:
        pieces.append(pp(
            rng.choice(deco),
            cell_cx(ix), cell_cz(iz),
            rng.choice([0.0, PI2, PI, PI32])
        ))


def strat_four_rooms(rng: random.Random, open_sides: Set[str]) -> Tuple[List[PlacedPiece], CellGrid]:
    """
    Four room-small pieces, one in each quadrant of the 5×5 grid.
    Together they fill the full grid — a 2×2 open room cluster.
    Inspired by module_09.
    """
    quad_sw = [(0, 0), (2, 0), (0, 2), (2, 2)]
    stems = [rng.choice(['room-small', 'room-small-variation']) for _ in range(4)]

    pieces = []
    grid = CellGrid()
    for i, (ox, oz) in enumerate(quad_sw):
        pieces.append(pp(stems[i], cell_cx(ox + 1), cell_cz(oz + 1)))
        grid.fill_room(ox, oz, 3, 3, set())

    # All adjacent walkable pairs become connected
    for iz in range(CELLS):
        for ix in range(CELLS):
            if not grid.c(ix, iz).walkable:
                continue
            for face, (dx, dz) in DELTA.items():
                nix, niz = ix + dx, iz + dz
                if grid.in_bounds(nix, niz) and grid.c(nix, niz).walkable:
                    grid.c(ix, iz).open_faces.add(face)

    # Boundary entrances
    for side in open_sides:
        dx, dz = DELTA[side]
        if side == 'N':   bnd = [(ix, 0) for ix in range(CELLS)]
        elif side == 'S': bnd = [(ix, CELLS-1) for ix in range(CELLS)]
        elif side == 'E': bnd = [(CELLS-1, iz) for iz in range(CELLS)]
        else:             bnd = [(0, iz) for iz in range(CELLS)]
        for bix, biz in bnd:
            if grid.c(bix, biz).walkable:
                grid.c(bix, biz).open_faces.add(side)
                break

    return pieces, grid


def strat_wide_corridor(rng: random.Random, open_sides: Set[str]) -> Tuple[List[PlacedPiece], CellGrid]:
    """
    Three corridor-wide pieces forming a continuous wide hallway across the
    full module length.  Orientation chosen to match the most requested sides.
    Inspired by module_06.
    """
    ns_score = ('N' in open_sides) + ('S' in open_sides)
    ew_score = ('E' in open_sides) + ('W' in open_sides)

    pieces = []
    grid = CellGrid()

    if ns_score >= ew_score:
        # N-S hallway: 3 corridor-wide at x=0, z ∈ {-6, 0, 6}, yaw=PI2 opens N+S
        for z_pos in [-6.0, 0.0, 6.0]:
            pieces.append(pp('corridor-wide', 0.0, z_pos, PI2))
        # The combined 3 pieces cover the centre 2 columns (ix=1,2) full height
        for iz in range(CELLS):
            for ix in [1, 2]:
                grid.c(ix, iz).walkable = True
                grid.c(ix, iz).open_faces.update({'N', 'S'})
        for iz in range(CELLS):
            grid.c(1, iz).open_faces.add('E')
            grid.c(2, iz).open_faces.add('W')
        # N-S inter-cell connectivity
        for iz in range(CELLS - 1):
            for ix in [1, 2]:
                grid.c(ix, iz).open_faces.add('S')
                grid.c(ix, iz + 1).open_faces.add('N')
        # Boundary openings
        for side in open_sides:
            if side == 'N':
                grid.c(1, 0).open_faces.add('N')
                grid.c(2, 0).open_faces.add('N')
            elif side == 'S':
                grid.c(1, CELLS-1).open_faces.add('S')
                grid.c(2, CELLS-1).open_faces.add('S')
            elif side == 'E':
                for iz in range(CELLS):
                    if grid.c(2, iz).walkable:
                        grid.c(2, iz).open_faces.add('E')
                        break
            elif side == 'W':
                for iz in range(CELLS):
                    if grid.c(1, iz).walkable:
                        grid.c(1, iz).open_faces.add('W')
                        break
    else:
        # E-W hallway: 3 corridor-wide at z=0, x ∈ {-6, 0, 6}, yaw=0 opens E+W
        for x_pos in [-6.0, 0.0, 6.0]:
            pieces.append(pp('corridor-wide', x_pos, 0.0, 0.0))
        for ix in range(CELLS):
            for iz in [1, 2]:
                grid.c(ix, iz).walkable = True
                grid.c(ix, iz).open_faces.update({'E', 'W'})
        for ix in range(CELLS):
            grid.c(ix, 1).open_faces.add('S')
            grid.c(ix, 2).open_faces.add('N')
        for ix in range(CELLS - 1):
            for iz in [1, 2]:
                grid.c(ix, iz).open_faces.add('E')
                grid.c(ix + 1, iz).open_faces.add('W')
        for side in open_sides:
            if side == 'E':
                grid.c(CELLS-1, 1).open_faces.add('E')
                grid.c(CELLS-1, 2).open_faces.add('E')
            elif side == 'W':
                grid.c(0, 1).open_faces.add('W')
                grid.c(0, 2).open_faces.add('W')
            elif side == 'N':
                for ix in range(CELLS):
                    if grid.c(ix, 0).walkable:
                        grid.c(ix, 0).open_faces.add('N')
                        break
            elif side == 'S':
                for ix in range(CELLS):
                    if grid.c(ix, CELLS-1).walkable:
                        grid.c(ix, CELLS-1).open_faces.add('S')
                        break

    return pieces, grid


def strat_hub_center(rng: random.Random, open_sides: Set[str]) -> Tuple[List[PlacedPiece], CellGrid]:
    """
    template-floor-big (2×2) at centre + corridor-transition pieces radiating
    outward in each requested direction — a clean four-way hub.
    Inspired by module_08.
    """
    pieces = [pp('template-floor-big', 0.0, 0.0, PI2)]
    grid = CellGrid()

    # Central 2×2 block: cells (1,1),(2,1),(1,2),(2,2)
    grid.fill_room(1, 1, 2, 2, set())

    # Transition yaw so the piece faces "inward" (opening toward the centre)
    arm_cfg = {
        'N': ((2, 1), (2, 0), PI32),   # transition at (2,1), entrance at (2,0)
        'S': ((2, 3), (2, 4), PI2),
        'E': ((3, 2), (4, 2), PI),
        'W': ((1, 2), (0, 2), 0.0),
    }
    for side in open_sides:
        tcell, ecell, yaw = arm_cfg[side]
        tix, tiz = tcell
        eix, eiz = ecell
        # Transition piece is adjacent to the central block
        grid.c(tix, tiz).walkable = True
        grid.c(tix, tiz).open_faces.update({side, OPPOSITE[side]})
        pieces.append(pp('corridor-transition', cell_cx(tix), cell_cz(tiz), yaw))
        # Boundary entrance cell (may equal transition cell if already at edge)
        if (eix, eiz) != (tix, tiz):
            grid.fill_corridor_cell(eix, eiz, side, OPPOSITE[side])
        grid.c(eix, eiz).open_faces.add(side)

    # Connect all walkable cells
    for iz in range(CELLS):
        for ix in range(CELLS):
            if not grid.c(ix, iz).walkable:
                continue
            for face, (dx, dz) in DELTA.items():
                nix, niz = ix + dx, iz + dz
                if grid.in_bounds(nix, niz) and grid.c(nix, niz).walkable:
                    grid.c(ix, iz).open_faces.add(face)

    return pieces, grid


def strat_room_with_walls(rng: random.Random, open_sides: Set[str]) -> Tuple[List[PlacedPiece], CellGrid]:
    """
    room-large with template-wall pieces explicitly closing the faces that are
    NOT requested as entrances.  Optionally adds stairs at one entrance.
    Creates a clean, well-defined single room (like module_05).
    """
    stem = rng.choice(['room-large', 'room-large-variation'])
    pieces = [pp(stem, 0.0, 0.0)]
    grid = CellGrid()
    grid.fill_room(0, 0, 5, 5, set())

    # Close every side that isn't an entrance
    for side in SIDES:
        if side not in open_sides:
            pieces.append(CLOSE[side]())

    # Boundary entrance cells
    face_cells = {'N': (2, 0), 'S': (2, 4), 'E': (4, 2), 'W': (0, 2)}
    for side in open_sides:
        ix, iz = face_cells[side]
        grid.c(ix, iz).open_faces.add(side)

    # Occasionally place wide stairs at one entrance (functional, like module_05)
    if rng.random() < 0.40 and open_sides:
        stair_side = rng.choice(sorted(open_sides))
        stair_pos = {'N': (0.0, -8.0), 'S': (0.0, 8.0), 'E': (8.0, 0.0), 'W': (-8.0, 0.0)}
        stair_yaw = {'N': PI, 'S': 0.0, 'E': PI2, 'W': PI32}
        pieces.append(pp('stairs-wide', stair_pos[stair_side][0],
                         stair_pos[stair_side][1], stair_yaw[stair_side]))

    return pieces, grid


# Room database.  Each entry has:
#   footprint  – (dx,dz) offsets from center anchor that the GLB occupies
#   connectors – {side: (dx,dz)} the one cell OUTSIDE the room face where a
#                corridor can legally enter.  The GLB has a built-in doorway on
#                the face between that connector cell and the adjacent room cell.
#   yaw        – rotation to apply to the GLB
#
# Derived from the existing verified strategies (strat_room_small_center,
# strat_room_small_corner, strat_room_wide_half).
ROOMS_DB: List[Dict] = [
    # ── room-small: all 4 sides open, connector 2 cells from anchor ──────────
    {
        'stem': 'room-small',
        'yaw':  0.0,
        'footprint': {
            (-1,-1),(0,-1),(1,-1),
            (-1, 0),(0, 0),(1, 0),
            (-1, 1),(0, 1),(1, 1),
        },
        'connectors': {'N':(0,-2), 'S':(0,2), 'E':(2,0), 'W':(-2,0)},
        'close_unused_doors': True,  # GLB has visible openings on all 4 sides
    },
    # ── room-corner: 2 open sides, 4 rotations.
    # Open-face → yaw mapping VERIFIED from live user ratings:
    #   W+S → PI2 (green),  S+E → PI,  N+W → 0,  E+N → PI32.
    # The GLB rotates CCW as yaw increases, so the missing-footprint corner
    # (the solid corner) sits opposite the open pair and matches the yaw below.
    {   # opens N+W: GLB at yaw 0 → solid corner SE
        'stem': 'room-corner', 'yaw': 0.0,
        'footprint': {(-1,-1),(0,-1),(1,-1),
                      (-1, 0),(0, 0),(1, 0),
                      (-1, 1),(0, 1)         },
        'connectors': {'N':(0,-2), 'W':(-2,0)},
        'close_unused_doors': True,
    },
    {   # opens W+S: GLB at yaw PI2 → solid corner NE  (green reference)
        'stem': 'room-corner', 'yaw': PI2,
        'footprint': {(-1,-1),(0,-1),
                      (-1, 0),(0, 0),(1, 0),
                      (-1, 1),(0, 1),(1, 1)},
        'connectors': {'W':(-2,0), 'S':(0,2)},
        'close_unused_doors': True,
    },
    {   # opens S+E: GLB at yaw PI → solid corner NW
        'stem': 'room-corner', 'yaw': PI,
        'footprint': {          (0,-1),(1,-1),
                      (-1, 0),(0, 0),(1, 0),
                      (-1, 1),(0, 1),(1, 1)},
        'connectors': {'S':(0,2), 'E':(2,0)},
        'close_unused_doors': True,
    },
    {   # opens E+N: GLB at yaw PI32 → solid corner SW
        'stem': 'room-corner', 'yaw': PI32,
        'footprint': {(-1,-1),(0,-1),(1,-1),
                      (-1, 0),(0, 0),(1, 0),
                               (0, 1),(1, 1)},
        'connectors': {'E':(2,0), 'N':(0,-2)},
        'close_unused_doors': True,
    },
    # room-small-variation is excluded for now: its true GLB opening geometry
    # is not yet verified (the only hand-made reference, module_10, places it at
    # yaw PI against module boundaries with hand-added walls, which is
    # insufficient to derive its opening sides).  Re-add once confirmed.
    # ── room-wide: N+S open at center column; E+W exits land on room boundary ─
    # Anchor (2,2) only (width 5 = module-wide, E/W at module edge).
    {
        'stem': 'room-wide', 'yaw': 0.0,
        'footprint': {
            (-2,-1),(-1,-1),(0,-1),(1,-1),(2,-1),
            (-2, 0),(-1, 0),(0, 0),(1, 0),(2, 0),
            (-2, 1),(-1, 1),(0, 1),(1, 1),(2, 1),
        },
        # E/W exits land directly on the room boundary cells (no corridor needed).
        # Only N and S have off-room connector cells.
        'connectors': {'N':(0,-2), 'S':(0,2)},
        # E+W exits can also be served but only when the exit cell is on the
        # room boundary (ix=0 or ix=4 at the room's center row, iz=anchor_iz).
        'boundary_exits': {'E':(2,0), 'W':(-2,0)},
        'anchor_fixed': (2, 2),   # room is module-wide; only one valid anchor
        'close_unused_doors': True,
    },
    # room-wide-variation is excluded: the N/S center cells are decorative "q"
    # features (half-height, closed) so neither N/S nor E/W faces have walkable
    # openings.  The GLB cannot be connected to corridors.
    # ── room-large: fills the full 5×5; exits land directly on room boundary ─
    {
        'stem': 'room-large', 'yaw': 0.0,
        'footprint': {
            (-2,-2),(-1,-2),(0,-2),(1,-2),(2,-2),
            (-2,-1),(-1,-1),(0,-1),(1,-1),(2,-1),
            (-2, 0),(-1, 0),(0, 0),(1, 0),(2, 0),
            (-2, 1),(-1, 1),(0, 1),(1, 1),(2, 1),
            (-2, 2),(-1, 2),(0, 2),(1, 2),(2, 2),
        },
        'connectors': {},   # no off-room connectors; exits land on room cells
        'boundary_exits': {'N':(0,-2), 'S':(0,2), 'E':(2,0), 'W':(-2,0)},
        'anchor_fixed': (2, 2),
    },
]


def strat_planned(
    rng: random.Random,
    open_sides: Set[str],
    *,
    fixed_exit_cells: Optional[Dict[str, Tuple[int, int]]] = None,
    no_rooms: bool = False,
) -> Tuple[List[PlacedPiece], CellGrid]:
    """
    Planning-based strategy with optional room placement:

    1. Choose exit cell positions (center-weighted 1-4-12-4-1), or use
       fixed_exit_cells when supplied (map generator tile alignment).
    2. Optionally place a room whose footprint becomes the initial paint.
    3. Route L-shaped corridors from each exit to the room (or exit-to-exit).
    4. Select corridor piece per cell (straight / corner / junction / intersection).
    5. Place room GLB at anchor.  Room cells never get corridor pieces.
    6. Close unused boundary opens with template-wall.

    When no_rooms=True, step 2 and 5 are skipped — corridors + template-floor
    only (used by gen_maps tile synthesis).
    """
    pieces   = []
    cell_grd = CellGrid()

    # ── 1. Exit cell positions ────────────────────────────────────────────────
    EXIT_WEIGHTS = [1, 4, 12, 4, 1]
    exit_cells: Dict[str, Tuple[int, int]] = {}
    if fixed_exit_cells is not None:
        exit_cells = dict(fixed_exit_cells)
    else:
        for side in open_sides:
            col = rng.choices(range(CELLS), weights=EXIT_WEIGHTS, k=1)[0]
            if side == 'N':   exit_cells[side] = (col, 0)
            elif side == 'S': exit_cells[side] = (col, CELLS - 1)
            elif side == 'E': exit_cells[side] = (CELLS - 1, col)
            else:             exit_cells[side] = (0, col)

    # ── 2. Optionally place a room ────────────────────────────────────────────
    room_cells:         Set[Tuple[int, int]]        = set()
    room_stem:          str                          = ''
    room_yaw:           float                        = 0.0
    room_anchor:        Tuple[int, int]              = (0, 0)
    room_conns_def:     Dict[str, Tuple[int, int]]   = {}
    room_close_unused:  bool                         = False
    # connector_target[exit_side] = (cx, cz) — the corridor cell that leads into the room
    connector_target: Dict[str, Tuple[int, int]] = {}

    if not no_rooms and rng.random() < 0.70:
        candidates = []
        for rdef in ROOMS_DB:
            fp      = rdef['footprint']
            conns   = rdef['connectors']                # side → (dx,dz)
            bexits  = rdef.get('boundary_exits', {})    # side → (dx,dz) on room boundary
            fixed   = rdef.get('anchor_fixed', None)

            anchor_list = [fixed] if fixed else \
                          [(ax, az) for ax in range(CELLS) for az in range(CELLS)]

            for ax, az in anchor_list:
                cells = frozenset((ax + dx, az + dz) for dx, dz in fp)
                if not all(0 <= ix < CELLS and 0 <= iz < CELLS for ix, iz in cells):
                    continue

                # Build side→connector mapping for this anchor.
                # Only off-room connectors go into side_conn.
                # boundary_exits are handled separately in the exit-check loop.
                side_conn: Dict[str, Tuple[int, int]] = {}
                for side, (dx, dz) in conns.items():
                    cx, cz = ax + dx, az + dz
                    if 0 <= cx < CELLS and 0 <= cz < CELLS:
                        side_conn[side] = (cx, cz)

                # Every exit must have a same-side connector OR land exactly on
                # a room boundary cell (for rooms whose exits share a wall with
                # the module boundary, e.g. room-wide, room-large).
                ok = True
                exit_conn: Dict[str, Tuple[int, int]] = {}
                for side, (ex, ez) in exit_cells.items():
                    if side in side_conn:
                        exit_conn[side] = side_conn[side]
                    elif side in bexits:
                        # boundary exit: exit cell must BE the room's boundary cell
                        bdx, bdz = bexits[side]
                        bx, bz = ax + bdx, az + bdz
                        if (bx, bz) in cells and (ex, ez) == (bx, bz):
                            exit_conn[side] = (bx, bz)
                        else:
                            ok = False; break
                    else:
                        # No connector on this side at all — reject this room
                        ok = False; break
                if ok:
                    candidates.append((rdef['stem'], rdef['yaw'], ax, az,
                                       frozenset(cells), exit_conn,
                                       conns,
                                       rdef.get('close_unused_doors', False)))

        if candidates:
            room_stem, room_yaw, ax, az, room_cells_frozen, exit_conn, \
                room_conns_def, room_close_unused = rng.choice(candidates)
            room_cells      = set(room_cells_frozen)
            room_anchor     = (ax, az)
            connector_target = exit_conn
            for ix, iz in room_cells:
                cell_grd.c(ix, iz).walkable = True

    # ── 3. Route corridors ────────────────────────────────────────────────────
    painted: Set[Tuple[int, int]] = set()

    if room_cells:
        for side, (ex, ez) in exit_cells.items():
            target = connector_target.get(side, (ex, ez))
            tx, tz = target
            # Route L-shaped path from exit to connector (stop before room cells)
            route: Set[Tuple[int, int]] = set()
            if rng.random() < 0.5:
                for x in range(min(ex, tx), max(ex, tx) + 1): route.add((x, ez))
                for z in range(min(ez, tz), max(ez, tz) + 1): route.add((tx, z))
            else:
                for z in range(min(ez, tz), max(ez, tz) + 1): route.add((ex, z))
                for x in range(min(ex, tx), max(ex, tx) + 1): route.add((x, tz))
            # Keep corridor cells; room cells stay as room
            painted |= route - room_cells

        # Ensure all exit cells themselves are painted (unless they're room cells)
        for ix, iz in exit_cells.values():
            if (ix, iz) not in room_cells:
                painted.add((ix, iz))

    else:
        # Pure corridor: chain exits with L-shaped paths
        painted = set(exit_cells.values())
        exit_list = list(exit_cells.values())
        if len(exit_list) >= 2:
            for i in range(len(exit_list) - 1):
                sx, sz = exit_list[i]; ex, ez = exit_list[i + 1]
                if rng.random() < 0.5:
                    for x in range(min(sx, ex), max(sx, ex) + 1): painted.add((x, sz))
                    for z in range(min(sz, ez), max(sz, ez) + 1): painted.add((ex, z))
                else:
                    for z in range(min(sz, ez), max(sz, ez) + 1): painted.add((sx, z))
                    for x in range(min(sx, ex), max(sx, ex) + 1): painted.add((x, ez))
        elif len(exit_list) == 1:
            side = next(iter(open_sides)); ix, iz = exit_list[0]
            dx, dz = DELTA[OPPOSITE[side]]
            for _ in range(rng.randint(1, 3)):
                ix += dx; iz += dz
                if 0 <= ix < CELLS and 0 <= iz < CELLS:
                    painted.add((ix, iz))

    # ── 4. Build CellGrid walkability + open faces ────────────────────────────
    for ix, iz in painted:
        cell_grd.c(ix, iz).walkable = True

    # Doorways: room cells whose outward face is a real GLB opening.
    # Derived from connectors: connector at (ax+cdx, az+cdz) → face cell at
    # (ax+cdx//2, az+cdz//2) with outward direction = side.
    room_doorways: Dict[Tuple[int, int], str] = {}
    if room_cells:
        for side, (cdx, cdz) in connector_target.items():
            ax, az = room_anchor
            if side in ('N','S','E','W'):
                # connector offset from anchor
                conn_dx = cdx - ax; conn_dz = cdz - az
                if abs(conn_dx) == 2 or abs(conn_dz) == 2:
                    fdx = conn_dx // 2; fdz = conn_dz // 2
                    face_cell = (ax + fdx, az + fdz)
                    if face_cell in room_cells:
                        room_doorways[face_cell] = side  # outward direction

    all_walkable = painted | room_cells
    for ix, iz in all_walkable:
        for face, (dx, dz) in DELTA.items():
            nix, niz = ix + dx, iz + dz
            if not (cell_grd.in_bounds(nix, niz) and cell_grd.c(nix, niz).walkable):
                continue
            # Corridor cell → room cell: only allow if that room cell has a
            # doorway facing back toward the corridor (outward == OPPOSITE[face]).
            if (ix, iz) in painted and (nix, niz) in room_cells:
                outward = room_doorways.get((nix, niz))
                if outward is None or OPPOSITE[outward] != face:
                    continue
            # Room cell → corridor cell: only allow if this room cell has a
            # doorway facing toward the corridor (outward == face).
            if (ix, iz) in room_cells and (nix, niz) in painted:
                outward = room_doorways.get((ix, iz))
                if outward is None or outward != face:
                    continue
            cell_grd.c(ix, iz).open_faces.add(face)

    for side, (ix, iz) in exit_cells.items():
        cell_grd.c(ix, iz).open_faces.add(side)

    # ── 5. Place corridor pieces (painted cells only — not room cells) ────────
    CORNER_YAW: Dict[frozenset, float] = {
        frozenset({'N', 'W'}): 0.0,
        frozenset({'S', 'W'}): PI2,
        frozenset({'S', 'E'}): PI,
        frozenset({'N', 'E'}): PI32,
    }
    JUNC_YAW: Dict[str, float] = {'S': 0.0, 'E': PI2, 'N': PI, 'W': PI32}

    for ix, iz in painted:
        conn   = cell_grd.c(ix, iz).open_faces
        n_conn = len(conn)

        if n_conn == 0:
            pieces.append(pp('template-floor', cell_cx(ix), cell_cz(iz)))
        elif n_conn == 1:
            fa = next(iter(conn))
            end_yaw = {'N': PI2, 'S': PI32, 'E': 0.0, 'W': PI}
            pieces.append(pp('corridor-end', cell_cx(ix), cell_cz(iz),
                             end_yaw.get(fa, 0.0)))
        elif n_conn == 2:
            fa_set = frozenset(conn)
            if fa_set in (frozenset({'N', 'S'}), frozenset({'E', 'W'})):
                pieces.append(pp('corridor', cell_cx(ix), cell_cz(iz),
                                 PI32 if 'N' in conn else 0.0))
            else:
                pieces.append(pp('corridor-corner', cell_cx(ix), cell_cz(iz),
                                 CORNER_YAW.get(fa_set, 0.0)))
        elif n_conn == 3:
            missing = (set(SIDES) - conn).pop()
            pieces.append(pp('corridor-junction', cell_cx(ix), cell_cz(iz),
                             JUNC_YAW.get(missing, 0.0)))
        else:
            pieces.append(pp('corridor-intersection', cell_cx(ix), cell_cz(iz)))

    # ── 6. Place room GLB ─────────────────────────────────────────────────────
    if room_stem:
        ax, az = room_anchor
        pieces.append(pp(room_stem, cell_cx(ax), cell_cz(az), room_yaw))

        # Close any room connector doorways that no module exit uses.
        # Room GLBs (especially room-corner) have visible openings on their
        # connector sides.  Without a template-wall, unused doorways are open.
        if room_close_unused:
            # Wall yaw verified against hand-made module_12 / module_07:
            # a wall sealing a cell's N face → PI, S → 0, E → PI2, W → PI32.
            # This makes the wall's finished face point inward toward the room
            # (not toward the adjacent 'n' cell).
            CLOSE_ROOM_YAW = {'N': PI, 'S': 0.0, 'E': PI2, 'W': PI32}
            for side, (cdx, cdz) in room_conns_def.items():
                if side in connector_target:
                    continue  # this side has a connected corridor — leave it open
                # Compute face cell offset (half the connector offset)
                fdx, fdz = cdx // 2, cdz // 2
                face_ix, face_iz = ax + fdx, az + fdz
                if (face_ix, face_iz) not in room_cells:
                    continue  # sanity check
                dx, dz = DELTA[side]
                wx = cell_cx(face_ix) + 2.0 * dx
                wz = cell_cz(face_iz) + 2.0 * dz
                pieces.append(pp('template-wall', wx, wz, CLOSE_ROOM_YAW[side]))

    # ── 7. Close unused boundary opens with template-wall at module edge ──────
    for side in SIDES:
        if side in open_sides:
            continue
        if side == 'N':   bnd = [(ix, 0)        for ix in range(CELLS)]
        elif side == 'S': bnd = [(ix, CELLS - 1) for ix in range(CELLS)]
        elif side == 'E': bnd = [(CELLS - 1, iz) for iz in range(CELLS)]
        else:             bnd = [(0, iz)          for iz in range(CELLS)]
        for bix, biz in bnd:
            if cell_grd.c(bix, biz).walkable and side in cell_grd.c(bix, biz).open_faces:
                # Wall facing verified vs hand-made module_12 / module_07:
                # N edge→PI, S edge→0, E edge→PI2, W edge→PI32 (face points in).
                if side == 'N':   pieces.append(pp('template-wall', cell_cx(bix), -10.0, PI))
                elif side == 'S': pieces.append(pp('template-wall', cell_cx(bix),  10.0, 0.0))
                elif side == 'E': pieces.append(pp('template-wall',  10.0, cell_cz(biz), PI2))
                else:             pieces.append(pp('template-wall', -10.0, cell_cz(biz), PI32))
                cell_grd.c(bix, biz).open_faces.discard(side)

    return pieces, cell_grd


# ─── strategy registry ───────────────────────────────────────────────────────

STRATS = {
    # ── planning-based (new approach: correct routing + piece selection) ──────
    'planned':          strat_planned,          # L-shaped routing, correct pieces
    # ── single-room strategies ────────────────────────────────────────────────
    'room_large':       strat_room_large,       # clean 5×5 room
    'room_with_walls':  strat_room_with_walls,  # 5×5 room + wall closures ± stairs
    # ── multi-room strategies ─────────────────────────────────────────────────
    'four_rooms':       strat_four_rooms,       # 4×room-small, 2×2 grid (module_09)
    'two_rooms':        strat_two_rooms,        # 2 diagonal rooms + corridor
    'alcove':           strat_alcove,           # room-wide + side pocket
    'corner_room':      strat_corner_room,      # L-shaped room-corner
    'two_corners':      strat_two_corners,      # S/Z-shaped double corner
    # ── corridor-centric strategies ───────────────────────────────────────────
    'wide_corridor':    strat_wide_corridor,    # 3×corridor-wide wide hall (module_06)
    'hub_center':       strat_hub_center,       # floor-big + corridor-transitions (module_08)
    'corridor_hub':     strat_corridor_hub,     # narrow corridors + optional rooms
    # ── positioned-room strategies ────────────────────────────────────────────
    'room_small_ctr':   strat_room_small_center,
    'room_small_corn':  strat_room_small_corner,
    'room_wide':        strat_room_wide_half,
    'free':             strat_free_placement,
}

# Only these strategies are used in an unforced (auto) run.  The legacy
# strategies remain callable via --strategy for debugging, but they are
# unverified: they assume room GLBs auto-seal unused faces (false — room-small
# has openings on all four sides) and they emit room-*-variation pieces whose
# decorative "q"/clipped cells get mistaken for doorways.  strat_planned is the
# only generator with correct doorway/wall-closing logic, so it is the sole
# auto strategy.  Add more here once they are individually verified.
ENABLED_STRATS = ['planned']


# ─── adaptive parameters ─────────────────────────────────────────────────────

class Params:
    def __init__(self):
        self.threshold    = 0.58
        # Only verified strategies participate in auto selection / adaptation.
        self.strat_w      = {s: 1.0 for s in ENABLED_STRATS}
        self.entrance_w   = [0.15, 1.0, 0.9, 0.35]  # weights for 1,2,3,4 entrances
        # rolling window for adaptation
        self._window      = 2000        # 2k-candidate windows at ~5k c/s = ~0.4s cycles
        self._recent      = []          # recent (score, strategy) pairs
        self._adapt_n     = 0

    def record(self, score: float, strat: str):
        self._recent.append((score, strat))
        if len(self._recent) > self._window * 2:
            self._recent = self._recent[-self._window:]

    def maybe_adapt(self, log_fn) -> bool:
        """Returns True if adaptation happened (and something meaningful changed)."""
        self._adapt_n += 1
        if self._adapt_n < self._window:
            return False
        self._adapt_n = 0

        window = self._recent[-self._window:]
        accept_rate = sum(1 for s, _ in window if s >= self.threshold) / len(window)
        mean_score  = sum(s for s, _ in window) / len(window)

        old_thr = self.threshold
        if accept_rate > 0.30:
            self.threshold = min(0.82, self.threshold + 0.015)
        elif accept_rate < 0.04:
            self.threshold = max(0.35, self.threshold - 0.015)

        # Adjust strategy weights (only verified/enabled strategies)
        strat_scores: Dict[str, List[float]] = {s: [] for s in self.strat_w}
        for sc, st in window:
            if st in strat_scores:
                strat_scores[st].append(sc)
        changed = abs(self.threshold - old_thr) > 1e-4
        for st in self.strat_w:
            if len(strat_scores[st]) >= 20:
                avg = sum(strat_scores[st]) / len(strat_scores[st])
                new_w = max(0.3, min(4.0, avg * 5.0))
                if abs(new_w - self.strat_w[st]) > 0.15:
                    changed = True
                self.strat_w[st] = new_w

        # Only log when something meaningful changed
        if changed:
            log_fn(f"ADAPT accept={accept_rate:.1%} mean={mean_score:.3f} "
                   f"thr {old_thr:.3f}->{self.threshold:.3f} | "
                   f"w={ {s: round(w,2) for s,w in self.strat_w.items()} }")
        return changed

    def pick_strategy(self, rng: random.Random) -> str:
        names   = list(self.strat_w.keys())
        weights = [self.strat_w[n] for n in names]
        return rng.choices(names, weights=weights, k=1)[0]

    def pick_n_entrances(self, rng: random.Random) -> int:
        return rng.choices([1, 2, 3, 4], weights=self.entrance_w, k=1)[0]


# ─── deduplication ────────────────────────────────────────────────────────────

def structural_fp(mod: Module) -> str:
    """
    Coarse fingerprint based only on walkable layout + entrance sides.
    Used to group variants of the same room layout.
    """
    ent_sides = frozenset(f for _, _, f in mod.grid.entrances())
    cells_bits = ''.join(
        '1' if mod.grid.c(ix, iz).walkable else '0'
        for iz in range(CELLS) for ix in range(CELLS)
    )
    return f"{','.join(sorted(ent_sides))}|{cells_bits}"


def piece_fp(mod: Module) -> str:
    """
    Fine fingerprint: structural + the set of structural piece stems used.
    Allows room-small vs room-small-variation for the same layout.
    """
    struct = structural_fp(mod)
    stems = tuple(sorted(
        p.stem for p in mod.pieces
        if PIECE_DB.get(p.stem, PieceDef('',0,0,_fs(),False)).is_structural
    ))
    return f"{struct}|{stems}"


# ─── logging ─────────────────────────────────────────────────────────────────

class Logger:
    def __init__(self, path: Path):
        path.parent.mkdir(parents=True, exist_ok=True)
        self._path = path
        self._lock = threading.Lock()
        # truncate log at start of new run
        self._path.write_text(
            f"=== gen_modules.py run started {time.strftime('%Y-%m-%d %H:%M:%S')} ===\n",
            encoding='utf-8'
        )

    def __call__(self, msg: str, level: str = 'INFO'):
        ts   = time.strftime('%H:%M:%S')
        line = f"[{ts}] {level:5s} {msg}"
        try:
            print(line, flush=True)
        except UnicodeEncodeError:
            print(line.encode('ascii', errors='replace').decode('ascii'), flush=True)
        with self._lock:
            with self._path.open('a', encoding='utf-8', errors='replace') as f:
                f.write(line + '\n')


# ─── main ─────────────────────────────────────────────────────────────────────

def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--pool',   default='generated',
                    help='Module pool name (default: generated)')
    ap.add_argument('--hours',  type=float, default=8.0,
                    help='Maximum run time in hours (default: 8).')
    ap.add_argument('--target', type=int,   default=500,
                    help='Target accepted modules before entering final phase (default: 500)')
    ap.add_argument('--seed',   type=int,   default=None,
                    help='RNG seed for reproducibility')
    ap.add_argument('--no-clean', action='store_true',
                    help='Skip deleting old generated modules on startup (append instead)')
    ap.add_argument('--strategy', default=None,
                    help=f'Force a single strategy. Available: {", ".join(STRATS)}')
    args = ap.parse_args()

    if args.strategy and args.strategy not in STRATS:
        ap.error(f'Unknown strategy "{args.strategy}". Available: {", ".join(STRATS)}')

    pool_dir   = Path('userinput/modules') / args.pool
    pool_dir.mkdir(parents=True, exist_ok=True)

    # ── adapt from ratings BEFORE cleanup (gen_index.json still present) ─────
    ratings_path = pool_dir / 'gallery_ratings.json'
    index_path   = pool_dir / 'gen_index.json'
    params       = Params()

    if ratings_path.exists():
        try:
            raw = json.loads(ratings_path.read_text(encoding='utf-8'))
            kept        = [name for name, v in raw.items() if v is True]
            rejected    = [name for name, v in raw.items() if v is False]
            total_rated = len(kept) + len(rejected)
            print(f"[startup] Ratings: {len(kept)} kept  {len(rejected)} rejected")

            if total_rated >= 5 and index_path.exists():
                index_meta = {
                    e['name']: e.get('strategy', '')
                    for e in json.loads(index_path.read_text(encoding='utf-8'))
                }
                strat_kept  = {s: 0 for s in params.strat_w}
                strat_total = {s: 0 for s in params.strat_w}
                for name in kept:
                    st = index_meta.get(name, '')
                    if st in strat_kept:
                        strat_kept[st]  += 1
                        strat_total[st] += 1
                for name in rejected:
                    st = index_meta.get(name, '')
                    if st in strat_total:
                        strat_total[st] += 1

                for st in params.strat_w:
                    n = strat_total[st]
                    if n >= 3:
                        keep_rate = strat_kept[st] / n
                        params.strat_w[st] = max(0.3, min(2.5, 0.3 + keep_rate * 2.2))
                        print(f"[startup]   {st:20s} kept={strat_kept[st]}/{n} "
                              f"({keep_rate:.0%}) -> w={params.strat_w[st]:.2f}")

                keep_rate_all = len(kept) / max(total_rated, 1)
                bump = +0.03 if keep_rate_all > 0.75 else (-0.03 if keep_rate_all < 0.30 else 0.0)
                params.threshold = max(0.45, min(0.88, params.threshold + bump))
                print(f"[startup] Overall keep rate {keep_rate_all:.0%} "
                      f"-> initial threshold {params.threshold:.2f}")
            elif total_rated > 0:
                print(f"[startup] Too few ratings ({total_rated}) to adapt — using defaults")
        except Exception as exc:
            print(f"[startup] WARNING: could not parse ratings: {exc}")
    else:
        print("[startup] No gallery_ratings.json — using default weights")

    # ── clean old run (keep ratings, delete everything else) ─────────────────
    KEEP_FILES = {'gallery_ratings.json'}
    if not args.no_clean:
        deleted = 0
        for f in pool_dir.iterdir():
            if f.is_file() and f.name not in KEEP_FILES:
                f.unlink()
                deleted += 1
        if deleted:
            print(f"[startup] Removed {deleted} old file(s) "
                  f"(gallery_ratings.json preserved).")

    log        = Logger(pool_dir / 'gen_log.txt')
    index_path = pool_dir / 'gen_index.json'   # fresh path after cleanup
    stop_flag  = Path('userinput/gen_stop.flag')

    log(f"Pool      : {args.pool}")
    log(f"Output    : {pool_dir.resolve()}")
    log(f"Max hours : {args.hours}  |  Target modules: {args.target}")
    log(f"Seed      : {args.seed}")
    log("Quit: type 'q' + Enter,  or  create file 'userinput/gen_stop.flag'")

    # Log the adapted weights now that the log file exists
    log(f"Initial threshold: {params.threshold:.2f}  "
        f"weights: { {s: round(w,2) for s,w in params.strat_w.items() if w != 1.0} }")

    # ── graceful exit via stdin ───────────────────────────────────────────────
    stop_evt = threading.Event()
    def _stdin():
        try:
            for line in sys.stdin:
                if line.strip().lower() == 'q':
                    log("Quit command received — finishing gracefully …")
                    stop_evt.set()
                    break
        except Exception:
            pass
    threading.Thread(target=_stdin, daemon=True).start()

    # ── init ─────────────────────────────────────────────────────────────────
    MAX_PER_STRUCT = 2   # no deco variation -> 2 variants per layout is enough

    rng        = random.Random(args.seed)
    index      = []
    seen_piece_fps: set = set()          # exact piece-stem duplicates
    struct_counts: Dict[str, int] = {}   # count of saved variants per structural layout
    accepted   = 0
    total      = 0
    run_tag    = time.strftime('%m%d%H%M')  # e.g. "06161423" — unique per run
    t_start    = time.time()
    t_deadline = t_start + args.hours * 3600
    t_last_status  = t_start
    t_last_index   = t_start

    # ── phase schedule ────────────────────────────────────────────────────────
    # Phase 1: broad sweep, accept most creative modules (fast)
    # Phase 2: raise standards, keep only high-quality designs
    # Phase 3: best-of-best; runs until time/exhaust
    phases = [
        # (min_saved, threshold_floor, description)
        (0,               0.58, "Phase 1 - broad sweep"),
        (args.target,     0.87, "Phase 2 - quality bar raised"),
        (args.target+200, 0.91, "Phase 3 - best-of-the-best"),
    ]
    phase_idx       = 0
    EXHAUST_WINDOW  = 500_000   # if no new unique module in this many candidates → advance
    last_unique_at  = 0         # total count when last unique was accepted

    log(f"Starting — initial threshold {params.threshold:.2f}")
    log(f"Phases: {[(p[0], p[1]) for p in phases]}")

    # ── generation loop ───────────────────────────────────────────────────────
    while not stop_evt.is_set():
        now = time.time()

        # ── phase advancement ─────────────────────────────────────────────────
        while phase_idx + 1 < len(phases) and accepted >= phases[phase_idx + 1][0]:
            phase_idx += 1
            new_floor = phases[phase_idx][1]
            if params.threshold < new_floor:
                params.threshold = new_floor
            log(f"*** {phases[phase_idx][2]} (threshold floor -> {new_floor:.2f}) ***")

        # ── stop conditions ───────────────────────────────────────────────────
        if now > t_deadline:
            log("Time limit reached — stopping.")
            break
        if phase_idx >= len(phases) - 1 and accepted >= phases[-1][0]:
            log(f"All phases complete ({accepted} modules saved) — stopping.")
            break
        if stop_flag.exists():
            log("Stop flag file found — stopping.")
            try: stop_flag.unlink()
            except Exception: pass
            break
        # ── exhaustion check (no new unique module found for a long time) ─────
        if total - last_unique_at > EXHAUST_WINDOW and total > EXHAUST_WINDOW:
            if phase_idx + 1 < len(phases):
                phase_idx += 1
                new_floor = phases[phase_idx][1]
                if params.threshold < new_floor:
                    params.threshold = new_floor
                last_unique_at = total   # reset so we give the new phase a chance
                log(f"Search space thinning — advancing: {phases[phase_idx][2]} "
                    f"(threshold floor -> {new_floor:.2f})")
            else:
                log(f"Search space exhausted at phase {phase_idx} after {total} candidates. Stopping.")
                break

        total += 1

        # ── generate candidate ────────────────────────────────────────────────
        strategy  = args.strategy if args.strategy else params.pick_strategy(rng)
        n_ent     = params.pick_n_entrances(rng)
        open_sides = set(rng.sample(SIDES, n_ent))

        try:
            build_fn     = STRATS[strategy]
            pieces, grid = build_fn(rng, open_sides)
        except Exception as exc:
            log(f"Strategy {strategy} raised {exc!r} — skipping", 'WARN')
            continue

        mod = Module(pool=args.pool, pieces=pieces, grid=grid,
                     strategy=strategy, gen_idx=total)

        # ── score ─────────────────────────────────────────────────────────────
        score = score_module(mod)
        params.record(score, strategy)

        # ── accept / reject ───────────────────────────────────────────────────
        if score >= params.threshold:
            sfp = structural_fp(mod)
            pfp = piece_fp(mod)

            struct_n = struct_counts.get(sfp, 0)
            is_piece_dup = pfp in seen_piece_fps

            if is_piece_dup or struct_n >= MAX_PER_STRUCT:
                pass  # duplicate or structural slot full — discard silently
            else:
                seen_piece_fps.add(pfp)
                struct_counts[sfp] = struct_n + 1
                accepted     += 1
                last_unique_at = total
                mod.name = f"gen_{run_tag}_{accepted:04d}_s{int(score*100):02d}"

                mod_path = pool_dir / f"{mod.name}.json"
                mod_path.write_text(json.dumps(mod.to_json(), indent=2))

                meta = mod.meta()
                index.append(meta)

                ent_sides = ','.join(meta['sides'])
                log(
                    f"SAVE #{accepted:4d} | score={score:.3f} "
                    f"str={strategy:18s} | ent={meta['entrances']} ({ent_sides:7s}) "
                    f"| acc={meta['breakdown']['accessibility']:.2f} "
                    f"safe={meta['breakdown']['safety']:.2f} "
                    f"creat={meta['breakdown']['creativity']:.2f} "
                    f"| pc={meta['total_pieces']:2d}"
                )

        # ── adaptation ───────────────────────────────────────────────────────
        params.maybe_adapt(log)

        # ── periodic status ───────────────────────────────────────────────────
        if now - t_last_status >= 60.0:
            elapsed    = now - t_start
            rate_cand  = total / elapsed
            rate_accept= accepted / elapsed
            ar = accepted / max(total, 1)
            eta_m = (args.target - accepted) / max(rate_accept, 1e-6) / 60
            log(
                f"STATUS total={total:6d} saved={accepted:4d} "
                f"rate={rate_cand:.0f}c/s ar={ar:.1%} "
                f"thr={params.threshold:.2f} "
                f"elapsed={elapsed/60:.0f}m ETA={eta_m:.0f}m"
            )
            t_last_status = now

        # ── save index periodically ───────────────────────────────────────────
        if now - t_last_index >= 300.0:
            index_path.write_text(json.dumps(index, indent=2))
            t_last_index = now

    # ── final save & summary ──────────────────────────────────────────────────
    stop_evt.set()
    elapsed = time.time() - t_start
    index_path.write_text(json.dumps(index, indent=2))

    log("=" * 60)
    log(f"DONE  candidates={total}  saved={accepted}  "
        f"accept_rate={accepted/max(total,1):.1%}  "
        f"elapsed={elapsed/60:.1f} min")

    # Per-strategy breakdown
    strat_stats: Dict[str, Dict] = {}
    for m in index:
        s = m['strategy']
        if s not in strat_stats:
            strat_stats[s] = {'count': 0, 'score_sum': 0.0}
        strat_stats[s]['count']     += 1
        strat_stats[s]['score_sum'] += m['score']
    for s, ss in sorted(strat_stats.items(), key=lambda x: -x[1]['count']):
        avg = ss['score_sum'] / ss['count']
        log(f"  {s:20s}  count={ss['count']:4d}  avg_score={avg:.3f}")

    log(f"Structural layouts explored : {len(struct_counts)}")
    log(f"Index   -> {index_path}")
    log(f"Modules -> {pool_dir}")
    log("=" * 60)


if __name__ == '__main__':
    main()
