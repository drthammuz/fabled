#!/usr/bin/env python3
"""Mass-simulation sweep of the baked enemy nav grid (doc["nav"]).

For every generated map:
  1. The nav graph must be SYMMETRIC (if A lists B open, B lists A open).
  2. Every open edge must obey the world rules re-derived from the pieces
     (no wall without door on the face; elevation steps need a stair cell).
  3. A* between many random walkable pairs: every pair reachable from the
     spawn component must produce a path, and the path must only use open
     faces (simulates thousands of enemy walks without booting the game).
  4. Enemy/NPC spawns must sit ON their cell's floor (y == nav y + 0.15).

Run: python tools/test_nav_grid.py [n_seeds] (default 6; ~24 maps, ~50k A* runs)
"""
import heapq
import random
import sys

sys.path.insert(0, "tools")
import gen_freeform as gf
import level_composition as lc

CELL = 4.0
DELTA = {"N": (0, -1), "S": (0, 1), "E": (1, 0), "W": (-1, 0)}
OPP = {"N": "S", "S": "N", "E": "W", "W": "E"}

COMPOSITIONS = [
    ("necropolis", "industrial_default", "priesthood"),
    ("outlaw", "industrial_default", "synth"),
    ("synth", "industrial_default", "priesthood"),
    ("synth", "industrial_default", "necropolis"),
]


def astar(nav, start, goal):
    """Grid A* over nav cells; returns path (list of cells) or None."""
    if start == goal:
        return [start]
    openq = [(0.0, start)]
    g = {start: 0.0}
    came = {}
    while openq:
        _, c = heapq.heappop(openq)
        if c == goal:
            path = [c]
            while c in came:
                c = came[c]
                path.append(c)
            return path[::-1]
        for s in nav[c]["open"]:
            dx, dz = DELTA[s]
            nb = (c[0] + dx, c[1] + dz)
            if nb not in nav:
                continue
            ng = g[c] + 1.0 + abs(nav[c]["y"] - nav[nb]["y"])
            if ng < g.get(nb, 1e18):
                g[nb] = ng
                came[nb] = c
                h = abs(nb[0] - goal[0]) + abs(nb[1] - goal[1])
                heapq.heappush(openq, (ng + h, nb))
    return None


def check(seed: int, prev_f: str, mid_f: str, next_f: str) -> list[str]:
    comp = lc.LevelComposition(mix_mode="transition", prev_faction=prev_f,
                               next_faction=next_f, default_faction=mid_f)
    fm = gf.generate_map(seed, cells=25, composition=comp)
    doc = gf.to_doc(fm, "t")
    errs: list[str] = []

    navdoc = doc.get("nav")
    if not navdoc or not navdoc.get("cells"):
        return [f"no nav grid in doc"]
    nav = {tuple(e["c"]): e for e in navdoc["cells"]}

    # 1. symmetry
    for c, e in nav.items():
        for s in e["open"]:
            dx, dz = DELTA[s]
            nb = (c[0] + dx, c[1] + dz)
            if nb not in nav:
                errs.append(f"{c} open {s} into non-walkable {nb}")
            elif OPP[s] not in nav[nb]["open"]:
                errs.append(f"asymmetric edge {c} {s} -> {nb}")

    # 2. edges obey re-derived world rules (independent re-derivation)
    ref = gf.build_nav_grid(doc["pieces"], fm,
                            *_spine_comp(fm, comp), fm.gx, fm.gz)
    ref_nav = {tuple(e["c"]): e for e in ref["cells"]}
    for c, e in nav.items():
        if set(e["open"]) != set(ref_nav[c]["open"]):
            errs.append(f"edge mismatch at {c}: {e['open']} vs {ref_nav[c]['open']}")

    # 3. A* mass simulation from the spawn component
    spawn = fm.rooms[fm.spawn_room]
    start = (spawn.cx, spawn.cz)
    seen = {start}
    stack = [start]
    while stack:
        c = stack.pop()
        for s in nav[c]["open"]:
            dx, dz = DELTA[s]
            nb = (c[0] + dx, c[1] + dz)
            if nb in nav and nb not in seen:
                seen.add(nb)
                stack.append(nb)
    unreached = set(nav) - seen
    if unreached:
        errs.append(f"{len(unreached)} nav cells unreachable from spawn e.g. {sorted(unreached)[:3]}")

    rng = random.Random(seed * 7919)
    comp_cells = sorted(seen)
    runs = 0
    for _ in range(500):
        a = rng.choice(comp_cells)
        b = rng.choice(comp_cells)
        path = astar(nav, a, b)
        runs += 1
        if path is None:
            errs.append(f"A* failed {a} -> {b} (both in spawn component)")
            break
        # path must only use open faces
        for u, v in zip(path, path[1:]):
            d = (v[0] - u[0], v[1] - u[1])
            side = {(0, -1): "N", (0, 1): "S", (1, 0): "E", (-1, 0): "W"}[d]
            if side not in nav[u]["open"]:
                errs.append(f"A* path uses closed face {u} {side}")
                break

    # 4. agent spawns sit on their cell floor (hub/sub-level spawns — NPCs at
    #    y=-3.85 — are deliberate and exempt).
    for kind in ("enemy_spawns", "npc_spawns"):
        for (x, y, z) in doc.get(kind, []):
            if y < -0.5:
                continue
            c = (int(round(x / CELL + fm.gx / 2 - 0.5)),
                 int(round(z / CELL + fm.gz / 2 - 0.5)))
            if c in nav and abs(y - (nav[c]["y"] + 0.15)) > 0.01:
                errs.append(f"{kind} at ({x:.1f},{z:.1f}) y={y} != floor {nav[c]['y']}+0.15")
    return errs


def _spine_comp(fm, comp):
    spine, _, _, _ = lc.plan_zones_for_map(fm)
    return spine, comp.normalized()


def main() -> None:
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 6
    failures = 0
    for prev_f, mid_f, next_f in COMPOSITIONS:
        for seed in range(1, n + 1):
            errs = check(seed, prev_f, mid_f, next_f)
            tag = f"{prev_f[:5]}-{next_f[:5]} seed {seed}"
            if errs:
                failures += 1
                print(f"FAIL {tag}")
                for e in errs[:6]:
                    print(f"   {e}")
            else:
                print(f"ok   {tag}")
    print("ALL PASS" if failures == 0 else f"{failures} FAILURES")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
