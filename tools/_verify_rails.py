import sys, struct, math, random
sys.path.insert(0,"tools")
import gen_freeform as gf, level_composition as lc, synth_transition as st, transition_entrances as te
from pygltflib import GLTF2
SYNTH="assets/models/factions/synth"
_cache={}
def rail_sign(stem):
    if stem in _cache: return _cache[stem]
    g=GLTF2().load(f"{SYNTH}/{stem}.glb"); blob=g.binary_blob(); xs=[]
    for mesh in g.meshes or []:
        for prim in mesh.primitives:
            pa=getattr(prim.attributes,"POSITION",None)
            if pa is None: continue
            acc=g.accessors[pa]; bv=g.bufferViews[acc.bufferView]
            s=(bv.byteOffset or 0)+(acc.byteOffset or 0); stp=bv.byteStride or 12
            for i in range(acc.count):
                x,_,_=struct.unpack_from("<fff",blob,s+i*stp); xs.append(x)
    m=sum(xs)/len(xs); _cache[stem]= (1 if m>0.001 else (-1 if m<-0.001 else 0)); return _cache[stem]
comp=lc.LevelComposition(mix_mode="transition",prev_faction="synth",next_faction="synth",default_faction="industrial_default")
bad=0; checked=0
for SEED in range(1,40):
    try:
        fm=gf.generate_map(SEED,cells=40,composition=comp); spine,_,_,zfn=lc.plan_zones_for_map(fm)
    except: continue
    rng=random.Random((SEED*1597334677)&0xFFFFFFFF)
    for b in te.find_zone_boundaries(spine,comp):
        plan=st.plan_transition(b,comp,fm.walkable,zfn,rng,ascending=(b.kind=="enter_faction"))
        if plan.strip.width<2: continue
        yaw=st._stair_yaw_for_cell(plan.strip.toward_faction, plan.ascending)
        lo,hi=st._active_span(plan.stair_stems)
        for slot in (lo,hi):
            stem=plan.stair_stems[slot]
            if not stem: continue
            mapped=st._oriented_end_stem(stem,slot,plan.stair_stems,yaw,plan.strip.lateral_axis)
            sgn=rail_sign(mapped)
            if sgn==0: continue
            factor=math.cos(yaw) if plan.strip.lateral_axis=="x" else -math.sin(yaw)
            factor=1 if factor>=0 else -1
            world_lat = sgn*factor   # rail direction along lateral axis
            outward = -1 if slot==lo else 1
            ok = (world_lat==outward)
            checked+=1
            if not ok:
                bad+=1
                print(f"BAD seed{SEED} {b.kind} slot{slot} {mapped} yaw{round(math.degrees(yaw))} world_lat={world_lat} want{outward}")
print(f"checked {checked} end pieces, bad={bad}")
