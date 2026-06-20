#!/usr/bin/env python3
"""
Compare catalog vs mesh borders on each module JSON in isolation.

If modules pass here but assembled maps fail, blame placement/assembly.
If modules fail here, fix catalog stamping or kenney_catalog.json first.

Usage:
    python tools/probe_module_pool.py
    python tools/probe_module_pool.py --pool generated --limit 50
    python tools/probe_module_pool.py --pool space_rooms
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Dict, List, Set, Tuple

sys.path.insert(0, str(Path(__file__).parent))
import gen_maps as gm
import probe_map_geometry as probe

Slot = Tuple[int, int]


def probe_one_module(path: Path, cat: Dict[str, dict]) -> List[str]:
    data = json.loads(path.read_text(encoding="utf-8"))
    name = data.get("name", path.stem)
    pieces = gm.pieces_from_json(data.get("pieces", []))
    tris = probe.build_module_tris(pieces)
    if not tris:
        return [f"{name}: no GLB triangles loaded"]

    mesh = probe.mesh_border_openings(tris)
    stored = probe.border_exits_from_json(data)
    geom = probe.build_module_geom(pieces, cat)
    catalog = geom.border_openings()
    issues: List[str] = []

    if stored is not None:
        for side in probe.SIDES:
            if stored.get(side, set()) != mesh.get(side, set()):
                issues.append(
                    f"{name} {side}: STORED {sorted(stored.get(side, set()))} "
                    f"vs MESH {sorted(mesh.get(side, set()))}"
                )
    else:
        for side in probe.SIDES:
            c = catalog.get(side, set())
            m = mesh.get(side, set())
            if c != m:
                issues.append(
                    f"{name} {side}: CATALOG {sorted(c)} [{geom.ascii_border(side)}] "
                    f"vs MESH {sorted(m)} [{_ascii(mesh, side)}]"
                )
    return issues


def _ascii(opens: Dict[str, Set[int]], side: str) -> str:
    tiles = opens.get(side, set())
    return "".join(str(i) if i in tiles else "." for i in range(5))


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--pool", default="generated", help="Module pool under userinput/modules/")
    ap.add_argument("--limit", type=int, default=0, help="Max modules to check (0=all)")
    ap.add_argument("--verbose", "-v", action="store_true", help="Print OK modules too")
    args = ap.parse_args()

    pool_dir = Path("userinput/modules") / args.pool
    if not pool_dir.exists():
        print(f"Pool not found: {pool_dir}")
        sys.exit(1)

    cat = probe.load_catalog()
    paths = sorted(p for p in pool_dir.glob("*.json") if p.name != "gen_index.json")
    if args.limit:
        paths = paths[: args.limit]

    total = 0
    bad = 0
    all_issues: List[str] = []
    for path in paths:
        total += 1
        issues = probe_one_module(path, cat)
        if issues:
            bad += 1
            all_issues.extend(issues)
        elif args.verbose:
            name = json.loads(path.read_text()).get("name", path.stem)
            print(f"OK  {name}")

    print(f"Probed {total} modules from pool '{args.pool}'")
    print(f"  {total - bad} OK, {bad} with catalog/mesh mismatch")
    if all_issues:
        print(f"\n{len(all_issues)} issue(s):")
        for line in all_issues[:40]:
            print(f"  · {line}")
        if len(all_issues) > 40:
            print(f"  · … and {len(all_issues) - 40} more")
        sys.exit(1)
    print("All modules: catalog borders match mesh borders.")


if __name__ == "__main__":
    main()
