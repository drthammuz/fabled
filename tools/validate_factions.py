#!/usr/bin/env python3
"""Validate faction asset manifests against the files + profiles on disk.

Checks each assets/models/factions/<id>/faction.json:
  * id matches its folder name
  * colormaps + material albedo/emissive/mr files exist
  * every role's GLB stem exists in the folder
  * provides ⊆ roles, and provided roles are generator roles (floor/wall/corridor)
  * material.client_slot is a known KenneyMaterialSlot
  * profiles reference real userinput/factions/*.json profile ids

Exit 0 = all PASS. Run after adding/editing a faction folder.
"""
from __future__ import annotations

import sys
from pathlib import Path

import faction_assets as fa

ROOT = fa.ROOT
PROFILE_DIR = ROOT / "userinput" / "factions"

# Mirror of the client KenneyMaterialSlot enum (test_showcase.rs). Keep in sync.
KNOWN_SLOTS = {
    "NativeGlb", "Priesthood", "Synth", "SpaceDefault", "SpaceCyber",
    "SpaceIndustrial", "Ceiling", "CeilingPink", "Lasers",
}


def _exists(folder: Path, rel: str) -> bool:
    if not rel:
        return True
    # Absolute-ish "models/..." paths resolve from assets/; folder-relative else.
    if rel.startswith("models/"):
        return (ROOT / "assets" / rel).is_file()
    return (folder / rel).is_file()


def validate() -> int:
    assets = fa.load_all()
    errors = []
    known_profiles = {p.stem for p in PROFILE_DIR.glob("*.json")} if PROFILE_DIR.is_dir() else set()

    for fid, a in assets.items():
        folder = fa.FACTIONS_DIR / fid
        prefix = f"[{fid}]"

        for key, rel in a.colormaps.items():
            if not _exists(folder, rel):
                errors.append(f"{prefix} colormap '{key}' missing: {rel}")
        for key in ("albedo", "emissive_map", "mr_map"):
            rel = a.material.get(key)
            if rel and not _exists(folder, rel):
                errors.append(f"{prefix} material.{key} missing: {rel}")

        slot = a.material.get("client_slot")
        if slot not in KNOWN_SLOTS:
            errors.append(f"{prefix} unknown material.client_slot: {slot!r}")

        for role in a.provides:
            if role not in fa.ROLES:
                errors.append(f"{prefix} provides unknown role: {role!r}")
            if role not in a.roles:
                errors.append(f"{prefix} provides '{role}' but no roles[{role}] entry")

        for role, spec in a.roles.items():
            stems = [spec[k] for k in ("stem", "corner_stem") if k in spec]
            for stem in stems:
                if not (folder / f"{stem}.glb").is_file():
                    errors.append(f"{prefix} role '{role}' stem missing: {stem}.glb")

        for pid in a.profiles:
            if known_profiles and pid not in known_profiles:
                errors.append(f"{prefix} references unknown profile id: {pid}")

    if errors:
        print(f"FAIL — {len(errors)} issue(s):")
        for e in errors:
            print("  " + e)
        return 1
    print(f"PASS — {len(assets)} faction manifest(s) valid: {sorted(assets)}")
    return 0


if __name__ == "__main__":
    sys.exit(validate())
