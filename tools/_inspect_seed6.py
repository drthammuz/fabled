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
seed = 6
fm = gf.generate_map(seed, cells=40, composition=comp)
doc = gf.to_doc(fm, "s6")
spine, _, _, zfn = lc.plan_zones_for_map(fm)
rng = random.Random((seed * 1597334677) & 0xFFFFFFFF)

for b in te.find_zone_boundaries(spine, comp):
    if b.kind != "exit_faction":
        continue
    plan = st.plan_transition(b, comp, fm.walkable, zfn, rng, ascending=False)
    print("substrate", plan.strip.substrate_cells)
    for i, (sc, s) in enumerate(zip(plan.strip.substrate_cells, plan.stair_stems)):
        if s:
            print(f"  [{i}] cell {sc} stem {st._stem_for_lateral_slot(s,i,len(plan.stair_stems))}")
    print("deck", sorted(plan.deck_cells), "door", plan.door_cell)

print("\nStairs pieces:")
for p in doc["pieces"]:
    if p.get("role") != "stairs" or "exit_faction" not in (p.get("tags") or []):
        continue
    ix = int(round(p["x"] / 4 + fm.gx / 2 - 0.5))
    iz = int(round(p["z"] / 4 + fm.gz / 2 - 0.5))
    print(f"  {p['stem']} ({ix},{iz})")

DELTA = {"N": (0, -1), "S": (0, 1), "E": (1, 0), "W": (-1, 0)}

def face_has_barrier(cell, side, pieces, gx, gz, walkable, zfn):
    dx, dz = DELTA[side]
    nb = (cell[0] + dx, cell[1] + dz)
    if nb in walkable and zfn(nb) == "prev":
        return "room"
    ex = (cell[0] - gx / 2 + 0.5) * 4 + dx * 2
    ez = (cell[1] - gz / 2 + 0.5) * 4 + dz * 2
    for p in pieces:
        if p.get("role") == "door" and abs(p["x"] - ex) < 0.3 and abs(p["z"] - ez) < 0.3:
            return "door"
        if p.get("role") == "wall" and abs(p["x"] - ex) < 0.2 and abs(p["z"] - ez) < 0.2:
            return "wall"
    if nb not in walkable:
        return "void"
    return f"open({zfn(nb)})"

print("\nIntegrity prev cells x23-32 z34-40:")
pieces = doc["pieces"]
for iz in range(34, 41):
    for ix in range(23, 33):
        c = (ix, iz)
        if c not in fm.walkable or zfn(c) != "prev":
            continue
        bad = []
        for side in DELTA:
            r = face_has_barrier(c, side, pieces, fm.gx, fm.gz, fm.walkable, zfn)
            if r not in ("room", "door", "wall"):
                bad.append(f"{side}:{r}")
        if bad:
            print(f"  ({ix},{iz}) {bad}")
