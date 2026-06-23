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

import mesh_metrics

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
class EntrancePieceSpec:
    """Door or stair GLB placed at an industrial ↔ faction boundary."""

    stem: str
    kit: str
    scale: float = 1.0
    yaw_offset: float = 0.0


def stair_ramp_footprint_m(stem: str, scale: float, kit: str = "space_station") -> Tuple[float, float]:
    """World metres (high-end extent, low-end extent) along the ramp axis."""
    return mesh_metrics.stair_ramp_footprint_m(stem, scale, kit)


def stair_top_height_m(stem: str, scale: float, kit: str = "space_station") -> float:
    return mesh_metrics.stair_top_y_m(stem, scale, kit)


@dataclass
class TransitionSpec:
    """How this faction meets the industrial substrate (manifest §2.3)."""

    mode: str = "direct_on_substrate"  # outdoor_then_indoor | direct_on_substrate
    substrate_overlap_cells: int = 0
    # Top of stairs-small-center @ scale 4 (space_station bbox ymax 0.3 × 4).
    elevation_rise: float = 1.2
    entrance_door: Optional[EntrancePieceSpec] = None
    entrance_stairs: Optional[EntrancePieceSpec] = None


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
    transition: TransitionSpec = field(default_factory=TransitionSpec)

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
        transition=TransitionSpec(
            mode="outdoor_then_indoor",
            entrance_door=EntrancePieceSpec(stem="gate-door", kit="dungeon"),
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
            stem="gate-door", kit="space", tint=(0.55, 0.95, 0.88),
        ),
        transition=TransitionSpec(
            mode="outdoor_then_indoor",
            elevation_rise=1.2,
            entrance_door=EntrancePieceSpec(
                stem="wall-door", kit="space_station", scale=4.0,
            ),
            entrance_stairs=EntrancePieceSpec(
                stem="stairs-small-center", kit="space_station", scale=4.0,
            ),
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
        transition=TransitionSpec(
            mode="direct_on_substrate",
            substrate_overlap_cells=3,
            entrance_door=EntrancePieceSpec(stem="gate-door", kit="space"),
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


def _piece_spec(raw: Optional[dict]) -> Optional[EntrancePieceSpec]:
    if not raw:
        return None
    return EntrancePieceSpec(
        stem=str(raw["stem"]),
        kit=str(raw.get("kit", "space")),
        scale=float(raw.get("scale", 1.0)),
        yaw_offset=float(raw.get("yaw_offset", 0.0)),
    )


def _transition_from_dict(raw: Optional[dict], profile_id: str) -> TransitionSpec:
    if not raw:
        if profile_id in _BUILTIN:
            return _BUILTIN[profile_id].transition
        return TransitionSpec()
    return TransitionSpec(
        mode=str(raw.get("mode", "direct_on_substrate")),
        substrate_overlap_cells=int(raw.get("substrate_overlap_cells", 0)),
        elevation_rise=float(raw.get("elevation_rise", 1.2)),
        entrance_door=_piece_spec(raw.get("entrance_door")),
        entrance_stairs=_piece_spec(raw.get("entrance_stairs")),
    )


def _profile_from_dict(data: dict) -> FactionProcgenProfile:
    hd_raw = data.pop("hidden_door", {}) or {}
    tr_raw = data.pop("transition", None)
    tint = hd_raw.get("tint")
    hidden_door = HiddenDoorSpec(
        stem=hd_raw.get("stem", "gate-door"),
        kit=hd_raw.get("kit", "space"),
        tint=tuple(tint) if tint else None,
        tag=hd_raw.get("tag", "hidden_entrance"),
    )
    pid = data["id"]
    transition = _transition_from_dict(tr_raw, pid)
    return FactionProcgenProfile(
        hidden_door=hidden_door, transition=transition, **data,
    )


MODULAR_GRAMMAR_KITS = frozenset({"space", "dungeon"})


def architecture_kit(profile: FactionProcgenProfile) -> Optional[str]:
    """Kit folder for modular tile emission; None = default ``space/`` (omit JSON field)."""
    bs = profile.building_system
    if bs == "space":
        return None
    if bs in MODULAR_GRAMMAR_KITS:
        return bs
    return None


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
