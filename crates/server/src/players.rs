//! Server-side player lifecycle. Movement is handled by the kinematic
//! character controller in `character.rs`; clients only send `PlayerInput`.

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use shared::config;
use shared::level;
use shared::protocol::{
    ClassPick, InventoryUpdate, NetTransform, Player, PlayerAlive, PlayerClass, PlayerInput,
    PlayerName, PlayTrainSound, YouAre,
};
use shared::{classes, items};

use crate::combat::Health;

use crate::character::{
    CharacterCollisions, CharacterController, CrouchState, GroundDetection, PlayerWaterContact,
    SpeedMultiplier,
};

pub struct ServerPlayersPlugin;

impl Plugin for ServerPlayersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpawnCounter>()
            .init_resource::<TrainSoundTimer>()
            .add_plugins(super::character::CharacterControllerPlugin)
            .add_observer(on_client_connected)
            .add_observer(on_client_disconnected)
            .add_systems(
                FixedUpdate,
                (
                    handle_class_pick,
                    tick_train_sound,
                )
                    .run_if(in_state(ClientState::Disconnected)),
            )
            .add_systems(
                FixedLast,
                sync_net_transforms.run_if(in_state(ClientState::Disconnected)),
            );
    }
}

/// Counts down until the next train-passing sound is broadcast to all clients.
#[derive(Resource)]
struct TrainSoundTimer {
    remaining: f32,
    /// Simple LCG state for pseudo-random interval generation.
    rng: u64,
}

impl Default for TrainSoundTimer {
    fn default() -> Self {
        Self { remaining: 60.0, rng: 0xdeadbeef_cafef00d }
    }
}

impl TrainSoundTimer {
    /// Next pseudo-random float in [0, 1).
    fn next_f32(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng as f32) / (u64::MAX as f32)
    }
}

fn tick_train_sound(
    time: Res<Time>,
    mut timer: ResMut<TrainSoundTimer>,
    mut writer: MessageWriter<ToClients<PlayTrainSound>>,
) {
    timer.remaining -= time.delta_secs();
    if timer.remaining <= 0.0 {
        // Broadcast to every connected client (and host).
        writer.write(ToClients {
            targets: SendTargets::All,
            message: PlayTrainSound,
        });
        // Next interval: 90–200 seconds.
        timer.remaining = 90.0 + timer.next_f32() * 110.0;
    }
}

/// Cycles through level player spawn points by join order.
#[derive(Resource, Default)]
pub struct SpawnCounter(usize);

/// Which client owns this player entity. Server-only, never replicated.
#[derive(Component)]
pub struct PlayerOwner(pub ClientId);

/// Latest movement intent received from the owning client. Server-only.
#[derive(Component, Default)]
pub struct LatestInput(pub PlayerInput);

/// Spawns the player entity for the local (listen-server) participant.
pub fn spawn_local_player(mut commands: Commands, mut counter: ResMut<SpawnCounter>) {
    let player = spawn_player(&mut commands, &mut counter, ClientId::Server, "Host".into());
    commands.server_trigger(ToClients {
        targets: SendTargets::Single(ClientId::Server),
        message: YouAre { player },
    });
}

fn on_client_connected(
    add: On<Add, AuthorizedClient>,
    mut commands: Commands,
    mut counter: ResMut<SpawnCounter>,
) {
    let client_entity = add.entity;
    let client_id = ClientId::Client(client_entity);
    let name = format!("Player {}", counter.0 + 1);
    info!("client {client_entity} connected, spawning '{name}'");
    let player = spawn_player(&mut commands, &mut counter, client_id, name);
    commands.server_trigger(ToClients {
        targets: SendTargets::Single(client_id),
        message: YouAre { player },
    });
}

fn on_client_disconnected(
    remove: On<Remove, ConnectedClient>,
    mut commands: Commands,
    mut players: Query<(Entity, &PlayerOwner, &Transform, &mut super::items::Inventory)>,
) {
    let client_id = ClientId::Client(remove.entity);
    for (entity, owner, transform, mut inventory) in &mut players {
        if owner.0 != client_id {
            continue;
        }
        // Spill carried loot into the world so it isn't lost for the team.
        // The grab (if any) releases by itself: the force-applying system
        // stops running once the player entity is gone.
        let mut dropped = 0;
        for (slot, item) in inventory.0.iter_mut().enumerate() {
            let Some(item) = item.take() else {
                continue;
            };
            let angle = slot as f32 / config::INVENTORY_SLOTS as f32 * std::f32::consts::TAU;
            let offset = Vec3::new(angle.cos(), 1.0, angle.sin()) * 0.6;
            super::level::spawn_world_item(
                &mut commands,
                item,
                transform.translation + offset,
                Vec3::ZERO,
                false,
            );
            dropped += 1;
        }
        info!(
            "client disconnected, despawning player {entity} ({dropped} items dropped)"
        );
        commands.entity(entity).despawn();
    }
}

fn spawn_player(
    commands: &mut Commands,
    counter: &mut SpawnCounter,
    owner: ClientId,
    name: String,
) -> Entity {
    let spawns = level::active_level().player_spawns;
    let spawn_pos = spawns[counter.0 % spawns.len()]
        + Vec3::Y * (config::PLAYER_CAPSULE_LENGTH / 2.0 + config::PLAYER_CAPSULE_RADIUS);
    counter.0 += 1;

    commands
        .spawn((
            (
                Replicated,
                Player,
                CharacterController,
                GroundDetection::default(),
                CharacterCollisions::default(),
                super::grab::GrabTarget::default(),
                PlayerName(name),
                PlayerAlive(true),
                PlayerClass::default(),
                PlayerOwner(owner),
                LatestInput::default(),
                SpeedMultiplier::default(),
                PlayerWaterContact::default(),
                Mass(config::PLAYER_MASS),
                LinearVelocity::default(),
            ),
            (
                NetTransform {
                    translation: spawn_pos,
                    rotation: Quat::IDENTITY,
                },
                Collider::capsule(config::PLAYER_CAPSULE_RADIUS, config::PLAYER_CAPSULE_LENGTH),
                Transform::from_translation(spawn_pos),
            ),
        ))
        .insert((
            CrouchState::default(),
            super::items::Inventory::default(),
            Health::default(),
        ))
        .id()
}

fn handle_class_pick(
    mut picks: MessageReader<FromClient<ClassPick>>,
    mut players: Query<
        (
            &PlayerOwner,
            &mut PlayerClass,
            &mut SpeedMultiplier,
            &mut super::items::Inventory,
            &mut Health,
            &PlayerAlive,
        ),
        With<Player>,
    >,
    mut writer: MessageWriter<ToClients<InventoryUpdate>>,
) {
    for FromClient { client_id, message } in picks.read() {
        let ClassPick(kind) = *message;
        let def = classes::class_def(kind);
        for (owner, mut class, mut speed, mut inv, mut health, alive) in &mut players {
            if owner.0 != *client_id {
                continue;
            }
            if !alive.0 {
                continue;
            }
            class.0 = kind;
            speed.0 = def.speed_mult;
            health.max = def.max_hp;
            health.current = def.max_hp;
            // Resize inventory to class limit; grant starting item in slot 0.
            inv.0 = vec![None; def.inventory_slots];
            if let Some(item_id) = def.starting_item_id {
                let item = match item_id {
                    items::PIPE_BAT => Some(items::pipe_bat()),
                    items::MEDICAL_BAG => Some(items::medical_bag()),
                    items::HACKER_DEVICE => Some(items::hacker_device()),
                    _ => None,
                };
                if let Some(item) = item {
                    inv.0[0] = Some(item);
                }
            }
            // Pad to config::INVENTORY_SLOTS so the client hotbar always
            // receives a full-length update.
            let mut padded = inv.0.clone();
            while padded.len() < config::INVENTORY_SLOTS {
                padded.push(None);
            }
            writer.write(ToClients {
                targets: SendTargets::Single(owner.0),
                message: InventoryUpdate { slots: padded },
            });
            info!("player {:?} chose {:?}", client_id, kind);
        }
    }
}

fn sync_net_transforms(mut query: Query<(&Transform, &mut NetTransform)>) {
    for (transform, mut net) in &mut query {
        net.set_if_neq(NetTransform {
            translation: transform.translation,
            rotation: transform.rotation,
        });
    }
}
