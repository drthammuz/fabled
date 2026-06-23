import struct
from pygltflib import GLTF2
SYNTH="assets/models/factions/synth"
def uv_stats(f):
    g=GLTF2().load(f"{SYNTH}/{f}.glb"); blob=g.binary_blob(); us=[];vs=[]
    for mesh in g.meshes or []:
        for prim in mesh.primitives:
            ta=getattr(prim.attributes,"TEXCOORD_0",None)
            if ta is None: continue
            acc=g.accessors[ta]; bv=g.bufferViews[acc.bufferView]
            start=(bv.byteOffset or 0)+(acc.byteOffset or 0); stride=bv.byteStride or 8
            for i in range(acc.count):
                u,v=struct.unpack_from("<ff",blob,start+i*stride); us.append(u); vs.append(v)
    return (round(min(us),4),round(max(us),4),round(min(vs),4),round(max(vs),4),len(us))
for f in ["stairs-small-edge","stairs-small-edge-r","floor"]:
    print(f, "u(min,max) v(min,max) n =", uv_stats(f))
