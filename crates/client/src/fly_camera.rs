//! Debug fly camera for level inspection (M1). Client-side only.
//!
//! Controls: left-click the window to capture the mouse, Esc to release.
//! WASD to move, Space/Ctrl for up/down, Shift to fly fast.

use bevy::camera::Exposure;
use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;
use bevy::render::view::Hdr;
use bevy::light::VolumetricFog;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use shared::config;

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
                    toggle_cursor_grab,
                    toggle_third_person,
                    (look, fly)
                        .chain()
                        .run_if(not(any_with_component::<crate::netplay::OwnPlayer>)),
                )
                    .chain(),
            );
    }
}

#[derive(Component, Default)]
pub struct FlyCamera {
    yaw: f32,
    pitch: f32,
}

fn spawn_camera(mut commands: Commands) {
    let transform = Transform::from_xyz(0.0, 10.0, 28.0).looking_at(Vec3::ZERO, Vec3::Y);
    let (yaw, pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
    commands.spawn((
        Camera3d::default(),
        Exposure { ev100: 5.5 },
        Hdr,
        VolumetricFog {
            ambient_color: Color::srgb(0.45, 0.52, 0.55),
            ambient_intensity: 0.15,
            step_count: 40,
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
    mut camera: Single<(&mut Transform, &mut FlyCamera)>,
) {
    if !cursor_grabbed(&window) {
        motion.clear();
        return;
    }
    let (transform, cam) = &mut *camera;
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
    mut camera: Single<&mut Transform, With<FlyCamera>>,
) {
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
