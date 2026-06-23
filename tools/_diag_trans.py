import sys, random
sys.path.insert(0,"tools")
import gen_freeform as gf, level_composition as lc, synth_transition as st, transition_entrances as te
comp=lc.LevelComposition(mix_mode="transition",prev_faction="synth",next_faction="synth",default_faction="industrial_default")
def g2w(ix): return round((ix-12.5+0.5)*4)
for SEED in (4,):
    fm=gf.generate_map(SEED,cells=25,composition=comp); doc=gf.to_doc(fm,'t'); ps=doc['pieces']
    spine,_,_,zfn=lc.plan_zones_for_map(fm); gx,gz=fm.gx,fm.gz
    rng=random.Random((SEED*1597334677)&0xFFFFFFFF)
    print(f"seed {SEED}: spine[0]={spine[0]} spine[-1]={spine[-1]}")
    # connected components of each synth zone
    import collections
    def comps(zone):
        cells={c for c in fm.walkable if zfn(c)==zone}
        seen=set(); out=[]
        for c in cells:
            if c in seen: continue
            comp_=[]; q=[c]; seen.add(c)
            while q:
                x=q.pop(); comp_.append(x)
                for dx,dz in ((0,1),(0,-1),(1,0),(-1,0)):
                    nb=(x[0]+dx,x[1]+dz)
                    if nb in cells and nb not in seen: seen.add(nb); q.append(nb)
            out.append(comp_)
        return out
    for zone in ('prev','next'):
        cc=comps(zone)
        print(f"  {zone}: {len(cc)} connected blob(s), sizes={sorted(len(c) for c in cc)}")
    print("  boundaries:")
    for b in te.find_zone_boundaries(spine,comp):
        plan=st.plan_transition(b,comp,fm.walkable,zfn,rng,ascending=(b.kind=='enter_faction'))
        print(f"    {b.kind} sub={b.substrate_cell}(w{g2w(b.substrate_cell[0])},{g2w(b.substrate_cell[1])}) fac={b.faction_cell} door={plan.door_cell} deck={sorted(plan.deck_cells)[:4]}")
    # main doors vs repair doors
    md=[p for p in ps if p.get('role')=='door' and ('enter_faction' in (p.get('tags') or []) or 'exit_faction' in (p.get('tags') or []))]
    rep=[p for p in ps if 'access_repair' in (p.get('tags') or [])]
    cross=[p for p in ps if 'crossing' in (p.get('tags') or []) and 'access_repair' not in (p.get('tags') or [])]
    print(f"  main-transition doors={len(md)} repair-pieces={len(rep)} envelope-crossings={len(cross)}")
