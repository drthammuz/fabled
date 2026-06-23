import sys; sys.path.insert(0,"tools")
import gen_freeform as gf, level_composition as lc
from collections import deque
comp=lc.LevelComposition(mix_mode="transition",prev_faction="synth",next_faction="synth",default_faction="industrial_default")
DELTA={'N':(0,-1),'S':(0,1),'E':(1,0),'W':(-1,0)}
def run(SEED):
    fm=gf.generate_map(SEED,cells=25,composition=comp); doc=gf.to_doc(fm,'t'); ps=doc['pieces']
    spine,_,_,zfn=lc.plan_zones_for_map(fm); gx,gz=fm.gx,fm.gz
    elev=lc.make_elevation_lookup(fm.walkable,spine,comp)
    def cell(p): return (int(round(p['x']/4+gx/2-0.5)),int(round(p['z']/4+gz/2-0.5)))
    def facepos(c,s): dx,dz=DELTA[s]; return ((c[0]-gx/2+0.5)*4+dx*2,(c[1]-gz/2+0.5)*4+dz*2)
    wall_faces=set(); door_faces=set()
    for p in ps:
        if int(p.get('floor_level',0))!=0 or p.get('ceiling'): continue
        if p.get('role')=='wall': wall_faces.add((round(p['x'],1),round(p['z'],1)))
        if p.get('role')=='door': door_faces.add((round(p['x'],1),round(p['z'],1)))
    stair_cells={cell(p) for p in ps if p.get('role')=='stairs'}
    def has_wall(c,s): return (lambda k: k in wall_faces)((round(facepos(c,s)[0],1),round(facepos(c,s)[1],1)))
    def has_door(c,s): return (lambda k: k in door_faces)((round(facepos(c,s)[0],1),round(facepos(c,s)[1],1)))
    def passable(a,b,s):
        if has_wall(a,s) and not has_door(a,s): return False
        ea,eb=elev(a),elev(b)
        if abs(ea-eb)>0.5:
            # need a stair on either cell to bridge the level
            return a in stair_cells or b in stair_cells
        return True
    spawn=fm.rooms[fm.spawn_room]; start=(spawn.cx,spawn.cz)
    seen={start}; q=deque([start])
    while q:
        c=q.popleft()
        for s,(dx,dz) in DELTA.items():
            nb=(c[0]+dx,c[1]+dz)
            if nb in fm.walkable and nb not in seen and passable(c,nb,s):
                seen.add(nb); q.append(nb)
    unreached=[c for c in fm.walkable if c not in seen]
    synth_un=[c for c in unreached if zfn(c) in ('prev','next')]
    # transition openings: each boundary substrate must connect default<->faction via stair
    from collections import Counter
    zc=Counter(zfn(c) for c in unreached)
    print(f"seed {SEED}: walkable={len(fm.walkable)} reached={len(seen)} UNREACHED={len(unreached)} (synth={len(synth_un)}) byzone={dict(zc)}")
    if unreached:
        print("   unreached sample:",sorted(unreached)[:12])
import sys
bad=0
for s in range(1,40):
    pass
