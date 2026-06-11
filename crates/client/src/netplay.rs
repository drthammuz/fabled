//! Client-side networking presentation: input sending, own-player
//! first-person camera, interpolation of remote entities, and name tags.
//! No gameplay logic lives here — the server owns all state.

use std::collections::VecDeque;

use bevy::ecs::schedule::common_conditions::any_with_component;
use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_replicon::prelude::*;
use shared::config;
use shared::protocol::{NetTransform, Player, PlayerInput, PlayerName, YouAre};

use crate::fly_camera::FlyCamera;

pub struct NetPlayPlugin;

impl Plugin for NetPlayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LookAngles>()
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
                    (look_input, send_input, drive_first_person_camera)
                        .chain()
                        .run_if(any_with_component::<OwnPlayer>),
                    attach_remote_player_visuals,
                    position_name_tags,
                    cleanup_name_tags,
                ),
            );
    }
}

/// Marker for the player entity this client controls.
#[derive(Component)]
pub struct OwnPlayer;

/// Local look state. Yaw is sent to the server (movement orientation);
/// pitch stays client-side, purely visual for now.
#[derive(Resource, Default)]
pub struct LookAngles {
    pub yaw: f32,
    pub pitch: f32,
}

/// Snapshot history of a replicated entity: (arrival time, pos, rot).
#[derive(Component, Default)]
struct InterpBuffer(VecDeque<(f64, Vec3, Quat)>);

/// UI text node following a player entity.
#[derive(Component)]
struct NameTag(Entity);

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
        grab: mouse.pressed(MouseButton::Left),
        throw_action: mouse.just_pressed(MouseButton::Right),
    });
}

fn drive_first_person_camera(
    look: Res<LookAngles>,
    player: Single<&Transform, (With<OwnPlayer>, Without<FlyCamera>)>,
    mut camera: Single<&mut Transform, With<FlyCamera>>,
) {
    camera.translation = player.translation + Vec3::Y * config::PLAYER_EYE_HEIGHT;
    camera.rotation = Quat::from_euler(EulerRot::YXZ, look.yaw, look.pitch, 0.0);
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
    mut query: Query<(&InterpBuffer, &mut Transform)>,
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

fn attach_remote_player_visuals(
    mut commands: Commands,
    players: Query<(Entity, &PlayerName), (Added<PlayerName>, With<Player>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, name) in &players {
        commands.entity(entity).insert((
            Mesh3d(meshes.add(Capsule3d::new(
                config::PLAYER_CAPSULE_RADIUS,
                config::PLAYER_CAPSULE_LENGTH,
            ))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.85, 0.35, 0.2),
                perceptual_roughness: 0.7,
                ..default()
            })),
        ));
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
