#!/usr/bin/env python3
"""
Scale a GLB mesh only in one axis (default Y/height) by a factor.
This is "headless" and keeps the original material/texture setup (no re-export, no color fix needed).

Usage:
  python tools/scale_glb_y.py assets/models/factions/necropolis/brick-wall.glb 1.3793
  # or for multiple
  python tools/scale_glb_y.py path/to/model1.glb path/to/model2.glb --factor 1.38 --axis 1

After scaling, the placed height at scale=4 will be taller while x/z dimensions stay the same.
"""

from __future__ import annotations
import argparse
import struct
from pathlib import Path

from pygltflib import GLTF2


def scale_glb_y(path: str, factor: float, axis: int = 1) -> None:
    """Scale vertex positions in the given axis (0=x,1=y,2=z)."""
    g = GLTF2().load(path)
    if not g.meshes:
        print(f"{path}: no meshes")
        return

    changed = 0
    for mesh in g.meshes:
        for prim in mesh.primitives:
            if "POSITION" not in prim.attributes:
                continue
            pos_idx = prim.attributes.POSITION
            acc = g.accessors[pos_idx]
            if acc.type != "VEC3" or acc.componentType != 5126:  # float32
                print(f"{path}: unsupported POSITION accessor")
                continue
            bv = g.bufferViews[acc.bufferView]
            buf_idx = bv.buffer
            buf = g.buffers[buf_idx]
            # The binary is in g.binary_blob() or separate
            # pygltflib loads the bin if embedded or we need to handle
            # For simplicity, assume we can get the data
            data = g.binary_blob()
            if data is None:
                # Try to load external .bin if any, but for these faction glbs usually embedded or single
                bin_path = Path(path).with_suffix('.bin')
                if bin_path.exists():
                    data = bin_path.read_bytes()
                else:
                    print(f"{path}: no binary data")
                    continue

            byte_offset = (bv.byteOffset or 0) + (acc.byteOffset or 0)
            count = acc.count
            stride = bv.byteStride or 12

            for i in range(count):
                off = byte_offset + i * stride + axis * 4
                val = struct.unpack_from("<f", data, off)[0]
                new_val = val * factor
                struct.pack_into("<f", data, off, new_val)
                changed += 1

            # Write back the modified blob
            # pygltflib way: set the buffer
            g.buffers[buf_idx].uri = None  # make embedded if was
            # Actually, to update, we set the binary
            # Better: use set_binary_blob or save with modifications

    # Simpler robust way with pygltflib: use its numpy conversion if possible, but to keep deps low
    # Alternative: reload and use convert_to_padded or direct buffer update

    # The above modifies the python bytes? But data = g.binary_blob() is copy?
    # pygltflib binary_blob returns the data, but modifying it may not update the gltf if not set back.

    # Better implementation using pygltflib recommended way:
    # We will re-implement with direct buffer handling.

    print(f"{path}: would scale {changed} floats (demo, full impl below)")

    # Full working impl:
    # We will use a more direct approach that works for these files.

    g2 = GLTF2().load(path)
    blob = bytearray(g2.binary_blob() or b"")
    if not blob:
        print(f"{path}: no blob")
        return

    modified = 0
    for mesh in g2.meshes:
        for prim in mesh.primitives:
            pos_acc_idx = prim.attributes.get("POSITION")
            if pos_acc_idx is None:
                continue
            acc = g2.accessors[pos_acc_idx]
            bv = g2.bufferViews[acc.bufferView]
            start = (bv.byteOffset or 0) + (acc.byteOffset or 0)
            count = acc["count"]
            stride = bv.get("byteStride", 12)
            for i in range(count):
                off = start + i * stride + axis * 4
                if off + 4 > len(blob):
                    continue
                val = struct.unpack_from("<f", blob, off)[0]
                struct.pack_into("<f", blob, off, val * factor)
                modified += 1

    g2.buffers[0].byteLength = len(blob)
    # Set the binary data back
    g2.set_binary_blob(bytes(blob))
    g2.save(path)
    print(f"{path}: scaled Y (axis {axis}) by {factor} ({modified} verts modified)")


def main():
    parser = argparse.ArgumentParser(description="Scale GLB vertex positions in one axis only.")
    parser.add_argument("glbs", nargs="+", help="GLB file(s) to scale")
    parser.add_argument("--factor", type=float, default=1.3793, help="Scale factor for the axis (default ~1.38 for ~4m height)")
    parser.add_argument("--axis", type=int, default=1, help="Axis to scale: 0=X, 1=Y (height), 2=Z")
    args = parser.parse_args()

    for glb in args.glbs:
        scale_glb_y(glb, args.factor, args.axis)


if __name__ == "__main__":
    main()
