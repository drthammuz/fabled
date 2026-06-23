import sys, math
sys.path.insert(0,"tools")
import gen_freeform as gf, level_composition as lc
from collections import Counter, defaultdict
comp=lc.LevelComposition(mix_mode="transition",prev_faction="synth",next_faction="synth",default_faction="industrial_default")
def run(SEED):
    fm=gf.generate_map(SEED,cells=25,composition=comp); doc=gf.to_doc(fm,"t")
    ps=doc['pieces']; spine,_,_,zfn=lc.plan_zones_for_map(fm); gx,gz=fm.gx,fm.gz
    def cell(p): return (int(round(p['x']/4+gx/2-0.5)),int(round(p['z']/4+gz/2-0.5)))
    # door stacks: group doors by rounded (x,z)
    doors=[p for p in ps if p.get('role')=='door']
    bykey=Counter((round(p['x'],1),round(p['z'],1)) for p in doors)
    stacks=[(k,v) for k,v in bykey.items() if v>1]
    # door-in-wall: door & wall at same face pos
    walls=[p for p in ps if p.get('role')=='wall']
    wkeys={(round(p['x'],1),round(p['z'],1)) for p in walls}
    dinw=[(round(p['x'],1),round(p['z'],1)) for p in doors if (round(p['x'],1),round(p['z'],1)) in wkeys]
    # structure-barrier count
    sb=[p for p in ps if p.get('stem')=='structure-barrier']
    # spawn
    spawn=fm.rooms[fm.spawn_room]; sc=(spawn.cx,spawn.cz)
    spawn_y=lc.make_elevation_lookup(fm.walkable,spine,comp)(sc)
    # floor under spawn
    floors_at=[p for p in ps if p.get('role') in ('floor','deck') and cell(p)==sc and int(p.get('floor_level',0))==0 and not p.get('ceiling')]
    fz=[(p.get('stem'),round(float(p.get('y',0) or 0),2)) for p in floors_at]
    print(f"seed {SEED}: spawn={sc} zone={zfn(sc)} spawn_y={spawn_y} floor_under_spawn={fz}")
    print(f"   doors={len(doors)} stacks(>1 at same pt)={len(stacks)} {stacks[:5]}")
    print(f"   doors_in_wall={len(dinw)} {dinw[:5]}")
    print(f"   structure-barriers={len(sb)}")
for s in (2,3,4,5,6,7,8,9,10):
    run(s)
