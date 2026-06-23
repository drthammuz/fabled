#!/usr/bin/env python3
"""Generate a multi-room synth dressing map for interior procgen rating."""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "userinput" / "synth_dressing"

sys.path.insert(0, str(ROOT / "tools"))
import synth_interior as si  # noqa: E402


def filled_mask() -> dict:
    return {"cells_x": 40, "cells_z": 40, "cells": [True] * (40 * 40)}


def main() -> None:
    seed = 42
    pieces, room_infos, floor_ix, corridor_ix = si.furnish_floor_plan(seed)
    name = "interior_rating_20"
    errors = si.validate_props(pieces, name)
    if errors:
        raise SystemExit(1)

    room_manifest = [
        {
            "id": info.room_id,
            "role": info.role,
            "area_cells": info.area,
            "mouths": info.corridor_mouths,
            "centre_xz": [round(info.centre_w[0], 2), round(info.centre_w[1], 2)],
        }
        for info in room_infos
    ]
    nprops = sum(1 for p in pieces if p.get("role") == "prop")
    nrooms = len(room_infos)
    ncorr = len(corridor_ix)

    doc = {
        "version": 1,
        "name": name,
        "vignette": "rating_wing",
        "seed": seed,
        "note": (
            f"Rating wing — {nrooms} furnished rooms, {ncorr} corridor cells, "
            f"{nprops} props. Walk corridors (empty), rate each room cluster."
        ),
        "floor_mask": filled_mask(),
        "room_manifest": room_manifest,
        "pieces": pieces,
        "spawn_xz": [0.0, 0.0],
        "spawn_y": 1.2,
    }

    OUT.mkdir(parents=True, exist_ok=True)
    path = OUT / f"{name}.json"
    path.write_text(json.dumps(doc, indent=2) + "\n", encoding="utf-8")
    role_counts: dict[str, int] = {}
    for info in room_manifest:
        role_counts[info["role"]] = role_counts.get(info["role"], 0) + 1
    print(f"wrote {path} ({len(pieces)} pieces, {nprops} props, {nrooms} rooms)")
    print("roles:", role_counts)
    for info in room_manifest:
        print(
            f"  room {info['id']:2d}: {info['role']:8s} "
            f"{info['area_cells']:2d} cells @ ({info['centre_xz'][0]:+.0f}, {info['centre_xz'][1]:+.0f})"
        )

    import audit_synth_scene as audit  # noqa: E402

    flags = audit.audit_doc(doc, name)
    if flags:
        print(f"AUDIT FAIL ({len(flags)} red flags):")
        for e in flags[:25]:
            print(f"  - {e}")
        raise SystemExit(1)
    print(f"AUDIT OK {path.name}")


if __name__ == "__main__":
    main()
