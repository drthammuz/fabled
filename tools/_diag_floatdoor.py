import sys; sys.path.insert(0,"tools")
import gen_freeform as gf, level_composition as lc
comp=lc.LevelComposition(mix_mode="transition",prev_faction="synth",next_faction="synth",default_faction="industrial_default")
DELTA={'N':(0,-1),'S':(0,1),'E':(1,0),'W':(-1,0)}
def g2w(ix): return round((ix-12.5+0.5)*4)
def run(SEED):
    fm=gf.generate_map(SEED,cells=25,composition=comp); doc=gf.to_doc(fm,'t'); ps=doc['pieces']
    gx,gz=fm.gx,fm.gz
    wf={(round(p['x'],1),round(p['z'],1)) for p in ps if p.get('role')in('wall','door') and int(p.get('floor_level',0))==0}
    floats=[]
    for p in ps:
        if p.get('role')!='door' or int(p.get('floor_level',0))!=0: continue
        x,z=p['x'],p['z']
        fx=x/4+gx/2-0.5; fz=z/4+gz/2-0.5
        # door on vertical line (E-W face) -> flanks are N/S along same x; else flanks E/W
        if abs(fx-round(fx))>0.25:  # vertical wall line at this x
            fl=[(round(x,1),round(z-4,1)),(round(x,1),round(z+4,1))]
        else:
            fl=[(round(x-4,1),round(z,1)),(round(x+4,1),round(z,1))]
        flanked=sum(1 for k in fl if k in wf)
        if flanked==0:
            floats.append((round(x),round(z),tuple(p.get('tags') or [])[-2:]))
    print(f"seed {SEED}: {len(floats)} floating doors (no flanking wall):")
    for f in floats: print("   world",(f[0],f[1]),"tags",f[2])
for s in (1,2,3,4,5): run(s)
