#!/usr/bin/env python3
"""
Run placement optimization sweeps for each faction ~2min each.
Uses catalog-driven logic in faction_interior.
Logs to userinput/sweeps_<faction>.log

Run per faction, adapt based on LLM review of logs (density, overlaps, center usage, relations).
Repeat loop as needed.

Usage: python tools/run_faction_placement_sweeps.py --faction necropolis --minutes 2
or for all.
"""

import argparse
import json
import time
import random
import sys
from pathlib import Path

import faction_interior as fi
# import synth_interior_sweep as sis  # reuse some if compatible

FACTIONS = ["priesthood", "necropolis", "urban", "industrial", "synth"]

def run_sweep_for_faction(faction: str, minutes: float, log_path: Path):
    start = time.time()
    budget = minutes * 60
    rng = random.Random(hash(faction))
    best_score = -1
    history = []
    log_path.parent.mkdir(parents=True, exist_ok=True)
    with open(log_path, "w") as logf:
        logf.write(f"Starting sweep for {faction} budget {minutes}min\n")
        trials = 0
        while time.time() - start < budget:
            trials += 1
            gap = max(0.5, min(3.0, 1.2 + rng.gauss(0, 0.5)))
            params = fi.SweepParams(gap_multiplier=gap)
            try:
                pieces = []
                # synthetic rooms for fast eval (use real room analysis logic but small)
                walk = {(x, z) for x in range(10) for z in range(10)}
                corr = set()
                fi.set_faction_context(faction, params)
                n = fi.furnish_faction_interior(pieces, 0, 0, walk, lambda c: "prev" if c in walk else None, set(), rng.randint(0, 10**9), corridor_cells=corr, faction=faction, sweep_params=params)
                # score: props + free space (simple bbox check) + relation bonus
                props = [p for p in pieces if p.get("role") == "prop"]
                n_props = len(props)
                # rough free: no overlaps via simple dist
                overlaps = 0
                center_count = 0
                room_cx = 18.0  # approx center of 10 cell *4 /2
                room_cz = 18.0
                for i, p1 in enumerate(props):
                    for p2 in props[i+1:]:
                        if abs(p1["x"]-p2["x"]) < 1.5 and abs(p1["z"]-p2["z"]) < 1.5:
                            overlaps += 1
                    dist = abs(p1.get("x",0) - room_cx) + abs(p1.get("z",0) - room_cz)
                    if dist < 4.0:
                        center_count += 1
                free = max(0, 20 - overlaps * 2)
                # penalize excessive center clustering (like synth center bias rule)
                center_pen = max(0, center_count - 1) * 3
                score = n_props * 2 + free * 1.5 - center_pen
                # bonus if relations likely fired (companions placed near)
                if n_props > 1:
                    score += 1.0
                if score > best_score:
                    best_score = score
                    logf.write(f"trial {trials}: score={score:.1f} props={n_props} gap={gap:.2f} overlaps={overlaps} centerish={center_count}\n")
                    logf.flush()
                    history.append({"trial": trials, "score": score, "params": {"gap": gap}})
            except Exception as e:
                logf.write(f"trial {trials} error: {e}\n")
        logf.write(f"Done {faction}: best {best_score} after {trials} trials in {time.time()-start:.1f}s\n")
    return {"best": best_score, "history": history[-5:] }  # last few for review

if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--faction", default="all")
    ap.add_argument("--minutes", type=float, default=2)
    args = ap.parse_args()
    factions = FACTIONS if args.faction == "all" else [args.faction]
    for f in factions:
        logp = Path("userinput") / f"sweeps_{f}.log"
        res = run_sweep_for_faction(f, args.minutes, logp)
        print(f"{f} done, best score {res['best']}. See {logp}")
        # LLM note: review log for density (aim <0.3 props per m2), relations used, center avoidance
        # If center spam high, increase avoid_center in density. If low props, decrease gap.