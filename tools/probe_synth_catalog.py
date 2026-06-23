#!/usr/bin/env python3
"""Probe synth GLB bounds and emit placement_catalog.json for editor + procgen."""

from __future__ import annotations

import json
import struct
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MODELS = ROOT / "assets" / "models" / "factions" / "synth"
OUT = MODELS / "placement_catalog.json"

# Keep in sync with editor_catalog.rs dressing lists.
STRUCTURE = [
    "floor", "floor-corner", "floor-detail", "floor-panel", "floor-panel-corner",
    "floor-panel-end", "floor-panel-straight", "wall", "wall-corner", "wall-corner-round",
    "wall-door", "wall-door-banner", "wall-door-center", "wall-door-edge",
    "wall-door-edge-banner", "wall-door-wide", "wall-door-wide-banner",
]
WALL_SWAP = [
    "wall-banner", "wall-pillar", "wall-pillar-banner", "wall-corner-banner",
    "wall-corner-round-banner", "wall-window", "wall-window-banner", "wall-window-frame",
    "wall-window-shutters",
]
WALL_DECAL = ["display-wall", "display-wall-wide", "wall-detail", "wall-switch"]
FLOOR_PROP = [
    "bed-single", "bed-single-cover", "bed-double", "bed-double-cover", "chair",
    "chair-armrest", "chair-armrest-headrest", "chair-cushion", "chair-cushion-headrest",
    "chair-headrest", "computer", "computer-screen", "computer-system", "computer-wide",
    "container", "container-flat", "container-flat-open", "container-tall", "container-wide",
    "table", "table-display", "table-display-planet", "table-display-small", "table-inset",
    "table-inset-small", "table-large",
]
BALCONY = [
    "balcony-floor", "balcony-floor-center", "balcony-floor-corner", "balcony-rail",
    "balcony-rail-center", "balcony-rail-corner", "rail", "rail-narrow",
]
ALL_DRESSING = STRUCTURE + WALL_SWAP + WALL_DECAL + FLOOR_PROP + BALCONY

CELL_M = 4.0
WALL_FACE_M = 2.0
DECK_Y = 1.2
DEFAULT_SCALE = 4.0


@dataclass
class Bounds:
    x0: float
    x1: float
    y0: float
    y1: float
    z0: float
    z1: float

    @property
    def hx(self) -> float:
        return (self.x1 - self.x0) * 0.5

    @property
    def hz(self) -> float:
        return (self.z1 - self.z0) * 0.5

    @property
    def cy(self) -> float:
        return (self.y0 + self.y1) * 0.5


def load_bounds(glb: Path) -> tuple[Bounds, list[tuple[float, float, float]]]:
    data = glb.read_bytes()
    chunk_len = struct.unpack_from("<II", data, 12)[0]
    doc = json.loads(data[20 : 20 + chunk_len])
    bin_off = 20 + chunk_len + 8
    blob = data[bin_off:]
    verts: list[tuple[float, float, float]] = []
    for mesh in doc.get("meshes", []):
        for prim in mesh.get("primitives", []):
            pos_i = prim.get("attributes", {}).get("POSITION")
            if pos_i is None:
                continue
            acc = doc["accessors"][pos_i]
            bv = doc["bufferViews"][acc["bufferView"]]
            start = bv.get("byteOffset", 0) + acc.get("byteOffset", 0)
            for i in range(acc["count"]):
                verts.append(struct.unpack_from("<fff", blob, start + i * 12))
    xs = [v[0] for v in verts]
    ys = [v[1] for v in verts]
    zs = [v[2] for v in verts]
    return Bounds(min(xs), max(xs), min(ys), max(ys), min(zs), max(zs)), verts


def mass_z(verts: list[tuple[float, float, float]]) -> float:
    return sum(v[2] for v in verts) / max(len(verts), 1)


def infer_front(stem: str, bb: Bounds, verts: list[tuple[float, float, float]]) -> str:
    """Kenney +Z front for seating/work surfaces; beds pillow at −Z."""
    if stem.startswith("chair"):
        return "+z"
    if stem.startswith("bed-") and not stem.endswith("-cover"):
        return "-z"
    if stem.startswith("computer"):
        return "+z"
    cz = mass_z(verts)
    if cz > bb.hz * 0.15:
        return "+z"
    if cz < -bb.hz * 0.15:
        return "-z"
    return "+z"


def infer_snap(stem: str) -> str:
    if stem in ("bed-single", "bed-double"):
        return "back_z"
    if stem.endswith("-cover"):
        return "stack"
    if stem.startswith("wall") and stem not in WALL_DECAL:
        return "wall_face"
    if stem in WALL_DECAL:
        return "wall_decal"
    if stem.startswith("balcony-") or stem in ("rail", "rail-narrow"):
        return "balcony_outward" if stem.startswith("balcony-floor") else "origin"
    if stem == "floor" or stem.startswith("floor-"):
        return "substrate"
    return "origin"


def stack_on(stem: str) -> str | None:
    if stem == "bed-single-cover":
        return "bed-single"
    if stem == "bed-double-cover":
        return "bed-double"
    return None


def deck_y_mode(stem: str, bb: Bounds) -> str:
    if stem == "floor" or stem.startswith("floor-"):
        return "substrate_block"
    if stem.startswith("balcony-floor"):
        return "deck_flush"
    return "deck_base"


def probe_stem(stem: str) -> dict | None:
    glb = MODELS / f"{stem}.glb"
    if not glb.is_file():
        return None
    bb, verts = load_bounds(glb)
    snap = infer_snap(stem)
    entry = {
        "class": (
            "structure"
            if stem in STRUCTURE
            else "wall_swap"
            if stem in WALL_SWAP
            else "wall_decal"
            if stem in WALL_DECAL
            else "balcony"
            if stem in BALCONY
            else "floor_prop"
        ),
        "bounds_scale1": {
            "x0": round(bb.x0, 4),
            "x1": round(bb.x1, 4),
            "y0": round(bb.y0, 4),
            "y1": round(bb.y1, 4),
            "z0": round(bb.z0, 4),
            "z1": round(bb.z1, 4),
        },
        "half_x_m": round(bb.hx, 4),
        "half_z_m": round(bb.hz, 4),
        "height_m": round(bb.y1 - bb.y0, 4),
        "front": infer_front(stem, bb, verts),
        "snap": snap,
        "deck_y": deck_y_mode(stem, bb),
    }
    if snap == "back_z":
        entry["back_anchor_local_m"] = 2.0
        entry["pillow_local_z"] = round(bb.z0, 4)
    so = stack_on(stem)
    if so:
        entry["stack_on"] = so
    if stem in WALL_DECAL:
        depth = {
            "display-wall": 0.765,
            "display-wall-wide": 0.765,
            "wall-detail": 0.52,
            "wall-switch": 0.05,
        }.get(stem, 0.5)
        entry["decal_depth_half_m"] = depth
    return entry


def main() -> None:
    stems: dict[str, dict] = {}
    missing: list[str] = []
    for stem in ALL_DRESSING:
        row = probe_stem(stem)
        if row is None:
            missing.append(stem)
            continue
        stems[stem] = row

    doc = {
        "version": 1,
        "generated_by": "tools/probe_synth_catalog.py",
        "default_scale": DEFAULT_SCALE,
        "cell_m": CELL_M,
        "wall_face_offset_m": WALL_FACE_M,
        "deck_y": DECK_Y,
        "wall_half_thickness_m": 0.6,
        "stems": stems,
    }
    OUT.write_text(json.dumps(doc, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {OUT} ({len(stems)} stems)")
    if missing:
        print("missing GLBs:", ", ".join(missing), file=sys.stderr)


if __name__ == "__main__":
    main()
