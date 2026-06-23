#!/usr/bin/env python3
"""Mirror a GLB across the X axis (creates a right-hand variant from a left-hand piece).

Usage:
  python tools/mirror_glb.py assets/models/factions/synth/stairs-small-edge.glb \\
      assets/models/factions/synth/stairs-small-edge-r.glb
"""
from __future__ import annotations

import struct
import sys
from pathlib import Path

from pygltflib import GLTF2

COMP = {5120: "b", 5121: "B", 5122: "h", 5123: "H", 5125: "I", 5126: "f"}
TYPE = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}


def _elem_fmt(acc) -> str:
    c = COMP[acc.componentType]
    n = TYPE[acc.type]
    return f"<{c * n}"


def _elem_size(acc) -> int:
    return struct.calcsize(_elem_fmt(acc))


def _flip_accessor(gltf: GLTF2, blob: bytearray, acc_idx: int, *, flip_x: bool, flip_u: bool) -> None:
    acc = gltf.accessors[acc_idx]
    bv = gltf.bufferViews[acc.bufferView]
    start = (bv.byteOffset or 0) + (acc.byteOffset or 0)
    stride = bv.byteStride or _elem_size(acc)
    esize = _elem_size(acc)
    for i in range(acc.count):
        off = start + i * stride
        if acc.type == "VEC3" and acc.componentType == 5126 and flip_x:
            x, y, z = struct.unpack_from("<fff", blob, off)
            struct.pack_into("<fff", blob, off, -x, y, z)
        elif acc.type == "VEC2" and acc.componentType == 5126 and flip_u:
            u, v = struct.unpack_from("<ff", blob, off)
            struct.pack_into("<ff", blob, off, 1.0 - u, v)


def _flip_triangles(gltf: GLTF2, blob: bytearray, acc_idx: int) -> None:
    acc = gltf.accessors[acc_idx]
    bv = gltf.bufferViews[acc.bufferView]
    start = (bv.byteOffset or 0) + (acc.byteOffset or 0)
    esize = _elem_size(acc)
    if acc.componentType == 5123:
        for i in range(acc.count // 3):
            off = start + i * esize * 3
            a, b, c = struct.unpack_from("<HHH", blob, off)
            struct.pack_into("<HHH", blob, off, a, c, b)
    elif acc.componentType == 5125:
        for i in range(acc.count // 3):
            off = start + i * esize * 3
            a, b, c = struct.unpack_from("<III", blob, off)
            struct.pack_into("<III", blob, off, a, c, b)


def mirror_glb(src: Path, dst: Path) -> None:
    gltf = GLTF2().load(str(src))
    blob = bytearray(gltf.binary_blob() or b"")
    if not blob:
        raise ValueError(f"no binary blob in {src}")

    for mesh in gltf.meshes or []:
        for prim in mesh.primitives:
            attrs = prim.attributes
            pos = getattr(attrs, "POSITION", None)
            if pos is not None:
                _flip_accessor(gltf, blob, pos, flip_x=True, flip_u=False)
            nrm = getattr(attrs, "NORMAL", None)
            if nrm is not None:
                _flip_accessor(gltf, blob, nrm, flip_x=True, flip_u=False)
            # NOTE: do NOT flip TEXCOORD_0. The Kenney ``colormap.png`` is a colour
            # *atlas* (each face's UV points at one solid swatch), not a tiling
            # surface texture. Flipping u (u -> 1-u) moves every face to a mirrored
            # column of the atlas → wrong colours (the "white/odd-coloured flipped
            # stair"). Geometry/winding mirror is enough; keep UVs so swatches match.
            if prim.indices is not None:
                _flip_triangles(gltf, blob, prim.indices)

    gltf.set_binary_blob(bytes(blob))
    dst.parent.mkdir(parents=True, exist_ok=True)
    gltf.save(str(dst))
    print(f"{dst}: mirrored from {src.name}")


def main() -> None:
    if len(sys.argv) < 3:
        print(__doc__)
        raise SystemExit(1)
    mirror_glb(Path(sys.argv[1]), Path(sys.argv[2]))


if __name__ == "__main__":
    main()
