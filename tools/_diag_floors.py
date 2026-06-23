import sys; sys.path.insert(0,"tools")
import gen_freeform as gf, level_composition as lc
from collections import Counter
comp=lc.LevelComposition(mix_mode="transition",prev_faction="synth",next_faction="synth",default_faction="industrial_default")
DELTA={'N':(0,-1),'S':(0,1),'E':(1,0),'W':(-1,0)}
def g2w(gx,ix): return (ix-gx/2+0.5)*4
def run(SEED):
    fm=gf.generate_map(SEED,cells=25,composition=comp); doc=gf.to_doc(fm,'t'); ps=doc['pieces']
    spine,_,_,zfn=lc.plan_zones_for_map(fm); gx,gz=fm.gx,fm.gz
    def cell(p): return (int(round(p['x']/4+gx/2-0.5)),int(round(p['z']/4+gz/2-0.5)))
    # floor TOP height per synth cell (block=y+1.2, thin=y)
    topbycell={}
    stembycell={}
    for p in ps:
        if p.get('ceiling') or int(p.get('floor_level',0))!=0: continue
        if p.get('role') not in ('floor','deck'): continue
        c=cell(p)
        if zfn(c) not in ('prev','next'): continue
        y=float(p.get('y',0) or 0); stem=p.get('stem','')
        isblock = (stem=='floor' and p.get('kit')=='factions/synth')
        top = y+1.2 if isblock else y
        topbycell.setdefault(c,[]).append(round(top,2))
        stembycell.setdefault(c,[]).append((stem,p.get('kit'),round(y,2),'block' if isblock else 'thin'))
    # synth cells whose floor top != 1.2 (the raised/sunken line)
    badtop=[(c,topbycell[c],stembycell[c]) for c in topbycell if not all(abs(t-1.2)<0.03 for t in topbycell[c])]
    # synth cells missing a floor entirely (hole)
    nofloor=[c for c in fm.walkable if zfn(c) in ('prev','next') and c not in topbycell]
    print(f"=== seed {SEED} === synth cells with non-1.2 floor top: {len(badtop)}; synth cells with NO floor: {len(nofloor)}")
    for c,tops,stems in badtop[:8]:
        print(f"   cell {c} world({g2w(gx,c[0]):.0f},{g2w(gz,c[1]):.0f}) tops={tops} {stems}")
    if nofloor: print("   NO-FLOOR cells:", [(c,(round(g2w(gx,c[0])),round(g2w(gz,c[1])))) for c in nofloor[:8]])
for s in (2,3,4,5): run(s)
