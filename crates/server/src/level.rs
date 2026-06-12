//! Server-side level instantiation: turns the shared `LevelDef` into
//! physics entities. The server owns all of this; clients only ever see
//! the resulting state.

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use shared::config;
use shared::level::{self, PropDef};
use shared::props::{Grabbable, PropShape};
use shared::protocol::NetTransform;

pub struct ServerLevelPlugin;

impl Plugin for ServerLevelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_level_physics);
    }
}

fn prop_collider(shape: &PropShape) -> Collider {
    match *shape {
        PropShape::Crate { size } => Collider::cuboid(size.x, size.y, size.z),
        PropShape::Ball { radius } => Collider::sphere(radius),
    }
}

fn spawn_level_physics(mut commands: Commands) {
    let level = level::active_level();

    // Terrain ground: a static trimesh built from the shared heightfield,
    // identical to what the client renders.
    commands.spawn((
        RigidBody::Static,
        Collider::trimesh(shared::terrain::grid_positions(), shared::terrain::grid_indices()),
        Transform::IDENTITY,
    ));

    for def in &level.statics {
        // Gables are cosmetic prisms tucked inside the roofs; no collider.
        if def.kind == level::StaticKind::Gable {
            continue;
        }
        commands.spawn((
            RigidBody::Static,
            Collider::cuboid(def.size.x, def.size.y, def.size.z),
            Transform::from_translation(def.position).with_rotation(def.rotation),
        ));
    }

    for PropDef {
        shape,
        position,
        density,
    } in &level.props
    {
        commands.spawn((
            Replicated,
            Grabbable,
            RigidBody::Dynamic,
            prop_collider(shape),
            ColliderDensity(*density),
            Friction::new(0.35),
            Restitution::new(config::PROP_RESTITUTION),
            AngularDamping(config::PROP_ANGULAR_DAMPING),
            *shape,
            NetTransform {
                translation: *position,
                rotation: Quat::IDENTITY,
            },
            Transform::from_translation(*position),
        ));
    }

    info!(
        "level physics spawned: {} static colliders, {} dynamic props",
        level.statics.len(),
        level.props.len()
    );
}
