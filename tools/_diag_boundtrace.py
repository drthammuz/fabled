import sys, random
sys.path.insert(0,"tools")
import gen_freeform as gf, level_composition as lc, synth_transition as st, transition_entrances as te
comp=lc.LevelComposition(mix_mode="transition",prev_faction="synth",next_faction="synth",default_faction="industrial_default")
def g2w(ix): return round((ix-12.5+0.5)*4)
for SEED in (1,2,3,4,5):
    fm=gf.generate_map(SEED,cells=25,composition=comp); doc=gf.to_doc(fm,'t'); ps=doc['pieces']
    spine,_,_,zfn=lc.plan_zones_for_map(fm); gx,gz=fm.gx,fm.gz
    def cell(p): return (int(round(p['x']/4+gx/2-0.5)),int(round(p['z']/4+gz/2-0.5)))
    stair_cells={cell(p) for p in ps if p.get('role')=='stairs'}
    door_cells={cell(p) for p in ps if p.get('role')=='door'}
    rng=random.Random((SEED*1597334677)&0xFFFFFFFF)
    print(f"=== seed {SEED} ===")
    for b in te.find_zone_boundaries(spine,comp):
        plan=st.plan_transition(b,comp,fm.walkable,zfn,rng,ascending=(b.kind=='enter_faction'))
        # is there a stair piece on the substrate cells?
        sub_has_stair = any(s in stair_cells for s in plan.strip.substrate_cells)
        door_present = plan.door_cell in door_cells
        print(f"  {b.kind} sub={b.substrate_cell} w({g2w(b.substrate_cell[0])},{g2w(b.substrate_cell[1])}) "
              f"stems={plan.stair_stems} STAIR_EMITTED={sub_has_stair} door={plan.door_cell} DOOR_EMITTED={door_present} deck={len(plan.deck_cells)}")
