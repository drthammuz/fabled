import sys
sys.path.insert(0, "tools")
import gen_freeform as gf
import level_composition as lc
import synth_transition as st
import transition_entrances as te
import random

comp = lc.LevelComposition(mix_mode="transition", prev_faction="synth", next_faction="synth",
                           default_faction="industrial_default")
fm = gf.generate_map(1, cells=40, composition=comp)
spine, _, _, zfn = lc.plan_zones_for_map(fm)
rng = random.Random((1 * 1597334677) & 0xFFFFFFFF)

# bounds
xs=[c[0] for c in fm.walkable]; zs=[c[1] for c in fm.walkable]
print("walkable x", min(xs), max(xs), "z", min(zs), max(zs))

print("\n=== ALL BOUNDARIES ===")
for b in te.find_zone_boundaries(spine, comp):
    plan = st.plan_transition(b, comp, fm.walkable, zfn, rng, ascending=(b.kind=="enter_faction"))
    print(f"{b.kind} faction={b.faction_id} sub={b.substrate_cell} fac={b.faction_cell}")
    print(f"   width={plan.strip.width} toward={plan.strip.toward_faction} lat={plan.strip.lateral_axis}")
    print(f"   substrate={plan.strip.substrate_cells}")
    print(f"   stems={plan.stair_stems}")
    for i,s in enumerate(plan.stair_stems):
        if s:
            print(f"      slot[{i}] sub={plan.strip.substrate_cells[i]} -> {st._stem_for_lateral_slot(s,i,len(plan.stair_stems))}")
    print(f"   deck={sorted(plan.deck_cells)} door={plan.door_cell}")
