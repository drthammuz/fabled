#!/usr/bin/env python3
"""Write mesh-probed border_exits into existing module JSON files."""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import gen_maps as gm
import probe_map_geometry as probe


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--pool", default="generated")
    args = ap.parse_args()
    pool = Path("userinput/modules") / args.pool
    n = 0
    for path in sorted(pool.glob("*.json")):
        if path.name == "gen_index.json":
            continue
        data = json.loads(path.read_text(encoding="utf-8"))
        pieces = gm.pieces_from_json(data.get("pieces", []))
        if not pieces:
            continue
        exits = probe.border_exits_for_pieces(pieces)
        data["border_exits"] = probe.border_exits_to_json(exits)
        path.write_text(json.dumps(data, indent=2), encoding="utf-8")
        n += 1
    print(f"Updated border_exits on {n} modules in {pool}")


if __name__ == "__main__":
    main()
