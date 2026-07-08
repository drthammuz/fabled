//! Class definitions shared between client and server.

use serde::{Deserialize, Serialize};

use crate::items;

/// The four playable operator classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ClassKind {
    #[default]
    Soldier,
    Medic,
    Scout,
    Tech,
}

/// All data needed to render a class card and apply its stats.
#[derive(Clone, Copy)]
pub struct ClassDef {
    pub kind: ClassKind,
    pub name: &'static str,
    /// Newline-separated pros/cons for the selection screen.
    pub description: &'static str,
    pub max_hp: f32,
    /// Speed multiplier applied on top of config::PLAYER_MOVE_SPEED.
    pub speed_mult: f32,
    pub inventory_slots: usize,
    /// items:: ID constants granted at spawn, in slot order. Must fit in
    /// `inventory_slots`.
    pub starting_items: &'static [u32],
    /// Asset path to the portrait PNG (relative to `assets/`).
    pub skin_path: &'static str,
    /// Asset path to the in-game GLB character model (relative to `assets/`).
    pub model_path: &'static str,
    /// sRGB tint used for the 3-D capsule.
    pub capsule_color: [f32; 3],
}

pub const ALL_CLASSES: [ClassDef; 4] = [
    ClassDef {
        kind: ClassKind::Soldier,
        name: "Soldier",
        description: "+50% HP · Enemies prioritise you\nWears armor (others can't) · Faster melee\nHighest carry capacity · 4 slots\nStarts with Pipe Bat + Blaster",
        max_hp: 150.0,
        speed_mult: 1.0,
        inventory_slots: 4,
        starting_items: &[items::PIPE_BAT, items::SCRAP_PISTOL],
        skin_path: "characters/Skins/criminalMaleA.png",
        model_path: "models/cyberpunk/character/Character.gltf",
        capsule_color: [0.85, 0.25, 0.15],
    },
    ClassDef {
        kind: ClassKind::Medic,
        name: "Medic",
        description: "Heals others (+50% item effect) · 3 slots\nLeft-click an ally to heal, use on self too\nRevive downed teammates (coming soon)\nStarts with Medical Bag",
        max_hp: 100.0,
        speed_mult: 1.0,
        inventory_slots: 3,
        starting_items: &[items::MEDICAL_BAG],
        skin_path: "characters/Skins/skaterMaleA.png",
        model_path: "models/cyberpunk/character/Character.gltf",
        capsule_color: [0.2, 0.82, 0.38],
    },
    ClassDef {
        kind: ClassKind::Scout,
        name: "Scout",
        description: "+25% movement speed\nNear-full crouch speed · Silent movement\nTagger: mark enemies for the team (coming soon)\n— 30% less HP · 2 slots · Starts with Tagger",
        max_hp: 70.0,
        speed_mult: 1.25,
        inventory_slots: 2,
        starting_items: &[items::TAGGER],
        skin_path: "characters/Skins/skaterFemaleA.png",
        model_path: "models/cyberpunk/character/Character.gltf",
        capsule_color: [0.15, 0.55, 0.95],
    },
    ClassDef {
        kind: ClassKind::Tech,
        name: "Tech",
        description: "Hack faction terminals for credits & data\nExclusive terminal options (others locked out)\nHack doors, drones & turrets (coming soon)\nStarts with Hacker Device · 2 slots",
        max_hp: 100.0,
        speed_mult: 1.0,
        inventory_slots: 2,
        starting_items: &[items::HACKER_DEVICE],
        skin_path: "characters/Skins/cyborgFemaleA.png",
        model_path: "models/cyberpunk/character/Character.gltf",
        capsule_color: [0.95, 0.72, 0.08],
    },
];

/// Returns the definition for a given class (copy).
pub fn class_def(kind: ClassKind) -> ClassDef {
    ALL_CLASSES
        .iter()
        .copied()
        .find(|d| d.kind == kind)
        .unwrap()
}
