//! Water volumes, buoyancy, wading, and splash events.

use std::collections::HashMap;

use avian3d::math::Vector;
use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use shared::config;
use shared::protocol::WaterImpact;

use crate::character::{CharacterController, Grounded, PlayerWaterContact};
use crate::level::LevelEntity;
use crate::players::LatestInput;

pub struct LiquidsPlugin;

#[derive(Default, Component, Debug)]
pub struct Liquid;

/// Identifies a water channel for client splash routing.
#[derive(Component, Debug, Clone, Copy)]
pub struct WaterChannel(pub u32);

#[derive(Component, Reflect, Debug)]
#[component(storage = "SparseSet")]
pub struct Submerged {
    pub entity: Entity,
    pub estimated_percent: f32,
    pub estimated_volume: f32,
    pub delta_percent: f32,
    pub delta_volume: f32,
}

impl Plugin for LiquidsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                track_player_water_contact,
                update_submerged,
                update_estimated_percent_and_volume,
            )
                .chain(),
        )
        .add_systems(
            FixedUpdate,
            (
                emit_water_impacts,
                player_water_footfalls,
                apply_submerged_drag,
                apply_buoyancy,
            )
                .chain()
                .in_set(PhysicsStepSystems::First),
        );
    }
}

fn track_player_water_contact(
    mut started: MessageReader<CollisionStart>,
    mut ended: MessageReader<CollisionEnd>,
    water: Query<&WaterChannel, With<Liquid>>,
    players: Query<(), With<CharacterController>>,
    mut contacts: Query<&mut PlayerWaterContact, With<CharacterController>>,
    mut writer: MessageWriter<ToClients<WaterImpact>>,
    transforms: Query<&Transform>,
    velocities: Query<&LinearVelocity>,
) {
    for event in started.read() {
        let (player, channel) = if water.get(event.collider1).is_ok() {
            let player = event.body2.unwrap_or(event.collider2);
            (player, water.get(event.collider1).unwrap())
        } else if water.get(event.collider2).is_ok() {
            let player = event.body1.unwrap_or(event.collider1);
            (player, water.get(event.collider2).unwrap())
        } else {
            continue;
        };
        if !players.contains(player) {
            continue;
        }
        let Ok(mut contact) = contacts.get_mut(player) else {
            continue;
        };
        let was_dry = contact.0 == 0;
        contact.0 += 1;
        if was_dry {
            let speed = velocities
                .get(player)
                .map(|v| v.0.length())
                .unwrap_or(0.0);
            if speed >= config::WATER_SPLASH_MIN_SPEED {
                let pos = transforms
                    .get(player)
                    .map(|t| t.translation)
                    .unwrap_or(Vec3::ZERO);
                writer.write(ToClients {
                    targets: SendTargets::All,
                    message: WaterImpact {
                        channel_id: channel.0,
                        position: pos,
                        impulse: speed,
                    },
                });
            }
        }
    }

    for event in ended.read() {
        let player = if water.contains(event.collider1) {
            event.body2.unwrap_or(event.collider2)
        } else if water.contains(event.collider2) {
            event.body1.unwrap_or(event.collider1)
        } else {
            continue;
        };
        if !players.contains(player) {
            continue;
        }
        let Ok(mut contact) = contacts.get_mut(player) else {
            continue;
        };
        contact.0 = contact.0.saturating_sub(1);
    }
}

fn update_submerged(
    mut commands: Commands,
    mut collision_started: MessageReader<CollisionStart>,
    mut collision_ended: MessageReader<CollisionEnd>,
    liquid_query: Query<&Liquid>,
    rigid_body_query: Query<&RigidBody>,
    submerged_query: Query<&Submerged>,
) {
    for event in collision_ended.read() {
        if liquid_query.contains(event.collider1)
            && submerged_query
                .get(event.body2.unwrap_or(event.collider2))
                .is_ok_and(|submerged| submerged.entity == event.collider1)
        {
            commands
                .entity(event.body2.unwrap_or(event.collider2))
                .remove::<Submerged>();
        } else if liquid_query.contains(event.collider2)
            && submerged_query
                .get(event.body1.unwrap_or(event.collider1))
                .is_ok_and(|submerged| submerged.entity == event.collider2)
        {
            commands
                .entity(event.body1.unwrap_or(event.collider1))
                .remove::<Submerged>();
        }
    }

    for event in collision_started.read() {
        if liquid_query.contains(event.collider1) {
            let body = event.body2.unwrap_or(event.collider2);
            if rigid_body_query.get(body).is_ok() {
                commands.entity(body).try_insert(Submerged {
                    entity: event.collider1,
                    estimated_percent: 0.0,
                    estimated_volume: 0.0,
                    delta_percent: 0.0,
                    delta_volume: 0.0,
                });
            }
        } else if liquid_query.contains(event.collider2) {
            let body = event.body1.unwrap_or(event.collider1);
            if rigid_body_query.get(body).is_ok() {
                commands.entity(body).try_insert(Submerged {
                    entity: event.collider2,
                    estimated_percent: 0.0,
                    estimated_volume: 0.0,
                    delta_percent: 0.0,
                    delta_volume: 0.0,
                });
            }
        }
    }
}

fn update_estimated_percent_and_volume(
    collisions: Collisions,
    mut submerged_query: Query<(Entity, &mut Submerged, &Collider, &GlobalTransform)>,
) {
    for (entity, mut submerged, collider, global_transform) in &mut submerged_query {
        let Some(pair) = collisions.get(entity, submerged.entity) else {
            continue;
        };

        let aabb = collider.aabb(global_transform.translation(), global_transform.rotation());
        let height = aabb.size().y.max(0.05);

        if let Some(contact) = pair.find_deepest_contact() {
            let estimated_percent = (contact.penetration / height).clamp(0.0, 1.0);
            let estimated_volume =
                collider.shape_scaled().mass_properties(1.0).mass() * estimated_percent;

            submerged.delta_percent = estimated_percent - submerged.estimated_percent;
            submerged.delta_volume = estimated_volume - submerged.estimated_volume;
            submerged.estimated_percent = estimated_percent;
            submerged.estimated_volume = estimated_volume;
        }
    }
}

fn apply_submerged_drag(
    time: Res<Time>,
    mut submerged_query: Query<
        (
            &RigidBody,
            &Submerged,
            &mut LinearVelocity,
            &mut AngularVelocity,
        ),
    >,
) {
    const DAMPING: f32 = 0.25;

    for (rigid_body, submerged, mut linear_velocity, mut angular_velocity) in &mut submerged_query {
        if !rigid_body.is_dynamic() {
            continue;
        }

        if submerged.delta_percent > 0.0 {
            linear_velocity.0 *= 1.0 - submerged.delta_percent;
        }

        let damping_factor = DAMPING;
        linear_velocity.0 *=
            damping_factor.powf(time.delta_secs() * submerged.estimated_percent);
        angular_velocity.0 *=
            damping_factor.powf(time.delta_secs() * submerged.estimated_percent);
    }
}

fn apply_buoyancy(
    submerged_query: Query<(&RigidBody, &Submerged, Forces)>,
    liquid_query: Query<&ColliderDensity, With<Liquid>>,
    gravity: Res<Gravity>,
) {
    for (rigid_body, submerged, mut forces) in submerged_query {
        if !rigid_body.is_dynamic() {
            continue;
        }

        let Ok(liquid_density) = liquid_query.get(submerged.entity) else {
            continue;
        };

        let buoyancy = liquid_density.0 * submerged.estimated_volume;
        forces.apply_force(Vector::from(-gravity.0) * buoyancy);
    }
}

fn emit_water_impacts(
    mut started: MessageReader<CollisionStart>,
    water: Query<&WaterChannel, With<Liquid>>,
    bodies: Query<(&LinearVelocity, &Transform), (With<RigidBody>, Without<CharacterController>)>,
    mut writer: MessageWriter<ToClients<WaterImpact>>,
) {
    for event in started.read() {
        let (channel_id, body) = if let Ok(ch) = water.get(event.collider1) {
            (ch.0, event.body2.unwrap_or(event.collider2))
        } else if let Ok(ch) = water.get(event.collider2) {
            (ch.0, event.body1.unwrap_or(event.collider1))
        } else {
            continue;
        };

        let Ok((velocity, transform)) = bodies.get(body) else {
            continue;
        };
        let speed = velocity.0.length();
        if speed < config::WATER_SPLASH_MIN_SPEED {
            continue;
        }
        writer.write(ToClients {
            targets: SendTargets::All,
            message: WaterImpact {
                channel_id,
                position: transform.translation,
                impulse: speed,
            },
        });
    }
}

fn player_water_footfalls(
    time: Res<Time>,
    mut timers: Local<HashMap<Entity, f32>>,
    players: Query<
        (
            Entity,
            &Transform,
            &PlayerWaterContact,
            &LatestInput,
            Has<Grounded>,
        ),
        With<CharacterController>,
    >,
    mut writer: MessageWriter<ToClients<WaterImpact>>,
) {
    for (entity, transform, contact, input, grounded) in &players {
        if contact.0 == 0 || !grounded || input.0.move_dir.length_squared() < 0.01 {
            timers.remove(&entity);
            continue;
        }
        let t = timers.entry(entity).or_insert(0.0);
        *t += time.delta_secs();
        if *t >= config::WATER_FOOTFALL_INTERVAL {
            *t = 0.0;
            writer.write(ToClients {
                targets: SendTargets::All,
                message: WaterImpact {
                    channel_id: 0,
                    position: transform.translation,
                    impulse: 1.2,
                },
            });
        }
    }
}

/// Spawn a thin sensor volume for a `SewerWater` static.
pub fn spawn_water_volume(
    commands: &mut Commands,
    def: &shared::level::StaticDef,
    channel_id: u32,
) {
    commands.spawn((
        LevelEntity,
        Liquid,
        WaterChannel(channel_id),
        RigidBody::Static,
        Collider::cuboid(def.size.x, 0.6, def.size.z),
        ColliderDensity(config::WATER_DENSITY),
        Sensor,
        CollisionEventsEnabled,
        Transform::from_translation(def.position + Vec3::Y * 0.3),
    ));
}
