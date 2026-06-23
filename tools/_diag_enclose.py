import sys; sys.path.insert(0,"tools")
import gen_freeform as gf, level_composition as lc
comp=lc.LevelComposition(mix_mode="transition",prev_faction="synth",next_faction="synth",default_faction="industrial_default")
DELTA={'N':(0,-1),'S':(0,1),'E':(1,0),'W':(-1,0)}
def run(SEED):
    fm=gf.generate_map(SEED,cells=25,composition=comp); doc=gf.to_doc(fm,'t'); ps=doc['pieces']
    spine,_,_,zfn=lc.plan_zones_for_map(fm); gx,gz=fm.gx,fm.gz
    def cell(p): return (int(round(p['x']/4+gx/2-0.5)),int(round(p['z']/4+gz/2-0.5)))
    def face(c,s): dx,dz=DELTA[s]; return (round((c[0]-gx/2+0.5)*4+dx*2,1),round((c[1]-gz/2+0.5)*4+dz*2,1))
    wf={(round(p['x'],1),round(p['z'],1)) for p in ps if p.get('role')=='wall' and int(p.get('floor_level',0))==0}
    df={(round(p['x'],1),round(p['z'],1)) for p in ps if p.get('role')=='door' and int(p.get('floor_level',0))==0}
    sc={cell(p) for p in ps if p.get('role')=='stairs'}
    def g2w(ix): return round((ix-12.5+0.5)*4)
    holes=[]
    for c in fm.walkable:
        if zfn(c) not in ('prev','next'): continue
        for s,(dx,dz) in DELTA.items():
            nb=(c[0]+dx,c[1]+dz)
            if nb in fm.walkable and zfn(nb) in ('prev','next'): continue  # interior
            k=face(c,s)
            if k in wf or k in df: continue
            if nb in sc: continue  # stair mouth
            holes.append((c,s,nb,(g2w(c[0]),g2w(c[1]))))
    print(f"=== seed {SEED}: {len(holes)} enclosure holes ===")
    for c,s,nb,w in holes[:12]:
        nbz = zfn(nb) if nb in fm.walkable else 'void'
        print(f"   synth {c} world{w} side {s} -> {nb}({nbz})")
for s in (2,3,4,5): run(s)
