#!/usr/bin/env python3
"""Synth elevated transition assembly — multi-piece stairs, foyer, walls.

Spec: docs/synth-transition-architecture.md
Deck support: tools/synth_deck.py (``floor`` @ y=0 — not ``structure-panel``).
"""
from __future__ import annotations

import math
import random
from dataclasses import dataclass, field
from typing import Callable, Dict, List, Literal, Optional, Sequence, Set, Tuple

import faction_profiles as fp
import level_composition as lc
import transition_entrances as te

import synth_deck as sd

Cell = Tuple[int, int]
Zone = te.Zone

CELL = te.CELL
DELTA = te.DELTA
OPPOSITE = te.OPPOSITE

KIT = "factions/synth"
SCALE = 4.0
FLIGHT_RISE = 1.2  # metres per flight @ scale 4
SECOND_DECK_Y = FLIGHT_RISE * 3  # 3.6 m — three flights to “second floor”

# Small stair family (1 cell wide @ scale 4).  Left/right are distinct GLBs (``-r`` = mirrored X).
SMALL_SOLO = "stairs-small-edges"
SMALL_LEFT = "stairs-small-edge"
SMALL_RIGHT = "stairs-small-edge-r"
SMALL_MID = "stairs-small-center"
SMALL_CORNER = "stairs-small-corner"
SMALL_CORNER_R = "stairs-small-corner-r"
SMALL_CORNER_INNER = "stairs-small-corner-inner"
SMALL_CORNER_INNER_R = "stairs-small-corner-inner-r"

WALL_INDUSTRIAL = "template-wall"
WALL_SYNTH = "wall"
FOYER_PROP = "table-display-planet"


@dataclass
class SeamStrip:
    """Substrate cells along the industrial side of a zone seam."""

    toward_faction: str
    lateral_axis: str  # "x" | "z"
    substrate_cells: List[Cell]  # sorted along lateral axis
    faction_cells: List[Cell]  # aligned faction neighbours per substrate cell

    @property
    def width(self) -> int:
        return len(self.substrate_cells)


@dataclass
class TransitionPlan:
    """Resolved layout for one elevated boundary."""

    boundary: te.ZoneBoundary
    zone: str
    ascending: bool
    strip: SeamStrip
    stair_stems: List[Optional[str]]  # per lateral slot; None = empty tile
    flights_per_column: List[int]  # 1 or 3 flights per lateral slot with stairs
    target_deck_y: float
    door_cell: Cell
    deck_cells: Set[Cell] = field(default_factory=set)
    foyer_cells: List[Cell] = field(default_factory=list)
    multi_floor: bool = False


def _lateral_key(cell: Cell, axis: str) -> int:
    return cell[0] if axis == "x" else cell[1]


def _perp_delta(toward: str) -> Tuple[int, int]:
    tdx, tdz = DELTA[toward]
    return -tdz, tdx


def _zone_of(
    cell: Cell,
    walkable: Set[Cell],
    zone_lookup: Callable[[Cell], Optional[str]],
) -> Optional[str]:
    if cell not in walkable:
        return None
    return zone_lookup(cell)


def _contiguous_lateral_run(
    cells: Set[Cell],
    anchor: Cell,
    lateral_axis: str,
) -> List[Cell]:
    """Keep one contiguous seam segment through ``anchor`` (drops diagonal/gap artifacts)."""
    if not cells:
        return []
    pdx, pdz = (1, 0) if lateral_axis == "x" else (0, 1)

    def step(cur: Cell, sign: int) -> Optional[Cell]:
        nxt = (cur[0] + sign * pdx, cur[1] + sign * pdz)
        return nxt if nxt in cells else None

    run: List[Cell] = [anchor] if anchor in cells else []
    if not run:
        return sorted(cells, key=lambda c: _lateral_key(c, lateral_axis))
    cur = anchor
    while True:
        nxt = step(cur, 1)
        if nxt is None:
            break
        run.append(nxt)
        cur = nxt
    cur = anchor
    while True:
        nxt = step(cur, -1)
        if nxt is None:
            break
        run.insert(0, nxt)
        cur = nxt
    return run


def _faction_neighbor(
    substrate: Cell,
    toward_faction: str,
    walkable: Set[Cell],
    zone_lookup: Callable[[Cell], Optional[str]],
    faction_zone: str,
) -> Optional[Cell]:
    tdx, tdz = DELTA[toward_faction]
    fc = (substrate[0] + tdx, substrate[1] + tdz)
    if _zone_of(fc, walkable, zone_lookup) == faction_zone:
        return fc
    return None


def _faction_seam_row(
    anchor_fc: Cell,
    toward_faction: str,
    faction_zone: str,
    walkable: Set[Cell],
    zone_lookup: Callable[[Cell], Optional[str]],
) -> List[Cell]:
    """Contiguous faction cells on the seam row (each faces default across ``toward_faction``)."""
    tdx, tdz = DELTA[toward_faction]
    pdx, pdz = _perp_delta(toward_faction)
    lateral_axis = "x" if pdx != 0 else "z"

    def is_seam_fc(fc: Cell) -> bool:
        if _zone_of(fc, walkable, zone_lookup) != faction_zone:
            return False
        sc = (fc[0] - tdx, fc[1] - tdz)
        return (
            sc in walkable
            and _zone_of(sc, walkable, zone_lookup) == "default"
        )

    if not is_seam_fc(anchor_fc):
        return []
    row: List[Cell] = [anchor_fc]
    for sign in (1, -1):
        cur = anchor_fc
        while True:
            nxt = (cur[0] + sign * pdx, cur[1] + sign * pdz)
            if not is_seam_fc(nxt):
                break
            row.append(nxt)
            cur = nxt
    return sorted(set(row), key=lambda c: _lateral_key(c, lateral_axis))


def scan_seam_strip(
    walkable: Set[Cell],
    zone_lookup: Callable[[Cell], Optional[str]],
    substrate_cell: Cell,
    toward_faction: str,
    faction_zone: str,
    *,
    faction_anchor: Optional[Cell] = None,
) -> SeamStrip:
    """Substrate + faction cells along the full contiguous seam row, sorted laterally.

    Width is driven by the **faction seam row** (one substrate column per seam
    faction cell).  Older anchor-only substrate walks could truncate the run so
    edge caps landed one column short of the last stair (seed 1: edge at z=38
    instead of z=42).
    """
    tdx, tdz = DELTA[toward_faction]
    pdx, pdz = _perp_delta(toward_faction)
    lateral_axis = "x" if pdx != 0 else "z"

    anchor_fc = faction_anchor or _faction_neighbor(
        substrate_cell, toward_faction, walkable, zone_lookup, faction_zone,
    )
    faction_row = (
        _faction_seam_row(anchor_fc, toward_faction, faction_zone, walkable, zone_lookup)
        if anchor_fc is not None
        else []
    )

    substrate_cells: List[Cell] = []
    faction_cells: List[Cell] = []
    for fc in faction_row:
        sc = (fc[0] - tdx, fc[1] - tdz)
        if sc in walkable and _zone_of(sc, walkable, zone_lookup) == "default":
            substrate_cells.append(sc)
            faction_cells.append(fc)

    if substrate_cells:
        return SeamStrip(toward_faction, lateral_axis, substrate_cells, faction_cells)

    # Fallback: substrate anchor only (degenerate seam).
    def on_seam(c: Cell) -> bool:
        if _zone_of(c, walkable, zone_lookup) != "default":
            return False
        return _faction_neighbor(c, toward_faction, walkable, zone_lookup, faction_zone) is not None

    substrate_set: Set[Cell] = set()
    if on_seam(substrate_cell):
        substrate_set.add(substrate_cell)
        for sign in (1, -1):
            cur = substrate_cell
            while True:
                nxt = (cur[0] + sign * pdx, cur[1] + sign * pdz)
                if not on_seam(nxt):
                    break
                substrate_set.add(nxt)
                cur = nxt

    if not substrate_set:
        return SeamStrip(toward_faction, lateral_axis, [substrate_cell], [])

    cells = _contiguous_lateral_run(substrate_set, substrate_cell, lateral_axis)
    if not cells:
        cells = sorted(substrate_set, key=lambda c: _lateral_key(c, lateral_axis))
    faction = [
        _faction_neighbor(c, toward_faction, walkable, zone_lookup, faction_zone)
        for c in cells
    ]
    return SeamStrip(toward_faction, lateral_axis, cells, faction)


def _depth_into_faction(
    fc: Cell,
    toward_faction: str,
    walkable: Set[Cell],
    zone_lookup: Callable[[Cell], Optional[str]],
    faction_zone: str,
) -> int:
    """Walkable faction cells from the seam row deeper into the zone (includes seam = 1)."""
    tdx, tdz = DELTA[toward_faction]
    depth = 0
    cur = fc
    while _zone_of(cur, walkable, zone_lookup) == faction_zone:
        depth += 1
        cur = (cur[0] + tdx, cur[1] + tdz)
        if cur not in walkable:
            break
    return depth


def _pick_stems_width1() -> List[Optional[str]]:
    return [SMALL_SOLO]


def _pick_stems_width2(rng: random.Random) -> List[Optional[str]]:
    return [None, SMALL_SOLO]


def _pick_stems_width3(rng: random.Random) -> List[Optional[str]]:
    if rng.random() < 0.55:
        return [SMALL_LEFT, SMALL_MID, SMALL_LEFT]
    return [SMALL_CORNER, SMALL_MID, SMALL_CORNER]


def _pick_stems_width4(rng: random.Random) -> List[Optional[str]]:
    r = rng.random()
    if r < 0.5:
        return [SMALL_LEFT, SMALL_MID, SMALL_MID, SMALL_LEFT]
    return [SMALL_CORNER, SMALL_MID, SMALL_MID, SMALL_CORNER]


def _pick_stems_width5(rng: random.Random) -> List[Optional[str]]:
    mode = rng.choice(("one", "three", "five"))
    if mode == "five":
        return [SMALL_LEFT, SMALL_MID, SMALL_MID, SMALL_MID, SMALL_LEFT]
    if mode == "three":
        if rng.random() < 0.5:
            return [None, SMALL_LEFT, SMALL_MID, SMALL_LEFT, None]
        return [None, SMALL_CORNER, SMALL_MID, SMALL_CORNER, None]
    # single centre — variants a–d
    v = rng.random()
    if v < 0.25:
        return [None, None, SMALL_SOLO, None, None]
    if v < 0.5:
        return [SMALL_CORNER, SMALL_CORNER_INNER, SMALL_SOLO, SMALL_CORNER_INNER, SMALL_CORNER]
    if v < 0.75:
        return [SMALL_LEFT, SMALL_CORNER_INNER, SMALL_SOLO, SMALL_CORNER_INNER, SMALL_LEFT]
    return [
        SMALL_CORNER, SMALL_CORNER_INNER, SMALL_SOLO,
        SMALL_CORNER_INNER, SMALL_CORNER,
    ]


def pick_lateral_stems(width: int, rng: random.Random) -> List[Optional[str]]:
    if width <= 1:
        return _pick_stems_width1()
    if width == 2:
        stems = _pick_stems_width2(rng)
    elif width == 3:
        stems = _pick_stems_width3(rng)
    elif width == 4:
        stems = _pick_stems_width4(rng)
    elif width == 5:
        stems = _pick_stems_width5(rng)
    else:
        # Wider than 5: centre a 5-wide pattern with empty flanks.
        core = _pick_stems_width5(rng)
        pad = (width - 5) // 2
        stems = [None] * pad + core + [None] * (width - 5 - pad)
    while len(stems) < width:
        stems = [None] + stems + [None]
        stems = stems[:width]
    return stems[:width]


# Mean local-x sign of each asymmetric stair GLB (the side the rail/closed face sits
# on).  Probed via tools/_probe_stairs.py — note the corner family's sign is OPPOSITE
# the edge family, so a static "plain = -x" assumption is wrong.  +1 = rail on +x.
_RAIL_LOCAL_SIGN = {
    SMALL_LEFT: -1, SMALL_RIGHT: +1,
    SMALL_CORNER: +1, SMALL_CORNER_R: -1,
    SMALL_CORNER_INNER: -1, SMALL_CORNER_INNER_R: +1,
}
# Per family: the two chiral variants keyed by their local-x rail sign.
_VARIANTS_BY_SIGN = {
    SMALL_LEFT: {-1: SMALL_LEFT, +1: SMALL_RIGHT},
    SMALL_RIGHT: {-1: SMALL_LEFT, +1: SMALL_RIGHT},
    # Corner family swapped vs the edge family: the corner GLB's tiny x-asymmetry does
    # NOT track its visible outer-corner direction, so the rail-sign rule put the outer
    # corners on the wrong side (user: "left/right corners have to be swapped").
    SMALL_CORNER: {-1: SMALL_CORNER, +1: SMALL_CORNER_R},
    SMALL_CORNER_R: {-1: SMALL_CORNER, +1: SMALL_CORNER_R},
    SMALL_CORNER_INNER: {-1: SMALL_CORNER_INNER, +1: SMALL_CORNER_INNER_R},
    SMALL_CORNER_INNER_R: {-1: SMALL_CORNER_INNER, +1: SMALL_CORNER_INNER_R},
}


def _active_span(stems: Sequence[Optional[str]]) -> Tuple[int, int]:
    idx = [i for i, s in enumerate(stems) if s]
    return (min(idx), max(idx)) if idx else (0, len(stems) - 1)


def _oriented_end_stem(
    stem: Optional[str],
    slot: int,
    stems: Sequence[Optional[str]],
    yaw: float,
    lateral_axis: str,
) -> Optional[str]:
    """Pick the chiral variant for an END stair so its rail faces OUTWARD in world space.

    Geometry, not enter/exit: the rail's world direction is set by the run's ``yaw``.
    For an axis-aligned yaw, ``lateral_comp = local_x_sign * factor`` with
    ``factor = cos(yaw)`` (x-run) or ``-sin(yaw)`` (z-run).  We want that component to
    point outward — negative at the low active end, positive at the high active end —
    so the required local-x sign is ``outward * factor``.  Interior pieces are left
    unchanged (only the two active ends carry the run-terminating rail).
    """
    if not stem or stem not in _RAIL_LOCAL_SIGN:
        return stem
    lo, hi = _active_span(stems)
    if slot == lo:
        outward = -1
    elif slot == hi:
        outward = +1
    else:
        return stem
    factor = math.cos(yaw) if lateral_axis == "x" else -math.sin(yaw)
    factor = 1 if factor >= 0 else -1
    required_sign = outward * factor
    return _VARIANTS_BY_SIGN[stem].get(required_sign, stem)


def _stem_for_lateral_slot(
    stem: Optional[str], slot: int, stems_or_width, ascending: bool = False,
) -> Optional[str]:
    """Back-compat shim for tests/inspect scripts (yaw-unaware, assumes x-run yaw=0).

    Production emit uses :func:`_oriented_end_stem` with the real yaw.  Accepts either
    the full ``stems`` list (preferred) or a width int (legacy).
    """
    if isinstance(stems_or_width, int):
        stems = [None] * stems_or_width
        stems[slot] = stem
    else:
        stems = list(stems_or_width)
    return _oriented_end_stem(stem, slot, stems, 0.0, "x")


def _side_toward(a: Cell, b: Cell) -> str:
    if b[0] > a[0]:
        return "E"
    if b[0] < a[0]:
        return "W"
    if b[1] > a[1]:
        return "S"
    return "N"


def _pick_door_and_foyer(
    strip: SeamStrip,
    walkable: Set[Cell],
    zone_lookup: Callable[[Cell], Optional[str]],
    faction_zone: str,
    toward_faction: str,
    rng: random.Random,
    min_door_depth: int = 1,
) -> Tuple[Cell, List[Cell]]:
    """Door at the entrance — the first faction cell right off the stairs (depth 1).

    The old 2-cell setback (an early directive, since retracted by the user) pushed the
    door deep into the floor where there are no perimeter walls to flank it, so it
    rendered as a freestanding doorframe ("doorway with no connected walls"). At depth 1
    the door sits in the building's entrance face, flanked by the transition walls.
    """
    mid = len(strip.faction_cells) // 2
    seam_fc = strip.faction_cells[mid]
    if seam_fc is None:
        seam_fc = strip.faction_cells[0]
    return seam_fc, []


def _deck_cells_for_plan(
    plan: "TransitionPlan",
    walkable: Set[Cell],
    zone_lookup: Callable[[Cell], Optional[str]],
) -> Set[Cell]:
    """Faction cells from stair landing (seam row) through door — floor goes here."""
    toward = plan.strip.toward_faction
    tdx, tdz = DELTA[toward]
    lo, hi = _stair_column_indices(plan.stair_stems)
    out: Set[Cell] = set()
    for i in range(lo, hi + 1):
        if i >= len(plan.strip.faction_cells):
            continue
        fc = plan.strip.faction_cells[i]
        if fc is None:
            continue
        cur: Optional[Cell] = fc
        while cur is not None and cur in walkable:
            if _zone_of(cur, walkable, zone_lookup) != plan.zone:
                break
            out.add(cur)
            if cur == plan.door_cell:
                break
            cur = (cur[0] + tdx, cur[1] + tdz)
    return out


def _plan_flights(
    stems: List[Optional[str]],
    strip: SeamStrip,
    depth: int,
    width: int,
    rng: random.Random,
) -> Tuple[List[int], float, bool]:
    """Flights per column (1 or 3) and target deck elevation."""
    flights = [0 if s is None else 1 for s in stems]
    multi = False
    deck_y = FLIGHT_RISE
    # Multi-floor (3-flight, 3.6 m, structure-barrier) is DISABLED: it emitted ~22
    # barriers "supporting nothing" + holes in the floor (D6).  Single 1.2 m flight only
    # until the second-floor deck is properly floored. Re-enable behind a real impl.
    _MULTI_FLOOR_ENABLED = False
    if _MULTI_FLOOR_ENABLED and depth >= 3 and width >= 2 and rng.random() < 0.28:
        multi = True
        deck_y = SECOND_DECK_Y
        # one column gets triple stack; optionally second column single flight
        candidates = [i for i, s in enumerate(stems) if s is not None]
        if candidates:
            hi = rng.choice(candidates)
            flights[hi] = 3
            if width >= 3 and rng.random() < 0.45:
                others = [i for i in candidates if i != hi]
                if others:
                    flights[rng.choice(others)] = 1
    return flights, deck_y, multi


def plan_transition(
    boundary: te.ZoneBoundary,
    comp: lc.LevelComposition,
    walkable: Set[Cell],
    zone_lookup: Callable[[Cell], Optional[str]],
    rng: random.Random,
    *,
    ascending: bool,
) -> TransitionPlan:
    # Zone MUST come from the cell's painted position, not faction_id: in the common
    # synth–industrial–synth case prev_faction == next_faction == "synth", so the old
    # ``faction_id == prev_faction`` test always returned "prev" and the NEXT zone's
    # deck/door/elevation (all keyed on plan.zone) silently broke → deck=[], door stuck
    # on the substrate cell = "transition without an opening".
    zone = zone_lookup(boundary.faction_cell) or (
        "prev" if boundary.faction_id == comp.prev_faction else "next"
    )
    if zone not in ("prev", "next"):
        zone = "prev" if boundary.faction_id == comp.prev_faction else "next"
    toward = te._toward_faction_side(boundary.substrate_cell, boundary.faction_cell)
    strip = scan_seam_strip(
        walkable, zone_lookup, boundary.substrate_cell, toward, zone,
        faction_anchor=boundary.faction_cell,
    )
    w = max(1, strip.width)
    stems = pick_lateral_stems(w, rng)
    # Width ≥4 never uses a lone solo-edges piece (rule: 4 straight or 2+2 capped).
    if w >= 4:
        active = [s for s in stems if s]
        if len(active) == 1 and active[0] == SMALL_SOLO:
            stems = _pick_stems_width4(rng) if w == 4 else _pick_stems_width5(rng)
            while len(stems) < w:
                stems = [None] + stems + [None]
            stems = stems[:w]
    while len(stems) < w:
        stems.append(None)
    if not strip.faction_cells or all(c is None for c in strip.faction_cells):
        strip = SeamStrip(
            strip.toward_faction,
            strip.lateral_axis,
            strip.substrate_cells,
            [boundary.faction_cell] * len(strip.substrate_cells),
        )
    mid_fc = strip.faction_cells[len(strip.faction_cells) // 2] or boundary.faction_cell
    depth = _depth_into_faction(mid_fc, toward, walkable, zone_lookup, zone)
    flights, deck_y, multi = _plan_flights(stems, strip, depth, w, rng)
    door, foyer = _pick_door_and_foyer(
        strip, walkable, zone_lookup, zone, toward, rng,
    )
    plan = TransitionPlan(
        boundary=boundary,
        zone=zone,
        ascending=ascending,
        strip=strip,
        stair_stems=stems,
        flights_per_column=flights,
        target_deck_y=deck_y,
        door_cell=door,
        foyer_cells=foyer,
        multi_floor=multi,
    )
    plan.deck_cells = _deck_cells_for_plan(plan, walkable, zone_lookup)
    return plan


def _cell_center(gx: int, gz: int, cell: Cell) -> Tuple[float, float]:
    return te._world_x(gx, cell[0]), te._world_z(gz, cell[1])


def _piece(
    stem: str,
    x: float,
    z: float,
    yaw: float,
    *,
    y: float = 0.0,
    zone: str,
    tags: List[str],
    role: str,
    scale: float = SCALE,
) -> dict:
    return {
        "stem": stem,
        "x": x, "z": z, "yaw": yaw,
        "y": y,
        "floor_level": 0,
        "scale": scale,
        "kit": KIT,
        "zone": zone,
        "tags": tags,
        "role": role,
    }


def _stair_yaw_for_cell(toward_faction: str, ascending: bool) -> float:
    travel = toward_faction if ascending else OPPOSITE[toward_faction]
    return te._stairs_yaw(travel, ascending)


def _place_stair_at_cell(
    gx: int,
    gz: int,
    cell: Cell,
    toward_faction: str,
    stem: str,
    *,
    ascending: bool,
    base_y: float,
    zone: str,
    tags: List[str],
) -> dict:
    """One stair module on its substrate cell, ramp lip toward the faction seam."""
    tdx, tdz = DELTA[toward_faction]
    bx, bz = te._zone_seam_xz(gx, gz, cell, toward_faction)
    # ``stairs-small-*`` are 1-unit modules @ scale 4 → one full 4 m cell each; uniform
    # scale keeps lateral width matched (per-stem rise scaling shrank edges to 3 m).
    scale = SCALE
    high_ext, _ = fp.stair_ramp_footprint_m(stem, scale, KIT)
    sx = bx - tdx * high_ext + tdx * te.STAIRS_SEAM_OVERLAP_M
    sz = bz - tdz * high_ext + tdz * te.STAIRS_SEAM_OVERLAP_M
    yaw = _stair_yaw_for_cell(toward_faction, ascending)
    return _piece(
        stem, sx, sz, yaw, y=base_y, zone=zone,
        tags=tags + ["entrance_stairs", stem],
        role="stairs",
        scale=scale,
    )


def _door_pose(
    gx: int,
    gz: int,
    door_cell: Cell,
    toward_sub: str,
) -> Tuple[float, float, float]:
    """Door on the substrate-facing edge of the door tile (approach side)."""
    return te._cell_face_pose(gx, gz, door_cell, toward_sub)


def _emit_transition_deck(
    plan: TransitionPlan,
    gx: int,
    gz: int,
    walkable: Set[Cell],
    zone_lookup: Callable[[Cell], Optional[str]],
    base_tags: List[str],
) -> List[dict]:
    """``floor-*`` at y=0 on landing path — placed after stairs, before door."""
    out: List[dict] = []
    deck_y = plan.target_deck_y if plan.multi_floor else FLIGHT_RISE
    for cell in sorted(plan.deck_cells):
        stem = sd.pick_deck_stem(
            cell, walkable, zone_lookup, plan.zone, plan.deck_cells, deck_y=deck_y,
        )
        wx, wz = _cell_center(gx, gz, cell)
        p = sd.deck_piece(stem, wx, wz, plan.zone)
        p["tags"] = base_tags + ["transition_deck", stem]
        out.append(p)
    return out


def _wall_piece(
    stem: str,
    x: float,
    z: float,
    yaw: float,
    *,
    y: float,
    zone: str,
    kit: Optional[str],
    tags: List[str],
) -> dict:
    p: dict = {
        "stem": stem,
        "x": x, "z": z, "yaw": yaw,
        "y": y,
        "floor_level": 0,
        "scale": SCALE if kit == KIT else 1.0,
        "zone": zone,
        "tags": tags,
        "role": "wall",
    }
    if kit:
        p["kit"] = kit
    return p


def _has_wall_near(
    existing: List[dict],
    x: float,
    z: float,
    *,
    eps: float = 0.15,
    y: Optional[float] = None,
) -> bool:
    for p in existing:
        if p.get("role") != "wall":
            continue
        if abs(float(p["x"]) - x) < eps and abs(float(p["z"]) - z) < eps:
            if y is None:
                return True
            if abs(float(p.get("y", 0.0)) - y) < 0.05:
                return True
    return False


def _stair_column_indices(stems: List[Optional[str]]) -> Tuple[int, int]:
    idx = [i for i, s in enumerate(stems) if s]
    if not idx:
        return 0, 0
    return min(idx), max(idx)


def emit_transition_walls(
    plan: TransitionPlan,
    gx: int,
    gz: int,
    walkable: Set[Cell],
    zone_lookup: Callable[[Cell], Optional[str]],
    existing: List[dict],
) -> List[dict]:
    """Industrial flank walls along the elevated approach (stairs → door).

    The synth building starts at the door; only industrial walls enclose the
    exterior deck/landing path — not synth walls.
    """
    out: List[dict] = []
    pdx, pdz = _perp_delta(plan.strip.toward_faction)
    base_tags = ["transition_entrance", "synth_transition", "transition_wall", "approach_wall"]

    def add_industrial(cell: Cell, side: str) -> None:
        wx, wz, wyaw = te._cell_face_pose(gx, gz, cell, side)
        if _has_wall_near(existing + out, wx, wz):
            return
        out.append(_wall_piece(
            WALL_INDUSTRIAL, wx, wz, wyaw, y=0.0, zone="default", kit=None,
            tags=base_tags + ["default"],
        ))

    for fc in sorted(plan.deck_cells):
        for sign in (1, -1):
            nb = (fc[0] + sign * pdx, fc[1] + sign * pdz)
            nb_zone = _zone_of(nb, walkable, zone_lookup)
            if nb not in walkable or nb_zone == "default":
                add_industrial(fc, _side_toward(fc, nb))

    return out


def emit_room_integrity_walls(
    plan: TransitionPlan,
    gx: int,
    gz: int,
    walkable: Set[Cell],
    zone_lookup: Callable[[Cell], Optional[str]],
    existing: List[dict],
) -> List[dict]:
    """Close deck/stair corners so room tiles beside wide stairs do not leak outside.

    Two cases at each lateral end of the stair run (see §6 in architecture doc):

    * Side corridor present: synth wall @ deck height on the room cell, facing the
      stair column (closes the void beside the side door).
    * No side corridor (void lateral to deck): synth wall @ deck height on the
      outward face of the deck seam cell.
    """
    w = plan.strip.width
    if w < 2:
        return []

    out: List[dict] = []
    pdx, pdz = _perp_delta(plan.strip.toward_faction)
    base_tags = ["transition_entrance", "synth_transition", "integrity_wall"]
    footprint = plan.deck_cells

    def add_wall(cell: Cell, side: str, tag: str) -> None:
        wx, wz, wyaw = te._cell_face_pose(gx, gz, cell, side)
        if _has_wall_near(existing + out, wx, wz, y=FLIGHT_RISE):
            return
        out.append(_wall_piece(
            WALL_SYNTH, wx, wz, wyaw, y=FLIGHT_RISE, zone=plan.zone, kit=KIT,
            tags=base_tags + [tag],
        ))

    for idx in (0, w - 1):
        fc = plan.strip.faction_cells[idx] if idx < len(plan.strip.faction_cells) else None
        sc = plan.strip.substrate_cells[idx] if idx < len(plan.strip.substrate_cells) else None
        if fc is None or sc is None:
            continue
        out_sign = -1 if idx == 0 else 1
        room = (fc[0] + out_sign * pdx, fc[1] + out_sign * pdz)
        void_nb = (sc[0] + out_sign * pdx, sc[1] + out_sign * pdz)

        if (
            room in walkable
            and _zone_of(room, walkable, zone_lookup) == plan.zone
            and room not in footprint
        ):
            add_wall(room, _side_toward(room, sc), "stair_corner_room")
            continue

        outward = (fc[0] + out_sign * pdx, fc[1] + out_sign * pdz)
        if outward in walkable and _zone_of(outward, walkable, zone_lookup) == plan.zone:
            continue
        if void_nb in walkable and _zone_of(void_nb, walkable, zone_lookup) == plan.zone:
            continue
        odx, odz = out_sign * pdx, out_sign * pdz
        if odx == 1:
            deck_side = "E"
        elif odx == -1:
            deck_side = "W"
        elif odz == 1:
            deck_side = "S"
        else:
            deck_side = "N"
        add_wall(fc, deck_side, "stair_corner_deck")

    return out


WALL_WINDOW = "wall-window"
WALL_BANNER = "wall-banner"
WALL_PILLAR = "wall-pillar"

# Against-wall furniture that fits inside a 4 m cell (front = +z), scale 4.
_WALL_PROPS = ["computer", "computer-system", "computer-screen", "container", "display-wall"]
_CENTRE_PROPS = ["table-display-small", "table-display-planet", "container"]


def _wall_face_cells(gx: int, gz: int, x: float, z: float) -> Tuple[Cell, Cell]:
    """The two cells a wall on this face separates."""
    fxg = x / CELL + gx / 2 - 0.5
    fzg = z / CELL + gz / 2 - 0.5
    if abs(fxg - round(fxg)) > 0.25:  # vertical line (E-W face)
        return (int(round(fxg - 0.5)), int(round(fzg))), (int(round(fxg + 0.5)), int(round(fzg)))
    return (int(round(fxg)), int(round(fzg - 0.5))), (int(round(fxg)), int(round(fzg + 0.5)))


def decorate_synth_walls(
    pieces: List[dict],
    gx: int,
    gz: int,
    walkable: Set[Cell],
    zone_lookup: Callable[[Cell], Optional[str]],
    seed: int,
) -> int:
    """Drop-in dressing for synth walls — windows on the outward perimeter, the odd
    banner inside. ``wall-window``/``wall-banner`` share the exact ``wall`` footprint, so
    this is a pure stem swap: same x/z/yaw/y/scale, still a solid wall (integrity and
    reachability unchanged). Deterministic per seed; sparse on purpose.
    """
    synth = {"prev", "next"}
    rng = random.Random((seed * 2654435761) & 0xFFFFFFFF)
    swapped = 0
    for p in pieces:
        if (
            p.get("role") != "wall"
            or p.get("stem") != "wall"
            or p.get("kit") != KIT
            or int(p.get("floor_level", 0)) != 0
            or p.get("ceiling")
        ):
            continue
        a, b = _wall_face_cells(gx, gz, p["x"], p["z"])
        za = zone_lookup(a) if a in walkable else None
        zb = zone_lookup(b) if b in walkable else None
        a_syn, b_syn = za in synth, zb in synth
        if not (a_syn or b_syn):
            continue
        if a_syn and b_syn:
            # Interior divider — occasional banner / pillar (drop-in, no void concern).
            r = rng.random()
            if r < 0.12:
                p["stem"] = WALL_BANNER
                p["tags"] = list(p.get("tags") or []) + ["synth_decor", "synth_banner"]
                swapped += 1
            elif r < 0.22:
                p["stem"] = WALL_PILLAR
                p["tags"] = list(p.get("tags") or []) + ["synth_decor", "synth_pillar"]
                swapped += 1
            continue
        # Perimeter: one side synth, the other is the far side. A window may ONLY go
        # here if the far side is a real map tile — never void/unbuilt, or the player
        # would see into (and jump through to) the empty world.
        other = b if a_syn else a
        if other not in walkable:
            continue  # faces nothing → keep it a solid wall
        if rng.random() < 0.45:
            p["stem"] = WALL_WINDOW
            p["tags"] = list(p.get("tags") or []) + ["synth_decor", "synth_window"]
            swapped += 1
    return swapped


def furnish_synth_interior(
    pieces: List[dict],
    gx: int,
    gz: int,
    walkable: Set[Cell],
    zone_lookup: Callable[[Cell], Optional[str]],
    deck_cells: Set[Cell],
    seed: int,
) -> int:
    """Sparse furniture on synth interior floor cells — props against a wall facing the
    room, or a centre piece in open bays. Skips deck/entrance cells, door/stair cells and
    their neighbours, and never places two props adjacent (no clutter). Deterministic per
    seed; ``y`` omitted so the elevation pass lifts props onto the 1.2 m deck."""
    synth = {"prev", "next"}
    rng = random.Random((seed * 40503 + 17) & 0xFFFFFFFF)

    def cell_of(p) -> Cell:
        return (int(round(p["x"] / CELL + gx / 2 - 0.5)), int(round(p["z"] / CELL + gz / 2 - 0.5)))

    floor_cells = {
        cell_of(p)
        for p in pieces
        if p.get("role") in ("floor", "deck")
        and p.get("stem") == "floor"
        and p.get("kit") == KIT
        and int(p.get("floor_level", 0)) == 0
        and not p.get("ceiling")
    }
    wall_faces = {
        (round(p["x"], 1), round(p["z"], 1))
        for p in pieces
        if p.get("role") == "wall" and int(p.get("floor_level", 0)) == 0
    }
    block_cells: Set[Cell] = set()  # door/stair cells + neighbours — keep clear
    for p in pieces:
        if p.get("role") in ("door", "stairs") and int(p.get("floor_level", 0)) == 0:
            c = cell_of(p)
            for dx in (-1, 0, 1):
                for dz in (-1, 0, 1):
                    block_cells.add((c[0] + dx, c[1] + dz))

    def has_wall(cell: Cell, side: str) -> bool:
        dx, dz = DELTA[side]
        return (round(te._world_x(gx, cell[0]) + dx * CELL * 0.5, 1),
                round(te._world_z(gz, cell[1]) + dz * CELL * 0.5, 1)) in wall_faces

    placed: Set[Cell] = set()
    n = 0
    for cell in sorted(floor_cells):
        if zone_lookup(cell) not in synth or cell in deck_cells or cell in block_cells:
            continue
        if any((cell[0] + dx, cell[1] + dz) in placed for dx, dz in DELTA.values()):
            continue  # no two props adjacent
        if rng.random() > 0.14:
            continue
        wall_sides = [s for s in DELTA if has_wall(cell, s)]
        open_sides = [
            s for s, (dx, dz) in DELTA.items()
            if (cell[0] + dx, cell[1] + dz) in floor_cells and not has_wall(cell, s)
        ]
        if not open_sides:
            continue
        if wall_sides:
            face = OPPOSITE[wall_sides[0]] if OPPOSITE[wall_sides[0]] in open_sides else open_sides[0]
            stem = _WALL_PROPS[rng.randrange(len(_WALL_PROPS))]
        else:
            face = open_sides[0]
            stem = _CENTRE_PROPS[rng.randrange(len(_CENTRE_PROPS))]
        fdx, fdz = DELTA[face]
        yaw = math.atan2(float(fdx), float(fdz))  # front (+z) points ``face``
        prop = _piece(
            stem, te._world_x(gx, cell[0]), te._world_z(gz, cell[1]), yaw,
            zone=zone_lookup(cell), tags=["synth_decor", "synth_prop", stem], role="prop",
            scale=SCALE,
        )
        prop.pop("y", None)  # elevation pass sets 1.2 m
        pieces.append(prop)
        placed.add(cell)
        n += 1
    return n


def _has_door_near(existing: List[dict], x: float, z: float, *, eps: float = 0.35) -> bool:
    for p in existing:
        if p.get("role") != "door":
            continue
        if abs(float(p["x"]) - x) < eps and abs(float(p["z"]) - z) < eps:
            return True
    return False


def emit_synth_envelope_walls(
    gx: int,
    gz: int,
    walkable: Set[Cell],
    zone_lookup: Callable[[Cell], Optional[str]],
    deck_cells: Set[Cell],
    existing: List[dict],
    corridor_cells: Optional[Set[Cell]] = None,
    door_spec: Optional[fp.EntrancePieceSpec] = None,
) -> List[dict]:
    """Close every synth cell that borders a *walkable* non-synth cell.

    Base-gen walls a side only when its neighbour is the **void**; it never walls a
    synth cell against a walkable ``default`` (industrial) cell, so the synth
    footprint leaks to the outside there — the user's "house with missing walls"
    (e.g. the cells flanking a wide stair run, or a synth corridor abutting a sewer).

    Each such seam face is closed, skipping faces that already carry a wall/door and
    the stair mouths.  The synth zone is one elevated building (every synth cell at
    1.2 m), so a seam face is closed one of two ways:

    * **Corridor crossing** (either side is a corridor cell): it is an intended path,
      so emit a **synth door** (1.2 m) on the synth face **plus stairs** on the
      ``default`` cell bridging 0 → 1.2 m — never a blocking wall.  *"Wherever there
      is a door there is a stair down before the next faction's level."*
    * **Otherwise** (room edge): a synth ``wall`` at 1.2 m.

    Walls are emitted with no ``y`` so ``_apply_zone_elevation`` lifts them to 1.2 m.
    Connectivity is an emergent property of closing the footprint, not an imposed rule.
    """
    out: List[dict] = []
    synth_zones = {"prev", "next"}
    corridor_cells = corridor_cells or set()
    stair_cells: Set[Cell] = set()
    for p in existing:
        if p.get("role") == "stairs":
            ix = int(round(p["x"] / CELL + gx / 2 - 0.5))
            iz = int(round(p["z"] / CELL + gz / 2 - 0.5))
            stair_cells.add((ix, iz))
    base_tags = ["synth_transition", "envelope_wall"]
    crossing_subs: Set[Cell] = set()  # default cells that already got a crossing stair

    def add_wall(cell: Cell, side: str, z: str) -> None:
        wx, wz, wyaw = te._cell_face_pose(gx, gz, cell, side)
        if _has_wall_near(existing + out, wx, wz) or _has_door_near(existing + out, wx, wz):
            return
        p = _wall_piece(WALL_SYNTH, wx, wz, wyaw, y=0.0, zone=z, kit=KIT, tags=base_tags)
        p.pop("y", None)  # zone-elevation pass sets height per cell
        out.append(p)

    def add_crossing(cell: Cell, nb: Cell, side: str, z: str) -> None:
        """Door on the synth face + ascending stairs on the default cell (0 → 1.2)."""
        toward_faction = OPPOSITE[side]  # default ``nb`` → synth ``cell``
        if nb not in stair_cells and nb not in crossing_subs:
            out.append(_place_stair_at_cell(
                gx, gz, nb, toward_faction, SMALL_SOLO,
                ascending=True, base_y=0.0, zone=z,
                tags=base_tags + ["crossing", "ascend"],
            ))
            crossing_subs.add(nb)
        dx, dy, dyaw = te._cell_face_pose(gx, gz, cell, side)
        if not _has_door_near(existing + out, dx, dy):
            out.append(_piece(
                door_spec.stem, dx, dy, dyaw, y=FLIGHT_RISE, zone=z,
                tags=base_tags + ["crossing", "elevated_door"],
                role="door", scale=door_spec.scale,
            ))

    for cell in sorted(walkable):
        z = zone_lookup(cell)
        if z not in synth_zones or cell in deck_cells:
            continue
        for side, (dx, dz) in DELTA.items():
            nb = (cell[0] + dx, cell[1] + dz)
            if nb not in walkable:
                continue  # void face — base-gen / corridor GLB owns it
            if zone_lookup(nb) == z:
                continue  # same building interior — open
            if nb in stair_cells:
                continue  # stair mouth — intended opening
            is_crossing = (cell in corridor_cells or nb in corridor_cells)
            if is_crossing and door_spec is not None:
                add_crossing(cell, nb, side, z)
            else:
                add_wall(cell, side, z)
    return out


def crossing_pieces(
    gx: int,
    gz: int,
    synth_cell: Cell,
    default_cell: Cell,
    zone: str,
    door_spec: fp.EntrancePieceSpec,
    *,
    need_stair: bool,
) -> List[dict]:
    """Door on the synth face toward ``default_cell`` + (optionally) ascending stairs on
    the default cell (0 → 1.2 m).  Used by the accessibility-repair pass to open an
    entrance into an otherwise-sealed synth region."""
    side = _side_toward(synth_cell, default_cell)  # synth -> default
    toward_faction = OPPOSITE[side]                 # default -> synth
    tags = ["synth_transition", "envelope_wall", "crossing", "access_repair"]
    out: List[dict] = []
    if need_stair:
        out.append(_place_stair_at_cell(
            gx, gz, default_cell, toward_faction, SMALL_SOLO,
            ascending=True, base_y=0.0, zone=zone, tags=tags + ["ascend"],
        ))
    dx, dy, dyaw = te._cell_face_pose(gx, gz, synth_cell, side)
    out.append(_piece(
        door_spec.stem, dx, dy, dyaw, y=FLIGHT_RISE, zone=zone,
        tags=tags + ["elevated_door"], role="door", scale=door_spec.scale,
    ))
    return out


def emit_planned_transition(
    plan: TransitionPlan,
    gx: int,
    gz: int,
    walkable: Set[Cell],
    zone_lookup: Callable[[Cell], Optional[str]],
    door_spec: fp.EntrancePieceSpec,
) -> List[dict]:
    pieces: List[dict] = []
    b = plan.boundary
    toward = plan.strip.toward_faction
    # Door faces the SUBSTRATE (the stairs) so it sits in the entrance face flanked by
    # perimeter walls — not the interior edge. (te._toward_substrate_side actually
    # returns the toward-faction direction, which put the door on the inner edge =
    # freestanding doorframe.)
    toward_sub = OPPOSITE[toward]
    kind_tag = "enter_faction" if plan.ascending else "exit_faction"
    base_tags = ["transition_entrance", "synth_transition", kind_tag]

    solo_span = (
        plan.strip.width > 1
        and len([s for s in plan.stair_stems if s]) == 1
        and plan.stair_stems[next(i for i, s in enumerate(plan.stair_stems) if s)] == SMALL_SOLO
    )

    if solo_span:
        mid = len(plan.strip.substrate_cells) // 2
        sc = plan.strip.substrate_cells[mid]
        idx = next(i for i, s in enumerate(plan.stair_stems) if s)
        for f in range(plan.flights_per_column[idx]):
            pieces.append(_place_stair_at_cell(
                gx, gz, sc, toward, SMALL_SOLO,
                ascending=plan.ascending,
                base_y=f * FLIGHT_RISE,
                zone=plan.zone,
                tags=base_tags + (["ascend"] if plan.ascending else ["descend"]) + ["solo"],
            ))
    else:
        run_yaw = _stair_yaw_for_cell(toward, plan.ascending)
        for i, (sc, stem) in enumerate(zip(plan.strip.substrate_cells, plan.stair_stems)):
            if not stem or plan.flights_per_column[i] < 1:
                continue
            stem = _oriented_end_stem(
                stem, i, plan.stair_stems, run_yaw, plan.strip.lateral_axis,
            )
            for f in range(plan.flights_per_column[i]):
                pieces.append(_place_stair_at_cell(
                    gx, gz, sc, toward, stem,
                    ascending=plan.ascending,
                    base_y=f * FLIGHT_RISE,
                    zone=plan.zone,
                    tags=base_tags + (["ascend"] if plan.ascending else ["descend"]),
                ))

    # Floor deck on landing path (after stairs, before door/props).
    pieces.extend(_emit_transition_deck(
        plan, gx, gz, walkable, zone_lookup, base_tags,
    ))

    # Door on deck — substrate-facing edge of door tile.
    dx, dy, dyaw = _door_pose(gx, gz, plan.door_cell, toward_sub)
    # ``space_station`` door origin @ y=0; mesh bottom sits on deck top (1.2 m @ scale 4).
    pieces.append(_piece(
        door_spec.stem, dx, dy, dyaw, y=FLIGHT_RISE, zone=plan.zone,
        tags=base_tags + ["elevated_door"],
        role="door",
        scale=door_spec.scale,
    ))

    # At most one foyer prop; y omitted so zone elevation matches deck (not y=1.2 double-count).
    prop_cells = [fc for fc in plan.foyer_cells if fc != plan.door_cell]
    if prop_cells:
        fc = prop_cells[0]
        fx, fz = _cell_center(gx, gz, fc)
        prop = _piece(
            FOYER_PROP, fx, fz, 0.0, zone=plan.zone,
            tags=base_tags + ["foyer_prop"],
            role="prop",
            scale=SCALE,
        )
        prop.pop("y", None)
        pieces.append(prop)

    return pieces


def interior_cells_for_plan(plan: TransitionPlan, walkable: Set[Cell], zone_lookup) -> Set[Cell]:
    """Faction cells deep enough into the building to receive zone elevation."""
    toward = plan.strip.toward_faction
    tdx, tdz = DELTA[toward]
    start = (plan.door_cell[0] + tdx, plan.door_cell[1] + tdz)
    out: Set[Cell] = set()
    if start not in walkable or _zone_of(start, walkable, zone_lookup) != plan.zone:
        return out
    stack = [start]
    while stack:
        c = stack.pop()
        if c in out:
            continue
        if c not in walkable or _zone_of(c, walkable, zone_lookup) != plan.zone:
            continue
        out.add(c)
        nxt = (c[0] + tdx, c[1] + tdz)
        if nxt in walkable:
            stack.append(nxt)
    return out


def building_footprint(plan: TransitionPlan) -> Set[Cell]:
    return set(plan.deck_cells)


def emit_building_face_doors(
    plan: TransitionPlan,
    gx: int,
    gz: int,
    walkable: Set[Cell],
    zone_lookup: Callable[[Cell], Optional[str]],
    door_spec: fp.EntrancePieceSpec,
    existing: List[dict],
) -> List[dict]:
    """Ground-level synth doors where exterior corridors meet the building footprint."""
    out: List[dict] = []
    footprint = building_footprint(plan)
    base_tags = ["transition_entrance", "synth_transition", "building_face_door"]
    door_cells = {
        (int(round(p["x"] / CELL + gx / 2 - 0.5)), int(round(p["z"] / CELL + gz / 2 - 0.5)))
        for p in existing + out
        if p.get("role") == "door"
    }

    for cell in sorted(walkable):
        if _zone_of(cell, walkable, zone_lookup) != plan.zone:
            continue
        if cell in footprint:
            continue
        for side, (dx, dz) in DELTA.items():
            nb = (cell[0] + dx, cell[1] + dz)
            if nb not in footprint:
                continue
            if cell in door_cells:
                continue
            wx, wz, wyaw = te._cell_face_pose(gx, gz, cell, side)
            out.append(_piece(
                door_spec.stem, wx, wz, wyaw, y=FLIGHT_RISE, zone=plan.zone,
                tags=base_tags + ["elevated_door"],
                role="door",
                scale=door_spec.scale,
            ))
            door_cells.add(cell)
            break
    return out


def collect_interior_cells(
    plans: Sequence[TransitionPlan],
    walkable: Set[Cell],
    zone_lookup: Callable[[Cell], Optional[str]],
) -> Set[Cell]:
    out: Set[Cell] = set()
    for plan in plans:
        out |= interior_cells_for_plan(plan, walkable, zone_lookup)
    return out


def emit_synth_boundary(
    boundary: te.ZoneBoundary,
    gx: int,
    gz: int,
    comp: lc.LevelComposition,
    walkable: Set[Cell],
    zone_lookup: Callable[[Cell], Optional[str]],
    rng: random.Random,
    door_spec: fp.EntrancePieceSpec,
    floor_pieces: Optional[List[dict]] = None,
    wall_context: Optional[List[dict]] = None,
) -> Tuple[List[dict], TransitionPlan]:
    ascending = boundary.kind == "enter_faction"
    plan = plan_transition(
        boundary, comp, walkable, zone_lookup, rng, ascending=ascending,
    )
    if floor_pieces is not None:
        sd.strip_template_floors_at(floor_pieces, gx, gz, plan.deck_cells)
    ctx = wall_context if wall_context is not None else (floor_pieces or [])
    pieces = emit_planned_transition(plan, gx, gz, walkable, zone_lookup, door_spec)
    pieces.extend(emit_transition_walls(
        plan, gx, gz, walkable, zone_lookup, ctx + pieces,
    ))
    # NOTE: emit_building_face_doors REMOVED — it dropped a door on every synth cell
    # facing the deck footprint (a whole column of doors = D1 spam).  The deck is part
    # of the building interior (open), and real synth/non-synth seams are handled by
    # emit_synth_envelope_walls (walls + corridor-crossing doors).
    pieces.extend(emit_room_integrity_walls(
        plan, gx, gz, walkable, zone_lookup, ctx + pieces,
    ))
    return pieces, plan
