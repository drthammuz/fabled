"""Repoint a GLB's embedded image(s) to an external texture URI.

Some DCC re-exports embed the colormap inside the .glb. Bevy's loader has not
been reliably decoding those embedded PNGs in this project (renders white),
while the external `Textures/colormap.png` reference used by every other piece
works. This rewrites the image record(s) to reference the external file so the
piece picks up the shared (recolorable) texture.

Usage: python tools/glb_externalize_texture.py <glb_path> [uri]
       uri defaults to "Textures/colormap.png"
"""
import sys
from pygltflib import GLTF2


def main() -> None:
    path = sys.argv[1]
    uri = sys.argv[2] if len(sys.argv) > 2 else "Textures/colormap.png"
    g = GLTF2().load(path)
    changed = 0
    for img in g.images:
        if img.uri != uri or img.bufferView is not None:
            img.uri = uri
            img.bufferView = None
            img.mimeType = None
            changed += 1
    g.save(path)
    print(f"{path}: repointed {changed} image(s) -> {uri}")


if __name__ == "__main__":
    main()
