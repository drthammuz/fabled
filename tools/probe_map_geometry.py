#!/usr/bin/env python3
"""
Probe placed map pieces against kenney_catalog.json mesh-derived cell grids.

For each 5×5 module slot:
  - floor at each tile centre (y≈0 walkability from catalog cells)
  - wall/open on each tile's four faces (catalog perimeter edges, rotated with yaw)

Compare module-border openings with gen_maps intent (honest_boundary_openings).

Usage:
    python tools/probe_map_geometry.py userinput/maps/debug_placement.json
    python tools/probe_map_geometry.py userinput/maps/gen_map_test.json --verbose
"""
from __future__ import annotations

import argparse
import json
import math
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Set, Tuple

sys.path.insert(0, str(Path(__file__).parent))
import gen_maps as gm  # noqa: E402
import generate_kenney_catalog as kcat  # noqa: E402

CELL = 4.0
FLOOR_HEIGHT_M = 4.5  # shared::level::MOD_H
PIT_CELL_HALF = 2.05
PIT_FLOOR_MAX_Y = 0.45
CELLS = 5
HALF = 10.0
PROBE_UP_M = 2.0        # metres above floor at wall-face centre
PROBE_ALONG_WALL_M = 2.0  # 4 m tile → midpoint of wall segment from corner
RAY_INSET_M = 0.5       # start inside tile so ray exits through own wall
RAY_MAX_M = CELL + 1.0

GLB_DIR = Path("assets/models/space")
CATALOG_PATH = GLB_DIR / "kenney_catalog.json"
OPENING_BAND = kcat.OPENING_BAND
FLOOR_BAND = kcat.FLOOR_BAND

Tri = Tuple[Tuple[float, float, float], Tuple[float, float, float], Tuple[float, float, float]]
_TRI_CACHE: Dict[str, List[Tri]] = {}
SIDES = ['N', 'W', 'S', 'E']
OPPOSITE = {'N': 'S', 'S': 'N', 'E': 'W', 'W': 'E'}
# Outward unit normal in module-local XZ (N = −Z).
SIDE_NORMAL = {'N': (0.0, -1.0), 'W': (-1.0, 0.0), 'S': (0.0, 1.0), 'E': (1.0, 0.0)}
SIDE_CYCLE = ['N', 'W', 'S', 'E']


def quantize_yaw(yaw: float) -> float:
    return round(yaw / (math.pi / 2)) * (math.pi / 2)


def yaw_steps(yaw: float) -> int:
    return int(round(quantize_yaw(yaw) / (math.pi / 2))) % 4


def rotate_xz(dx: float, dz: float, steps: int) -> Tuple[float, float]:
    for _ in range(steps % 4):
        dx, dz = dz, -dx
    return dx, dz


def rotate_side(side: str, steps: int) -> str:
    i = SIDE_CYCLE.index(side)
    return SIDE_CYCLE[(i + steps) % 4]


def module_cell_center(ix: int, iz: int) -> Tuple[float, float]:
    return -8.0 + ix * CELL, -8.0 + iz * CELL


def world_to_module_cell(mx: float, mz: float) -> Tuple[int, int]:
    return round((mx + 8.0) / 4.0), round((mz + 8.0) / 4.0)


def load_catalog() -> Dict[str, dict]:
    data = json.loads(CATALOG_PATH.read_text(encoding='utf-8'))
    return {p['stem']: p for p in data['pieces']}


def load_piece_tris(stem: str) -> List[Tri]:
    cached = _TRI_CACHE.get(stem)
    if cached is not None:
        return cached
    path = GLB_DIR / f"{stem}.glb"
    if not path.exists():
        _TRI_CACHE[stem] = []
        return []
    j, bin_data = kcat.read_glb(str(path))
    tris = kcat.extract_triangles(j, bin_data)
    _TRI_CACHE[stem] = tris
    return tris


def transform_vertex(
    v: Tuple[float, float, float],
    px: float,
    pz: float,
    py: float,
    steps: int,
) -> Tuple[float, float, float]:
    dx, dz = rotate_xz(v[0], v[2], steps)
    return (px + dx, py + v[1], pz + dz)


def build_module_tris(pieces: List[gm.PlacedPiece]) -> List[Tri]:
    """Merge all placed GLB triangles in module-local coordinates."""
    out: List[Tri] = []
    for p in pieces:
        local = load_piece_tris(p.stem)
        if not local:
            continue
        steps = yaw_steps(p.yaw)
        py = p.floor_level * FLOOR_HEIGHT_M
        for tri in local:
            out.append(tuple(transform_vertex(v, p.x, p.z, py, steps) for v in tri))  # type: ignore[misc]
    return out


def filter_pit_floor_tris(tris: List[Tri], ex: float, ez: float) -> List[Tri]:
    """Drop room-shell floor triangles in the centre trap tile (floor 0)."""
    return filter_pit_floor_tris_at(tris, ex, ez, PIT_FLOOR_MAX_Y)


def filter_pit_floor_tris_at(
    tris: List[Tri], tx: float, tz: float, floor_top_y: float,
) -> List[Tri]:
    """Drop room-shell floor triangles in a trap tile at arbitrary floor height."""
    out: List[Tri] = []
    for tri in tris:
        if all(
            abs(v[0] - tx) <= PIT_CELL_HALF
            and abs(v[2] - tz) <= PIT_CELL_HALF
            and v[1] < floor_top_y
            for v in tri
        ):
            continue
        out.append(tri)
    return out


def mesh_floor_at(cx: float, cz: float, tris: List[Tri]) -> bool:
    hits = kcat.vertical_ray_hits(cx, cz, tris)
    return any(kcat.in_band(h, FLOOR_BAND) for h in hits)


def wall_face_midpoint(ix: int, iz: int, side: str) -> Tuple[float, float, float]:
    """
    Centre of this tile's outward wall panel: PROBE_ALONG_WALL_M from the
    along-wall corner (midpoint of 4 m edge), PROBE_UP_M above floor.
    """
    cx, cz = module_cell_center(ix, iz)
    nx, nz = SIDE_NORMAL[side]
    ex = cx + nx * (CELL * 0.5)
    ez = cz + nz * (CELL * 0.5)
    return ex, PROBE_UP_M, ez


def ray_triangle(
    ox: float, oy: float, oz: float,
    dx: float, dy: float, dz: float,
    max_t: float,
    tri: Tri,
) -> Optional[float]:
    eps = 1e-7
    v0, v1, v2 = tri
    e1 = (v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2])
    e2 = (v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2])
    h = (dy * e2[2] - dz * e2[1], dz * e2[0] - dx * e2[2], dx * e2[1] - dy * e2[0])
    a = e1[0] * h[0] + e1[1] * h[1] + e1[2] * h[2]
    if abs(a) < eps:
        return None
    f = 1.0 / a
    s = (ox - v0[0], oy - v0[1], oz - v0[2])
    u = f * (s[0] * h[0] + s[1] * h[1] + s[2] * h[2])
    if u < 0.0 or u > 1.0:
        return None
    q = (s[1] * e1[2] - s[2] * e1[1], s[2] * e1[0] - s[0] * e1[2], s[0] * e1[1] - s[1] * e1[0])
    v = f * (dx * q[0] + dy * q[1] + dz * q[2])
    if v < 0.0 or u + v > 1.0:
        return None
    t = f * (e2[0] * q[0] + e2[1] * q[1] + e2[2] * q[2])
    if t < eps or t > max_t:
        return None
    return t


def mesh_face_blocked(ix: int, iz: int, side: str, tris: List[Tri]) -> bool:
    """Horizontal ray at chest height through wall-face centre, outward."""
    cx, cz = module_cell_center(ix, iz)
    nx, nz = SIDE_NORMAL[side]
    ox = cx - nx * RAY_INSET_M
    oy = PROBE_UP_M
    oz = cz - nz * RAY_INSET_M
    nearest = RAY_MAX_M
    for tri in tris:
        t = ray_triangle(ox, oy, oz, nx, 0.0, nz, RAY_MAX_M, tri)
        if t is not None and t < nearest:
            nearest = t
    if nearest >= RAY_MAX_M - 1e-6:
        return False
    hy = oy
    return kcat.in_band(hy, OPENING_BAND) or abs(hy - PROBE_UP_M) < 1.0


@dataclass
class ModuleGeom:
    """Merged catalog geometry for one module (local coords, centre at 0,0)."""
    floor: List[List[bool]] = field(default_factory=lambda: [[False] * CELLS for _ in range(CELLS)])
    # Per-cell outward face: None | 'open' | 'wall'
    face: List[List[Dict[str, Optional[str]]]] = field(
        default_factory=lambda: [
            [{'N': None, 'S': None, 'E': None, 'W': None} for _ in range(CELLS)]
            for _ in range(CELLS)
        ]
    )

    def set_floor(self, ix: int, iz: int, cell_type: str) -> None:
        if not (0 <= ix < CELLS and 0 <= iz < CELLS):
            return
        if cell_type == 'floor':
            self.floor[iz][ix] = True
        elif cell_type == 'wall':
            self.floor[iz][ix] = False

    def set_face(self, ix: int, iz: int, side: str, val: str) -> None:
        if not (0 <= ix < CELLS and 0 <= iz < CELLS):
            return
        if val not in ('open', 'wall'):
            return
        cur = self.face[iz][ix][side]
        if cur == 'wall' or val == 'wall':
            self.face[iz][ix][side] = 'wall'
        elif cur is None:
            self.face[iz][ix][side] = val

    def border_open_tiles(self, side: str) -> Set[int]:
        """Tile indices along a module outer face that are open (catalog)."""
        out: Set[int] = set()
        if side in ('N', 'S'):
            iz = 0 if side == 'N' else CELLS - 1
            for ix in range(CELLS):
                if self.face[iz][ix][side] == 'open':
                    out.add(ix)
        else:
            ix = 0 if side == 'W' else CELLS - 1
            for iz in range(CELLS):
                if self.face[iz][ix][side] == 'open':
                    out.add(iz)
        return out

    def border_openings(self) -> Dict[str, Set[int]]:
        return {s: self.border_open_tiles(s) for s in SIDES}

    def ascii_border(self, side: str) -> str:
        tiles = self.border_open_tiles(side)
        return ''.join(str(i) if i in tiles else '.' for i in range(CELLS))


def stamp_piece(geom: ModuleGeom, stem: str, px: float, pz: float, yaw: float, cat: Dict[str, dict]) -> None:
    # template-wall: catalog "open" edge faces the room interior; on module perimeter
    # we only care that the outward face reads as wall.
    if stem == 'template-wall':
        if abs(px + HALF) < 0.1:
            iz = round((pz + 8.0) / CELL)
            if 0 <= iz < CELLS:
                geom.set_face(0, iz, 'W', 'wall')
        elif abs(px - HALF) < 0.1:
            iz = round((pz + 8.0) / CELL)
            if 0 <= iz < CELLS:
                geom.set_face(CELLS - 1, iz, 'E', 'wall')
        elif abs(pz + HALF) < 0.1:
            ix = round((px + 8.0) / CELL)
            if 0 <= ix < CELLS:
                geom.set_face(ix, 0, 'N', 'wall')
        elif abs(pz - HALF) < 0.1:
            ix = round((px + 8.0) / CELL)
            if 0 <= ix < CELLS:
                geom.set_face(ix, CELLS - 1, 'S', 'wall')
        return

    piece = cat.get(stem)
    if not piece:
        return
    cg = piece.get('cell_grid')
    if not cg:
        return

    nx0, nz0 = int(cg['nx']), int(cg['nz'])
    steps = yaw_steps(yaw)
    sw_u_x = px - nx0 * CELL * 0.5
    sw_u_z = pz - nz0 * CELL * 0.5
    cells = cg['cells']
    edges = cg['edges']

    for row in range(nz0):
        for col in range(nx0):
            ctype = cells[row][col]
            lx = sw_u_x + (col + 0.5) * CELL
            lz = sw_u_z + (row + 0.5) * CELL
            dx, dz = lx - px, lz - pz
            rdx, rdz = rotate_xz(dx, dz, steps)
            ix, iz = world_to_module_cell(px + rdx, pz + rdz)
            geom.set_floor(ix, iz, ctype)

    def stamp_edge(col: int, row: int, side: str, val: str) -> None:
        if side == 'S':
            lx = sw_u_x + (col + 0.5) * CELL
            lz = sw_u_z + 0.5 * CELL
        elif side == 'N':
            lx = sw_u_x + (col + 0.5) * CELL
            lz = sw_u_z + (nz0 - 0.5) * CELL
        elif side == 'W':
            lx = sw_u_x + 0.5 * CELL
            lz = sw_u_z + (row + 0.5) * CELL
        else:  # E
            lx = sw_u_x + (nx0 - 0.5) * CELL
            lz = sw_u_z + (row + 0.5) * CELL
        dx, dz = lx - px, lz - pz
        rdx, rdz = rotate_xz(dx, dz, steps)
        ix, iz = world_to_module_cell(px + rdx, pz + rdz)
        geom.set_face(ix, iz, rotate_side(side, steps), val)

    for col, val in enumerate(edges.get('south', [])):
        stamp_edge(col, 0, 'S', val)
    for col, val in enumerate(edges.get('north', [])):
        stamp_edge(col, nz0 - 1, 'N', val)
    for row, val in enumerate(edges.get('west', [])):
        stamp_edge(0, row, 'W', val)
    for row, val in enumerate(edges.get('east', [])):
        stamp_edge(nx0 - 1, row, 'E', val)


def catalog_border_openings(pieces: List[gm.PlacedPiece], cat: Dict[str, dict]) -> Dict[str, Set[int]]:
    return build_module_geom(pieces, cat).border_openings()


def close_for_placement_catalog(
    pieces: List[gm.PlacedPiece],
    required: Set[str],
    keep_tile: Optional[Dict[str, int]],
    cat: Dict[str, dict],
) -> List[gm.PlacedPiece]:
    """Wall every catalog-open segment except the single link tile on each required side."""
    keep_tile = keep_tile or {}
    out = list(pieces)
    geom = build_module_geom(out, cat)
    for side in SIDES:
        if side in required:
            allowed = {keep_tile.get(side, gm.CENTER_TILE)}
        else:
            allowed = set()
        for tile in sorted(geom.border_open_tiles(side) - allowed):
            out.append(gm.wall_for_opening(side, tile))
    return out


def probe_point_has_wall(
    geom: ModuleGeom,
    ix: int,
    iz: int,
    side: str,
    tris: List[Tri],
) -> Optional[bool]:
    """
    Mesh raycast at wall-face centre (2 m along wall, 2 m up).
    Horizontal ray outward from inside the tile. True=wall, False=open.
    """
    if not (0 <= ix < CELLS and 0 <= iz < CELLS):
        return None
    cx, cz = module_cell_center(ix, iz)
    if not mesh_floor_at(cx, cz, tris):
        return None
    return mesh_face_blocked(ix, iz, side, tris)


def mesh_border_open_tiles(side: str, tris: List[Tri]) -> Set[int]:
    """Which tiles along a module face are open per merged mesh rays."""
    out: Set[int] = set()
    if side in ('N', 'S'):
        iz = 0 if side == 'N' else CELLS - 1
        for ix in range(CELLS):
            w = probe_point_has_wall(ModuleGeom(), ix, iz, side, tris)
            if w is False:
                out.add(ix)
    else:
        ix = 0 if side == 'W' else CELLS - 1
        for iz in range(CELLS):
            w = probe_point_has_wall(ModuleGeom(), ix, iz, side, tris)
            if w is False:
                out.add(iz)
    return out


def mesh_border_openings(tris: List[Tri]) -> Dict[str, Set[int]]:
    return {s: mesh_border_open_tiles(s, tris) for s in SIDES}


def border_exits_for_pieces(pieces: List[gm.PlacedPiece]) -> Dict[str, Set[int]]:
    """Authoritative per-face open tile indices from merged GLB mesh rays."""
    tris = build_module_tris(pieces)
    if not tris:
        return {s: set() for s in SIDES}
    return mesh_border_openings(tris)


def border_exits_to_json(exits: Dict[str, Set[int]]) -> Dict[str, List[int]]:
    return {s: sorted(tiles) for s, tiles in exits.items() if tiles}


def border_exits_from_json(data: dict) -> Optional[Dict[str, Set[int]]]:
    raw = data.get("border_exits")
    if raw is None:
        return None
    return {s: set(int(t) for t in tiles) for s, tiles in raw.items()}


def close_for_placement_mesh(
    pieces: List[gm.PlacedPiece],
    required: Set[str],
    keep_tile: Optional[Dict[str, int]],
) -> List[gm.PlacedPiece]:
    """Wall every mesh-open segment except the kept link tile on each required side."""
    keep_tile = keep_tile or {}
    out = list(pieces)
    opens = border_exits_for_pieces(out)
    for side in SIDES:
        if side in required:
            allowed = {keep_tile.get(side, gm.CENTER_TILE)}
        else:
            allowed = set()
        for tile in sorted(opens.get(side, set()) - allowed):
            out.append(gm.wall_for_opening(side, tile))
    return out


def build_module_geom(pieces: List[gm.PlacedPiece], cat: Dict[str, dict]) -> ModuleGeom:
    geom = ModuleGeom()
    for p in pieces:
        stamp_piece(geom, p.stem, p.x, p.z, p.yaw, cat)
    return geom


def expected_border_openings(required_sides: Set[str]) -> Dict[str, Set[int]]:
    return {s: ({gm.CENTER_TILE} if s in required_sides else set()) for s in SIDES}


def load_expected_exits(path: Path) -> Dict[gm.Slot, Dict[str, Set[int]]]:
    data = json.loads(path.read_text(encoding='utf-8'))
    raw = data.get('module_exits', {})
    out: Dict[gm.Slot, Dict[str, Set[int]]] = {}
    for key, sides in raw.items():
        col_s, row_s = key.split(',')
        slot = (int(col_s), int(row_s))
        out[slot] = {s: set(tiles) for s, tiles in sides.items()}
    return out


def load_map_modules(path: Path) -> Dict[gm.Slot, List[gm.PlacedPiece]]:
    data = json.loads(path.read_text(encoding='utf-8'))
    by_gid: Dict[int, List[dict]] = {}
    for p in data.get('pieces', []):
        by_gid.setdefault(int(p.get('group_id', 0)), []).append(p)

    by_slot: Dict[gm.Slot, List[gm.PlacedPiece]] = {}
    for group in by_gid.values():
        if not group:
            continue
        ax = sum(float(p['x']) for p in group) / len(group)
        az = sum(float(p['z']) for p in group) / len(group)
        slot = gm.slot_from_world(ax, az)
        mcx, mcz = gm.module_center(*slot)
        local = [
            gm.PlacedPiece(
                stem=p['stem'], x=float(p['x']) - mcx, z=float(p['z']) - mcz,
                yaw=float(p['yaw']), floor_level=int(p.get('floor_level', 0)),
                scale=float(p.get('scale', 1.0)),
            )
            for p in group
        ]
        by_slot.setdefault(slot, []).extend(local)
    return by_slot


def compare_module(
    slot: gm.Slot,
    pieces: List[gm.PlacedPiece],
    cat: Dict[str, dict],
    verbose: bool,
    expected: Optional[Dict[str, Set[int]]] = None,
) -> List[str]:
    geom = build_module_geom(pieces, cat)
    tris = build_module_tris(pieces)
    catalog_open = geom.border_openings()
    mesh_open = mesh_border_openings(tris) if tris else {s: set() for s in SIDES}
    issues: List[str] = []

    if verbose:
        print(f"\n--- slot {slot} ---")
        print("floor (iz=0 north .. iz=4 south):")
        for iz in range(CELLS):
            row = ''.join('.' if not geom.floor[iz][ix] else 'F' for ix in range(CELLS))
            print(f"  iz={iz} {row}")

    for side in SIDES:
        cat_tiles = catalog_open.get(side, set())
        mesh_tiles = mesh_open.get(side, set())
        wanted = expected.get(side, set()) if expected else mesh_tiles
        if mesh_tiles != wanted:
            mesh_pat = ''.join(str(i) if i in mesh_tiles else '.' for i in range(CELLS))
            issues.append(
                f"{slot} {side}: MESH {mesh_tiles} vs expected {wanted} [{mesh_pat}]"
            )
        if verbose:
            mesh_pat = ''.join(str(i) if i in mesh_tiles else '.' for i in range(CELLS))
            print(
                f"  {side} mesh [{mesh_pat}] expected {wanted} "
                f"(catalog [{geom.ascii_border(side)}])"
            )

    if verbose and tris:
        print("  border mesh probes (floor tile required):")
        mcx, mcz = gm.module_center(*slot)
        for side in SIDES:
            for iz in range(CELLS):
                for ix in range(CELLS):
                    on = (
                        (side == 'N' and iz == 0) or (side == 'S' and iz == CELLS - 1)
                        or (side == 'W' and ix == 0) or (side == 'E' and ix == CELLS - 1)
                    )
                    if not on:
                        continue
                    w = probe_point_has_wall(geom, ix, iz, side, tris)
                    if w is None:
                        continue
                    idx = ix if side in ('N', 'S') else iz
                    cx, cz = module_cell_center(ix, iz)
                    fx, fy, fz = wall_face_midpoint(ix, iz, side)
                    print(
                        f"    {side} tile {idx} cell({ix},{iz}) "
                        f"world({mcx+cx:.0f},{mcz+cz:.0f}) "
                        f"face({mcx+fx:.0f},{fy:.1f},{mcz+fz:.0f}) "
                        f"mesh={'wall' if w else 'open'}"
                    )

    return issues


def compare_adjacent(
    slot_a: gm.Slot,
    mesh_a: Dict[str, Set[int]],
    slot_b: gm.Slot,
    mesh_b: Dict[str, Set[int]],
    side: str,
) -> List[str]:
    """Check shared border: mesh openings must match on both sides."""
    issues: List[str] = []
    opp = OPPOSITE[side]
    open_a = mesh_a.get(side, set())
    open_b = mesh_b.get(opp, set())
    if open_a != open_b:
        issues.append(
            f"ADJACENT {slot_a}.{side} {open_a} <-> {slot_b}.{opp} {open_b}"
        )
    return issues


def probe_map(path: Path, verbose: bool = False) -> List[str]:
    if not CATALOG_PATH.exists():
        return [f"missing catalog: {CATALOG_PATH}"]
    cat = load_catalog()
    modules = load_map_modules(path)
    expected_all = load_expected_exits(path)
    all_issues: List[str] = []

    print(f"Probing {path.name}: {len(modules)} modules (mesh vs intent)")

    mesh_by_slot = {
        s: border_exits_for_pieces(p) for s, p in modules.items()
    }

    for slot, pieces in sorted(modules.items()):
        all_issues.extend(
            compare_module(slot, pieces, cat, verbose, expected_all.get(slot))
        )

    for slot, mesh in mesh_by_slot.items():
        col, row = slot
        for side in ('E', 'S'):
            nc, nr = gm.neighbor_slot(col, row, side)
            nslot = (nc, nr)
            if nslot in mesh_by_slot:
                all_issues.extend(
                    compare_adjacent(slot, mesh, nslot, mesh_by_slot[nslot], side)
                )

    return all_issues


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('map', type=Path, help='Map JSON path')
    ap.add_argument('--verbose', '-v', action='store_true')
    args = ap.parse_args()

    issues = probe_map(args.map, args.verbose)
    if not issues:
        print("OK — mesh geometry matches intent on all module borders.")
    else:
        print(f"{len(issues)} mismatch(es):")
        for line in issues:
            print(f"  · {line}")
        sys.exit(1)


if __name__ == '__main__':
    main()
