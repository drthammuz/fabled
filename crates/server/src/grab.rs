//! Server-side grab/throw. Clients send button state only; the server
//! raycasts, applies spring forces, and throws via impulse. Multiple
//! players holding the same object each apply their own force.

use std::collections::HashMap;

use avian3d::{math::*, prelude::*};
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use shared::config;
use shared::props::Grabbable;
use shared::protocol::Player;

use crate::players::LatestInput;

pub struct ServerGrabPlugin;

impl Plugin for ServerGrabPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (update_grab_targets, apply_grab_forces, apply_throws)
                .chain()
                .run_if(in_state(ClientState::Disconnected)),
        );
    }
}

/// Which object this player is currently holding (server-only).
#[derive(Component, Default)]
pub(crate) struct GrabTarget(pub Option<Entity>);

fn look_direction(yaw: f32, pitch: f32) -> Vec3 {
    Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0) * -Vec3::Z
}

fn hold_point(origin: Vec3, yaw: f32, pitch: f32) -> Vec3 {
    origin + look_direction(yaw, pitch) * config::GRAB_HOLD_DISTANCE
}

fn update_grab_targets(
    spatial: SpatialQuery,
    colliders: Query<&ColliderOf>,
    grabbables: Query<(), With<Grabbable>>,
    mut players: Query<(Entity, &Transform, &LatestInput, &mut GrabTarget), With<Player>>,
) {
    for (player, transform, input, mut grab) in &mut players {
        if !input.0.grab {
            grab.0 = None;
            continue;
        }

        // Keep holding the current target while the button stays down;
        // re-acquiring every tick would let the hold switch objects mid-carry.
        if grab.0.is_some() {
            continue;
        }

        let eye = transform.translation + Vec3::Y * config::PLAYER_EYE_HEIGHT;
        let dir = Dir3::new(look_direction(input.0.yaw, input.0.pitch)).unwrap_or(Dir3::NEG_Z);

        if let Some(hit) = spatial.cast_ray(
            eye.adjust_precision(),
            dir,
            config::GRAB_RANGE,
            true,
            &SpatialQueryFilter::from_excluded_entities([player]),
        ) {
            let body = colliders
                .get(hit.entity)
                .map(|c| c.body)
                .unwrap_or(hit.entity);
            if grabbables.get(body).is_ok() {
                grab.0 = Some(body);
            }
        }
    }
}

fn apply_grab_forces(
    players: Query<(&Transform, &LatestInput, &GrabTarget), With<Player>>,
    mut queries: ParamSet<(
        Query<(&Transform, &ComputedMass, &LinearVelocity), With<Grabbable>>,
        Query<Forces, With<Grabbable>>,
    )>,
    gravity: Res<Gravity>,
) {
    // Sum per-body force first — Forces conflicts with LinearVelocity
    // in the same system, so read and write happen in separate ParamSet passes.
    let mut held: HashMap<Entity, Vector> = HashMap::new();

    for (player_tf, input, grab) in &players {
        let Some(target) = grab.0 else {
            continue;
        };
        if !input.0.grab {
            continue;
        }
        let body = {
            let bodies = queries.p0();
            let Ok((body_tf, mass, velocity)) = bodies.get(target) else {
                continue;
            };
            (body_tf.translation, mass.value(), velocity.0)
        };

        let hold = hold_point(player_tf.translation, input.0.yaw, input.0.pitch);
        let error = (hold - body.0).adjust_precision();
        // PD controller toward the hold point plus gravity compensation,
        // converted to a force and capped at one player's "strength". Light
        // objects snap to the hold point; heavy ones sag or need two players.
        let accel =
            error * config::GRAB_SPRING - body.2 * config::GRAB_DAMPING - gravity.0;
        let force = (accel * body.1).clamp_length_max(config::GRAB_MAX_FORCE);
        *held.entry(target).or_insert(Vector::ZERO) += force;
    }

    for (entity, force) in held {
        if let Ok(mut body_forces) = queries.p1().get_mut(entity) {
            body_forces.apply_force(force);
        }
    }
}

fn apply_throws(
    mut players: Query<(&mut LatestInput, &mut GrabTarget), With<Player>>,
    mut bodies: Query<(&ComputedMass, &mut LinearVelocity), With<Grabbable>>,
) {
    for (mut input, mut grab) in &mut players {
        if !input.0.throw_action {
            continue;
        }
        input.0.throw_action = false;
        let Some(target) = grab.0.take() else {
            continue;
        };
        let Ok((mass, mut velocity)) = bodies.get_mut(target) else {
            continue;
        };
        // Heavy objects can't be hurled at full speed.
        let speed =
            config::THROW_IMPULSE * (config::THROW_REF_MASS / mass.value()).min(1.0);
        let dir = look_direction(input.0.yaw, input.0.pitch);
        velocity.0 = dir.adjust_precision() * speed;
    }
}
