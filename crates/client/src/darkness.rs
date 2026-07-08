//! Underground atmosphere: dim but readable base lighting + optional flashlight.

use bevy::light::VolumetricLight;
use bevy::prelude::*;
use shared::items;
use shared::EditorMode;
use shared::CityViewMode;
use shared::TestMode;
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
        .add_systems(Startup, (apply_scene_brightness, spawn_level_fill_light, spawn_flashlight))
        .add_systems(Update, (drive_flashlight, hide_dead_players));
    }
}

/// Editor + playtest: bright-enough defaults so empty layouts stay readable.
const DEV_AMBIENT_BRIGHTNESS: f32 = 8_000.0;
const DEV_FILL_ILLUMINANCE: f32 = 45_000.0;
const DEV_FILL_ILLUMINANCE_SOFT: f32 = 18_000.0;
/// Real pool games are lit to match editor+G, where the maps were authored
/// and tuned — same ambient + a single straight-down overhead sun. (The
/// legacy sewer's near-black ambient assumes a dense PointLight grid the pool
/// maps don't have; the old dev-preview flood used two *angled* directionals
/// that glinted a bright streak across every ceiling.)
const REAL_GAME_AMBIENT_BRIGHTNESS: f32 = 3_000.0;
const REAL_GAME_SUN_ILLUMINANCE: f32 = 34_000.0;

/// True only for the flat developer test map / editor preview — NOT real
/// pool-game sessions (`RealGameRun`), which ride on the same `TestMode` but
/// need proper atmospheric lighting (single soft directional + shadows), not
/// the "empty layout stays readable" flood lighting meant for a bare editor
/// canvas. Two bright, always-on directional lights produced a hard specular
/// highlight on every ceiling from a constant world-space direction — visible
/// in every room since directional lights have no position, only direction.
fn is_dev_preview(
    test: Option<&TestMode>,
    editor: Option<&EditorMode>,
    _city: Option<&CityViewMode>,
    real_game: Option<&shared::RealGameRun>,
) -> bool {
    (test.is_some() || editor.is_some()) && real_game.is_none()
}

/// City / editor / playtest scene lighting.
fn apply_scene_brightness(
    test: Option<Res<TestMode>>,
    editor: Option<Res<EditorMode>>,
    city: Option<Res<CityViewMode>>,
    real_game: Option<Res<shared::RealGameRun>>,
    mut commands: Commands,
    mut ambient: ResMut<GlobalAmbientLight>,
) {
    if city.is_some() {
        ambient.color = Color::srgb(0.92, 0.94, 1.0);
        ambient.brightness = 1_200.0;
        commands.insert_resource(ClearColor(Color::srgb(0.55, 0.72, 0.92)));
        return;
    }
    if editor.is_some() {
        ambient.color = Color::srgb(0.68, 0.72, 0.78);
        ambient.brightness = 2_200.0;
        commands.insert_resource(ClearColor(Color::srgb(0.36, 0.40, 0.46)));
        return;
    }
    if real_game.is_some() {
        // Match editor+G ambient exactly (see REAL_GAME_AMBIENT_BRIGHTNESS).
        ambient.color = Color::srgb(0.68, 0.72, 0.78);
        ambient.brightness = REAL_GAME_AMBIENT_BRIGHTNESS;
        commands.insert_resource(ClearColor(Color::srgb(0.10, 0.11, 0.14)));
        return;
    }
    if !is_dev_preview(test.as_deref(), editor.as_deref(), city.as_deref(), real_game.as_deref()) {
        return;
    }
    ambient.color = Color::srgb(0.82, 0.86, 0.95);
    ambient.brightness = DEV_AMBIENT_BRIGHTNESS;
    commands.insert_resource(ClearColor(Color::srgb(0.18, 0.20, 0.26)));
}

/// Very faint directional fill — just enough for normal maps to show surface
/// detail in the darkest corners. Real illumination comes from level PointLights.
fn spawn_level_fill_light(
    mut commands: Commands,
    test: Option<Res<TestMode>>,
    editor: Option<Res<EditorMode>>,
    city: Option<Res<CityViewMode>>,
    real_game: Option<Res<shared::RealGameRun>>,
) {
    if city.is_some() {
        commands.spawn((
            DirectionalLight {
                color: Color::srgb(1.0, 0.98, 0.92),
                illuminance: 28_000.0,
                shadows_enabled: true,
                ..default()
            },
            Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.85, 0.35, 0.0)),
        ));
        return;
    }
    if editor.is_some() {
        // Kenney editor spawns its own overhead fill via spawn_editor_sun.
        return;
    }
    if real_game.is_some() {
        // Straight-down overhead sun, matching the editor's `spawn_editor_sun`.
        // Pointing straight down (not angled) means no bright specular streak
        // reflected off the ceilings toward the camera.
        commands.spawn((
            DirectionalLight {
                color: Color::srgb(0.92, 0.94, 0.98),
                illuminance: REAL_GAME_SUN_ILLUMINANCE,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        ));
        return;
    }
    let dev = is_dev_preview(test.as_deref(), editor.as_deref(), city.as_deref(), real_game.as_deref());
    if dev {
        commands.spawn((
            DirectionalLight {
                color: Color::srgb(0.82, 0.88, 1.0),
                illuminance: DEV_FILL_ILLUMINANCE,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -1.35, 0.25, 0.0)),
        ));
        commands.spawn((
            DirectionalLight {
                color: Color::srgb(0.68, 0.74, 0.88),
                illuminance: DEV_FILL_ILLUMINANCE_SOFT,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.55, -2.0, 0.0)),
        ));
        return;
    }
    let rotation = Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -1.35, 0.25, 0.0));
    commands.spawn((
            DirectionalLight {
                color: Color::srgb(0.6, 0.65, 0.8),
                illuminance: 450.0,
                shadows_enabled: true,
                ..default()
            },
            VolumetricLight,
            rotation,
        ));
}

#[derive(Component)]
struct PlayerFlashlight;

fn spawn_flashlight(mut commands: Commands, test: Option<Res<TestMode>>) {
    let mut entity = commands.spawn((
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
        Transform::IDENTITY,
    ));
    if test.is_none() {
        entity.insert(VolumetricLight);
    }
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
    capture: Res<crate::netplay::InputCapture>,
    test: Option<Res<TestMode>>,
    city: Option<Res<CityViewMode>>,
    real_game: Option<Res<shared::RealGameRun>>,
    mut on: ResMut<FlashlightOn>,
    inventory: Res<OwnInventory>,
    look: Res<LookAngles>,
    player: Query<&Transform, (With<OwnPlayer>, Without<PlayerFlashlight>)>,
    mut light: Query<
        (&mut SpotLight, &mut Transform),
        (With<PlayerFlashlight>, Without<OwnPlayer>),
    >,
) {
    // Only the flat dev showcase map / city view get a free always-on light. The
    // REAL game rides on TestMode too, but there the flashlight must be an actual
    // item you carry (buyable from vendors) and toggle with F — otherwise every
    // player spawns with a phantom beam that reads like a free 3rd inventory item.
    let auto_light = (test.is_some() && real_game.is_none()) || city.is_some();
    let has_light = auto_light
        || inventory.slots.iter().any(|s| s.as_ref().is_some_and(items::is_flashlight));
    if !capture.0 && keys.just_pressed(KeyCode::KeyF) && has_light && !auto_light {
        on.on = !on.on;
    }
    let Ok(player) = player.single() else { return };
    let Ok((mut spot, mut transform)) = light.single_mut() else {
        return;
    };
    let active = has_light && on.on;
    let intensity = if auto_light { 400_000.0 } else { 120_000.0 };
    spot.intensity = if active { intensity } else { 0.0 };
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
