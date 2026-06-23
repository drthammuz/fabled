import sys, struct
from pygltflib import GLTF2
SYNTH="assets/models/factions/synth"
files=["stairs-small-edge","stairs-small-edge-r","stairs-small-corner","stairs-small-corner-r",
       "stairs-small-corner-inner","stairs-small-corner-inner-r","stairs-small-edges","floor"]
def pos_stats(g):
    blob=g.binary_blob()
    xs=[]
    for mesh in g.meshes or []:
        for prim in mesh.primitives:
            pa=getattr(prim.attributes,"POSITION",None)
            if pa is None: continue
            acc=g.accessors[pa]; bv=g.bufferViews[acc.bufferView]
            start=(bv.byteOffset or 0)+(acc.byteOffset or 0)
            stride=bv.byteStride or 12
            for i in range(acc.count):
                x,y,z=struct.unpack_from("<fff",blob,start+i*stride); xs.append(x)
    if not xs: return None
    return (min(xs),max(xs),sum(xs)/len(xs))
def mat_info(g):
    out=[]
    for m in g.materials or []:
        pbr=m.pbrMetallicRoughness
        bc=getattr(pbr,"baseColorFactor",None) if pbr else None
        tex=getattr(pbr,"baseColorTexture",None) if pbr else None
        ti=tex.index if tex else None
        out.append((m.name,bc,ti))
    return out
def imgs(g):
    return [(im.uri, im.bufferView) for im in (g.images or [])]
for f in files:
    p=f"{SYNTH}/{f}.glb"
    try:
        g=GLTF2().load(p)
    except Exception as e:
        print(f"{f}: LOAD FAIL {e}"); continue
    st=pos_stats(g)
    print(f"\n{f}: xmin/xmax/xmean={tuple(round(v,3) for v in st) if st else None}")
    print(f"   materials={mat_info(g)}")
    print(f"   images={imgs(g)}")
