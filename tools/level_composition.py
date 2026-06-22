#!/usr/bin/env python3
"""Level composition — manifest §5.2 / Phase 4 zone planner inputs.

Maps spawn→extract into three zones (previous camp faction / industrial default /
next camp faction) with tunable fractions along the main-path spine.
"""
from __future__ import annotations

from collections import deque
from dataclasses import dataclass, field
from typing import Callable, Dict, List, Literal, Optional, Set, Tuple

import faction_profiles as fp

Cell = Tuple[int, int]
Zone = Literal["prev", "default", "next"]


@dataclass
class LevelComposition:
    mix_mode: str = "single"  # single | transition
    prev_faction: str = "priesthood"
    next_faction: str = "industrial_default"
    default_faction: str = "industrial_default"
    prev_fraction: float = 0.25
    default_fraction: float = 0.50
    next_fraction: float = 0.25

    def normalized(self) -> "LevelComposition":
        """Return a copy with fractions summing to 1."""
        if self.mix_mode != "transition":
            return LevelComposition(
                mix_mode=self.mix_mode,
                prev_faction=self.prev_faction,
                next_faction=self.next_faction,
                default_faction=self.default_faction,
                prev_fraction=0.0,
                default_fraction=0.0,
                next_fraction=1.0,
            )
        p, d, n = self.prev_fraction, self.default_fraction, self.next_fraction
        s = p + d + n
        if s <= 1e-6:
            p, d, n = 0.25, 0.50, 0.25
            s = 1.0
        p, d, n = p / s, d / s, n / s
        return LevelComposition(
            mix_mode="transition",
            prev_faction=self.prev_faction,
            next_faction=self.next_faction,
            default_faction=self.default_faction,
            prev_fraction=p,
            default_fraction=d,
            next_fraction=n,
        )

    def to_doc(self) -> dict:
        c = self.normalized()
        return {
            "mix_mode": c.mix_mode,
            "prev_faction": c.prev_faction,
            "next_faction": c.next_faction,
            "default_faction": c.default_faction,
            "prev_fraction": round(c.prev_fraction, 3),
            "default_fraction": round(c.default_fraction, 3),
            "next_fraction": round(c.next_fraction, 3),
        }

    @classmethod
    def from_doc(cls, data: Optional[dict]) -> "LevelComposition":
        if not data:
            return cls()
        return cls(
            mix_mode=str(data.get("mix_mode", "single")),
            prev_faction=str(data.get("prev_faction", "priesthood")),
            next_faction=str(data.get("next_faction", "industrial_default")),
            default_faction=str(data.get("default_faction", "industrial_default")),
            prev_fraction=float(data.get("prev_fraction", 0.25)),
            default_fraction=float(data.get("default_fraction", 0.50)),
            next_fraction=float(data.get("next_fraction", 0.25)),
        ).normalized()


def _neighbors(cell: Cell, walkable: Set[Cell]) -> List[Cell]:
    x, z = cell
    out: List[Cell] = []
    for dx, dz in ((0, -1), (0, 1), (1, 0), (-1, 0)):
        c = (x + dx, z + dz)
        if c in walkable:
            out.append(c)
    return out


def compute_spine_path(
    walkable: Set[Cell],
    start: Cell,
    goal: Cell,
) -> List[Cell]:
    """Shortest path on the walkable grid (BFS). Falls back to [start] if blocked."""
    if start == goal:
        return [start]
    if start not in walkable or goal not in walkable:
        return [start]

    parent: Dict[Cell, Optional[Cell]] = {start: None}
    q: deque[Cell] = deque([start])
    while q:
        cur = q.popleft()
        if cur == goal:
            break
        for nxt in _neighbors(cur, walkable):
            if nxt in parent:
                continue
            parent[nxt] = cur
            q.append(nxt)

    if goal not in parent:
        return [start]

    path: List[Cell] = []
    c: Optional[Cell] = goal
    while c is not None:
        path.append(c)
        c = parent[c]
    path.reverse()
    return path


def _nearest_spine_index(cell: Cell, spine: List[Cell]) -> int:
    if not spine:
        return 0
    best_i, best_d = 0, 10**9
    cx, cz = cell
    for i, (sx, sz) in enumerate(spine):
        d = abs(cx - sx) + abs(cz - sz)
        if d < best_d:
            best_d, best_i = d, i
    return best_i


def zone_at_spine_t(t: float, comp: LevelComposition) -> Zone:
    c = comp.normalized()
    if t < c.prev_fraction:
        return "prev"
    if t < c.prev_fraction + c.default_fraction:
        return "default"
    return "next"


def zone_for_cell(cell: Cell, spine: List[Cell], comp: LevelComposition) -> Zone:
    if not spine:
        return "default"
    idx = _nearest_spine_index(cell, spine)
    t = idx / max(1, len(spine) - 1)
    return zone_at_spine_t(t, comp)


def _kit_for_zone(zone: Zone, comp: LevelComposition) -> Optional[str]:
    if zone == "prev":
        prof = fp.load_profile(comp.prev_faction)
    elif zone == "next":
        prof = fp.load_profile(comp.next_faction)
    else:
        prof = fp.load_profile(comp.default_faction)
    return fp.architecture_kit(prof)


def hidden_door_profile(comp: LevelComposition, zone: Zone) -> fp.FactionProcgenProfile:
    if zone == "prev":
        return fp.load_profile(comp.prev_faction)
    if zone == "next":
        return fp.load_profile(comp.next_faction)
    return fp.load_profile(comp.default_faction)


def make_kit_lookup(
    walkable: Set[Cell],
    spine: List[Cell],
    comp: LevelComposition,
    single_profile_id: str,
) -> Callable[[Cell], Optional[str]]:
    """Return kit folder for a floor-0 cell (None = default space path)."""
    if comp.mix_mode != "transition":
        kit = fp.architecture_kit(fp.load_profile(single_profile_id))
        return lambda _c: kit

    c = comp.normalized()

    def lookup(cell: Cell) -> Optional[str]:
        if cell not in walkable:
            return fp.architecture_kit(fp.load_profile(c.default_faction))
        z = zone_for_cell(cell, spine, c)
        return _kit_for_zone(z, c)

    return lookup


def make_zone_lookup(
    walkable: Set[Cell],
    spine: List[Cell],
    comp: LevelComposition,
) -> Callable[[Cell], Optional[str]]:
    """Return composition zone id per cell (``prev`` / ``default`` / ``next``)."""
    if comp.mix_mode != "transition":
        return lambda _c: None

    c = comp.normalized()

    def lookup(cell: Cell) -> Optional[str]:
        if cell not in walkable:
            return "default"
        return zone_for_cell(cell, spine, c)

    return lookup


def plan_zones_for_map(fm) -> Tuple[
    List[Cell],
    LevelComposition,
    Callable[[Cell], Optional[str]],
    Callable[[Cell], Optional[str]],
]:
    """Build spine + kit/zone lookups for a generated ``FreeformMap``."""
    comp = getattr(fm, "composition", LevelComposition(mix_mode="single"))
    start = (fm.rooms[fm.spawn_room].cx, fm.rooms[fm.spawn_room].cz)
    goal = fm.hub.trap0 if fm.hub else (fm.rooms[fm.end_room].cx, fm.rooms[fm.end_room].cz)
    spine = compute_spine_path(fm.walkable, start, goal)
    lookup = make_kit_lookup(fm.walkable, spine, comp, fm.faction_profile_id)
    zone_lookup = make_zone_lookup(fm.walkable, spine, comp)
    return spine, comp, lookup, zone_lookup
