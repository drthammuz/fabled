#!/usr/bin/env python3
"""GLB mesh probes for procgen placement (stairs ↔ deck seams, ramp height)."""
from __future__ import annotations

import json
import struct
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path
from typing import Iterable, List, Optional, Sequence, Tuple

ROOT = Path(__file__).resolve().parent.parent
MODELS = ROOT / "assets" / "models"


@dataclass(frozen=True)
class StairRampMetrics:
    """Ramp metrics at scale 1, axis +local Z (``stairs-small-center`` convention).

    ``toward_neg_z_m`` — origin → top lip along −Z (horizontal, metres).
    ``toward_pos_z_m`` — origin → low lip along +Z.
    ``top_y_m`` — walkable surface height at the top lip (metres).
    """

    toward_neg_z_m: float
    toward_pos_z_m: float
    top_y_m: float

    def scaled(self, scale: float) -> StairRampMetrics:
        return StairRampMetrics(
            self.toward_neg_z_m * scale,
            self.toward_pos_z_m * scale,
            self.top_y_m * scale,
        )


def resolve_glb(stem: str, kit: str = "space_station") -> Optional[Path]:
    """Find a stair GLB under ``assets/models``."""
    kit_folder = {
        "space_station": "factions/synth",
        "space": "space",
        "dungeon": "dungeon",
    }.get(kit, kit)
    for base in (MODELS / kit_folder, MODELS / "factions" / "synth", MODELS / "space"):
        path = base / f"{stem}.glb"
        if path.is_file():
            return path
    return None


def _load_positions(glb: Path) -> List[Tuple[float, float, float]]:
    data = glb.read_bytes()
    chunk_len = struct.unpack_from("<II", data, 12)[0]
    doc = json.loads(data[20 : 20 + chunk_len])
    bin_off = 20 + chunk_len + 8
    bin_len = struct.unpack_from("<II", data, 20 + chunk_len)[0]
    blob = data[bin_off : bin_off + bin_len]
    out: List[Tuple[float, float, float]] = []
    for mesh in doc.get("meshes", []):
        for prim in mesh.get("primitives", []):
            pos_i = prim.get("attributes", {}).get("POSITION")
            if pos_i is None:
                continue
            acc = doc["accessors"][pos_i]
            bv = doc["bufferViews"][acc["bufferView"]]
            start = bv.get("byteOffset", 0) + acc.get("byteOffset", 0)
            for i in range(acc["count"]):
                out.append(struct.unpack_from("<fff", blob, start + i * 12))
    return out


def _max_y(verts: Sequence[Tuple[float, float, float]]) -> float:
    return max(v[1] for v in verts)


def probe_stair_ramp(glb: Path) -> StairRampMetrics:
    """Measure ramp lip from vertex data (not axis-aligned bbox)."""
    verts = _load_positions(glb)
    if not verts:
        raise ValueError(f"no vertices in {glb}")
    ymax = _max_y(verts)
    top = [v for v in verts if v[1] >= ymax - 0.05]
    lip = min(top, key=lambda v: v[2])
    pos_z = max(v[2] for v in verts)
    return StairRampMetrics(toward_neg_z_m=-lip[2], toward_pos_z_m=pos_z, top_y_m=ymax)


@lru_cache(maxsize=64)
def stair_metrics(stem: str, kit: str = "space_station") -> StairRampMetrics:
    path = resolve_glb(stem, kit)
    if path is None:
        # Fallback: space-station ``stairs-small-center`` probe values.
        return StairRampMetrics(0.2, 0.1, 0.3)
    return probe_stair_ramp(path)


def stair_ramp_footprint_m(stem: str, scale: float, kit: str = "space_station") -> Tuple[float, float]:
    m = stair_metrics(stem, kit).scaled(scale)
    return m.toward_neg_z_m, m.toward_pos_z_m


def stair_top_y_m(stem: str, scale: float, kit: str = "space_station") -> float:
    return stair_metrics(stem, kit).top_y_m * scale


def probe_all_stems(stems: Iterable[str], kit: str = "space_station") -> dict:
    out = {}
    for stem in stems:
        path = resolve_glb(stem, kit)
        if path is None:
            continue
        m = probe_stair_ramp(path)
        out[stem] = {
            "toward_neg_z_m": m.toward_neg_z_m,
            "toward_pos_z_m": m.toward_pos_z_m,
            "top_y_m": m.top_y_m,
            "path": str(path.relative_to(ROOT)),
        }
    return out
