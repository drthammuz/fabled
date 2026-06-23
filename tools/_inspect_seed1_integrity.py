"""Deep inspect seed 1 synth exit transition for walls/stairs."""
import sys
sys.path.insert(0, "tools")
import gen_freeform as gf
import level_composition as lc
import synth_transition as st
import transition_entrances as te
import random

comp = lc.LevelComposition(
    mix_mode="transition", prev_faction="synth", next_faction="synth",
    default_faction="industrial_default",
)
fm = gf.generate_map(1, cells=40, composition=comp)
doc = gf.to_doc(fm, "s1")
spine, _, _, zfn = lc.plan_zones_for_map(fm)
rng = random.Random((1 * 1597334677) & 0xFFFFFFFF)

print("=== ZONE MAP near transition (x 22-32, z 28-36) ===")
for iz in range(28, 37):
    row = []
    for ix in range(22, 33):
        c = (ix, iz)
        if c not in fm.walkable:
            row.append(" . ")
        else:
            z = zfn(c)
            row.append(f"{z[0]} ")
    print(f"z={iz:2d} " + "".join(row))

print("\n=== EXIT FACTION PLAN ===")
for b in te.find_zone_boundaries(spine, comp):
    if b.kind != "exit_faction":
        continue
    plan = st.plan_transition(b, comp, fm.walkable, zfn, rng, ascending=False)
    print("substrate", plan.strip.substrate_cells)
    print("stems", plan.stair_stems)
    for i, s in enumerate(plan.stair_stems):
        if s:
            print(f"  [{i}] {plan.strip.substrate_cells[i]} -> {st._stem_for_lateral_slot(s, i, len(plan.stair_stems))}")
    print("deck", sorted(plan.deck_cells))
    print("door", plan.door_cell)
    print("toward", plan.strip.toward_faction, "lateral", plan.strip.lateral_axis)

print("\n=== PIECES (transition + base walls) x22-32 z28-36 ===")
for p in doc["pieces"]:
    ix = int(round(p["x"] / 4 + fm.gx / 2 - 0.5))
    iz = int(round(p["z"] / 4 + fm.gz / 2 - 0.5))
    if not (22 <= ix <= 32 and 28 <= iz <= 36):
        continue
    if p.get("role") not in ("wall", "door", "stairs", "deck", "floor"):
        continue
    src = "T" if "synth_transition" in (p.get("tags") or []) else "B"
    print(f"{src} {p.get('role'):6s} {p.get('stem',''):22s} ({ix:2d},{iz:2d}) y={p.get('y','-')}")

print("\n=== OPEN SIDES (room integrity) synth prev cells ===")
DELTA = {"N": (0, -1), "S": (0, 1), "E": (1, 0), "W": (-1, 0)}
OP = {"N": "S", "S": "N", "E": "W", "W": "E"}

def has_wall_on_face(cell, side, pieces, gx, gz):
    dx, dz = DELTA[side]
    wx = (cell[0] - gx/2 + 0.5) * 4 + dx * 2
    wz = (cell[1] - gz/2 + 0.5) * 4 + dz * 2
    for p in pieces:
        if p.get("role") != "wall":
            continue
        if abs(p["x"] - wx) < 0.2 and abs(p["z"] - wz) < 0.2:
            return True
    return False

def has_door_on_face(cell, side, pieces, gx, gz):
    dx, dz = DELTA[side]
    wx = (cell[0] - gx/2 + 0.5) * 4 + dx * 2
    wz = (cell[1] - gz/2 + 0.5) * 4 + dz * 2
    for p in pieces:
        if p.get("role") != "door":
            continue
        if abs(p["x"] - wx) < 0.3 and abs(p["z"] - wz) < 0.3:
            return True
    return False

pieces = doc["pieces"]
for iz in range(28, 37):
    for ix in range(22, 33):
        c = (ix, iz)
        if c not in fm.walkable or zfn(c) != "prev":
            continue
        open_sides = []
        for side, (dx, dz) in DELTA.items():
            nb = (ix + dx, iz + dz)
            if nb in fm.walkable and zfn(nb) == "prev":
                continue  # room-room OK
            if has_door_on_face(c, side, pieces, fm.gx, fm.gz):
                continue
            if has_wall_on_face(c, side, pieces, fm.gx, fm.gz):
                continue
            open_sides.append(f"{side}->({nb[0]},{nb[1]}) z={zfn(nb) if nb in fm.walkable else 'void'}")
        if open_sides:
            print(f"  ({ix},{iz}) OPEN: {open_sides}")
