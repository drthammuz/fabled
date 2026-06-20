#!/usr/bin/env python3
"""
Audit Kenney layout for duplicate floor meshes and misaligned gates.

  python tools/probe_layout_decor.py userinput/kenney_layout.json

Duplicate floor: room/corridor GLB + template-floor-* at overlapping XZ.
Misaligned gate: gate not on module wall plane (±10 m local) — should sit between
modules, not at door-tile centre (±8 m).
"""
from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import gen_modules as gm  # noqa: E402

CELL = gm.CELL_M
HALF = gm.MODULE_M / 2
FLOOR_STEMS = frozenset({
    "template-floor", "template-floor-big", "template-floor-detail",
    "template-floor-detail-a", "template-floor-layer",
})
SHELL_STEMS = ("room-large", "room-", "corridor")


def piece_aabb(stem: str, x: float, z: float) -> tuple[float, float, float, float]:
    nx, nz = 1, 1
    if stem == "template-floor-big":
        nx, nz = 2, 2
    elif stem.startswith("room-large"):
        nx, nz = 5, 5
    elif stem.startswith("corridor"):
        nx, nz = 1, 1
    hw, hd = nx * CELL / 2, nz * CELL / 2
    return x - hw, x + hw, z - hd, z + hd


def overlaps(a, b) -> bool:
    return not (a[1] <= b[0] or b[1] <= a[0] or a[3] <= b[2] or b[3] <= a[2])


def module_center_for(piece: dict, pieces: list[dict]) -> tuple[float, float] | None:
    gid = piece.get("group_id")
    if gid is None:
        return None
    xs, zs = [], []
    for p in pieces:
        if p.get("group_id") != gid or p.get("floor", 0) != piece.get("floor", 0):
            continue
        if p["stem"].startswith("room-large") or p["stem"] == "corridor-intersection":
            xs.append(p["x"])
            zs.append(p["z"])
    if not xs:
        return None
    return sum(xs) / len(xs), sum(zs) / len(zs)


def audit(path: Path) -> list[str]:
    doc = json.loads(path.read_text(encoding="utf-8"))
    pieces = doc.get("pieces", [])
    issues: list[str] = []

    floor0 = [p for p in pieces if p.get("floor", 0) == 0]
    shells = [
        (p["stem"], piece_aabb(p["stem"], p["x"], p["z"]))
        for p in floor0
        if p["stem"].startswith(SHELL_STEMS)
    ]
    extras = [
        (p["stem"], p["x"], p["z"], piece_aabb(p["stem"], p["x"], p["z"]), p.get("group_id"))
        for p in floor0
        if p["stem"] in FLOOR_STEMS
    ]
    for estem, ex, ez, ebox, gid in extras:
        for sstem, sbox in shells:
            if overlaps(ebox, sbox):
                issues.append(
                    f"duplicate floor: {estem} @ ({ex:.0f},{ez:.0f}) gid={gid} "
                    f"overlaps {sstem}"
                )

    for p in floor0:
        if p["stem"] not in ("gate", "gate-door", "gate-opening"):
            continue
        mc = module_center_for(p, pieces)
        if mc is None:
            continue
        mcx, mcz = mc
        lx, lz = p["x"] - mcx, p["z"] - mcz
        half_cell = CELL / 2
        wall_pos = HALF
        cell_center_pos = HALF - half_cell  # ±8 m local (door tile centre)
        def inset(v: float) -> bool:
            a = abs(v)
            return abs(a - cell_center_pos) < 0.15 and abs(a - wall_pos) > 0.15
        if inset(lx) or inset(lz):
            issues.append(
                f"gate @ world ({p['x']:.0f},{p['z']:.0f}) gid={p.get('group_id')}: "
                f"local ({lx:.0f},{lz:.0f}) is at tile centre (±8), expected wall (±10)"
            )

    return issues


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("layout", type=Path, nargs="?", default=Path("userinput/kenney_layout.json"))
    args = ap.parse_args()
    issues = audit(args.layout)
    if not issues:
        print(f"PASS: {args.layout} — no duplicate floor tiles or inset gates on floor 0")
        return
    print(f"FAIL: {len(issues)} issue(s) in {args.layout}:")
    for line in issues:
        print(f"  · {line}")
    sys.exit(1)


if __name__ == "__main__":
    main()
