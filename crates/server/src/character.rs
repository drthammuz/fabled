//! Kinematic character controller (Avian move-and-slide). Handles
//! acceleration-based movement, gravity, wall sliding, and pushing
//! dynamic props — all server-side.

use std::f32::consts::PI;

use avian3d::{math::*, prelude::*};
use bevy::ecs::query::Has;
use bevy::prelude::*;
use shared::config;
use bevy_replicon::prelude::*;
use shared::protocol::PlayerInput;

use super::players::LatestInput;

pub struct CharacterControllerPlugin;

impl Plugin for CharacterControllerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                apply_player_inputs,
                update_grounded,
                steer_from_input,
                apply_gravity,
                apply_movement_damping,
                move_and_slide,
                push_dynamic_bodies,
            )
                .chain()
                .run_if(in_state(ClientState::Disconnected)),
        );
    }
}

/// Marker: this entity uses manual move-and-slide instead of dynamic physics.
#[derive(Component)]
#[require(
    RigidBody::Kinematic,
    CustomPositionIntegration,
    SpeculativeMargin(0.0),
)]
pub struct CharacterController;

#[derive(Component, Default, Deref)]
pub struct CharacterCollisions(pub Vec<CharacterCollision>);

pub struct CharacterCollision {
    pub collider: Entity,
    pub point: Vector,
    pub normal: Dir3,
    pub character_velocity: Vector,
}

/// Set when a walkable surface is detected below the capsule.
#[derive(Component)]
pub struct Grounded;

#[derive(Component)]
pub struct GroundDetection {
    pub max_angle: Scalar,
    pub max_distance: Scalar,
    pub cast_shape: Collider,
}

impl Default for GroundDetection {
    fn default() -> Self {
        Self {
            max_angle: PI / 6.0,
            max_distance: config::PLAYER_GROUND_PROBE,
            cast_shape: Collider::capsule(
                config::PLAYER_CAPSULE_RADIUS,
                config::PLAYER_CAPSULE_LENGTH,
            ),
        }
    }
}

fn apply_player_inputs(
    mut inputs: MessageReader<FromClient<PlayerInput>>,
    mut players: Query<(&super::players::PlayerOwner, &mut LatestInput)>,
) {
    for FromClient { client_id, message } in inputs.read() {
        for (owner, mut latest) in &mut players {
            if owner.0 == *client_id {
                let jump = latest.0.jump || message.jump;
                let throw_action = latest.0.throw_action || message.throw_action;
                latest.0 = *message;
                latest.0.jump = jump;
                latest.0.throw_action = throw_action;
            }
        }
    }
}

fn update_grounded(
    mut commands: Commands,
    query: Query<(Entity, &GroundDetection, &GlobalTransform), With<CharacterController>>,
    spatial: SpatialQuery,
) {
    for (entity, ground, transform) in &query {
        let translation = transform.translation().adjust_precision();
        let rotation = transform.rotation().adjust_precision();
        let hit = spatial.cast_shape(
            &ground.cast_shape,
            translation,
            rotation,
            Dir3::NEG_Y,
            &ShapeCastConfig::from_max_distance(ground.max_distance),
            &SpatialQueryFilter::from_excluded_entities([entity]),
        );
        let up = transform.up().adjust_precision();
        let is_grounded = hit.is_some_and(|hit| {
            (rotation * hit.normal1).angle_between(up) <= ground.max_angle
        });
        if is_grounded {
            commands.entity(entity).insert(Grounded);
        } else {
            commands.entity(entity).remove::<Grounded>();
        }
    }
}

/// Accelerate toward the input target speed; jump when grounded.
fn steer_from_input(
    time: Res<Time>,
    mut players: Query<
        (
            &mut LatestInput,
            &mut LinearVelocity,
            Has<Grounded>,
        ),
        With<CharacterController>,
    >,
) {
    let dt = time.delta_secs_f64().adjust_precision();
    for (mut input, mut velocity, grounded) in &mut players {
        let intent = &input.0;
        let move_dir = if intent.move_dir.length_squared() > 1.0 {
            intent.move_dir.normalize()
        } else {
            intent.move_dir
        };
        let world_dir = Quat::from_rotation_y(intent.yaw)
            * Vec3::new(move_dir.x, 0.0, -move_dir.y);
        let move_speed = if intent.sprint {
            config::PLAYER_SPRINT_SPEED
        } else {
            config::PLAYER_MOVE_SPEED
        };
        let has_move_input = move_dir.length_squared() > 0.0;
        let horizontal = Vector::new(velocity.x, 0.0, velocity.z);
        let desired_h = if has_move_input {
            Vector::new(
                world_dir.x * move_speed,
                0.0,
                world_dir.z * move_speed,
            )
        } else {
            horizontal
        };
        let accel = if grounded {
            config::PLAYER_ACCELERATION
        } else {
            config::PLAYER_ACCELERATION * config::PLAYER_AIR_CONTROL
        };
        let delta = (desired_h - horizontal).clamp_length_max(accel * dt);
        velocity.x += delta.x;
        velocity.z += delta.z;

        if intent.jump && grounded {
            velocity.y = config::PLAYER_JUMP_IMPULSE;
            input.0.jump = false;
        }
    }
}

fn apply_gravity(
    time: Res<Time>,
    mut query: Query<&mut LinearVelocity, With<CharacterController>>,
) {
    let dt = time.delta_secs_f64().adjust_precision();
    let gravity = Vector::Y * config::PLAYER_GRAVITY;
    for mut velocity in &mut query {
        let fall_speed = (-velocity.y).max(0.0);
        if fall_speed < config::PLAYER_TERMINAL_VELOCITY {
            velocity.0 += gravity * dt;
        } else {
            velocity.y = -config::PLAYER_TERMINAL_VELOCITY;
        }
    }
}

fn apply_movement_damping(
    time: Res<Time>,
    mut query: Query<(&LatestInput, Has<Grounded>, &mut LinearVelocity), With<CharacterController>>,
) {
    let dt = time.delta_secs_f64().adjust_precision();
    for (input, grounded, mut velocity) in &mut query {
        let has_move_input = input.0.move_dir.length_squared() > 0.0;
        if grounded {
            if has_move_input {
                continue;
            }
            let factor = 1.0 / (1.0 + dt * config::PLAYER_MOVE_DAMPING);
            velocity.x *= factor;
            velocity.z *= factor;
        } else if !has_move_input {
            // Light friction in air when coasting — preserves jump momentum.
            let factor = 1.0 / (1.0 + dt * config::PLAYER_AIR_FRICTION);
            velocity.x *= factor;
            velocity.z *= factor;
        }
    }
}

fn move_and_slide(
    mut query: Query<
        (
            Entity,
            &GroundDetection,
            &mut CharacterCollisions,
            &mut Transform,
            &mut LinearVelocity,
            &Collider,
        ),
        With<CharacterController>,
    >,
    move_and_slide: MoveAndSlide,
    time: Res<Time>,
) {
    for (entity, ground, mut collisions, mut transform, mut lin_vel, collider) in &mut query {
        collisions.0.clear();
        let up = transform.up().adjust_precision();
        let mut hit_ground_or_ceiling = false;

        let MoveAndSlideOutput {
            position: new_position,
            projected_velocity,
        } = move_and_slide.move_and_slide(
            collider,
            transform.translation.adjust_precision(),
            transform.rotation.adjust_precision(),
            lin_vel.0,
            time.delta(),
            &MoveAndSlideConfig::default(),
            &SpatialQueryFilter::from_excluded_entities([entity]),
            |hit| {
                let angle = up.angle_between(hit.normal.adjust_precision());
                let is_ground = angle <= ground.max_angle;
                let is_ceiling = is_ground && up.dot(hit.normal.adjust_precision()) < 0.0;
                if is_ground || is_ceiling {
                    hit_ground_or_ceiling = true;
                }
                collisions.0.push(CharacterCollision {
                    collider: hit.entity,
                    point: hit.point,
                    normal: *hit.normal,
                    character_velocity: *hit.velocity,
                });
                MoveAndSlideHitResponse::Accept
            },
        );

        transform.translation = new_position.f32();
        if hit_ground_or_ceiling {
            let velocity_along_up = lin_vel.dot(up);
            let new_velocity_along_up = projected_velocity.dot(up);
            lin_vel.0 += (new_velocity_along_up - velocity_along_up) * up;
        }
    }
}

/// Impart momentum to dynamic bodies the character walked into.
fn push_dynamic_bodies(
    characters: Query<(&ComputedMass, &CharacterCollisions), With<CharacterController>>,
    colliders: Query<&ColliderOf>,
    mut bodies: Query<(&RigidBody, Forces)>,
) {
    for (mass, collisions) in &characters {
        let mass = mass.value();
        for collision in &collisions.0 {
            let Ok(collider_of) = colliders.get(collision.collider) else {
                continue;
            };
            let Ok((body, mut forces)) = bodies.get_mut(collider_of.body) else {
                continue;
            };
            if !body.is_dynamic() {
                continue;
            }
            let touch_dir = -collision.normal.adjust_precision();
            let relative = collision.character_velocity - forces.linear_velocity();
            let touch_velocity = touch_dir.dot(relative) * touch_dir;
            // Full player mass shoves light crates violently; scale it down.
            let impulse = touch_velocity * mass * config::PLAYER_PUSH_FACTOR;
            forces.apply_linear_impulse_at_point(impulse, collision.point);
        }
    }
}
