//! Client-side visuals for level geometry. Purely cosmetic: builds meshes
//! and materials from the shared `LevelDef`. No gameplay logic here.

use bevy::math::Affine2;
use bevy::prelude::*;
use shared::level::{self, StaticDef, StaticKind};

use crate::terrain_render::load_tiling;

pub struct LevelRenderPlugin;

impl Plugin for LevelRenderPlugin {
    fn build(&self, app: &mut App) {
        // Simple sky.
        app.insert_resource(ClearColor(Color::srgb(0.55, 0.75, 0.95)))
            .add_systems(Startup, (spawn_level_visuals, spawn_sun));
    }
}

/// Texture, tint, and tile size (meters per repeat) per static kind.
/// Untextured kinds (greybox test level) fall back to flat colors.
fn kind_style(kind: StaticKind) -> (Option<&'static str>, Color, f32) {
    match kind {
        StaticKind::Floor => (Some("textures/grass.jpg"), Color::WHITE, 4.0),
        StaticKind::Wall => (None, Color::srgb(0.65, 0.62, 0.55), 1.0),
        StaticKind::Ramp => (None, Color::srgb(0.50, 0.60, 0.50), 1.0),
        StaticKind::Platform => (None, Color::srgb(0.55, 0.50, 0.62), 1.0),
        StaticKind::Building => (Some("textures/plaster.jpg"), Color::srgb(0.95, 0.87, 0.72), 2.4),
        StaticKind::Field => (Some("textures/dirt.jpg"), Color::srgb(0.95, 0.9, 0.82), 3.0),
        StaticKind::Square => (Some("textures/cobble.jpg"), Color::WHITE, 2.2),
        StaticKind::Pier => (Some("textures/planks.jpg"), Color::srgb(0.9, 0.85, 0.8), 2.0),
        StaticKind::Roof => (Some("textures/roof.jpg"), Color::srgb(0.95, 0.8, 0.7), 2.0),
        StaticKind::Gable => (Some("textures/plaster.jpg"), Color::srgb(0.95, 0.87, 0.72), 2.4),
    }
}

/// A triangular prism for roof gables: base width `size.x` along x, apex
/// `size.y` up, extruded `size.z` deep. Local origin at the base center.
fn gable_mesh(size: Vec3) -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};

    let (b, r, t) = (size.x / 2.0, size.y, size.z / 2.0);
    // UVs in tile units to match the plaster scale (~2.4 m per repeat).
    let uv = |x: f32, y: f32| Vec2::new((x + b) / 2.4, (r - y) / 2.4);

    let mut positions: Vec<Vec3> = Vec::new();
    let mut normals: Vec<Vec3> = Vec::new();
    let mut uvs: Vec<Vec2> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut tri = |a: Vec3, bb: Vec3, c: Vec3| {
        let normal = (bb - a).cross(c - a).normalize();
        let base = positions.len() as u32;
        for p in [a, bb, c] {
            positions.push(p);
            normals.push(normal);
            uvs.push(uv(p.x, p.y));
        }
        indices.extend([base, base + 1, base + 2]);
    };

    let (a_f, b_f, c_f) = (Vec3::new(-b, 0.0, t), Vec3::new(b, 0.0, t), Vec3::new(0.0, r, t));
    let (a_b, b_b, c_b) = (Vec3::new(-b, 0.0, -t), Vec3::new(b, 0.0, -t), Vec3::new(0.0, r, -t));
    // Front and back faces.
    tri(a_f, b_f, c_f);
    tri(b_b, a_b, c_b);
    // Slanted sides.
    tri(b_f, b_b, c_b);
    tri(b_f, c_b, c_f);
    tri(a_f, c_f, c_b);
    tri(a_f, c_b, a_b);

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

/// Cuboid faces all map UV 0..1; scale the material's UV transform so the
/// texture repeats roughly every `tile` meters on the dominant faces.
fn uv_tiling(def: &StaticDef, tile: f32) -> Affine2 {
    let flat = def.size.y < 0.6;
    let scale = if flat {
        // Patches: the top face dominates.
        Vec2::new(def.size.x / tile, def.size.z / tile)
    } else {
        // Walls: the long vertical faces dominate.
        Vec2::new(def.size.x.max(def.size.z) / tile, def.size.y / tile)
    };
    Affine2::from_scale(scale.max(Vec2::splat(0.05)))
}

fn spawn_level_visuals(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let level = level::active_level();

    for def in &level.statics {
        let (texture, tint, tile) = kind_style(def.kind);
        let gable = def.kind == StaticKind::Gable;
        let material = StandardMaterial {
            base_color: tint,
            base_color_texture: texture.map(|path| load_tiling(&asset_server, path)),
            // Gable meshes carry their own metric UVs.
            uv_transform: if gable {
                Affine2::IDENTITY
            } else {
                uv_tiling(def, tile)
            },
            perceptual_roughness: 0.9,
            ..default()
        };
        let mesh = if gable {
            meshes.add(gable_mesh(def.size))
        } else {
            meshes.add(Cuboid::from_size(def.size))
        };
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(materials.add(material)),
            Transform::from_translation(def.position).with_rotation(def.rotation),
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
