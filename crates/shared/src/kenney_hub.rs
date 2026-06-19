//! Hub branch exits (L2 / L3 / L4) — physical volumes and group ids.

use std::collections::HashMap;

use bevy::prelude::Vec3;

use crate::kenney_layout::{BranchLevel, KenneyLayout, KenneyPlacement};
use crate::kenney_pit::{self, HUB_FLOOR_LEVEL, PIT_DROP_HALF, PIT_SHAFT_BOTTOM_Y};
use crate::level::MOD_H;

/// Hub branch module spacing (5 cells × 4 m).
pub const HUB_MODULE_SPAN: f32 = crate::kenney_pit::HUB_MODULE_SPAN;

pub const DEPTH_FLOOR_LEVEL: i32 = -2;

pub fn west_module_center(ex: f32, ez: f32) -> (f32, f32) {
    crate::kenney_pit::hub_west_module_center(ex, ez)
}

/// L2 stairs in the west module east antechamber — on floor -2, entry faces the hub door.
pub fn l2_stairs_placement(west_mcx: f32, west_mcz: f32) -> (f32, f32, f32, i32) {
    const PI2: f32 = std::f32::consts::FRAC_PI_2;
    (
        west_mcx + 6.0,
        west_mcz,
        PI2,
        DEPTH_FLOOR_LEVEL,
    )
}

/// World XZ of the hub-floor opening above the L2 stairs (walk down from floor -1).
pub fn hub_stairs_opening(ex: f32, ez: f32) -> bevy::prelude::Vec2 {
    crate::kenney_pit::hub_stairs_opening(ex, ez)
}

pub fn in_hub_stairs_opening(x: f32, z: f32, ex: f32, ez: f32) -> bool {
    crate::kenney_pit::in_hub_stairs_opening(x, z, ex, ez)
}

/// Rim position when the extraction shaft deposits players onto the hub floor.
pub fn hub_shaft_landing_xz(ex: f32, ez: f32) -> [f32; 2] {
    [ex + 3.5, ez]
}

/// `gen_maps.py` branch module group ids.
pub const BRANCH_GID_L2: u32 = 92;
pub const BRANCH_GID_L3: u32 = 93;
pub const BRANCH_GID_L4: u32 = 94;

pub fn branch_gid(exit: u8) -> Option<u32> {
    match exit {
        2 => Some(BRANCH_GID_L2),
        3 => Some(BRANCH_GID_L3),
        4 => Some(BRANCH_GID_L4),
        _ => None,
    }
}

pub fn all_branch_gids() -> [u32; 3] {
    [BRANCH_GID_L2, BRANCH_GID_L3, BRANCH_GID_L4]
}

/// Player capsule centre is inside a branch destination room.
pub fn in_branch_destination(pos: Vec3, branch: &BranchLevel) -> bool {
    let floor_y = branch.floor as f32 * MOD_H;
    let y_min = floor_y - 1.0;
    let y_max = floor_y + MOD_H - 0.5;
    pos.y >= y_min
        && pos.y <= y_max
        && (pos.x - branch.x).abs() < PIT_DROP_HALF
        && (pos.z - branch.z).abs() < PIT_DROP_HALF
}

/// Pit drop (L3): past the shaft floor while inside the open column.
pub fn in_pit_exit_commit(pos: Vec3, ex: f32, ez: f32) -> bool {
    pos.y < PIT_SHAFT_BOTTOM_Y && kenney_pit::in_extraction_drop_zone(pos.x, pos.z, ex, ez)
}

/// West corridor hole (L4): falling through the corridor drop above L4.
pub fn in_west_drop_commit(pos: Vec3, ex: f32, ez: f32) -> bool {
    let hole_x = ex - 8.0;
    let hole_z = ez;
    (pos.x - hole_x).abs() < PIT_DROP_HALF
        && (pos.z - hole_z).abs() < PIT_DROP_HALF
        && pos.y < 0.5
        && pos.y > -MOD_H * 2.5
}

/// Stairs antechamber (L2): east side of west module on floor -2.
pub fn in_stairs_exit_commit(pos: Vec3, l2_x: f32, l2_z: f32) -> bool {
    let sx = l2_x + 6.0;
    let sz = l2_z;
    let floor_y = DEPTH_FLOOR_LEVEL as f32 * MOD_H;
    (pos.x - sx).abs() < PIT_DROP_HALF
        && (pos.z - sz).abs() < PIT_DROP_HALF
        && pos.y >= floor_y - 1.0
        && pos.y <= floor_y + MOD_H - 0.5
}

pub fn detects_branch_commit(
    pos: Vec3,
    exit: u8,
    branch: &BranchLevel,
    extraction_xz: Option<[f32; 2]>,
) -> bool {
    if in_branch_destination(pos, branch) {
        return true;
    }
    let Some([ex, ez]) = extraction_xz else {
        return false;
    };
    match exit {
        3 => in_pit_exit_commit(pos, ex, ez),
        4 => in_west_drop_commit(pos, ex, ez),
        2 => in_stairs_exit_commit(pos, branch.x, branch.z),
        _ => false,
    }
}

fn has_room_shell(pieces: &[KenneyPlacement], x: f32, z: f32, floor: i32) -> bool {
    pieces.iter().any(|p| {
        kenney_pit::is_room_shell(&p.stem)
            && p.floor == floor
            && (p.x - x).abs() < 0.5
            && (p.z - z).abs() < 0.5
    })
}

/// Default hub branch anchors for a resolved extraction centre.
pub fn default_branch_levels(ex: f32, ez: f32) -> HashMap<String, BranchLevel> {
    let (wx, wz) = west_module_center(ex, ez);
    HashMap::from([
        (
            "2".into(),
            BranchLevel {
                x: wx + 6.0,
                z: wz,
                floor: HUB_FLOOR_LEVEL,
                label: "Stairs route".into(),
            },
        ),
        (
            "3".into(),
            BranchLevel {
                x: ex,
                z: ez,
                floor: DEPTH_FLOOR_LEVEL,
                label: "Pit drop".into(),
            },
        ),
        (
            "4".into(),
            BranchLevel {
                x: wx,
                z: wz,
                floor: DEPTH_FLOOR_LEVEL,
                label: "West drop".into(),
            },
        ),
    ])
}

/// Fix common hub-branch mistakes in saved editor layouts (stairs floor/yaw, missing L3 shell).
pub fn patch_hub_branch_layout(mut layout: KenneyLayout) -> KenneyLayout {
    layout = layout.resolve_for_playtest();
    let Some([ex, ez]) = layout.extraction_xz else {
        return layout;
    };

    let (west_mcx, west_mcz) = west_module_center(ex, ez);
    let (sx, sz, syaw, sfloor) = l2_stairs_placement(west_mcx, west_mcz);

    layout.pieces.retain(|p| {
        !(p.stem == "stairs" && p.floor == HUB_FLOOR_LEVEL && (p.x - sx).abs() < 1.0)
    });

    // Hub drops are mesh-cut room floors — hatch props render a misleading diagonal wedge.
    layout.pieces.retain(|p| {
        !(p.stem == "template-floor-hole"
            && p.floor == HUB_FLOOR_LEVEL
            && layout.extraction_xz.is_some_and(|[ex, ez]| {
                kenney_pit::hide_hub_floor_hatch_piece(&p.stem, p.floor, p.x, p.z, ex, ez)
            }))
    });
    layout.pieces.retain(|p| {
        !(matches!(
            p.stem.as_str(),
            "template-floor" | "template-floor-layer" | "template-floor-big"
        ) && p.floor == HUB_FLOOR_LEVEL
            && layout.extraction_xz.is_some_and(|[ex, ez]| {
                kenney_pit::in_hub_l3_drop_zone(p.x, p.z, ex, ez)
                    || kenney_pit::in_hub_west_drop_zone(p.x, p.z, ex, ez)
                    || kenney_pit::in_hub_stairs_opening(p.x, p.z, ex, ez)
            }))
    });

    if let Some(stairs) = layout.pieces.iter_mut().find(|p| p.stem == "stairs" && (p.x - sx).abs() < 1.0) {
        stairs.x = sx;
        stairs.z = sz;
        stairs.yaw = syaw;
        stairs.floor = sfloor;
        stairs.group_id = Some(BRANCH_GID_L2);
    } else {
        layout.pieces.push(KenneyPlacement {
            stem: "stairs".into(),
            x: sx,
            z: sz,
            yaw: syaw,
            floor: sfloor,
            scale: 1.0,
            group_id: Some(BRANCH_GID_L2),
        });
    }

    if !has_room_shell(&layout.pieces, ex, ez, DEPTH_FLOOR_LEVEL) {
        layout.pieces.push(KenneyPlacement {
            stem: "room-large".into(),
            x: ex,
            z: ez,
            yaw: 0.0,
            floor: DEPTH_FLOOR_LEVEL,
            scale: 1.0,
            group_id: Some(BRANCH_GID_L3),
        });
    }

    let west_hole_x = ex - 8.0;
    let stair_open = kenney_pit::hub_stairs_opening(ex, ez);

    let cell = layout.grid_unit_m;
    if let Some(mask) = layout.floors.get_mut(&0) {
        cut_floor_mask_at(mask, cell, ex, ez);
    }
    if let Some(mask) = layout.floors.get_mut(&HUB_FLOOR_LEVEL) {
        cut_floor_mask_at(mask, cell, stair_open.x, stair_open.y);
        cut_floor_mask_at(mask, cell, ex, ez);
        cut_floor_mask_at(mask, cell, west_hole_x, ez);
    }

    layout
}

fn cut_floor_mask_at(mask: &mut crate::editor_map::FloorMask, cell: f32, wx: f32, wz: f32) {
    let x0 = mask.world_x0();
    let z0 = mask.world_z0();
    let ix = ((wx - x0) / cell - 0.5).round().max(0.0) as u32;
    let iz = ((wz - z0) / cell - 0.5).round().max(0.0) as u32;
    mask.set(ix, iz, false);
}
