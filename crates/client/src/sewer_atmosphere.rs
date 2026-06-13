//! Toxic gas (volumetric fog) and sparse, soft, glowing air motes (dust).

use bevy::asset::RenderAssetUsages;
use bevy::image::Image;
use bevy::light::FogVolume;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_hanabi::prelude::*;
use bevy_hanabi::Gradient;
use shared::level::{StaticDef, StaticKind};

use crate::fog_noise::AnimatedFogVolume;
use crate::level_render::LevelVisual;

pub struct SewerAtmospherePlugin;

impl Plugin for SewerAtmospherePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(HanabiPlugin)
            .add_systems(Startup, setup_atmosphere_effects)
            .insert_resource(AtmosphereEffects::default());
    }
}

#[derive(Resource, Default)]
pub struct AtmosphereEffects {
    pub air_motes: Option<Handle<EffectAsset>>,
    /// Soft radial alpha texture so motes render as fuzzy dots, not hard squares.
    pub dust_tex: Option<Handle<Image>>,
}

pub type CorruptionEffectHandle = AtmosphereEffects;

fn setup_atmosphere_effects(
    mut effects: ResMut<Assets<EffectAsset>>,
    mut images: ResMut<Assets<Image>>,
    mut handles: ResMut<AtmosphereEffects>,
) {
    handles.dust_tex = Some(images.add(make_soft_dot(64)));
    handles.air_motes = Some(effects.add(build_air_motes_effect()));
}

/// Build an `R8Unorm` texture with a smooth radial falloff (1.0 at center →
/// 0.0 at the edge). Used as the particle opacity mask via
/// `ImageSampleMapping::ModulateOpacityFromR`, turning each billboard quad
/// into a soft round glow instead of a hard square.
fn make_soft_dot(size: u32) -> Image {
    let mut data = vec![0u8; (size * size) as usize];
    let c = (size as f32 - 1.0) * 0.5;
    let r = c;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - c;
            let dy = y as f32 - c;
            let dist = (dx * dx + dy * dy).sqrt() / r;
            // Smooth quadratic falloff; fully transparent beyond the radius.
            let a = (1.0 - dist.clamp(0.0, 1.0)).powf(1.8);
            data[(y * size + x) as usize] = (a * 255.0) as u8;
        }
    }
    Image::new(
        Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::R8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    )
}

fn build_air_motes_effect() -> EffectAsset {
    // Soft glowing motes drifting through the air — the TLOU2 / Stranger Things
    // look. The radial texture makes them round and fuzzy; high drag keeps them
    // nearly stationary. Sparse: rate 2/s, 18 s life → ~36 alive at once.
    let mut gradient = Gradient::new();
    gradient.add_key(0.0,  Vec4::new(0.95, 0.93, 0.82, 0.0));
    gradient.add_key(0.12, Vec4::new(0.98, 0.96, 0.85, 0.85));
    gradient.add_key(0.85, Vec4::new(0.92, 0.90, 0.80, 0.7));
    gradient.add_key(1.0,  Vec4::new(0.90, 0.88, 0.78, 0.0));

    let mut module = Module::default();
    let init_pos = SetPositionSphereModifier {
        center: module.lit(Vec3::ZERO),
        radius: module.lit(9.0),
        dimension: ShapeDimension::Volume,
    };
    let init_vel = SetVelocitySphereModifier {
        center: module.lit(Vec3::ZERO),
        speed: module.lit(0.06),
    };
    let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, module.lit(18.0));
    // 4 cm soft dots — large enough to read as glowing motes, soft enough
    // (via the radial texture) to never look like geometric squares.
    let init_size = SetAttributeModifier::new(Attribute::SIZE, module.lit(0.04_f32));
    let drag = LinearDragModifier::constant(&mut module, 3.0);
    let update_accel = AccelModifier::new(module.lit(Vec3::new(0.0, 0.008, 0.0)));

    // Texture slot 0 → bound to the soft-dot image via `EffectMaterial`.
    let texture_slot = module.lit(0u32);
    module.add_texture_slot("color");

    EffectAsset::new(600, SpawnerSettings::rate(2.0.into()), module)
        .with_name("sewer_air_motes")
        .with_alpha_mode(bevy_hanabi::AlphaMode::Blend)
        .init(init_pos)
        .init(init_vel)
        .init(init_lifetime)
        .init(init_size)
        .update(drag)
        .update(update_accel)
        // ORDER MATTERS: set the lifetime color/alpha FIRST, then let the
        // radial texture multiply the alpha (ModulateOpacityFromR) so the soft
        // round shape survives. Doing it the other way overwrites the texture
        // and leaves a hard square.
        .render(ColorOverLifetimeModifier {
            gradient,
            blend: ColorBlendMode::Overwrite,
            mask: ColorBlendMask::RGBA,
        })
        .render(ParticleTextureModifier {
            texture_slot,
            sample_mapping: ImageSampleMapping::ModulateOpacityFromR,
        })
        .render(OrientModifier::new(OrientMode::FaceCameraPosition))
}

fn channel_steam_fog() -> FogVolume {
    FogVolume {
        fog_color: Color::srgba(0.10, 0.6, 0.26, 1.0),
        // Moderate base density. The 3D noise density texture (FogNoisePlugin)
        // varies this in space so it reads as rolling wisps, not a solid box.
        density_factor: 0.22,
        absorption: 0.05,
        scattering: 0.9,
        scattering_asymmetry: 0.55,
        light_tint: Color::srgb(0.25, 1.0, 0.5),
        light_intensity: 1.4,
        ..default()
    }
}

fn tunnel_haze_fog() -> FogVolume {
    FogVolume {
        fog_color: Color::srgba(0.42, 0.48, 0.52, 1.0),
        density_factor: 0.12,
        absorption: 0.10,
        scattering: 0.5,
        scattering_asymmetry: 0.45,
        light_tint: Color::srgb(0.6, 0.68, 0.72),
        light_intensity: 0.6,
        ..default()
    }
}

pub fn attach_water_atmosphere(commands: &mut Commands, def: &StaticDef) {
    let center = def.position;
    let size = def.size;
    // Tall animated FogVolume rising off the water — 3 m fills the air at and
    // above player eye height so the toxic green steam is clearly visible, not
    // a thin strip on the floor. Widened past the channel so it drifts over the
    // walkways too.
    commands.spawn((
        LevelVisual,
        AnimatedFogVolume,
        channel_steam_fog(),
        Transform::from_translation(center + Vec3::Y * 1.5)
            .with_scale(Vec3::new(size.x.max(2.0) * 1.6, 3.0, size.z.max(2.0) * 1.6)),
    ));
}

fn is_walk_surface(kind: StaticKind) -> bool {
    matches!(kind, StaticKind::SewerFloor | StaticKind::SewerWalkway)
}

pub fn spawn_level_atmosphere(
    commands: &mut Commands,
    effects: &AtmosphereEffects,
    statics: &[StaticDef],
) {
    let Some(motes) = effects.air_motes.as_ref() else {
        return;
    };
    let dust_tex = effects.dust_tex.clone();

    let mut emitter_count = 0u32;
    const MAX_EMITTERS: u32 = 6;

    for def in statics {
        if !is_walk_surface(def.kind) {
            continue;
        }
        let span = def.size.x.max(def.size.z);
        if span < 6.0 {
            continue;
        }

        commands.spawn((
            LevelVisual,
            AnimatedFogVolume,
            tunnel_haze_fog(),
            Transform::from_translation(def.position + Vec3::Y * 1.6)
                .with_scale(Vec3::new(def.size.x * 0.95, 3.2, def.size.z * 0.95)),
        ));

        if span >= 8.0 && emitter_count < MAX_EMITTERS {
            let mut e = commands.spawn((
                LevelVisual,
                ParticleEffect::new(motes.clone()),
                Transform::from_translation(def.position + Vec3::Y * 1.1),
            ));
            // Bind the soft-dot texture to the effect's texture slot 0.
            if let Some(tex) = dust_tex.clone() {
                e.insert(EffectMaterial { images: vec![tex] });
            }
            emitter_count += 1;
        }
    }
}
