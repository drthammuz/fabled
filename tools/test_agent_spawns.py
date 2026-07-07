#!/usr/bin/env python3
"""Gate for enemy spawn placement + baked patrol routes (plan3 pass A).

For every generated map (transition compositions AND single-faction):
  1. No enemy spawns in the STARTING faction zone (transition: the zone of the
     spawn room; single: the spawn room cells), and never in hidden rooms.
  2. Every enemy spawn sits on a nav cell.
  3. Patrol routes (doc["enemy_patrols"], parallel to enemy_spawns): 2-4
     waypoints, every waypoint on a nav cell outside the starting faction, and
     every consecutive waypoint pair (incl. the loop-closing last->first)
     reachable via nav A* — so a patrolling enemy can actually walk its loop.
  4. NPC placement rules unchanged: NPCs only in the hub (y < -0.5) or in
     hidden rooms.

Run: python tools/test_agent_spawns.py [n_seeds]   (default 10; 5 comps each)
"""
import heapq
import sys

sys.path.insert(0, "tools")
import gen_freeform as gf
import level_composition as lc

CELL = 4.0
DELTA = {"N": (0, -1), "S": (0, 1), "E": (1, 0), "W": (-1, 0)}

COMPOSITIONS = [
    ("necropolis", "industrial_default", "priesthood"),
    ("outlaw", "industrial_default", "synth"),
    ("synth", "industrial_default", "necropolis"),
    ("priesthood", "industrial_default", "outlaw"),
    None,  # single-faction map
]


def astar_ok(nav, start, goal) -> bool:
    if start == goal:
        return True
    openq = [(0.0, start)]
    g = {start: 0.0}
    while openq:
        _, c = heapq.heappop(openq)
        if c == goal:
            return True
        for s in nav[c]["open"]:
            dx, dz = DELTA[s]
            nb = (c[0] + dx, c[1] + dz)
            if nb not in nav:
                continue
            ng = g[c] + 1.0
            if ng < g.get(nb, 1e18):
                g[nb] = ng
                h = abs(nb[0] - goal[0]) + abs(nb[1] - goal[1])
                heapq.heappush(openq, (ng + h, nb))
    return False


def cell_of(x: float, z: float, gx: int, gz: int):
    return (int(round(x / CELL + gx / 2 - 0.5)),
            int(round(z / CELL + gz / 2 - 0.5)))


def check(seed: int, comp_spec) -> list[str]:
    if comp_spec is None:
        comp = None
    else:
        prev_f, mid_f, next_f = comp_spec
        comp = lc.LevelComposition(mix_mode="transition", prev_faction=prev_f,
                                   next_faction=next_f, default_faction=mid_f)
    fm = gf.generate_map(seed, cells=25, composition=comp)
    if fm is None:
        return ["generate_map returned None"]
    doc = gf.to_doc(fm, "t")
    errs: list[str] = []

    navdoc = doc.get("nav") or {}
    nav = {tuple(e["c"]): e for e in navdoc.get("cells", [])}
    if not nav:
        return ["no nav grid in doc"]

    _, _, _, zone_lookup = lc.plan_zones_for_map(fm)
    spawn_c = (fm.rooms[fm.spawn_room].cx, fm.rooms[fm.spawn_room].cz)
    start_zone = zone_lookup(spawn_c)
    spawn_room = set(fm.rooms[fm.spawn_room].cells())
    hidden = set()
    for i in fm.hidden_rooms:
        hidden |= set(fm.rooms[i].cells())

    def in_start_faction(c) -> bool:
        if start_zone is not None:
            return zone_lookup(c) == start_zone
        return c in spawn_room

    spawns = doc.get("enemy_spawns", [])
    patrols = doc.get("enemy_patrols", [])
    if len(patrols) > len(spawns):
        errs.append(f"{len(patrols)} patrols for {len(spawns)} spawns")

    for i, (x, y, z) in enumerate(spawns):
        c = cell_of(x, z, fm.gx, fm.gz)
        if in_start_faction(c):
            errs.append(f"enemy {i} spawned in STARTING faction at cell {c}")
        if c in hidden:
            errs.append(f"enemy {i} spawned in hidden room {c}")
        if c not in nav:
            errs.append(f"enemy {i} spawn cell {c} not on nav grid")

    for i, route in enumerate(patrols):
        if not route:
            continue  # wander fallback is allowed
        if not (2 <= len(route) <= 4):
            errs.append(f"patrol {i} has {len(route)} waypoints (want 2-4)")
        cells = [cell_of(x, z, fm.gx, fm.gz) for (x, _y, z) in route]
        for j, c in enumerate(cells):
            if c not in nav:
                errs.append(f"patrol {i} wp {j} cell {c} off nav grid")
            elif in_start_faction(c):
                errs.append(f"patrol {i} wp {j} in starting faction {c}")
        if all(c in nav for c in cells):
            loop = cells + [cells[0]]
            for a, b in zip(loop, loop[1:]):
                if not astar_ok(nav, a, b):
                    errs.append(f"patrol {i} leg {a} -> {b} unreachable on nav")
                    break

    # NPC placement rules (plan4 2026-07-07): every hidden room keeps 1-2 NPCs;
    # the hub holds one keeper per market stall (within 2 m of its awning)
    # plus 1-2 wanderers. No NPCs anywhere else.
    hidden_counts: dict[int, int] = {}
    hub_npcs: list[tuple[float, float]] = []
    for i, (x, y, z) in enumerate(doc.get("npc_spawns", [])):
        if y < -0.5:
            hub_npcs.append((x, z))  # hub sub-floor — deliberate
            continue
        c = cell_of(x, z, fm.gx, fm.gz)
        room = next((ri for ri in fm.hidden_rooms
                     if c in set(fm.rooms[ri].cells())), None)
        if room is None:
            errs.append(f"npc {i} on floor 0 outside hidden rooms at {c}")
        else:
            hidden_counts[room] = hidden_counts.get(room, 0) + 1
    for ri in fm.hidden_rooms:
        n_here = hidden_counts.get(ri, 0)
        if not (1 <= n_here <= 2):
            errs.append(f"hidden room {ri} has {n_here} NPCs (want 1-2)")
    awnings = [(p["x"], p["z"]) for p in doc.get("pieces", [])
               if p.get("stem") == "detail-awning-wide"
               and "hub_market" in p.get("tags", [])]
    if fm.hub:
        if not (1 <= len(awnings) <= 3):
            errs.append(f"{len(awnings)} hub market stalls (want 1-3)")
        for ax, az in awnings:
            if not any((nx - ax) ** 2 + (nz - az) ** 2 <= 2.0 ** 2
                       for nx, nz in hub_npcs):
                errs.append(f"stall at ({ax:.1f},{az:.1f}) has no keeper NPC")
        wanderers = len(hub_npcs) - len(awnings)
        if not (1 <= wanderers <= 2):
            errs.append(f"{wanderers} hub wanderer NPCs (want 1-2)")
    return errs


def main() -> None:
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 10
    failures = 0
    for comp_spec in COMPOSITIONS:
        tag_c = "single" if comp_spec is None else f"{comp_spec[0][:5]}-{comp_spec[2][:5]}"
        for seed in range(1, n + 1):
            errs = check(seed, comp_spec)
            tag = f"{tag_c} seed {seed}"
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
