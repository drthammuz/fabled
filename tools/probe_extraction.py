#!/usr/bin/env python3
"""Verify extraction pit + hub layout (map JSON and runtime kenney_layout.json)."""
from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import gen_maps as gm  # noqa: E402
import generate_kenney_catalog as kcat  # noqa: E402
import probe_map_geometry as probe  # noqa: E402

LAYOUT_PATH = Path("userinput/kenney_layout.json")


def in_end_module(p: dict, mcx: float, mcz: float) -> bool:
    return gm.in_module_bounds(p, mcx, mcz)


def end_slot_from_extraction(exx: float, exz: float) -> tuple[int, int]:
    for c in range(gm.MAP_MODULES):
        for r in range(gm.MAP_MODULES):
            mcx, mcz = gm.module_center(c, r)
            if abs(mcx - exx) < 0.1 and abs(mcz - exz) < 0.1:
                return (c, r)
    raise ValueError(f"extraction_xz ({exx},{exz}) not on a module centre")


def check_map(path: Path) -> list[str]:
    data = json.loads(path.read_text(encoding="utf-8"))
    ex = data.get("extraction_xz")
    if not ex:
        return ["missing extraction_xz in map JSON"]
    exx, exz = float(ex[0]), float(ex[1])
    try:
        slot = end_slot_from_extraction(exx, exz)
    except ValueError as e:
        return [str(e)]

    cells_total = gm.MAP_MODULES * gm.CELLS
    mask = data["floors"]["0"]["cells"]
    cx = cz = gm.CENTER_TILE
    mix = slot[0] * gm.CELLS + cx
    miz = slot[1] * gm.CELLS + cz
    errors: list[str] = []

    off = [
        (mi, mj)
        for mj in range(cells_total)
        for mi in range(cells_total)
        if not mask[mj * cells_total + mi]
    ]
    if off != [(mix, miz)]:
        errors.append(f"floor mask holes {off}, expected only {(mix, miz)}")

    mcx, mcz = gm.module_center(*slot)
    hole_x = mcx + gm.cell_cx(cx)
    hole_z = mcz + gm.cell_cz(cz)

    hole_stems = {gm.EXTRACTION_HOLE_STEM, "template-floor-layer-hole"}
    has_hole = any(
        p.get("stem") in hole_stems
        and abs(float(p["x"]) - hole_x) < 0.1
        and abs(float(p["z"]) - hole_z) < 0.1
        and p.get("floor_level", 0) == 0
        for p in data.get("pieces", [])
    )
    if not has_hole:
        errors.append(
            f"missing {gm.EXTRACTION_HOLE_STEM} at centre tile ({hole_x},{hole_z})"
        )

    has_room = any(
        p.get("stem") in (gm.EXTRACTION_ROOM_STEM, "room-large-variation")
        and p.get("floor_level", 0) == 0
        and abs(float(p["x"]) - mcx) < 0.1
        and abs(float(p["z"]) - mcz) < 0.1
        for p in data.get("pieces", [])
    )
    if not has_room:
        errors.append(
            f"missing {gm.EXTRACTION_ROOM_STEM} at finish module centre ({mcx},{mcz})"
        )

    patch_floors = sum(
        1 for p in data.get("pieces", [])
        if p.get("stem") == "template-floor"
        and p.get("floor_level", 0) == 0
        and in_end_module(p, mcx, mcz)
    )
    if patch_floors:
        errors.append(
            f"finish module has {patch_floors} template-floor patch tiles (expected 0)"
        )

    modules = probe.load_map_modules(path)
    tris = probe.filter_pit_floor_tris(
        probe.build_module_tris(modules[slot]), 0.0, 0.0,
    )
    if probe.mesh_floor_at(0.0, 0.0, tris):
        errors.append("extraction centre has mesh floor after pit cutout (should be open)")

    for dix, diz in ((0, -1), (0, 1), (-1, 0), (1, 0)):
        ix, iz = cx + dix, cz + diz
        lx, lz = probe.module_cell_center(ix, iz)
        if not probe.mesh_floor_at(lx, lz, tris):
            errors.append(f"walk-up tile ({ix},{iz}) beside pit has no mesh floor")

    for iz in range(gm.CELLS):
        for ix in range(gm.CELLS):
            if ix == cx and iz == cz:
                continue
            lx, lz = probe.module_cell_center(ix, iz)
            if not probe.mesh_floor_at(lx, lz, tris):
                errors.append(f"end module tile ({ix},{iz}) missing mesh floor")

    hub_key = str(gm.HUB_FLOOR_LEVEL)
    hub_mask = data.get("floors", {}).get(hub_key, {}).get("cells")
    if hub_mask is None:
        errors.append(f"missing floors[{hub_key}] hub floor mask")
    else:
        for iz in range(gm.CELLS):
            for ix in range(gm.CELLS):
                mix = slot[0] * gm.CELLS + ix
                miz = slot[1] * gm.CELLS + iz
                if not hub_mask[miz * cells_total + mix]:
                    errors.append(f"hub floor mask missing tile ({ix},{iz})")

    has_hub_room = any(
        p.get("stem") == gm.HUB_ROOM_STEM
        and p.get("floor_level") == gm.HUB_FLOOR_LEVEL
        and abs(float(p["x"]) - mcx) < 0.1
        and abs(float(p["z"]) - mcz) < 0.1
        for p in data.get("pieces", [])
    )
    if not has_hub_room:
        errors.append(
            f"missing {gm.HUB_ROOM_STEM} at floor {gm.HUB_FLOOR_LEVEL} under extraction"
        )

    return errors


def check_runtime_layout(map_path: Path) -> list[str]:
    """Validate userinput/kenney_layout.json matches map extraction/hub."""
    if not LAYOUT_PATH.exists():
        return [f"missing runtime layout {LAYOUT_PATH}"]
    layout = json.loads(LAYOUT_PATH.read_text(encoding="utf-8"))
    map_data = json.loads(map_path.read_text(encoding="utf-8"))
    errors: list[str] = []

    ex_map = map_data.get("extraction_xz")
    ex_layout = layout.get("extraction_xz")
    if ex_map != ex_layout:
        errors.append(f"extraction_xz mismatch map={ex_map} layout={ex_layout}")

    hub_key = str(gm.HUB_FLOOR_LEVEL)
    if hub_key not in layout.get("floors", {}):
        errors.append(f"layout missing floors[{hub_key}]")

    if ex_layout:
        exx, exz = float(ex_layout[0]), float(ex_layout[1])
        slot = end_slot_from_extraction(exx, exz)
        mcx, mcz = gm.module_center(*slot)

        hub_rooms = [
            p for p in layout.get("pieces", [])
            if p.get("stem") == gm.HUB_ROOM_STEM
            and abs(float(p["x"]) - mcx) < 0.1
            and abs(float(p["z"]) - mcz) < 0.1
        ]
        if not hub_rooms:
            errors.append("layout missing hub room-large at extraction centre")
        elif not any(int(p.get("floor", 0)) == gm.HUB_FLOOR_LEVEL for p in hub_rooms):
            floors = [p.get("floor") for p in hub_rooms]
            errors.append(
                f"hub room-large has wrong floor {floors} (expected {gm.HUB_FLOOR_LEVEL})"
            )

        stretch_rooms = [
            p for p in layout.get("pieces", [])
            if p.get("stem") in (gm.EXTRACTION_ROOM_STEM, "room-large-variation")
            and abs(float(p["x"]) - mcx) < 0.1
            and abs(float(p["z"]) - mcz) < 0.1
        ]
        if not any(int(p.get("floor", 0)) == 0 for p in stretch_rooms):
            errors.append("layout missing floor-0 extraction room-large")

        hole_x = mcx + gm.cell_cx(gm.CENTER_TILE)
        hole_z = mcz + gm.cell_cz(gm.CENTER_TILE)
        if not any(
            p.get("stem") == gm.EXTRACTION_HOLE_STEM
            and int(p.get("floor", 0)) == 0
            and abs(float(p["x"]) - hole_x) < 0.1
            and abs(float(p["z"]) - hole_z) < 0.1
            for p in layout.get("pieces", [])
        ):
            errors.append("layout missing template-floor-hole on centre tile")

        # Simulate runtime pit cutout on floor-0 end module tris.
        by_gid: dict[int, list[dict]] = {}
        for p in layout.get("pieces", []):
            by_gid.setdefault(int(p.get("group_id", 0)), []).append(p)
        end_gid = None
        for gid, group in by_gid.items():
            if not group:
                continue
            ax = sum(float(p["x"]) for p in group) / len(group)
            az = sum(float(p["z"]) for p in group) / len(group)
            if gm.slot_from_world(ax, az) == slot:
                end_gid = gid
                break
        if end_gid is None:
            errors.append("could not find end module group in layout")
        else:
            local = [
                gm.pp(
                    p["stem"],
                    float(p["x"]) - mcx,
                    float(p["z"]) - mcz,
                    float(p.get("yaw", 0)),
                )
                for p in by_gid[end_gid]
            ]
            for i, p in enumerate(local):
                p.floor_level = int(by_gid[end_gid][i].get("floor", 0))
            tris = probe.filter_pit_floor_tris(
                probe.build_module_tris(local), 0.0, 0.0,
            )
            if probe.mesh_floor_at(0.0, 0.0, tris):
                errors.append(
                    "layout centre has mesh floor after pit cutout simulation"
                )

        # Hub floor-1 mesh should exist at module centre (y = HUB_FLOOR_LEVEL * MOD_H).
        hub_local = [
            gm.pp(gm.HUB_ROOM_STEM, 0.0, 0.0, 0.0),
        ]
        hub_local[0].floor_level = gm.HUB_FLOOR_LEVEL
        hub_tris = probe.build_module_tris(hub_local)
        hub_y = gm.HUB_FLOOR_LEVEL * probe.FLOOR_HEIGHT_M
        hub_band = (hub_y + probe.FLOOR_BAND[0], hub_y + probe.FLOOR_BAND[1])
        hub_hits = kcat.vertical_ray_hits(0.0, 0.0, hub_tris)
        if not any(kcat.in_band(h, hub_band) for h in hub_hits):
            errors.append("hub floor-1 has no walkable mesh at module centre")

    return errors


def main() -> None:
    path = Path(sys.argv[1] if len(sys.argv) > 1 else "userinput/maps/level_stretch.json")
    border = probe.probe_map(path, verbose=False)
    ext = check_map(path)
    layout_err = check_runtime_layout(path)
    print()
    print("=== extraction pit (map JSON) ===")
    if ext:
        print(f"FAIL ({len(ext)}):")
        for e in ext:
            print(f"  · {e}")
    else:
        print("PASS")

    print()
    print("=== runtime layout (kenney_layout.json) ===")
    if layout_err:
        print(f"FAIL ({len(layout_err)}):")
        for e in layout_err:
            print(f"  · {e}")
    else:
        print("PASS: hub at floor -1, hole on centre tile, pit mesh open")

    if border:
        print(f"\n=== border probe ({len(border)} note(s), connectivity only) ===")
        for e in border:
            print(f"  · {e}")
    sys.exit(1 if ext or layout_err else 0)


if __name__ == "__main__":
    main()
