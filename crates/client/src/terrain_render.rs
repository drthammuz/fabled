//! Client-side terrain: builds a mesh from the shared heightfield and
//! renders it with a slope-blended grass/dirt/rock material (see
//! `assets/shaders/terrain.wgsl`).

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use shared::terrain;

pub struct TerrainRenderPlugin;

impl Plugin for TerrainRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<TerrainMaterial>::default())
            .add_systems(Startup, spawn_terrain);
    }
}

type TerrainMaterial = ExtendedMaterial<StandardMaterial, TerrainExtension>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
struct TerrainExtension {
    #[texture(100)]
    #[sampler(101)]
    grass: Handle<Image>,
    #[texture(102)]
    #[sampler(103)]
    dirt: Handle<Image>,
    #[texture(104)]
    #[sampler(105)]
    rock: Handle<Image>,
}

impl MaterialExtension for TerrainExtension {
    fn fragment_shader() -> ShaderRef {
        "shaders/terrain.wgsl".into()
    }
}

/// Loads an image set to tile (repeat) instead of clamping at the edges.
pub fn load_tiling(asset_server: &AssetServer, path: &'static str) -> Handle<Image> {
    asset_server.load_with_settings(path, |settings: &mut ImageLoaderSettings| {
        settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            ..ImageSamplerDescriptor::linear()
        });
    })
}

fn terrain_mesh() -> Mesh {
    let positions = terrain::grid_positions();
    let normals: Vec<Vec3> = positions
        .iter()
        .map(|p| terrain::normal(p.x, p.z))
        .collect();
    let uvs: Vec<Vec2> = positions.iter().map(|p| p.xz() / 4.0).collect();
    let indices: Vec<u32> = terrain::grid_indices().into_iter().flatten().collect();

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

fn spawn_terrain(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(terrain_mesh())),
        MeshMaterial3d(materials.add(TerrainMaterial {
            base: StandardMaterial {
                perceptual_roughness: 0.95,
                ..default()
            },
            extension: TerrainExtension {
                grass: load_tiling(&asset_server, "textures/grass.jpg"),
                dirt: load_tiling(&asset_server, "textures/dirt.jpg"),
                rock: load_tiling(&asset_server, "textures/rock.jpg"),
            },
        })),
        Transform::IDENTITY,
    ));
}
