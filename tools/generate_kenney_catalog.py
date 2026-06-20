#!/usr/bin/env python3
"""Generate a Kenney kit catalogue (kenney_catalog.json) from GLB bounds + mesh cell grids.

Floor cells: vertical ray through (x, z) — hit in y≈0 band counts as floor (hollow rooms).
Edge openings: ray hits absent in player-height band on outer perimeter segments.

Works on any kit folder under assets/models/ that uses the 4 m modular grammar
(space, dungeon, …). The mesh measurement is per-GLB; MANUAL only supplies
category/role/open_faces/purpose, which transfer across kits that share stem names.

Run from repo root:
  python tools/generate_kenney_catalog.py            # default kit: space
  python tools/generate_kenney_catalog.py --kit dungeon
"""

from __future__ import annotations

import argparse
import json
import os
import struct
import glob
from typing import Any

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

GRID_UNIT = 4.0
OPENING_W = 4.0
OPENING_H = 4.25
CEILING = 4.25
FLOOR_PLANE_Y = 0.0
FLOOR_BAND = (-0.25, 0.5)
PLAYER_BAND = (1.0, 3.9)
OPENING_BAND = (1.8, 3.2)  # edge openings: ignore ceiling (y>3.5) and floor lip (y<1.5)
RAMP_Y_MAX = 4.6

# Curated metadata (category, open_faces, …). Keys = stem without .glb.
MANUAL: dict[str, dict[str, Any]] = {
    "corridor": {
        "category": "corridor",
        "role": "connector",
        "open_faces": ["south", "north"],
        "purpose": "Straight 4 m corridor; chains end-to-end.",
    },
    "corridor-corner": {
        "category": "corridor",
        "role": "connector",
        "open_faces": ["south", "east"],
        "purpose": "90° L-turn (4 m lanes).",
    },
    "corridor-end": {
        "category": "corridor",
        "role": "connector",
        "open_faces": ["north"],
        "purpose": "Dead-end cap; one open face opposite the blank wall.",
    },
    "corridor-intersection": {
        "category": "corridor",
        "role": "connector",
        "open_faces": ["south", "north", "east", "west"],
        "purpose": "4-way cross (4 m lanes).",
    },
    "corridor-junction": {
        "category": "corridor",
        "role": "connector",
        "open_faces": ["south", "north", "east"],
        "purpose": "T-junction (3 openings).",
    },
    "corridor-transition": {
        "category": "corridor",
        "role": "adapter",
        "open_faces": ["south", "north", "east"],
        "purpose": "Adapts 4 m lane to 8 m wide corridor (8×8 footprint).",
    },
    "corridor-wide": {
        "category": "corridor_wide",
        "role": "connector",
        "open_faces": ["south", "north"],
        "purpose": "Straight 8 m wide corridor segment.",
    },
    "corridor-wide-corner": {
        "category": "corridor_wide",
        "role": "connector",
        "open_faces": ["south", "east"],
        "purpose": "90° L-turn (8 m lanes).",
    },
    "corridor-wide-end": {
        "category": "corridor_wide",
        "role": "connector",
        "open_faces": ["north"],
        "purpose": "Wide dead-end cap.",
    },
    "corridor-wide-intersection": {
        "category": "corridor_wide",
        "role": "connector",
        "open_faces": ["south", "north", "east", "west"],
        "purpose": "4-way cross (8 m lanes).",
    },
    "corridor-wide-junction": {
        "category": "corridor_wide",
        "role": "connector",
        "open_faces": ["south", "north", "east"],
        "purpose": "T-junction (8 m lanes).",
    },
    "room-small": {
        "category": "room",
        "role": "module",
        "open_faces": ["south", "north", "east", "west"],
        "open_slots": ["C"],
        "purpose": "12×12 m room; one centred 4 m opening per wall (C-slot).",
    },
    "room-small-variation": {
        "category": "room",
        "role": "module",
        "variant_of": "room-small",
        "open_faces": ["south", "north", "east", "west"],
        "open_slots": ["C"],
        "purpose": "room-small with alternate interior trim.",
    },
    "room-corner": {
        "category": "room",
        "role": "module",
        "open_faces": ["south", "east"],
        "purpose": "L-shaped 12×12 m room.",
    },
    "room-large": {
        "category": "room",
        "role": "module",
        "open_faces": ["south", "north", "east", "west"],
        "open_slots": ["C"],
        "purpose": "20×20 m large room (5×5 grid units).",
    },
    "room-large-variation": {
        "category": "room",
        "role": "module",
        "variant_of": "room-large",
        "open_faces": ["south", "north", "east", "west"],
        "open_slots": ["C"],
        "purpose": "room-large with alternate interior trim.",
    },
    "room-wide": {
        "category": "room",
        "role": "module",
        "open_faces": ["south", "north", "east", "west"],
        "open_slots": ["C"],
        "purpose": "20×12 m rectangular room.",
    },
    "room-wide-variation": {
        "category": "room",
        "role": "module",
        "variant_of": "room-wide",
        "open_faces": ["south", "north", "east", "west"],
        "open_slots": ["C"],
        "purpose": "room-wide with alternate interior trim.",
    },
    "stairs": {
        "category": "stairs",
        "role": "vertical",
        "open_faces": ["south", "north"],
        "stairs": {"entry_z": -6.1, "landing_z": 2.1, "rise_m": 4.35, "width_m": 4.0},
        "purpose": "4 m wide stair run between floors.",
    },
    "stairs-wide": {
        "category": "stairs",
        "role": "vertical",
        "variant_of": "stairs",
        "open_faces": ["south", "north"],
        "stairs": {"entry_z": -6.1, "landing_z": 2.1, "rise_m": 4.35, "width_m": 8.0},
        "purpose": "8 m wide stair run between floors.",
    },
    "gate": {
        "category": "gate",
        "role": "door",
        "open_faces": ["south", "north"],
        "purpose": "Gate frame (no animated door panel).",
    },
    "gate-door": {
        "category": "gate",
        "role": "door",
        "open_faces": ["south", "north"],
        "purpose": "Animated door frame; straddles wall plane.",
    },
    "gate-door-window": {
        "category": "gate",
        "role": "door",
        "variant_of": "gate-door",
        "open_faces": ["south", "north"],
        "purpose": "Door frame with window insert.",
    },
    "gate-metal-bars": {
        "category": "gate",
        "role": "door",
        "open_faces": ["south", "north"],
        "purpose": "Barred gate (dungeon kit); straddles wall plane like gate-door.",
    },
    "gate-lasers": {
        "category": "gate",
        "role": "door_overlay",
        "collide_default": False,
        "purpose": "Laser-bar overlay for gate-door.",
    },
    "gate-lasers-edited": {
        "category": "gate",
        "role": "door_overlay",
        "variant_of": "gate-lasers",
        "collide_default": False,
        "purpose": "Edited laser overlay variant.",
    },
    "cables": {
        "category": "prop",
        "role": "decoration",
        "collide_default": False,
        "purpose": "Floor/ceiling cable bundle.",
    },
    "template-wall": {
        "category": "template_wall",
        "role": "wall_tile",
        "footprint_override": {"x": 4.0, "z": 1.0},
        "purpose": "4 m × 1 m wall panel (depth along -Z).",
    },
    "template-wall-half": {
        "category": "template_wall",
        "role": "wall_tile",
        "footprint_override": {"x": 2.0, "z": 1.0},
        "purpose": "2 m half-wall panel.",
    },
    "template-wall-corner": {
        "category": "template_wall",
        "role": "wall_tile",
        "footprint_override": {"x": 1.0, "z": 1.0},
        "purpose": "1 m corner wall filler.",
    },
    "template-wall-detail-a": {
        "category": "template_wall",
        "role": "wall_tile",
        "footprint_override": {"x": 4.0, "z": 1.37},
        "purpose": "4 m wall with surface detail.",
    },
    "template-wall-stairs": {
        "category": "template_wall",
        "role": "wall_tile",
        "footprint_override": {"x": 4.2, "z": 0.78},
        "purpose": "Wall segment with stair-well cutout.",
    },
    "template-wall-top": {
        "category": "template_wall",
        "role": "wall_tile",
        "footprint_override": {"x": 4.2, "z": 0.78},
        "purpose": "Upper wall / railing segment.",
    },
    "template-corner": {
        "category": "template",
        "role": "trim",
        "purpose": "4×4 m corner trim block.",
    },
    "template-detail": {
        "category": "template",
        "role": "trim",
        "purpose": "Small 1.56 m surface detail prop.",
    },
    "template-floor": {
        "category": "template_floor",
        "role": "floor_tile",
        "purpose": "4×4 m flat floor tile.",
    },
    "template-floor-big": {
        "category": "template_floor",
        "role": "floor_tile",
        "purpose": "8×8 m floor tile.",
    },
    "template-floor-detail": {
        "category": "template_floor",
        "role": "floor_tile",
        "purpose": "4×4 m floor with shallow detail.",
    },
    "template-floor-detail-a": {
        "category": "template_floor",
        "role": "floor_tile",
        "purpose": "4×4 m floor detail variant A.",
    },
    "template-floor-hole": {
        "category": "template_floor",
        "role": "floor_tile",
        "purpose": "4×4 m floor tile with opening/hole.",
    },
    "template-floor-layer": {
        "category": "template_floor",
        "role": "floor_layer",
        "footprint_override": {"x": 4.2, "z": 4.2},
        "purpose": "Thin floor layer slab (0.4 m tall).",
    },
    "template-floor-layer-raised": {
        "category": "template_floor",
        "role": "floor_layer",
        "footprint_override": {"x": 4.2, "z": 4.2},
        "purpose": "Raised floor layer (3.4 m tall).",
    },
    "template-floor-layer-hole": {
        "category": "template_floor",
        "role": "floor_layer",
        "footprint_override": {"x": 4.2, "z": 4.2},
        "purpose": "Floor layer with opening/hole.",
    },
}

COMP_TYPES = {5120: "b", 5121: "B", 5122: "h", 5123: "H", 5125: "I", 5126: "f"}
COMP_SIZE = {5120: 1, 5121: 1, 5122: 2, 5123: 2, 5125: 4, 5126: 4}


def read_glb(path: str) -> tuple[dict, bytes]:
    with open(path, "rb") as f:
        magic, _, _ = struct.unpack("<4sII", f.read(12))
        if magic != b"glTF":
            raise ValueError("not glTF")
        jlen, _ = struct.unpack("<I4s", f.read(8))
        j = json.loads(f.read(jlen))
        blen, _ = struct.unpack("<I4s", f.read(8))
        bin_data = f.read(blen)
    return j, bin_data


def read_accessor_vec3(j: dict, bin_data: bytes, acc_idx: int) -> list[tuple[float, float, float]]:
    acc = j["accessors"][acc_idx]
    bv = j["bufferViews"][acc["bufferView"]]
    start = bv.get("byteOffset", 0) + acc.get("byteOffset", 0)
    count = acc["count"]
    stride = bv.get("byteStride") or 12
    out = []
    for i in range(count):
        off = start + i * stride
        out.append(struct.unpack_from("<fff", bin_data, off))
    return out


def read_indices(j: dict, bin_data: bytes, acc_idx: int) -> list[int]:
    acc = j["accessors"][acc_idx]
    bv = j["bufferViews"][acc["bufferView"]]
    start = bv.get("byteOffset", 0) + acc.get("byteOffset", 0)
    count = acc["count"]
    ct = acc["componentType"]
    esize = COMP_SIZE[ct]
    stride = bv.get("byteStride") or esize
    fmt = "<" + COMP_TYPES[ct]
    return [struct.unpack_from(fmt, bin_data, start + i * stride)[0] for i in range(count)]


def extract_triangles(j: dict, bin_data: bytes) -> list[tuple[tuple[float, float, float], ...]]:
    tris: list[tuple[tuple[float, float, float], ...]] = []
    for mesh in j.get("meshes", []):
        for prim in mesh.get("primitives", []):
            verts = read_accessor_vec3(j, bin_data, prim["attributes"]["POSITION"])
            idx = prim.get("indices")
            if idx is None:
                for i in range(0, len(verts), 3):
                    if i + 2 < len(verts):
                        tris.append((verts[i], verts[i + 1], verts[i + 2]))
                continue
            indices = read_indices(j, bin_data, idx)
            for t in range(0, len(indices) - 2, 3):
                tris.append((verts[indices[t]], verts[indices[t + 1]], verts[indices[t + 2]]))
    return tris


def xz_bary(px: float, pz: float, tri: tuple[tuple[float, float, float], ...]) -> tuple[float, float, float] | None:
    v0, v1, v2 = tri
    x0, z0 = v0[0], v0[2]
    x1, z1 = v1[0], v1[2]
    x2, z2 = v2[0], v2[2]
    d00 = (x1 - x0) ** 2 + (z1 - z0) ** 2
    d01 = (x1 - x0) * (x2 - x0) + (z1 - z0) * (z2 - z0)
    d11 = (x2 - x0) ** 2 + (z2 - z0) ** 2
    d20 = (px - x0) * (x1 - x0) + (pz - z0) * (z1 - z0)
    d21 = (px - x0) * (x2 - x0) + (pz - z0) * (z2 - z0)
    denom = d00 * d11 - d01 * d01
    if abs(denom) < 1e-12:
        return None
    v = (d11 * d20 - d01 * d21) / denom
    w = (d00 * d21 - d01 * d20) / denom
    u = 1.0 - v - w
    if u < -0.02 or v < -0.02 or w < -0.02:
        return None
    return u, v, w


def vertical_ray_hits(px: float, pz: float, tris: list) -> list[float]:
    hits: list[float] = []
    for tri in tris:
        bw = xz_bary(px, pz, tri)
        if bw is None:
            continue
        u, v, w = bw
        y = u * tri[0][1] + v * tri[1][1] + w * tri[2][1]
        hits.append(y)
    return hits


def in_band(y: float, band: tuple[float, float]) -> bool:
    return band[0] <= y <= band[1]


def classify_point(px: float, pz: float, tris: list, category: str, role: str) -> str:
    hits = vertical_ray_hits(px, pz, tris)
    if not hits:
        return "void"

    floor_at_y0 = any(in_band(h, FLOOR_BAND) for h in hits)
    player_blocks = [h for h in hits if in_band(h, PLAYER_BAND)]

    if category == "stairs":
        ramp_hits = [h for h in hits if FLOOR_BAND[0] <= h <= RAMP_Y_MAX]
        if ramp_hits and (max(ramp_hits) - min(ramp_hits) > 0.25 or max(ramp_hits) > 0.6):
            return "stairs"
        if floor_at_y0 and len(player_blocks) < 2:
            return "stairs"

    if role == "door_overlay":
        return "door"

    if role == "door":
        return "door" if player_blocks else "void"

    if role == "decoration":
        return "prop" if floor_at_y0 else "void"

    if category in ("template_wall",) or role == "wall_tile":
        if player_blocks:
            return "wall"
        if floor_at_y0:
            return "wall"
        return "void"

    if category == "template_floor" and role == "floor_layer":
        if not floor_at_y0 and not player_blocks:
            return "hole"

    blocked = len(player_blocks) >= 2
    if floor_at_y0 and not blocked:
        return "floor"
    if blocked:
        return "wall"
    if floor_at_y0:
        return "floor"
    return "void"


def stem_is_hole_layer(category: str, role: str) -> bool:
    return False  # set per-piece below via stem param — patched in classify_cell


def sample_points_in_cell(xa: float, xb: float, za: float, zb: float) -> list[tuple[float, float]]:
    mx, mz = (xa + xb) * 0.5, (za + zb) * 0.5
    inset = 0.35
    return [
        (mx, mz),
        (xa + inset, za + inset),
        (xb - inset, za + inset),
        (xa + inset, zb - inset),
        (xb - inset, zb - inset),
    ]


def majority(labels: list[str]) -> str:
    if not labels:
        return "void"
    order = ["floor", "stairs", "prop", "door", "wall", "hole", "void"]
    counts: dict[str, int] = {}
    for lb in labels:
        counts[lb] = counts.get(lb, 0) + 1
    best = max(counts, key=lambda k: (counts[k], -order.index(k) if k in order else 99))
    return best


def classify_cell(
    xa: float,
    xb: float,
    za: float,
    zb: float,
    tris: list,
    category: str,
    role: str,
    stem: str,
) -> str:
    if stem == "template-floor-layer-hole":
        pts = sample_points_in_cell(xa, xb, za, zb)
        labels = []
        for px, pz in pts:
            hits = vertical_ray_hits(px, pz, tris)
            floor_hit = any(in_band(h, FLOOR_BAND) for h in hits)
            deep = any(h < -0.5 for h in hits)
            if not hits or deep:
                labels.append("hole")
            elif floor_hit:
                labels.append("floor")
            else:
                labels.append("void")
        return majority(labels)

    mx, mz = (xa + xb) * 0.5, (za + zb) * 0.5
    center = classify_point(mx, mz, tris, category, role)
    # Hollow rooms: floor exists at y≈0 under the cell centre; corners hit perimeter walls.
    if center in ("floor", "stairs", "prop", "door", "hole"):
        return center
    if center == "wall":
        return "wall"

    pts = sample_points_in_cell(xa, xb, za, zb)
    labels = [classify_point(px, pz, tris, category, role) for px, pz in pts if (px, pz) != (mx, mz)]
    return majority(labels) if labels else "void"


def edge_segment_open(
    x0: float,
    z0: float,
    x1: float,
    z1: float,
    tris: list,
    samples: int = 7,
) -> str:
    open_count = 0
    wall_count = 0
    for i in range(samples):
        t = (i + 0.5) / samples
        px = x0 + (x1 - x0) * t
        pz = z0 + (z1 - z0) * t
        hits = vertical_ray_hits(px, pz, tris)
        if not hits:
            wall_count += 1
            continue
        blocked = any(in_band(h, OPENING_BAND) for h in hits)
        if blocked:
            wall_count += 1
        else:
            open_count += 1
    return "open" if open_count > wall_count else "wall"


def build_cell_grid(
    stem: str,
    tris: list,
    fp_x: float,
    fp_z: float,
    meta: dict,
) -> dict[str, Any]:
    category = meta.get("category", "unknown")
    role = meta.get("role", "unknown")
    nx = max(1, int(round(fp_x / GRID_UNIT)))
    nz = max(1, int(round(fp_z / GRID_UNIT)))
    x0o, z0o = -fp_x / 2.0, -fp_z / 2.0

    cells: list[list[str]] = []
    for iz in range(nz):
        row: list[str] = []
        za = z0o + iz * GRID_UNIT
        zb = za + GRID_UNIT
        for ix in range(nx):
            xa = x0o + ix * GRID_UNIT
            xb = xa + GRID_UNIT
            row.append(classify_cell(xa, xb, za, zb, tris, category, role, stem))
        cells.append(row)

    edges: dict[str, list[str]] = {"south": [], "north": [], "west": [], "east": []}
    for ix in range(nx):
        xa, xb = x0o + ix * GRID_UNIT, x0o + (ix + 1) * GRID_UNIT
        za, zb = z0o, z0o + nz * GRID_UNIT
        edges["south"].append(edge_segment_open(xa, za, xb, za, tris))
        edges["north"].append(edge_segment_open(xa, zb, xb, zb, tris))
    for iz in range(nz):
        za, zb = z0o + iz * GRID_UNIT, z0o + (iz + 1) * GRID_UNIT
        xa, xb = x0o, x0o + nx * GRID_UNIT
        edges["west"].append(edge_segment_open(xa, za, xa, zb, tris))
        edges["east"].append(edge_segment_open(xb, za, xb, zb, tris))

    # Variation meshes share topology with base piece.
    if meta.get("variant_of"):
        confidence = "mesh_variant"
    else:
        confidence = "mesh"

    floor_cells = sum(row.count("floor") + row.count("stairs") for row in cells)
    if floor_cells == 0 and category not in ("gate",):
        confidence = "review"

    return {
        "origin": "sw",
        "axis": {"x": "east", "z": "north"},
        "units_m": GRID_UNIT,
        "floor_plane_y": FLOOR_PLANE_Y,
        "nx": nx,
        "nz": nz,
        "cells": cells,
        "edges": edges,
        "confidence": confidence,
        "note": "cells[0]=south row; floor=walkable at y≈0; edges=outer perimeter per column/row",
    }


def mesh_bounds(j: dict, bin_data: bytes, mesh_idx: int) -> tuple[list[float], list[float]]:
    mins = [1e9, 1e9, 1e9]
    maxs = [-1e9, -1e9, -1e9]
    mesh = j["meshes"][mesh_idx]
    for prim in mesh["primitives"]:
        for x, y, z in read_accessor_vec3(j, bin_data, prim["attributes"]["POSITION"]):
            for k, v in enumerate((x, y, z)):
                mins[k] = min(mins[k], v)
                maxs[k] = max(maxs[k], v)
    return mins, maxs


def scene_bounds(j: dict, bin_data: bytes) -> tuple[list[float], list[float]]:
    mins = [1e9, 1e9, 1e9]
    maxs = [-1e9, -1e9, -1e9]

    def visit_node(idx: int) -> None:
        node = j["nodes"][idx]
        t = node.get("translation", [0, 0, 0])
        s = node.get("scale", [1, 1, 1])
        if "mesh" in node:
            mn, mx = mesh_bounds(j, bin_data, node["mesh"])
            for i in range(3):
                mn[i] = mn[i] * s[i] + t[i]
                mx[i] = mx[i] * s[i] + t[i]
            for i in range(3):
                mins[i] = min(mins[i], mn[i])
                maxs[i] = max(maxs[i], mx[i])
        for c in node.get("children", []):
            visit_node(c)

    for root in j.get("scenes", [{}])[0].get("nodes", []):
        visit_node(root)
    return mins, maxs


def snap_units(v: float) -> float:
    u = round(v / GRID_UNIT)
    return max(u, 1) * GRID_UNIT if u >= 1 else round(v, 2)


def build_piece(stem: str, mins: list[float], maxs: list[float], tris: list) -> dict:
    meta = MANUAL.get(stem, {})
    fx = round(maxs[0] - mins[0], 2)
    fz = round(maxs[2] - mins[2], 2)
    fy = round(maxs[1] - mins[1], 2)

    fp_override = meta.get("footprint_override")
    if fp_override:
        fp = fp_override
    elif meta.get("category") in ("prop", "template"):
        fp = {"x": fx, "z": fz}
    else:
        fp = {"x": snap_units(fx), "z": snap_units(fz)}

    piece: dict[str, Any] = {
        "stem": stem,
        "file": f"{stem}.glb",
        "category": meta.get("category", "unknown"),
        "role": meta.get("role", "unknown"),
        "footprint_m": fp,
        "grid_units": {
            "x": round(fp["x"] / GRID_UNIT, 2),
            "z": round(fp["z"] / GRID_UNIT, 2),
        },
        "bounds": {
            "x_min": round(mins[0], 2),
            "x_max": round(maxs[0], 2),
            "z_min": round(mins[2], 2),
            "z_max": round(maxs[2], 2),
            "y_min": round(mins[1], 2),
            "y_max": round(maxs[1], 2),
        },
        "mesh_extent_m": {"x": fx, "y": fy, "z": fz},
        "collide_default": meta.get(
            "collide_default", meta.get("category") not in ("prop", "gate")
        ),
        "cell_grid": build_cell_grid(stem, tris, fp["x"], fp["z"], meta),
    }
    for key in ("open_faces", "open_slots", "stairs", "variant_of", "purpose"):
        if key in meta:
            piece[key] = meta[key]
    if "purpose" not in piece:
        piece["purpose"] = f"Kenney {stem}."
    return piece


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--kit", default="space",
                    help="kit folder under assets/models/ (e.g. space, dungeon)")
    args = ap.parse_args()
    kit_dir = os.path.join(ROOT, "assets", "models", args.kit)
    out = os.path.join(kit_dir, "kenney_catalog.json")
    if not os.path.isdir(kit_dir):
        raise SystemExit(f"kit folder not found: {kit_dir}")

    pieces = []
    unknown: list[str] = []
    for path in sorted(glob.glob(os.path.join(kit_dir, "*.glb"))):
        stem = os.path.splitext(os.path.basename(path))[0]
        if stem not in MANUAL:
            unknown.append(stem)
        j, bin_data = read_glb(path)
        tris = extract_triangles(j, bin_data)
        mins, maxs = scene_bounds(j, bin_data)
        pieces.append(build_piece(stem, mins, maxs, tris))

    catalog = {
        "version": 2,
        "grid_unit_m": GRID_UNIT,
        "opening_w_m": OPENING_W,
        "opening_h_m": OPENING_H,
        "ceiling_m": CEILING,
        "floor_plane_y": FLOOR_PLANE_Y,
        "slot_l_m": GRID_UNIT * 0.5,
        "slot_c_m": GRID_UNIT * 1.5,
        "slot_r_m": GRID_UNIT * 2.5,
        "module_m": GRID_UNIT * 3,
        "cell_values": ["floor", "wall", "void", "stairs", "door", "prop", "hole"],
        "placement_fields": {
            "cell_grid.cells": "2D array [south→north][west→east]; floor = y≈0 hit + player clearance",
            "cell_grid.edges": "outer perimeter: open | wall per unit segment",
            "cell_grid.confidence": "mesh | mesh_variant | review",
        },
        "pieces": pieces,
    }

    with open(out, "w", encoding="utf-8") as f:
        json.dump(catalog, f, indent=2)
        f.write("\n")

    print(f"wrote {out} ({len(pieces)} pieces)")
    if unknown:
        print(f"  NOTE: {len(unknown)} stem(s) had no MANUAL metadata (category=unknown): "
              + ", ".join(unknown))
    review = [p["stem"] for p in pieces if p["cell_grid"]["confidence"] == "review"]
    if review:
        print(f"  REVIEW: {len(review)} piece(s) measured no floor cells: " + ", ".join(review))
    for stem in ("room-small", "corridor", "room-corner", "stairs", "template-wall"):
        p = next((x for x in pieces if x["stem"] == stem), None)
        if p is None:
            continue
        g = p["cell_grid"]
        print(f"\n{stem} ({g['confidence']}):")
        for row in g["cells"]:
            print(" ", row)
        print("  edges:", g["edges"])


if __name__ == "__main__":
    main()
