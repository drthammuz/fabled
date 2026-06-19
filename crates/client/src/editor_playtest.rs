//! In-process editor ↔ playtest toggle (player physics, no process relaunch).

use bevy::camera::Exposure;
use bevy::ecs::schedule::common_conditions::resource_exists;
use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use shared::kenney_pit;
use shared::map_pool;
use shared::{EditorMode, KenneyPlaytestGeneration, TestMapStyle, TestMode};

use crate::editor_selection::EditorPlaced;
use crate::editor_state::floor_y;
use crate::kenney_editor::EditorModuleReady;
use crate::test_showcase::{apply_room_shell_mesh_cutouts, KenneyModule};
use crate::editor_workspace::{EditorMenuRoot, EditorSidebarRoot, FloorSlab};
use crate::fly_camera::FlyCamera;
use crate::netplay::{LookAngles, OwnPlayer};

/// Active while walking the layout in-process (G from editor).
#[derive(Resource, Clone, Copy)]
pub struct EditorPlaytestActive;

#[derive(Component)]
pub struct EditorPlaytestCamera;

#[derive(Component)]
pub struct PlaytestCoordsHud;

pub struct EditorPlaytestPlugin;

impl Plugin for EditorPlaytestPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                playtest_cursor_lock,
                editor_playtest_look,
                editor_playtest_camera,
                sync_playtest_player_visibility,
                sync_playtest_patched_pieces,
                sync_playtest_mesh_cutouts,
                update_playtest_coords_hud,
            )
                .chain()
                .run_if(resource_exists::<EditorPlaytestActive>),
        )
        .add_systems(
            PostStartup,
            spawn_standalone_test_coords_hud.run_if(standalone_kenney_test),
        )
        .add_systems(
            Update,
            update_playtest_coords_hud.run_if(standalone_kenney_test),
        );
    }
}

fn standalone_kenney_test(
    test: Option<Res<TestMode>>,
    editor: Option<Res<EditorMode>>,
    playtest: Option<Res<EditorPlaytestActive>>,
) -> bool {
    playtest.is_none()
        && editor.is_none()
        && test.as_ref().is_some_and(|t| t.style == TestMapStyle::Kenney)
}

fn spawn_playtest_coords_hud(commands: &mut Commands) -> Entity {
    commands
        .spawn((
            PlaytestCoordsHud,
            Text::new(""),
            TextFont {
                font_size: 15.0,
                ..default()
            },
            TextColor(Color::srgb(0.85, 1.0, 0.85)),
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(10.0),
                left: Val::Px(12.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        ))
        .id()
}

fn spawn_standalone_test_coords_hud(mut commands: Commands) {
    spawn_playtest_coords_hud(&mut commands);
}

fn update_playtest_coords_hud(
    player: Query<&Transform, With<OwnPlayer>>,
    mut hud: Query<&mut Text, With<PlaytestCoordsHud>>,
    editor: Option<Res<EditorMode>>,
) {
    let Ok(tf) = player.single() else {
        return;
    };
    let Ok(mut text) = hud.single_mut() else {
        return;
    };
    let layout = map_pool::play_layout(editor.is_some());
    let spawn_line = layout
        .spawn_xz
        .map(|[x, z]| format!("spawn marker: ({x:.1}, {z:.1})"))
        .unwrap_or_else(|| "spawn marker: (not set)".to_string());
    let p = tf.translation;
    let next = format!(
        "position: ({:.1}, {:.1}, {:.1})\n{spawn_line}\nmap centre: (0.0, 0.0)",
        p.x, p.y, p.z
    );
    if text.0 != next {
        text.0 = next;
    }
}

fn sync_playtest_patched_pieces(
    mut commands: Commands,
    generation: Res<KenneyPlaytestGeneration>,
    mut last_gen: Local<u32>,
    mut placed: Query<(
        Entity,
        &KenneyModule,
        &GlobalTransform,
        &mut Transform,
        &mut EditorPlaced,
        Option<&EditorModuleReady>,
    )>,
) {
    if *last_gen == generation.0 {
        return;
    }
    *last_gen = generation.0;

    let patched = map_pool::play_layout(true);
    let extraction = patched.extraction_xz;

    for (entity, module, gt, ..) in &placed {
        if kenney_pit::hide_extraction_hatch_piece(
            module.name,
            module.floor,
            gt.translation().x,
            gt.translation().z,
            patched.floors.get(&module.floor),
        ) {
            commands.entity(entity).despawn();
        }
    }

    for (entity, module, _, _, placed, ..) in &placed {
        if kenney_pit::is_room_shell(module.name)
            && matches!(placed.floor_level, 0 | -1 | -2)
            && extraction.is_some()
        {
            commands.entity(entity).remove::<EditorModuleReady>();
        }
    }

    for piece in &patched.pieces {
        if piece.stem != "stairs" {
            continue;
        }
        for (_, module, _, mut tf, mut ep, ..) in &mut placed {
            if module.name != "stairs" {
                continue;
            }
            ep.floor_level = piece.floor;
            tf.translation = Vec3::new(piece.x, floor_y(piece.floor), piece.z);
            tf.rotation = Quat::from_rotation_y(piece.yaw);
        }
    }
}

fn sync_playtest_mesh_cutouts(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    placed: Query<
        (
            Entity,
            &KenneyModule,
            &GlobalTransform,
            &EditorPlaced,
        ),
        Without<EditorModuleReady>,
    >,
    children_q: Query<&Children>,
    mesh_q: Query<(&Mesh3d, &GlobalTransform)>,
) {
    let layout = map_pool::play_layout(true);
    let Some([ex, ez]) = layout.extraction_xz else {
        return;
    };
    let extraction = Some(Vec2::new(ex, ez));

    for (entity, module, gt, placed) in &placed {
        if !kenney_pit::is_room_shell(module.name) {
            continue;
        }
        if !matches!(placed.floor_level, 0 | -1 | -2) {
            continue;
        }
        if apply_room_shell_mesh_cutouts(
            &mut commands,
            entity,
            module.name,
            placed.floor_level,
            gt,
            extraction,
            layout.floors.get(&placed.floor_level),
            &mut meshes,
            &children_q,
            &mesh_q,
        ) {
            commands.entity(entity).insert(EditorModuleReady);
        }
    }
}

pub fn enter_in_process_playtest(
    mut commands: Commands,
    editor_cam: Query<Entity, Or<(With<crate::kenney_editor::EditorCamera>, With<EditorPlaytestCamera>)>>,
    menu: Query<Entity, With<EditorMenuRoot>>,
    sidebar: Query<Entity, With<EditorSidebarRoot>>,
    ghosts: Query<Entity, With<crate::kenney_editor::EditorGhost>>,
    toast: Query<Entity, With<crate::kenney_editor::SaveToastText>>,
    floors: Query<Entity, With<FloorSlab>>,
    mut test_mode: ResMut<TestMode>,
    mut generation: ResMut<KenneyPlaytestGeneration>,
    mut window: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    commands.insert_resource(EditorPlaytestActive);
    test_mode.style = TestMapStyle::Kenney;
    shared::level::set_test_map_style(TestMapStyle::Kenney);
    generation.0 = generation.0.wrapping_add(1);

    for e in editor_cam
        .iter()
        .chain(menu.iter())
        .chain(sidebar.iter())
        .chain(ghosts.iter())
        .chain(toast.iter())
    {
        commands.entity(e).despawn();
    }
    for e in &floors {
        commands.entity(e).despawn();
    }

    let layout = map_pool::play_layout(true);
    let look = layout
        .spawn_xz
        .map(|[sx, sz]| Vec3::new(sx, 0.0, sz))
        .unwrap_or_else(|| {
            let focus = layout.focus_xz();
            Vec3::new(focus.x, 0.0, focus.y)
        });
    let cam_pos = look + Vec3::new(0.0, shared::config::PLAYER_EYE_HEIGHT, 0.0);
    commands.spawn((
        Camera3d::default(),
        Exposure { ev100: 9.0 },
        EditorPlaytestCamera,
        FlyCamera {
            yaw: std::f32::consts::PI,
            pitch: 0.0,
        },
        Transform::from_translation(cam_pos).looking_at(look + Vec3::Y, Vec3::Y),
    ));

    spawn_playtest_coords_hud(&mut commands);

    window.grab_mode = CursorGrabMode::Locked;
    window.visible = false;

    info!("in-process playtest — G return · WASD move · R respawn · mouse look");
}

pub fn exit_in_process_playtest(
    commands: &mut Commands,
    mut test_mode: ResMut<TestMode>,
    mut generation: ResMut<KenneyPlaytestGeneration>,
    mut window: Single<&mut CursorOptions, With<PrimaryWindow>>,
    mut player_vis: Query<&mut Visibility, With<OwnPlayer>>,
    playtest_cam: Query<Entity, With<EditorPlaytestCamera>>,
    coords_hud: Query<Entity, With<PlaytestCoordsHud>>,
) {
    commands.remove_resource::<EditorPlaytestActive>();
    test_mode.style = TestMapStyle::Rusty;
    shared::level::set_test_map_style(TestMapStyle::Rusty);
    generation.0 = generation.0.wrapping_add(1);

    for e in playtest_cam.iter().chain(coords_hud.iter()) {
        commands.entity(e).despawn();
    }

    window.grab_mode = CursorGrabMode::None;
    window.visible = true;
    for mut vis in &mut player_vis {
        *vis = Visibility::Hidden;
    }
}

/// Re-locks the cursor in playtest if it was released (Alt-Tab, OS focus change, etc.).
/// Left-click re-grabs; once grabbed, keeps it locked every frame.
fn playtest_cursor_lock(
    mouse: Res<ButtonInput<MouseButton>>,
    mut window: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if window.grab_mode != CursorGrabMode::None {
        // Already locked — keep it that way.
        window.grab_mode = CursorGrabMode::Locked;
        window.visible = false;
    } else if mouse.just_pressed(MouseButton::Left) {
        window.grab_mode = CursorGrabMode::Locked;
        window.visible = false;
    }
}

fn editor_playtest_look(
    mut motion: MessageReader<MouseMotion>,
    window: Single<&CursorOptions, With<PrimaryWindow>>,
    mut look: ResMut<LookAngles>,
) {
    if window.grab_mode == CursorGrabMode::None {
        motion.clear();
        return;
    }
    for ev in motion.read() {
        look.yaw -= ev.delta.x * shared::config::LOOK_SENSITIVITY;
        look.pitch =
            (look.pitch - ev.delta.y * shared::config::LOOK_SENSITIVITY).clamp(-1.54, 1.54);
    }
}

fn editor_playtest_camera(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    look: Res<LookAngles>,
    mut eye_height: Local<f32>,
    player: Query<&Transform, (With<OwnPlayer>, Without<EditorPlaytestCamera>)>,
    mut camera: Query<&mut Transform, (With<EditorPlaytestCamera>, Without<OwnPlayer>)>,
) {
    let Ok(mut cam) = camera.single_mut() else {
        return;
    };
    let Ok(player) = player.single() else {
        cam.rotation = Quat::from_euler(EulerRot::YXZ, look.yaw, look.pitch, 0.0);
        return;
    };
    let crouching =
        keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let target = if crouching {
        shared::config::PLAYER_CROUCH_EYE_HEIGHT
    } else {
        shared::config::PLAYER_EYE_HEIGHT
    };
    let t = 1.0 - f32::exp(-12.0 * time.delta_secs());
    *eye_height += (target - *eye_height) * t;
    cam.translation = player.translation + Vec3::Y * *eye_height;
    cam.rotation = Quat::from_euler(EulerRot::YXZ, look.yaw, look.pitch, 0.0);
}

fn sync_playtest_player_visibility(
    mut q: Query<&mut Visibility, With<OwnPlayer>>,
) {
    for mut vis in &mut q {
        *vis = Visibility::Inherited;
    }
}
