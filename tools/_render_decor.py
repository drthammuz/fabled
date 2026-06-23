import sys; sys.path.insert(0,"tools")
import gen_freeform as gf, level_composition as lc
import matplotlib; matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Rectangle
comp=lc.LevelComposition(mix_mode="transition",prev_faction="synth",next_faction="synth",default_faction="industrial_default")
SEED=int(sys.argv[1]) if len(sys.argv)>1 else 5
fm=gf.generate_map(SEED,cells=25,composition=comp); doc=gf.to_doc(fm,'t'); ps=doc['pieces']
spine,_,_,zfn=lc.plan_zones_for_map(fm); gx,gz=fm.gx,fm.gz
def cf(p): return (p['x']/4+gx/2-0.5, p['z']/4+gz/2-0.5)
fig,ax=plt.subplots(figsize=(13,13))
for p in ps:
    if int(p.get('floor_level',0))!=0 or p.get('ceiling'): continue
    if p.get('role') not in ('floor','deck'): continue
    fx,fz=cf(p); stem=p.get('stem',''); y=float(p.get('y',0) or 0)
    isb=(stem=='floor' and p.get('kit')=='factions/synth'); top=y+1.2 if isb else y
    col=(1,0,0) if 'hole' in stem else (0.6,0.8,1.0) if top>1.1 else (0.85,0.85,0.85)
    ax.add_patch(Rectangle((fx-0.5,fz-0.5),1,1,facecolor=col,edgecolor='none'))
for p in ps:
    if int(p.get('floor_level',0))!=0 or p.get('ceiling'): continue
    fx,fz=cf(p); r=p.get('role'); stem=p.get('stem','')
    if r=='wall':
        dx=fx-round(fx); dz=fz-round(fz)
        if stem=='wall-window': col='deepskyblue'; lw=4
        elif stem=='wall-banner': col='purple'; lw=4
        else: col='black' if abs(float(p.get('y',0) or 0))>0.5 else (0.55,0.27,0.07); lw=2.5
        if abs(dx)>0.25: ax.plot([fx,fx],[fz-0.5,fz+0.5],color=col,lw=lw)
        elif abs(dz)>0.25: ax.plot([fx-0.5,fx+0.5],[fz,fz],color=col,lw=lw)
    elif r=='door': ax.plot(fx,fz,'gs',ms=7)
    elif r=='stairs': ax.plot(fx,fz,'y^',ms=9)
sp=fm.rooms[fm.spawn_room]; ax.plot(sp.cx,sp.cz,'m*',ms=20)
ax.set_xlim(-1,gx); ax.set_ylim(gz,-1); ax.set_aspect('equal')
ax.set_title(f"seed {SEED}: blue lines=windows purple=banners black=synth-wall green=door yellow^=stair")
plt.savefig(f"tools/_decor_s{SEED}.png",dpi=78,bbox_inches='tight'); print("wrote",f"tools/_decor_s{SEED}.png")

# (props overlay appended)
