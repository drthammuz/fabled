#!/usr/bin/env python3
"""Regression sweep: double-height industrial factory halls.

For every generated map (editor fractions 0.14/0.55/0.32, cells=25):
  1. Every hall cell's floor-1 ceiling slab is raised by exactly one wall tier
     (y == 4.495 + 4.25); every NON-hall walkable cell keeps the default slab.
  2. Every hall perimeter face carries a tier-2 wall at y == WALL_TIER_H,
     and no tier-2 wall is duplicated on a face.
  3. Catwalks: stairs walking-top height equals the deck height; deck segments
     form one contiguous run flush to the stairs; all pieces lie inside the
     hall's room bounds; the deck stays >= 2 m walking clearance under the
     raised roof; stairs/deck never sit within a door band.
  4. Bank trunks (pipe-large-long @ y=2.6) end embedded in a wall face, never
     over a doorway (>= 1.2 m from any door centre at both ends).
  5. Nav-grid sanity: tier-2 walls must NOT close any nav face that the
     ground-level wall set leaves open (hall corridor mouths stay passable).

Run: python tools/test_industrial_halls.py [n_seeds]
"""
import sys
sys.path.insert(0, "tools")
import math
import gen_freeform as gf
import level_composition as lc

CELL = 4.0
DELTA = {"N": (0, -1), "S": (0, 1), "E": (1, 0), "W": (-1, 0)}
ROOF_BASE = 4.5 - 0.005
HALL_ROOF_Y = ROOF_BASE + gf.HALL_ROOF_LIFT

COMPOSITIONS = [
    ("necropolis", "industrial_default", "priesthood"),
    ("outlaw", "industrial_default", "necropolis"),
    ("priesthood", "industrial_default", "outlaw"),
    ("synth", "industrial_default", "necropolis"),
]


def check(seed: int, prev_f: str, mid_f: str, next_f: str):
    errs = []
    comp = lc.LevelComposition(
        mix_mode="transition", prev_faction=prev_f, next_faction=next_f,
        default_faction=mid_f, prev_fraction=0.14, default_fraction=0.55,
        next_fraction=0.32,
    )
    fm = gf.generate_map(seed, cells=25, composition=comp)
    if fm is None:
        return [], {"halls": 0, "catwalks": 0, "trunks": 0}
    doc = gf.to_doc(fm, "t")
    pieces = doc["pieces"]
    gx, gz = fm.gx, fm.gz
    spine, _, _, zone_lookup = lc.plan_zones_for_map(fm)
    halls = gf._select_industrial_halls(fm, zone_lookup, comp.normalized())

    def cell_of(p):
        return gf._world_to_cell(gx, gz, float(p["x"]), float(p["z"]))

    # 1. roof heights
    for p in pieces:
        if not p.get("ceiling") or int(p.get("floor_level", 0)) != 1:
            continue
        c = cell_of(p)
        y = float(p.get("y", ROOF_BASE))
        if c in halls and abs(y - HALL_ROOF_Y) > 0.01:
            errs.append(f"hall cell {c} roof y={y} != {HALL_ROOF_Y}")
        if c not in halls and y > ROOF_BASE + 0.01:
            errs.append(f"non-hall cell {c} raised roof y={y}")

    # 2. tier-2 wall on every hall perimeter face, no duplicates
    upper = {}
    for p in pieces:
        if "hall_upper_wall" not in (p.get("tags") or []):
            continue
        key = (round(float(p["x"]), 1), round(float(p["z"]), 1))
        if key in upper:
            errs.append(f"duplicate tier-2 wall at {key}")
        upper[key] = p
        if abs(float(p.get("y", 0)) - gf.WALL_TIER_H) > 0.01:
            errs.append(f"tier-2 wall at {key} y={p.get('y')}")
    for (ix, iz) in halls:
        for side, (dx, dz) in DELTA.items():
            if (ix + dx, iz + dz) in halls:
                continue
            fx = gf.world_x(gx, ix) + dx * CELL * 0.5
            fz = gf.world_z(gz, iz) + dz * CELL * 0.5
            k = (round(fx, 1), round(fz, 1))
            if k not in upper:
                errs.append(f"hall face {k} missing tier-2 wall")

    door_xy = [(float(p["x"]), float(p["z"])) for p in pieces
               if p.get("role") == "door" or "hidden_entrance" in (p.get("tags") or [])]

    # 3. catwalks
    stairs = [p for p in pieces if p.get("stem") == "catwalk-stairs"]
    decks = [p for p in pieces if p.get("stem") == "catwalk-straight"]
    for st in stairs:
        s = float(st.get("scale", 1.0))
        top = float(st.get("y", 0.0)) + 1.4 * s
        near = [d for d in decks
                if abs(float(d.get("y", 0.0)) - top) < 0.02]
        if not near:
            errs.append(f"catwalk stairs at ({st['x']},{st['z']}): no deck at top height {top:.2f}")
        # deck clearance under the raised hall roof
        for d in near:
            if HALL_ROOF_Y - float(d["y"]) < 2.0:
                errs.append("deck too close to hall roof")
        for (dx_w, dz_w) in door_xy:
            if abs(float(st["x"]) - dx_w) < 2.0 and abs(float(st["z"]) - dz_w) < 2.0:
                errs.append(f"catwalk stairs inside door band at ({dx_w},{dz_w})")
    # contiguity: decks sharing a run (same lateral coord + y) step by CW seg
    runs = {}
    for d in decks:
        yaw = round(float(d.get("yaw", 0.0)) % math.pi, 2)
        lat = round(float(d["z"]) if yaw < 0.5 else float(d["x"]), 2)
        runs.setdefault((yaw, lat, round(float(d.get("y", 0)), 2)), []).append(d)
    for (yaw, lat, y), ds in runs.items():
        axis = [float(d["x"]) if yaw < 0.5 else float(d["z"]) for d in ds]
        axis.sort()
        for a, b in zip(axis, axis[1:]):
            if abs(b - a - 2.0) > 0.05:
                errs.append(f"catwalk run gap {b - a:.2f} at lat {lat}")
        # every deck cell must be over a hall cell
        for d in ds:
            if cell_of(d) not in halls:
                errs.append(f"deck piece outside hall at ({d['x']},{d['z']})")

    # 4. trunk ends embedded in walls / clear of doors
    trunks = [p for p in pieces if p.get("stem") == "pipe-large-long"
              and abs(float(p.get("y", 0)) - 2.6) < 0.01]
    truns = {}
    for p in trunks:
        yaw = round(float(p.get("yaw", 0.0)) % math.pi, 2)
        lat = round(float(p["z"]) if yaw < 0.5 else float(p["x"]), 2)
        truns.setdefault((yaw, lat), []).append(p)
    for (yaw, lat), ps in truns.items():
        axis = sorted(float(p["x"]) if yaw < 0.5 else float(p["z"]) for p in ps)
        for endv, sign in ((axis[0], -1), (axis[-1], 1)):
            end_edge = endv + sign * 1.0  # seg half length @ scale 1
            # wall-face lines sit at cell centre ± 2 → end_edge ≡ 2 (mod 4)
            frac = (end_edge - 2.0) % CELL
            if min(frac, CELL - frac) > 0.15:
                errs.append(f"trunk end at {end_edge:.2f} not on a wall face (lat {lat})")
        for p in (ps[0], ps[-1]):
            for (dx_w, dz_w) in door_xy:
                if abs(float(p["x"]) - dx_w) < 1.2 and abs(float(p["z"]) - dz_w) < 1.2:
                    errs.append(f"trunk end in door band at ({dx_w},{dz_w})")

    # 5. nav faces at hall mouths unaffected by tier-2 walls
    nav = doc.get("nav", {})
    open_faces = {}
    for cell in nav.get("cells", []):
        open_faces[tuple(cell["c"])] = set(cell["open"])
    for (ix, iz) in halls:
        for side, (dx, dz) in DELTA.items():
            nb = (ix + dx, iz + dz)
            if nb not in fm.walkable or (ix + dx, iz + dz) in halls:
                continue
            fx = gf.world_x(gx, ix) + dx * CELL * 0.5
            fz = gf.world_z(gz, iz) + dz * CELL * 0.5
            # ground truth: is there a ground-level wall (y<2) on this face?
            grounded = any(
                p.get("role") == "wall" and float(p.get("y") or 0) < 2.0
                and abs(float(p["x"]) - fx) < 0.7 and abs(float(p["z"]) - fz) < 0.7
                for p in pieces
            )
            has_door = any(abs(dx_w - fx) < 0.7 and abs(dz_w - fz) < 0.7
                           for (dx_w, dz_w) in door_xy)
            if not grounded and not has_door:
                if side not in open_faces.get((ix, iz), set()):
                    errs.append(f"hall mouth {(ix, iz)}->{side} closed in nav grid")

    stats = {
        "halls": len({c for c in halls}) and len(halls),
        "catwalks": len(stairs),
        "trunks": len(truns),
    }
    return errs, stats


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 10
    total_errs = 0
    agg = {"maps": 0, "hall_cells": 0, "catwalks": 0, "trunks": 0, "maps_with_hall": 0}
    for i in range(n):
        prev_f, mid_f, next_f = COMPOSITIONS[i % len(COMPOSITIONS)]
        seed = 1000 + i * 37
        try:
            errs, stats = check(seed, prev_f, mid_f, next_f)
        except Exception as e:
            print(f"seed {seed} ({prev_f}->{next_f}): EXCEPTION {e!r}")
            total_errs += 1
            continue
        agg["maps"] += 1
        agg["hall_cells"] += stats["halls"]
        agg["catwalks"] += stats["catwalks"]
        agg["trunks"] += stats["trunks"]
        agg["maps_with_hall"] += 1 if stats["halls"] else 0
        if errs:
            total_errs += len(errs)
            print(f"seed {seed} ({prev_f}->{next_f}): {len(errs)} errors")
            for e in errs[:8]:
                print("   ", e)
        else:
            print(f"seed {seed} ({prev_f}->{next_f}): OK "
                  f"(hall cells {stats['halls']}, catwalks {stats['catwalks']}, "
                  f"trunks {stats['trunks']})")
    print(f"\n{agg['maps']} maps: {agg['maps_with_hall']} with halls, "
          f"{agg['hall_cells']} hall cells, {agg['catwalks']} catwalks, "
          f"{agg['trunks']} bank trunks — {total_errs} errors")
    sys.exit(1 if total_errs else 0)


if __name__ == "__main__":
    main()
