//! Network protocol: replicated components and client/server messages.
//!
//! IMPORTANT: registration order must be identical on server and client,
//! so ALL registration lives here and every run mode adds `ProtocolPlugin`.

use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::props::PropShape;

/// Marker for player entities. Replicated to all clients.
#[derive(Component, Serialize, Deserialize)]
pub struct Player;

/// Display name of a player. Replicated to all clients.
#[derive(Component, Serialize, Deserialize, Clone)]
pub struct PlayerName(pub String);

/// Server-authoritative transform, written by the server every fixed tick
/// and replicated. Kept separate from `Transform` so clients can interpolate
/// `Transform` freely without replication overwriting it mid-frame.
#[derive(Component, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub struct NetTransform {
    pub translation: Vec3,
    pub rotation: Quat,
}

/// Client -> server: movement intent for one player. Sent unreliably every
/// frame; the server keeps the latest one per client.
#[derive(Message, Serialize, Deserialize, Clone, Copy, Default)]
pub struct PlayerInput {
    /// Local-space move intent: x = strafe right, y = forward. Length <= 1.
    pub move_dir: Vec2,
    /// Look yaw in radians; the server uses it to orient movement.
    pub yaw: f32,
    /// Look pitch in radians (negative = look down).
    pub pitch: f32,
    /// True if jump was pressed since the last input message.
    pub jump: bool,
    /// Hold Shift to sprint.
    pub sprint: bool,
    /// Hold to grab a dynamic object in view.
    pub grab: bool,
    /// True if throw was pressed since the last input message.
    pub throw_action: bool,
    /// True if interact (pickup) was pressed since the last input message.
    pub interact: bool,
    /// Set when drop was pressed: which inventory slot to drop.
    pub drop_slot: Option<u8>,
}

/// A pickup item. On world entities this is replicated to everyone;
/// inside inventories it only travels to the owning client via
/// `InventoryUpdate`.
#[derive(Component, Serialize, Deserialize, Clone, Debug)]
pub struct Item {
    pub id: u32,
    pub name: String,
    pub weight: f32,
    pub value: u32,
}

/// Server -> owning client only: full contents of YOUR inventory.
#[derive(Message, Serialize, Deserialize, Clone)]
pub struct InventoryUpdate {
    pub slots: Vec<Option<Item>>,
}

/// A simulated villager. Replicated to all clients; the client picks a
/// character model and tint from the profession.
#[derive(Component, Serialize, Deserialize, Clone)]
pub struct Villager {
    pub name: String,
    pub profession: String,
}

/// What a villager is currently doing, for animation and the action labels.
/// Updated by the server only when it changes.
#[derive(Component, Serialize, Deserialize, Clone, PartialEq)]
pub struct VillagerState {
    /// Sim action name ("sleep", "eat", "work", "warm_up", "socialize", "idle").
    pub action: String,
    /// Where it happens ("tavern", "farm", "home", ...).
    pub place: String,
    /// True while walking to the venue.
    pub walking: bool,
}

/// A villager's live stats, for the overhead info panel. Needs and mood
/// are 0..=100; purse is in coins. Updated by the server each sim minute
/// (only when changed).
#[derive(Component, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub struct VillagerStats {
    pub hunger: u8,
    pub energy: u8,
    pub warmth: u8,
    pub social: u8,
    pub mood: u8,
    pub purse: i64,
}

/// Village time, on a single marker entity. Drives the client's sun.
#[derive(Component, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub struct VillageClock {
    pub day: u64,
    pub minute_of_day: u64,
}

/// Server -> owning client: "this replicated entity is your player".
#[derive(Event, Serialize, Deserialize, Clone, Copy)]
pub struct YouAre {
    pub player: Entity,
}

impl MapEntities for YouAre {
    fn map_entities<M: EntityMapper>(&mut self, mapper: &mut M) {
        self.player = mapper.get_mapped(self.player);
    }
}

pub struct ProtocolPlugin;

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        app.replicate::<Player>()
            .replicate::<PlayerName>()
            .replicate::<NetTransform>()
            .replicate::<PropShape>()
            .replicate::<Item>()
            .replicate::<Villager>()
            .replicate::<VillagerState>()
            .replicate::<VillagerStats>()
            .replicate::<VillageClock>()
            .add_client_message::<PlayerInput>(Channel::Unreliable)
            .add_server_message::<InventoryUpdate>(Channel::Ordered)
            .add_mapped_server_event::<YouAre>(Channel::Ordered);
    }
}
