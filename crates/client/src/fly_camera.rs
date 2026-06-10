//! Debug fly camera for level inspection (M1). Client-side only.
//!
//! Controls: left-click the window to capture the mouse, Esc to release.
//! WASD to move, Space/Ctrl for up/down, Shift to fly fast.

use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use shared::config;

pub struct FlyCameraPlugin;

impl Plugin for FlyCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(Update, (toggle_cursor_grab, look, fly).chain());
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
    commands.spawn((Camera3d::default(), transform, FlyCamera { yaw, pitch }));
}

fn cursor_grabbed(options: &CursorOptions) -> bool {
    options.grab_mode != CursorGrabMode::None
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
