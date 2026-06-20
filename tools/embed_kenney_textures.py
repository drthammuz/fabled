#!/usr/bin/env python3
"""Embed external Textures/colormap.png into self-contained GLB copies.

Windows 3D Viewer cannot resolve Kenney's external texture path; only models
with embedded textures (e.g. gate-lasers.glb) preview correctly when opened alone.

Writes: assets/models/space/viewer/<name>.glb

Usage (from repo root):
  python tools/embed_kenney_textures.py
"""

from __future__ import annotations

import glob
import os
import sys

from pygltflib import GLTF2
from pygltflib.utils import ImageFormat

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SPACE = os.path.join(ROOT, "assets", "models", "space")
VIEWER = os.path.join(SPACE, "viewer")


def embed_one(src: str, dst: str) -> str:
    gltf = GLTF2().load(src)
    if gltf.images:
        gltf.convert_images(ImageFormat.DATAURI)
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    gltf.save(dst)
    return "embedded" if gltf.images else "no-texture"


def main() -> int:
    paths = sorted(glob.glob(os.path.join(SPACE, "*.glb")))
    if not paths:
        print(f"no GLBs in {SPACE}", file=sys.stderr)
        return 1

    ok = 0
    for src in paths:
        name = os.path.basename(src)
        dst = os.path.join(VIEWER, name)
        try:
            status = embed_one(src, dst)
            print(f"ok  {name} ({status})")
            ok += 1
        except Exception as exc:
            print(f"FAIL {name}: {exc}", file=sys.stderr)

    print(f"\n{ok}/{len(paths)} -> {VIEWER}")
    print("Open files from viewer/ in 3D Viewer (textures are inside the GLB).")
    return 0 if ok == len(paths) else 1


if __name__ == "__main__":
    raise SystemExit(main())
