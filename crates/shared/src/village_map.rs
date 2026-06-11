//! World-space layout of the village: where the sim's abstract places sit
//! in the 3D level. The simulation keeps its own (larger) abstract map for
//! travel times; this is the compressed, walkable stage version. Server
//! and client both build from this, so it lives in `shared`.

use bevy::prelude::*;

/// Mirrors `sim`'s `PlaceKind` names (the wire format sends place names as
/// strings, so only the positions need to stay in sync).
pub fn place_world_pos(place: &str) -> Vec3 {
    match place {
        "square" => Vec3::new(0.0, 0.0, 0.0),
        "tavern" => Vec3::new(11.0, 0.0, 7.0),
        "bakery" => Vec3::new(-11.0, 0.0, 7.0),
        "dock" => Vec3::new(24.0, 0.0, -42.0),
        "farm" => Vec3::new(-52.0, 0.0, 38.0),
        _ => Vec3::ZERO,
    }
}

/// One hut per roster slot, on a ring south of the square.
pub fn home_world_pos(index: usize) -> Vec3 {
    let angle = (index as f32 / 8.0) * std::f32::consts::TAU + 0.4;
    let radius = 17.0 + (index % 3) as f32 * 4.0;
    Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius)
}

/// Footprint (full extents) of each building, door always facing the square.
pub fn building_size(place: &str) -> Vec2 {
    match place {
        "tavern" => Vec2::new(8.0, 6.0),
        "bakery" => Vec2::new(6.0, 5.0),
        _ => Vec2::new(4.0, 4.0), // huts
    }
}
