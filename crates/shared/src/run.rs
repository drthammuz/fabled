//! Run state, camp types, and the stretch routing graph.

use bevy::prelude::Component;
use serde::{Deserialize, Serialize};

use crate::level::LevelDef;

/// High-level run phase (server authoritative).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RunPhase {
    #[default]
    InStretch,
    InHub,
    RunOver,
}

/// Hub specialization — biases what the shop stocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CampKind {
    MedBay,
    Armory,
    Workshop,
    Intel,
}

impl CampKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::MedBay => "medbay",
            Self::Armory => "armory",
            Self::Workshop => "workshop",
            Self::Intel => "intel",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::MedBay => "Med Bay",
            Self::Armory => "Armory",
            Self::Workshop => "Workshop",
            Self::Intel => "Intel Post",
        }
    }
}

/// Replicated summary of the current run (lives on a marker entity).
#[derive(Component, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct RunState {
    pub phase: RunPhase,
    /// ID of the active sewer stretch level. Never changes while in the hub
    /// (the hub room is physically embedded in the extraction cell of this level).
    pub level_id: String,
    /// ID of the hub/camp node we are currently shopping at (Some while InHub).
    /// Kept separate from level_id so the client never reloads visuals on extraction.
    pub hub_id: Option<String>,
    pub credits: u32,
    pub scrap: u32,
    /// Name of the player currently carrying the sector map, if any.
    pub map_holder: Option<String>,
    /// Route choices offered at the hub (empty outside hub phase).
    pub route_options: Vec<RouteOption>,
    /// Seed for this run's procedural level generation. Replicated so
    /// client generates identical geometry from the same seed.
    pub run_seed: u64,
}

/// One selectable path leaving a hub.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct RouteOption {
    pub target_id: String,
    pub label: String,
    pub camp: CampKind,
    pub cost: u32,
}

/// A node in the stretch graph.
pub struct StretchNode {
    pub id: &'static str,
    pub label: &'static str,
    /// Build the level geometry from a seed. Hub rooms ignore the seed;
    /// sewer stretches use it for deterministic procedural generation.
    pub build: fn(seed: u64) -> LevelDef,
    pub camp: CampKind,
    /// `(target_id, route_label, cost)`.
    pub routes: &'static [(&'static str, &'static str, u32)],
}

/// All authored stretches/hubs. The first node is the run start.
pub fn stretch_graph() -> &'static [StretchNode] {
    use crate::level::{
        hub_armory_level, hub_intel_level, hub_medbay_level,
        sewer_branch_a_level, sewer_branch_b_level, sewer_entry_level,
    };
    &[
        StretchNode {
            id: "sewer_entry",
            label: "Drop Shaft — Sector 7",
            build: sewer_entry_level,
            camp: CampKind::MedBay,
            routes: &[("hub_medbay", "Med Bay camp", 0)],
        },
        StretchNode {
            id: "hub_medbay",
            label: "Camp Med Bay",
            build: hub_medbay_level,
            camp: CampKind::MedBay,
            routes: &[
                ("sewer_branch_a", "Subway spur (east)", 5),
                ("sewer_branch_b", "Vent crawl (west)", 5),
                ("hub_intel", "Intel relay (deep)", 10),
            ],
        },
        StretchNode {
            id: "sewer_branch_a",
            label: "Subway Spur A",
            build: sewer_branch_a_level,
            camp: CampKind::Armory,
            routes: &[("hub_armory", "Armory outpost", 0)],
        },
        StretchNode {
            id: "sewer_branch_b",
            label: "Vent Crawl B",
            build: sewer_branch_b_level,
            camp: CampKind::Workshop,
            routes: &[("hub_armory", "Armory outpost", 0)],
        },
        StretchNode {
            id: "hub_armory",
            label: "Camp Armory",
            build: hub_armory_level,
            camp: CampKind::Armory,
            routes: &[
                ("sewer_entry", "Backtrack to drop shaft", 8),
                ("hub_intel", "Intel relay", 6),
            ],
        },
        StretchNode {
            id: "hub_intel",
            label: "Intel Relay",
            build: hub_intel_level,
            camp: CampKind::Intel,
            routes: &[
                ("sewer_branch_a", "Subway spur", 5),
                ("sewer_branch_b", "Vent crawl", 5),
            ],
        },
    ]
}

pub fn node(id: &str) -> Option<&'static StretchNode> {
    stretch_graph().iter().find(|n| n.id == id)
}

pub fn start_node() -> &'static StretchNode {
    &stretch_graph()[0]
}
