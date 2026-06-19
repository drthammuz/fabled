//! Quake III-style kinematic character controller (Avian move-and-slide).
//!
//! Pipeline each fixed tick (mirrors `PmoveSingle` in `bg_pmove.c`):
//!   1. `PM_GroundTrace` — short downward cast, no body teleport
//!   2. Ground/air acceleration, friction, jump
//!   3. Gravity when airborne
//!   4. `PM_StepSlideMove` — slide; step-up **only when blocked**
//!   5. `PM_GroundTrace` again — refresh grounded flag, still no teleport
//!
//! Uses an AABB hull (not a capsule) so stair treads and trap-door lips do not
//! “roll” the player off edges.

use std::f32::consts::PI;

use avian3d::{math::*, prelude::*};
use bevy::ecs::query::Has;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use shared::config;
use shared::protocol::{PlayerGrounded, PlayerInput};

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
                enter_crouch,
                try_stand_up,
                categorize_position,
                player_movement,
                apply_gravity,
                step_slide_move,
                ground_trace_after_move,
                sync_grounded_component,
                push_dynamic_bodies,
                rotate_to_yaw,
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

/// Replicated marker — kept in sync from [`GroundContact`] for other systems.
#[derive(Component)]
pub struct Grounded;

/// Authoritative ground state for this tick (updated synchronously, not via
/// deferred `Commands`). Movement reads this and live floor probes.
#[derive(Component, Default)]
pub struct GroundContact {
    pub on_ground: bool,
}

/// Brief window after leaving the ground where jump still works.
#[derive(Component)]
pub struct CoyoteTime(pub f32);

/// Per-player speed multiplier (1.0 = base). Scout uses 1.25.
#[derive(Component, Clone, Copy)]
pub struct SpeedMultiplier(pub f32);

impl Default for SpeedMultiplier {
    fn default() -> Self { Self(1.0) }
}

/// Tracks crouch state and drives capsule resizing.
#[derive(Component, Default)]
pub struct CrouchState {
    pub crouching: bool,
}

/// Ref-count of overlapping water sensor volumes (wading).
#[derive(Component, Default, Debug)]
pub struct PlayerWaterContact(pub u32);

fn standing_collider() -> Collider {
    Collider::cuboid(
        config::PLAYER_BODY_WIDTH,
        config::PLAYER_BODY_HEIGHT,
        config::PLAYER_BODY_WIDTH,
    )
}

fn crouch_collider() -> Collider {
    let h = config::PLAYER_CROUCH_LENGTH + config::PLAYER_BODY_WIDTH;
    Collider::cuboid(config::PLAYER_BODY_WIDTH, h, config::PLAYER_BODY_WIDTH)
}

/// Vertical body shift on crouch/stand: half the collider height difference.
const CROUCH_Y_SHIFT: f32 =
    (config::PLAYER_BODY_HEIGHT - (config::PLAYER_CROUCH_LENGTH + config::PLAYER_BODY_WIDTH)) * 0.5;

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
            max_distance: config::PLAYER_GROUND_TRACE_DIST,
            cast_shape: standing_collider(),
        }
    }
}

/// Quake `PM_GroundTrace`: short downward cast. Does **not** move the body —
/// only reports whether walkable ground is directly under the hull.
fn ground_trace(
    spatial: &SpatialQuery,
    collider: &Collider,
    pos: Vector,
    rot: Quat,
    velocity: Vector,
    exclude: &[Entity],
) -> bool {
    if velocity.y > config::PLAYER_JUMP_GROUND_CUTOFF {
        return false;
    }

    let up = (rot * Vector::Y).normalize_or_zero();
    let filter = SpatialQueryFilter::from_excluded_entities(exclude.to_vec());
    let Some(hit) = spatial.cast_shape(
        collider,
        pos,
        rot,
        Dir3::NEG_Y,
        &ShapeCastConfig::from_max_distance(config::PLAYER_GROUND_TRACE_DIST),
        &filter,
    ) else {
        return false;
    };

    let normal = (rot * hit.normal1).normalize_or_zero();
    if normal.dot(up) < config::PLAYER_MIN_WALK_NORMAL {
        return false;
    }

    // Quake kickoff: moving up and away from the surface → leave the ground.
    if velocity.y > 0.0 && velocity.dot(normal) > config::PLAYER_GROUND_KICKOFF_SPEED {
        return false;
    }

    true
}

fn update_ground_contact(
    on_ground: bool,
    dt: f32,
    contact: &mut GroundContact,
    coyote: &mut CoyoteTime,
) {
    if on_ground {
        contact.on_ground = true;
        coyote.0 = config::PLAYER_COYOTE_TIME;
    } else {
        contact.on_ground = false;
        coyote.0 = (coyote.0 - dt).max(0.0);
    }
}

/// Pre-move ground trace (Quake calls `PM_GroundTrace` before `PM_WalkMove`).
fn categorize_position(
    spatial: SpatialQuery,
    liquids: Query<Entity, With<crate::liquids::Liquid>>,
    time: Res<Time>,
    mut players: Query<
        (
            Entity,
            &Collider,
            &Transform,
            &LinearVelocity,
            &mut GroundContact,
            &mut CoyoteTime,
            &PlayerWaterContact,
        ),
        With<CharacterController>,
    >,
) {
    let dt = time.delta_secs();
    let liquid_ents: Vec<Entity> = liquids.iter().collect();

    for (entity, collider, transform, velocity, mut contact, mut coyote, water) in &mut players
    {
        let pos = transform.translation.adjust_precision();
        let rot = transform.rotation.adjust_precision();
        let mut exclude = liquid_ents.clone();
        exclude.push(entity);

        let mut on_ground = ground_trace(
            &spatial,
            collider,
            pos,
            rot,
            velocity.0,
            &exclude,
        );

        // Wading: require feet near the bed, not just within trace distance of it.
        if on_ground && water.0 > 0 {
            on_ground = spatial
                .cast_shape(
                    collider,
                    pos,
                    rot,
                    Dir3::NEG_Y,
                    &ShapeCastConfig::from_max_distance(config::PLAYER_WADE_GROUND_PROBE),
                    &SpatialQueryFilter::from_excluded_entities(exclude),
                )
                .is_some();
        }

        update_ground_contact(on_ground, dt, &mut contact, &mut coyote);
    }
}

fn enter_crouch(
    mut players: Query<
        (
            &LatestInput,
            &mut CrouchState,
            &mut Collider,
            &mut GroundDetection,
            &mut Transform,
        ),
        With<CharacterController>,
    >,
) {
    for (input, mut crouch, mut collider, mut ground, mut transform) in &mut players {
        if input.0.crouch && !crouch.crouching {
            crouch.crouching = true;
            *collider = crouch_collider();
            ground.cast_shape = crouch_collider();
            transform.translation.y -= CROUCH_Y_SHIFT;
        }
    }
}

fn try_stand_up(
    mut param_set: ParamSet<(
        Query<
            (
                Entity,
                &LatestInput,
                &CrouchState,
                &Transform,
            ),
            With<CharacterController>,
        >,
        SpatialQuery,
        Query<
            (
                Entity,
                &mut CrouchState,
                &mut Collider,
                &mut GroundDetection,
                &mut Transform,
            ),
            With<CharacterController>,
        >,
    )>,
) {
    let candidates: Vec<(Entity, Vec3, Quat)> = param_set
        .p0()
        .iter()
        .filter_map(|(entity, input, crouch, transform)| {
            if input.0.crouch || !crouch.crouching {
                None
            } else {
                Some((entity, transform.translation, transform.rotation))
            }
        })
        .collect();

    let can_stand: Vec<Entity> = {
        let spatial = param_set.p1();
        candidates
            .into_iter()
            .filter(|(entity, translation, rotation)| {
                spatial
                    .cast_shape(
                        &standing_collider(),
                        translation.adjust_precision(),
                        rotation.adjust_precision(),
                        Dir3::Y,
                        &ShapeCastConfig::from_max_distance(config::PLAYER_STAND_UP_CLEARANCE),
                        &SpatialQueryFilter::from_excluded_entities([*entity]),
                    )
                    .is_none()
            })
            .map(|(entity, _, _)| entity)
            .collect()
    };

    for (entity, mut crouch, mut collider, mut ground, mut transform) in param_set.p2().iter_mut() {
        if can_stand.contains(&entity) {
            crouch.crouching = false;
            *collider = standing_collider();
            ground.cast_shape = standing_collider();
            transform.translation.y += CROUCH_Y_SHIFT;
        }
    }
}

fn sanitize_input(message: &PlayerInput) -> Option<PlayerInput> {
    if !message.move_dir.is_finite()
        || !message.yaw.is_finite()
        || !message.pitch.is_finite()
    {
        return None;
    }
    let mut input = *message;
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

fn sync_grounded_component(
    mut commands: Commands,
    query: Query<(Entity, &GroundContact), With<CharacterController>>,
) {
    for (entity, contact) in &query {
        if contact.on_ground {
            commands.entity(entity).insert(Grounded);
        } else {
            commands.entity(entity).remove::<Grounded>();
        }
        commands
            .entity(entity)
            .insert(PlayerGrounded(contact.on_ground));
    }
}

/// Quake ground movement: friction (grounded) → accelerate → jump. Only shapes
/// velocity; collision response happens in [`step_slide_move`].
fn player_movement(
    time: Res<Time>,
    mut players: Query<
        (
            &mut LatestInput,
            &mut LinearVelocity,
            &mut GroundContact,
            &mut CoyoteTime,
            &SpeedMultiplier,
            &PlayerWaterContact,
        ),
        With<CharacterController>,
    >,
) {
    let dt = time.delta_secs_f64().adjust_precision();

    for (mut input, mut velocity, mut contact, mut coyote, speed_mult, water) in &mut players {
        let intent = input.0;
        let on_ground = contact.on_ground;
        let wading = water.0 > 0;

        if on_ground && velocity.y <= 0.0 {
            velocity.y = 0.0;
        }

        let move_dir = if intent.move_dir.length_squared() > 1.0 {
            intent.move_dir.normalize()
        } else {
            intent.move_dir
        };
        let wish_dir = Quat::from_rotation_y(intent.yaw) * Vec3::new(move_dir.x, 0.0, -move_dir.y);
        let wish_speed = (if intent.crouch {
            config::PLAYER_CROUCH_SPEED
        } else if intent.sprint {
            config::PLAYER_SPRINT_SPEED
        } else {
            config::PLAYER_MOVE_SPEED
        }) * speed_mult.0
            * if wading {
                config::PLAYER_WADE_SPEED_MULT
            } else {
                1.0
            };

        if on_ground {
            let horizontal = Vector::new(velocity.x, 0.0, velocity.z);
            let speed = horizontal.length();
            if speed > 1e-4 {
                let drop = speed * config::PLAYER_FRICTION * dt;
                let scale = ((speed - drop).max(0.0)) / speed;
                velocity.x *= scale;
                velocity.z *= scale;
            }
        }

        if let Some(wish_dir) = wish_dir.try_normalize() {
            let accel_rate = if on_ground {
                config::PLAYER_ACCEL_RATE
            } else {
                config::PLAYER_AIR_ACCEL_RATE
            };
            let horizontal = Vector::new(velocity.x, 0.0, velocity.z);
            let current = horizontal.dot(wish_dir.adjust_precision());
            let add = (wish_speed - current).clamp(0.0, accel_rate * wish_speed * dt);
            velocity.x += wish_dir.x * add;
            velocity.z += wish_dir.z * add;
        }

        let can_jump = on_ground || coyote.0 > 0.0;
        if intent.jump && can_jump && !intent.crouch {
            velocity.y = config::PLAYER_JUMP_IMPULSE;
            input.0.jump = false;
            contact.on_ground = false;
            coyote.0 = 0.0;
        }
    }
}

fn apply_gravity(
    time: Res<Time>,
    mut query: Query<(&mut LinearVelocity, &GroundContact), With<CharacterController>>,
) {
    let dt = time.delta_secs_f64().adjust_precision();
    let gravity = Vector::Y * config::PLAYER_GRAVITY;
    for (mut velocity, contact) in &mut query {
        if contact.on_ground && velocity.y <= 0.0 {
            velocity.y = 0.0;
            continue;
        }
        let fall_speed = (-velocity.y).max(0.0);
        if fall_speed < config::PLAYER_TERMINAL_VELOCITY {
            velocity.0 += gravity * dt;
        } else {
            velocity.y = -config::PLAYER_TERMINAL_VELOCITY;
        }
    }
}

/// Horizontal (up-plane) length of a displacement vector.
fn horizontal_len(v: Vector, up: Vector) -> Scalar {
    (v - up * v.dot(up)).length()
}

/// Horizontal wish velocity from player input (same basis as [`player_movement`]).
fn wish_horizontal(input: &PlayerInput, speed: f32) -> Vector {
    let move_dir = if input.move_dir.length_squared() > 1.0 {
        input.move_dir.normalize()
    } else {
        input.move_dir
    };
    let wish = Quat::from_rotation_y(input.yaw) * Vec3::new(move_dir.x, 0.0, -move_dir.y);
    wish.try_normalize()
        .map(|d| d.adjust_precision() * speed)
        .unwrap_or(Vector::ZERO)
}

/// Quake `PM_StepSlideMove`: slide first; if horizontal travel is blocked, lift by
/// [`config::PLAYER_STEP_HEIGHT`], slide again, then drop back down. Step-up runs
/// **only when blocked**, not every grounded frame. No post-move Y snap — position
/// comes entirely from slide + step-down planting (like Q3 `bg_slidemove.c`).
fn step_slide_move(
    mut query: Query<
        (
            Entity,
            &mut CharacterCollisions,
            &mut Transform,
            &mut LinearVelocity,
            &Collider,
            &GroundContact,
            &LatestInput,
            &SpeedMultiplier,
        ),
        With<CharacterController>,
    >,
    move_and_slide: MoveAndSlide,
    time: Res<Time>,
) {
    let dt = time.delta();
    let mut cfg = MoveAndSlideConfig::default();
    cfg.skin_width = 0.02;
    let step = config::PLAYER_STEP_HEIGHT;

    for (entity, mut collisions, mut transform, mut lin_vel, collider, contact, input, speed_mult) in
        &mut query
    {
        collisions.0.clear();
        let up = transform.up().adjust_precision();
        let start = transform.translation.adjust_precision();
        let start_vel = lin_vel.0;
        let rot = transform.rotation.adjust_precision();
        let filter = SpatialQueryFilter::from_excluded_entities([entity]);

        let on_hit = |hit: MoveAndSlideHitData<'_>| {
            collisions.0.push(CharacterCollision {
                collider: hit.entity,
                point: hit.point,
                normal: *hit.normal,
                character_velocity: *hit.velocity,
            });
            MoveAndSlideHitResponse::Accept
        };

        // 1. Primary slide (Quake `PM_SlideMove`).
        let flat = move_and_slide.move_and_slide(
            collider,
            start,
            rot,
            start_vel,
            dt,
            &cfg,
            &filter,
            on_hit,
        );

        let mut final_pos = flat.position;
        let mut final_vel = flat.projected_velocity;

        let intent = input.0;
        let wish_speed = (if intent.crouch {
            config::PLAYER_CROUCH_SPEED
        } else if intent.sprint {
            config::PLAYER_SPRINT_SPEED
        } else {
            config::PLAYER_MOVE_SPEED
        }) * speed_mult.0;
        let horizontal_vel = Vector::new(start_vel.x, 0.0, start_vel.z);
        let mut step_vel = if horizontal_vel.length() > 0.05 {
            horizontal_vel
        } else {
            wish_horizontal(&intent, wish_speed)
        };

        let dt_s = time.delta_secs_f64().adjust_precision();
        let intended_h = horizontal_len(start_vel * dt_s, up);
        let actual_h = horizontal_len(flat.position - start, up);
        let blocked = intended_h > 0.02
            && actual_h < intended_h * config::PLAYER_STEP_BLOCKED_FRAC;

        // 2. Step-up only when the first slide did not reach its destination.
        if contact.on_ground && blocked && start_vel.y <= 0.0 && step_vel.length() > 0.05 {
            // Quake: never step while moving up (jump / kickoff).
            let headroom = move_and_slide
                .spatial_query
                .cast_shape(
                    collider,
                    start,
                    rot,
                    Dir3::Y,
                    &ShapeCastConfig::from_max_distance(step),
                    &filter,
                )
                .map(|h| h.distance)
                .unwrap_or(step);

            if headroom >= 0.05 {
                let lift = headroom.min(step);
                let elevated = start + up * lift;

                let stepped = move_and_slide.move_and_slide(
                    collider,
                    elevated,
                    rot,
                    step_vel,
                    dt,
                    &cfg,
                    &filter,
                    |_| MoveAndSlideHitResponse::Accept,
                );

                if let Some(down) = move_and_slide.spatial_query.cast_shape(
                    collider,
                    stepped.position,
                    rot,
                    Dir3::NEG_Y,
                    &ShapeCastConfig::from_max_distance(lift + 0.05),
                    &filter,
                ) {
                    let normal = (rot * down.normal1).normalize_or_zero();
                    if normal.dot(up) >= config::PLAYER_MIN_WALK_NORMAL {
                        final_pos = stepped.position - up * down.distance;
                        final_vel = Vector::new(
                            stepped.projected_velocity.x,
                            flat.projected_velocity.y,
                            stepped.projected_velocity.z,
                        );
                    }
                }
            }
        }

        transform.translation = final_pos.f32();
        lin_vel.0 = final_vel;
    }
}

/// Post-move ground refresh (Quake calls `PM_GroundTrace` again after the move).
/// Updates grounded state only — **never** teleports the body.
fn ground_trace_after_move(
    spatial: SpatialQuery,
    liquids: Query<Entity, With<crate::liquids::Liquid>>,
    time: Res<Time>,
    mut players: Query<
        (
            Entity,
            &Collider,
            &Transform,
            &LinearVelocity,
            &mut GroundContact,
            &mut CoyoteTime,
            &PlayerWaterContact,
        ),
        With<CharacterController>,
    >,
) {
    let dt = time.delta_secs();
    let liquid_ents: Vec<Entity> = liquids.iter().collect();

    for (entity, collider, transform, velocity, mut contact, mut coyote, water) in &mut players
    {
        let pos = transform.translation.adjust_precision();
        let rot = transform.rotation.adjust_precision();
        let mut exclude = liquid_ents.clone();
        exclude.push(entity);

        let mut on_ground = ground_trace(
            &spatial,
            collider,
            pos,
            rot,
            velocity.0,
            &exclude,
        );

        if on_ground && water.0 > 0 {
            on_ground = spatial
                .cast_shape(
                    collider,
                    pos,
                    rot,
                    Dir3::NEG_Y,
                    &ShapeCastConfig::from_max_distance(config::PLAYER_WADE_GROUND_PROBE),
                    &SpatialQueryFilter::from_excluded_entities(exclude),
                )
                .is_some();
        }

        update_ground_contact(on_ground, dt, &mut contact, &mut coyote);
    }
}

fn rotate_to_yaw(
    mut players: Query<(&LatestInput, &mut Transform), With<CharacterController>>,
) {
    for (input, mut transform) in &mut players {
        transform.rotation = Quat::from_rotation_y(input.0.yaw);
    }
}

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
