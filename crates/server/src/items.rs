//! Server-side items and inventories. World items are physics objects
//! replicated to everyone; inventory contents are private and only sent
//! to the owning client via `InventoryUpdate`.

use avian3d::{math::*, prelude::*};
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use shared::config;
use shared::items::{self, CREDITS};
use shared::protocol::{InventoryUpdate, Item, Player, PlayerName};

use crate::character::CharacterSystems;
use crate::level::spawn_world_item;
use crate::players::{LatestInput, PlayerOwner};
use crate::run::RunEntity;

pub struct ServerItemsPlugin;

impl Plugin for ServerItemsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (pickup_items, drop_items, map_holder_rules)
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
        (Entity, &Transform, &mut LatestInput, &mut Inventory, &PlayerOwner, &PlayerName),
        With<Player>,
    >,
    mut run: Query<&mut shared::run::RunState, With<RunEntity>>,
    mut writer: MessageWriter<ToClients<InventoryUpdate>>,
) {
    for (player, transform, mut input, mut inventory, owner, name) in &mut players {
        if !input.0.interact {
            continue;
        }
        input.0.interact = false;

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

        if item.id == CREDITS {
            if let Ok(mut run) = run.single_mut() {
                run.credits += item.value;
                info!("{} picked up {} credits (party: {})", name.0, item.value, run.credits);
            }
            commands.entity(body).despawn();
            continue;
        }

        if items::is_map(item) {
            if let Ok(mut run) = run.single_mut() {
                if let Some(holder) = &run.map_holder {
                    if holder != &name.0 {
                        info!("map already held by {holder}");
                        continue;
                    }
                } else {
                    run.map_holder = Some(name.0.clone());
                }
            }
        }

        let Some(free_slot) = inventory.0.iter().position(Option::is_none) else {
            info!("player {player} tried to pick up with a full inventory");
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
        (&Transform, &mut LatestInput, &mut Inventory, &PlayerOwner, &PlayerName),
        With<Player>,
    >,
    mut run: Query<&mut shared::run::RunState, With<RunEntity>>,
    mut writer: MessageWriter<ToClients<InventoryUpdate>>,
) {
    for (transform, mut input, mut inventory, owner, name) in &mut players {
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

        if items::is_map(&item) {
            if let Ok(mut run) = run.single_mut() {
                if run.map_holder.as_deref() == Some(name.0.as_str()) {
                    run.map_holder = None;
                }
            }
        }

        let dir = look_direction(input.0.yaw, input.0.pitch);
        let position =
            transform.translation + Vec3::Y * config::PLAYER_EYE_HEIGHT + dir * 1.2;
        info!("player dropped '{}'", item.name);
        spawn_world_item(
            &mut commands,
            item,
            position,
            dir * config::ITEM_DROP_SPEED,
            false,
        );
        send_inventory(&mut writer, owner.0, &inventory);
    }
}

/// Clear map holder if they no longer carry a map.
fn map_holder_rules(
    mut run: Query<&mut shared::run::RunState, With<RunEntity>>,
    players: Query<(&PlayerName, &Inventory), With<Player>>,
) {
    let Ok(mut run) = run.single_mut() else {
        return;
    };
    let Some(holder) = run.map_holder.clone() else {
        return;
    };
    let still_has = players.iter().any(|(name, inv)| {
        name.0 == holder && inv.0.iter().any(|s| s.as_ref().is_some_and(items::is_map))
    });
    if !still_has {
        run.map_holder = None;
    }
}
