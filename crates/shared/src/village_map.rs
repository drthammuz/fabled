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

/// One hut per roster slot, on a ring around the square. The inner radius
/// must clear the tavern/bakery footprints (they reach ~15 m out), or huts
/// intersect them and form sealed wall pockets.
pub fn home_world_pos(index: usize) -> Vec3 {
    let angle = (index as f32 / 8.0) * std::f32::consts::TAU + 0.4;
    let radius = 20.0 + (index % 3) as f32 * 5.0;
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

/// Which axis-aligned wall of a building holds the door: a unit offset
/// pointing from the center toward the door wall. Doors always face the
/// square; the wall builder and NPC pathing must agree on this.
pub fn door_side(center: Vec3, size: Vec2) -> Vec3 {
    let to = place_world_pos("square") - center;
    let (hx, hz) = (size.x / 2.0, size.y / 2.0);
    if to.x.abs() * hz > to.z.abs() * hx {
        Vec3::new(to.x.signum(), 0.0, 0.0)
    } else {
        Vec3::new(0.0, 0.0, to.z.signum())
    }
}

/// A point just outside a building's door, for walk waypoints.
fn door_pos(center: Vec3, size: Vec2) -> Vec3 {
    let side = door_side(center, size);
    let half = Vec3::new(size.x / 2.0, 0.0, size.y / 2.0);
    center + side * (half + Vec3::splat(0.8))
}

/// Door waypoint for a venue, if it has walls (tavern, bakery, homes).
/// Open-air venues (square, farm, dock) need no door.
pub fn place_door_pos(place: &str) -> Option<Vec3> {
    match place {
        "tavern" | "bakery" => {
            Some(door_pos(place_world_pos(place), building_size(place)))
        }
        _ => None,
    }
}

/// Door waypoint for a villager's hut.
pub fn home_door_pos(index: usize) -> Vec3 {
    door_pos(home_world_pos(index), building_size("home"))
}

/// All building footprints (center, full extents), for keeping outdoor
/// wander/patrol targets out of walls.
pub fn building_footprints() -> Vec<(Vec2, Vec2)> {
    let mut list = vec![
        (place_world_pos("tavern").xz(), building_size("tavern")),
        (place_world_pos("bakery").xz(), building_size("bakery")),
        // The farm barn (see `shared::level::village_level`).
        (place_world_pos("farm").xz() + Vec2::new(0.0, -9.0), Vec2::new(5.0, 4.0)),
    ];
    for index in 0..8 {
        list.push((home_world_pos(index).xz(), building_size("home")));
    }
    list
}

/// Is this point inside (or within `margin` of) any building footprint?
pub fn inside_any_building(p: Vec2, margin: f32) -> bool {
    building_footprints().iter().any(|(center, size)| {
        (p.x - center.x).abs() <= size.x / 2.0 + margin
            && (p.y - center.y).abs() <= size.y / 2.0 + margin
    })
}
