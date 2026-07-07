#!/usr/bin/env python3
"""
Generalized probe for all faction GLBs.

Catalogues EVERY stem with:
- physical bounds (GLB vertices)
- visual features (CoM, flatness, protrusions, clearances)
- inferred rotation/front
- purpose/usage (name-based + rules)
- relations (e.g. computer + chair)
- density/placement hints

Output: per-faction placement_catalog.json under assets/models/factions/<id>/

Double-check this output as LLM: review for accuracy, add/adjust purpose/relations manually in follow-up if needed.

Run: python tools/probe_faction_catalog.py
Then use in placement planners.
"""

from __future__ import annotations

import json
import struct
from dataclasses import dataclass, asdict
from pathlib import Path
from typing import Dict, List, Tuple

ROOT = Path(__file__).resolve().parents[1]
FACTIONS_DIR = ROOT / "assets" / "models" / "factions"
FACTIONS = ["synth", "necropolis", "priesthood", "urban", "industrial"]

# Name-based purpose inference rules (extend as needed)
PURPOSE_RULES = {
    "bed": ("sleep", "place in quarters against wall, head to wall"),
    "chair": ("seating", "face desk/computer, front clearance required"),
    "computer": ("technical", "workstation; pair with chair in front"),
    "table": ("surface", "center or against wall for clusters"),
    "container": ("storage", "stack or line against walls"),
    "grave": ("burial", "rows in crypt, aligned"),
    "candle": ("light", "near graves/altars"),
    "bench": ("seating", "rows facing focal, or against wall"),
    "pallet": ("storage", "stacked or floor"),
    "detail": ("decor", "small, against walls or on surfaces"),
    "altar": ("focal", "center of chapel"),
    "rock": ("decor", "scatter, edges"),
    "fence": ("boundary", "perimeter"),
    "wall": ("structure", "perimeter"),
    "floor": ("structure", "base"),
    "stairs": ("transition", "connect levels, base support"),
    "gate": ("access", "doorways"),
    "pipe": ("utility", "along walls"),
    "display": ("info", "wall mounted"),
    "pallet": ("storage", "stack or against wall"),
    "detail-bench": ("surface/seating", "table-like or seating in breakrooms"),
    "detail-block": ("storage", "crate-like, stack in storage"),
    "detail-barrier": ("boundary", "perimeter or divider"),
    "hay": ("storage/decor", "scatter or stack"),
    "urn": ("decor", "near altars"),
    "pumpkin": ("decor", "seasonal scatter"),
}

RELATION_RULES = {
    "computer": [{"with": "chair", "offset_z": -1.5, "yaw": 0, "clearance": 1.0, "note": "chair in front facing screen"}],
    "altar": [{"with": "bench", "rows": True, "facing": True, "clearance": 1.5}],
    "grave": [{"with": "candle", "offset_z": 0.8, "yaw": 0, "clearance": 0.5}],
}

DENSITY_HINTS = {
    "bed": {"weight": 3, "prefers_edge": True, "avoid_center": True},
    "chair": {"weight": 2, "cluster": True},
    "computer": {"weight": 2.5, "cluster": True},
    "default": {"weight": 1, "avoid_center": True},
}

CELL_M = 4.0
DEFAULT_SCALE = 4.0

# --- Kit-unit scale calibration -------------------------------------------
# WHY past scaling was off: every Kenney kit here is authored on a normalized
# grid where wall stems are EXACTLY 1.0 unit tall (graveyard brick-wall=1.0,
# retro_urban wall-a=1.0, space_station wall=1.0, retro_fantasy column=1.0),
# and one map cell is 4 m — so props need x4, the same factor the user-approved
# synth interior already uses. Placing them at scale=1 made everything toy-size.
# The factory kit is the exception: it is authored chunkier (machine=1.3 units,
# conveyor=0.4 units belt height ≈ waist at x2), so it gets x2.
KIT_BASE_SCALE = {
    "factory": 2.0,
    # Priesthood props read small next to the 4 m stone pillars — chunkier
    # low-poly reads better slightly oversized (user pass 2026-07-03). Tall
    # items (columns/ladders/pulleys) are unaffected: the 3.4 m height clamp
    # already caps them below this base.
    "retro_fantasy": 4.8,
}
FACTION_BASE_SCALE = {
    "synth": 4.0,
    "necropolis": 3.0,
    "urban": 4.0,
    "priesthood": 4.0,   # faction-folder items; dungeon architecture stays scale 1 (4-unit kit)
    "industrial": 2.0,
}
# Per-stem clamps so the base scale never produces absurd pieces:
MAX_PROP_HEIGHT_M = 3.4     # rooms have 4 m clear height
MAX_PROP_FOOTPRINT_M = 3.6  # props must fit inside one 4 m cell with margin


def compute_auto_scale(height_m: float, half_x_m: float, half_z_m: float,
                       base: float) -> float:
    """Largest scale <= base keeping the prop under room height / cell footprint."""
    s = base
    if height_m > 1e-6:
        s = min(s, MAX_PROP_HEIGHT_M / height_m)
    footprint = 2.0 * max(half_x_m, half_z_m)
    if footprint > 1e-6:
        s = min(s, MAX_PROP_FOOTPRINT_M / footprint)
    return round(max(0.25, min(s, base)), 3)


@dataclass
class ItemCatalog:
    class_: str = "prop"
    bounds_scale1: Dict[str, float] = None
    half_x_m: float = 0.5
    half_z_m: float = 0.5
    height_m: float = 0.0
    front: str = "+z"
    snap: str = "floor"
    visual: Dict = None
    purpose: str = "decor"
    usage: str = "place on floor"
    relations: List[Dict] = None
    density: Dict = None
    tags: List[str] = None
    notes: str = ""

    def to_dict(self):
        d = asdict(self)
        d["class"] = d.pop("class_")
        return d


def load_glb_verts(glb: Path) -> List[Tuple[float, float, float]]:
    data = glb.read_bytes()
    chunk_len = struct.unpack_from("<II", data, 12)[0]
    doc = json.loads(data[20:20+chunk_len])
    bin_off = 20 + chunk_len + 8
    blob = data[bin_off:]
    verts = []
    for mesh in doc.get("meshes", []):
        for prim in mesh.get("primitives", []):
            pos_i = prim.get("attributes", {}).get("POSITION")
            if pos_i is None: continue
            acc = doc["accessors"][pos_i]
            bv = doc["bufferViews"][acc["bufferView"]]
            start = bv.get("byteOffset", 0) + acc.get("byteOffset", 0)
            for i in range(acc["count"]):
                verts.append(struct.unpack_from("<fff", blob, start + i * 12))
    return verts


def compute_bounds(verts):
    if not verts:
        return {"x0": -0.5, "x1": 0.5, "y0": 0, "y1": 0.3, "z0": -0.5, "z1": 0.5}
    xs = [v[0] for v in verts]
    ys = [v[1] for v in verts]
    zs = [v[2] for v in verts]
    return {
        "x0": min(xs), "x1": max(xs),
        "y0": min(ys), "y1": max(ys),
        "z0": min(zs), "z1": max(zs)
    }


def infer_purpose(stem: str) -> tuple[str, str]:
    stem_l = stem.lower()
    for key, (purp, use) in PURPOSE_RULES.items():
        if key in stem_l:
            return purp, use
    return "decor", "place on floor or against wall"


def infer_relations(stem: str) -> List[Dict]:
    stem_l = stem.lower()
    for key, rels in RELATION_RULES.items():
        if key in stem_l:
            return rels
    return []


def infer_density(stem: str) -> Dict:
    stem_l = stem.lower()
    for key, d in DENSITY_HINTS.items():
        if key in stem_l:
            return d
    return DENSITY_HINTS["default"]


def probe_stem(glb: Path, stem: str, base_scale: float = DEFAULT_SCALE) -> Dict:
    verts = load_glb_verts(glb)
    bb = compute_bounds(verts)
    hx = (bb["x1"] - bb["x0"]) / 2
    hz = (bb["z1"] - bb["z0"]) / 2
    height = bb["y1"] - bb["y0"]
    purpose, usage = infer_purpose(stem)
    rels = infer_relations(stem)
    dens = infer_density(stem)
    tags = [purpose, "placement"]
    if "wall" in stem.lower(): tags.append("wall")
    if "floor" in stem.lower(): tags.append("floor")

    visual = {
        "center_of_mass": {"x": 0, "y": height/2, "z": 0},
        "top_flatness": 0.8 if "floor" in stem.lower() else 0.3,
        "front_protrusion_local_z": hz,
        "needs_front_clearance": "chair" in stem.lower() or "computer" in stem.lower(),
        "suggested_front_clearance_m": 0.75 if "seating" in purpose else 0.3,
    }

    # Front axis probed from geometry: the bulky UPPER side (backrests, screen
    # mounts, back panels) is the back — the front faces away from the
    # upper-half vertex-mass bias. Only override the default when the bias is
    # clear (>10% of depth); near-symmetric items keep +z.
    front = "+z" if "chair" not in stem.lower() else "-z"
    if verts and hz > 1e-6:
        y_mid = (bb["y0"] + bb["y1"]) / 2.0
        uz = [v[2] for v in verts if v[1] > y_mid]
        if uz:
            bias = sum(uz) / len(uz)
            if abs(bias) / (2.0 * hz) > 0.1:
                front = "-z" if bias > 0 else "+z"
            else:
                # Ambiguous body — check the topmost band: an open/hinged lid
                # (dumpsters) or crown detail sits at the BACK edge.
                top = [v[2] for v in verts
                       if v[1] > bb["y1"] - (bb["y1"] - bb["y0"]) * 0.1]
                if top:
                    tbias = sum(top) / len(top)
                    if abs(tbias) / (2.0 * hz) > 0.25:
                        front = "-z" if tbias > 0 else "+z"

    is_structure = any(k in stem for k in ["floor", "wall", "corner", "stairs"])
    return {
        "class": "structure" if is_structure else "prop",
        "bounds_scale1": bb,
        "half_x_m": hx,
        "half_z_m": hz,
        "height_m": height,
        # Recommended placement scale for PROPS (structure stems are placed by
        # base-gen with the faction manifest scale, not this).
        "auto_scale": compute_auto_scale(height, hx, hz, base_scale),
        "front": front,
        "snap": "floor" if "floor" in stem.lower() else "wall" if "wall" in stem.lower() else "floor",
        "visual": visual,
        "purpose": purpose,
        "usage": usage,
        "relations": rels,
        "density": dens,
        "tags": tags,
        "notes": f"Auto-catalogued from GLB. Physical bounds; visual CoM/flatness inferred. Doublecheck for {stem}."
    }


MODELS_DIR = ROOT / "assets" / "models"

# Which kit folder holds the dressing/prop items for each faction.
# These map to assets/models/<kit>/ and use kit = "<kit>" in placed pieces.
PROP_KIT_MAP: Dict[str, str] = {
    "industrial": "factory",
    "priesthood": "retro_fantasy",
}

# Curated allow-lists of stems appropriate for *interior room dressing*.
# Excludes terrain tiles, vehicles, structural walls, UI elements, etc.
INTERIOR_PROP_ALLOW: Dict[str, set] = {
    "factory": {
        # Machinery (focal / large)
        "machine", "machine-window", "machine-fortified", "machine-connection-pipe",
        # Conveyors (runs along walls or centre; part-end stems cap a run)
        "conveyor", "conveyor-stripe", "conveyor-sides", "conveyor-corner",
        "conveyor-bars", "conveyor-bars-stripe",
        "conveyor-stripe-part-end", "conveyor-stripe-part-middle",
        "conveyor-long-part-end", "conveyor-long-part-middle",
        # Pipes (metal + glass families share the same footprint per stem name)
        "pipe-large", "pipe-large-bend", "pipe-large-long", "pipe-large-junction",
        "pipe-large-curve", "pipe-large-valve",
        "pipe-glass-large", "pipe-glass-large-bend", "pipe-glass-large-long",
        "pipe-glass-large-junction", "pipe-glass-large-curve", "pipe-glass-large-valve",
        # Screens / info
        "screen-flat", "screen-wide", "screen-small",
        "screen-panel-flat", "screen-panel-wide", "screen-panel-small",
        "screen-hanging-wide", "screen-hanging-small",
        # Hoppers / storage
        "hopper-round", "hopper-square", "hopper-high-round", "hopper-high-square",
        "box-large", "box-small", "box-long", "box-wide",
        # Mechanical
        "piston-round", "piston-square", "piston-thin-round", "piston-thin-square",
        "cog-a", "cog-b", "cog-c", "cog-d", "cog-e",
        "robot-arm-a", "robot-arm-b",
        "crane", "crane-magnet",
        "scanner-high", "scanner-low",
        # Catwalks (decorative, against walls)
        "catwalk-straight", "catwalk-corner", "catwalk-junction", "catwalk-stairs",
        # Signs / warnings
        "warning-orange", "warning-traffic",
        # Buttons / levers (small accent)
        "button-floor-round", "button-floor-square",
        "lever",
    },
    "retro_fantasy": {
        # Barrels / crates (storage)
        "barrels", "detail-barrel",
        "detail-crate", "detail-crate-small", "detail-crate-ropes",
        # Scattered debris
        "bricks",
        # Columns (chapel / ruins)
        "column", "column-damaged", "column-paint", "column-wood",
        # Utility / accent
        "ladder",
        "fence-wood", "fence",
        "pulley", "pulley-crate",
        # Structural remnants (ruins rooms)
        "structure-pole", "structure-wall",
        "structure-poles",
        # Props
        "tree-shrub", "tree-large",
    },
}

# Purpose overrides for prop-kit items (supplement default name-based inference).
PROP_PURPOSE_OVERRIDES: Dict[str, Dict[str, tuple]] = {
    "factory": {
        "machine":               ("machinery", "focal or against wall, face room"),
        "machine-window":        ("machinery", "focal or against wall, face room"),
        "machine-fortified":     ("machinery", "heavy machinery, corner or centre"),
        "conveyor":              ("conveyor", "run along wall or floor, directional"),
        "conveyor-stripe":       ("conveyor", "run along wall or floor, directional"),
        "conveyor-sides":        ("conveyor", "run along wall or floor, directional"),
        "conveyor-corner":       ("conveyor", "conveyor corner turn"),
        "pipe-large":            ("utility", "along walls or ceiling-drop"),
        "pipe-large-bend":       ("utility", "pipe corner, along walls"),
        "pipe-large-junction":   ("utility", "pipe junction, along walls"),
        "screen-flat":           ("info", "wall-adjacent, face room"),
        "screen-wide":           ("info", "wall-adjacent, face room"),
        "screen-panel-wide":     ("info", "wall-adjacent, face room"),
        "hopper-round":          ("storage", "corner or against wall"),
        "hopper-square":         ("storage", "corner or against wall"),
        "box-large":             ("storage", "stack or against wall"),
        "box-small":             ("storage", "scatter or stack"),
        "robot-arm-a":           ("machinery", "against wall, working arm"),
        "robot-arm-b":           ("machinery", "against wall, working arm"),
        "crane":                 ("machinery", "large, corner or centre of big rooms"),
        "scanner-high":          ("scanner", "against wall, face room"),
        "scanner-low":           ("scanner", "floor or wall-adjacent"),
        "warning-orange":        ("decor", "small accent near machinery"),
        "warning-traffic":       ("decor", "small accent near exits"),
    },
    "retro_fantasy": {
        "barrels":               ("storage", "stack against wall"),
        "detail-barrel":         ("storage", "single barrel against wall"),
        "detail-crate":          ("storage", "stack or against wall"),
        "detail-crate-small":    ("storage", "small crate scatter"),
        "detail-crate-ropes":    ("storage", "crate with ropes, against wall"),
        "bricks":                ("decor", "scatter on floor, ruins feel"),
        "column":                ("structural", "upright, flanking altar or doorway"),
        "column-damaged":        ("structural", "ruins column, scatter or corner"),
        "column-wood":           ("structural", "wooden post, against wall"),
        "ladder":                ("access", "against wall, vertical"),
        "fence-wood":            ("boundary", "low divider, edge of rooms"),
        "pulley":                ("utility", "against wall or ceiling"),
        "pulley-crate":          ("utility", "storage/lift against wall"),
        "structure-pole":        ("structural", "ruins remnant, scatter"),
        "structure-wall":        ("structural", "ruins wall fragment, against wall"),
        "tree-shrub":            ("decor", "organic accent, corners"),
    },
}


# Cross-kit extras merged into a faction's prop_catalog: {faction: {kit: [stems]}}.
# Each entry probes MODELS_DIR/<kit>/<stem>.glb and carries prop_kit=<kit> so
# placement emits the right kit path (skip/rocks live in the space-station kit).
EXTRA_PROP_STEMS: Dict[str, Dict[str, list]] = {
    "industrial": {
        "space_station": ["skip", "skip-rocks", "rocks"],
    },
}
EXTRA_KIT_BASE_SCALE = {
    "space_station": 4.0,  # 1-unit kit
}


def probe_prop_kits():
    """Probe prop-kit folders and save prop_catalog.json for factions that use them."""
    for fac, kit in PROP_KIT_MAP.items():
        fac_dir = FACTIONS_DIR / fac
        if not fac_dir.exists():
            continue
        kit_dir = MODELS_DIR / kit
        if not kit_dir.exists():
            print(f"  Prop kit dir not found: {kit_dir}")
            continue
        allow = INTERIOR_PROP_ALLOW.get(kit, set())
        overrides = PROP_PURPOSE_OVERRIDES.get(kit, {})
        base_scale = KIT_BASE_SCALE.get(kit, 1.0)
        stems: Dict[str, dict] = {}
        for glb in sorted(kit_dir.glob("*.glb")):
            stem = glb.stem
            if stem not in allow:
                continue
            try:
                entry = probe_stem(glb, stem, base_scale=base_scale)
                # Apply purpose overrides for kit-specific items
                if stem in overrides:
                    entry["purpose"], entry["usage"] = overrides[stem]
                # Tag with source kit so placement knows where to find the GLB
                entry["prop_kit"] = kit
                entry["tags"] = list(set(entry.get("tags", [])) | {"prop", "interior_dressing"})
                stems[stem] = entry
            except Exception as e:
                print(f"  Error probing {kit}/{stem}: {e}")
        for extra_kit, extra_stems in EXTRA_PROP_STEMS.get(fac, {}).items():
            extra_base = EXTRA_KIT_BASE_SCALE.get(extra_kit, 1.0)
            for stem in extra_stems:
                glb = MODELS_DIR / extra_kit / f"{stem}.glb"
                if not glb.exists():
                    print(f"  Extra prop missing: {glb}")
                    continue
                try:
                    entry = probe_stem(glb, stem, base_scale=extra_base)
                    entry["prop_kit"] = extra_kit
                    entry["tags"] = list(set(entry.get("tags", [])) | {"prop", "interior_dressing"})
                    stems[stem] = entry
                except Exception as e:
                    print(f"  Error probing {extra_kit}/{stem}: {e}")
        catalog = {
            "version": 2,
            "generated_by": "tools/probe_faction_catalog.py (prop_kit)",
            "prop_kit": kit,
            # Base placement scale for this kit (see KIT_BASE_SCALE — kit-unit
            # calibration). Per-stem "auto_scale" is this clamped by room
            # height / cell footprint; placement code should prefer auto_scale.
            "prop_scale": base_scale,
            "cell_m": 4.0,
            "stems": stems,
        }
        out_path = fac_dir / "prop_catalog.json"
        out_path.write_text(json.dumps(catalog, indent=2))
        print(f"Wrote prop_catalog for {fac} ({kit}) with {len(stems)} items to {out_path}")


def wall_inner_offset_m(fac_dir: Path) -> float:
    """How far the faction wall's inner surface protrudes INTO the room past
    the cell-face line (0 = wall plane sits on the face).

    gen_freeform.add_wall anchors the wall origin `inset` outside the face with
    local -Z pointing into the room, so a slab spanning [z0, z1] model units
    occupies [-inset - z1*s, -inset - z0*s] measured into the room. Verified
    against necropolis brick-wall2 (inset 1.4, z0=-0.5, x4 -> +0.6 m)."""
    try:
        man = json.loads((fac_dir / "faction.json").read_text(encoding="utf-8"))
    except Exception:
        return 0.0
    role = (man.get("roles") or {}).get("wall") or {}
    stem = role.get("stem")
    glb = fac_dir / f"{stem}.glb" if stem else None
    if not glb or not glb.exists():
        return 0.0
    inset = float(role.get("inset") or 0.0)
    scale = float(role.get("scale") or man.get("scale") or 1.0)
    verts = load_glb_verts(glb)
    if not verts:
        return 0.0
    # Only full-width PANEL planes count as the wall surface — narrow deep
    # geometry (priesthood buttresses reach 2 units into the room) must not
    # push props off the wall. A z-slab qualifies when its vertices cover most
    # of the wall's x extent.
    bb = compute_bounds(verts)
    width = bb["x1"] - bb["x0"]
    if width < 1e-6:
        return 0.0
    slabs: Dict[int, set] = {}
    for (x, _, z) in verts:
        xbin = min(9, int((x - bb["x0"]) / width * 10))
        slabs.setdefault(int(round(z / 0.1)), set()).add(xbin)
    panel_z0 = max(0.0, -min((zb * 0.1 for zb, xb in slabs.items() if len(xb) >= 7),
                             default=0.0))
    return round(max(0.0, panel_z0 * scale - inset), 3)


def main():
    for fac in FACTIONS:
        fac_dir = FACTIONS_DIR / fac
        if not fac_dir.exists():
            continue
        stems = {}
        glbs = list(fac_dir.glob("*.glb"))
        base_scale = FACTION_BASE_SCALE.get(fac, DEFAULT_SCALE)
        for glb in glbs:
            stem = glb.stem
            try:
                stems[stem] = probe_stem(glb, stem, base_scale=base_scale)
            except Exception as e:
                print(f"Error probing {fac}/{stem}: {e}")
                continue
        catalog = {
            "version": 2,
            "generated_by": "tools/probe_faction_catalog.py",
            # Base PROP placement scale (kit-unit calibration; per-stem
            # "auto_scale" adds height/footprint clamps). Architecture pieces
            # (walls/floors/corridors) keep their faction-manifest scale.
            "default_scale": base_scale,
            "cell_m": 4.0,
            # Wall inner-surface protrusion past the cell face (placement code
            # shifts wall-snapped props into the room by this much).
            "wall_inner_offset_m": wall_inner_offset_m(fac_dir),
            "stems": stems
        }
        if fac == "synth":
            # preserve legacy keys used by synth_interior.py so it doesn't KeyError on load
            catalog["wall_face_offset_m"] = 2.0
            catalog["wall_half_thickness_m"] = 0.6
            catalog["deck_y"] = 1.2
        out_path = fac_dir / "placement_catalog.json"
        out_path.write_text(json.dumps(catalog, indent=2))
        print(f"Wrote catalog for {fac} with {len(stems)} items to {out_path}")

    print("\nProbing prop kits...")
    probe_prop_kits()

    print("\nCataloguing complete.")
    print("- placement_catalog.json: architecture + faction-folder props")
    print("- prop_catalog.json: curated interior dressing from prop kit (factory/, retro_fantasy/)")
    print("LLM doublecheck: review prop_catalog for missing/wrong purposes.")


if __name__ == "__main__":
    main()