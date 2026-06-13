//! Client-side networking presentation: input sending, own-player
//! first-person camera, interpolation of remote entities, and name tags.
//! No gameplay logic lives here — the server owns all state.

use std::collections::VecDeque;

use bevy::ecs::schedule::common_conditions::any_with_component;
use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_replicon::prelude::*;
use shared::classes::ClassKind;
use shared::config;
use shared::protocol::{NetTransform, Player, PlayerClass, PlayerInput, PlayerName, YouAre};

use crate::class_select::SelectState;
use crate::fly_camera::FlyCamera;

/// Preloaded character GLTF scenes keyed by class index.
#[derive(Resource, Default)]
pub struct CharacterScenes {
    pub scenes: [Option<Handle<Scene>>; 4],
}

pub struct NetPlayPlugin;

impl Plugin for NetPlayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LookAngles>()
            .init_resource::<CharacterScenes>()
            .init_resource::<SmoothEyeHeight>()
            .add_systems(Startup, preload_character_scenes)
            .add_observer(on_you_are)
            .add_systems(
                PreUpdate,
                buffer_snapshots
                    .after(ClientSystems::Receive)
                    .run_if(in_state(ClientState::Connected)),
            )
            .add_systems(
                Update,
                (
                    interpolate_remote_entities.run_if(in_state(ClientState::Connected)),
                    // Remote client only: push server NetTransform → Transform so the
                    // camera follows the replicated position. Gated on Connected so it
                    // doesn't fight physics interpolation on the listen server host
                    // (where ClientState is Disconnected and physics owns the transform).
                    sync_own_player_transform.run_if(in_state(ClientState::Connected)),
                    (look_input, send_input, drive_first_person_camera)
                        .chain()
                        .run_if(
                            any_with_component::<OwnPlayer>
                                .and(in_state(SelectState::Playing)),
                        ),
                    attach_remote_player_visuals,
                    update_class_model,
                    remove_own_player_model,
                    position_name_tags,
                    cleanup_name_tags,
                ),
            );
    }
}

/// Marker for the player entity this client controls.
#[derive(Component)]
pub struct OwnPlayer;

/// Smoothly lerped eye height for crouch visual transition.
#[derive(Resource)]
struct SmoothEyeHeight(f32);

impl Default for SmoothEyeHeight {
    fn default() -> Self {
        Self(config::PLAYER_EYE_HEIGHT)
    }
}

/// Local look state. Yaw is sent to the server (movement orientation);
/// pitch stays client-side, purely visual for now.
#[derive(Resource)]
pub struct LookAngles {
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for LookAngles {
    fn default() -> Self {
        // Level runs toward +Z; yaw=PI faces that direction.
        Self { yaw: std::f32::consts::PI, pitch: 0.0 }
    }
}

/// Snapshot history of a replicated entity: (arrival time, pos, rot).
#[derive(Component, Default)]
struct InterpBuffer(VecDeque<(f64, Vec3, Quat)>);

/// UI text node following a player entity.
#[derive(Component)]
struct NameTag(Entity);

fn preload_character_scenes(
    asset_server: Res<AssetServer>,
    mut scenes: ResMut<CharacterScenes>,
) {
    use shared::classes::ALL_CLASSES;
    for (i, def) in ALL_CLASSES.iter().enumerate() {
        let path = GltfAssetLabel::Scene(0).from_asset(def.model_path);
        scenes.scenes[i] = Some(asset_server.load(path));
    }
}

fn class_scene(scenes: &CharacterScenes, kind: ClassKind) -> Option<Handle<Scene>> {
    use shared::classes::ClassKind::*;
    let idx = match kind { Soldier => 0, Medic => 1, Scout => 2, Tech => 3 };
    scenes.scenes[idx].clone()
}

fn on_you_are(you: On<YouAre>, mut commands: Commands) {
    info!("server assigned player entity {}", you.player);
    commands
        .entity(you.player)
        .insert((OwnPlayer, Visibility::Hidden));
}

// --- Input ---

fn look_input(
    mut motion: MessageReader<MouseMotion>,
    window: Single<&CursorOptions, With<PrimaryWindow>>,
    mut look: ResMut<LookAngles>,
) {
    if window.grab_mode == CursorGrabMode::None {
        motion.clear();
        return;
    }
    for ev in motion.read() {
        look.yaw -= ev.delta.x * config::LOOK_SENSITIVITY;
        look.pitch = (look.pitch - ev.delta.y * config::LOOK_SENSITIVITY).clamp(-1.54, 1.54);
    }
}

fn send_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    look: Res<LookAngles>,
    inventory: Res<crate::hotbar::OwnInventory>,
    mut writer: MessageWriter<PlayerInput>,
) {
    let mut move_dir = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        move_dir.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        move_dir.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        move_dir.x += 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        move_dir.x -= 1.0;
    }
    writer.write(PlayerInput {
        move_dir: move_dir.normalize_or_zero(),
        yaw: look.yaw,
        pitch: look.pitch,
        jump: keys.just_pressed(KeyCode::Space),
        sprint: keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight),
        crouch: keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight),
        grab: mouse.pressed(MouseButton::Left),
        throw_action: mouse.just_pressed(MouseButton::Right),
        interact: keys.just_pressed(KeyCode::KeyE),
        drop_slot: keys
            .just_pressed(KeyCode::KeyQ)
            .then_some(inventory.selected as u8),
        shop_buy: shop_buy_key(&keys),
        route_select: route_select_key(&keys),
        attack: keys.just_pressed(KeyCode::KeyV),
        flashlight_toggle: keys.just_pressed(KeyCode::KeyF),
    });
}

fn shop_buy_key(keys: &ButtonInput<KeyCode>) -> Option<u32> {
    if keys.just_pressed(KeyCode::Digit1) {
        return Some(10);
    }
    if keys.just_pressed(KeyCode::Digit2) {
        return Some(12);
    }
    if keys.just_pressed(KeyCode::Digit3) {
        return Some(11);
    }
    None
}

fn route_select_key(keys: &ButtonInput<KeyCode>) -> Option<u8> {
    if keys.just_pressed(KeyCode::Digit7) {
        return Some(0);
    }
    if keys.just_pressed(KeyCode::Digit8) {
        return Some(1);
    }
    if keys.just_pressed(KeyCode::Digit9) {
        return Some(2);
    }
    None
}

fn drive_first_person_camera(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    look: Res<LookAngles>,
    third_person: Res<crate::fly_camera::ThirdPersonMode>,
    mut eye_height: ResMut<SmoothEyeHeight>,
    player: Single<&Transform, (With<OwnPlayer>, Without<FlyCamera>)>,
    mut camera: Single<&mut Transform, With<FlyCamera>>,
) {
    let crouching =
        keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let target = if crouching {
        config::PLAYER_CROUCH_EYE_HEIGHT
    } else {
        config::PLAYER_EYE_HEIGHT
    };
    // ~12 Hz effective speed: fully transitions in ~0.08 s
    let t = 1.0 - f32::exp(-12.0 * time.delta_secs());
    eye_height.0 = eye_height.0 + (target - eye_height.0) * t;

    if third_person.0 {
        // Position camera behind and above the player, looking at head height.
        let behind = Quat::from_rotation_y(look.yaw) * Vec3::new(0.0, 0.0, 3.5);
        camera.translation = player.translation + Vec3::Y * 1.8 + behind;
        camera.look_to(-behind.normalize(), Vec3::Y);
    } else {
        camera.translation = player.translation + Vec3::Y * eye_height.0;
        camera.rotation = Quat::from_euler(EulerRot::YXZ, look.yaw, look.pitch, 0.0);
    }
}

// --- Interpolation of replicated entities ---

fn buffer_snapshots(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut query: Query<
        (Entity, &NetTransform, Option<&mut InterpBuffer>),
        Changed<NetTransform>,
    >,
) {
    let now = time.elapsed_secs_f64();
    for (entity, net, buffer) in &mut query {
        match buffer {
            Some(mut buffer) => {
                buffer.0.push_back((now, net.translation, net.rotation));
                while buffer.0.len() > 2
                    && buffer.0[1].0 < now - config::INTERP_DELAY
                {
                    buffer.0.pop_front();
                }
            }
            None => {
                // First sight of this entity: place it immediately.
                let mut buf = InterpBuffer::default();
                buf.0.push_back((now, net.translation, net.rotation));
                commands.entity(entity).insert((
                    buf,
                    Transform::from_translation(net.translation)
                        .with_rotation(net.rotation),
                ));
            }
        }
    }
}

fn interpolate_remote_entities(
    time: Res<Time<Real>>,
    mut query: Query<(&InterpBuffer, &mut Transform), Without<OwnPlayer>>,
) {
    let render_time = time.elapsed_secs_f64() - config::INTERP_DELAY;
    for (buffer, mut transform) in &mut query {
        let buf = &buffer.0;
        let Some(&(newest_t, newest_pos, newest_rot)) = buf.back() else {
            continue;
        };
        if buf.len() == 1 || render_time >= newest_t {
            transform.translation = newest_pos;
            transform.rotation = newest_rot;
            continue;
        }
        // Find the two snapshots surrounding render_time and blend.
        for pair in buf.iter().collect::<Vec<_>>().windows(2) {
            let (t0, p0, r0) = *pair[0];
            let (t1, p1, r1) = *pair[1];
            if render_time >= t0 && render_time <= t1 {
                let alpha = ((render_time - t0) / (t1 - t0)) as f32;
                transform.translation = p0.lerp(p1, alpha);
                transform.rotation = r0.slerp(r1, alpha);
                break;
            }
        }
    }
}

// --- Remote player visuals ---

/// Smoothly follows the server-authoritative position for the own player on a
/// remote client. Uses exponential lerp at ~20 Hz effective bandwidth so the
/// camera doesn't stutter at the 30 Hz replication rate.
/// Only runs on a remote client (Connected); host physics owns the transform.
fn sync_own_player_transform(
    time: Res<Time>,
    mut own_player: Query<(&NetTransform, &mut Transform), With<OwnPlayer>>,
) {
    let t = 1.0 - f32::exp(-20.0 * time.delta_secs());
    for (net, mut transform) in &mut own_player {
        transform.translation = transform.translation.lerp(net.translation, t);
    }
}

/// Re-spawn the character model when a player's class changes (e.g., after
/// class selection arrives from the server after `PlayerName` was already added).
fn update_class_model(
    mut commands: Commands,
    changed: Query<(Entity, &PlayerClass, Option<&Children>), (Changed<PlayerClass>, Without<OwnPlayer>)>,
    scenes: Res<CharacterScenes>,
) {
    for (entity, class, children) in &changed {
        // Despawn old model child(ren).
        if let Some(children) = children {
            for child in children.iter() {
                commands.entity(child).despawn();
            }
        }
        if let Some(scene_handle) = class_scene(&scenes, class.0) {
            let child = commands.spawn((
                SceneRoot(scene_handle),
                Transform {
                    translation: Vec3::new(0.0, CHAR_OFFSET_Y, 0.0),
                    rotation: Quat::from_rotation_y(std::f32::consts::PI),
                    scale: Vec3::splat(CHAR_SCALE),
                },
                crate::character_animation::PlayerSceneLink(entity),
            )).id();
            commands.entity(entity).add_child(child);
        }
    }
}

/// Despawn any model children from the own player entity — handles the race
/// where the character model is attached before `YouAre` / `OwnPlayer` arrives.
fn remove_own_player_model(
    mut commands: Commands,
    new_own: Query<&Children, Added<OwnPlayer>>,
) {
    for children in &new_own {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }
}

/// Scale factor to fit the KayKit Adventurer (~2.31 units tall, feet at y≈0)
/// into the game's 1.8 m player capsule, plus a Y offset so feet sit at the
/// capsule base (entity Transform = capsule centre = 0.9 m above feet).
const CHAR_SCALE: f32 = 0.78;     // 1.8 / 2.31
const CHAR_OFFSET_Y: f32 = -0.9;  // shift model root down to capsule base

/// Spawn the floating name tag for each new remote player.
///
/// The character *model* is attached solely by `update_class_model` (which
/// fires on `Changed<PlayerClass>`, including the initial replication). Doing
/// it here too would race: both systems run in the same frame and neither sees
/// the other's deferred child, producing two overlapping models.
fn attach_remote_player_visuals(
    mut commands: Commands,
    players: Query<(Entity, &PlayerName), (Added<PlayerName>, With<Player>, Without<OwnPlayer>)>,
) {
    for (entity, name) in &players {
        commands.spawn((
            NameTag(entity),
            Text::new(name.0.clone()),
            TextFont {
                font_size: 16.0,
                ..default()
            },
            TextColor(Color::WHITE),
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
        ));
    }
}


fn position_name_tags(
    camera: Single<(&Camera, &GlobalTransform), With<FlyCamera>>,
    players: Query<&GlobalTransform, With<Player>>,
    mut tags: Query<(&NameTag, &mut Node, &mut Visibility)>,
) {
    let (camera, cam_transform) = *camera;
    for (tag, mut node, mut visibility) in &mut tags {
        let Ok(player) = players.get(tag.0) else {
            continue;
        };
        let head = player.translation()
            + Vec3::Y * (config::PLAYER_CAPSULE_LENGTH / 2.0 + config::PLAYER_CAPSULE_RADIUS + 0.3);
        match camera.world_to_viewport(cam_transform, head) {
            Ok(pos) => {
                node.left = Val::Px(pos.x);
                node.top = Val::Px(pos.y);
                *visibility = Visibility::Visible;
            }
            Err(_) => *visibility = Visibility::Hidden,
        }
    }
}

/// Removes tags whose player despawned, and the tag over our own head
/// (YouAre can arrive after the tag was created).
fn cleanup_name_tags(
    mut commands: Commands,
    tags: Query<(Entity, &NameTag)>,
    players: Query<Has<OwnPlayer>, With<Player>>,
) {
    for (entity, tag) in &tags {
        match players.get(tag.0) {
            Ok(false) => {}
            Ok(true) | Err(_) => commands.entity(entity).despawn(),
        }
    }
}
