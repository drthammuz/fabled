//! Debug fly camera for level inspection (M1). Client-side only.
//!
//! Controls: left-click the window to capture the mouse, Esc to release.
//! WASD to move, Space/Ctrl for up/down, Shift to fly fast.

use bevy::camera::Exposure;
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;
use bevy::render::view::Hdr;
use bevy::light::VolumetricFog;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy::ecs::schedule::common_conditions::{any_with_component, not, resource_exists};
use shared::config;
use shared::EditorMode;
use shared::CityViewMode;
use shared::{TestMapStyle, TestMode};

use crate::editor_playtest::EditorPlaytestActive;

/// Whether the player is viewing in third-person (middle-mouse toggle).
#[derive(Resource, Default)]
pub struct ThirdPersonMode(pub bool);

pub struct FlyCameraPlugin;

impl Plugin for FlyCameraPlugin {
    fn build(&self, app: &mut App) {
        // Free flight is only active until the server gives us a player to
        // possess; after that the camera is driven first-person by netplay.
        app.init_resource::<ThirdPersonMode>()
            .add_systems(Startup, spawn_camera)
            .add_systems(
                Update,
                (
                    (
                        toggle_cursor_grab,
                        toggle_third_person,
                    )
                        .run_if(not(resource_exists::<EditorMode>)),
                    (look, fly)
                        .chain()
                        .run_if(
                            not(any_with_component::<crate::netplay::OwnPlayer>)
                                .and(not(resource_exists::<EditorPlaytestActive>)),
                        ),
                ),
            );
    }
}

#[derive(Component, Default)]
pub struct FlyCamera {
    pub yaw: f32,
    pub pitch: f32,
}

fn spawn_camera(
    mut commands: Commands,
    editor: Option<Res<EditorMode>>,
    city: Option<Res<CityViewMode>>,
    test: Option<Res<TestMode>>,
) {
    if editor.is_some() || city.is_some() {
        return;
    }
    let is_test = test.is_some();
    let transform = kenney_overview_transform(test.as_deref())
        .unwrap_or_else(|| Transform::from_xyz(0.0, 10.0, 28.0).looking_at(Vec3::ZERO, Vec3::Y));
    let (yaw, pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
    if is_test {
        commands.spawn((
            Camera3d::default(),
            Exposure { ev100: 13.5 },
            transform,
            FlyCamera { yaw, pitch },
        ));
        return;
    }
    commands.spawn((
        Camera3d::default(),
        Exposure { ev100: 5.5 },
        Hdr,
        // Required by bevy_water (depth_prepass feature) for depth-based deep/
        // shallow water colour and shoreline foam — without it the water shader
        // can't read scene depth and renders as a flat single colour.
        DepthPrepass,
        VolumetricFog {
            // ambient_* lights the fog uniformly even with no direct light
            // shaft — essential indoors (the ceiling blocks the directional
            // light). Low values made the fog invisible.
            ambient_color: Color::srgb(0.35, 0.55, 0.42),
            ambient_intensity: 0.5,
            step_count: 56,
            ..default()
        },
        DistanceFog {
            color: Color::srgba(0.02, 0.02, 0.05, 1.0),
            falloff: FogFalloff::ExponentialSquared { density: 0.032 },
            ..default()
        },
        transform,
        FlyCamera { yaw, pitch },
    ));
}

fn kenney_overview_transform(test: Option<&TestMode>) -> Option<Transform> {
    let test = test?;
    if test.style != TestMapStyle::Kenney {
        return None;
    }
    let layout = shared::map_pool::test_play_layout();
    let look = layout
        .spawn_xz
        .map(|[sx, sz]| Vec3::new(sx, 0.0, sz))
        .unwrap_or_else(|| {
            let focus = layout.focus_xz();
            Vec3::new(focus.x, 0.0, focus.y)
        });
    Some(
        Transform::from_translation(look + Vec3::new(0.0, 28.0, -18.0)).looking_at(look, Vec3::Y),
    )
}

fn cursor_grabbed(options: &CursorOptions) -> bool {
    options.grab_mode != CursorGrabMode::None
}

fn toggle_third_person(
    mouse: Res<ButtonInput<MouseButton>>,
    mut mode: ResMut<ThirdPersonMode>,
) {
    if mouse.just_pressed(MouseButton::Middle) {
        mode.0 = !mode.0;
    }
}

fn toggle_cursor_grab(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut window: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if mouse.just_pressed(MouseButton::Left) && !cursor_grabbed(&window) {
        window.grab_mode = CursorGrabMode::Locked;
        window.visible = false;
    }
    if keys.just_pressed(KeyCode::Escape) {
        window.grab_mode = CursorGrabMode::None;
        window.visible = true;
    }
}

fn look(
    mut motion: MessageReader<MouseMotion>,
    window: Single<&CursorOptions, With<PrimaryWindow>>,
    mut camera: Query<(&mut Transform, &mut FlyCamera), With<FlyCamera>>,
) {
    if !cursor_grabbed(&window) {
        motion.clear();
        return;
    }
    let Ok((mut transform, mut cam)) = camera.single_mut() else {
        motion.clear();
        return;
    };
    for ev in motion.read() {
        cam.yaw -= ev.delta.x * config::FLY_CAM_SENSITIVITY;
        cam.pitch = (cam.pitch - ev.delta.y * config::FLY_CAM_SENSITIVITY)
            .clamp(-1.54, 1.54);
    }
    transform.rotation = Quat::from_euler(EulerRot::YXZ, cam.yaw, cam.pitch, 0.0);
}

fn fly(
    keys: Res<ButtonInput<KeyCode>>,
    window: Single<&CursorOptions, With<PrimaryWindow>>,
    time: Res<Time>,
    mut camera: Query<&mut Transform, With<FlyCamera>>,
) {
    let Ok(mut camera) = camera.single_mut() else {
        return;
    };
    if !cursor_grabbed(&window) {
        return;
    }
    let mut dir = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        dir += *camera.forward();
    }
    if keys.pressed(KeyCode::KeyS) {
        dir -= *camera.forward();
    }
    if keys.pressed(KeyCode::KeyD) {
        dir += *camera.right();
    }
    if keys.pressed(KeyCode::KeyA) {
        dir -= *camera.right();
    }
    if keys.pressed(KeyCode::Space) {
        dir += Vec3::Y;
    }
    if keys.pressed(KeyCode::ControlLeft) {
        dir -= Vec3::Y;
    }
    let mut speed = config::FLY_CAM_SPEED;
    if keys.pressed(KeyCode::ShiftLeft) {
        speed *= config::FLY_CAM_FAST_MULT;
    }
    if let Some(dir) = dir.try_normalize() {
        camera.translation += dir * speed * time.delta_secs();
    }
}
