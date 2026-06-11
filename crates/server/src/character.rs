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

/// Label for the input + movement chain so other server systems (grab,
/// items) can order themselves after fresh input has been applied.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CharacterSystems;

impl Plugin for CharacterControllerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                apply_player_inputs,
                update_grounded,
                ground_move,
                apply_gravity,
                move_and_slide,
                push_dynamic_bodies,
            )
                .chain()
                .in_set(CharacterSystems)
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

/// Server-side validation: never trust the client. Returns `None` when the
/// message is malformed (NaN/infinite floats); otherwise returns a copy with
/// every field clamped to legal ranges.
fn sanitize_input(message: &PlayerInput) -> Option<PlayerInput> {
    if !message.move_dir.is_finite()
        || !message.yaw.is_finite()
        || !message.pitch.is_finite()
    {
        return None;
    }
    let mut input = *message;
    // A hacked client could send move_dir of length 100 for super-speed.
    input.move_dir = input.move_dir.clamp_length_max(1.0);
    input.pitch = input
        .pitch
        .clamp(-config::PLAYER_MAX_PITCH, config::PLAYER_MAX_PITCH);
    if let Some(slot) = input.drop_slot {
        if slot as usize >= config::INVENTORY_SLOTS {
            input.drop_slot = None;
        }
    }
    Some(input)
}

fn apply_player_inputs(
    mut inputs: MessageReader<FromClient<PlayerInput>>,
    mut players: Query<(&super::players::PlayerOwner, &mut LatestInput)>,
) {
    for FromClient { client_id, message } in inputs.read() {
        let Some(message) = sanitize_input(message) else {
            warn_once!("rejected malformed input from {client_id:?}");
            continue;
        };
        for (owner, mut latest) in &mut players {
            if owner.0 == *client_id {
                // Edge-triggered actions stay latched until a system consumes
                // them; otherwise a press could fall between fixed ticks.
                let jump = latest.0.jump || message.jump;
                let throw_action = latest.0.throw_action || message.throw_action;
                let interact = latest.0.interact || message.interact;
                let drop_slot = message.drop_slot.or(latest.0.drop_slot);
                latest.0 = message;
                latest.0.jump = jump;
                latest.0.throw_action = throw_action;
                latest.0.interact = interact;
                latest.0.drop_slot = drop_slot;
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

/// Quake/Source-style ground movement — the long-standing standard for
/// grippy FPS character control:
/// 1. While grounded, friction always removes a fraction of speed.
/// 2. Acceleration adds speed along the wish direction, capped so the
///    velocity component in that direction never exceeds wish speed.
/// Friction kills the old direction quickly while acceleration rebuilds
/// the new one, which is what makes turns feel planted. In the air there
/// is no friction (momentum is preserved) and only weak steering.
fn ground_move(
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
        let wish_dir = Quat::from_rotation_y(intent.yaw)
            * Vec3::new(move_dir.x, 0.0, -move_dir.y);
        let wish_speed = if intent.sprint {
            config::PLAYER_SPRINT_SPEED
        } else {
            config::PLAYER_MOVE_SPEED
        };

        // 1) Friction (grounded only).
        if grounded {
            let horizontal = Vector::new(velocity.x, 0.0, velocity.z);
            let speed = horizontal.length();
            if speed > 1e-4 {
                let drop = speed * config::PLAYER_FRICTION * dt;
                let scale = ((speed - drop).max(0.0)) / speed;
                velocity.x *= scale;
                velocity.z *= scale;
            }
        }

        // 2) Accelerate toward the wish direction.
        if let Some(wish_dir) = wish_dir.try_normalize() {
            let accel_rate = if grounded {
                config::PLAYER_ACCEL_RATE
            } else {
                config::PLAYER_AIR_ACCEL_RATE
            };
            let horizontal = Vector::new(velocity.x, 0.0, velocity.z);
            let current = horizontal.dot(wish_dir.adjust_precision());
            let add = (wish_speed - current)
                .clamp(0.0, accel_rate * wish_speed * dt);
            velocity.x += wish_dir.x * add;
            velocity.z += wish_dir.z * add;
        }

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
///
/// Uses the reduced mass of the player/body pair — the standard collision-
/// response formula. Light objects (items, small crates) receive at most
/// roughly the contact velocity instead of being launched at dozens of m/s,
/// while heavy objects still feel heavy.
fn push_dynamic_bodies(
    characters: Query<(&ComputedMass, &CharacterCollisions), With<CharacterController>>,
    colliders: Query<&ColliderOf>,
    mut bodies: Query<(
        &RigidBody,
        &ComputedMass,
        Forces,
        Has<shared::protocol::Item>,
    )>,
) {
    for (player_mass, collisions) in &characters {
        let player_mass = player_mass.value();
        for collision in &collisions.0 {
            let Ok(collider_of) = colliders.get(collision.collider) else {
                continue;
            };
            let Ok((body, body_mass, mut forces, is_item)) =
                bodies.get_mut(collider_of.body)
            else {
                continue;
            };
            // Items are collected (E), never kicked around by walking into
            // them — walking over loot used to punt it through the floor.
            if !body.is_dynamic() || is_item {
                continue;
            }
            let touch_dir = -collision.normal.adjust_precision();
            let relative = collision.character_velocity - forces.linear_velocity();
            let touch_velocity = touch_dir.dot(relative) * touch_dir;
            let body_mass = body_mass.value();
            let reduced_mass = player_mass * body_mass / (player_mass + body_mass);
            let impulse = touch_velocity * reduced_mass * config::PLAYER_PUSH_STRENGTH;
            forces.apply_linear_impulse_at_point(impulse, collision.point);
        }
    }
}
