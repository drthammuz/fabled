//! Server-side items and inventories. World items are physics objects
//! replicated to everyone; inventory contents are private and only sent
//! to the owning client via `InventoryUpdate`.

use avian3d::{math::*, prelude::*};
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use shared::config;
use shared::level;
use shared::props::PropShape;
use shared::protocol::{Item, InventoryUpdate, NetTransform, Player};

use crate::character::CharacterSystems;
use crate::players::{LatestInput, PlayerOwner};

pub struct ServerItemsPlugin;

impl Plugin for ServerItemsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_level_items).add_systems(
            FixedUpdate,
            (pickup_items, drop_items)
                .chain()
                .after(CharacterSystems)
                .run_if(in_state(ClientState::Disconnected)),
        );
    }
}

/// Server-side inventory: fixed slots, never replicated as a component.
#[derive(Component)]
pub struct Inventory(pub Vec<Option<Item>>);

impl Default for Inventory {
    fn default() -> Self {
        Self(vec![None; config::INVENTORY_SLOTS])
    }
}

fn item_catalog() -> Vec<Item> {
    vec![
        Item {
            id: 1,
            name: "Medkit".into(),
            weight: 2.0,
            value: 50,
        },
        Item {
            id: 2,
            name: "Battery".into(),
            weight: 1.0,
            value: 30,
        },
        Item {
            id: 3,
            name: "Gold Bar".into(),
            weight: 8.0,
            value: 500,
        },
    ]
}

fn spawn_level_items(mut commands: Commands) {
    let spawns = level::test_level().item_spawns;
    let catalog = item_catalog();
    for (i, pos) in spawns.iter().enumerate() {
        let item = catalog[i % catalog.len()].clone();
        spawn_world_item(&mut commands, item, *pos, Vec3::ZERO);
    }
    info!("spawned {} world items", spawns.len());
}

/// Spawns a pickup item as a physics object in the world. Used for both
/// initial level spawns and player drops.
pub fn spawn_world_item(
    commands: &mut Commands,
    item: Item,
    position: Vec3,
    velocity: Vec3,
) -> Entity {
    let size = config::ITEM_SIZE;
    commands
        .spawn((
            Replicated,
            RigidBody::Dynamic,
            Collider::cuboid(size, size, size),
            ColliderDensity(80.0),
            Friction::new(0.5),
            Restitution::new(config::PROP_RESTITUTION),
            AngularDamping(config::PROP_ANGULAR_DAMPING),
            // Small fast objects tunnel through level geometry without CCD.
            SweptCcd::default(),
            PropShape::Crate {
                size: Vec3::splat(size),
            },
            item,
            LinearVelocity(velocity.adjust_precision()),
            NetTransform {
                translation: position,
                rotation: Quat::IDENTITY,
            },
            Transform::from_translation(position),
        ))
        .id()
}

fn look_direction(yaw: f32, pitch: f32) -> Vec3 {
    Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0) * -Vec3::Z
}

fn send_inventory(
    writer: &mut MessageWriter<ToClients<InventoryUpdate>>,
    owner: ClientId,
    inventory: &Inventory,
) {
    writer.write(ToClients {
        targets: SendTargets::Single(owner),
        message: InventoryUpdate {
            slots: inventory.0.clone(),
        },
    });
}

fn pickup_items(
    mut commands: Commands,
    spatial: SpatialQuery,
    colliders: Query<&ColliderOf>,
    items: Query<&Item>,
    mut players: Query<
        (Entity, &Transform, &mut LatestInput, &mut Inventory, &PlayerOwner),
        With<Player>,
    >,
    mut writer: MessageWriter<ToClients<InventoryUpdate>>,
) {
    for (player, transform, mut input, mut inventory, owner) in &mut players {
        if !input.0.interact {
            continue;
        }
        input.0.interact = false;

        let Some(free_slot) = inventory.0.iter().position(Option::is_none) else {
            info!("player {player} tried to pick up with a full inventory");
            continue;
        };

        let eye = transform.translation + Vec3::Y * config::PLAYER_EYE_HEIGHT;
        let dir =
            Dir3::new(look_direction(input.0.yaw, input.0.pitch)).unwrap_or(Dir3::NEG_Z);
        let Some(hit) = spatial.cast_ray(
            eye.adjust_precision(),
            dir,
            config::INTERACT_RANGE,
            true,
            &SpatialQueryFilter::from_excluded_entities([player]),
        ) else {
            continue;
        };

        let body = colliders
            .get(hit.entity)
            .map(|c| c.body)
            .unwrap_or(hit.entity);
        let Ok(item) = items.get(body) else {
            continue;
        };

        info!("player picked up '{}' into slot {free_slot}", item.name);
        inventory.0[free_slot] = Some(item.clone());
        commands.entity(body).despawn();
        send_inventory(&mut writer, owner.0, &inventory);
    }
}

fn drop_items(
    mut commands: Commands,
    mut players: Query<
        (&Transform, &mut LatestInput, &mut Inventory, &PlayerOwner),
        With<Player>,
    >,
    mut writer: MessageWriter<ToClients<InventoryUpdate>>,
) {
    for (transform, mut input, mut inventory, owner) in &mut players {
        let Some(slot) = input.0.drop_slot.take() else {
            continue;
        };
        let slot = slot as usize;
        if slot >= inventory.0.len() {
            continue;
        }
        let Some(item) = inventory.0[slot].take() else {
            continue;
        };

        let dir = look_direction(input.0.yaw, input.0.pitch);
        let position =
            transform.translation + Vec3::Y * config::PLAYER_EYE_HEIGHT + dir * 1.2;
        info!("player dropped '{}'", item.name);
        spawn_world_item(
            &mut commands,
            item,
            position,
            dir * config::ITEM_DROP_SPEED,
        );
        send_inventory(&mut writer, owner.0, &inventory);
    }
}
