//! Client-side visuals for level geometry. Purely cosmetic: builds meshes
//! and materials from the shared `LevelDef`. No gameplay logic here.

use bevy::prelude::*;
use shared::level::{self, StaticKind};

pub struct LevelRenderPlugin;

impl Plugin for LevelRenderPlugin {
    fn build(&self, app: &mut App) {
        // Simple sky.
        app.insert_resource(ClearColor(Color::srgb(0.55, 0.75, 0.95)))
            .add_systems(Startup, (spawn_level_visuals, spawn_sun));
    }
}

fn kind_color(kind: StaticKind) -> Color {
    match kind {
        StaticKind::Floor => Color::srgb(0.45, 0.45, 0.48),
        StaticKind::Wall => Color::srgb(0.65, 0.62, 0.55),
        StaticKind::Ramp => Color::srgb(0.50, 0.60, 0.50),
        StaticKind::Platform => Color::srgb(0.55, 0.50, 0.62),
    }
}

fn spawn_level_visuals(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let level = level::test_level();

    for def in &level.statics {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::from_size(def.size))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: kind_color(def.kind),
                perceptual_roughness: 0.9,
                ..default()
            })),
            Transform::from_translation(def.position).with_rotation(def.rotation),
        ));
    }

    // Debug markers for spawn points so the level can be inspected.
    let marker_mesh = meshes.add(Sphere::new(0.2));
    let item_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.9, 0.8, 0.2),
        unlit: true,
        ..default()
    });
    let player_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.2, 0.8, 0.9),
        unlit: true,
        ..default()
    });
    for pos in &level.item_spawns {
        commands.spawn((
            Mesh3d(marker_mesh.clone()),
            MeshMaterial3d(item_mat.clone()),
            Transform::from_translation(*pos),
        ));
    }
    for pos in &level.player_spawns {
        commands.spawn((
            Mesh3d(marker_mesh.clone()),
            MeshMaterial3d(player_mat.clone()),
            Transform::from_translation(*pos),
        ));
    }
}

fn spawn_sun(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            illuminance: 10_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, -0.6, -1.0, 0.0)),
    ));
}
