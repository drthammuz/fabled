import sys; sys.path.insert(0,"tools")
import gen_freeform as gf, level_composition as lc
import matplotlib; matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Rectangle
comp=lc.LevelComposition(mix_mode="transition",prev_faction="synth",next_faction="synth",default_faction="industrial_default")
SEED=int(sys.argv[1]); x0,x1,z0,z1=[int(a) for a in sys.argv[2:6]]
fm=gf.generate_map(SEED,cells=25,composition=comp); doc=gf.to_doc(fm,'t'); ps=doc['pieces']
spine,_,_,zfn=lc.plan_zones_for_map(fm); gx,gz=fm.gx,fm.gz
def cf(p): return (p['x']/4+gx/2-0.5, p['z']/4+gz/2-0.5)
fig,ax=plt.subplots(figsize=(11,11))
for p in ps:
    if int(p.get('floor_level',0))!=0 or p.get('ceiling'): continue
    if p.get('role') not in ('floor','deck'): continue
    fx,fz=cf(p); stem=p.get('stem',''); y=float(p.get('y',0) or 0)
    isblock=(stem=='floor' and p.get('kit')=='factions/synth'); top=y+1.2 if isblock else y
    col=(1,0,0) if 'hole' in stem else (0.6,0.8,1.0) if top>1.1 else (1,1,0.5) if top>0.01 else (0.85,0.85,0.85)
    ax.add_patch(Rectangle((fx-0.5,fz-0.5),1,1,facecolor=col,edgecolor=(0,0,0,0.1)))
    ax.text(fx,fz+0.32,f"{top:.2f}",ha='center',va='center',fontsize=6,color='gray')
for p in ps:
    if int(p.get('floor_level',0))!=0 or p.get('ceiling'): continue
    fx,fz=cf(p); r=p.get('role')
    if r=='wall':
        dx=fx-round(fx); dz=fz-round(fz)
        col='black' if abs(float(p.get('y',0) or 0))>0.5 else (0.55,0.27,0.07)
        if abs(dx)>0.25: ax.plot([fx,fx],[fz-0.5,fz+0.5],color=col,lw=4)
        elif abs(dz)>0.25: ax.plot([fx-0.5,fx+0.5],[fz,fz],color=col,lw=4)
    elif r=='door': ax.plot(fx,fz,'gs',ms=14)
    elif r=='stairs': ax.plot(fx,fz,'b^',ms=16)
sp=fm.rooms[fm.spawn_room]
if x0<=sp.cx<=x1 and z0<=sp.cz<=z1: ax.plot(sp.cx,sp.cz,'m*',ms=26)
ax.set_xlim(x0-0.6,x1+0.6); ax.set_ylim(z1+0.6,z0-0.6); ax.set_aspect('equal'); ax.grid(True,lw=0.4)
ax.set_xticks(range(x0,x1+1)); ax.set_yticks(range(z0,z1+1))
ax.set_title(f"seed {SEED} zoom x{x0}-{x1} z{z0}-{z1} (numbers=floor top height)")
plt.savefig(f"tools/_zoom_s{SEED}_{x0}_{z0}.png",dpi=90,bbox_inches='tight'); print("wrote",f"tools/_zoom_s{SEED}_{x0}_{z0}.png")
