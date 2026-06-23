import sys; sys.path.insert(0,"tools")
import gen_freeform as gf, level_composition as lc
comp=lc.LevelComposition(mix_mode="transition",prev_faction="synth",next_faction="synth",default_faction="industrial_default")
for SEED in (9,6,30):
    fm=gf.generate_map(SEED,cells=40,composition=comp); doc=gf.to_doc(fm,"t")
    spine,_,_,zfn=lc.plan_zones_for_map(fm)
    gx,gz=fm.gx,fm.gz
    def cell(p): return (int(round(p['x']/4+gx/2-0.5)),int(round(p['z']/4+gz/2-0.5)))
    low=[]
    for p in doc['pieces']:
        if p.get('role')!='wall': continue
        if p.get('kit')!='factions/synth': continue
        y=p.get('y',0.0)
        if y is None: y=0.0
        if y<1.19:
            low.append((cell(p),round(float(y),2),'env' if 'envelope_wall' in (p.get('tags') or []) else 'other'))
    print(f"seed {SEED}: {len(low)} synth walls below 1.2m")
    from collections import Counter
    for c,y,src in low[:12]:
        z=zfn(c)
        print(f"   cell {c} y={y} src={src} zone={z}")
