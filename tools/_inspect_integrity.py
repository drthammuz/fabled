import sys, random
sys.path.insert(0,"tools")
import gen_freeform as gf, level_composition as lc, synth_transition as st, transition_entrances as te
SEED=int(sys.argv[1]) if len(sys.argv)>1 else 9
comp=lc.LevelComposition(mix_mode="transition",prev_faction="synth",next_faction="synth",default_faction="industrial_default")
fm=gf.generate_map(SEED,cells=40,composition=comp)
doc=gf.to_doc(fm,f"s{SEED}")
spine,_,_,zfn=lc.plan_zones_for_map(fm)
pieces=doc["pieces"]; gx,gz=fm.gx,fm.gz
DELTA={"N":(0,-1),"S":(0,1),"E":(1,0),"W":(-1,0)}
def face(cell,side):
    dx,dz=DELTA[side]
    return ((cell[0]-gx/2+0.5)*4+dx*2,(cell[1]-gz/2+0.5)*4+dz*2)
def has(role,cell,side,eps=0.3):
    wx,wz=face(cell,side)
    for p in pieces:
        if p.get("role")!=role: continue
        if abs(p["x"]-wx)<eps and abs(p["z"]-wz)<eps: return True
    return False
# which boundaries
rng=random.Random((SEED*1597334677)&0xFFFFFFFF)
for b in te.find_zone_boundaries(spine,comp):
    plan=st.plan_transition(b,comp,fm.walkable,zfn,rng,ascending=(b.kind=="enter_faction"))
    print(f"{b.kind} w={plan.strip.width} sub={plan.strip.substrate_cells} deck={sorted(plan.deck_cells)} door={plan.door_cell}")
print("=== synth cells with a side open to non-synth & no wall/door ===")
opencount=0
for c in sorted(fm.walkable):
    if zfn(c)!="prev" and zfn(c)!="next": continue
    if zfn(c) not in ("prev","next"): continue
    for side,(dx,dz) in DELTA.items():
        nb=(c[0]+dx,c[1]+dz)
        if nb in fm.walkable and zfn(nb)==zfn(c): continue
        if has("door",c,side) or has("wall",c,side): continue
        # also a stair on substrate side counts as the entrance mouth
        nbz = zfn(nb) if nb in fm.walkable else "void"
        print(f"  {c} OPEN {side}->{nb} ({nbz})")
        opencount+=1
print("open sides:",opencount)
