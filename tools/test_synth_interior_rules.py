#!/usr/bin/env python3
"""Acceptance tests for synth procgen interior feedback (beds, balconies, mezz, zone spill)."""
from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))

import gen_freeform as gf  # noqa: E402
import level_composition as lc  # noqa: E402
import synth_interior as si  # noqa: E402
import transition_entrances as te  # noqa: E402

DECK = 1.2
COMP = lc.LevelComposition(
    mix_mode="transition", prev_faction="synth", next_faction="synth",
)


def _transition_ix(ps, fm, zone_lookup):
    synth = frozenset({"prev", "next"})
    deck_cells = {si._world_to_cell(p["x"], p["z"], fm.gx, fm.gz) for p in ps if p.get("role") == "deck"}
    trans = set(deck_cells)
    for p in ps:
        if p.get("role") not in ("door", "stairs"):
            continue
        if "indoor_stairs" in (p.get("tags") or []):
            continue
        c = si._world_to_cell(p["x"], p["z"], fm.gx, fm.gz)
        if zone_lookup(c) in synth:
            trans.add(c)
    return deck_cells, trans


def audit_seed(seed: int) -> list[str]:
    fm = gf.generate_map(seed, cells=25, composition=COMP)
    doc = gf.to_doc(fm, f"s{seed}")
    ps = doc["pieces"]
    errs: list[str] = []
    _, _, _, zone_lookup = lc.plan_zones_for_map(fm)
    synth = frozenset({"prev", "next"})

    def world_at(c):
        return te._world_x(fm.gx, c[0]), te._world_z(fm.gz, c[1])

    def cell_at(x, z):
        return si._world_to_cell(x, z, fm.gx, fm.gz)

    deck_cells, trans = _transition_ix(ps, fm, zone_lookup)
    floor_ix, corridor_ix, rooms = si.analyze_synth_zone(
        fm.walkable, zone_lookup, deck_cells, fm.corridor_cells, transition_cells=trans,
    )

    exp = si.expected_balcony_floors(
        floor_ix,
        corridor_ix,
        world_at=world_at,
        walkable=fm.walkable,
        zone_lookup=zone_lookup,
        substrate_cells=si.substrate_floor_cells(ps, zone_lookup, cell_at=cell_at),
    )
    placed = {
        (round(p["x"], 1), round(p["z"], 1))
        for p in ps
        if str(p.get("stem", "")).startswith("balcony-floor")
    }
    if placed != set(exp):
        errs.append(f"balcony layout mismatch: extra={len(placed - set(exp))} missing={len(set(exp) - placed)}")

    for p in ps:
        if not str(p.get("stem", "")).startswith("bed"):
            continue
        if abs(float(p.get("y", 0)) - DECK) > 0.05:
            errs.append(f"bed y={p.get('y')}")
        c = cell_at(p["x"], p["z"])
        if c in corridor_ix:
            errs.append(f"bed in corridor {c}")
        for cells in rooms.values():
            if c in cells and not si.valid_quarters_room(cells, corridor_ix, trans):
                errs.append(f"bed in invalid quarters room at {c}")

    for p in ps:
        if "synth_interior" not in (p.get("tags") or []):
            continue
        if p.get("stem", "").startswith(("computer", "chair")):
            bb = si.world_bbox(p["stem"], p["x"], p["z"], p["yaw"], p.get("scale", 4.0))
            for fc in si._bbox_footprint_cells(bb, cell_at):
                z = zone_lookup(fc)
                if z is not None and z not in synth:
                    errs.append(f"desk/chair spills to {z}: {p['stem']}")

    roles = si.assign_roles(rooms, corridor_ix, trans)
    room_infos = si.build_room_infos(rooms, corridor_ix, roles)
    for info in room_infos:
        if info.role != "quarters":
            continue
        cells_w = {si._cell_world(c, world_at) for c in info.cells_ix}
        rng = __import__("random").Random((seed * 40503 + 17) & 0xFFFFFFFF)
        expected = si.setup_quarters(
            cells_w, info.cells_ix, corridor_ix, rng,
            zone_lookup=zone_lookup, transition_ix=trans, cell_at=cell_at,
        )
        expected_beds = [p for p in expected if str(p.get("stem", "")).startswith("bed")]
        beds = [
            p for p in ps
            if str(p.get("stem", "")).startswith("bed")
            and f"room_{info.room_id}" in (p.get("tags") or [])
        ]
        if expected_beds and not beds:
            errs.append(f"quarters room {info.room_id} has no beds")

    command = next((i for i in room_infos if i.role == "command"), None)
    if command and si.mezzanine_plan(command, corridor_ix):
        mf = [p for p in ps if "mezz_floor" in (p.get("tags") or [])]
        if not mf:
            errs.append("command mezz missing mezz_floor pieces")
        elif any(p.get("stem") != "floor" for p in mf):
            errs.append("mezz floor must use floor stem (same GLB as deck)")
        elif any(abs(float(p.get("y", 0)) - mf[0].get("y", 0)) > 0.01 for p in mf):
            errs.append("mezz floor pieces must share deck_origin y")
        plan = si.mezzanine_plan(command, corridor_ix)
        if plan and si._deck_adjacent_corridor(plan["deck_cells"], corridor_ix):
            errs.append("mezz deck must not adjoin corridor")
        stairs = [
            p for p in ps
            if p.get("stem") == si.MEZZ_STAIR_STEM and "indoor_stairs" in (p.get("tags") or [])
        ]
        if plan and len(stairs) != 1:
            errs.append(f"mezz must use one short stair, got {len(stairs)}")
        elif plan and stairs:
            deck_izs = sorted({iz for _, iz in plan["deck_cells"]})
            want_yaw = round(te._stairs_yaw(plan["stair_travel"], True), 4)
            got_yaw = round(float(stairs[0].get("yaw", 0.0)), 4)
            if got_yaw != want_yaw:
                errs.append(f"mezz stair yaw {got_yaw} != {want_yaw} (travel {plan['stair_travel']})")
            loft = [p for p in ps if "loft_ops" in (p.get("tags") or []) or "loft_storage" in (p.get("tags") or [])]
            for p in loft:
                if abs(float(p.get("y", 0)) - plan["deck_top"]) > 0.05:
                    errs.append(f"loft prop y={p.get('y')} != deck_top {plan['deck_top']}")
                want_face = si.WALL_YAW_INTO[plan["parapet_wall"]]
                if abs(float(p.get("yaw", 0)) - want_face) > 0.05:
                    errs.append(f"loft prop yaw {p.get('yaw')} != {want_face} ({plan['parapet_wall']})")

    return errs


def main() -> None:
    bad = []
    for seed in range(1, 51):
        errs = audit_seed(seed)
        if errs:
            bad.append((seed, errs))
    if bad:
        print(f"FAIL: {len(bad)}/50 seeds")
        for seed, errs in bad[:8]:
            print(f"  seed {seed}: {errs[0]}")
        sys.exit(1)
    print("OK: synth interior rules (50 seeds)")


if __name__ == "__main__":
    main()
