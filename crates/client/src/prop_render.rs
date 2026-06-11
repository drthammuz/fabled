//! Client-side visuals for dynamic props: when an entity gains a
//! `PropShape` (spawned by the server locally in host mode, replicated
//! over the network from M3), attach a mesh and material to it.

use bevy::prelude::*;
use shared::props::PropShape;
use shared::protocol::Item;

pub struct PropRenderPlugin;

impl Plugin for PropRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, attach_prop_visuals);
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
