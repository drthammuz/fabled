//! Kinematic character controller (Avian move-and-slide). Handles
//! acceleration-based movement, gravity, wall sliding, and pushing
//! dynamic props — all server-side.

use std::f32::consts::PI;

use avian3d::{math::*, prelude::*};
use bevy::ecs::query::Has;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use shared::config;
use shared::kenney_pit;
use shared::protocol::{PlayerGrounded, PlayerInput};
use shared::run::RunState;

use super::level::KenneyLayoutCache;
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
                ground_move,
                apply_gravity,
                move_and_slide,
                step_up,
                resolve_ground,
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

fn standing_capsule() -> Collider {
    Collider::capsule(config::PLAYER_CAPSULE_RADIUS, config::PLAYER_CAPSULE_LENGTH)
}

fn crouch_capsule() -> Collider {
    Collider::capsule(config::PLAYER_CAPSULE_RADIUS, config::PLAYER_CROUCH_LENGTH)
}

/// Vertical body shift applied on each crouch/stand transition: half the
/// capsule-length difference. The kinematic `move_and_slide` has no penetration
/// recovery, so when the tall capsule is restored on standing it would be left
/// embedded in the floor (body stuck low / model sunk). Shifting the body
/// keeps the feet planted and reliably restores standing height.
const CROUCH_Y_SHIFT: f32 =
    (config::PLAYER_CAPSULE_LENGTH - config::PLAYER_CROUCH_LENGTH) * 0.5;

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
            cast_shape: standing_capsule(),
        }
    }
}

/// Downward shape-cast: returns vertical gap to the nearest walkable floor.
fn probe_floor_gap(
    spatial: &SpatialQuery,
    collider: &Collider,
    pos: Vector,
    rot: Quat,
    ground: &GroundDetection,
    exclude: &[Entity],
) -> Option<Scalar> {
    let up = (rot * Vector::Y).normalize_or_zero();
    let filter = SpatialQueryFilter::from_excluded_entities(exclude.to_vec());
    let hit = spatial.cast_shape(
        collider,
        pos,
        rot,
        Dir3::NEG_Y,
        &ShapeCastConfig::from_max_distance(config::PLAYER_GROUND_PROBE),
        &filter,
    )?;
    if (rot * hit.normal1).angle_between(up) > ground.max_angle {
        return None;
    }
    Some(hit.distance)
}

/// Whether floor contact should count this tick (jump, friction, snap).
fn floor_contact_allowed(gap: Scalar, wading: bool) -> bool {
    if gap > config::PLAYER_GROUND_PROBE {
        return false;
    }
    // Straddling water + dry tile: only ground when feet are clearly on the floor.
    if wading && gap > config::PLAYER_SNAP_TO_GROUND {
        return false;
    }
    true
}

fn extraction_xz(layout: &KenneyLayoutCache, run: Option<&RunState>) -> Option<[f32; 2]> {
    if let Some(ex) = layout.0.extraction_xz {
        return Some(ex);
    }
    let run = run?;
    let def = shared::level::level_by_id(&run.level_id, run.run_seed);
    def.extraction.map(|v| [v.x, v.z])
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
            *collider = crouch_capsule();
            ground.cast_shape = crouch_capsule();
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
                        &standing_capsule(),
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
            *collider = standing_capsule();
            ground.cast_shape = standing_capsule();
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

/// Quake order: categorize (trace + snap) → jump → friction → accelerate.
///
/// Ground is decided with a **live** floor probe each tick so jump and friction
/// work while moving. (`Commands`-inserted `Grounded` is deferred and was the
/// root cause of “jump only when standing still”.)
fn ground_move(
    spatial: SpatialQuery,
    liquids: Query<Entity, With<crate::liquids::Liquid>>,
    time: Res<Time>,
    layout: Res<KenneyLayoutCache>,
    run: Query<&RunState>,
    mut players: Query<
        (
            Entity,
            &Collider,
            &GroundDetection,
            &Transform,
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
    let liquid_ents: Vec<Entity> = liquids.iter().collect();
    let extraction = extraction_xz(&layout, run.single().ok());

    for (
        entity,
        collider,
        ground,
        transform,
        mut input,
        mut velocity,
        mut contact,
        mut coyote,
        speed_mult,
        water,
    ) in &mut players
    {
        let intent = &input.0;
        let world_pos = transform.translation;
        let pos = world_pos.adjust_precision();
        let rot = transform.rotation.adjust_precision();
        let mut exclude = liquid_ents.clone();
        exclude.push(entity);

        let rising_fast = velocity.y > config::PLAYER_JUMP_GROUND_CUTOFF;
        let gap = if rising_fast {
            None
        } else {
            probe_floor_gap(&spatial, collider, pos, rot, ground, &exclude)
        };

        let wading = water.0 > 0;
        let mut on_ground = coyote.0 > 0.0;
        if let Some(gap) = gap {
            if floor_contact_allowed(gap, wading) {
                on_ground = true;
            }
        }

        if let Some([ex, ez]) = extraction {
            if kenney_pit::suppress_extraction_grounding(world_pos.x, world_pos.y, world_pos.z, ex, ez) {
                on_ground = false;
                coyote.0 = 0.0;
            }
        }

        contact.on_ground = on_ground;

        if on_ground && velocity.y <= 0.0 {
            velocity.y = 0.0;
        }

        let move_dir = if intent.move_dir.length_squared() > 1.0 {
            intent.move_dir.normalize()
        } else {
            intent.move_dir
        };
        let wish_dir = Quat::from_rotation_y(intent.yaw)
            * Vec3::new(move_dir.x, 0.0, -move_dir.y);
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
        } else if intent.move_dir.length_squared() < 1e-4 {
            // Bleed horizontal momentum in air with no input — stops "ghost run"
            // after jumping into a wall and releasing keys.
            let horizontal = Vector::new(velocity.x, 0.0, velocity.z);
            let speed = horizontal.length();
            if speed > 1e-4 {
                let drop = speed * config::PLAYER_AIR_STOP_FRICTION * dt;
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
            let add = (wish_speed - current)
                .clamp(0.0, accel_rate * wish_speed * dt);
            velocity.x += wish_dir.x * add;
            velocity.z += wish_dir.z * add;
        }

        if intent.jump && on_ground && !intent.crouch {
            velocity.y = config::PLAYER_JUMP_IMPULSE;
            input.0.jump = false;
            contact.on_ground = false;
            coyote.0 = 0.0;
        } else if on_ground {
            coyote.0 = config::PLAYER_COYOTE_TIME;
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

fn clip_velocity_against_walls(
    velocity: &mut Vector,
    up: Vector,
    max_ground_angle: Scalar,
    collisions: &[CharacterCollision],
) {
    for hit in collisions {
        let normal = hit.normal.adjust_precision();
        if up.angle_between(normal) <= max_ground_angle {
            continue;
        }
        let into = velocity.dot(normal);
        if into < 0.0 {
            *velocity -= normal * into;
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
                collisions.0.push(CharacterCollision {
                    collider: hit.entity,
                    point: hit.point,
                    normal: *hit.normal,
                    character_velocity: *hit.velocity,
                });
                let _ = is_ceiling;
                MoveAndSlideHitResponse::Accept
            },
        );

        transform.translation = new_position.f32();
        // Use the solver's full slide-resolved velocity. The old code only copied
        // the vertical component, leaving horizontal speed into walls untouched —
        // that caused wall shake, sticking, and ghost running after landing.
        lin_vel.0 = projected_velocity;
        clip_velocity_against_walls(
            &mut lin_vel.0,
            up,
            ground.max_angle,
            &collisions.0,
        );
    }
}

fn step_up(
    spatial: SpatialQuery,
    mut players: Query<
        (
            Entity,
            &Collider,
            &mut Transform,
            &mut LinearVelocity,
            &GroundDetection,
            &GroundContact,
        ),
        With<CharacterController>,
    >,
) {
    let step = config::PLAYER_STEP_HEIGHT;
    let min_climb = config::PLAYER_STEP_MIN_CLIMB;
    let min_riser = config::PLAYER_STEP_MIN_RISER_ANGLE;
    let front_probe: Scalar = 0.15;
    let sample_dist: Scalar = 0.45;

    for (entity, collider, mut transform, mut velocity, ground, contact) in &mut players {
        if !contact.on_ground {
            continue;
        }
        let horizontal = Vector::new(velocity.x, 0.0, velocity.z);
        if horizontal.length() < 0.05 {
            continue;
        }
        let Ok(dir) = Dir3::new(Vec3::new(velocity.x, 0.0, velocity.z)) else {
            continue;
        };
        let fwd = dir.as_vec3().adjust_precision();
        let pos = transform.translation.adjust_precision();
        let rot = transform.rotation.adjust_precision();
        let up = transform.up().adjust_precision();
        let filter = SpatialQueryFilter::from_excluded_entities([entity]);

        let Some(front) = spatial.cast_shape(
            collider,
            pos,
            rot,
            dir,
            &ShapeCastConfig::from_max_distance(front_probe),
            &filter,
        ) else {
            continue;
        };
        let riser_angle = (rot * front.normal1).angle_between(up);
        if riser_angle <= ground.max_angle || riser_angle < min_riser {
            continue;
        }

        let head_room = spatial
            .cast_shape(
                collider,
                pos,
                rot,
                Dir3::Y,
                &ShapeCastConfig::from_max_distance(step),
                &filter,
            )
            .map(|h| h.distance)
            .unwrap_or(step);
        if head_room < 0.05 {
            continue;
        }
        let lift = head_room.min(step);
        let raised = pos + up * lift;

        if spatial
            .cast_shape(
                collider,
                raised,
                rot,
                dir,
                &ShapeCastConfig::from_max_distance(sample_dist),
                &filter,
            )
            .is_some()
        {
            continue;
        }

        let sample = raised + fwd * sample_dist;
        let Some(down) = spatial.cast_shape(
            collider,
            sample,
            rot,
            Dir3::NEG_Y,
            &ShapeCastConfig::from_max_distance(lift + 0.05),
            &filter,
        ) else {
            continue;
        };
        if (rot * down.normal1).angle_between(up) > ground.max_angle {
            continue;
        }

        let landed = sample - up * down.distance;
        let climb = landed.y - pos.y;
        if climb >= min_climb && climb <= step + 0.05 {
            transform.translation.y = landed.y as f32;
            if velocity.y > 0.0 {
                velocity.y = 0.0;
            }
        }
    }
}

/// After movement: snap to floor and refresh [`GroundContact`] for next tick.
fn resolve_ground(
    time: Res<Time>,
    spatial: SpatialQuery,
    liquids: Query<Entity, With<crate::liquids::Liquid>>,
    layout: Res<KenneyLayoutCache>,
    run: Query<&RunState>,
    mut players: Query<
        (
            Entity,
            &Collider,
            &GroundDetection,
            &mut Transform,
            &LinearVelocity,
            &mut GroundContact,
            &mut CoyoteTime,
            &PlayerWaterContact,
        ),
        With<CharacterController>,
    >,
) {
    let dt = time.delta_secs();
    let snap = config::PLAYER_SNAP_TO_GROUND;
    let liquid_ents: Vec<Entity> = liquids.iter().collect();
    let extraction = extraction_xz(&layout, run.single().ok());

    for (entity, collider, ground, mut transform, velocity, mut contact, mut coyote, water) in
        &mut players
    {
        if velocity.y > config::PLAYER_JUMP_GROUND_CUTOFF {
            contact.on_ground = false;
            coyote.0 = (coyote.0 - dt).max(0.0);
            continue;
        }

        let world_pos = transform.translation;
        if let Some([ex, ez]) = extraction {
            if kenney_pit::suppress_extraction_grounding(world_pos.x, world_pos.y, world_pos.z, ex, ez) {
                contact.on_ground = false;
                coyote.0 = 0.0;
                continue;
            }
        }

        let pos = world_pos.adjust_precision();
        let rot = transform.rotation.adjust_precision();
        let mut exclude = liquid_ents.clone();
        exclude.push(entity);
        let wading = water.0 > 0;

        if let Some(gap) = probe_floor_gap(&spatial, collider, pos, rot, ground, &exclude) {
            if floor_contact_allowed(gap, wading) {
                if let Some([ex, ez]) = extraction {
                    let hub_top = kenney_pit::pit_floor_top_y(kenney_pit::HUB_FLOOR_LEVEL);
                    // Nudge shaft fallers onto the hub rim — not hub-floor walkers stepping into the pit.
                    if kenney_pit::in_extraction_drop_zone(world_pos.x, world_pos.z, ex, ez)
                        && world_pos.y > hub_top + 1.2
                        && world_pos.y < kenney_pit::pit_floor_plane_y(0) - 0.25
                        && velocity.y < 0.0
                    {
                        let [lx, lz] = shared::kenney_hub::hub_shaft_landing_xz(ex, ez);
                        transform.translation.x = lx;
                        transform.translation.z = lz;
                    }
                }
                if gap <= snap && gap > 1e-5 {
                    transform.translation.y -= gap as f32;
                }
                contact.on_ground = true;
                coyote.0 = config::PLAYER_COYOTE_TIME;
                continue;
            }
        }

        contact.on_ground = false;
        coyote.0 = (coyote.0 - dt).max(0.0);
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
