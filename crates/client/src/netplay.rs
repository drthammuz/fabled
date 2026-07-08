//! Client-side networking presentation: input sending, own-player
//! first-person camera, interpolation of remote entities, and name tags.
//! No gameplay logic lives here — the server owns all state.

use std::collections::VecDeque;

use bevy::ecs::schedule::common_conditions::{any_with_component, not, resource_exists};
use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_replicon::prelude::*;
use shared::classes::ClassKind;
use shared::config;
use shared::protocol::{NetTransform, Player, PlayerClass, PlayerInput, PlayerName, YouAre};

use crate::class_select::SelectState;
use crate::editor_playtest::EditorPlaytestActive;
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
            .init_resource::<InputCapture>()
            .init_resource::<PendingTrade>()
            .init_resource::<TerminalHackPending>()
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
                    (look_input, send_input)
                        .chain()
                        .run_if(
                            any_with_component::<OwnPlayer>
                                .and(in_state(SelectState::Playing)),
                        ),
                    drive_first_person_camera
                        .run_if(
                            any_with_component::<OwnPlayer>
                                .and(in_state(SelectState::Playing))
                                .and(not(resource_exists::<EditorPlaytestActive>)),
                        ),
                    attach_remote_player_visuals,
                    update_class_model,
                    tag_own_player_model,
                    drive_own_player_model,
                    // Remote client ONLY. On the host this entity is an
                    // avian-interpolated kinematic body: writing its Transform
                    // from Update marks it changed with the *interpolated*
                    // (lagging) translation, which the physics sync copies back
                    // into Position every frame — rewinding the body and
                    // reducing walk speed to a crawl (and with it the per-frame
                    // position delta that drives the walk animation). The
                    // server's `rotate_to_yaw` already rotates the host player.
                    rotate_own_player_third_person
                        .run_if(in_state(ClientState::Connected)),
                    position_name_tags.run_if(any_with_component::<FlyCamera>),
                    cleanup_name_tags,
                ),
            );
    }
}

/// Marker for the player entity this client controls.
#[derive(Component)]
pub struct OwnPlayer;

/// While true, a UI window (NPC dialogue/trade — later: terminals) owns the
/// keyboard and mouse: look input freezes and `send_input` sends a neutral
/// input (no movement, no actions) so keys pressed in the UI can't fire guns
/// or move the player. Set by `dialogue.rs` and `terminal.rs`.
#[derive(Resource, Default)]
pub struct InputCapture(pub bool);

/// Set by the terminal when the Tech runs `hack`; drained by `send_input` into
/// `PlayerInput::terminal_hack` (fires even while the terminal holds capture).
#[derive(Resource, Default)]
pub struct TerminalHackPending(pub bool);

/// Trade actions queued by the dialogue UI; drained into the next
/// `PlayerInput` message.
#[derive(Resource, Default)]
pub struct PendingTrade {
    pub buy: Option<u8>,
    pub sell: Option<u8>,
}

/// Smoothly lerped eye height for crouch visual transition.
#[derive(Resource)]
pub(crate) struct SmoothEyeHeight(f32);

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
    capture: Res<InputCapture>,
    mut look: ResMut<LookAngles>,
) {
    if window.grab_mode == CursorGrabMode::None || capture.0 {
        motion.clear();
        return;
    }
    for ev in motion.read() {
        look.yaw -= ev.delta.x * config::LOOK_SENSITIVITY;
        look.pitch = (look.pitch - ev.delta.y * config::LOOK_SENSITIVITY).clamp(-1.54, 1.54);
    }
}

pub(crate) fn send_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    look: Res<LookAngles>,
    aim: Res<crate::fly_camera::CrosshairAim>,
    inventory: Res<crate::hotbar::OwnInventory>,
    capture: Res<InputCapture>,
    mut trade: ResMut<PendingTrade>,
    mut hack: ResMut<TerminalHackPending>,
    mut writer: MessageWriter<PlayerInput>,
) {
    // Drain the terminal hack request (fires even while a UI holds capture).
    let terminal_hack = std::mem::take(&mut hack.0);
    if capture.0 {
        // A UI window owns the keyboard: keep the server fed (yaw/pitch/slot
        // stay current) but send no movement or actions, only queued trades.
        writer.write(PlayerInput {
            yaw: look.yaw,
            pitch: look.pitch,
            aim: aim.0,
            selected_slot: inventory.selected as u8,
            trade_buy: trade.buy.take(),
            trade_sell: trade.sell.take(),
            terminal_hack,
            ..default()
        });
        return;
    }
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
        // Left click fires (pistol) / swings (bat), per the selected hotbar slot.
        attack: mouse.just_pressed(MouseButton::Left),
        aim: aim.0,
        selected_slot: inventory.selected as u8,
        flashlight_toggle: keys.just_pressed(KeyCode::KeyF),
        trade_buy: None,
        trade_sell: None,
        terminal_hack,
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

pub(crate) fn drive_first_person_camera(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    look: Res<LookAngles>,
    third_person: Res<crate::fly_camera::ThirdPersonMode>,
    mut eye_height: ResMut<SmoothEyeHeight>,
    player: Query<&Transform, (With<OwnPlayer>, Without<FlyCamera>)>,
    mut camera: Query<&mut Transform, With<FlyCamera>>,
) {
    let Ok(player) = player.single() else {
        return;
    };
    let Ok(mut cam) = camera.single_mut() else {
        return;
    };
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
        *cam = crate::fly_camera::third_person_transform(
            player.translation,
            look.yaw,
            look.pitch,
        );
    } else {
        cam.translation = player.translation + Vec3::Y * eye_height.0;
        cam.rotation = Quat::from_euler(EulerRot::YXZ, look.yaw, look.pitch, 0.0);
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

/// Marker on the own player's character-model scene root. Hidden in first
/// person, shown in third person (`drive_own_player_model`).
#[derive(Component)]
pub struct OwnPlayerModel;

/// Re-spawn the character model when a player's class changes (e.g., after
/// class selection arrives from the server after `PlayerName` was already added).
/// The own player gets a model too — tagged `OwnPlayerModel` and hidden until
/// third-person mode is toggled on.
fn update_class_model(
    mut commands: Commands,
    changed: Query<(Entity, &PlayerClass, Option<&Children>, Has<OwnPlayer>), Changed<PlayerClass>>,
    scenes: Res<CharacterScenes>,
) {
    for (entity, class, children, is_own) in &changed {
        // Despawn old model child(ren).
        if let Some(children) = children {
            for child in children.iter() {
                commands.entity(child).despawn();
            }
        }
        if let Some(scene_handle) = class_scene(&scenes, class.0) {
            let mut child = commands.spawn((
                SceneRoot(scene_handle),
                Transform {
                    translation: Vec3::new(0.0, CHAR_OFFSET_Y, 0.0),
                    rotation: Quat::from_rotation_y(std::f32::consts::PI),
                    scale: Vec3::splat(CHAR_SCALE),
                },
                crate::character_animation::PlayerSceneLink(entity),
            ));
            if is_own {
                // Explicit Hidden so the model can't flash across the camera
                // for a frame before drive_own_player_model first runs.
                child.insert((OwnPlayerModel, Visibility::Hidden));
            }
            let child = child.id();
            commands.entity(entity).add_child(child);
        }
    }
}

/// Tags model children that were attached before `YouAre` / `OwnPlayer`
/// arrived, so they pick up first/third-person visibility control.
fn tag_own_player_model(
    mut commands: Commands,
    new_own: Query<&Children, Added<OwnPlayer>>,
    models: Query<(), With<crate::character_animation::PlayerSceneLink>>,
) {
    for children in &new_own {
        for child in children.iter() {
            if models.contains(child) {
                commands.entity(child).insert((OwnPlayerModel, Visibility::Hidden));
            }
        }
    }
}

/// Shows the own-player model only in third person, and keeps its feet on the
/// floor while crouched (the capsule centre drops to crouch half-height, but
/// the model's Y offset is authored for the standing capsule).
fn drive_own_player_model(
    third_person: Res<crate::fly_camera::ThirdPersonMode>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut models: Query<(&mut Visibility, &mut Transform), With<OwnPlayerModel>>,
) {
    let crouching =
        keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    // Capsule centre sits (length/2 + radius) above the feet.
    let target_y = if crouching {
        -(config::PLAYER_CROUCH_LENGTH * 0.5 + config::PLAYER_CAPSULE_RADIUS)
    } else {
        CHAR_OFFSET_Y
    };
    let t = 1.0 - f32::exp(-12.0 * time.delta_secs());
    for (mut vis, mut transform) in &mut models {
        *vis = if third_person.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        transform.translation.y += (target_y - transform.translation.y) * t;
    }
}

/// In third person the model must face the look yaw. Runs on remote clients
/// only (nothing else updates the own entity's rotation there — interpolation
/// excludes OwnPlayer). On the host the server's `rotate_to_yaw` handles it,
/// and writing Transform here would fight physics interpolation (see plugin).
fn rotate_own_player_third_person(
    third_person: Res<crate::fly_camera::ThirdPersonMode>,
    look: Res<LookAngles>,
    mut own: Query<&mut Transform, With<OwnPlayer>>,
) {
    if !third_person.0 {
        return;
    }
    for mut transform in &mut own {
        transform.rotation = Quat::from_rotation_y(look.yaw);
    }
}

/// Scale factor to fit the Quaternius cyberpunk Character (~1.40 units tall,
/// feet at y≈0) into the game's 1.8 m player capsule, plus a Y offset so feet
/// sit at the capsule base (entity Transform = capsule centre = 0.9 m above feet).
const CHAR_SCALE: f32 = 1.28;     // 1.8 / 1.40
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
    camera: Query<(&Camera, &GlobalTransform), With<FlyCamera>>,
    players: Query<&GlobalTransform, With<Player>>,
    mut tags: Query<(&NameTag, &mut Node, &mut Visibility)>,
) {
    let Ok((camera, cam_transform)) = camera.single() else {
        return;
    };
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
