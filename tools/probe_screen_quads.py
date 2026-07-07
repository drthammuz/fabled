#!/usr/bin/env python3
"""
Pass F terminals: probe computer/monitor/screen prop GLBs (all kits) for the
flat SCREEN quad, so the client can overlay an emissive terminal texture on
that exact rectangle.

Method (no Blender needed — pure GLB parse, pattern: probe_faction_catalog.py):
- walk the glTF node tree with full transforms (space_kit meshes are offset),
- group triangles into coplanar planar regions,
- score regions as "screen glass": large, rectangular (high fill ratio),
  roughly vertical, and DARK (colormap kits: sample the atlas at the tri UVs;
  material kits: 'dark'/'metalDark' baseColorFactor),
- emit the winner (plus close runners-up for double-sided/multi-monitor stems)
  into assets/models/screen_catalog.json:
    key "kit/stem" -> quads: [{center, normal, up, size_wh}] in GLB LOCAL units
  (the client scales by the placed piece's scale, as with all kit props).

Run: python tools/probe_screen_quads.py            (writes catalog + report)
"""

from __future__ import annotations

import json
import math
import struct
from pathlib import Path
from typing import Dict, List, Optional, Tuple

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
MODELS = ROOT / "assets" / "models"
OUT = MODELS / "screen_catalog.json"

# kit-relative GLB -> probe. Key in the catalog is "<kit>/<stem>" where <kit>
# is the path under assets/models/ (matches the runtime `models/{kit}/{stem}.glb`).
CANDIDATES = [
    # synth faction (space-station kit re-export; the placed stems in maps)
    "factions/synth/computer.glb",
    "factions/synth/computer-screen.glb",
    "factions/synth/computer-wide.glb",
    "factions/synth/computer-system.glb",
    "factions/synth/display-wall.glb",
    "factions/synth/display-wall-wide.glb",
    "factions/synth/table-display.glb",
    "factions/synth/table-display-small.glb",
    "factions/synth/table-display-planet.glb",
    # factory kit (industrial zones)
    "factory/screen-flat.glb",
    "factory/screen-small.glb",
    "factory/screen-wide.glb",
    "factory/screen-hanging-small.glb",
    "factory/screen-hanging-wide.glb",
    "factory/screen-panel-flat.glb",
    "factory/screen-panel-small.glb",
    "factory/screen-panel-wide.glb",
    # space_station originals (same geometry as synth; probed for completeness)
    "space_station/computer.glb",
    "space_station/computer-screen.glb",
    "space_station/computer-wide.glb",
    "space_station/computer-system.glb",
    "space_station/display-wall.glb",
    "space_station/display-wall-wide.glb",
    # desk sets
    "furniture/computerScreen.glb",
    "space_kit/desk_computer.glb",
    "space_kit/desk_computerCorner.glb",
    "space_kit/desk_computerScreen.glb",
]

# 'metalDark' is CASING in the space/furniture kits — only 'dark' is glass.
DARK_MATERIALS = {"dark", "black", "screen"}

# Per-stem curation (reviewed against probe output 2026-07-07; front faces
# VERIFIED by Blender renders from ±quad-normal 2026-07-07 round 2 — the
# "dark = glass" heuristic had picked the monitor BACK panel on every
# space-station-family stem, so quads rendered facing the wall and the
# terminal's front-side check rejected the player from the playable side).
# Rules:
#   "best1"      keep the top-scored quad (default; verified correct for the
#                factory kit — its screens really are dark navy glass)
#   "orange"     keep the best ORANGE region (space-station colormap: the
#                screen swatch is rgb≈(255,180,72); back panels are grey-blue)
#   "sym4"       keep up to 4 quads within 0.75 of the best score (holo tables
#                readable from all sides)
#   "mat:<name>" keep the best quad using that material (space_kit desks: the
#                'dark' angled console face ranks below big casing faces;
#                furniture monitor: the light 'metal' face is the screen —
#                'metalDark' is its BACK casing, render-verified)
#   None         no usable screen — excluded from the catalog
KEEP_RULES: Dict[str, Optional[str]] = {
    "factions/synth/computer": "orange",
    "factions/synth/computer-screen": "orange",
    "factions/synth/computer-wide": "orange",
    "factions/synth/computer-system": "orange",  # small angled console screen
    "factions/synth/display-wall": "orange",
    "factions/synth/display-wall-wide": "orange",
    "factions/synth/table-display": "sym4",
    "factions/synth/table-display-small": "sym4",
    "factions/synth/table-display-planet": None,   # holo planet, no flat screen
    "space_station/computer": "orange",
    "space_station/computer-screen": "orange",
    "space_station/computer-wide": "orange",
    "space_station/computer-system": "orange",
    "space_station/display-wall": "orange",
    "space_station/display-wall-wide": "orange",
    "space_station/table-display": "sym4",
    "space_station/table-display-small": "sym4",
    "space_station/table-display-planet": None,
    "space_kit/desk_computer": "mat:dark",
    "space_kit/desk_computerCorner": "mat:dark",
    "space_kit/desk_computerScreen": "mat:dark",
    "furniture/computerScreen": "mat:metal",
}


def is_orange_screen(reg) -> bool:
    """Space-station colormap screen swatch (rgb≈(255,180,72))."""
    rgb = reg.get("rgb")
    return bool(rgb) and rgb[0] >= 170 and rgb[0] > rgb[2] + 40


# --- GLB / glTF ------------------------------------------------------------

def load_glb(glb: Path) -> Tuple[dict, bytes]:
    data = glb.read_bytes()
    json_len = struct.unpack_from("<II", data, 12)[0]
    doc = json.loads(data[20:20 + json_len])
    bin_off = 20 + json_len + 8
    return doc, data[bin_off:]


COMP_FMT = {5120: "b", 5121: "B", 5122: "h", 5123: "H", 5125: "I", 5126: "f"}
TYPE_N = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}


def read_accessor(doc: dict, blob: bytes, idx: int) -> List[tuple]:
    acc = doc["accessors"][idx]
    bv = doc["bufferViews"][acc["bufferView"]]
    start = bv.get("byteOffset", 0) + acc.get("byteOffset", 0)
    n = TYPE_N[acc["type"]]
    fmt = COMP_FMT[acc["componentType"]]
    size = struct.calcsize(fmt) * n
    stride = bv.get("byteStride") or size
    out = []
    for i in range(acc["count"]):
        out.append(struct.unpack_from(f"<{n}{fmt}", blob, start + i * stride))
    return out


def quat_mat(q) -> List[List[float]]:
    x, y, z, w = q
    return [
        [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
        [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
        [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
    ]


def node_local(node: dict) -> Tuple[List[List[float]], List[float]]:
    if "matrix" in node:
        m = node["matrix"]  # column-major
        rot = [[m[0], m[4], m[8]], [m[1], m[5], m[9]], [m[2], m[6], m[10]]]
        return rot, [m[12], m[13], m[14]]
    rot = quat_mat(node.get("rotation", [0, 0, 0, 1]))
    s = node.get("scale", [1, 1, 1])
    rot = [[rot[r][c] * s[c] for c in range(3)] for r in range(3)]
    return rot, list(node.get("translation", [0, 0, 0]))


def compose(pr, pt, cr, ct):
    rot = [[sum(pr[r][k] * cr[k][c] for k in range(3)) for c in range(3)] for r in range(3)]
    t = [sum(pr[r][k] * ct[k] for k in range(3)) + pt[r] for r in range(3)]
    return rot, t


def xform(rot, t, v):
    return tuple(sum(rot[r][k] * v[k] for k in range(3)) + t[r] for r in range(3))


# --- vector helpers ----------------------------------------------------------

def sub(a, b): return (a[0] - b[0], a[1] - b[1], a[2] - b[2])
def cross(a, b): return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])
def dot(a, b): return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
def norm(a):
    l = math.sqrt(dot(a, a))
    return (a[0] / l, a[1] / l, a[2] / l) if l > 1e-12 else (0.0, 0.0, 0.0)


# --- colormap sampling -------------------------------------------------------

class TexSampler:
    def __init__(self, glb: Path, doc: dict):
        self.img = None
        try:
            texs = doc.get("textures", [])
            imgs = doc.get("images", [])
            if texs and imgs:
                uri = imgs[texs[0].get("source", 0)].get("uri")
                if uri:
                    p = (glb.parent / uri).resolve()
                    if p.exists():
                        self.img = Image.open(p).convert("RGB")
        except Exception:
            self.img = None

    def sample(self, uv) -> Optional[Tuple[int, int, int]]:
        if self.img is None or uv is None:
            return None
        w, h = self.img.size
        u = min(max(uv[0] % 1.0, 0.0), 1.0)
        v = min(max(uv[1] % 1.0, 0.0), 1.0)
        return self.img.getpixel((min(int(u * w), w - 1), min(int(v * h), h - 1)))


def mat_base_color(doc: dict, mi: Optional[int]) -> Tuple[Optional[str], Optional[Tuple[int, int, int]]]:
    if mi is None or mi >= len(doc.get("materials", [])):
        return None, None
    m = doc["materials"][mi]
    f = m.get("pbrMetallicRoughness", {}).get("baseColorFactor")
    rgb = tuple(int(c * 255) for c in f[:3]) if f else None
    return m.get("name"), rgb


# --- planar region extraction ------------------------------------------------

def gather_triangles(glb: Path):
    """Yield (v0,v1,v2, uv0,uv1,uv2, mat_index) in GLB scene space."""
    doc, blob = load_glb(glb)
    sampler = TexSampler(glb, doc)
    tris = []

    def walk(ni: int, prot, pt):
        node = doc["nodes"][ni]
        lrot, lt = node_local(node)
        rot, t = compose(prot, pt, lrot, lt)
        mi = node.get("mesh")
        if mi is not None:
            for prim in doc["meshes"][mi].get("primitives", []):
                attrs = prim.get("attributes", {})
                if "POSITION" not in attrs:
                    continue
                pos = [xform(rot, t, v) for v in read_accessor(doc, blob, attrs["POSITION"])]
                uvs = (read_accessor(doc, blob, attrs["TEXCOORD_0"])
                       if "TEXCOORD_0" in attrs else None)
                if "indices" in prim:
                    idx = [i[0] for i in read_accessor(doc, blob, prim["indices"])]
                else:
                    idx = list(range(len(pos)))
                pm = prim.get("material")
                for k in range(0, len(idx) - 2, 3):
                    a, b, c = idx[k], idx[k + 1], idx[k + 2]
                    tris.append((pos[a], pos[b], pos[c],
                                 uvs[a] if uvs else None,
                                 uvs[b] if uvs else None,
                                 uvs[c] if uvs else None, pm))
        for ch in node.get("children", []):
            walk(ch, rot, t)

    scene = doc.get("scenes", [{}])[doc.get("scene", 0)]
    ident = [[1, 0, 0], [0, 1, 0], [0, 0, 1]]
    for ni in scene.get("nodes", []):
        walk(ni, ident, [0, 0, 0])
    return doc, sampler, tris


def planar_regions(tris):
    """Group triangles by quantized plane (normal, offset)."""
    groups: Dict[tuple, list] = {}
    for tri in tris:
        v0, v1, v2 = tri[0], tri[1], tri[2]
        n = cross(sub(v1, v0), sub(v2, v0))
        area2 = math.sqrt(dot(n, n))
        if area2 < 1e-9:
            continue
        n = (n[0] / area2, n[1] / area2, n[2] / area2)
        d = dot(n, v0)
        key = (round(n[0], 2), round(n[1], 2), round(n[2], 2), round(d, 3))
        groups.setdefault(key, []).append((tri, area2 * 0.5))
    return groups


def analyze_region(key, members, sampler, doc):
    n = norm(key[:3])
    # in-plane axes: up = world-Y projected onto plane (screens read upright);
    # fall back for near-horizontal quads (table displays)
    upw = (0.0, 1.0, 0.0)
    if abs(dot(n, upw)) > 0.95:
        upw = (0.0, 0.0, 1.0)
    right = norm(cross(upw, n))
    up = norm(cross(n, right))
    area = sum(a for _, a in members)
    us, vs = [], []
    colors = []
    for (tri, _a) in members:
        for i in range(3):
            v = tri[i]
            us.append(dot(v, right))
            vs.append(dot(v, up))
        cuv = tri[3]
        if cuv is not None:
            # centroid uv of the tri (kenney palette: all 3 usually identical)
            uv = [(tri[3][0] + tri[4][0] + tri[5][0]) / 3,
                  (tri[3][1] + tri[4][1] + tri[5][1]) / 3]
            c = sampler.sample(uv)
            if c:
                colors.append(c)
        else:
            _, rgb = mat_base_color(doc, tri[6])
            if rgb:
                colors.append(rgb)
    w = max(us) - min(us)
    h = max(vs) - min(vs)
    rect_area = w * h
    fill = area / rect_area if rect_area > 1e-9 else 0.0
    # region 3D center from the 2D rect center
    cu = (max(us) + min(us)) / 2
    cv = (max(vs) + min(vs)) / 2
    cd = key[3]
    center = tuple(right[i] * cu + up[i] * cv + n[i] * cd for i in range(3))
    if colors:
        r = sum(c[0] for c in colors) / len(colors)
        g = sum(c[1] for c in colors) / len(colors)
        b = sum(c[2] for c in colors) / len(colors)
        lum = 0.2126 * r + 0.7152 * g + 0.0722 * b
        avg_rgb = (round(r), round(g), round(b))
    else:
        lum, avg_rgb = 255.0, None
    mat_names = set()
    for (tri, _a) in members:
        name, _ = mat_base_color(doc, tri[6])
        if name:
            mat_names.add(name)
    return {
        "normal": [round(x, 4) for x in n],
        "up": [round(x, 4) for x in up],
        "center": [round(x, 4) for x in center],
        "size_wh": [round(w, 4), round(h, 4)],
        "area": round(area, 5),
        "fill": round(fill, 3),
        "lum": round(lum, 1),
        "rgb": avg_rgb,
        "mats": sorted(mat_names),
        "tris": len(members),
    }


def score(reg, bbox_diag: float) -> float:
    """Higher = more screen-like."""
    n = reg["normal"]
    s = 0.0
    s += min(reg["area"] / (bbox_diag * bbox_diag * 0.02), 3.0)   # big
    s += 2.0 * min(reg["fill"], 1.0)                              # solid rect
    s += 1.5 * (1.0 - abs(n[1]))                                  # vertical-ish
    s += 2.0 * (1.0 - min(reg["lum"], 160.0) / 160.0)             # dark glass
    if set(reg["mats"]) & DARK_MATERIALS:
        s += 3.0
    ar = max(reg["size_wh"]) / max(min(reg["size_wh"]), 1e-6)
    if ar > 6.0:                                                  # strips = bezels
        s -= 2.0
    return s


def probe(glb_rel: str):
    glb = MODELS / glb_rel
    doc, sampler, tris = gather_triangles(glb)
    if not tris:
        return None, []
    allv = [v for t in tris for v in (t[0], t[1], t[2])]
    dx = max(v[0] for v in allv) - min(v[0] for v in allv)
    dy = max(v[1] for v in allv) - min(v[1] for v in allv)
    dz = max(v[2] for v in allv) - min(v[2] for v in allv)
    diag = math.sqrt(dx * dx + dy * dy + dz * dz)
    regs = []
    for key, members in planar_regions(tris).items():
        reg = analyze_region(key, members, sampler, doc)
        if reg["area"] < 1e-4 or min(reg["size_wh"]) < 0.02:
            continue
        reg["score"] = round(score(reg, diag), 2)
        regs.append(reg)
    regs.sort(key=lambda r: -r["score"])
    return {"bbox": [round(dx, 3), round(dy, 3), round(dz, 3)]}, regs


def main() -> None:
    catalog = {}
    for rel in CANDIDATES:
        glb = MODELS / rel
        if not glb.exists():
            print(f"SKIP (missing) {rel}")
            continue
        meta, regs = probe(rel)
        stem_key = rel[:-4]  # strip .glb -> "kit/stem"
        if not regs:
            print(f"NO REGIONS    {rel}")
            continue
        rule = KEEP_RULES.get(stem_key, "best1")
        if rule is None:
            print(f"EXCLUDED      {rel} (no usable screen)")
            continue
        if rule == "best1":
            quads = [regs[0]]
        elif rule == "sym4":
            best = regs[0]
            quads = [r for r in regs[:6] if r["score"] >= best["score"] - 0.75][:4]
        elif rule == "orange":
            orange = [r for r in regs if is_orange_screen(r)]
            if not orange:
                print(f"EXCLUDED      {rel} (no orange-screen quad)")
                continue
            quads = [orange[0]]
        elif rule.startswith("mat:"):
            want = rule[4:]
            dark = [r for r in regs if want in r["mats"]]
            if not dark:
                print(f"EXCLUDED      {rel} (no '{want}'-material quad)")
                continue
            quads = [dark[0]]
        else:
            raise ValueError(rule)
        catalog[stem_key] = {
            "bbox": meta["bbox"],
            "quads": [{"center": q["center"], "normal": q["normal"],
                       "up": q["up"], "size_wh": q["size_wh"]} for q in quads],
        }
        print(f"--- {rel}  bbox={meta['bbox']}")
        for q in regs[:4]:
            mark = " *" if q in quads else "  "
            print(f"  {mark} score={q['score']:5.2f} n={q['normal']} "
                  f"c={q['center']} wh={q['size_wh']} fill={q['fill']} "
                  f"lum={q['lum']} rgb={q['rgb']} mats={q['mats']}")
    OUT.write_text(json.dumps(catalog, indent=1), encoding="utf-8")
    print(f"\nwrote {OUT} ({len(catalog)} stems)")


if __name__ == "__main__":
    main()
