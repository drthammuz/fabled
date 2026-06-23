import sys; sys.path.insert(0,"tools")
import gen_freeform as gf, level_composition as lc
comp=lc.LevelComposition(mix_mode="transition",prev_faction="synth",next_faction="synth",default_faction="industrial_default")
def run(SEED, cx, cz, rad=2):
    fm=gf.generate_map(SEED,cells=25,composition=comp); doc=gf.to_doc(fm,'t'); ps=doc['pieces']
    spine,_,_,zfn=lc.plan_zones_for_map(fm); gx,gz=fm.gx,fm.gz
    def cell(p): return (p['x']/4+gx/2-0.5, p['z']/4+gz/2-0.5)
    print(f"=== seed {SEED} around grid ({cx},{cz}) ===")
    print("  zones:", {(ix,iz):zfn((ix,iz))[0] if (ix,iz) in fm.walkable else '.' for iz in range(cz-rad,cz+rad+1) for ix in range(cx-rad,cx+rad+1)})
    rows=[]
    for p in ps:
        if int(p.get('floor_level',0))!=0 or p.get('ceiling'): continue
        fx,fz=cell(p)
        if abs(fx-cx)>rad+0.6 or abs(fz-cz)>rad+0.6: continue
        if p.get('role') not in ('floor','deck','wall','door','stairs'): continue
        rows.append((round(fx,2),round(fz,2),p.get('role'),p.get('stem',''),round(float(p.get('y',0) or 0),2),round(p.get('yaw',0),2)))
    for r in sorted(rows, key=lambda r:(r[1],r[0])):
        print("   ",r)
run(5,20,14)
run(5,20,13)
print("\n##### SEED 4 transition (3,10) #####")
run(4,3,10,3)
print("\n##### SEED 3 door (11,22) #####")
run(3,11,22,3)
