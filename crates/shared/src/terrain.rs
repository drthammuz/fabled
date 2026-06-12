//! Procedural terrain: a deterministic heightfield shared by the server
//! (trimesh collider) and the client (rendered mesh). Rolling hills in the
//! distance; the village itself and the walking corridors between its
//! venues sit on a flattened plateau at y = 0 so buildings and NPC paths
//! stay simple.

use bevy::prelude::*;

use crate::village_map::place_world_pos;

/// Full extent of the terrain patch, centered on the village square.
pub const TERRAIN_SIZE: f32 = 240.0;
/// Quads per side (so `TERRAIN_CELLS + 1` vertices per side).
pub const TERRAIN_CELLS: usize = 160;

/// Deterministic integer hash -> [0, 1).
fn hash(ix: i32, iz: i32) -> f32 {
    let mut h = (ix as u32).wrapping_mul(0x85eb_ca6b) ^ (iz as u32).wrapping_mul(0xc2b2_ae35);
    h ^= h >> 13;
    h = h.wrapping_mul(0x27d4_eb2f);
    h ^= h >> 16;
    (h & 0x00ff_ffff) as f32 / 16_777_216.0
}

/// Smoothly interpolated value noise -> [-1, 1].
fn value_noise(x: f32, z: f32) -> f32 {
    let (ix, iz) = (x.floor() as i32, z.floor() as i32);
    let (fx, fz) = (x - x.floor(), z - z.floor());
    let (sx, sz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let top = hash(ix, iz) + (hash(ix + 1, iz) - hash(ix, iz)) * sx;
    let bottom = hash(ix, iz + 1) + (hash(ix + 1, iz + 1) - hash(ix, iz + 1)) * sx;
    (top + (bottom - top) * sz) * 2.0 - 1.0
}

/// Three octaves of value noise -> roughly [-1, 1].
fn fbm(x: f32, z: f32) -> f32 {
    value_noise(x, z) * 0.6
        + value_noise(x * 2.1 + 31.7, z * 2.1 - 18.2) * 0.28
        + value_noise(x * 4.3 - 7.5, z * 4.3 + 51.0) * 0.12
}

fn point_segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// 0 on the village plateau, fading to 1 in the open hills. The flat zone
/// covers the square + home ring, the farm, the dock, and corridors along
/// the walking routes between them.
fn hill_mask(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let square = place_world_pos("square");
    let farm = place_world_pos("farm");
    let dock = place_world_pos("dock");
    let (square, farm, dock) = (square.xz(), farm.xz(), dock.xz());

    // Distance beyond each flat feature (negative = inside it).
    let mut d = p.distance(square) - 36.0;
    d = d.min(p.distance(farm) - 19.0);
    d = d.min(p.distance(dock) - 14.0);
    d = d.min(point_segment_distance(p, square, farm) - 8.0);
    d = d.min(point_segment_distance(p, square, dock) - 8.0);

    let fade = 18.0;
    let t = (d / fade).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Terrain height at a world position. Exactly 0 across the village.
pub fn height(x: f32, z: f32) -> f32 {
    let mask = hill_mask(x, z);
    if mask <= 0.0 {
        return 0.0;
    }
    let hills = fbm(x * 0.028, z * 0.028) * 6.5 + 1.0;
    hills * mask
}

/// Grid vertices for the terrain mesh/collider, row-major.
pub fn grid_positions() -> Vec<Vec3> {
    let verts = TERRAIN_CELLS + 1;
    let step = TERRAIN_SIZE / TERRAIN_CELLS as f32;
    let half = TERRAIN_SIZE / 2.0;
    let mut positions = Vec::with_capacity(verts * verts);
    for iz in 0..verts {
        for ix in 0..verts {
            let x = ix as f32 * step - half;
            let z = iz as f32 * step - half;
            positions.push(Vec3::new(x, height(x, z), z));
        }
    }
    positions
}

/// Triangle indices matching `grid_positions`.
pub fn grid_indices() -> Vec<[u32; 3]> {
    let verts = (TERRAIN_CELLS + 1) as u32;
    let mut indices = Vec::with_capacity(TERRAIN_CELLS * TERRAIN_CELLS * 2);
    for iz in 0..TERRAIN_CELLS as u32 {
        for ix in 0..TERRAIN_CELLS as u32 {
            let a = iz * verts + ix;
            let b = a + 1;
            let c = a + verts;
            let d = c + 1;
            indices.push([a, c, b]);
            indices.push([b, c, d]);
        }
    }
    indices
}

/// Outward normal at a world position (finite differences).
pub fn normal(x: f32, z: f32) -> Vec3 {
    const E: f32 = 0.75;
    Vec3::new(
        height(x - E, z) - height(x + E, z),
        2.0 * E,
        height(x, z - E) - height(x, z + E),
    )
    .normalize()
}
