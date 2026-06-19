//! Extraction pit: cut room shell GLBs for open pits, shaft, and hub exits.

use bevy::prelude::{Vec2, Vec3};

use crate::kenney_catalog::{self, quantize_yaw};
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
/// Below this Y the extraction shaft ends and hub-floor grounding resumes.
pub const PIT_SHAFT_BOTTOM_Y: f32 = HUB_FLOOR_LEVEL as f32 * MOD_H - 0.35;
/// West module link opening half-width on Z.
pub const WEST_OPENING_HALF_Z: f32 = 2.05;
/// Thickness of west wall triangles to remove around the link opening.
pub const WEST_WALL_THICK: f32 = 1.6;

#[derive(Clone, Copy, Debug, Default)]
pub struct KenneyMeshCutouts {
    pub floor: i32,
    pub floor_holes: [Option<Vec2>; 3],
    pub floor_hole_count: u8,
    pub pit_shaft: Option<Vec2>,
    /// Room centre XZ for a west-face link opening (hub → corridor).
    pub west_opening: Option<Vec2>,
    /// Room centre XZ for an east-face link opening (L2 module ← corridor).
    pub east_opening: Option<Vec2>,
}

impl KenneyMeshCutouts {
    pub fn is_empty(&self) -> bool {
        self.floor_hole_count == 0
            && self.pit_shaft.is_none()
            && self.west_opening.is_none()
            && self.east_opening.is_none()
    }

    pub fn floor_holes(&self) -> impl Iterator<Item = Vec2> + '_ {
        self.floor_holes
            .into_iter()
            .take(self.floor_hole_count as usize)
            .flatten()
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

fn push_hole(holes: &mut [Option<Vec2>; 3], count: &mut u8, centre: Vec2) {
    if (*count as usize) < holes.len() {
        holes[*count as usize] = Some(centre);
        *count += 1;
    }
}

/// All mesh surgery needed for a placed Kenney piece.
pub fn mesh_cutouts_for_piece(
    stem: &str,
    floor: i32,
    px: f32,
    pz: f32,
    yaw: f32,
    ex: f32,
    ez: f32,
) -> KenneyMeshCutouts {
    let mut cutouts = KenneyMeshCutouts {
        floor,
        ..Default::default()
    };
    if !is_room_shell(stem) {
        return cutouts;
    }

    if (floor == 0) && piece_overlaps_tile(px, pz, stem, yaw, ex, ez) {
        push_hole(
            &mut cutouts.floor_holes,
            &mut cutouts.floor_hole_count,
            Vec2::new(ex, ez),
        );
    }
    if floor == HUB_FLOOR_LEVEL {
        if piece_overlaps_tile(px, pz, stem, yaw, ex, ez) {
            push_hole(
                &mut cutouts.floor_holes,
                &mut cutouts.floor_hole_count,
                Vec2::new(ex, ez),
            );
        }
        let west_drop = hub_west_drop_centre(ex, ez);
        if piece_overlaps_tile(px, pz, stem, yaw, west_drop.x, west_drop.y) {
            push_hole(
                &mut cutouts.floor_holes,
                &mut cutouts.floor_hole_count,
                west_drop,
            );
        }
        let stair = hub_stairs_opening(ex, ez);
        if piece_overlaps_tile(px, pz, stem, yaw, stair.x, stair.y) {
            push_hole(
                &mut cutouts.floor_holes,
                &mut cutouts.floor_hole_count,
                stair,
            );
        }
        // Hub floor stays solid under hatch props; only carve door link walls.
        if (px - ex).abs() < 0.5 && (pz - ez).abs() < 0.5 {
            cutouts.west_opening = Some(Vec2::new(px, pz));
        } else {
            // L2 branch room on the west module — open the east link wall.
            cutouts.east_opening = Some(Vec2::new(px, pz));
        }
    }
    if floor == -2 && (px - ex).abs() > 5.0 {
        // L4 west-module room — open east wall toward the corridor drop.
        cutouts.east_opening = Some(Vec2::new(px, pz));
    }
    if floor == 0 && piece_overlaps_pit_tile(px, pz, stem, yaw, ex, ez) {
        cutouts.pit_shaft = Some(Vec2::new(ex, ez));
    }
    cutouts
}

pub fn floor_cutout_centers(
    stem: &str,
    floor: i32,
    px: f32,
    pz: f32,
    yaw: f32,
    ex: f32,
    ez: f32,
) -> Vec<Vec2> {
    mesh_cutouts_for_piece(stem, floor, px, pz, yaw, ex, ez)
        .floor_holes()
        .collect()
}

pub fn needs_pit_floor_cutout(
    stem: &str,
    floor: i32,
    px: f32,
    pz: f32,
    yaw: f32,
    ex: f32,
    ez: f32,
) -> bool {
    !floor_cutout_centers(stem, floor, px, pz, yaw, ex, ez).is_empty()
}

pub fn needs_pit_shaft_cutout(stem: &str, floor: i32, px: f32, pz: f32, yaw: f32, ex: f32, ez: f32) -> bool {
    mesh_cutouts_for_piece(stem, floor, px, pz, yaw, ex, ez)
        .pit_shaft
        .is_some()
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

/// World XZ of the hub-floor opening above the L2 stairs.
pub fn hub_stairs_opening(ex: f32, ez: f32) -> Vec2 {
    let (wx, wz) = hub_west_module_center(ex, ez);
    Vec2::new(wx + 6.0, wz)
}

pub fn in_hub_stairs_opening(x: f32, z: f32, ex: f32, ez: f32) -> bool {
    let s = hub_stairs_opening(ex, ez);
    in_hub_drop_column(x, z, s.x, s.y)
}

/// Hub branch module spacing (5 cells × 4 m).
pub const HUB_MODULE_SPAN: f32 = 20.0;

pub fn in_extraction_drop_zone(x: f32, z: f32, ex: f32, ez: f32) -> bool {
    (x - ex).abs() < PIT_DROP_HALF && (z - ez).abs() < PIT_DROP_HALF
}

/// West-corridor drop hole on the hub band (floor -1).
pub fn hub_west_drop_centre(ex: f32, ez: f32) -> Vec2 {
    Vec2::new(ex - 8.0, ez)
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

pub fn in_hub_l3_drop_zone(x: f32, z: f32, ex: f32, ez: f32) -> bool {
    in_hub_drop_column(x, z, ex, ez)
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

/// Skip solid colliders on decorative door frames / link walls that should stay open.
pub fn skip_hub_passage_collider(
    stem: &str,
    floor: i32,
    px: f32,
    pz: f32,
    ex: f32,
    ez: f32,
) -> bool {
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
    if matches!(stem, "template-floor" | "template-floor-layer" | "template-floor-big")
        && (in_hub_l3_drop_zone(px, pz, ex, ez)
            || in_hub_west_drop_zone(px, pz, ex, ez)
            || in_hub_stairs_opening(px, pz, ex, ez))
    {
        return true;
    }
    false
}

/// While inside the open hole column, ignore rim/adjacent floor probes and coyote time.
pub fn suppress_extraction_grounding(x: f32, y: f32, z: f32, ex: f32, ez: f32) -> bool {
    if y <= PIT_SHAFT_BOTTOM_Y {
        return false;
    }
    let hub_walk = pit_floor_top_y(HUB_FLOOR_LEVEL);
    if y <= hub_walk + 0.25 {
        let west = hub_west_drop_centre(ex, ez);
        let stair = hub_stairs_opening(ex, ez);
        return in_hub_drop_column(x, z, ex, ez)
            || in_hub_drop_column(x, z, west.x, west.y)
            || in_hub_drop_column(x, z, stair.x, stair.y);
    }
    in_hub_drop_column(x, z, ex, ez)
}

/// Hide decorative hatch / patch floor tiles — hub drops use room-shell mesh cutouts.
pub fn hide_extraction_hatch_piece(stem: &str, floor: i32, px: f32, pz: f32, ex: f32, ez: f32) -> bool {
    if hide_hub_floor_hatch_piece(stem, floor, px, pz, ex, ez) {
        return true;
    }
    if floor == HUB_FLOOR_LEVEL
        && matches!(
            stem,
            "template-floor" | "template-floor-layer" | "template-floor-big"
        )
        && (in_hub_l3_drop_zone(px, pz, ex, ez)
            || in_hub_west_drop_zone(px, pz, ex, ez)
            || in_hub_stairs_opening(px, pz, ex, ez))
    {
        return true;
    }
    matches!(
        stem,
        "template-floor-hole" | "template-floor-layer-hole"
    ) && floor == 0
        && (px - ex).abs() < 0.5
        && (pz - ez).abs() < 0.5
}

/// Hub floor -1 hatches are mesh-cut openings; the GLB prop reads as a diagonal wedge.
pub fn hide_hub_floor_hatch_piece(stem: &str, floor: i32, px: f32, pz: f32, ex: f32, ez: f32) -> bool {
    if floor != HUB_FLOOR_LEVEL {
        return false;
    }
    if !matches!(
        stem,
        "template-floor-hole" | "template-floor-layer-hole"
    ) {
        return false;
    }
    in_extraction_drop_zone(px, pz, ex, ez)
        || in_hub_west_drop_zone(px, pz, ex, ez)
        || in_hub_stairs_opening(px, pz, ex, ez)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Vec2;

    const EX: f32 = 20.0;
    const EZ: f32 = 20.0;

    #[test]
    fn floor_0_room_gets_pit_hole_and_shaft() {
        let cut = mesh_cutouts_for_piece("room-large", 0, EX, EZ, 0.0, EX, EZ);
        assert_eq!(cut.floor_hole_count, 1);
        assert_eq!(cut.pit_shaft, Some(Vec2::new(EX, EZ)));
    }

    #[test]
    fn hub_room_gets_centre_and_west_holes() {
        let cut = mesh_cutouts_for_piece("room-large", HUB_FLOOR_LEVEL, EX, EZ, 0.0, EX, EZ);
        assert_eq!(cut.floor_hole_count, 2);
        assert!(cut.west_opening.is_some());
    }

    #[test]
    fn west_hub_room_gets_stair_hole() {
        let cut = mesh_cutouts_for_piece("room-large", HUB_FLOOR_LEVEL, 0.0, EZ, 0.0, EX, EZ);
        assert_eq!(cut.floor_hole_count, 1);
        assert_eq!(cut.floor_holes[0], Some(hub_stairs_opening(EX, EZ)));
    }

    #[test]
    fn hub_room_overlaps_west_drop_tile() {
        assert!(piece_overlaps_tile(20.0, 20.0, "room-large", 0.0, 12.0, 20.0));
    }

    #[test]
    fn west_room_overlaps_stair_opening() {
        assert!(piece_overlaps_tile(0.0, 20.0, "room-large", 0.0, 6.0, 20.0));
    }
}
