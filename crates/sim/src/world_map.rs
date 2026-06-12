//! The village layout: fixed positions (meters) for public places and one
//! home per villager. Travel is straight-line walking.
//!
//! KEEP IN SYNC with `shared::village_map`: these positions are the 3D
//! world layout scaled by `WORLD_SCALE`, so travel times tuned in headless
//! runs stay proportional to what players see in the live village. (The
//! 3D stage is compressed for gameplay; the sim walks the "real-world"
//! distances.)

use bevy::prelude::*;
use serde::Serialize;

use crate::params;

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceKind {
    Farm,
    Dock,
    Bakery,
    Tavern,
    Square,
    Home,
}

impl PlaceKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Farm => "farm",
            Self::Dock => "dock",
            Self::Bakery => "bakery",
            Self::Tavern => "tavern",
            Self::Square => "square",
            Self::Home => "home",
        }
    }

    /// Open-air places expose you to the weather while you're there.
    pub fn outdoors(self) -> bool {
        matches!(self, Self::Farm | Self::Dock | Self::Square)
    }

    /// Where public places sit. `Home` positions are per-NPC (see `home_pos`).
    /// These are `shared::village_map::place_world_pos` * `WORLD_SCALE`.
    pub fn pos(self) -> Vec2 {
        match self {
            Self::Square => Vec2::new(0.0, 0.0),
            Self::Tavern => Vec2::new(11.0, 7.0) * WORLD_SCALE,
            Self::Bakery => Vec2::new(-11.0, 7.0) * WORLD_SCALE,
            Self::Dock => Vec2::new(24.0, -42.0) * WORLD_SCALE,
            Self::Farm => Vec2::new(-52.0, 38.0) * WORLD_SCALE,
            Self::Home => Vec2::ZERO,
        }
    }
}

/// How many "real" meters one meter of the compressed 3D stage stands for.
pub const WORLD_SCALE: f32 = 4.0;

/// Homes ring the square; deterministic per roster index. Mirrors
/// `shared::village_map::home_world_pos` * `WORLD_SCALE`.
pub fn home_pos(index: usize) -> Vec2 {
    let angle = index as f32 / params::HOME_RING_COUNT as f32 * std::f32::consts::TAU + 0.4;
    let radius = (20.0 + (index % 3) as f32 * 5.0) * WORLD_SCALE;
    Vec2::new(angle.cos() * radius, angle.sin() * radius)
}

/// Walking time in whole ticks (sim minutes), at least 0.
pub fn travel_ticks(from: Vec2, to: Vec2) -> u64 {
    let distance = from.distance(to);
    (distance / params::WALK_METERS_PER_MINUTE).ceil() as u64
}
