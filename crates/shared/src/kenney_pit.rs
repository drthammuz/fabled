//! Extraction pit: cut room shell GLBs for open pits, shaft, and hub exits.

use bevy::prelude::{Vec2, Vec3};

use crate::editor_map::FloorMask;
use crate::kenney_catalog::{self, quantize_yaw};
use crate::kenney_layout::KenneyLayout;
use crate::level::MOD_H;

/// Half-extent of one grid cell in world XZ for floor-hole cutouts (exact tile, no bleed).
pub fn floor_tile_half() -> f32 {
    kenney_catalog::KENNEY_CELL * 0.5 - 0.02
}

/// Half-extent of the 4 m centre trap tile in world XZ (collision rim cutout).
pub const PIT_CELL_HALF: f32 = 2.05;
/// Inner drop opening half-extent in world XZ (~3 m hole).
pub const PIT_DROP_HALF: f32 = 1.25;
/// Room floor slabs sit this far above the floor plane (world Y offset).
pub const PIT_FLOOR_BAND: f32 = 0.5;
/// Hub branch level (one MOD_H below stretch).
pub const HUB_FLOOR_LEVEL: i32 = -1;
pub const DEPTH_FLOOR_LEVEL: i32 = -2;
/// Below this Y the extraction shaft ends and hub-floor grounding resumes.
pub const PIT_SHAFT_BOTTOM_Y: f32 = HUB_FLOOR_LEVEL as f32 * MOD_H - 0.35;
/// West module link opening half-width on Z.
pub const WEST_OPENING_HALF_Z: f32 = 2.05;
/// Thickness of west wall triangles to remove around the link opening.
pub const WEST_WALL_THICK: f32 = 1.6;

#[derive(Clone, Debug, Default)]
pub struct KenneyMeshCutouts {
    pub floor: i32,
    /// Centres (world XZ) of grid cells under this piece whose floor mask is a hole.
    /// Single source of truth: derived from `FloorMask`, never from hub geometry.
    pub floor_holes: Vec<Vec2>,
    pub pit_shaft: Option<Vec2>,
    /// Room centre XZ for a west-face link opening (hub → corridor).
    pub west_opening: Option<Vec2>,
    /// Room centre XZ for an east-face link opening (L2 module ← corridor).
    pub east_opening: Option<Vec2>,
}

impl KenneyMeshCutouts {
    pub fn is_empty(&self) -> bool {
        self.floor_holes.is_empty()
            && self.pit_shaft.is_none()
            && self.west_opening.is_none()
            && self.east_opening.is_none()
    }

    pub fn floor_holes(&self) -> impl Iterator<Item = Vec2> + '_ {
        self.floor_holes.iter().copied()
    }

    /// Shift every hole/opening centre by (dx,dz) — used to lift local-frame cutouts to world.
    pub fn translated(mut self, dx: f32, dz: f32) -> Self {
        let shift = Vec2::new(dx, dz);
        for h in &mut self.floor_holes {
            *h += shift;
        }
        for opt in [&mut self.pit_shaft, &mut self.west_opening, &mut self.east_opening] {
            if let Some(v) = opt {
                *v += shift;
            }
        }
        self
    }
}

pub fn pit_floor_plane_y(floor: i32) -> f32 {
    floor as f32 * MOD_H
}

pub fn pit_floor_top_y(floor: i32) -> f32 {
    pit_floor_plane_y(floor) + PIT_FLOOR_BAND
}

pub fn is_room_shell(stem: &str) -> bool {
    matches!(
        stem,
        "room-large" | "room-large-variation" | "room-wide" | "room-wide-variation"
    )
}

pub fn piece_overlaps_tile(px: f32, pz: f32, stem: &str, yaw: f32, tx: f32, tz: f32) -> bool {
    let (nx, nz) = kenney_catalog::piece_grid_size(stem);
    let (wx, wz) = kenney_catalog::rotated_grid_size(nx, nz, quantize_yaw(yaw));
    let half_w = wx * kenney_catalog::KENNEY_CELL * 0.5;
    let half_d = wz * kenney_catalog::KENNEY_CELL * 0.5;
    let tile = kenney_catalog::KENNEY_CELL * 0.5;
    let lx = px - tx;
    let lz = pz - tz;
    (lx - half_w) < tile && (lx + half_w) > -tile && (lz - half_d) < tile && (lz + half_d) > -tile
}

pub fn piece_overlaps_pit_tile(px: f32, pz: f32, stem: &str, yaw: f32, ex: f32, ez: f32) -> bool {
    piece_overlaps_tile(px, pz, stem, yaw, ex, ez)
}

/// Pieces that carry a baked walkable floor we must carve holes out of.
pub fn carves_floor(stem: &str) -> bool {
    is_room_shell(stem)
        || stem.starts_with("corridor")
        || (stem.starts_with("template-floor") && !stem.contains("hole"))
}

/// True when the floor mask reports a solid floor at world XZ (out of bounds = solid).
pub fn mask_has_floor_world(mask: &FloorMask, x: f32, z: f32) -> bool {
    let cell = kenney_catalog::KENNEY_CELL;
    let ix = ((x - mask.world_x0()) / cell).floor();
    let iz = ((z - mask.world_z0()) / cell).floor();
    if ix < 0.0 || iz < 0.0 {
        return true;
    }
    mask.get(ix as u32, iz as u32)
}

/// World XZ centres of every cell under a piece footprint whose mask cell is a hole.
fn footprint_hole_cells(stem: &str, px: f32, pz: f32, yaw: f32, mask: &FloorMask) -> Vec<Vec2> {
    let (nx, nz) = kenney_catalog::piece_grid_size(stem);
    let (wx, wz) = kenney_catalog::rotated_grid_size(nx, nz, quantize_yaw(yaw));
    let cell = kenney_catalog::KENNEY_CELL;
    let sw_x = px - wx * cell * 0.5;
    let sw_z = pz - wz * cell * 0.5;
    let mut out = Vec::new();
    for j in 0..wz.round() as u32 {
        for i in 0..wx.round() as u32 {
            let cx = sw_x + (i as f32 + 0.5) * cell;
            let cz = sw_z + (j as f32 + 0.5) * cell;
            if !mask_has_floor_world(mask, cx, cz) {
                out.push(Vec2::new(cx, cz));
            }
        }
    }
    out
}

/// All mesh surgery needed for a placed Kenney piece.
///
/// Floor holes come **only** from `mask` (single source of truth shared with physics).
/// `ex`/`ez` drive the floor-0 extraction shaft clear and the hub door wall openings only.
/// Ceiling slabs (`ceiling == true`) skip mask hole carving — they intentionally sit over void.
pub fn mesh_cutouts_for_piece(
    stem: &str,
    floor: i32,
    px: f32,
    pz: f32,
    yaw: f32,
    extraction: Option<Vec2>,
    mask: Option<&FloorMask>,
    ceiling: bool,
) -> KenneyMeshCutouts {
    let mut cutouts = KenneyMeshCutouts {
        floor,
        ..Default::default()
    };

    // Floor holes: mask-driven, for any floor-bearing piece (works without a hub).
    if carves_floor(stem) && !ceiling {
        if let Some(mask) = mask {
            cutouts.floor_holes = footprint_hole_cells(stem, px, pz, yaw, mask);
        }
    }

    if !is_room_shell(stem) {
        return cutouts;
    }
    let Some(ext) = extraction else {
        return cutouts;
    };
    let (ex, ez) = (ext.x, ext.y);

    // Floor-0 extraction shaft: clear the room-interior tris under the open drop tile so
    // the faller drops cleanly to the hub. Only when (ex,ez) is actually a hole in the mask.
    let drop_is_hole = mask.is_some_and(|m| !mask_has_floor_world(m, ex, ez));
    if floor == 0 && drop_is_hole && piece_overlaps_pit_tile(px, pz, stem, yaw, ex, ez) {
        cutouts.pit_shaft = Some(Vec2::new(ex, ez));
    }

    // Hub door wall openings (walls, not floor — kept geometric).
    if floor == HUB_FLOOR_LEVEL {
        if (px - ex).abs() < 0.5 && (pz - ez).abs() < 0.5 {
            cutouts.west_opening = Some(Vec2::new(px, pz));
        } else {
            cutouts.east_opening = Some(Vec2::new(px, pz));
        }
    }
    if floor == -2 && (px - ex).abs() > 5.0 {
        cutouts.east_opening = Some(Vec2::new(px, pz));
    }

    cutouts
}

/// True when a floor slab must render its underside (walkable floor one level below).
pub fn floor_slab_needs_underside(
    stem: &str,
    floor: i32,
    x: f32,
    z: f32,
    ceiling: bool,
    floors: &std::collections::HashMap<i32, FloorMask>,
) -> bool {
    if ceiling {
        return true;
    }
    if !carves_floor(stem) {
        return false;
    }
    floors
        .get(&(floor - 1))
        .is_some_and(|m| mask_has_floor_world(m, x, z))
}

pub fn floor_cutout_centers(
    stem: &str,
    floor: i32,
    px: f32,
    pz: f32,
    yaw: f32,
    extraction: Option<Vec2>,
    mask: Option<&FloorMask>,
) -> Vec<Vec2> {
    mesh_cutouts_for_piece(stem, floor, px, pz, yaw, extraction, mask, false)
        .floor_holes()
        .collect()
}

pub fn needs_pit_floor_cutout(
    stem: &str,
    floor: i32,
    px: f32,
    pz: f32,
    yaw: f32,
    extraction: Option<Vec2>,
    mask: Option<&FloorMask>,
) -> bool {
    !floor_cutout_centers(stem, floor, px, pz, yaw, extraction, mask).is_empty()
}

fn tri_centroid(v0: Vec3, v1: Vec3, v2: Vec3) -> Vec3 {
    (v0 + v1 + v2) / 3.0
}

fn tri_max_y(v0: Vec3, v1: Vec3, v2: Vec3) -> f32 {
    v0.y.max(v1.y).max(v2.y)
}

fn tri_min_y(v0: Vec3, v1: Vec3, v2: Vec3) -> f32 {
    v0.y.min(v1.y).min(v2.y)
}

/// Remove floor triangles whose centroid lies in an open grid cell.
fn triangle_is_open_hole_floor(
    v0: Vec3,
    v1: Vec3,
    v2: Vec3,
    tx: f32,
    tz: f32,
    floor_plane_y: f32,
) -> bool {
    let c = tri_centroid(v0, v1, v2);
    let half = floor_tile_half();
    tri_max_y(v0, v1, v2) < floor_plane_y + 1.35
        && (c.x - tx).abs() <= half
        && (c.z - tz).abs() <= half
}

pub fn filter_open_hole_floor_indices(
    world_verts: &[Vec3],
    indices: &[[u32; 3]],
    tx: f32,
    tz: f32,
    floor_plane_y: f32,
) -> Vec<[u32; 3]> {
    indices
        .iter()
        .filter(|tri| {
            let v0 = world_verts[tri[0] as usize];
            let v1 = world_verts[tri[1] as usize];
            let v2 = world_verts[tri[2] as usize];
            !triangle_is_open_hole_floor(v0, v1, v2, tx, tz, floor_plane_y)
        })
        .copied()
        .collect()
}

/// Legacy rim cutout (floor 0 collision).
pub fn filter_pit_floor_indices(
    world_verts: &[Vec3],
    indices: &[[u32; 3]],
    tx: f32,
    tz: f32,
    floor_top_y: f32,
) -> Vec<[u32; 3]> {
    indices
        .iter()
        .filter(|tri| {
            let v0 = world_verts[tri[0] as usize];
            let v1 = world_verts[tri[1] as usize];
            let v2 = world_verts[tri[2] as usize];
            let in_xz = |v: Vec3| {
                (v.x - tx).abs() <= PIT_CELL_HALF && (v.z - tz).abs() <= PIT_CELL_HALF && v.y < floor_top_y
            };
            !(in_xz(v0) && in_xz(v1) && in_xz(v2))
        })
        .copied()
        .collect()
}

pub fn apply_floor_cutouts(
    world_verts: &[Vec3],
    indices: &[[u32; 3]],
    cutouts: &[Vec2],
    floor: i32,
) -> Vec<[u32; 3]> {
    let plane_y = pit_floor_plane_y(floor);
    let mut out = indices.to_vec();
    for centre in cutouts {
        out = filter_open_hole_floor_indices(world_verts, &out, centre.x, centre.y, plane_y);
    }
    out
}

fn triangle_blocks_shaft(v0: Vec3, v1: Vec3, v2: Vec3, ex: f32, ez: f32) -> bool {
    let c = tri_centroid(v0, v1, v2);
    let half = floor_tile_half();
    tri_min_y(v0, v1, v2) > PIT_SHAFT_BOTTOM_Y
        && (c.x - ex).abs() < half
        && (c.z - ez).abs() < half
}

pub fn filter_pit_shaft_indices(
    world_verts: &[Vec3],
    indices: &[[u32; 3]],
    ex: f32,
    ez: f32,
) -> Vec<[u32; 3]> {
    indices
        .iter()
        .filter(|tri| {
            let v0 = world_verts[tri[0] as usize];
            let v1 = world_verts[tri[1] as usize];
            let v2 = world_verts[tri[2] as usize];
            !triangle_blocks_shaft(v0, v1, v2, ex, ez)
        })
        .copied()
        .collect()
}

fn triangle_is_west_opening(
    v0: Vec3,
    v1: Vec3,
    v2: Vec3,
    room_cx: f32,
    room_cz: f32,
    floor_plane_y: f32,
) -> bool {
    let wall_x = room_cx - 10.0;
    let c = tri_centroid(v0, v1, v2);
    // Wall band only — do not carve floor triangles at the door threshold.
    c.y > floor_plane_y + PIT_FLOOR_BAND + 0.35
        && c.x > wall_x - WEST_WALL_THICK
        && c.x < wall_x + WEST_WALL_THICK
        && (c.z - room_cz).abs() < WEST_OPENING_HALF_Z
        && c.y < floor_plane_y + MOD_H
}

fn triangle_is_east_opening(
    v0: Vec3,
    v1: Vec3,
    v2: Vec3,
    room_cx: f32,
    room_cz: f32,
    floor_plane_y: f32,
) -> bool {
    let wall_x = room_cx + 10.0;
    let c = tri_centroid(v0, v1, v2);
    c.y > floor_plane_y + PIT_FLOOR_BAND + 0.35
        && c.x > wall_x - WEST_WALL_THICK
        && c.x < wall_x + WEST_WALL_THICK
        && (c.z - room_cz).abs() < WEST_OPENING_HALF_Z
        && c.y < floor_plane_y + MOD_H
}

pub fn filter_east_opening_indices(
    world_verts: &[Vec3],
    indices: &[[u32; 3]],
    room_cx: f32,
    room_cz: f32,
    floor_plane_y: f32,
) -> Vec<[u32; 3]> {
    indices
        .iter()
        .filter(|tri| {
            let v0 = world_verts[tri[0] as usize];
            let v1 = world_verts[tri[1] as usize];
            let v2 = world_verts[tri[2] as usize];
            !triangle_is_east_opening(v0, v1, v2, room_cx, room_cz, floor_plane_y)
        })
        .copied()
        .collect()
}

pub fn filter_west_opening_indices(
    world_verts: &[Vec3],
    indices: &[[u32; 3]],
    room_cx: f32,
    room_cz: f32,
    floor_plane_y: f32,
) -> Vec<[u32; 3]> {
    indices
        .iter()
        .filter(|tri| {
            let v0 = world_verts[tri[0] as usize];
            let v1 = world_verts[tri[1] as usize];
            let v2 = world_verts[tri[2] as usize];
            !triangle_is_west_opening(v0, v1, v2, room_cx, room_cz, floor_plane_y)
        })
        .copied()
        .collect()
}

/// Apply every cutout for a room shell mesh (visual + collision).
pub fn apply_mesh_cutouts(
    world_verts: &[Vec3],
    indices: &[[u32; 3]],
    cutouts: &KenneyMeshCutouts,
) -> Vec<[u32; 3]> {
    if cutouts.is_empty() {
        return indices.to_vec();
    }
    let plane_y = pit_floor_plane_y(cutouts.floor);
    let mut out = indices.to_vec();
    for hole in cutouts.floor_holes() {
        out = filter_open_hole_floor_indices(world_verts, &out, hole.x, hole.y, plane_y);
    }
    if let Some(shaft) = cutouts.pit_shaft {
        out = filter_pit_shaft_indices(world_verts, &out, shaft.x, shaft.y);
    }
    if let Some(room) = cutouts.west_opening {
        out = filter_west_opening_indices(world_verts, &out, room.x, room.y, plane_y);
    }
    if let Some(room) = cutouts.east_opening {
        out = filter_east_opening_indices(world_verts, &out, room.x, room.y, plane_y);
    }
    out
}

/// Open vertical drop above the hub — no floor snap / stick while inside this column.
pub fn in_extraction_shaft(x: f32, y: f32, z: f32, ex: f32, ez: f32) -> bool {
    if y < PIT_SHAFT_BOTTOM_Y {
        return false;
    }
    in_extraction_drop_zone(x, z, ex, ez)
}

/// West module centre one span west of extraction.
pub fn hub_west_module_center(ex: f32, ez: f32) -> (f32, f32) {
    (ex - HUB_MODULE_SPAN, ez)
}

/// World XZ of the hub-floor opening above the L2 stairs (the stair-top cell).
/// Offset is a whole number of cells from the west-module centre so the opening lands
/// exactly on a 4 m grid cell (mask cut and mesh carve must agree).
pub fn hub_stairs_opening(ex: f32, ez: f32) -> Vec2 {
    let (wx, wz) = hub_west_module_center(ex, ez);
    Vec2::new(wx + 4.0, wz)
}

/// Both hub-floor cells above the L2 stairs. Derived from [`crate::kenney_transitions::hub_l2_stair_up`].
pub fn hub_stairs_opening_cells(ex: f32, ez: f32) -> [Vec2; 2] {
    let (wx, wz) = hub_west_module_center(ex, ez);
    crate::kenney_transitions::hub_l2_stair_up().footprint_world(wx, wz)
}

pub fn in_hub_stairs_opening(x: f32, z: f32, ex: f32, ez: f32) -> bool {
    hub_stairs_opening_cells(ex, ez)
        .iter()
        .any(|s| in_hub_drop_column(x, z, s.x, s.y))
}

/// Hub branch module spacing (5 cells × 4 m).
pub const HUB_MODULE_SPAN: f32 = 20.0;

pub fn in_extraction_drop_zone(x: f32, z: f32, ex: f32, ez: f32) -> bool {
    (x - ex).abs() < PIT_DROP_HALF && (z - ez).abs() < PIT_DROP_HALF
}

/// West drop hole on the hub band (floor -1). Sits in the SW corner (ex-8, ez+8) so it
/// is off the centre row the player walks from the landing to the west gate / stairs.
pub fn hub_west_drop_centre(ex: f32, ez: f32) -> Vec2 {
    Vec2::new(ex - 8.0, ez + 8.0)
}

pub fn in_hub_west_drop_zone(x: f32, z: f32, ex: f32, ez: f32) -> bool {
    let c = hub_west_drop_centre(ex, ez);
    in_hub_drop_column(x, z, c.x, c.y)
}

/// L3 pit drop (floor -2 centre) — only below the hub walking surface.
/// Full-tile open column on the hub walking band (floor -1 drops).
pub fn in_hub_drop_column(x: f32, z: f32, tx: f32, tz: f32) -> bool {
    let half = floor_tile_half();
    (x - tx).abs() <= half && (z - tz).abs() <= half
}

/// L3 pit drop opening on the hub floor — a separate tile north of the landing,
/// so dropping through the floor-0 trap lands on solid hub floor at (ex,ez).
pub fn hub_l3_drop_centre(ex: f32, ez: f32) -> Vec2 {
    Vec2::new(ex, ez - 8.0)
}

pub fn in_hub_l3_drop_zone(x: f32, z: f32, ex: f32, ez: f32) -> bool {
    let c = hub_l3_drop_centre(ex, ez);
    in_hub_drop_column(x, z, c.x, c.y)
}

/// @deprecated use `in_hub_west_drop_zone` / `in_hub_l3_drop_zone`
pub fn hub_drop_centres(ex: f32, ez: f32) -> [Vec2; 2] {
    [Vec2::new(ex, ez), hub_west_drop_centre(ex, ez)]
}

pub fn in_hub_drop_zone(x: f32, z: f32, ex: f32, ez: f32) -> bool {
    in_hub_west_drop_zone(x, z, ex, ez) || in_hub_l3_drop_zone(x, z, ex, ez)
}

/// West module link opening on a room shell (hub ↔ west corridor).
pub fn in_hub_west_link(x: f32, z: f32, room_cx: f32, room_cz: f32) -> bool {
    let wall_x = room_cx - 10.0;
    (x - wall_x).abs() < WEST_WALL_THICK && (z - room_cz).abs() < WEST_OPENING_HALF_Z
}

pub fn in_hub_east_link(x: f32, z: f32, room_cx: f32, room_cz: f32) -> bool {
    let wall_x = room_cx + 10.0;
    (x - wall_x).abs() < WEST_WALL_THICK && (z - room_cz).abs() < WEST_OPENING_HALF_Z
}

/// Skip solid colliders on decorative door frames / link walls that should stay open,
/// and on any floor prop that sits over a mask hole.
pub fn skip_hub_passage_collider(
    stem: &str,
    floor: i32,
    px: f32,
    pz: f32,
    ex: f32,
    ez: f32,
    mask: Option<&FloorMask>,
) -> bool {
    // Floor props over a hole carry no collider (any floor, mask-driven).
    if floor_prop_on_hole(stem, px, pz, mask, false) {
        return true;
    }
    if floor != HUB_FLOOR_LEVEL {
        return false;
    }
    if matches!(stem, "gate" | "gate-opening" | "gate-lasers") {
        return true;
    }
    if stem.starts_with("template-wall")
        && (in_hub_west_link(px, pz, ex, ez) || in_hub_east_link(px, pz, ex - 20.0, ez))
    {
        return true;
    }
    false
}

/// Floor level whose walking surface is nearest a given world Y.
pub fn floor_level_at_y(y: f32) -> i32 {
    (y / MOD_H).round() as i32
}

/// True when the cell under (x,z) at the player's current floor band is a hole.
/// Single rule that replaces every hub-specific grounding-suppression zone.
pub fn over_open_hole(layout: &KenneyLayout, x: f32, y: f32, z: f32) -> bool {
    let level = floor_level_at_y(y);
    let Some(mask) = layout.floors.get(&level) else {
        return false;
    };
    !mask_has_floor_world(mask, x, z)
}

/// A floor prop placed over a mask hole (its visual + collider must be suppressed).
pub fn floor_prop_on_hole(
    stem: &str,
    px: f32,
    pz: f32,
    mask: Option<&FloorMask>,
    ceiling: bool,
) -> bool {
    if ceiling {
        return false;
    }
    if !stem.starts_with("template-floor") {
        return false;
    }
    // `template-floor-hole` is the intentional hole *frame* (a raised rim with an open
    // centre): keep it rendered. Its collider is skipped separately (floor < 0 / the
    // extraction tile in `kenney_skip_piece_collider`), so the hole stays physically open.
    if stem.contains("hole") {
        return false;
    }
    // Only suppress a *solid* floor tile that happens to sit over a mask hole.
    mask.is_some_and(|m| !mask_has_floor_world(m, px, pz))
}

/// Hide decorative hatch / patch floor tiles that sit over a mask hole.
pub fn hide_extraction_hatch_piece(
    stem: &str,
    _floor: i32,
    px: f32,
    pz: f32,
    mask: Option<&FloorMask>,
    ceiling: bool,
) -> bool {
    floor_prop_on_hole(stem, px, pz, mask, ceiling)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Vec2;

    const EX: f32 = 20.0;
    const EZ: f32 = 20.0;

    /// 15×15 cell map (3 modules) mask with a single hole punched at world (hx,hz).
    fn mask_with_hole(hx: f32, hz: f32) -> FloorMask {
        let mut mask = FloorMask::filled(15, 15);
        let cell = kenney_catalog::KENNEY_CELL;
        let ix = ((hx - mask.world_x0()) / cell).floor() as u32;
        let iz = ((hz - mask.world_z0()) / cell).floor() as u32;
        mask.set(ix, iz, false);
        mask
    }

    #[test]
    fn ceiling_slabs_skip_mask_hole_carving() {
        let mask = mask_with_hole(0.0, 0.0);
        let cut = mesh_cutouts_for_piece(
            "template-floor",
            0,
            0.0,
            0.0,
            0.0,
            None,
            Some(&mask),
            true,
        );
        assert!(cut.floor_holes.is_empty());
    }

    #[test]
    fn floor_holes_come_from_mask_only() {
        let ext = Some(Vec2::new(EX, EZ));
        // No mask -> no holes, even on the hub floor.
        let cut = mesh_cutouts_for_piece("room-large", HUB_FLOOR_LEVEL, EX, EZ, 0.0, ext, None, false);
        assert!(cut.floor_holes.is_empty());

        // A masked hole under the room footprint produces exactly one carved cell.
        let mask = mask_with_hole(EX, EZ);
        let cut =
            mesh_cutouts_for_piece("room-large", HUB_FLOOR_LEVEL, EX, EZ, 0.0, ext, Some(&mask), false);
        assert_eq!(cut.floor_holes.len(), 1);
        let h = cut.floor_holes[0];
        assert!((h.x - EX).abs() < 2.0 && (h.y - EZ).abs() < 2.0);
    }

    #[test]
    fn floor_0_shaft_only_when_drop_is_masked_hole() {
        let ext = Some(Vec2::new(EX, EZ));
        let solid = FloorMask::filled(15, 15);
        let cut = mesh_cutouts_for_piece("room-large", 0, EX, EZ, 0.0, ext, Some(&solid), false);
        assert!(cut.pit_shaft.is_none());

        let mask = mask_with_hole(EX, EZ);
        let cut = mesh_cutouts_for_piece("room-large", 0, EX, EZ, 0.0, ext, Some(&mask), false);
        assert_eq!(cut.pit_shaft, Some(Vec2::new(EX, EZ)));
    }

    #[test]
    fn hub_room_keeps_door_wall_opening() {
        let ext = Some(Vec2::new(EX, EZ));
        let cut = mesh_cutouts_for_piece("room-large", HUB_FLOOR_LEVEL, EX, EZ, 0.0, ext, None, false);
        assert!(cut.west_opening.is_some());
    }

    #[test]
    fn l3_drop_is_separate_from_landing_tile() {
        // The L3 opening must not be the (ex,ez) landing tile.
        let l3 = hub_l3_drop_centre(EX, EZ);
        assert!((l3.x - EX).abs() + (l3.y - EZ).abs() > 1.0);
    }
}
