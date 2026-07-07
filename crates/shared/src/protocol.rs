//! Network protocol: replicated components and client/server messages.
//!
//! IMPORTANT: registration order must be identical on server and client,
//! so ALL registration lives here and every run mode adds `ProtocolPlugin`.

use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::classes::ClassKind;
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
    /// True on the client tick where jump was pressed (`just_pressed`), not while held.
    pub jump: bool,
    /// Hold Shift to sprint.
    pub sprint: bool,
    /// Hold Ctrl to crouch (reduced capsule, duck under ducts).
    pub crouch: bool,
    /// Hold to grab a dynamic object in view.
    pub grab: bool,
    /// True if throw was pressed since the last input message.
    pub throw_action: bool,
    /// True if interact (pickup) was pressed since the last input message.
    pub interact: bool,
    /// Set when drop was pressed: which inventory slot to drop.
    pub drop_slot: Option<u8>,
    /// Hub shop: buy item by catalog id (10=flashlight, 11=map, 12=bat).
    pub shop_buy: Option<u32>,
    /// Hub routing: index into `RunState.route_options`.
    pub route_select: Option<u8>,
    /// Melee attack this tick (pipe bat).
    pub attack: bool,
    /// World-space point under the crosshair: the client raycasts from its
    /// CAMERA through screen center (first person ≈ the eye ray; third person
    /// diverges from it near the shoulder camera). The server fires from a
    /// muzzle point toward this, so shots land exactly on the crosshair.
    /// Vec3::ZERO / non-finite / far off the look ray = ignored (server falls
    /// back to the raw yaw/pitch ray).
    pub aim: Vec3,
    /// Which hotbar slot is selected (drives which weapon `attack` uses).
    pub selected_slot: u8,
    /// Toggle flashlight (if owned).
    pub flashlight_toggle: bool,
    /// NPC trade: buy the item at this index of the stock last sent in
    /// [`NpcDialogue`]. Validated server-side against NPC proximity — the
    /// server keeps no dialogue session state.
    pub trade_buy: Option<u8>,
    /// NPC trade: sell the item in this inventory slot.
    pub trade_sell: Option<u8>,
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

/// Whether this player is alive in the current run.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Default)]
pub struct PlayerAlive(pub bool);

/// Physics-authoritative grounded state, replicated so clients can use it
/// for footstep logic and future airborne effects.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Default, PartialEq)]
pub struct PlayerGrounded(pub bool);

/// Server → all clients: play the train-passing ambient sound now.
/// The server fires this at random intervals (45–120 s) so all clients
/// hear it simultaneously.
#[derive(Message, Serialize, Deserialize, Clone, Copy)]
pub struct PlayTrainSound;

/// Server → all clients: something hit the water (splash / ripple trigger).
#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct WaterImpact {
    pub channel_id: u32,
    pub position: Vec3,
    pub impulse: f32,
}

/// Which class this player has chosen. Replicated so all clients can
/// display the correct capsule colour.
#[derive(Component, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlayerClass(pub ClassKind);

/// Client → server: sent once when the player picks a class on the
/// selection screen.
#[derive(Message, Serialize, Deserialize, Clone, Copy)]
pub struct ClassPick(pub ClassKind);

/// Hostile creature in a stretch.
#[derive(Component, Serialize, Deserialize, Clone, Copy)]
pub struct Enemy;

/// Friendly NPC (non-hostile). Main logic server-authoritative.
#[derive(Component, Serialize, Deserialize, Clone, Copy)]
pub struct Npc;

/// One purchasable line in an NPC's trade stock.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ShopEntry {
    pub item_id: u32,
    pub name: String,
    pub cost: u32,
}

/// Server → owning client: opens the dialogue window for the NPC the player
/// just pressed E on. Trade actions come back via `PlayerInput::trade_buy` /
/// `trade_sell` and are re-validated against NPC proximity, so the server
/// holds no dialogue session state. The client auto-closes the window when
/// the player walks away from `npc_pos`.
#[derive(Message, Serialize, Deserialize, Clone, Debug)]
pub struct NpcDialogue {
    pub npc_pos: Vec3,
    pub npc_name: String,
    pub greeting: String,
    pub rumor: String,
    pub stock: Vec<ShopEntry>,
}

/// Enemy AI mode, replicated for the dev marker above each enemy's head.
/// `mode` mirrors `server::combat::AiMode` (0 Patrol, 1 Suspicious, 2 Combat,
/// 3 Search, 4 Cover). `frac` is the remaining fraction (0..=63) of the
/// current mode's countdown — the marker's clock hand; 0 = no timer running.
#[derive(Component, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
pub struct EnemyAiMode {
    pub mode: u8,
    pub frac: u8,
}

/// Open/closed state of a door seal, replicated so clients can drive the
/// door animation from the same state that blocks physics/vision/bullets.
#[derive(Component, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
pub struct DoorState {
    pub open: bool,
}

/// A flying bullet/tracer. Server simulates; clients render a glowing streak
/// from the replicated `NetTransform`. `friendly` = fired by a player.
#[derive(Component, Serialize, Deserialize, Clone, Copy)]
pub struct Projectile {
    pub friendly: bool,
}

/// This player's health, replicated so every client can draw HP bars.
#[derive(Component, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub struct PlayerHealth {
    pub current: f32,
    pub max: f32,
}

impl Default for PlayerHealth {
    fn default() -> Self {
        Self {
            current: 100.0,
            max: 100.0,
        }
    }
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
            .replicate::<crate::run::RunState>()
            .replicate::<PlayerAlive>()
            .replicate::<PlayerClass>()
            .replicate::<PlayerGrounded>()
            .replicate::<Enemy>()
            .replicate::<Npc>()
            .replicate::<EnemyAiMode>()
            .replicate::<Projectile>()
            .replicate::<PlayerHealth>()
            .replicate::<DoorState>()
            .add_client_message::<PlayerInput>(Channel::Unreliable)
            .add_client_message::<ClassPick>(Channel::Ordered)
            .add_server_message::<InventoryUpdate>(Channel::Ordered)
            .add_server_message::<PlayTrainSound>(Channel::Ordered)
            .add_server_message::<NpcDialogue>(Channel::Ordered)
            .add_server_message::<WaterImpact>(Channel::Unreliable)
            .add_mapped_server_event::<YouAre>(Channel::Ordered);
    }
}
