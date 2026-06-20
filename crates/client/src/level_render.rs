//! Client-side visuals for level geometry. Purely cosmetic.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::light::VolumetricLight;
use bevy::math::Affine2;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use shared::level::{self, StaticKind};
use shared::run::RunState;
use shared::EditorMode;
use shared::TestMode;

use bevy_water::material::StandardWaterMaterial;

use crate::sewer_atmosphere::{spawn_level_atmosphere, AtmosphereEffects};
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
            .add_systems(PostStartup, bootstrap_test_level_visuals)
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

/// Full PBR texture set for one surface kind: albedo (sRGB), tangent-space
/// normal map (linear), and packed ambient/roughness/metallic (linear).
#[derive(Clone, Default)]
struct SurfaceTextures {
    color: Option<Handle<Image>>,
    normal: Option<Handle<Image>>,
    orm: Option<Handle<Image>>, // R=AO, G=roughness, B=metallic
}

#[derive(Resource, Default)]
struct CyberTextures {
    floor: SurfaceTextures,
    wall: SurfaceTextures,
    ceiling: SurfaceTextures,
}

/// Repeat-tiled, sRGB — for albedo (base color) maps.
fn load_color(asset_server: &AssetServer, path: &'static str) -> Handle<Image> {
    asset_server.load_with_settings(path, |s: &mut ImageLoaderSettings| {
        s.is_srgb = true;
        s.sampler = tiling_sampler();
    })
}

/// Repeat-tiled, LINEAR — for normal and ORM (data) maps. Loading these as
/// sRGB would corrupt the surface relief and metallic/roughness response.
fn load_linear(asset_server: &AssetServer, path: &'static str) -> Handle<Image> {
    asset_server.load_with_settings(path, |s: &mut ImageLoaderSettings| {
        s.is_srgb = false;
        s.sampler = tiling_sampler();
    })
}

fn tiling_sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    })
}

fn load_cyber_textures(asset_server: Res<AssetServer>, mut cyber: ResMut<CyberTextures>) {
    cyber.floor = SurfaceTextures {
        color:  Some(load_color(&asset_server,  "textures/cyberpunk/floor_color.jpg")),
        normal: Some(load_linear(&asset_server, "textures/cyberpunk/floor_normal.jpg")),
        orm:    Some(load_linear(&asset_server, "textures/cyberpunk/floor_orm.png")),
    };
    cyber.wall = SurfaceTextures {
        color:  Some(load_color(&asset_server,  "textures/cyberpunk/wall_color.jpg")),
        normal: Some(load_linear(&asset_server, "textures/cyberpunk/wall_normal.jpg")),
        orm:    Some(load_linear(&asset_server, "textures/cyberpunk/wall_orm.png")),
    };
    cyber.ceiling = SurfaceTextures {
        color:  Some(load_color(&asset_server,  "textures/cyberpunk/ceiling_color.jpg")),
        normal: Some(load_linear(&asset_server, "textures/cyberpunk/ceiling_normal.jpg")),
        orm:    Some(load_linear(&asset_server, "textures/cyberpunk/ceiling_orm.png")),
    };
    info!("CyberTextures (full PBR) queued for load");
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
    tex: SurfaceTextures,
    tint: Color,
    tile_m: f32,
    emissive_strength: f32,
}

fn pbr_material(
    materials: &mut Assets<StandardMaterial>,
    preset: &SurfacePreset,
    uv_scale: Vec2,
) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: preset.tint,
        base_color_texture: preset.tex.color.clone(),
        // Tangent-space relief (rivets, panel seams, dents) — the thing that
        // makes flat geometry read as a real worn metal surface.
        normal_map_texture: preset.tex.normal.clone(),
        // Packed ambient/roughness/metallic. Scalars are 1.0 so the maps fully
        // drive the response: shiny where worn, matte where grimy, metal where
        // the metal mask says so (ceiling ORM has metallic=0 → stays concrete).
        metallic_roughness_texture: preset.tex.orm.clone(),
        metallic: 1.0,
        perceptual_roughness: 1.0,
        // Faint self-glow so deep-shadow areas stay readable; point lights do
        // the real work and now produce proper metal highlights.
        emissive_texture: preset.tex.color.clone(),
        emissive: LinearRgba::rgb(0.10, 0.095, 0.085) * preset.emissive_strength,
        uv_transform: Affine2::from_scale(uv_scale),
        ..default()
    })
}

/// Weathered rusted-steel material shared by pipes, bends, bars and grates.
fn steel_material(cyber: &CyberTextures, double_sided: bool) -> StandardMaterial {
    let mut m = StandardMaterial {
        base_color: Color::srgb(0.46, 0.30, 0.20),
        base_color_texture: cyber.wall.color.clone(),
        normal_map_texture: cyber.wall.normal.clone(),
        metallic: 0.25,
        perceptual_roughness: 0.78,
        emissive: LinearRgba::rgb(0.03, 0.022, 0.016),
        uv_transform: Affine2::from_scale(Vec2::splat(1.5)),
        double_sided,
        ..default()
    };
    if double_sided {
        m.cull_mode = None;
    }
    m
}

fn preset_for(kind: StaticKind, cyber: &CyberTextures) -> Option<SurfacePreset> {
    match kind {
        StaticKind::SewerWalkway | StaticKind::SewerFloor | StaticKind::Square => Some(SurfacePreset {
            tex: cyber.floor.clone(),
            tint: Color::srgb(1.0, 1.0, 1.0),
            tile_m: 2.0,
            emissive_strength: 0.5,
        }),
        StaticKind::SewerWall
        | StaticKind::Building
        | StaticKind::Wall
        | StaticKind::SewerDuct
        | StaticKind::SewerBrace => Some(SurfacePreset {
            // Aged metal panels: own albedo + relief normals + metal/roughness ORM.
            tex: cyber.wall.clone(),
            tint: Color::srgb(1.0, 1.0, 1.0),
            tile_m: 2.5,
            emissive_strength: 0.8,
        }),
        StaticKind::Roof | StaticKind::Platform | StaticKind::Ramp => Some(SurfacePreset {
            tex: cyber.ceiling.clone(),
            tint: Color::srgb(0.85, 0.88, 0.95),
            tile_m: 3.0,
            emissive_strength: 0.35,
        }),
        _ => None,
    }
}

fn material_for_piece(
    kind: StaticKind,
    size: Vec3,
    cyber: &CyberTextures,
    materials: &mut Assets<StandardMaterial>,
    test_map: bool,
) -> Handle<StandardMaterial> {
    let _ = test_map;
    if let Some(preset) = preset_for(kind, cyber) {
        let uv = uv_scale_for_size(size, preset.tile_m);
        return pbr_material(materials, &preset, uv);
    }
    match kind {
        // Pipes (straight, bends, elbows) AND reinforcing bars / grates all share
        // the same weathered rusted-steel look — pipes used to read as bright
        // aluminium; matching them to the grate steel looks much better.
        StaticKind::SewerPipe
        | StaticKind::SewerPipeBend
        | StaticKind::PipeElbow
        | StaticKind::SewerArch => materials.add(steel_material(cyber, false)),
        // The throat of a culvert mouth — a dark, murky-green unlit disc so the
        // opening reads as a water-filled pipe (sewer water seen inside) rather
        // than a capped cylinder or a black hole.
        StaticKind::PipeBore => materials.add(StandardMaterial {
            base_color: Color::srgb(0.02, 0.09, 0.06),
            unlit: true,
            ..default()
        }),
        // Open pipe-mouth tube: same steel, but double-sided so the interior wall
        // is visible when looking into the open end.
        StaticKind::PipeRing => materials.add(steel_material(cyber, true)),
        StaticKind::Neon => materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.9, 1.0),
            emissive: LinearRgba::from(Color::srgb(0.2, 0.9, 1.0)) * 6.0,
            unlit: true,
            ..default()
        }),
        StaticKind::DoorMarker => materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.15, 0.15),
            emissive: LinearRgba::rgb(2.0, 0.2, 0.2),
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

/// Test maps: force the first level visual build once RunState exists.
fn bootstrap_test_level_visuals(
    editor: Option<Res<EditorMode>>,
    test: Option<Res<TestMode>>,
    run: Query<&RunState>,
    visuals: Query<Entity, With<LevelVisual>>,
    mut last: ResMut<LastRenderedLevel>,
) {
    if editor.is_some() || test.is_none() {
        return;
    }
    let Ok(state) = run.single() else {
        warn!("test map: waiting for RunState");
        return;
    };
    if !visuals.is_empty() {
        return;
    }
    last.id.clear();
    last.seed = state.run_seed.wrapping_sub(1);
    info!("test map: bootstrap level visuals for '{}'", state.level_id);
}

fn watch_level_changes(
    editor: Option<Res<EditorMode>>,
    run: Query<&RunState>,
    test: Option<Res<TestMode>>,
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
    if editor.is_some() {
        return;
    }
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
        test.is_some(),
        &mut warning_lights,
    );
}

/// A cuboid mesh WITH tangents. Bevy's primitive `Cuboid` ships positions,
/// normals and UVs but NO tangents — and `normal_map_texture` is silently
/// ignored without them, so the surface renders perfectly flat. Generating
/// tangents is what lets the rivet/seam relief actually show.
fn cuboid_mesh(meshes: &mut Assets<Mesh>, size: Vec3) -> Handle<Mesh> {
    let mut mesh = Mesh::from(Cuboid::from_size(size));
    let _ = mesh.generate_tangents();
    meshes.add(mesh)
}

/// A cylinder mesh WITH tangents, so pipes can carry a normal map (Bevy's
/// `Cylinder` primitive omits tangents, which silently disables relief).
fn cylinder_mesh(meshes: &mut Assets<Mesh>, radius: f32, length: f32) -> Handle<Mesh> {
    let mut mesh = Mesh::from(Cylinder::new(radius, length));
    let _ = mesh.generate_tangents();
    meshes.add(mesh)
}

/// An open-ended pipe TUBE with real wall thickness (inner wall, outer wall, and
/// an annulus rim at each end) along Y, so a pipe mouth can be seen into AND
/// shows a thick rim. Inner radius is the opening; thickness is added outside.
/// Rendered with a double-sided material, so triangle winding doesn't matter.
fn pipe_tube_mesh(
    meshes: &mut Assets<Mesh>,
    inner_r: f32,
    outer_r: f32,
    height: f32,
) -> Handle<Mesh> {
    use std::f32::consts::TAU;
    const N: usize = 24;
    let hy = height * 0.5;
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut nor: Vec<[f32; 3]> = Vec::new();
    let mut uv: Vec<[f32; 2]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();

    // A cylindrical wall (two rings of N+1 verts) with the given radius/normal-sign.
    let wall = |radius: f32, nsign: f32, pos: &mut Vec<[f32; 3]>, nor: &mut Vec<[f32; 3]>, uv: &mut Vec<[f32; 2]>, idx: &mut Vec<u32>| {
        let base = pos.len() as u32;
        for i in 0..=N {
            let a = i as f32 / N as f32 * TAU;
            let (s, c) = a.sin_cos();
            for (k, y) in [(0u32, -hy), (1u32, hy)] {
                pos.push([radius * c, y, radius * s]);
                nor.push([nsign * c, 0.0, nsign * s]);
                uv.push([i as f32 / N as f32, k as f32]);
            }
        }
        for i in 0..N as u32 {
            let b = base + i * 2;
            idx.extend_from_slice(&[b, b + 2, b + 1, b + 1, b + 2, b + 3]);
        }
    };
    wall(outer_r, 1.0, &mut pos, &mut nor, &mut uv, &mut idx); // outer
    wall(inner_r, -1.0, &mut pos, &mut nor, &mut uv, &mut idx); // inner

    // Annulus rim caps at both ends (the visible pipe-wall thickness).
    for (y, ny) in [(-hy, -1.0f32), (hy, 1.0f32)] {
        let base = pos.len() as u32;
        for i in 0..=N {
            let a = i as f32 / N as f32 * TAU;
            let (s, c) = a.sin_cos();
            pos.push([outer_r * c, y, outer_r * s]);
            nor.push([0.0, ny, 0.0]);
            uv.push([0.0, 0.0]);
            pos.push([inner_r * c, y, inner_r * s]);
            nor.push([0.0, ny, 0.0]);
            uv.push([1.0, 0.0]);
        }
        for i in 0..N as u32 {
            let b = base + i * 2;
            idx.extend_from_slice(&[b, b + 2, b + 1, b + 1, b + 2, b + 3]);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_indices(Indices::U32(idx));
    meshes.add(mesh)
}

/// A smooth quarter-bend pipe (+ vertical stub) as ONE gap-free swept tube: a
/// circular profile of `radius` swept along the quarter-arc of radius `arc_r`
/// (canonical local frame: from +Z, bending to +Y) and then straight up by
/// `stub_len`. This is the "bevel a circle along a curve" technique done in code,
/// so there are no seams between segments. Oriented by the caller's rotation.
fn pipe_elbow_mesh(
    meshes: &mut Assets<Mesh>,
    radius: f32,
    arc_r: f32,
    stub_len: f32,
) -> Handle<Mesh> {
    use std::f32::consts::{FRAC_PI_2, TAU};
    let (from, to, u) = (Vec3::Z, Vec3::Y, Vec3::X);
    const KARC: usize = 16;
    const M: usize = 16;

    // Spine: (point, tangent) samples along the arc, then up the stub.
    let mut spine: Vec<(Vec3, Vec3)> = Vec::new();
    for k in 0..=KARC {
        let th = k as f32 / KARC as f32 * FRAC_PI_2;
        let p = arc_r * (from * th.sin() + to * (1.0 - th.cos()));
        let t = (from * th.cos() + to * th.sin()).normalize();
        spine.push((p, t));
    }
    if stub_len > 1e-3 {
        let tip = arc_r * (from + to);
        for k in 1..=3 {
            spine.push((tip + to * (stub_len * k as f32 / 3.0), to));
        }
    }

    let row = M + 1;
    let mut pos = Vec::with_capacity(spine.len() * row);
    let mut nor = Vec::with_capacity(spine.len() * row);
    let mut uv = Vec::with_capacity(spine.len() * row);
    let last = (spine.len() - 1) as f32;
    for (si, (p, t)) in spine.iter().enumerate() {
        let v = t.cross(u).normalize(); // u is always ⟂ t (t stays in the YZ plane)
        for j in 0..=M {
            let phi = j as f32 / M as f32 * TAU;
            let n = (u * phi.cos() + v * phi.sin()).normalize();
            let pt = *p + radius * n;
            pos.push([pt.x, pt.y, pt.z]);
            nor.push([n.x, n.y, n.z]);
            uv.push([si as f32 / last, j as f32 / M as f32]);
        }
    }
    let mut idx = Vec::with_capacity((spine.len() - 1) * M * 6);
    for si in 0..spine.len() - 1 {
        for j in 0..M {
            let a = (si * row + j) as u32;
            let b = a + 1;
            let c = ((si + 1) * row + j) as u32;
            let dd = c + 1;
            idx.extend_from_slice(&[a, c, b, b, c, dd]);
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_indices(Indices::U32(idx));
    let _ = mesh.generate_tangents();
    meshes.add(mesh)
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
    test_map: bool,
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
            // Recessed channel bed below the water so the channel has a visible
            // solid bottom (and the water reads as filling a trench, not floating).
            let bed_mat = materials.add(StandardMaterial {
                base_color: Color::srgb(0.10, 0.12, 0.11),
                perceptual_roughness: 0.95,
                metallic: 0.0,
                ..default()
            });
            commands.spawn((
                LevelVisual,
                Mesh3d(cuboid_mesh(meshes, Vec3::new(def.size.x, 0.05, def.size.z))),
                MeshMaterial3d(bed_mat),
                Transform::from_translation(Vec3::new(
                    def.position.x,
                    def.position.y + def.size.y * 0.5 - 0.06,
                    def.position.z,
                )),
            ));
            spawn_channel_water(
                commands,
                meshes,
                water_materials,
                def,
                channel_id,
            );
            channel_id += 1;
            continue;
        }

        let material = material_for_piece(def.kind, def.size, cyber, materials, test_map);

        if panelize_kind(def.kind) {
            for (offset, panel_size) in wall_panels(def, seed, wall_index) {
                let local = offset;
                let world_offset = def.rotation * local;
                let panel_mat = material_for_piece(def.kind, panel_size, cyber, materials, test_map);
                commands.spawn((
                    LevelVisual,
                    Mesh3d(cuboid_mesh(meshes, panel_size)),
                    MeshMaterial3d(panel_mat),
                    Transform::from_translation(def.position + world_offset)
                        .with_rotation(def.rotation),
                ));
            }
            wall_index += 1;
            continue;
        }

        let (mesh, rotation) = match def.kind {
            StaticKind::SewerPipe => {
                let (radius, length, rot) = level::straight_pipe(def);
                (Mesh3d(cylinder_mesh(meshes, radius, length)), rot)
            }
            StaticKind::SewerPipeBend | StaticKind::PipeBore => {
                let (radius, length, rot) = level::bend_pipe(def);
                (Mesh3d(cylinder_mesh(meshes, radius, length)), rot)
            }
            StaticKind::PipeRing => {
                // Inner radius = the opening (= stream width); add wall thickness
                // OUTSIDE so the inner diameter is unchanged.
                let (radius, length, rot) = level::bend_pipe(def);
                (Mesh3d(pipe_tube_mesh(meshes, radius, radius + 0.05, length)), rot)
            }
            StaticKind::PipeElbow => {
                // size = (pipe_radius, arc_radius, stub_len); rotation orients it.
                (Mesh3d(pipe_elbow_mesh(meshes, def.size.x, def.size.y, def.size.z)), def.rotation)
            }
            _ => (Mesh3d(cuboid_mesh(meshes, def.size)), def.rotation),
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

        let intensity = if test_map {
            light.intensity * 4.0
        } else {
            light.intensity
        };

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

        if test_map {
            commands.spawn((
                LevelVisual,
                PointLight {
                    color: light.color,
                    intensity,
                    range: light.range.max(28.0),
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_translation(light.position),
            ));
        } else {
            commands.spawn((
                LevelVisual,
                PointLight {
                    color: light.color,
                    intensity,
                    range: light.range,
                    shadows_enabled: false,
                    ..default()
                },
                VolumetricLight,
                Transform::from_translation(light.position),
            ));
        }
    }

    if test_map {
        for (x, z) in [(-8.0_f32, 8.0), (4.0, 14.0), (16.0, 20.0)] {
            commands.spawn((
                LevelVisual,
                PointLight {
                    color: Color::srgb(0.85, 0.92, 1.0),
                    intensity: 1_500_000.0,
                    range: 40.0,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_xyz(x, 5.5, z),
            ));
        }
    }

    spawn_level_atmosphere(commands, atmosphere, &level.statics, test_map);

    info!(
        "level visuals: {} statics, {} lights, {} meshes spawned (test_map={test_map})",
        level.statics.len(),
        level.lights.len(),
        level.statics.len(),
    );
}
