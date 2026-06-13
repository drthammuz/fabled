//! Client-side visuals for dynamic props: when an entity gains a
//! `PropShape` (spawned by the server locally in host mode, replicated
//! over the network from M3), attach a mesh and material to it.

use bevy::prelude::*;
use shared::props::PropShape;
use shared::protocol::{Enemy, Item};

pub struct PropRenderPlugin;

impl Plugin for PropRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (attach_prop_visuals, attach_enemy_visuals));
    }
}

fn attach_prop_visuals(
    mut commands: Commands,
    props: Query<(Entity, &PropShape, Has<Item>), Added<PropShape>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, shape, is_item) in &props {
        let (mesh, color) = match *shape {
            PropShape::Crate { size } => (
                meshes.add(Cuboid::from_size(size)),
                if is_item {
                    // Pickup items glow gold so they stand out.
                    Color::srgb(0.95, 0.78, 0.2)
                } else {
                    Color::srgb(0.65, 0.45, 0.25)
                },
            ),
            PropShape::Ball { radius } => (
                meshes.add(Sphere::new(radius)),
                Color::srgb(0.75, 0.3, 0.3),
            ),
        };
        commands.entity(entity).insert((
            Mesh3d(mesh),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                perceptual_roughness: 0.8,
                ..default()
            })),
        ));
    }
}

fn attach_enemy_visuals(
    mut commands: Commands,
    enemies: Query<Entity, Added<Enemy>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Placeholder: dark armoured capsule body with a single red sensor eye glow.
    // Replace with GLTF model when assets arrive.
    for entity in &enemies {
        commands.entity(entity).insert((
            Mesh3d(meshes.add(Capsule3d::new(0.32, 0.75))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.12, 0.13, 0.16),
                metallic: 0.9,
                perceptual_roughness: 0.35,
                emissive: LinearRgba::from(Color::srgb(1.0, 0.05, 0.0)) * 3.5,
                ..default()
            })),
        ));
    }
}
