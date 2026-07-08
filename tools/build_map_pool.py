#!/usr/bin/env python3
"""Regenerate userinput/maps/pool/ with current gen_maps.py + editor presets.

map_001 (start): industrial / industrial / outlaw (user spec 2026-07-07).
Others: two maps per chain faction as prev (outlaw/priesthood/synth/necropolis)
with varied next factions, so mount_hub_candidates' chain preference
(prev_faction == active's next_faction) always finds a match.

Writes pool/index.json with prev_faction/next_faction per entry.
"""
import json
import random
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
POOL = ROOT / "userinput/maps/pool"
GEN = ROOT / "tools/gen_maps.py"

# (id, prev, next)
SPECS = [
    ("map_001", "industrial_default", "outlaw"),
    ("map_002", "outlaw", "priesthood"),
    ("map_003", "outlaw", "synth"),
    ("map_004", "priesthood", "necropolis"),
    ("map_005", "priesthood", "outlaw"),
    ("map_006", "synth", "outlaw"),
    ("map_007", "synth", "necropolis"),
    ("map_008", "necropolis", "synth"),
    ("map_009", "necropolis", "priesthood"),
]

KNOBS = dict(cells=25, rooms=11, loops=3, attempts=30, organicness=0.0,
             corridor_width=1.0, hidden=1.0,
             prev_fraction=0.14, default_fraction=0.55, next_fraction=0.32,
             num_enemies=5)


def gen_one(map_id: str, prev: str, nxt: str, seed: int) -> Path:
    out = POOL / f"{map_id}.json"
    args = [sys.executable, str(GEN), "--preview", "--no-layout-export",
            "--seed", str(seed),
            "--attempts", str(KNOBS["attempts"]),
            "--cells", str(KNOBS["cells"]),
            "--rooms", str(KNOBS["rooms"]),
            "--loops", str(KNOBS["loops"]),
            "--organicness", f"{KNOBS['organicness']:.2f}",
            "--corridor-width", f"{KNOBS['corridor_width']:.2f}",
            "--hidden", f"{KNOBS['hidden']:.2f}",
            "--mix-mode", "transition",
            "--faction-profile", "industrial_default",
            "--prev-faction", prev,
            "--next-faction", nxt,
            "--default-faction", "industrial_default",
            "--prev-fraction", f"{KNOBS['prev_fraction']:.2f}",
            "--default-fraction", f"{KNOBS['default_fraction']:.2f}",
            "--next-fraction", f"{KNOBS['next_fraction']:.2f}",
            "--num-enemies", str(KNOBS["num_enemies"]),
            "--out", str(out)]
    r = subprocess.run(args, cwd=ROOT, capture_output=True, text=True)
    if r.returncode != 0 or not out.exists():
        raise RuntimeError(f"{map_id} failed (seed {seed}): {r.stderr[-800:]}")
    return out


def main() -> None:
    POOL.mkdir(parents=True, exist_ok=True)
    rng = random.Random(20260707)
    entries = []
    for map_id, prev, nxt in SPECS:
        doc = None
        best = None  # (hidden_count, doc) fallback if no seed hits the target
        for attempt in range(12):
            seed = rng.randrange(1, 1_000_000)
            try:
                out = gen_one(map_id, prev, nxt, seed)
            except RuntimeError as e:
                print(f"retry {map_id}: {e}", flush=True)
                continue
            cand = json.loads(out.read_text())
            n_hidden = sum(1 for p in cand.get("pieces", [])
                           if "hidden_entrance" in (p.get("tags") or []))
            if best is None or n_hidden > best[0]:
                best = (n_hidden, cand)
            # Require at least one hidden room so every map is testable; keep
            # trying seeds until one lands (placement is stochastic on dense maps).
            if n_hidden >= 1:
                doc = cand
                break
        if doc is None:
            if best is None:
                raise SystemExit(f"could not generate {map_id}")
            # No seed produced a hidden room in the budget — keep the best one.
            print(f"WARN {map_id}: no hidden room after retries "
                  f"(best {best[0]}); keeping it", flush=True)
            doc = best[1]
            (POOL / f"{map_id}.json").write_text(json.dumps(doc))
        entries.append({
            "id": map_id,
            "path": f"pool/{map_id}.json",
            "spawn_xz": doc.get("spawn_xz") or [0.0, 0.0],
            "extraction_xz": doc.get("extraction_xz"),
            "hub_exits": doc.get("hub_exits") or {},
            "prev_faction": prev,
            "next_faction": nxt,
            "modules_x": doc.get("modules_x", 25),
            "modules_z": doc.get("modules_z", 25),
        })
        print(f"OK {map_id} prev={prev} next={nxt} pieces={len(doc.get('pieces', []))} "
              f"exits={list((doc.get('hub_exits') or {}).keys())}", flush=True)

    index = {
        "version": 1,
        "modules": entries[0]["modules_x"],
        "grid_unit_m": 4.0,
        "start_id": "map_001",
        "maps": entries,
    }
    (POOL / "index.json").write_text(json.dumps(index, indent=1))
    print(f"pool index written: {len(entries)} maps", flush=True)


if __name__ == "__main__":
    main()
