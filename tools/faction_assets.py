#!/usr/bin/env python3
"""Faction asset manifests — the source-of-truth catalogue for faction GLBs.

Each faction folder ``assets/models/factions/<id>/`` is self-contained:
GLB copies + ``Textures/`` colormap(s) + ``faction.json`` (this manifest) +
``catalogue.md`` (human notes). Moving/duplicating a faction = copy the folder;
moving an item between factions = read its role entry here (stem, facing, flags)
and recreate it under the new faction.

``faction.json`` schema (version 1)::

    {
      "schema": 1,
      "id": "priesthood",                 # == folder name
      "label": "...",
      "profiles": ["priesthood"],         # faction-profile ids this asset serves
      "source_kit": "dungeon",            # Kenney kit the GLBs were copied from
      "grammar": "modular",               # placement grammar (generator family)
      "colormaps": {"primary": "Textures/colormap.png"},
      "material": {                        # how the CLIENT editor renders it
        "mode": "explicit" | "native_glb",
        "client_slot": "Priesthood",      # KenneyMaterialSlot name (cross-ref)
        "albedo": "Textures/colormap.png",
        "base_color": [1,1,1], "metallic": 0.0, "roughness": 1.0,
        "double_sided": true,
        "emissive_map": null, "emissive": [0,0,0],
        "mr_map": null, "uv_transform": null
      },
      "roles": {                           # per-role usage metadata
        "floor": {"stem": "template-floor", "yaw_offset": 0.0, "collide": true},
        "wall":  {"stem": "template-wall", "yaw_offset": 0.0, "collide": true,
                  "placement": "edge", "faces_at_yaw0": "+Z"},
        "corridor": {"corner_stem": "corridor-corner", "yaw_offset": 0.0,
                     "floorless_corner": true,
                     "built_from": ["floor", "wall"]}
      },
      "provides": ["floor", "wall", "corridor"],  # roles routed to THIS folder;
                                                   # others fall back to space grammar
      "notes": "..."
    }

A faction profile with NO manifest here falls back to its
``building_system`` kit for all roles (legacy whole-kit behaviour).
"""
from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Set

ROOT = Path(__file__).resolve().parent.parent
FACTIONS_DIR = ROOT / "assets" / "models" / "factions"

# Generator piece roles (what `gen_freeform._piece_role` returns).
ROLES = ("floor", "wall", "corridor")


# Stems the generator emits per role when a faction does not override them
# (the space/dungeon modular grammar). Faction manifests override via roles.<role>.
DEFAULT_STEMS = {"floor": "template-floor", "wall": "template-wall", "corner": "corridor-corner"}


@dataclass
class FactionAsset:
    id: str
    label: str
    profiles: List[str]
    source_kit: str
    grammar: str
    colormaps: Dict[str, str]
    material: Dict
    roles: Dict[str, Dict]
    provides: List[str]
    scale: float = 1.0
    notes: str = ""

    @property
    def kit(self) -> str:
        """Kit string the generator/client use for this faction's pieces."""
        return f"factions/{self.id}"

    @property
    def floorless_corner(self) -> bool:
        return bool(self.roles.get("corridor", {}).get("floorless_corner", False))

    @property
    def walls_only_corner(self) -> bool:
        """L-bends use floor + two straight walls instead of a corner GLB."""
        return bool(self.roles.get("corridor", {}).get("walls_only_corner", False))

    def kit_for_role(self, role: str) -> Optional[str]:
        """Kit folder for a role, or None to fall back to the space grammar."""
        return self.kit if role in self.provides else None

    def stem(self, slot: str) -> Optional[str]:
        """GLB stem this faction uses for an emit slot (floor/wall/corner)."""
        if slot == "corner":
            return self.roles.get("corridor", {}).get("corner_stem")
        return self.roles.get(slot, {}).get("stem")


def load_all() -> Dict[str, FactionAsset]:
    """Load every ``faction.json`` under assets/models/factions/."""
    out: Dict[str, FactionAsset] = {}
    if not FACTIONS_DIR.is_dir():
        return out
    for manifest in sorted(FACTIONS_DIR.glob("*/faction.json")):
        data = json.loads(manifest.read_text(encoding="utf-8"))
        fa = FactionAsset(
            id=data["id"],
            label=data.get("label", data["id"]),
            profiles=list(data.get("profiles", [])),
            source_kit=data.get("source_kit", ""),
            grammar=data.get("grammar", "modular"),
            colormaps=data.get("colormaps", {}),
            material=data.get("material", {}),
            roles=data.get("roles", {}),
            provides=list(data.get("provides", [])),
            scale=float(data.get("scale", 1.0)),
            notes=data.get("notes", ""),
        )
        out[fa.id] = fa
    return out


def profile_to_asset(assets: Optional[Dict[str, FactionAsset]] = None) -> Dict[str, str]:
    """Map faction-profile id -> asset id, declared by each manifest's ``profiles``."""
    assets = assets if assets is not None else load_all()
    out: Dict[str, str] = {}
    for fa in assets.values():
        for pid in fa.profiles:
            out[pid] = fa.id
    return out


def asset_for_profile(
    profile_id: str, assets: Optional[Dict[str, FactionAsset]] = None
) -> Optional[FactionAsset]:
    assets = assets if assets is not None else load_all()
    aid = profile_to_asset(assets).get(profile_id)
    return assets.get(aid) if aid else None


def floorless_corner_kits(assets: Optional[Dict[str, FactionAsset]] = None) -> Set[str]:
    assets = assets if assets is not None else load_all()
    return {fa.kit for fa in assets.values() if fa.floorless_corner}


def walls_only_corner_kits(assets: Optional[Dict[str, FactionAsset]] = None) -> Set[str]:
    assets = assets if assets is not None else load_all()
    return {fa.kit for fa in assets.values() if fa.walls_only_corner}


if __name__ == "__main__":
    import sys

    assets = load_all()
    if len(sys.argv) > 1 and sys.argv[1] == "show" and len(sys.argv) > 2:
        fa = assets[sys.argv[2]]
        print(json.dumps(fa.__dict__, indent=2, default=str))
    else:
        for fa in assets.values():
            print(f"{fa.id:16} profiles={fa.profiles} provides={fa.provides} "
                  f"material={fa.material.get('mode')} floorless={fa.floorless_corner}")
