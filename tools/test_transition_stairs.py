#!/usr/bin/env python3
"""Stair yaw, seam alignment, and elevation checks (no visual playtest needed)."""
from __future__ import annotations

import math
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))

import faction_profiles as fp  # noqa: E402
import level_composition as lc  # noqa: E402
import mesh_metrics  # noqa: E402
import transition_entrances as te  # noqa: E402


def _assert(cond: bool, msg: str) -> None:
    if not cond:
        raise AssertionError(msg)


def test_descend_yaw_differs_from_wrong_mirror() -> None:
    for toward in te.DELTA:
        travel = te.OPPOSITE[toward]
        dx, dz = te.DELTA[travel]
        wrong = math.atan2(-float(dx), -float(dz))
        right = te._stairs_yaw(travel, ascending=False)
        _assert(
            abs((wrong - right + math.pi) % (2 * math.pi) - math.pi) > 0.01,
            f"descend yaw for travel {travel} should differ from ascend mirror",
        )


def test_ascend_descend_share_ramp_yaw() -> None:
    for toward in te.DELTA:
        ya = te._stairs_yaw(toward, ascending=True)
        yd = te._stairs_yaw(te.OPPOSITE[toward], ascending=False)
        diff = abs((yd - ya + math.pi) % (2 * math.pi) - math.pi)
        _assert(diff < 1e-4, f"ascend/descend yaws should match for toward={toward}")


def test_ramp_lip_at_seam_all_directions() -> None:
    """Probed lip lands on the zone seam (2 mm deck overlap, not centimetres shy)."""
    stem, kit, scale = "stairs-small-center", "space_station", 4.0
    gx = gz = 40
    substrate = (20, 20)
    for toward in te.DELTA:
        sx, sz, _ = te._stairs_on_substrate_pose(
            gx, gz, substrate, toward,
            stem=stem, kit=kit, scale=scale, ascending=True, yaw_offset=0.0,
        )
        bx, bz = te._zone_seam_xz(gx, gz, substrate, toward)
        lx, lz = te.ramp_lip_xz(sx, sz, toward, stem, kit, scale)
        err = te.seam_alignment_error_m(lx, lz, bx, bz, toward)
        _assert(
            abs(err - te.STAIRS_SEAM_OVERLAP_M) < 1e-3,
            f"seam error {err:.4f}m toward={toward} (want {te.STAIRS_SEAM_OVERLAP_M})",
        )


def test_stair_top_matches_synth_elevation() -> None:
    prof = fp.load_profile("synth")
    stairs = prof.transition.entrance_stairs
    assert stairs is not None
    top = fp.stair_top_height_m(stairs.stem, stairs.scale, stairs.kit)
    rise = prof.transition.elevation_rise
    _assert(abs(top - rise) < 0.01, f"stair top {top} != rise {rise}")


def test_mesh_probe_matches_hardcoded_lip() -> None:
    m = mesh_metrics.stair_metrics("stairs-small-center", "space_station")
    _assert(abs(m.toward_neg_z_m - 0.2) < 0.01, "lip -Z extent drift")
    _assert(abs(m.top_y_m - 0.3) < 0.01, "top Y extent drift")


def test_full_gen_seam_alignment() -> None:
    """Procgen map: every synth stair lip within 15 mm of its zone seam."""
    import gen_freeform as gf

    comp = lc.LevelComposition(
        mix_mode="transition", prev_faction="synth", next_faction="synth",
    )
    fm = gf.generate_map(1, cells=40, composition=comp)
    assert fm is not None
    doc = gf.to_doc(fm, "seam_test")
    spine, comp_n, _, _ = lc.plan_zones_for_map(fm)
    boundaries = te.find_zone_boundaries(spine, comp_n)
    by_kind = {b.kind: b for b in boundaries}
    for s in [p for p in doc["pieces"] if p.get("role") == "stairs"]:
        if "crossing" in s.get("tags", []):
            continue  # corridor-crossing stair aligns to its own cell seam, not a boundary
        toward = None
        for b in boundaries:
            if "ascend" in s.get("tags", []) and b.kind == "enter_faction":
                toward = te._toward_faction_side(b.substrate_cell, b.faction_cell)
                substrate = b.substrate_cell
                break
            if "descend" in s.get("tags", []) and b.kind == "exit_faction":
                toward = te._toward_faction_side(b.substrate_cell, b.faction_cell)
                substrate = b.substrate_cell
                break
        _assert(toward is not None, f"unmatched stair {s}")
        bx, bz = te._zone_seam_xz(fm.gx, fm.gz, substrate, toward)
        lx, lz = te.ramp_lip_xz(
            s["x"], s["z"], toward, s["stem"], s.get("kit", "space_station"), s["scale"],
        )
        err = te.seam_alignment_error_m(lx, lz, bx, bz, toward)
        _assert(
            abs(err - te.STAIRS_SEAM_OVERLAP_M) < 0.015,
            f"gen seam error {err:.4f}m tags={s.get('tags')}",
        )


def main() -> None:
    test_descend_yaw_differs_from_wrong_mirror()
    test_ascend_descend_share_ramp_yaw()
    test_ramp_lip_at_seam_all_directions()
    test_stair_top_matches_synth_elevation()
    test_mesh_probe_matches_hardcoded_lip()
    test_full_gen_seam_alignment()
    print("ok: transition stair tests passed")


if __name__ == "__main__":
    main()
