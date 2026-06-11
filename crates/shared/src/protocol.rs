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
            .add_client_message::<PlayerInput>(Channel::Unreliable)
            .add_mapped_server_event::<YouAre>(Channel::Ordered);
    }
}
