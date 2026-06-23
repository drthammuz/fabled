#!/usr/bin/env python3
"""Synth transition layout rules (edge caps, width, structure)."""
from __future__ import annotations

import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))

import gen_freeform as gf  # noqa: E402
import level_composition as lc  # noqa: E402
import synth_transition as st  # noqa: E402
import transition_entrances as te  # noqa: E402

SOLO = st.SMALL_SOLO
CENTER = st.SMALL_MID
EDGE = st.SMALL_LEFT


def _assert(cond: bool, msg: str) -> None:
    if not cond:
        raise AssertionError(msg)


def test_single_stem_is_edges() -> None:
    for w in (1, 2, 5):
        stems = st.pick_lateral_stems(w, __import__("random").Random(0))
        if w == 1:
            _assert(stems == [SOLO], f"width 1 must be solo edges, got {stems}")
        if w == 2 and stems.count(SOLO) == 1:
            _assert(SOLO in stems, "width 2 solo must use edges piece")


def test_width3_never_bare_center() -> None:
    rng = __import__("random").Random(1)
    for _ in range(20):
        stems = st.pick_lateral_stems(3, rng)
        if stems == [CENTER, CENTER, CENTER]:
            raise AssertionError("3-wide must not be three bare centers")
        if CENTER in stems and stems.count(CENTER) == 1:
            ends = (stems[0], stems[-1])
            cap = ("edge" in ends[0] or "corner" in ends[0]) and (
                "edge" in ends[1] or "corner" in ends[1]
            )
            _assert(cap, f"3-wide center run needs end caps: {stems}")


def test_width4_never_solo() -> None:
    rng = __import__("random").Random(3)
    for _ in range(30):
        stems = st.pick_lateral_stems(4, rng)
        active = [s for s in stems if s]
        assert not (len(active) == 1 and active[0] == SOLO), stems


def test_no_duplicate_floor_at_deck() -> None:
    comp = lc.LevelComposition(
        mix_mode="transition", prev_faction="synth", next_faction="synth",
    )
    for seed in range(12):
        fm = gf.generate_map(seed, cells=40, composition=comp)
        overlap = gf.audit_floor_overlaps(gf.to_doc(fm, "t")["pieces"])
        assert not overlap, f"seed {seed}: {overlap[:3]}"


def test_gen_transition_deck_on_landing_path() -> None:
    comp = lc.LevelComposition(
        mix_mode="transition", prev_faction="synth", next_faction="synth",
    )
    fm = gf.generate_map(6, cells=40, composition=comp)
    doc = gf.to_doc(fm, "t")
    decks = [p for p in doc["pieces"] if p.get("role") == "deck"]
    stairs = [p for p in doc["pieces"] if p.get("role") == "stairs"]
    ind_walls = [
        p for p in doc["pieces"]
        if p.get("role") == "wall" and "approach_wall" in (p.get("tags") or [])
    ]
    synth_walls = [
        p for p in doc["pieces"]
        if p.get("role") == "wall" and "transition_wall" in (p.get("tags") or [])
        and p.get("zone") in ("prev", "next")
    ]
    _assert(len(stairs) >= 2, "expected synth stairs at boundaries")
    _assert(len(decks) > 0, "expected transition deck floor blocks")
    _assert(len(decks) < 80, "deck must be transition-local, not whole zone")
    for p in decks:
        _assert(p.get("y", 0) == 0, f"deck {p['stem']} must sit at y=0")
        _assert(p["stem"] == "floor" or p["stem"] == "structure-barrier", p["stem"])
        _assert(p.get("kit") == "factions/synth", p.get("kit"))
    _assert(len(synth_walls) == 0, f"synth walls only inside building, got {len(synth_walls)}")
    spine, _, _, zfn = lc.plan_zones_for_map(fm)
    rng = __import__("random").Random((6 * 1597334677) & 0xFFFFFFFF)
    for b in te.find_zone_boundaries(spine, comp):
        if b.kind != "exit_faction":
            continue
        plan = st.plan_transition(b, comp, fm.walkable, zfn, rng, ascending=False)
        approach = st.emit_transition_walls(plan, fm.gx, fm.gz, fm.walkable, zfn, [])
        _assert(len(approach) >= 1, "expected industrial approach walls from planner")
        if plan.strip.width >= 3:
            _assert_end_rails_outward(plan)
            break
    else:
        _assert(False, "expected a wide (≥3) exit transition")


def _assert_end_rails_outward(plan) -> None:
    """Both active-end stairs must carry their rail OUTWARD along the lateral axis."""
    import math

    yaw = st._stair_yaw_for_cell(plan.strip.toward_faction, plan.ascending)
    lo, hi = st._active_span(plan.stair_stems)
    factor = math.cos(yaw) if plan.strip.lateral_axis == "x" else -math.sin(yaw)
    factor = 1 if factor >= 0 else -1
    for slot, outward in ((lo, -1), (hi, +1)):
        stem = plan.stair_stems[slot]
        if not stem or stem not in st._RAIL_LOCAL_SIGN:
            continue
        mapped = st._oriented_end_stem(
            stem, slot, plan.stair_stems, yaw, plan.strip.lateral_axis,
        )
        world_lat = st._RAIL_LOCAL_SIGN[mapped] * factor
        _assert(
            world_lat == outward,
            f"end stair {mapped} at slot {slot} rail faces inward (yaw {yaw})",
        )


def test_seam_strip_contiguous() -> None:
    """Faction-row walk must not merge non-adjacent substrate cells (seed 6 regression)."""
    import random

    comp = lc.LevelComposition(
        mix_mode="transition", prev_faction="synth", next_faction="synth",
    )
    fm = gf.generate_map(6, cells=40, composition=comp)
    spine, _, _, zfn = lc.plan_zones_for_map(fm)
    rng = random.Random((6 * 1597334677) & 0xFFFFFFFF)
    for b in te.find_zone_boundaries(spine, comp):
        if b.kind != "exit_faction":
            continue
        plan = st.plan_transition(b, comp, fm.walkable, zfn, rng, ascending=False)
        cells = plan.strip.substrate_cells
        if len(cells) < 2:
            continue
        axis = plan.strip.lateral_axis
        keys = [c[0] if axis == "x" else c[1] for c in cells]
        for a, b in zip(keys, keys[1:]):
            _assert(abs(a - b) == 1, f"non-contiguous seam strip {cells}")


def test_stair_uniform_scale() -> None:
    comp = lc.LevelComposition(
        mix_mode="transition", prev_faction="synth", next_faction="synth",
    )
    fm = gf.generate_map(6, cells=40, composition=comp)
    doc = gf.to_doc(fm, "t")
    scales = {round(float(p["scale"]), 3) for p in doc["pieces"] if p.get("role") == "stairs"}
    _assert(scales == {4.0}, f"stairs must use uniform scale 4, got {scales}")


def test_no_prev_next_adjacency() -> None:
    comp = lc.LevelComposition(
        mix_mode="transition", prev_faction="synth", next_faction="synth",
    )
    for seed in range(20):
        fm = gf.generate_map(seed, cells=40, composition=comp)
        spine, _, _, zfn = lc.plan_zones_for_map(fm)
        zones = lc.assign_zones_for_map(fm.walkable, spine, comp)
        for c in fm.walkable:
            for dx, dz in ((0, 1), (0, -1), (1, 0), (-1, 0)):
                nb = (c[0] + dx, c[1] + dz)
                if nb not in fm.walkable:
                    continue
                pair = {zones[c], zones[nb]}
                _assert(
                    pair != {"prev", "next"},
                    f"seed {seed}: prev/next touch at {c}/{nb}",
                )


def test_room_integrity_walls_on_wide_exit() -> None:
    """Wide stair + side corridor must get deck-height corner walls (seed 6 regression)."""
    import random

    comp = lc.LevelComposition(
        mix_mode="transition", prev_faction="synth", next_faction="synth",
    )
    fm = gf.generate_map(6, cells=40, composition=comp)
    doc = gf.to_doc(fm, "t")
    spine, _, _, zfn = lc.plan_zones_for_map(fm)
    rng = random.Random((6 * 1597334677) & 0xFFFFFFFF)
    integrity = [
        p for p in doc["pieces"]
        if "integrity_wall" in (p.get("tags") or [])
    ]
    for b in te.find_zone_boundaries(spine, comp):
        if b.kind != "exit_faction":
            continue
        plan = st.plan_transition(b, comp, fm.walkable, zfn, rng, ascending=False)
        if plan.strip.width < 2:
            continue
        _assert(len(integrity) >= 1, "expected integrity walls on wide exit transition")
        for p in integrity:
            _assert(p.get("y") == st.FLIGHT_RISE, "integrity wall must sit on deck top")
        return
    _assert(False, "seed 6 should have a wide exit transition")


def main() -> None:
    test_single_stem_is_edges()
    test_width3_never_bare_center()
    test_width4_never_solo()
    test_no_duplicate_floor_at_deck()
    test_gen_transition_deck_on_landing_path()
    test_seam_strip_contiguous()
    test_stair_uniform_scale()
    test_no_prev_next_adjacency()
    test_room_integrity_walls_on_wide_exit()
    print("ok: synth transition tests passed")


if __name__ == "__main__":
    main()
