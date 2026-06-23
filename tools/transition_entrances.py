#!/usr/bin/env python3
"""Industrial ↔ faction transition entrances (manifest §2.3 / §4.1b).

Elevated factions (e.g. synth): industrial is at y=0; faction deck is raised.
  * Enter (industrial → faction): stairs UP on industrial, then door on elevated deck.
  * Exit  (faction → industrial): door on elevated deck, then stairs DOWN on industrial.

Stair placement is computed from probed GLB lip geometry (``tools/mesh_metrics.py``) so
the ramp top meets the elevated deck seam without manual tuning.
"""
from __future__ import annotations

import math
import random
from dataclasses import dataclass
from typing import Callable, List, Literal, Optional, Set, Tuple

import faction_profiles as fp
import level_composition as lc
import mesh_metrics

Cell = Tuple[int, int]
Zone = Literal["prev", "default", "next"]
BoundaryKind = Literal["exit_faction", "enter_faction"]
BoundaryShape = Literal["straight", "outer_corner"]

CELL = 4.0
PI = math.pi
PI2 = math.pi / 2.0
PI32 = 3.0 * math.pi / 2.0
# Lip sits this far onto the faction deck (metres) — closes sub-centimetre render gaps only.
STAIRS_SEAM_OVERLAP_M = 0.002

DELTA: dict[str, Cell] = {"N": (0, -1), "S": (0, 1), "E": (1, 0), "W": (-1, 0)}
OPPOSITE: dict[str, str] = {"N": "S", "S": "N", "E": "W", "W": "E"}
WALL_YAW = {"N": PI, "S": 0.0, "E": PI2, "W": PI32}


@dataclass(frozen=True)
class ZoneBoundary:
    spine_index: int
    kind: BoundaryKind
    faction_id: str
    substrate_cell: Cell
    faction_cell: Cell


def _world_x(gx: int, ix: int) -> float:
    return (ix - gx / 2 + 0.5) * CELL


def _world_z(gz: int, iz: int) -> float:
    return (iz - gz / 2 + 0.5) * CELL


def find_zone_boundaries(spine: List[Cell], comp: lc.LevelComposition) -> List[ZoneBoundary]:
    if comp.mix_mode != "transition" or len(spine) < 2:
        return []

    c = comp.normalized()
    out: List[ZoneBoundary] = []
    n = len(spine) - 1
    for i in range(n):
        t0 = i / max(1, n)
        t1 = (i + 1) / max(1, n)
        z0 = lc.zone_at_spine_t(t0, c)
        z1 = lc.zone_at_spine_t(t1, c)
        if z0 == z1:
            continue
        if z0 != "default" and z1 == "default":
            out.append(ZoneBoundary(
                spine_index=i,
                kind="exit_faction",
                faction_id=c.prev_faction if z0 == "prev" else c.next_faction,
                substrate_cell=spine[i + 1],
                faction_cell=spine[i],
            ))
        elif z0 == "default" and z1 != "default":
            out.append(ZoneBoundary(
                spine_index=i,
                kind="enter_faction",
                faction_id=c.next_faction if z1 == "next" else c.prev_faction,
                substrate_cell=spine[i],
                faction_cell=spine[i + 1],
            ))
    return out


def _toward_faction_side(substrate_cell: Cell, faction_cell: Cell) -> str:
    ax, az = substrate_cell
    bx, bz = faction_cell
    if bx > ax:
        return "E"
    if bx < ax:
        return "W"
    if bz > az:
        return "S"
    return "N"


def _toward_substrate_side(faction_cell: Cell, substrate_cell: Cell) -> str:
    return _toward_faction_side(substrate_cell, faction_cell)


def _cell_face_pose(
    gx: int,
    gz: int,
    cell: Cell,
    side: str,
) -> Tuple[float, float, float]:
    dx, dz = DELTA[side]
    ax, az = cell
    return (
        _world_x(gx, ax) + dx * CELL * 0.5,
        _world_z(gz, az) + dz * CELL * 0.5,
        WALL_YAW[side],
    )


def _walkable_neighbor(cell: Cell, side: str, walkable: Optional[Set[Cell]]) -> bool:
    if not walkable:
        return False
    dx, dz = DELTA[side]
    return (cell[0] + dx, cell[1] + dz) in walkable


def _substrate_boundary_shape(
    substrate_cell: Cell,
    toward_faction: str,
    walkable: Optional[Set[Cell]],
) -> BoundaryShape:
    """Classify the industrial cell at a zone seam (straight vs outer corner)."""
    if not walkable:
        return "straight"
    tdx, tdz = DELTA[toward_faction]
    perp_a = (-tdz, -tdx)
    perp_b = (tdz, tdx)
    interior = _walkable_neighbor(substrate_cell, OPPOSITE[toward_faction], walkable)
    side_a = (substrate_cell[0] + perp_a[0], substrate_cell[1] + perp_a[1]) in walkable
    side_b = (substrate_cell[0] + perp_b[0], substrate_cell[1] + perp_b[1]) in walkable
    if interior and (side_a ^ side_b) and not (side_a and side_b):
        return "outer_corner"
    return "straight"


def _pick_stair_stem(
    default: fp.EntrancePieceSpec,
    shape: BoundaryShape,
) -> fp.EntrancePieceSpec:
    """Select stair GLB from boundary topology (corner variants probed like straight)."""
    if shape == "outer_corner":
        corner = "stairs-small-corner"
        if mesh_metrics.resolve_glb(corner, default.kit):
            return fp.EntrancePieceSpec(
                stem=corner, kit=default.kit, scale=default.scale,
                yaw_offset=default.yaw_offset,
            )
    return default


def _stairs_yaw(travel_side: str, ascending: bool) -> float:
    """Yaw for ``stairs-small-center`` family (ramp climbs along local −Z).

    Ascending: walk ``travel_side`` uphill (−Z aligned with travel).
    Descending: walk ``travel_side`` downhill (+Z aligned with travel).
    """
    dx, dz = DELTA[travel_side]
    if ascending:
        return math.atan2(-float(dx), -float(dz))
    return math.atan2(float(dx), float(dz))


def _zone_seam_xz(
    gx: int,
    gz: int,
    substrate_cell: Cell,
    toward_faction: str,
) -> Tuple[float, float]:
    """World XZ of the industrial/faction cell boundary (shared edge)."""
    tdx, tdz = DELTA[toward_faction]
    cx = _world_x(gx, substrate_cell[0])
    cz = _world_z(gz, substrate_cell[1])
    half = CELL * 0.5
    return cx + tdx * half, cz + tdz * half


def _stairs_on_substrate_pose(
    gx: int,
    gz: int,
    substrate_cell: Cell,
    toward_faction_side: str,
    *,
    stem: str,
    kit: str,
    scale: float,
    ascending: bool,
    yaw_offset: float,
) -> Tuple[float, float, float]:
    """Place stairs so the probed ramp lip meets the zone seam (± overlap)."""
    tdx, tdz = DELTA[toward_faction_side]
    bx, bz = _zone_seam_xz(gx, gz, substrate_cell, toward_faction_side)
    high_ext, _low_ext = fp.stair_ramp_footprint_m(stem, scale, kit)
    # Lip at seam + tiny overlap onto deck (mm-scale, not a tread width).
    sx = bx - tdx * high_ext + tdx * STAIRS_SEAM_OVERLAP_M
    sz = bz - tdz * high_ext + tdz * STAIRS_SEAM_OVERLAP_M
    travel = toward_faction_side if ascending else OPPOSITE[toward_faction_side]
    yaw = _stairs_yaw(travel, ascending) + yaw_offset
    return sx, sz, yaw


def ramp_lip_xz(
    sx: float,
    sz: float,
    toward_faction: str,
    stem: str,
    kit: str,
    scale: float,
) -> Tuple[float, float]:
    """World XZ of the stair ramp top lip (for automated seam verification)."""
    tdx, tdz = DELTA[toward_faction]
    high_ext, _ = fp.stair_ramp_footprint_m(stem, scale, kit)
    return sx + tdx * high_ext, sz + tdz * high_ext


def seam_alignment_error_m(
    lip_x: float,
    lip_z: float,
    seam_x: float,
    seam_z: float,
    toward_faction: str,
) -> float:
    """Signed metres along the seam normal (+ = onto faction deck)."""
    tdx, tdz = DELTA[toward_faction]
    return (lip_x - seam_x) * tdx + (lip_z - seam_z) * tdz


def _assert_stair_height_matches_rise(stairs: fp.EntrancePieceSpec, rise: float) -> None:
    top = fp.stair_top_height_m(stairs.stem, stairs.scale, stairs.kit)
    if abs(top - rise) > 0.05:
        raise ValueError(
            f"{stairs.stem} top {top:.3f}m @ scale {stairs.scale} "
            f"!= elevation_rise {rise:.3f}m — adjust scale or profile"
        )


def _faction_has_elevation(prof: fp.FactionProcgenProfile) -> bool:
    tr = prof.transition
    return tr.entrance_stairs is not None and tr.elevation_rise > 1e-6


def _resolve_door(prof: fp.FactionProcgenProfile) -> fp.EntrancePieceSpec:
    """Door GLB for a boundary — always an ``EntrancePieceSpec`` (with scale)."""
    if prof.transition.entrance_door:
        return prof.transition.entrance_door
    hd = prof.hidden_door
    return fp.EntrancePieceSpec(stem=hd.stem, kit=hd.kit, scale=1.0)


def emit_transition_pieces(
    gx: int,
    gz: int,
    spine: List[Cell],
    comp: lc.LevelComposition,
    walkable: Optional[Set[Cell]] = None,
    zone_lookup: Optional[Callable[[Cell], Optional[str]]] = None,
    rng: Optional[random.Random] = None,
    existing_pieces: Optional[List[dict]] = None,
) -> Tuple[List[dict], List["st.TransitionPlan"]]:
    import random as _random

    import synth_transition as st

    pieces: List[dict] = []
    plans: List[st.TransitionPlan] = []
    rng = rng or _random.Random(0)
    zfn = zone_lookup or (lambda _c: None)
    walkable = walkable or set()
    base = existing_pieces or []

    for boundary in find_zone_boundaries(spine, comp):
        prof = fp.load_profile(boundary.faction_id)
        tr = prof.transition
        door = _resolve_door(prof)
        zone = "prev" if boundary.faction_id == comp.prev_faction else "next"
        elevated = _faction_has_elevation(prof)

        if elevated and tr.entrance_stairs and prof.id == "synth":
            batch, plan = st.emit_synth_boundary(
                boundary, gx, gz, comp, walkable, zfn, rng, door,
                floor_pieces=base,
                wall_context=base + pieces,
            )
            pieces.extend(batch)
            plans.append(plan)
            continue

        rise = tr.elevation_rise if elevated else 0.0
        toward_faction = _toward_faction_side(
            boundary.substrate_cell, boundary.faction_cell,
        )
        toward_substrate = _toward_substrate_side(
            boundary.faction_cell, boundary.substrate_cell,
        )

        if elevated and tr.entrance_stairs:
            shape = _substrate_boundary_shape(
                boundary.substrate_cell, toward_faction, walkable,
            )
            stairs = _pick_stair_stem(tr.entrance_stairs, shape)
            _assert_stair_height_matches_rise(stairs, rise)
            if boundary.kind == "enter_faction":
                # Industrial → synth: stairs up, then elevated door.
                sx, sz, syaw = _stairs_on_substrate_pose(
                    gx, gz, boundary.substrate_cell, toward_faction,
                    stem=stairs.stem, kit=stairs.kit, scale=stairs.scale,
                    ascending=True, yaw_offset=stairs.yaw_offset,
                )
                pieces.append({
                    "stem": stairs.stem,
                    "x": sx, "z": sz, "yaw": syaw,
                    "y": 0.0,
                    "floor_level": 0,
                    "scale": stairs.scale,
                    "kit": stairs.kit,
                    "zone": zone,
                    "tags": ["transition_entrance", "entrance_stairs", "ascend", shape],
                    "role": "stairs",
                })
                dx, dy, dyaw = _cell_face_pose(
                    gx, gz, boundary.faction_cell, toward_substrate,
                )
                pieces.append({
                    "stem": door.stem,
                    "x": dx, "z": dy, "yaw": dyaw,
                    "y": rise,
                    "floor_level": 0,
                    "scale": door.scale,
                    "kit": door.kit,
                    "zone": zone,
                    "tags": ["transition_entrance", boundary.kind, "elevated_door"],
                    "role": "door",
                })
            else:
                # Synth → industrial: elevated door, then stairs down.
                dx, dy, dyaw = _cell_face_pose(
                    gx, gz, boundary.faction_cell, toward_substrate,
                )
                pieces.append({
                    "stem": door.stem,
                    "x": dx, "z": dy, "yaw": dyaw,
                    "y": rise,
                    "floor_level": 0,
                    "scale": door.scale,
                    "kit": door.kit,
                    "zone": zone,
                    "tags": ["transition_entrance", boundary.kind, "elevated_door"],
                    "role": "door",
                })
                sx, sz, syaw = _stairs_on_substrate_pose(
                    gx, gz, boundary.substrate_cell, toward_faction,
                    stem=stairs.stem, kit=stairs.kit, scale=stairs.scale,
                    ascending=False, yaw_offset=stairs.yaw_offset,
                )
                pieces.append({
                    "stem": stairs.stem,
                    "x": sx, "z": sz, "yaw": syaw,
                    "y": 0.0,
                    "floor_level": 0,
                    "scale": stairs.scale,
                    "kit": stairs.kit,
                    "zone": zone,
                    "tags": ["transition_entrance", "entrance_stairs", "descend", shape],
                    "role": "stairs",
                })
        else:
            wx, wz, yaw = _cell_face_pose(
                gx, gz, boundary.substrate_cell, toward_faction,
            )
            pieces.append({
                "stem": door.stem,
                "x": wx, "z": wz, "yaw": yaw,
                "floor_level": 0,
                "scale": door.scale,
                "kit": door.kit,
                "zone": zone,
                "tags": ["transition_entrance", boundary.kind],
                "role": "door",
            })

    return pieces, plans
