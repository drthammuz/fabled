//! Procedural 3D noise texture for `FogVolume` density masks.

use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::FogVolume;
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat,
};
use bevy::asset::RenderAssetUsages;

const FOG_RES: u32 = 32;

/// Shared 3D density texture + animated UVW offset for fog volumes.
#[derive(Resource)]
pub struct FogNoiseTexture {
    pub handle: Handle<Image>,
    pub offset: Vec3,
}

pub struct FogNoisePlugin;

impl Plugin for FogNoisePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init_fog_noise)
            .add_systems(Update, (animate_fog_noise, sync_fog_offsets).chain());
    }
}

fn init_fog_noise(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(generate_fog_noise(&mut images));
}

pub fn generate_fog_noise(images: &mut Assets<Image>) -> FogNoiseTexture {
    let mut data = Vec::with_capacity((FOG_RES * FOG_RES * FOG_RES) as usize);
    for z in 0..FOG_RES {
        for y in 0..FOG_RES {
            for x in 0..FOG_RES {
                let fx = x as f32 / FOG_RES as f32;
                let fy = y as f32 / FOG_RES as f32;
                let fz = z as f32 / FOG_RES as f32;
                // Two octaves of value noise for rolling cloud structure.
                let n1 = value_noise_3d(fx * 3.0, fy * 3.0, fz * 3.0);
                let n2 = value_noise_3d(fx * 6.0, fy * 6.0, fz * 6.0);
                let n = (n1 * 0.65 + n2 * 0.35).clamp(0.0, 1.0);
                // Keep a solid floor (0.35) so fog is always present, with
                // variation up to 1.0 for wispy structure — never a flat box,
                // never invisible. Denser near the floor (fog settles low).
                let height_bias = (1.0 - fy * 0.6).clamp(0.3, 1.0);
                let density = (0.35 + 0.65 * n) * height_bias;
                data.push((density.clamp(0.0, 1.0) * 255.0) as u8);
            }
        }
    }

    let size = Extent3d {
        width: FOG_RES,
        height: FOG_RES,
        depth_or_array_layers: FOG_RES,
    };
    let mut image = Image::new(
        size,
        TextureDimension::D3,
        data,
        TextureFormat::R8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });

    FogNoiseTexture {
        handle: images.add(image),
        offset: Vec3::ZERO,
    }
}

fn animate_fog_noise(time: Res<Time>, mut fog: ResMut<FogNoiseTexture>) {
    let t = time.elapsed_secs();
    fog.offset = Vec3::new(t * 0.02, 0.0, t * 0.01);
}

#[derive(Component)]
pub struct AnimatedFogVolume;

fn sync_fog_offsets(
    fog: Res<FogNoiseTexture>,
    mut volumes: Query<&mut FogVolume, With<AnimatedFogVolume>>,
) {
    for mut volume in &mut volumes {
        volume.density_texture = Some(fog.handle.clone());
        volume.density_texture_offset = fog.offset;
    }
}

fn value_noise_3d(x: f32, y: f32, z: f32) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let zi = z.floor() as i32;
    let xf = x - x.floor();
    let yf = y - y.floor();
    let zf = z - z.floor();
    let u = smooth(xf);
    let v = smooth(yf);
    let w = smooth(zf);

    let c000 = hash01(xi, yi, zi);
    let c100 = hash01(xi + 1, yi, zi);
    let c010 = hash01(xi, yi + 1, zi);
    let c110 = hash01(xi + 1, yi + 1, zi);
    let c001 = hash01(xi, yi, zi + 1);
    let c101 = hash01(xi + 1, yi, zi + 1);
    let c011 = hash01(xi, yi + 1, zi + 1);
    let c111 = hash01(xi + 1, yi + 1, zi + 1);

    let x00 = lerp(c000, c100, u);
    let x10 = lerp(c010, c110, u);
    let x01 = lerp(c001, c101, u);
    let x11 = lerp(c011, c111, u);
    let y0 = lerp(x00, x10, v);
    let y1 = lerp(x01, x11, v);
    lerp(y0, y1, w)
}

fn hash01(x: i32, y: i32, z: i32) -> f32 {
    let mut h = (x as u32)
        .wrapping_mul(374761393)
        .wrapping_add(y as u32)
        .wrapping_mul(668265263)
        .wrapping_add(z as u32);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    (h & 0xFFFF) as f32 / 65535.0
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
