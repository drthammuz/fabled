//! Server-side player lifecycle. Movement is handled by the kinematic
//! character controller in `character.rs`; clients only send `PlayerInput`.

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use shared::config;
use shared::level;
use shared::protocol::{NetTransform, Player, PlayerInput, PlayerName, YouAre};

use crate::character::{
    CharacterCollisions, CharacterController, GroundDetection,
};

pub struct ServerPlayersPlugin;

impl Plugin for ServerPlayersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpawnCounter>()
            .add_plugins(super::character::CharacterControllerPlugin)
            .add_observer(on_client_connected)
            .add_observer(on_client_disconnected)
            .add_systems(
                FixedLast,
                sync_net_transforms.run_if(in_state(ClientState::Disconnected)),
            );
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
            super::items::spawn_world_item(
                &mut commands,
                item,
                transform.translation + offset,
                Vec3::ZERO,
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
            Replicated,
            Player,
            CharacterController,
            GroundDetection::default(),
            CharacterCollisions::default(),
            super::grab::GrabTarget::default(),
            super::items::Inventory::default(),
            PlayerName(name),
            PlayerOwner(owner),
            LatestInput::default(),
            Mass(config::PLAYER_MASS),
            LinearVelocity::default(),
            NetTransform {
                translation: spawn_pos,
                rotation: Quat::IDENTITY,
            },
            Collider::capsule(config::PLAYER_CAPSULE_RADIUS, config::PLAYER_CAPSULE_LENGTH),
            Transform::from_translation(spawn_pos),
        ))
        .id()
}

fn sync_net_transforms(mut query: Query<(&Transform, &mut NetTransform)>) {
    for (transform, mut net) in &mut query {
        net.set_if_neq(NetTransform {
            translation: transform.translation,
            rotation: transform.rotation,
        });
    }
}
