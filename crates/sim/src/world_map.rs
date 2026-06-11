//! The village layout: fixed positions (meters) for public places and one
//! home per villager. Travel is straight-line walking for now; real paths
//! and obstacles arrive with the 3D binding (V6).

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
    pub fn pos(self) -> Vec2 {
        match self {
            Self::Square => Vec2::new(0.0, 0.0),
            Self::Tavern => Vec2::new(18.0, 10.0),
            Self::Bakery => Vec2::new(-16.0, 9.0),
            Self::Dock => Vec2::new(70.0, -130.0),
            Self::Farm => Vec2::new(-260.0, 190.0),
            Self::Home => Vec2::ZERO,
        }
    }
}

/// Homes ring the square; deterministic per roster index.
pub fn home_pos(index: usize) -> Vec2 {
    let angle = index as f32 / params::HOME_RING_COUNT as f32 * std::f32::consts::TAU;
    let radius = 45.0 + (index % 3) as f32 * 18.0;
    Vec2::new(angle.cos() * radius, angle.sin() * radius)
}

/// Walking time in whole ticks (sim minutes), at least 0.
pub fn travel_ticks(from: Vec2, to: Vec2) -> u64 {
    let distance = from.distance(to);
    (distance / params::WALK_METERS_PER_MINUTE).ceil() as u64
}
