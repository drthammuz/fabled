#!/usr/bin/env python3
"""Top-down render incl. PROP footprints (rotation-aware) for any composition.

Usage:
  python tools/_render_props.py <seed> [prev] [mid] [next] [out.png]

Floors grey (synth deck light blue), walls black/brown, doors green squares,
stairs yellow triangles, props as filled rectangles colored by kit with stem
labels. Corridor cells hatched so prop-in-corridor violations are obvious.
"""
import sys, math
sys.path.insert(0, "tools")
import gen_freeform as gf, level_composition as lc
import faction_interior as fi
import matplotlib; matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Rectangle, Polygon

SEED = int(sys.argv[1]) if len(sys.argv) > 1 else 5
prev_f = sys.argv[2] if len(sys.argv) > 2 else "necropolis"
mid_f = sys.argv[3] if len(sys.argv) > 3 else "industrial_default"
next_f = sys.argv[4] if len(sys.argv) > 4 else "priesthood"
out = sys.argv[5] if len(sys.argv) > 5 else f"tools/_props_s{SEED}_{prev_f}-{next_f}.png"

comp = lc.LevelComposition(mix_mode="transition", prev_faction=prev_f,
                           next_faction=next_f, default_faction=mid_f)
fm = gf.generate_map(SEED, cells=25, composition=comp)
doc = gf.to_doc(fm, "t")
ps = doc["pieces"]
gx, gz = fm.gx, fm.gz

def cf(x, z):
    return (x / 4 + gx / 2 - 0.5, z / 4 + gz / 2 - 0.5)

KIT_COLORS = {
    "factions/necropolis": "darkred", "factions/urban": "darkorange",
    "factory": "teal", "retro_fantasy": "saddlebrown",
    "factions/synth": "navy", "factions/priesthood": "grey",
    "factions/industrial": "olive",
}

fig, ax = plt.subplots(figsize=(15, 15))
for p in ps:
    if int(p.get("floor_level", 0)) != 0 or p.get("ceiling"):
        continue
    if p.get("role") not in ("floor", "deck"):
        continue
    fx, fz = cf(p["x"], p["z"])
    stem = p.get("stem", ""); y = float(p.get("y", 0) or 0)
    isb = (stem == "floor" and p.get("kit") == "factions/synth")
    top = y + 1.2 if isb else y
    col = (1, 0, 0) if "hole" in stem else (0.75, 0.85, 1.0) if top > 1.1 else (0.92, 0.92, 0.92)
    ax.add_patch(Rectangle((fx - 0.5, fz - 0.5), 1, 1, facecolor=col, edgecolor="none"))
for (ix, iz) in fm.corridor_cells:
    ax.add_patch(Rectangle((ix - 0.5, iz - 0.5), 1, 1, facecolor="none",
                           edgecolor="0.7", hatch="//", lw=0))
for p in ps:
    if int(p.get("floor_level", 0)) != 0 or p.get("ceiling"):
        continue
    fx, fz = cf(p["x"], p["z"]); r = p.get("role")
    if r == "wall":
        col = "black" if abs(float(p.get("y", 0) or 0)) > 0.5 else (0.45, 0.25, 0.1)
        if "seam" in str(p.get("tags", [])):
            col = "magenta"
        yaw = float(p.get("yaw", 0.0)) % math.pi  # 0 = spans x, pi/2 = spans z
        if abs(yaw - math.pi / 2) < 0.3:
            ax.plot([fx, fx], [fz - 0.5, fz + 0.5], color=col, lw=2.5)
        else:
            ax.plot([fx - 0.5, fx + 0.5], [fz, fz], color=col, lw=2.5)
    elif r == "door":
        ax.plot(fx, fz, "gs", ms=9, zorder=6)
    elif r == "stairs":
        ax.plot(fx, fz, "y^", ms=9, zorder=6)

# Props: rotation-aware footprint from the same catalogs the placer used.
import json
from pathlib import Path
CATS = {}
def cat_bounds(kit, stem):
    key = kit
    if key not in CATS:
        CATS[key] = {}
        for fac_dir in (fi.FACTIONS_DIR).iterdir():
            for name in ("placement_catalog.json", "prop_catalog.json"):
                f = fac_dir / name
                if not f.exists():
                    continue
                d = json.loads(f.read_text())
                match = (kit == f"factions/{fac_dir.name}") or (d.get("prop_kit") == kit)
                if match:
                    CATS[key].update(d.get("stems", {}))
    e = CATS[key].get(stem, {})
    return e.get("bounds_scale1", {"x0": -0.4, "x1": 0.4, "z0": -0.4, "z1": 0.4})

for p in ps:
    if p.get("role") != "prop":
        continue
    b = cat_bounds(p.get("kit", ""), p["stem"])
    s = float(p.get("scale", 1)); yaw = float(p.get("yaw", 0))
    cy, sy = math.cos(yaw), math.sin(yaw)
    pts = []
    for lx, lz in ((b["x0"], b["z0"]), (b["x0"], b["z1"]), (b["x1"], b["z1"]), (b["x1"], b["z0"])):
        wx = p["x"] + (lx * cy + lz * sy) * s
        wz = p["z"] + (-lx * sy + lz * cy) * s
        pts.append(cf(wx, wz))
    col = KIT_COLORS.get(p.get("kit", ""), "green")
    elevated = float(p.get("y", 0) or 0) > 1.4
    ax.add_patch(Polygon(pts, closed=True, facecolor=col,
                         alpha=0.25 if elevated else 0.55, edgecolor=col, lw=1.2))
    ax.annotate(p["stem"][:10], cf(p["x"], p["z"]), fontsize=4.5, ha="center", color=col)

sp = fm.rooms[fm.spawn_room]; ax.plot(sp.cx, sp.cz, "m*", ms=18)
ax.set_xlim(-1, gx); ax.set_ylim(gz, -1); ax.set_aspect("equal")
ax.set_title(f"seed {SEED} {prev_f} -> {mid_f} -> {next_f} | props={sum(1 for p in ps if p.get('role')=='prop')}")
plt.savefig(out, dpi=90, bbox_inches="tight")
print("wrote", out)
