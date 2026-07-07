#!/usr/bin/env python3
"""Regression sweep: zone-seam closure + reachability across seeds/compositions.

For every generated map:
  1. BFS from spawn honouring walls (block), doors (pass), stairs (elevation
     bridge) must reach EVERY walkable cell (unreached == 0).
  2. Every ground-faction seam face (prev|next <-> default, both walkable) must
     carry a wall or a door — no open zone seams.
  3. No door may free-stand: each ground transition door must have a wall,
     door, or void on both flanking face positions along its wall line.

Run: python tools/test_zone_seams.py [n_seeds]
"""
import sys
sys.path.insert(0, "tools")
import gen_freeform as gf
import level_composition as lc

CELL = 4.0
DELTA = {"N": (0, -1), "S": (0, 1), "E": (1, 0), "W": (-1, 0)}

COMPOSITIONS = [
    ("necropolis", "industrial_default", "priesthood"),
    ("outlaw", "industrial_default", "necropolis"),
    ("priesthood", "industrial_default", "outlaw"),
    ("outlaw", "industrial_default", "synth"),
    ("synth", "industrial_default", "necropolis"),
]


def check(seed: int, prev_f: str, mid_f: str, next_f: str):
    comp = lc.LevelComposition(mix_mode="transition", prev_faction=prev_f,
                               next_faction=next_f, default_faction=mid_f)
    fm = gf.generate_map(seed, cells=25, composition=comp)
    doc = gf.to_doc(fm, "t")
    pieces = doc["pieces"]
    gx, gz = fm.gx, fm.gz
    spine, _, _, zfn = lc.plan_zones_for_map(fm)
    elev = lc.make_elevation_lookup(fm.walkable, spine, comp.normalized())

    def wx(ix):
        return (ix - gx / 2 + 0.5) * CELL

    def wz(iz):
        return (iz - gz / 2 + 0.5) * CELL

    def facekey(c, side):
        dx, dz = DELTA[side]
        return (round(wx(c[0]) + dx * CELL * 0.5, 1), round(wz(c[1]) + dz * CELL * 0.5, 1))

    import math

    def snap_face(v, half):
        return (round(v / CELL + half) - half) * CELL

    def wall_key(p):
        x, z = float(p["x"]), float(p["z"])
        yaw = float(p.get("yaw", 0.0)) % math.pi
        if abs(yaw - math.pi / 2.0) < 0.3:
            x = snap_face(x, gx / 2.0)
        else:
            z = snap_face(z, gz / 2.0)
        return (round(x, 1), round(z, 1))

    walls, doors, stairs = set(), set(), set()
    for p in pieces:
        if p.get("ceiling") or int(p.get("floor_level", 0)) != 0:
            continue
        r = p.get("role")
        key = (round(p["x"], 1), round(p["z"], 1))
        if r == "wall":
            # Industrial-hall tier-2 walls float a full wall height up — they
            # never block ground movement (mirrors build_nav_grid).
            if float(p.get("y") or 0.0) >= 2.0:
                continue
            walls.add(wall_key(p))
        elif r == "door":
            doors.add(key)
        elif r == "stairs":
            stairs.add((int(round(p["x"] / CELL + gx / 2 - 0.5)),
                        int(round(p["z"] / CELL + gz / 2 - 0.5))))

    # 1. reachability
    spawn = fm.rooms[fm.spawn_room]
    start = (spawn.cx, spawn.cz)
    seen = {start}
    stack = [start]
    while stack:
        c = stack.pop()
        for side, (dx, dz) in DELTA.items():
            nb = (c[0] + dx, c[1] + dz)
            if nb not in fm.walkable or nb in seen:
                continue
            k = facekey(c, side)
            if k in walls and k not in doors:
                continue
            if abs(elev(c) - elev(nb)) > 0.5 and not (c in stairs or nb in stairs):
                continue
            seen.add(nb)
            stack.append(nb)
    unreached = len(fm.walkable) - len(seen)

    # 2. ground-faction seam closure
    import faction_profiles as fp
    open_seams = 0
    for zone_id, fac in (("prev", prev_f), ("next", next_f)):
        if fac == "synth":
            continue
        for c in fm.walkable:
            if zfn(c) != zone_id:
                continue
            for side, (dx, dz) in DELTA.items():
                nb = (c[0] + dx, c[1] + dz)
                if nb not in fm.walkable or zfn(nb) != "default":
                    continue
                k = facekey(c, side)
                if k not in walls and k not in doors:
                    open_seams += 1

    # 3. free-standing ground doors (flanks must be wall/door/void)
    floating = 0
    for p in pieces:
        if p.get("role") != "door" or int(p.get("floor_level", 0)) != 0:
            continue
        if "elevated_door" in (p.get("tags") or []):
            continue
        x, z = p["x"], p["z"]
        gxf = x / CELL + gx / 2 - 0.5
        vertical = abs(gxf - round(gxf)) > 0.25
        flanks = [(x, z - CELL), (x, z + CELL)] if vertical else [(x - CELL, z), (x + CELL, z)]
        for fx, fz in flanks:
            key = (round(fx, 1), round(fz, 1))
            if key in walls or key in doors:
                continue
            # both flank cells walkable = truly open flank
            if vertical:
                a = (int(round(gxf - 0.5)), int(round(fz / CELL + gz / 2 - 0.5)))
                b = (int(round(gxf + 0.5)), int(round(fz / CELL + gz / 2 - 0.5)))
            else:
                gzf = z / CELL + gz / 2 - 0.5
                a = (int(round(fx / CELL + gx / 2 - 0.5)), int(round(gzf - 0.5)))
                b = (int(round(fx / CELL + gx / 2 - 0.5)), int(round(gzf + 0.5)))
            if a in fm.walkable and b in fm.walkable:
                floating += 1

    return unreached, open_seams, floating


def main():
    n_seeds = int(sys.argv[1]) if len(sys.argv) > 1 else 20
    bad = 0
    for prev_f, mid_f, next_f in COMPOSITIONS:
        tot_u = tot_s = tot_f = 0
        for seed in range(1, n_seeds + 1):
            u, s, f = check(seed, prev_f, mid_f, next_f)
            tot_u += u
            tot_s += s
            tot_f += f
            if u or s or f:
                print(f"  FAIL {prev_f}->{next_f} seed {seed}: unreached={u} open_seams={s} floating_doors={f}")
                bad += 1
        print(f"{prev_f:11s}->{next_f:11s}: unreached={tot_u} open_seams={tot_s} floating_doors={tot_f} over {n_seeds} seeds")
    if bad:
        print(f"\n{bad} FAILING seed/composition combos")
        sys.exit(1)
    print("\nALL PASS")


if __name__ == "__main__":
    main()
