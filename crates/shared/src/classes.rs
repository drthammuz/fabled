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
    /// items:: ID constant for the starting item, or None.
    pub starting_item_id: Option<u32>,
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
        description: "+50% HP · Enemies prioritise you\n+Melee damage · +10% vendor discount\nStarts with Pipe Bat\n2 inventory slots",
        max_hp: 150.0,
        speed_mult: 1.0,
        inventory_slots: 2,
        starting_item_id: Some(items::PIPE_BAT),
        skin_path: "characters/Skins/criminalMaleA.png",
        model_path: "models/Knight.glb",
        capsule_color: [0.85, 0.25, 0.15],
    },
    ClassDef {
        kind: ClassKind::Medic,
        name: "Medic",
        description: "Revive teammates · 3 inventory slots\nCraft medkits anywhere (no station)\nToxin resistance in sewer zones\nStarts with Medical Bag",
        max_hp: 100.0,
        speed_mult: 1.0,
        inventory_slots: 3,
        starting_item_id: Some(items::MEDICAL_BAG),
        skin_path: "characters/Skins/skaterMaleA.png",
        model_path: "models/Mage.glb",
        capsule_color: [0.2, 0.82, 0.38],
    },
    ClassDef {
        kind: ClassKind::Scout,
        name: "Scout",
        description: "+25% movement speed\nNear-full crouch speed · Silent movement\nShortened enemy detection range\n— 30% less HP · 2 slots · No starter",
        max_hp: 70.0,
        speed_mult: 1.25,
        inventory_slots: 2,
        starting_item_id: None,
        skin_path: "characters/Skins/skaterFemaleA.png",
        model_path: "models/Rogue.glb",
        capsule_color: [0.15, 0.55, 0.95],
    },
    ClassDef {
        kind: ClassKind::Tech,
        name: "Tech",
        description: "Hack doors, drones & faction terminals\nAccess hidden loot · Disable turrets\nReprogram drones as temporary allies\nStarts with Hacker Device · 2 slots",
        max_hp: 100.0,
        speed_mult: 1.0,
        inventory_slots: 2,
        starting_item_id: Some(items::HACKER_DEVICE),
        skin_path: "characters/Skins/cyborgFemaleA.png",
        model_path: "models/Barbarian.glb",
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
