#!/usr/bin/env python3
"""Extract + prepare the newly downloaded Kenney kits for Bevy use.

For each model kit zip in assets/downloads/, this:
  1. Extracts the GLB pieces into assets/models/<dest>/
  2. Copies the external texture sibling(s) into assets/models/<dest>/Textures/
     so each GLB's relative image URI resolves (Bevy fails the whole load if not).
     Embedded-texture kits (furniture, space-kit GLTF) need no Textures/ folder.
  3. Texture-only packs are extracted into assets/textures/<dest>/.
  4. Writes a machine-readable inventory: assets/models/kenney_kits_index.json
     (kit -> dest, theme, texture scheme, stems grouped by leading token).

OBJ-only kits (3d-road-tiles) are extracted to assets/staging/<dest>/ and flagged
needs_conversion=True (Bevy has no built-in OBJ loader).

Idempotent: re-running overwrites the extracted files in place.

Run from repo root:
    python tools/extract_kenney_kits.py
"""
from __future__ import annotations

import collections
import json
import os
import struct
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DL = ROOT / "assets" / "downloads"
MODELS = ROOT / "assets" / "models"
TEXTURES = ROOT / "assets" / "textures"
STAGING = ROOT / "assets" / "staging"

# kit zip stem (without .zip) -> config
MODEL_KITS = {
    "kenney_building-kit":            dict(dest="building",      glb_dir="Models/GLB format",  theme="Generic urban / modular buildings (brick, doorways, barricades)"),
    "kenney_factory-kit_3.0":         dict(dest="factory",       glb_dir="Models/GLB format",  theme="Industrial factory: pipes, tanks, conveyors, machinery, catwalks"),
    "kenney_furniture-kit":           dict(dest="furniture",     glb_dir="Models/GLTF format", theme="Interior furniture props (embedded textures)"),
    "kenney_modular-dungeon-kit_1.0": dict(dest="dungeon",       glb_dir="Models/GLB format",  theme="Modular stone dungeon: corridors, rooms, stairs, gates, props"),
    "kenney_prototype-kit":           dict(dest="prototype",     glb_dir="Models/GLB format",  theme="Greybox primitives + placeholder animals/items for blockout"),
    "kenney_retro-fantasy-kit":       dict(dest="retro_fantasy", glb_dir="Models/GLB format",  theme="Low-poly medieval/fantasy village (multi-texture)"),
    "kenney_space-kit":               dict(dest="space_kit",     glb_dir="Models/GLTF format", theme="Original Kenney space kit: rockets, rovers, astronauts, terrain (embedded)"),
    "kenney_space-station-kit":       dict(dest="space_station", glb_dir="Models/GLB format",  theme="Sleek space-station interior: corridors, windows, balconies, props"),
}

TEXTURE_PACKS = {
    "kenney_development-essentials":  dict(dest="dev_essentials", theme="Prototype/dev textures: checkerboard, gradient, noise, UV, normal"),
    "kenney_retro-textures-fantasy":  dict(dest="retro_fantasy",  theme="Fantasy surface textures (PNG)"),
}

OBJ_ONLY = {
    "kenney_3d-road-tiles":           dict(dest="road_tiles", theme="Road / track tiles (OBJ only - needs OBJ->GLB conversion for Bevy)"),
}


def glb_image_uris(data: bytes) -> list[str]:
    """Return external image URIs referenced by a binary glTF (empty if embedded/none)."""
    if data[:4] != b"glTF":
        return []
    clen = struct.unpack("<I", data[12:16])[0]
    j = json.loads(data[20:20 + clen])
    out = []
    for im in j.get("images", []):
        uri = im.get("uri")
        if uri:  # external; embedded images use bufferView, no uri
            out.append(uri)
    return out


def group_stems(stems: list[str]) -> dict[str, list[str]]:
    """Cluster stems by leading token (text before first '-') for a readable inventory."""
    groups: dict[str, list[str]] = collections.defaultdict(list)
    for s in sorted(stems):
        key = s.split("-")[0]
        groups[key].append(s)
    return dict(sorted(groups.items(), key=lambda kv: (-len(kv[1]), kv[0])))


def extract_model_kit(zip_stem: str, cfg: dict) -> dict:
    zpath = DL / f"{zip_stem}.zip"
    dest = MODELS / cfg["dest"]
    texdir = dest / "Textures"
    dest.mkdir(parents=True, exist_ok=True)

    zf = zipfile.ZipFile(zpath)
    names = zf.namelist()
    gdir = cfg["glb_dir"]
    glbs = [n for n in names if n.startswith(gdir + "/") and n.lower().endswith(".glb")]

    needed_tex: set[str] = set()
    stems: list[str] = []
    for n in glbs:
        data = zf.read(n)
        stem = Path(n).stem
        stems.append(stem)
        (dest / f"{stem}.glb").write_bytes(data)
        for uri in glb_image_uris(data):
            needed_tex.add(uri)  # e.g. "Textures/colormap.png"

    # Copy required textures (paths are relative to the GLB inside Models/<fmt>/).
    tex_copied = []
    for uri in sorted(needed_tex):
        # uri like "Textures/colormap.png" -> find any zip entry ending with it
        cand = [n for n in names if n.replace("\\", "/").endswith(uri)]
        if not cand:
            continue
        texdir.mkdir(parents=True, exist_ok=True)
        out = texdir / Path(uri).name
        out.write_bytes(zf.read(cand[0]))
        tex_copied.append(Path(uri).name)

    return dict(
        dest=str(dest.relative_to(ROOT)).replace("\\", "/"),
        theme=cfg["theme"],
        glb_count=len(stems),
        texture_scheme=("embedded" if not needed_tex else "external"),
        textures=tex_copied,
        groups=group_stems(stems),
    )


def extract_texture_pack(zip_stem: str, cfg: dict) -> dict:
    zpath = DL / f"{zip_stem}.zip"
    dest = TEXTURES / cfg["dest"]
    dest.mkdir(parents=True, exist_ok=True)
    zf = zipfile.ZipFile(zpath)
    pngs = [n for n in zf.namelist() if n.lower().endswith(".png")
            and not Path(n).name.lower().startswith(("preview", "sample"))]
    for n in pngs:
        out = dest / Path(n).name
        out.write_bytes(zf.read(n))
    return dict(dest=str(dest.relative_to(ROOT)).replace("\\", "/"),
                theme=cfg["theme"], png_count=len(pngs))


def extract_obj_only(zip_stem: str, cfg: dict) -> dict:
    zpath = DL / f"{zip_stem}.zip"
    dest = STAGING / cfg["dest"]
    dest.mkdir(parents=True, exist_ok=True)
    zf = zipfile.ZipFile(zpath)
    objs = [n for n in zf.namelist() if n.lower().endswith(".obj")]
    # keep the archive layout under staging for later conversion
    for n in objs:
        out = dest / Path(n).name
        out.write_bytes(zf.read(n))
    return dict(dest=str(dest.relative_to(ROOT)).replace("\\", "/"),
                theme=cfg["theme"], obj_count=len(objs), needs_conversion=True)


def main() -> None:
    index = {"model_kits": {}, "texture_packs": {}, "needs_conversion": {}}

    for zs, cfg in MODEL_KITS.items():
        print(f"[model] {zs} -> assets/models/{cfg['dest']}/ ...", flush=True)
        index["model_kits"][cfg["dest"]] = extract_model_kit(zs, cfg)

    for zs, cfg in TEXTURE_PACKS.items():
        print(f"[tex]   {zs} -> assets/textures/{cfg['dest']}/ ...", flush=True)
        index["texture_packs"].setdefault(cfg["dest"], {}).update(extract_texture_pack(zs, cfg))

    for zs, cfg in OBJ_ONLY.items():
        print(f"[obj]   {zs} -> assets/staging/{cfg['dest']}/ (needs conversion) ...", flush=True)
        index["needs_conversion"][cfg["dest"]] = extract_obj_only(zs, cfg)

    out = MODELS / "kenney_kits_index.json"
    out.write_text(json.dumps(index, indent=2), encoding="utf-8")
    print(f"\nWrote {out.relative_to(ROOT)}")
    total = sum(k["glb_count"] for k in index["model_kits"].values())
    print(f"Prepared {total} GLBs across {len(index['model_kits'])} model kits.")


if __name__ == "__main__":
    main()
