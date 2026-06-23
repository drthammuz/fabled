//! Hub branch exits (L2 / L3 / L4) — physical volumes and group ids.

use std::collections::HashMap;

use bevy::prelude::Vec3;

use crate::kenney_layout::{BranchLevel, KenneyLayout, KenneyPlacement};
use crate::kenney_pit::{self, HUB_FLOOR_LEVEL, PIT_DROP_HALF};
use crate::level::MOD_H;

/// Hub branch module spacing (5 cells × 4 m).
pub const HUB_MODULE_SPAN: f32 = crate::kenney_pit::HUB_MODULE_SPAN;

pub use crate::kenney_pit::DEPTH_FLOOR_LEVEL;

pub fn west_module_center(ex: f32, ez: f32) -> (f32, f32) {
    crate::kenney_pit::hub_west_module_center(ex, ez)
}

/// L2 stairs in the west module east antechamber — on floor -2, entry faces the hub door.
/// Aligned to the same 4 m cell as `hub_stairs_opening` so the hole sits over the stairs.
pub fn l2_stairs_placement(west_mcx: f32, west_mcz: f32) -> (f32, f32, f32, i32) {
    const PI2: f32 = std::f32::consts::FRAC_PI_2;
    (
        west_mcx + 4.0,
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

/// Pit drop (L3): falling through the dedicated hub-floor L3 opening (not the landing tile).
pub fn in_pit_exit_commit(pos: Vec3, ex: f32, ez: f32) -> bool {
    let c = kenney_pit::hub_l3_drop_centre(ex, ez);
    (pos.x - c.x).abs() < PIT_DROP_HALF
        && (pos.z - c.y).abs() < PIT_DROP_HALF
        && pos.y < kenney_pit::pit_floor_plane_y(HUB_FLOOR_LEVEL) - 0.5
}

/// West corridor hole (L4): falling through the corridor drop above L4.
pub fn in_west_drop_commit(pos: Vec3, ex: f32, ez: f32) -> bool {
    let hole = kenney_pit::hub_west_drop_centre(ex, ez);
    (pos.x - hole.x).abs() < PIT_DROP_HALF
        && (pos.z - hole.y).abs() < PIT_DROP_HALF
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

/// True when any floor-bearing piece (room shell, corridor, or a single-cell
/// template-floor tile) covers `(x, z)` on `floor` — i.e. there is already a walkable
/// surface there and no fallback shell is needed.
fn has_floor_coverage(pieces: &[KenneyPlacement], x: f32, z: f32, floor: i32) -> bool {
    pieces.iter().any(|p| {
        p.floor == floor
            && kenney_pit::carves_floor(&p.stem)
            && (p.x - x).abs() < 2.0
            && (p.z - z).abs() < 2.0
    })
}

/// Default hub branch anchors for a resolved extraction centre.
pub fn default_branch_levels(ex: f32, ez: f32) -> HashMap<String, BranchLevel> {
    let (wx, wz) = west_module_center(ex, ez);
    HashMap::from([
        (
            "2".into(),
            BranchLevel {
                x: wx + 4.0,
                z: wz,
                floor: HUB_FLOOR_LEVEL,
                label: "Stairs route".into(),
            },
        ),
        (
            "3".into(),
            BranchLevel {
                x: kenney_pit::hub_l3_drop_centre(ex, ez).x,
                z: kenney_pit::hub_l3_drop_centre(ex, ez).y,
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

/// True for maps from `gen_freeform.py` (two exits keyed `"0"`/`"1"`, no L2/L3/L4 branches).
pub fn is_freeform_hub_layout(layout: &KenneyLayout) -> bool {
    if layout.hub_model.as_deref() == Some("freeform_v1") {
        return true;
    }
    layout.hub_exits.contains_key("0")
        && layout.hub_exits.contains_key("1")
        && !layout.hub_exits.contains_key("2")
        && layout.branch_levels.is_empty()
}

/// Fix common hub-branch mistakes in saved editor layouts (stairs floor/yaw, missing L3 shell).
pub fn patch_hub_branch_layout(mut layout: KenneyLayout) -> KenneyLayout {
    layout = layout.resolve_for_playtest();
    if is_freeform_hub_layout(&layout) {
        // Do not inject west stairs, L3/L4 mask holes, or room-large shells — the
        // free-form generator authors hub holes and landings explicitly.
        return layout;
    }
    let Some([ex, ez]) = layout.extraction_xz else {
        return layout;
    };

    let (west_mcx, west_mcz) = west_module_center(ex, ez);
    let (sx, sz, syaw, sfloor) = l2_stairs_placement(west_mcx, west_mcz);

    layout.pieces.retain(|p| {
        !(p.stem == "stairs" && p.floor == HUB_FLOOR_LEVEL && (p.x - sx).abs() < 1.0)
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
            ceiling: false,
            underside: false,
            kit: None,
            tint: None,
            tags: vec![],
            zone: None,
            y: None,
        });
    }

    // Only add a fallback shell if L3 has NO floor coverage at all. When L3 is a tiled
    // room (template-floor at the centre), adding a room-large here would be an invisible
    // double collider overlapping the tiles.
    if !has_floor_coverage(&layout.pieces, ex, ez, DEPTH_FLOOR_LEVEL) {
        layout.pieces.push(KenneyPlacement {
            stem: "room-large".into(),
            x: ex,
            z: ez,
            yaw: 0.0,
            floor: DEPTH_FLOOR_LEVEL,
            scale: 1.0,
            group_id: Some(BRANCH_GID_L3),
            ceiling: false,
            underside: false,
            kit: None,
            tint: None,
            tags: vec![],
            zone: None,
            y: None,
        });
    }

    // --- Floor mask is the single source of truth for holes (visual + physics). ---
    let west_hole = kenney_pit::hub_west_drop_centre(ex, ez);
    let stair_cells = kenney_pit::hub_stairs_opening_cells(ex, ez);
    let l3 = kenney_pit::hub_l3_drop_centre(ex, ez);

    let cell = layout.grid_unit_m;
    let cells_x = layout.modules_x * crate::editor_map::CELLS_PER_MODULE;
    let cells_z = layout.modules_z * crate::editor_map::CELLS_PER_MODULE;

    // Sub-floor masks aren't authored in the editor; derive them from floor-piece coverage
    // so the mask is a faithful single source for both visuals and physics. Floor 0 is the
    // solid ground level (filled), holes are cut afterwards.
    layout
        .floors
        .entry(0)
        .or_insert_with(|| crate::editor_map::FloorMask::filled(cells_x, cells_z));
    for lvl in [HUB_FLOOR_LEVEL, DEPTH_FLOOR_LEVEL] {
        let mask = subfloor_mask_from_coverage(&layout, lvl, cells_x, cells_z, cell);
        layout.floors.insert(lvl, mask);
    }

    // Floor 0: the extraction trap. You drop through here onto the hub floor.
    if let Some(mask) = layout.floors.get_mut(&0) {
        cut_floor_mask_at(mask, cell, ex, ez);
    }
    // Hub floor (-1): three SEPARATE openings; the (ex,ez) landing tile stays SOLID.
    if let Some(mask) = layout.floors.get_mut(&HUB_FLOOR_LEVEL) {
        // Force the landing solid first, overriding any stale hole from older exports.
        set_floor_mask_at(mask, cell, ex, ez, true);
        for s in stair_cells {
            cut_floor_mask_at(mask, cell, s.x, s.y); // L2 stairs (2 cells along the run)
        }
        cut_floor_mask_at(mask, cell, west_hole.x, west_hole.y); // L4 west drop
        cut_floor_mask_at(mask, cell, l3.x, l3.y); // L3 pit drop
    }
    // Depth floor (-2): omit floor under the stair footprint (stair mesh is the surface).
    if let Some(mask) = layout.floors.get_mut(&DEPTH_FLOOR_LEVEL) {
        for s in stair_cells {
            cut_floor_mask_at(mask, cell, s.x, s.y);
        }
    }

    // Remove any floor prop that now sits over a hole (legacy hatch wedges included).
    let floors_snapshot = layout.floors.clone();
    layout.pieces.retain(|p| {
        !kenney_pit::floor_prop_on_hole(&p.stem, p.x, p.z, floors_snapshot.get(&p.floor), p.ceiling)
    });

    layout
}

/// Build a floor mask for a sub-ground level by marking every cell covered by a
/// floor-bearing piece (room shells, corridors, explicit floor tiles). This makes the
/// mask the single source of truth even though sub-floors are not authored in the editor.
fn subfloor_mask_from_coverage(
    layout: &KenneyLayout,
    floor: i32,
    cells_x: u32,
    cells_z: u32,
    cell: f32,
) -> crate::editor_map::FloorMask {
    use crate::kenney_catalog::{self, quantize_yaw};
    let mut mask = crate::editor_map::FloorMask {
        cells_x,
        cells_z,
        cells: vec![false; (cells_x * cells_z) as usize],
    };
    let x0 = mask.world_x0();
    let z0 = mask.world_z0();
    for p in &layout.pieces {
        if p.floor != floor || !kenney_pit::carves_floor(&p.stem) {
            continue;
        }
        let (nx, nz) = kenney_catalog::piece_grid_size(&p.stem);
        let (wx, wz) = kenney_catalog::rotated_grid_size(nx, nz, quantize_yaw(p.yaw));
        let sw_x = p.x - wx * cell * 0.5;
        let sw_z = p.z - wz * cell * 0.5;
        for j in 0..wz.round() as i32 {
            for i in 0..wx.round() as i32 {
                let cx = sw_x + (i as f32 + 0.5) * cell;
                let cz = sw_z + (j as f32 + 0.5) * cell;
                let ix = ((cx - x0) / cell).floor();
                let iz = ((cz - z0) / cell).floor();
                if ix >= 0.0 && iz >= 0.0 {
                    mask.set(ix as u32, iz as u32, true);
                }
            }
        }
    }
    mask
}

fn set_floor_mask_at(mask: &mut crate::editor_map::FloorMask, cell: f32, wx: f32, wz: f32, on: bool) {
    let x0 = mask.world_x0();
    let z0 = mask.world_z0();
    let ix = ((wx - x0) / cell - 0.5).round().max(0.0) as u32;
    let iz = ((wz - z0) / cell - 0.5).round().max(0.0) as u32;
    mask.set(ix, iz, on);
}

fn cut_floor_mask_at(mask: &mut crate::editor_map::FloorMask, cell: f32, wx: f32, wz: f32) {
    set_floor_mask_at(mask, cell, wx, wz, false);
}
