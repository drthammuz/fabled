//! Server-side level instantiation: turns the shared `LevelDef` into
//! physics entities. Supports reload when the run transitions stretches.

use avian3d::{math::*, prelude::*};
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use shared::config;
use shared::items;
use shared::level::{self, LevelDef, PropDef};
use shared::props::{Grabbable, PropShape};
use shared::protocol::{Item, NetTransform};

use crate::combat::{EnemyBrain, Health};
use crate::liquids;
use shared::protocol::Enemy;

/// Marks geometry/props/items spawned by the current level (despawned on reload).
#[derive(Component)]
pub struct LevelEntity;

#[derive(Resource, Default)]
pub struct LoadedLevel {
    pub id: String,
}

pub struct ServerLevelPlugin;

impl Plugin for ServerLevelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadedLevel>()
            // PostStartup runs after Startup commands are flushed, so RunState
            // (spawned by RunPlugin at Startup) is available here.
            .add_systems(PostStartup, initial_load);
    }
}

pub fn load_level(commands: &mut Commands, existing: &Query<Entity, With<LevelEntity>>, def: &LevelDef) {
    for entity in existing.iter() {
        commands.entity(entity).despawn();
    }
    spawn_level_content(commands, def);
}

fn initial_load(
    mut commands: Commands,
    existing: Query<Entity, With<LevelEntity>>,
    run: Query<&shared::run::RunState>,
) {
    let (id, seed) = run.single()
        .map(|r| (r.level_id.clone(), r.run_seed))
        .unwrap_or_else(|_| ("sewer_entry".to_string(), 0));
    let def = level::level_by_id(&id, seed);
    load_level(&mut commands, &existing, &def);
}

fn prop_collider(shape: &PropShape) -> Collider {
    match *shape {
        PropShape::Crate { size } => Collider::cuboid(size.x, size.y, size.z),
        PropShape::Ball { radius } => Collider::sphere(radius),
    }
}

fn spawn_level_content(commands: &mut Commands, level: &LevelDef) {
    let mut channel_id = 0u32;
    for def in &level.statics {
        if def.kind == level::StaticKind::SewerWater {
            liquids::spawn_water_volume(commands, def, channel_id);
            channel_id += 1;
            continue;
        }
        if matches!(
            def.kind,
            level::StaticKind::Gable
                | level::StaticKind::Neon
                | level::StaticKind::SewerPipe
                | level::StaticKind::SewerBrace
                | level::StaticKind::SewerArch
                | level::StaticKind::SewerWalkway
        ) {
            continue;
        }
        commands.spawn((
            LevelEntity,
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
            LevelEntity,
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

    // Credit pickups scattered in the stretch.
    for (i, pos) in level.item_spawns.iter().enumerate() {
        let amount = 8 + (i as u32) * 4;
        spawn_world_item(commands, items::credits(amount), *pos, Vec3::ZERO, true);
    }

    for pos in &level.enemy_spawns {
        spawn_enemy(commands, *pos);
    }

    info!(
        "level '{}' spawned: {} statics, {} props, {} pickups, {} enemies",
        level.id,
        level.statics.len(),
        level.props.len(),
        level.item_spawns.len(),
        level.enemy_spawns.len()
    );
}

fn spawn_enemy(commands: &mut Commands, position: Vec3) {
    commands.spawn((
        LevelEntity,
        Replicated,
        Enemy,
        EnemyBrain::at(position),
        Health {
            current: 40.0,
            max: 40.0,
        },
        RigidBody::Kinematic,
        Collider::sphere(0.55),
        Transform::from_translation(position),
        NetTransform {
            translation: position,
            rotation: Quat::IDENTITY,
        },
    ));
}

/// Spawns a pickup item as a physics object in the world.
pub fn spawn_world_item(
    commands: &mut Commands,
    item: Item,
    position: Vec3,
    velocity: Vec3,
    level_owned: bool,
) -> Entity {
    let size = config::ITEM_SIZE;
    let mut entity = commands.spawn((
        Replicated,
        RigidBody::Dynamic,
        Collider::cuboid(size, size, size),
        ColliderDensity(80.0),
        Friction::new(0.5),
        Restitution::new(config::PROP_RESTITUTION),
        AngularDamping(config::PROP_ANGULAR_DAMPING),
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
    ));
    if level_owned {
        entity.insert(LevelEntity);
    }
    entity.id()
}
