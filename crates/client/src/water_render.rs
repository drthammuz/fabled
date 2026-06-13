//! Animated sewer water via `bevy_water`, plus splash VFX on `WaterImpact`.

use std::time::Duration;

use bevy::light::NotShadowCaster;
use bevy::mesh::PlaneMeshBuilder;
use bevy::prelude::*;
use bevy::prelude::AlphaMode;
use bevy_hanabi::prelude::*;
use bevy_hanabi::Gradient;
use bevy_water::material::{StandardWaterMaterial, WaterMaterial};
use bevy_water::{WaterPlugin, WaterQuality, WaterSettings, WaterTile, WaveDirection};
use shared::config;
use shared::level::StaticDef;
use shared::protocol::WaterImpact;

use crate::level_render::LevelVisual;

pub struct WaterRenderPlugin;

impl Plugin for WaterRenderPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(sewer_water_settings())
            .add_plugins(WaterPlugin)
            .init_resource::<WaterSplashEffects>()
            .add_systems(Startup, setup_splash_effect)
            .add_systems(Update, (spawn_water_splashes, despawn_splashes));
    }
}

#[derive(Resource, Default)]
pub struct SewerWaterMesh(pub Option<Handle<Mesh>>);

#[derive(Resource, Default)]
pub struct WaterSplashEffects {
    pub splash: Option<Handle<EffectAsset>>,
}

#[derive(Component)]
pub struct SewerWaterChannel(pub u32);

fn sewer_water_settings() -> WaterSettings {
    let (r, g, b, a) = config::WATER_BASE_COLOR;
    let (sr, sg, sb, sa) = config::WATER_SHALLOW_COLOR;
    let (dr, dg, db, da) = config::WATER_DEEP_COLOR;
    WaterSettings {
        spawn_tiles: None,
        height: config::WATER_SURFACE_HEIGHT,
        amplitude: config::WATER_WAVE_AMPLITUDE,
        base_color: Color::srgba(r, g, b, a),
        shallow_color: Color::srgba(sr, sg, sb, sa),
        deep_color: Color::srgba(dr, dg, db, da),
        clarity: 0.55,
        water_quality: WaterQuality::Medium,
        wave_direction: Vec2::new(0.35, 0.85),
        alpha_mode: bevy::prelude::AlphaMode::Blend,
        ..default()
    }
}

fn build_channel_plane(meshes: &mut Assets<Mesh>, width: f32, depth: f32) -> Handle<Mesh> {
    let subdiv = ((width.max(depth) / 2.0).ceil() as u32).clamp(4, 32);
    let mut plane = PlaneMeshBuilder::from_size(Vec2::new(width, depth))
        .subdivisions(subdiv)
        .build();
    if let Err(e) = plane.generate_tangents() {
        warn!("water mesh tangents: {e}");
    }
    meshes.add(plane)
}

/// Spawn one animated water surface sized to the channel static.
/// Uses bevy_water's `StandardWaterMaterial` (Gerstner-style vertex waves +
/// PBR reflection) so the channel ripples and reflects like real water.
pub fn spawn_channel_water(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    water_materials: &mut Assets<StandardWaterMaterial>,
    def: &StaticDef,
    channel_id: u32,
) {
    let width = def.size.x.max(0.5);
    let depth = def.size.z.max(0.5);
    let mesh = build_channel_plane(meshes, width, depth);

    let (sr, sg, sb, sa) = config::WATER_SHALLOW_COLOR;
    let (dr, dg, db, da) = config::WATER_DEEP_COLOR;
    let wave_dir = Vec2::new(0.35, 0.85);

    let material = water_materials.add(StandardWaterMaterial {
        base: StandardMaterial {
            base_color: Color::srgba(0.06, 0.5, 0.22, 0.92),
            // Faint glow so the channel reads even in near-total darkness.
            emissive: LinearRgba::new(0.01, 0.12, 0.05, 1.0),
            perceptual_roughness: 0.16,
            metallic: 0.0,
            reflectance: 0.7,
            alpha_mode: AlphaMode::Blend,
            ..default()
        },
        extension: WaterMaterial {
            // Gentle ripples for a shallow sewer stream. Kept small so wave
            // troughs never dip below the channel floor (which would occlude
            // the alpha-blended surface).
            amplitude: 0.02,
            clarity: 0.4,
            deep_color: Color::srgba(dr, dg, db, da),
            shallow_color: Color::srgba(sr, sg, sb, sa),
            edge_color: Color::srgb(0.25, 0.95, 0.45),
            edge_scale: 0.25,
            // Map mesh UV (0..1) to a few wavelengths across the channel.
            coord_offset: Vec2::new(def.position.x - width * 0.5, def.position.z - depth * 0.5),
            coord_scale: Vec2::new(width, depth),
            wave_dir_a: wave_dir.normalize(),
            wave_dir_b: wave_dir.normalize(),
            wave_blend: 1.0,
            quality: 4,
        },
    });

    // Place the surface just above the top of the channel volume so the whole
    // (small-amplitude) wave stays above the surrounding floor and stays visible.
    let surface_y = def.position.y + def.size.y * 0.5 + 0.05;

    commands.spawn((
        LevelVisual,
        SewerWaterChannel(channel_id),
        WaterTile { offset: Vec2::new(def.position.x, def.position.z) },
        WaveDirection::new(wave_dir),
        Mesh3d(mesh),
        MeshMaterial3d(material),
        NotShadowCaster,
        Transform::from_translation(Vec3::new(def.position.x, surface_y, def.position.z)),
    ));
}

fn setup_splash_effect(
    mut effects: ResMut<Assets<EffectAsset>>,
    mut handles: ResMut<WaterSplashEffects>,
) {
    let mut gradient = Gradient::new();
    gradient.add_key(0.0, Vec4::new(0.15, 0.9, 0.35, 0.9));
    gradient.add_key(0.4, Vec4::new(0.08, 0.55, 0.22, 0.5));
    gradient.add_key(1.0, Vec4::ZERO);

    let mut module = Module::default();
    let init_pos = SetPositionSphereModifier {
        center: module.lit(Vec3::ZERO),
        radius: module.lit(0.35),
        dimension: ShapeDimension::Volume,
    };
    let init_vel = SetVelocitySphereModifier {
        center: module.lit(Vec3::ZERO),
        speed: module.lit(1.8),
    };
    let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, module.lit(0.55));
    let init_size = SetAttributeModifier::new(Attribute::SIZE, module.lit(0.04));
    let update_accel = AccelModifier::new(module.lit(Vec3::new(0.0, -2.5, 0.0)));

    handles.splash = Some(effects.add(
        EffectAsset::new(128, SpawnerSettings::once(24.0.into()), module)
            .init(init_pos)
            .init(init_vel)
            .init(init_lifetime)
            .init(init_size)
            .update(update_accel)
            .render(ColorOverLifetimeModifier {
                gradient,
                blend: ColorBlendMode::Overwrite,
                mask: ColorBlendMask::RGBA,
            })
            .render(OrientModifier::new(OrientMode::FaceCameraPosition)),
    ));
}

#[derive(Component)]
struct SplashDespawn(Timer);

fn spawn_water_splashes(
    mut commands: Commands,
    mut reader: MessageReader<WaterImpact>,
    splash: Res<WaterSplashEffects>,
    settings: Res<WaterSettings>,
) {
    let Some(effect) = splash.splash.as_ref() else {
        return;
    };
    for impact in reader.read() {
        let scale = (impact.impulse / 4.0).clamp(0.4, 2.5);
        let pos = Vec3::new(
            impact.position.x,
            settings.height + 0.02,
            impact.position.z,
        );
        commands.spawn((
            ParticleEffect::new(effect.clone()),
            Transform::from_translation(pos).with_scale(Vec3::splat(scale)),
            SplashDespawn(Timer::new(Duration::from_secs_f32(1.2), TimerMode::Once)),
        ));
    }
}

fn despawn_splashes(
    mut commands: Commands,
    time: Res<Time>,
    mut splashes: Query<(Entity, &mut SplashDespawn)>,
) {
    for (entity, mut timer) in &mut splashes {
        timer.0.tick(time.delta());
        if timer.0.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}
