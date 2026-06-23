import sys, random
sys.path.insert(0,"tools")
import gen_freeform as gf, level_composition as lc, synth_transition as st, transition_entrances as te
comp=lc.LevelComposition(mix_mode="transition",prev_faction="synth",next_faction="synth",default_faction="industrial_default")
best=[]
for seed in range(1,40):
    try:
        fm=gf.generate_map(seed,cells=40,composition=comp)
        spine,_,_,zfn=lc.plan_zones_for_map(fm)
    except Exception as e:
        continue
    rng=random.Random((seed*1597334677)&0xFFFFFFFF)
    for b in te.find_zone_boundaries(spine,comp):
        plan=st.plan_transition(b,comp,fm.walkable,zfn,rng,ascending=(b.kind=="enter_faction"))
        w=plan.strip.width
        if w>=2:
            best.append((w,seed,b.kind,plan.strip.substrate_cells,[st._stem_for_lateral_slot(s,i,len(plan.stair_stems)) for i,s in enumerate(plan.stair_stems)]))
best.sort(reverse=True)
for b in best[:15]:
    print(f"w={b[0]} seed={b[1]} {b[2]} sub={b[3]}")
    print(f"    stems={b[4]}")
