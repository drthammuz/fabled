#!/usr/bin/env python3
"""Audit freeform hub / landing / main roofs in a generated map JSON.

Roof rule: normal ``template-floor`` one level above walkable, only when no
functional floor already occupies that centre (0 px). Ceiling slabs are tagged
``ceiling: true`` and must not duplicate an existing floor tile.

Usage:
    python tools/probe_freeform_roofs.py userinput/maps/_editor_preview.json
    python tools/probe_freeform_roofs.py --seed 42
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import gen_freeform as gf  # noqa: E402


def audit(doc: dict, fm: gf.FreeformMap) -> list[str]:
    errs: list[str] = []
    pieces = doc.get("pieces", [])
    hub = fm.hub
    if not hub:
        return ["no hub in map"]
    gx, gz = fm.gx, fm.gz
    open0 = {hub.trap0}

    def ceil_at(fl: int, wx: float, wz: float) -> bool:
        return any(
            p.get("ceiling")
            and int(p.get("floor_level", 0)) == fl
            and abs(float(p["x"]) - wx) < 0.01
            and abs(float(p["z"]) - wz) < 0.01
            for p in pieces
        )

    # Hub −1 → floor 0
    for c in sorted(hub.floor1 - hub.holes1):
        if c in open0:
            continue
        wx, wz = gf.world_x(gx, c[0]), gf.world_z(gz, c[1])
        func = gf._has_solid_floor_at(pieces, 0, wx, wz)
        ceil = ceil_at(0, wx, wz)
        if not func and not ceil:
            errs.append(f"hub {c}: missing roof at floor 0")
        if func and ceil:
            errs.append(f"hub {c}: duplicate func floor + ceiling at floor 0")

    # Landings −2 → floor −1
    for i, ex in enumerate(hub.exits):
        traps = ex.landing & hub.holes1
        inside = sum(1 for c in ex.landing if c in hub.floor1 - hub.holes1)
        outside = len(ex.landing - traps) - inside
        roofs = 0
        for c in sorted(ex.landing - traps):
            wx, wz = gf.world_x(gx, c[0]), gf.world_z(gz, c[1])
            func = gf._has_solid_floor_at(pieces, -1, wx, wz)
            ceil = ceil_at(-1, wx, wz)
            if func and ceil:
                errs.append(f"exit {i} {c}: duplicate f−1 floor + ceiling")
            elif not func and not ceil:
                errs.append(f"exit {i} {c}: missing roof at floor −1")
            elif ceil:
                roofs += 1
        print(
            f"exit {i} ({ex.kind}): landing={len(ex.landing)} "
            f"under_hub_f1={inside} outside={outside} ceiling_slabs={roofs}"
        )

    errs.extend(gf.audit_floor_overlaps(pieces))
    return errs


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("path", nargs="?", help="Map JSON path")
    ap.add_argument("--seed", type=int, help="Regenerate from seed instead of reading file")
    args = ap.parse_args()

    if args.seed is not None:
        fm = gf.generate_map(args.seed)
        if not fm:
            raise SystemExit(f"seed {args.seed}: generation failed")
        doc = gf.to_doc(fm, "probe")
        print(f"seed {args.seed} hub exits {len(fm.hub.exits)} trap0 {fm.hub.trap0}")
    else:
        path = Path(args.path or "userinput/maps/_editor_preview.json")
        doc = json.loads(path.read_text(encoding="utf-8"))
        seed = doc.get("seed")
        fm = gf.generate_map(seed) if seed is not None else None
        if fm is None:
            raise SystemExit("need --seed or map JSON with seed field for hub geometry")

    errs = audit(doc, fm)
    if errs:
        print(f"FAIL ({len(errs)} issues):")
        for e in errs[:20]:
            print(" ", e)
        raise SystemExit(1)
    print("OK — all hub / landing roofs satisfy dedup rule")


if __name__ == "__main__":
    main()
