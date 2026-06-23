import sys; sys.path.insert(0,"tools")
import gen_freeform as gf, level_composition as lc
comp=lc.LevelComposition(mix_mode="transition",prev_faction="synth",next_faction="synth",default_faction="industrial_default")
def run(SEED):
    fm=gf.generate_map(SEED,cells=25,composition=comp); doc=gf.to_doc(fm,'t'); ps=doc['pieces']
    gx,gz=fm.gx,fm.gz
    wf={(round(p['x'],1),round(p['z'],1)) for p in ps if p.get('role')in('wall','door') and int(p.get('floor_level',0))==0}
    bad=[]
    for p in ps:
        if p.get('role')!='door' or int(p.get('floor_level',0))!=0: continue
        x,z=p['x'],p['z']; fx=x/4+gx/2-0.5; fz=z/4+gz/2-0.5
        # the two flank face-positions along the door's wall line
        if abs(fx-round(fx))>0.25: fl=[(round(x,1),round(z-4,1)),(round(x,1),round(z+4,1))]
        else: fl=[(round(x-4,1),round(z,1)),(round(x+4,1),round(z,1))]
        # the two cells the flank faces would separate must be checked: bad if BOTH flanks
        # are NOT walls AND lead between two walkable cells (open room on both sides)
        openflank=0
        for (fxx,fzz) in fl:
            if (round(fxx,1),round(fzz,1)) in wf: continue  # wall there -> fine
            # is this flank between two walkable cells (open) or against void?
            ix=int(round(fxx/4+gx/2-0.5)); iz=int(round(fzz/4+gz/2-0.5))
            # flank face midpoint -> the two cells around it
            dxx=fxx/4+gx/2-0.5-round(fxx/4+gx/2-0.5)
            if abs((fxx/4+gx/2-0.5)-round(fxx/4+gx/2-0.5))>0.25:
                a=(int(round(fxx/4+gx/2-1)),iz); b=(int(round(fxx/4+gx/2)),iz)
            else:
                a=(ix,int(round(fzz/4+gz/2-1))); b=(ix,int(round(fzz/4+gz/2)))
            if a in fm.walkable and b in fm.walkable: openflank+=1
        if openflank==2: bad.append((round(x),round(z),tuple(t for t in (p.get('tags') or []) if t in ('enter_faction','exit_faction','crossing','access_repair'))))
    return bad
tot=0
for s in range(1,40):
    b=run(s); tot+=len(b)
    if b and s<=10: print(f"seed {s}: {len(b)} bad doors (open floor both flanks):",b)
print("TOTAL bad doors seeds1-39:",tot)
