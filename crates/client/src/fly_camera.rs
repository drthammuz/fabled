//! Debug fly camera for level inspection (M1). Client-side only.
//!
//! Controls: left-click the window to capture the mouse, Esc to release.
//! WASD to move, Space/Ctrl for up/down, Shift to fly fast.

use avian3d::collider_tree::ColliderTrees;
use avian3d::prelude::{Collider, Sensor, ShapeCastConfig, SpatialQuery, SpatialQueryFilter};
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

/// World-space point currently under the crosshair: the camera ray cast
/// against geometry (or a far point along it). `netplay::send_input` ships it
/// as `PlayerInput.aim`; the server fires from the muzzle toward it, so shots
/// land on the crosshair in BOTH camera modes (in third person the camera ray
/// starts at the shoulder camera, not the eye — aiming down the eye ray is
/// what made close shots land left of the crosshair).
#[derive(Resource, Default)]
pub struct CrosshairAim(pub Vec3);

pub struct FlyCameraPlugin;

impl Plugin for FlyCameraPlugin {
    fn build(&self, app: &mut App) {
        // Free flight is only active until the server gives us a player to
        // possess; after that the camera is driven first-person by netplay.
        app.init_resource::<ThirdPersonMode>()
            .init_resource::<CrosshairAim>()
            .add_systems(Startup, spawn_camera)
            .add_systems(
                Update,
                (
                    toggle_cursor_grab.run_if(not(resource_exists::<EditorMode>)),
                    // Third person also works during editor playtest (the
                    // editor's middle-mouse orbit is gated off while playing).
                    toggle_third_person.run_if(
                        not(resource_exists::<EditorMode>)
                            .or(resource_exists::<EditorPlaytestActive>),
                    ),
                    (look, fly)
                        .chain()
                        .run_if(
                            not(any_with_component::<crate::netplay::OwnPlayer>)
                                .and(not(resource_exists::<EditorPlaytestActive>)),
                        ),
                    // Camera wall avoidance. Ordered after both third-person
                    // drivers so it corrects the transform they wrote this
                    // frame (no one-frame clip). Gated on the physics pipeline
                    // existing: on a pure remote client there is none.
                    occlude_third_person_camera
                        .after(crate::netplay::drive_first_person_camera)
                        .after(crate::editor_playtest::editor_playtest_camera)
                        .run_if(
                            resource_exists::<ColliderTrees>.and(
                                in_state(crate::class_select::SelectState::Playing)
                                    .or(resource_exists::<EditorPlaytestActive>),
                            ),
                        ),
                    // Crosshair aim point: cast the final camera ray of the
                    // frame (after all camera drivers + wall avoidance).
                    update_crosshair_aim
                        .after(occlude_third_person_camera)
                        .after(crate::netplay::drive_first_person_camera)
                        .after(crate::editor_playtest::editor_playtest_camera)
                        .run_if(resource_exists::<ColliderTrees>),
                    // No physics world (pure remote client): far point on the
                    // camera ray still beats the raw eye ray in third person.
                    update_crosshair_aim_fallback
                        .after(crate::netplay::drive_first_person_camera)
                        .after(crate::editor_playtest::editor_playtest_camera)
                        .run_if(not(resource_exists::<ColliderTrees>)),
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

/// Distance the third-person camera orbits behind the player's eyes.
const THIRD_PERSON_DISTANCE: f32 = 4.5;
/// Extra downward pitch on the camera *boom* (not the view): raises the camera
/// above the look ray so the aim point stays visible over the player model.
const THIRD_PERSON_BOOM_TILT: f32 = 0.20; // ~11.5°
/// Right-shoulder offset so the model sits left of the crosshair.
const THIRD_PERSON_SHOULDER: f32 = 0.55;
/// The camera aims at the look ray this far from the eye, so the view frames
/// roughly what the player looks at. (Shot accuracy no longer depends on this
/// convergence — see [`CrosshairAim`].)
const THIRD_PERSON_AIM_DIST: f32 = 30.0;
/// Radius of the probe sphere used for wall avoidance — also the standoff the
/// camera keeps from geometry (> near plane, so walls never clip open).
const THIRD_PERSON_PROBE_RADIUS: f32 = 0.2;

/// Boom geometry for the over-the-shoulder third-person camera.
pub struct ThirdPersonRig {
    /// Player eye point: boom origin and start of the aim ray.
    pub pivot: Vec3,
    /// Offset from the pivot to the shoulder point (camera-right).
    pub shoulder: Vec3,
    /// Unit vector from the shoulder point back toward the camera.
    pub boom_back: Vec3,
    /// Point the camera looks at, far along the actual look ray.
    pub aim: Vec3,
}

pub fn third_person_rig(player_pos: Vec3, yaw: f32, pitch: f32) -> ThirdPersonRig {
    let look = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
    let pivot = player_pos + Vec3::Y * config::PLAYER_EYE_HEIGHT;
    let boom_pitch = (pitch - THIRD_PERSON_BOOM_TILT).clamp(-1.54, 1.54);
    let boom = Quat::from_euler(EulerRot::YXZ, yaw, boom_pitch, 0.0);
    ThirdPersonRig {
        pivot,
        shoulder: look * (Vec3::X * THIRD_PERSON_SHOULDER),
        boom_back: boom * Vec3::Z,
        aim: pivot + look * (-Vec3::Z * THIRD_PERSON_AIM_DIST),
    }
}

/// Unoccluded third-person transform. The camera drivers write this every
/// frame; where a physics world exists (host, editor playtest)
/// [`occlude_third_person_camera`] then pulls the camera in front of any
/// geometry the boom crosses. A remote client has no collision world, so it
/// keeps this raw transform.
pub fn third_person_transform(player_pos: Vec3, yaw: f32, pitch: f32) -> Transform {
    let rig = third_person_rig(player_pos, yaw, pitch);
    let pos = rig.pivot + rig.shoulder + rig.boom_back * THIRD_PERSON_DISTANCE;
    Transform::from_translation(pos).looking_at(rig.aim, Vec3::Y)
}

/// Wall avoidance for the third-person boom: two sphere-cast stages —
/// pivot → shoulder, then shoulder → camera — each stopping where the probe
/// first touches geometry, so the camera can never end up on the far side of
/// a wall (including when the *sideways* shoulder offset would poke through).
/// Runs after both camera drivers and overwrites their raw transform.
fn occlude_third_person_camera(
    mode: Res<ThirdPersonMode>,
    look: Res<crate::netplay::LookAngles>,
    spatial: SpatialQuery,
    player: Query<
        (Entity, &Transform),
        (With<crate::netplay::OwnPlayer>, Without<FlyCamera>),
    >,
    sensors: Query<Entity, With<Sensor>>,
    mut camera: Query<&mut Transform, With<FlyCamera>>,
) {
    if !mode.0 {
        return;
    }
    let Ok((player_entity, player_tf)) = player.single() else {
        return;
    };
    let Ok(mut cam) = camera.single_mut() else {
        return;
    };

    let rig = third_person_rig(player_tf.translation, look.yaw, look.pitch);
    // Liquid volumes etc. are sensors — avian spatial queries DO hit them, and
    // the camera must not treat water surfaces as walls.
    let mut excluded: Vec<Entity> = sensors.iter().collect();
    excluded.push(player_entity);
    let filter = SpatialQueryFilter::from_excluded_entities(excluded);
    let probe = Collider::sphere(THIRD_PERSON_PROBE_RADIUS);

    let shoulder_point = probe_stage(&spatial, &probe, rig.pivot, rig.shoulder, &filter);
    let cam_pos = probe_stage(
        &spatial,
        &probe,
        shoulder_point,
        rig.boom_back * THIRD_PERSON_DISTANCE,
        &filter,
    );
    *cam = Transform::from_translation(cam_pos).looking_at(rig.aim, Vec3::Y);
}

/// Sphere-cast from `from` along `offset`; returns the farthest reachable
/// point (probe center) along that segment.
fn probe_stage(
    spatial: &SpatialQuery,
    probe: &Collider,
    from: Vec3,
    offset: Vec3,
    filter: &SpatialQueryFilter,
) -> Vec3 {
    let len = offset.length();
    let Ok(dir) = Dir3::new(offset) else {
        return from;
    };
    let config = ShapeCastConfig {
        // Brushing geometry at the segment start (eye against a low ceiling)
        // must not pin the camera to the pivot.
        ignore_origin_penetration: true,
        ..ShapeCastConfig::from_max_distance(len)
    };
    match spatial.cast_shape(probe, from, Quat::IDENTITY, dir, &config, filter) {
        Some(hit) => from + dir * hit.distance.min(len),
        None => from + offset,
    }
}

/// Crosshair ray reach; with no hit the aim point sits this far out (shots
/// then simply fly along the camera ray).
const AIM_RAY_DIST: f32 = 120.0;

/// Cast the camera ray through screen center and store the first hit as the
/// aim point. Excludes the own player (the third-person camera looks past our
/// own capsule) and sensors (water volumes are not aim targets).
fn update_crosshair_aim(
    spatial: SpatialQuery,
    player: Query<Entity, With<crate::netplay::OwnPlayer>>,
    sensors: Query<Entity, With<Sensor>>,
    camera: Query<&Transform, With<FlyCamera>>,
    mut aim: ResMut<CrosshairAim>,
) {
    let Ok(cam) = camera.single() else {
        return;
    };
    let origin = cam.translation;
    let dir = cam.forward();
    let mut excluded: Vec<Entity> = sensors.iter().collect();
    excluded.extend(player.iter());
    let filter = SpatialQueryFilter::from_excluded_entities(excluded);
    aim.0 = match spatial.cast_ray(origin, dir, AIM_RAY_DIST, true, &filter) {
        Some(hit) => origin + *dir * hit.distance,
        None => origin + *dir * AIM_RAY_DIST,
    };
}

/// Same aim point without a physics world: far point on the camera ray.
fn update_crosshair_aim_fallback(
    camera: Query<&Transform, With<FlyCamera>>,
    mut aim: ResMut<CrosshairAim>,
) {
    if let Ok(cam) = camera.single() {
        aim.0 = cam.translation + *cam.forward() * AIM_RAY_DIST;
    }
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
    capture: Res<crate::netplay::InputCapture>,
    mut window: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if mouse.just_pressed(MouseButton::Left) && !cursor_grabbed(&window) {
        window.grab_mode = CursorGrabMode::Locked;
        window.visible = false;
    }
    // While a UI window owns input, Esc closes that window (dialogue.rs also
    // clears the press) instead of releasing the cursor.
    if keys.just_pressed(KeyCode::Escape) && !capture.0 {
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
