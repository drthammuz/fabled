import sys, struct, math, random
sys.path.insert(0,"tools")
import gen_freeform as gf, level_composition as lc, synth_transition as st, transition_entrances as te
from pygltflib import GLTF2
SYNTH="assets/models/factions/synth"

def rail_local_x(stem):
    """Return mean local-x (rail side: negative => rail on -x)."""
    g=GLTF2().load(f"{SYNTH}/{stem}.glb"); blob=g.binary_blob(); xs=[]
    for mesh in g.meshes or []:
        for prim in mesh.primitives:
            pa=getattr(prim.attributes,"POSITION",None)
            if pa is None: continue
            acc=g.accessors[pa]; bv=g.bufferViews[acc.bufferView]
            s=(bv.byteOffset or 0)+(acc.byteOffset or 0); st_=bv.byteStride or 12
            for i in range(acc.count):
                x,y,z=struct.unpack_from("<fff",blob,s+i*st_); xs.append(x)
    return sum(xs)/len(xs)

def world_rail_dir(local_x_sign, yaw):
    # Bevy yaw about +Y: rotate local (x,0,z). Use Transform convention.
    # world_x = x*cos + z*sin ; here rail offset is along local x only.
    wx = local_x_sign*math.cos(yaw)
    wz = -local_x_sign*math.sin(yaw)
    return (round(wx,3), round(wz,3))

comp=lc.LevelComposition(mix_mode="transition",prev_faction="synth",next_faction="synth",default_faction="industrial_default")
for SEED in (33,17,30):
    fm=gf.generate_map(SEED,cells=40,composition=comp)
    spine,_,_,zfn=lc.plan_zones_for_map(fm)
    rng=random.Random((SEED*1597334677)&0xFFFFFFFF)
    for b in te.find_zone_boundaries(spine,comp):
        plan=st.plan_transition(b,comp,fm.walkable,zfn,rng,ascending=(b.kind=="enter_faction"))
        if plan.strip.width<3: continue
        toward=plan.strip.toward_faction
        yaw=st._stair_yaw_for_cell(toward, plan.ascending)
        print(f"\nseed {SEED} {b.kind} toward={toward} lat={plan.strip.lateral_axis} yaw={round(yaw,3)} ({round(math.degrees(yaw))}deg)")
        for i,s in enumerate(plan.stair_stems):
            if not s: continue
            mapped=st._stem_for_lateral_slot(s,i,plan.stair_stems,plan.ascending) if 'stems' in st._stem_for_lateral_slot.__code__.co_varnames else st._stem_for_lateral_slot(s,i,len(plan.stair_stems),plan.ascending)
            rx=rail_local_x(mapped)
            sign = 1 if rx>0.001 else (-1 if rx<-0.001 else 0)
            wd=world_rail_dir(sign,yaw) if sign else (0,0)
            print(f"  slot{i} cell={plan.strip.substrate_cells[i]} {mapped:24s} local_railx={rx:+.3f} world_rail_dir={wd}")
