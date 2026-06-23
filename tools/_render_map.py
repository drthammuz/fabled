import sys; sys.path.insert(0,"tools")
import gen_freeform as gf, level_composition as lc
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Rectangle
comp=lc.LevelComposition(mix_mode="transition",prev_faction="synth",next_faction="synth",default_faction="industrial_default")
SEED=int(sys.argv[1]) if len(sys.argv)>1 else 5
fm=gf.generate_map(SEED,cells=25,composition=comp); doc=gf.to_doc(fm,'t'); ps=doc['pieces']
spine,_,_,zfn=lc.plan_zones_for_map(fm); gx,gz=fm.gx,fm.gz
def cellf(p): return (p['x']/4+gx/2-0.5, p['z']/4+gz/2-0.5)
fig,ax=plt.subplots(figsize=(14,14))
# floors colored by top height
for p in ps:
    if int(p.get('floor_level',0))!=0 or p.get('ceiling'): continue
    if p.get('role') not in ('floor','deck'): continue
    fx,fz=cellf(p); stem=p.get('stem','')
    y=float(p.get('y',0) or 0)
    isblock=(stem=='floor' and p.get('kit')=='factions/synth')
    top=y+1.2 if isblock else y
    if 'hole' in stem: col=(1,0,0)
    elif top>1.1: col=(0.6,0.8,1.0)   # elevated synth surface
    elif top>0.01: col=(1,1,0.5)
    else: col=(0.85,0.85,0.85)         # industrial ~0
    ax.add_patch(Rectangle((fx-0.5,fz-0.5),1,1,facecolor=col,edgecolor='none'))
# walls/doors/stairs as edges/marks
for p in ps:
    if int(p.get('floor_level',0))!=0 or p.get('ceiling'): continue
    fx,fz=cellf(p); r=p.get('role')
    if r=='wall':
        # wall on a face: determine orientation by fractional part
        dx=fx-round(fx); dz=fz-round(fz)
        col='black' if abs(float(p.get('y',0) or 0))>0.5 else (0.4,0.2,0)  # synth(1.2) black, industrial(0) brown
        if abs(dx)>0.25: ax.plot([fx,fx],[fz-0.5,fz+0.5],color=col,lw=2.5)
        elif abs(dz)>0.25: ax.plot([fx-0.5,fx+0.5],[fz,fz],color=col,lw=2.5)
    elif r=='door':
        ax.plot(fx,fz,'gs',ms=8)
    elif r=='stairs':
        ax.plot(fx,fz,'b^',ms=10)
sp=fm.rooms[fm.spawn_room]
ax.plot(sp.cx,sp.cz,'m*',ms=22)
ax.set_xlim(-1,gx); ax.set_ylim(gz,-1); ax.set_aspect('equal'); ax.grid(True,lw=0.3,alpha=0.3)
ax.set_xticks(range(0,gx)); ax.set_yticks(range(0,gz))
ax.set_title(f"seed {SEED} @cells=25  (blue=synth1.2 grey=industrial red=hole; black=synth wall brown=industrial wall; green=door blue^=stair pink*=spawn)")
plt.savefig(f"tools/_map_seed{SEED}.png",dpi=80,bbox_inches='tight')
print(f"wrote tools/_map_seed{SEED}.png")
