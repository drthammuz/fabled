//! Client-side visuals for level geometry. Purely cosmetic.

use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::light::VolumetricLight;
use bevy::math::Affine2;
use bevy::prelude::*;
use shared::level::{self, StaticKind};
use shared::run::RunState;

use bevy_water::material::StandardWaterMaterial;

use crate::sewer_atmosphere::{attach_water_atmosphere, spawn_level_atmosphere, AtmosphereEffects};
use crate::tunnel_mesh::{panelize_kind, wall_panels};
use crate::water_render::spawn_channel_water;

pub struct LevelRenderPlugin;

impl Plugin for LevelRenderPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.04)))
            .insert_resource(LastRenderedLevel::default())
            .init_resource::<CyberTextures>()
            .init_resource::<WarningLightPositions>()
            .add_systems(Startup, load_cyber_textures)
            .add_systems(Update, watch_level_changes);
    }
}

/// World positions of all warning (orange/amber) lights in the current level.
/// Audio uses this to drive electric-buzz proximity volume.
#[derive(Resource, Default)]
pub struct WarningLightPositions(pub Vec<Vec3>);

/// Tracks what was last rendered so we only rebuild on actual changes.
#[derive(Resource, Default)]
pub struct LastRenderedLevel {
    pub id: String,
    pub seed: u64,
}

#[derive(Component)]
pub struct LevelVisual;

#[derive(Resource, Default)]
struct CyberTextures {
    floor_color: Option<Handle<Image>>,
    wall_color: Option<Handle<Image>>,
    ceiling_color: Option<Handle<Image>>,
}

fn load_tiling(asset_server: &AssetServer, path: &'static str) -> Handle<Image> {
    asset_server.load_with_settings(path, |s: &mut ImageLoaderSettings| {
        s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            ..ImageSamplerDescriptor::linear()
        });
    })
}

fn load_cyber_textures(asset_server: Res<AssetServer>, mut cyber: ResMut<CyberTextures>) {
    cyber.floor_color   = Some(load_tiling(&asset_server, "textures/cyberpunk/floor_color.jpg"));
    cyber.wall_color    = Some(load_tiling(&asset_server, "textures/cyberpunk/wall_color.jpg"));
    cyber.ceiling_color = Some(load_tiling(&asset_server, "textures/cyberpunk/ceiling_color.jpg"));
    info!("CyberTextures queued for load");
}

/// Scale UVs so each world-space meter maps to one texture tile.
fn uv_scale_for_size(size: Vec3, tile_m: f32) -> Vec2 {
    let mut dims = [size.x, size.y, size.z];
    dims.sort_by(|a, b| b.partial_cmp(a).unwrap());
    Vec2::new(
        (dims[0] / tile_m).max(0.25),
        (dims[1] / tile_m).max(0.25),
    )
}

struct SurfacePreset {
    color: Handle<Image>,
    tint: Color,
    tile_m: f32,
    roughness: f32,
    emissive_strength: f32,
}

fn pbr_material(
    materials: &mut Assets<StandardMaterial>,
    preset: &SurfacePreset,
    uv_scale: Vec2,
) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: preset.tint,
        base_color_texture: Some(preset.color.clone()),
        metallic: 0.0,
        perceptual_roughness: preset.roughness,
        // Self-illuminate with the surface's OWN texture so walls stay clearly
        // visible across their whole face even where no point light reaches —
        // the sewer is so dark that lit-only walls read as ~invisible/see-through.
        // emissive_texture modulates the emissive color by the texture pattern,
        // so this shows the cyberpunk wall texture glowing, not a flat panel.
        emissive_texture: Some(preset.color.clone()),
        emissive: LinearRgba::rgb(0.55, 0.52, 0.48) * preset.emissive_strength,
        // Render both faces — guards against any inside-out panel winding.
        double_sided: true,
        cull_mode: None,
        uv_transform: Affine2::from_scale(uv_scale),
        ..default()
    })
}

fn preset_for(kind: StaticKind, cyber: &CyberTextures) -> Option<SurfacePreset> {
    match kind {
        StaticKind::SewerWalkway | StaticKind::SewerFloor | StaticKind::Square => Some(SurfacePreset {
            color: cyber.floor_color.clone()?,
            tint: Color::srgb(1.0, 1.0, 1.0),
            tile_m: 2.0,
            roughness: 0.85,
            emissive_strength: 0.6,
        }),
        StaticKind::SewerWall
        | StaticKind::Building
        | StaticKind::Wall
        | StaticKind::SewerDuct
        | StaticKind::SewerBrace
        | StaticKind::SewerArch => Some(SurfacePreset {
            color: cyber.wall_color.clone()?,
            tint: Color::srgb(1.0, 1.0, 1.0),
            tile_m: 1.5,
            roughness: 0.92,
            emissive_strength: 1.2,
        }),
        StaticKind::Roof | StaticKind::Platform | StaticKind::Ramp => Some(SurfacePreset {
            color: cyber.ceiling_color.clone()?,
            tint: Color::srgb(0.85, 0.88, 0.95),
            tile_m: 3.0,
            roughness: 0.90,
            emissive_strength: 0.4,
        }),
        _ => None,
    }
}

fn material_for_piece(
    kind: StaticKind,
    size: Vec3,
    cyber: &CyberTextures,
    materials: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    if let Some(preset) = preset_for(kind, cyber) {
        let uv = uv_scale_for_size(size, preset.tile_m);
        return pbr_material(materials, &preset, uv);
    }
    match kind {
        StaticKind::SewerPipe => materials.add(StandardMaterial {
            base_color: Color::srgb(0.35, 0.38, 0.42),
            metallic: 0.85,
            perceptual_roughness: 0.45,
            ..default()
        }),
        StaticKind::Neon => materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.9, 1.0),
            emissive: LinearRgba::from(Color::srgb(0.2, 0.9, 1.0)) * 6.0,
            unlit: true,
            ..default()
        }),
        _ => materials.add(StandardMaterial {
            base_color: Color::srgb(0.4, 0.42, 0.48),
            metallic: 0.6,
            perceptual_roughness: 0.55,
            ..default()
        }),
    }
}

fn watch_level_changes(
    run: Query<&RunState>,
    mut last: ResMut<LastRenderedLevel>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut water_materials: ResMut<Assets<StandardWaterMaterial>>,
    cyber: Res<CyberTextures>,
    atmosphere: Res<AtmosphereEffects>,
    visuals: Query<Entity, With<LevelVisual>>,
    mut warning_lights: ResMut<WarningLightPositions>,
) {
    let Ok(state) = run.single() else {
        // In host mode this fires while the server entity is being set up.
        return;
    };
    if state.level_id.is_empty() {
        return;
    }
    if state.level_id == last.id && state.run_seed == last.seed {
        return;
    }
    last.id = state.level_id.clone();
    last.seed = state.run_seed;
    for e in &visuals {
        commands.entity(e).despawn();
    }
    spawn_level_visuals(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut water_materials,
        &cyber,
        &atmosphere,
        &state.level_id,
        state.run_seed,
        &mut warning_lights,
    );
}

fn spawn_level_visuals(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    water_materials: &mut Assets<StandardWaterMaterial>,
    cyber: &CyberTextures,
    atmosphere: &AtmosphereEffects,
    level_id: &str,
    seed: u64,
    warning_lights: &mut WarningLightPositions,
) {
    info!("=== LEVEL: {}  seed: {} ===", level_id, seed);
    let level = level::level_by_id(level_id, seed);
    let mut wall_index = 0u32;
    let mut channel_id = 0u32;

    for def in &level.statics {
        if def.kind == StaticKind::Gable {
            continue;
        }

        if def.kind == StaticKind::SewerWater {
            spawn_channel_water(
                commands,
                meshes,
                water_materials,
                def,
                channel_id,
            );
            channel_id += 1;
            attach_water_atmosphere(commands, def);
            continue;
        }

        let material = material_for_piece(def.kind, def.size, cyber, materials);

        if panelize_kind(def.kind) {
            for (offset, panel_size) in wall_panels(def, seed, wall_index) {
                let local = offset;
                let world_offset = def.rotation * local;
                let panel_mat = material_for_piece(def.kind, panel_size, cyber, materials);
                commands.spawn((
                    LevelVisual,
                    Mesh3d(meshes.add(Cuboid::from_size(panel_size))),
                    MeshMaterial3d(panel_mat),
                    Transform::from_translation(def.position + world_offset)
                        .with_rotation(def.rotation),
                ));
            }
            wall_index += 1;
            continue;
        }

        let (mesh, rotation) = if def.kind == StaticKind::SewerPipe {
            let radius = def.size.x.min(def.size.y).min(def.size.z) * 0.5;
            let length = def.size.x.max(def.size.y).max(def.size.z);
            // Bevy Cylinder axis is Y. Rotate to match the longest dimension.
            let rot = if def.size.z >= def.size.x && def.size.z >= def.size.y {
                def.rotation * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)
            } else if def.size.x >= def.size.y {
                def.rotation * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)
            } else {
                def.rotation
            };
            (Mesh3d(meshes.add(Cylinder::new(radius, length))), rot)
        } else {
            (Mesh3d(meshes.add(Cuboid::from_size(def.size))), def.rotation)
        };

        commands.spawn((
            LevelVisual,
            mesh,
            MeshMaterial3d(material),
            Transform::from_translation(def.position).with_rotation(rotation),
        ));
    }

    // Spawn point lights — warning lights also get a small glowing mesh.
    warning_lights.0.clear();
    let warning_bulb_mesh = meshes.add(Sphere::new(0.08));
    for light in &level.lights {
        // Identify warning lights by orange/amber color: high R, mid G, low B.
        let srgba = light.color.to_srgba();
        let is_warning = srgba.red > 0.7 && srgba.green < 0.65 && srgba.blue < 0.25;

        if is_warning {
            warning_lights.0.push(light.position);
            // Small glowing orb on the wall so the light source is visible.
            let bulb_mat = materials.add(StandardMaterial {
                base_color: light.color,
                emissive: LinearRgba::from(light.color) * 8.0,
                unlit: true,
                ..default()
            });
            commands.spawn((
                LevelVisual,
                Mesh3d(warning_bulb_mesh.clone()),
                MeshMaterial3d(bulb_mat),
                Transform::from_translation(light.position),
            ));
        }

        commands.spawn((
            LevelVisual,
            PointLight {
                color: light.color,
                intensity: light.intensity,
                range: light.range,
                shadows_enabled: false,
                ..default()
            },
            VolumetricLight,
            Transform::from_translation(light.position),
        ));
    }

    spawn_level_atmosphere(commands, atmosphere, &level.statics);
}
