//! Underground atmosphere: dim but readable base lighting + optional flashlight.

use bevy::light::VolumetricLight;
use bevy::prelude::*;
use shared::items;
use crate::hotbar::OwnInventory;
use crate::netplay::{LookAngles, OwnPlayer};

pub struct DarknessPlugin;

impl Plugin for DarknessPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GlobalAmbientLight {
            color: Color::srgb(0.55, 0.58, 0.7),
            brightness: 200.0,
            ..default()
        })
        .init_resource::<FlashlightOn>()
        .add_systems(Startup, (spawn_level_fill_light, spawn_flashlight))
        .add_systems(Update, (drive_flashlight, hide_dead_players));
    }
}

/// Very faint directional fill — just enough for normal maps to show surface
/// detail in the darkest corners. Real illumination comes from level PointLights.
fn spawn_level_fill_light(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(0.6, 0.65, 0.8),
            illuminance: 450.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -1.35, 0.25, 0.0)),
    ));
}

#[derive(Component)]
struct PlayerFlashlight;

fn spawn_flashlight(mut commands: Commands) {
    commands.spawn((
        PlayerFlashlight,
        SpotLight {
            intensity: 120_000.0,
            range: 28.0,
            outer_angle: 0.55,
            inner_angle: 0.35,
            shadows_enabled: false,
            color: Color::srgb(0.85, 0.92, 1.0),
            ..default()
        },
        VolumetricLight,
        Transform::IDENTITY,
    ));
}

#[derive(Resource)]
struct FlashlightOn {
    on: bool,
}

impl Default for FlashlightOn {
    fn default() -> Self {
        Self { on: true }
    }
}

fn drive_flashlight(
    keys: Res<ButtonInput<KeyCode>>,
    mut on: ResMut<FlashlightOn>,
    inventory: Res<OwnInventory>,
    look: Res<LookAngles>,
    player: Query<&Transform, (With<OwnPlayer>, Without<PlayerFlashlight>)>,
    mut light: Query<
        (&mut SpotLight, &mut Transform),
        (With<PlayerFlashlight>, Without<OwnPlayer>),
    >,
) {
    let has_light = inventory.slots.iter().any(|s| s.as_ref().is_some_and(items::is_flashlight));
    if keys.just_pressed(KeyCode::KeyF) && has_light {
        on.on = !on.on;
    }
    let Ok(player) = player.single() else { return };
    let Ok((mut spot, mut transform)) = light.single_mut() else {
        return;
    };
    let active = has_light && on.on;
    spot.intensity = if active { 120_000.0 } else { 0.0 };
    let eye = player.translation + Vec3::Y * shared::config::PLAYER_EYE_HEIGHT;
    transform.translation = eye;
    transform.rotation = Quat::from_euler(EulerRot::YXZ, look.yaw, look.pitch, 0.0);
}

fn hide_dead_players(
    mut players: Query<(&shared::protocol::PlayerAlive, &mut Visibility), With<shared::protocol::Player>>,
) {
    for (alive, mut vis) in &mut players {
        *vis = if alive.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}
