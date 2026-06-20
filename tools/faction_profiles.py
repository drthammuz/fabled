#!/usr/bin/env python3
"""Faction procgen profiles (manifest §5.1 / Phase 2).

Each profile names a modular architecture kit, prop set, generation knobs, and
the hidden-room door piece (stem + kit + optional tint).  Profiles load from
``userinput/factions/*.json``; built-in defaults are merged when a file is absent.

Usage:
    python tools/faction_profiles.py list
    python tools/faction_profiles.py show priesthood
"""
from __future__ import annotations

import argparse
import json
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Tuple

ROOT = Path(__file__).resolve().parent.parent
FACTION_DIR = ROOT / "userinput" / "factions"


@dataclass
class HiddenDoorSpec:
    """Animated wall door sealing a single-entrance hidden room."""

    stem: str = "gate-door"
    kit: str = "space"  # assets/models/<kit>/
    tint: Optional[Tuple[float, float, float]] = (0.45, 0.82, 1.0)
    tag: str = "hidden_entrance"

    def to_piece_extras(self) -> dict:
        out: dict = {"kit": self.kit, "tags": [self.tag]}
        if self.tint:
            out["tint"] = list(self.tint)
        return out


@dataclass
class FactionProcgenProfile:
    """Quantifiable procgen knobs for one faction building tradition."""

    id: str
    label: str
    building_system: str  # modular kit folder (space, dungeon, …)
    prop_set: str = "factory"
    organicness: float = 0.0
    corridor_width: float = 1.0
    hidden_area_prevalence: float = 0.0
    room_min: int = 3
    room_max: int = 7
    max_rooms: int = 11
    loops: int = 3
    notes: str = ""
    hidden_door: HiddenDoorSpec = field(default_factory=HiddenDoorSpec)

    def apply_generation_defaults(self, kwargs: dict) -> dict:
        """Fill missing gen_freeform kwargs from profile defaults."""
        out = dict(kwargs)
        for key, attr in (
            ("organicness", "organicness"),
            ("corridor_width", "corridor_width"),
            ("hidden_area_prevalence", "hidden_area_prevalence"),
            ("room_min", "room_min"),
            ("room_max", "room_max"),
            ("max_rooms", "max_rooms"),
            ("loops", "loops"),
        ):
            if out.get(key) is None:
                out[key] = getattr(self, attr)
        return out


_BUILTIN: Dict[str, FactionProcgenProfile] = {
    "space_default": FactionProcgenProfile(
        id="space_default",
        label="Kenney space (default)",
        building_system="space",
        prop_set="factory",
        organicness=0.0,
        hidden_door=HiddenDoorSpec(
            stem="gate-door", kit="space", tint=(0.45, 0.82, 1.0),
        ),
    ),
    "priesthood": FactionProcgenProfile(
        id="priesthood",
        label="Priesthood (dungeon stone)",
        building_system="dungeon",
        prop_set="retro_fantasy",
        organicness=0.7,
        room_min=4,
        room_max=8,
        hidden_door=HiddenDoorSpec(
            stem="gate-door", kit="dungeon", tint=(0.95, 0.72, 0.35),
        ),
    ),
    "synth": FactionProcgenProfile(
        id="synth",
        label="Synth (space station)",
        building_system="space_station",
        prop_set="furniture",
        organicness=0.4,
        corridor_width=1.2,
        hidden_door=HiddenDoorSpec(
            # station kit has no gate-door; reuse space gate until station doors are wired
            stem="gate-door", kit="space", tint=(0.55, 0.95, 0.88),
        ),
    ),
    "outlaw": FactionProcgenProfile(
        id="outlaw",
        label="Outlaw (urban brick)",
        building_system="building",
        prop_set="factory",
        organicness=0.6,
        hidden_door=HiddenDoorSpec(
            stem="gate-door", kit="space", tint=(0.92, 0.55, 0.42),
        ),
    ),
    "industrial_default": FactionProcgenProfile(
        id="industrial_default",
        label="Industrial substrate",
        building_system="space",
        prop_set="factory",
        organicness=0.2,
        room_min=2,
        room_max=5,
        hidden_door=HiddenDoorSpec(
            stem="gate-door", kit="space", tint=(0.65, 0.70, 0.75),
        ),
    ),
}


def _profile_from_dict(data: dict) -> FactionProcgenProfile:
    hd_raw = data.pop("hidden_door", {}) or {}
    tint = hd_raw.get("tint")
    hidden_door = HiddenDoorSpec(
        stem=hd_raw.get("stem", "gate-door"),
        kit=hd_raw.get("kit", "space"),
        tint=tuple(tint) if tint else None,
        tag=hd_raw.get("tag", "hidden_entrance"),
    )
    pid = data["id"]
    return FactionProcgenProfile(hidden_door=hidden_door, **data)


def load_profile(profile_id: str) -> FactionProcgenProfile:
    path = FACTION_DIR / f"{profile_id}.json"
    if path.is_file():
        data = json.loads(path.read_text(encoding="utf-8"))
        return _profile_from_dict(data)
    if profile_id in _BUILTIN:
        return _BUILTIN[profile_id]
    raise KeyError(f"unknown faction profile: {profile_id}")


def list_profiles() -> List[str]:
    ids = set(_BUILTIN)
    if FACTION_DIR.is_dir():
        ids.update(p.stem for p in FACTION_DIR.glob("*.json"))
    return sorted(ids)


def export_builtin_profiles() -> None:
    """Write built-in JSON presets (idempotent — skips existing files)."""
    FACTION_DIR.mkdir(parents=True, exist_ok=True)
    for pid, prof in _BUILTIN.items():
        path = FACTION_DIR / f"{pid}.json"
        if path.exists():
            continue
        data = asdict(prof)
        path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    sub = ap.add_subparsers(dest="cmd")
    sub.add_parser("list", help="List profile ids")
    sub.add_parser("export", help="Write built-in JSON to userinput/factions/")
    show = sub.add_parser("show", help="Print one profile")
    show.add_argument("id")
    args = ap.parse_args()

    if args.cmd == "export":
        export_builtin_profiles()
        print(f"exported to {FACTION_DIR}")
    elif args.cmd == "show":
        prof = load_profile(args.id)
        print(json.dumps(asdict(prof), indent=2))
    else:
        for pid in list_profiles():
            prof = load_profile(pid)
            hd = prof.hidden_door
            print(f"{pid:20} kit={prof.building_system:14} door={hd.stem}@{hd.kit} tint={hd.tint}")


if __name__ == "__main__":
    main()
