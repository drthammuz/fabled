"""Normalize a priesthood GLB after Blender export for Bevy + 3D Viewer.

Blender re-exports often:
  - embed the colormap PNG (Bevy renders white — see glb_externalize_texture.py)
  - duplicate mesh/material nodes (.001 suffix)
  - use a material block Kenney originals do not (Bevy may fail to bind textures)

This keeps mesh geometry from the edited file but replaces image/material/texture
records with the working template-wall pattern and a single external colormap URI.

Usage:
  python tools/repair_priesthood_glb.py assets/models/factions/priesthood/corridor-corner.glb
"""
from __future__ import annotations

import copy
import sys

from pygltflib import GLTF2

PRIESTHOOD = "assets/models/factions/priesthood"
TEMPLATE = f"{PRIESTHOOD}/template-wall.glb"
DEFAULT_URI = "Textures/colormap.png"


def repair(path: str, uri: str = DEFAULT_URI) -> None:
    template = GLTF2().load(TEMPLATE)
    g = GLTF2().load(path)

    # Drop duplicate Blender nodes/meshes — keep the first mesh only.
    if g.meshes and len(g.meshes) > 1:
        kept = g.meshes[0]
        g.meshes = [kept]
        for mesh in g.meshes:
            for prim in mesh.primitives:
                prim.material = 0

    if g.nodes:
        root = g.nodes[0]
        root.mesh = 0
        root.children = None
        g.nodes = [root]

    # Kenney-style material block (known good in Bevy).
    g.images = copy.deepcopy(template.images)
    g.samplers = copy.deepcopy(template.samplers or [])
    g.textures = copy.deepcopy(template.textures)
    g.materials = copy.deepcopy(template.materials)
    for mesh in g.meshes or []:
        for prim in mesh.primitives:
            prim.material = 0

    for img in g.images or []:
        img.uri = uri
        img.bufferView = None
        img.mimeType = None

    g.save(path)
    verts = g.accessors[g.meshes[0].primitives[0].attributes.POSITION].count
    print(f"{path}: repaired ({verts} verts, 1 mesh, external {uri})")


def main() -> None:
    if len(sys.argv) < 2:
        print(__doc__)
        raise SystemExit(1)
    for p in sys.argv[1:]:
        repair(p)


if __name__ == "__main__":
    main()
