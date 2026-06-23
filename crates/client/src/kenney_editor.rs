//! Kenney module layout editor (`--host --editor`).
//!
//! Z menu · Map mode (5×5 modules) · Module mode (5×5 cells) · floor paint · snap modes.

use bevy::asset::RenderAssetUsages;
use bevy::camera::Exposure;
use bevy::ecs::schedule::common_conditions::{not, resource_exists};
use bevy::gltf::GltfLoaderSettings;
use bevy::input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use shared::editor_map::{
    ActiveDocKind, EditorTool, EditorWorkflow, GridSpec, PieceRecord,
};
use shared::editor_settings::UserEditorPrefs;
use crate::door_anim::HiddenEntranceDoor;
use shared::editor_catalog::{
    glb_asset_path, glb_asset_path_in_kit, synth_dressing_stems, synth_dressing_stems_filtered,
    synth_dressing_placement_y, synth_wall_decal_mount_offset, synth_back_anchor_offset,
    synth_wall_face_offset, synth_balcony_outward_offset, is_synth_balcony_stem,
    synth_face_yaw, is_synth_structure_wall, SYNTH_DRESSING_SCALE, SYNTH_KIT,
    SYNTH_DECK_Y,
};
use shared::kenney_catalog::{
    self, placement_for_hover, quantize_yaw, rotated_grid_size, sw_from_placement, KENNEY_CELL,
    KENNEY_MOD_M,
};
use shared::kenney_layout::KenneyLayout;
use shared::kenney_pit;
use shared::EditorMode;
use shared::{KenneyPlaytestGeneration, TestMapStyle, TestMode};

use crate::editor_floor::{
    cell_index_from_world, clone_floor_mask, is_floor_tool, paint_floor_rect, sync_floor_slabs,
};
use crate::editor_history::{
    snapshot_from_entity, snapshot_from_record, undo_redo_input, EditorHistory, FloorPaintSession,
    HistoryOp, PendingHistoryApply,
};
use crate::editor_ops::{apply_pending_history, remove_group_from_document, remove_piece_from_document};
use crate::editor_playtest::{enter_in_process_playtest, exit_in_process_playtest, EditorPlaytestActive};
use crate::editor_selection::{
    draw_selection_gizmo, pick_piece_at, select_drag_input,
    select_tool_input, EditorPlaced, EditorSelection, PieceOwner,
};
use crate::editor_sidebar::{
    gallery_button_input, gallery_controller_input, load_picker_input, naming_modal_input,
    rebuild_sidebar, sidebar_button_input, spawn_load_picker_ui, spawn_naming_modal,
    sync_module_info, sync_sidebar_highlight, sync_sidebar_visibility, GalleryRatings,
    SidebarCache,
};
use crate::editor_state::{current_stem, floor_y, EditorState};
use crate::editor_ui::{
    cancel_floor_tool, close_menus_on_pointer_leave, menu_button_input, spawn_editor_chrome,
    status_line, sync_dropdown_menus, sync_menu_labels, update_ui_hover_block,
};
use crate::editor_workspace::{EditorMenuRoot, EditorSidebarRoot, EditorWorkspace, FloorSlab, SpawnMarker};
use crate::process_spawn::relaunch_fabled;
use crate::test_showcase::{
    cut_kenney_mesh, init_editor_kenney_materials, kenney_material_slot, KenneyMaterialSlot,
    CyberLaserMaterial, CyberMaterial, CyberMaterialCeiling, CyberMaterialIndustrial,
    CyberMaterialPinkCeiling, KenneyModule, PriesthoodMaterial, SynthMaterial,
    PieceTint, EDITOR_BUILD_TAG,
};

fn editor_active(editor: Option<Res<EditorMode>>, playtest: Option<Res<EditorPlaytestActive>>) -> bool {
    editor.is_some() && playtest.is_none()
}

pub struct KenneyEditorPlugin;

impl Plugin for KenneyEditorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EditorState>()
            .init_resource::<EditorWorkspace>()
            .init_resource::<EditorCam>()
            .init_resource::<SaveFeedback>()
            .init_resource::<SidebarCache>()
            .init_resource::<GalleryRatings>()
            .init_resource::<EditorHistory>()
            .init_resource::<EditorSelection>()
            .init_resource::<PendingHistoryApply>()
            .init_resource::<FloorPaintSession>()
            .init_resource::<KenneyPlaytestGeneration>()
            .init_resource::<crate::editor_map_gen::MapGenSettings>()
            .init_resource::<crate::editor_map_gen::MapGenRuntime>()
            .init_resource::<SkipPlaytestExit>()
            .init_resource::<EditorCamState>()
            .add_systems(Startup, (editor_startup, spawn_editor_sun, set_editor_window_title))
            .add_systems(
                PostUpdate,
                maintain_editor_lighting.run_if(resource_exists::<EditorMode>),
            )
            .add_systems(
                PostUpdate,
                close_menus_on_pointer_leave.run_if(editor_active),
            )
            .add_systems(
                Update,
                (
                    sync_dropdown_menus,
                    sync_sidebar_visibility,
                    rebuild_sidebar,
                    sync_sidebar_highlight,
                    sync_module_info,
                    sidebar_button_input,
                    crate::editor_sidebar::sidebar_pointer_and_scroll,
                    spawn_naming_ui,
                    naming_modal_input,
                    clear_workflow_pieces,
                    file_menu_actions,
                    persist_module_on_map_switch,
                    sync_cam_on_workflow_change,
                    load_module_requested,
                    maybe_respawn_ghost,
                )
                    .chain()
                    .run_if(editor_active),
            )
            .add_systems(
                Update,
                (
                    spawn_load_picker_ui,
                    load_picker_input,
                    menu_button_input,
                    sync_menu_labels,
                    cancel_floor_tool,
                )
                    .run_if(editor_active),
            )
            .add_systems(
                Update,
                (gallery_button_input, gallery_controller_input).run_if(editor_active),
            )
            .add_systems(
                Update,
                (
                    crate::editor_map_gen::map_gen_button_input,
                    crate::editor_map_gen::map_gen_poll,
                    crate::editor_map_gen::map_gen_debounce,
                )
                    .run_if(editor_active),
            )
            .add_systems(
                Update,
                (
                    keep_cursor_free,
                    update_ui_hover_block,
                    editor_input,
                    select_tool_input,
                    select_drag_system,
                    update_ghost,
                    update_piece_visibility,
                    pan_camera,
                    orbit_camera,
                    zoom_camera,
                    update_editor_camera,
                )
                    .chain()
                    .run_if(editor_active),
            )
            .add_systems(
                Update,
                (
                    draw_snap_gizmo,
                    draw_selection_gizmo,
                    sync_floor_slabs,
                    sync_spawn_marker,
                    update_save_toast,
                    refocus_camera,
                    undo_redo_input,
                    apply_pending_history,
                    autosave_on_exit,
                )
                    .chain()
                    .run_if(editor_active),
            )
            .add_systems(
                Update,
                (
                    sync_ceiling_piece_transforms,
                    editor_apply_materials,
                )
                    .chain()
                    .run_if(resource_exists::<EditorMode>),
            )
            .add_systems(
                Update,
                sync_spawn_marker_visibility.run_if(resource_exists::<EditorMode>),
            )
            .add_systems(
                Update,
                (
                    editor_playtest_enter.run_if(editor_active),
                    editor_playtest_exit
                        .after(editor_playtest_enter)
                        .run_if(resource_exists::<EditorPlaytestActive>),
                ),
            )
            .add_systems(
                Update,
                test_return_to_editor.run_if(
                    resource_exists::<TestMode>.and(not(resource_exists::<EditorMode>)),
                ),
            );
    }
}

#[derive(Resource)]
pub struct EditorCam {
    pub focus: Vec3,
    /// Orbit distance from `focus` (scroll wheel adjusts this).
    pub height: f32,
    pub orbit_yaw: f32,
    pub orbit_pitch: f32,
}

/// Default elevation above the xz plane — matches legacy editor (height : back = 1 : 0.45).
const DEFAULT_EDITOR_ELEVATION: f32 = 1.148;

#[derive(Clone, Copy)]
struct CamSnapshot {
    focus: Vec3,
    height: f32,
    orbit_yaw: f32,
    orbit_pitch: f32,
}

#[derive(Resource)]
pub struct EditorCamState {
    map: CamSnapshot,
    module: CamSnapshot,
    dressing: CamSnapshot,
}

impl Default for EditorCamState {
    fn default() -> Self {
        let (cx, cz) = shared::editor_map::MapDocument::new_default().grid().center_xz();
        let (mx, mz) = GridSpec::for_workflow(EditorWorkflow::ModuleMaker, 1, 1).center_xz();
        let (dx, dz) = shared::editor_map::DressingDocument::grid().center_xz();
        Self {
            map: CamSnapshot {
                focus: Vec3::new(cx, 0.0, cz),
                height: 88.0,
                orbit_yaw: 0.0,
                orbit_pitch: DEFAULT_EDITOR_ELEVATION,
            },
            module: CamSnapshot {
                focus: Vec3::new(mx, 0.0, mz),
                height: 42.0,
                orbit_yaw: 0.0,
                orbit_pitch: DEFAULT_EDITOR_ELEVATION,
            },
            dressing: CamSnapshot {
                focus: Vec3::new(dx, 0.0, dz),
                height: 220.0,
                orbit_yaw: 0.0,
                orbit_pitch: DEFAULT_EDITOR_ELEVATION,
            },
        }
    }
}

impl EditorCamState {
    fn get(&self, workflow: EditorWorkflow) -> CamSnapshot {
        match workflow {
            EditorWorkflow::MapMaker => self.map,
            EditorWorkflow::ModuleMaker => self.module,
            EditorWorkflow::SynthDressing => self.dressing,
        }
    }

    fn set(&mut self, workflow: EditorWorkflow, cam: &EditorCam) {
        let snap = CamSnapshot {
            focus: cam.focus,
            height: cam.height,
            orbit_yaw: cam.orbit_yaw,
            orbit_pitch: cam.orbit_pitch,
        };
        match workflow {
            EditorWorkflow::MapMaker => self.map = snap,
            EditorWorkflow::ModuleMaker => self.module = snap,
            EditorWorkflow::SynthDressing => self.dressing = snap,
        }
    }

    fn apply(snap: CamSnapshot, cam: &mut EditorCam) {
        cam.focus = snap.focus;
        cam.height = snap.height;
        cam.orbit_yaw = snap.orbit_yaw;
        cam.orbit_pitch = snap.orbit_pitch;
    }
}

#[derive(Resource, Default)]
struct SaveFeedback {
    message: String,
    ok: bool,
    hide_after: f32,
}

#[derive(Component)]
pub struct SaveToastText;

#[derive(Component)]
pub struct EditorCamera;

#[derive(Component)]
pub struct EditorGhost;

/// Marks one piece of a module ghost preview. Carries the piece-local offset from the module
/// placement center so `update_ghost` can reposition it each frame.
#[derive(Component)]
pub struct EditorModuleGhostPiece {
    pub offset: Vec2,
    pub yaw: f32,
}

#[derive(Component)]
pub(crate) struct EditorModuleReady;

#[derive(Component)]
struct EditorSunLight;

const ZOOM_MIN: f32 = 14.0;
const ZOOM_MAX: f32 = 90.0;

impl Default for EditorCam {
    fn default() -> Self {
        let (cx, cz) = shared::editor_map::MapDocument::new_default().grid().center_xz();
        Self {
            focus: Vec3::new(cx, 0.0, cz),
            height: 72.0,
            orbit_yaw: 0.0,
            orbit_pitch: DEFAULT_EDITOR_ELEVATION,
        }
    }
}

fn editor_startup(
    mut commands: Commands,
    editor: Option<Res<EditorMode>>,
    dressing_shell: Option<Res<shared::DressingShellMode>>,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut state: ResMut<EditorState>,
    mut ws: ResMut<EditorWorkspace>,
    mut cam: ResMut<EditorCam>,
    cam_state: ResMut<EditorCamState>,
    prefs: Res<UserEditorPrefs>,
    mut window: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if editor.is_none() {
        return;
    }

    window.grab_mode = CursorGrabMode::None;
    window.visible = true;

    if dressing_shell.is_some() {
        ws.dressing_only = true;
        ws.set_workflow(EditorWorkflow::SynthDressing);
        let name = shared::editor_catalog::suggest_dressing_name();
        ws.begin_new_dressing(&name);
        state.stems = synth_dressing_stems_filtered(ws.dressing_category);
        state.piece_index = 0;
    } else {
        load_initial_documents(&mut ws);
        crate::editor_sidebar::sync_stems_from_filters(&mut state, &ws.filters);
    }
    let (cx, cz) = ws.grid().center_xz();
    let snap = cam_state.get(ws.workflow);
    cam.focus = snap.focus;
    cam.height = snap.height;
    cam.orbit_yaw = snap.orbit_yaw;
    cam.orbit_pitch = snap.orbit_pitch;
    if cam.focus == Vec3::ZERO {
        cam.focus = Vec3::new(cx, 0.0, cz);
    }

    let (cyber, cyber_lasers, cyber_ceiling, cyber_industrial, pink_ceiling, priesthood, synth) =
        init_editor_kenney_materials(&asset_server, &mut materials);
    commands.insert_resource(cyber);
    commands.insert_resource(cyber_lasers);
    commands.insert_resource(cyber_ceiling);
    commands.insert_resource(cyber_industrial);
    commands.insert_resource(pink_ceiling);
    commands.insert_resource(priesthood);
    commands.insert_resource(synth);

    commands.spawn((
        Camera3d::default(),
        Exposure { ev100: 9.0 },
        EditorCamera,
        editor_cam_transform(&cam),
    ));

    spawn_ghost(&mut commands, &asset_server, &state, ws.as_ref());
    load_pieces(&mut commands, &asset_server, &mut state, &ws);

    commands.spawn((
        SaveToastText,
        Text::new(""),
        TextFont {
            font_size: 18.0,
            ..default()
        },
        TextColor(Color::srgb(0.3, 1.0, 0.45)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(24.0),
            width: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
    ));

    spawn_editor_chrome(&mut commands, &ws, &state, &prefs);
    ws.floor_dirty = true;
    ws.sidebar_dirty = true;
    if ws.dressing_only || ws.workflow == EditorWorkflow::SynthDressing {
        ws.spawn_marker_dirty = true;
    }

    info!(
        "Editor ready [build {}] — {} | piece: {}",
        EDITOR_BUILD_TAG,
        status_line(&ws, &state),
        state.stems.first().map(|s| s.as_str()).unwrap_or("(none)"),
    );
}

fn set_editor_window_title(ws: Res<EditorWorkspace>, mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    window.title = if ws.dressing_only {
        format!("fabled dressing [build {EDITOR_BUILD_TAG}]")
    } else {
        format!("fabled editor [build {EDITOR_BUILD_TAG}]")
    };
}

fn load_initial_documents(ws: &mut EditorWorkspace) {
    if let Some((path, map)) = load_latest_map() {
        ws.map = map;
        ws.active.path = Some(path);
        ws.active.kind = ActiveDocKind::Map;
        return;
    }
    let layout = KenneyLayout::load_from_disk();
    for p in &layout.pieces {
        ws.map.pieces.push(PieceRecord {
            stem: p.stem.clone(),
            x: p.x,
            z: p.z,
            yaw: p.yaw,
            floor_level: p.floor,
            scale: p.scale,
            group_id: p.group_id,
            ceiling: p.ceiling,
            underside: p.underside,
            kit: p.kit.clone(),
            tint: p.tint,
            tags: p.tags.clone(),
            zone: None,
            y: p.y,
        });
    }
}

fn load_latest_map() -> Option<(std::path::PathBuf, shared::editor_map::MapDocument)> {
    let dir = shared::editor_map::maps_dir();
    let mut best: Option<(std::path::PathBuf, std::time::SystemTime)> = None;
    if let Ok(read) = std::fs::read_dir(&dir) {
        for ent in read.flatten() {
            let path = ent.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(meta) = ent.metadata() {
                if let Ok(modified) = meta.modified() {
                    if best.as_ref().is_none_or(|(_, t)| modified > *t) {
                        best = Some((path, modified));
                    }
                }
            }
        }
    }
    let (path, _) = best?;
    shared::editor_map::MapDocument::load(&path).map(|m| (path, m))
}

fn owner_for_workflow(w: EditorWorkflow) -> PieceOwner {
    match w {
        EditorWorkflow::MapMaker => PieceOwner::Map,
        EditorWorkflow::ModuleMaker => PieceOwner::Module,
        EditorWorkflow::SynthDressing => PieceOwner::Dressing,
    }
}

fn is_dressing_workflow(ws: &EditorWorkspace) -> bool {
    ws.workflow == EditorWorkflow::SynthDressing || ws.dressing_only
}

fn dressing_ghost_scale(ws: &EditorWorkspace) -> f32 {
    if is_dressing_workflow(ws) {
        SYNTH_DRESSING_SCALE
    } else {
        1.0
    }
}

fn hover_plane_y(ws: &EditorWorkspace) -> f32 {
    if is_dressing_workflow(ws) {
        floor_y(0)
    } else {
        floor_y(ws.floor_level)
    }
}

fn dressing_snap_y(stem: &str, steps: i32) -> f32 {
    synth_dressing_placement_y(stem, steps)
}

fn update_snap(state: &mut EditorState, ws: &EditorWorkspace) {
    let stem = current_stem(state);
    let y = if is_dressing_workflow(ws) {
        dressing_snap_y(stem, state.dressing_y_steps)
    } else {
        floor_y(ws.floor_level)
    };
    let grid = ws.grid();
    let hover_x = state.cell_sw.x;
    let hover_z = state.cell_sw.y;
    let (pos, yaw, sw_x, sw_z) = placement_for_hover(
        hover_x,
        hover_z,
        stem,
        state.yaw,
        y,
        grid.world_x0(),
        grid.world_z0(),
        ws.snap,
    );
    let mut pos = pos;
    if is_dressing_workflow(ws) {
        if is_synth_structure_wall(stem) && ws.snap == shared::editor_map::SnapMode::FullCell {
            pos += synth_wall_face_offset(yaw);
        }
        if is_synth_balcony_stem(stem) && ws.snap == shared::editor_map::SnapMode::FullCell {
            pos += synth_balcony_outward_offset(yaw);
        }
        if let Some(off) = synth_back_anchor_offset(stem, yaw) {
            pos += off;
        }
        pos += synth_wall_decal_mount_offset(stem, yaw);
    }
    state.snap = pos;
    state.yaw = yaw;
    state.cell_sw = Vec2::new(sw_x, sw_z);
}

fn keep_cursor_free(mut window: Single<&mut CursorOptions, With<PrimaryWindow>>) {
    if window.grab_mode != CursorGrabMode::None {
        window.grab_mode = CursorGrabMode::None;
        window.visible = true;
    }
}

pub fn editor_cam_transform(cam: &EditorCam) -> Transform {
    let dist = cam.height.max(ZOOM_MIN);
    let elev = cam.orbit_pitch.clamp(0.12, 1.52);
    let yaw = cam.orbit_yaw;
    let horiz = dist * elev.cos();
    let vert = dist * elev.sin();
    let pos = cam.focus + Vec3::new(horiz * yaw.sin(), vert, horiz * yaw.cos());
    Transform::from_translation(pos).looking_at(cam.focus, Vec3::Y)
}

/// Horizontal forward/right on the xz plane for camera-relative panning.
fn editor_camera_pan_basis(cam: &EditorCam) -> (Vec2, Vec2) {
    let yaw = cam.orbit_yaw;
    let forward = Vec2::new(-yaw.sin(), -yaw.cos());
    let right = Vec2::new(yaw.cos(), -yaw.sin());
    (forward, right)
}

fn spawn_module(
    commands: &mut Commands,
    asset_server: &AssetServer,
    stem: &str,
    pos: Vec3,
    yaw: f32,
    scale: f32,
    collide: bool,
    floor_level: i32,
    sw_x: f32,
    sw_z: f32,
    owner: PieceOwner,
    piece_id: u32,
    group_id: Option<u32>,
    ceiling: bool,
    underside: bool,
    kit: Option<&str>,
    tint: Option<[f32; 3]>,
    tags: &[String],
) {
    let name = stem_static(stem);
    let path = kit
        .map(|k| glb_asset_path_in_kit(stem, k))
        .unwrap_or_else(|| glb_asset_path(stem));
    let kit_static = kit.map(stem_static);
    let bundle = (
        SceneRoot(asset_server.load_with_settings(
            GltfAssetLabel::Scene(0).from_asset(path),
            |s: &mut GltfLoaderSettings| s.load_meshes = RenderAssetUsages::all(),
        )),
        Transform::from_translation(pos)
            .with_rotation(Quat::from_rotation_y(yaw))
            .with_scale(Vec3::splat(scale.max(0.01))),
        KenneyModule {
            name,
            collide,
            mesh_cutouts: kenney_pit::KenneyMeshCutouts::default(),
            group_id,
            floor: floor_level,
            ceiling,
            kit: kit_static,
        },
        EditorPlaced {
            piece_id,
            floor_level,
            sw_x,
            sw_z,
            owner,
            group_id,
            ceiling,
            underside,
        },
        Visibility::Inherited,
    );
    let entity = commands.spawn(bundle).id();
    if let Some(rgb) = tint {
        commands.entity(entity).insert(crate::test_showcase::PieceTint(rgb));
    }
    if let Some(k) = kit {
        commands
            .entity(entity)
            .insert(crate::test_showcase::PieceKit(k.to_string()));
    }
    if tags.iter().any(|t| t == "hidden_entrance") {
        commands.entity(entity).insert(HiddenEntranceDoor);
    }
    let _ = tags;
}

pub fn spawn_piece_record_pub(
    commands: &mut Commands,
    asset_server: &AssetServer,
    p: &PieceRecord,
    owner: PieceOwner,
    piece_id: u32,
    _extraction_xz: Option<[f32; 2]>,
) {
    // Editor authoring view: only the legacy diagonal-wedge hole props are suppressed
    // (mask-driven hub holes are applied during playtest).
    if kenney_pit::hide_extraction_hatch_piece(&p.stem, p.floor_level, p.x, p.z, None, p.ceiling) {
        return;
    }
    let yaw = quantize_yaw(p.yaw);
    let pos = Vec3::new(p.x, p.world_y(), p.z);
    let (sw_x, sw_z) = sw_from_placement(pos, &p.stem, yaw);
    let collide = kenney_catalog::piece(&p.stem)
        .map(|x| x.collide_default)
        .unwrap_or(true)
        && !p.ceiling;
    spawn_module(
        commands,
        asset_server,
        &p.stem,
        pos,
        yaw,
        p.scale.max(0.01),
        collide,
        p.floor_level,
        sw_x,
        sw_z,
        owner,
        piece_id,
        p.group_id,
        p.ceiling,
        p.underside,
        p.kit.as_deref(),
        p.tint,
        &p.tags,
    );
}

fn spawn_ghost(
    commands: &mut Commands,
    asset_server: &AssetServer,
    state: &EditorState,
    ws: &EditorWorkspace,
) {
    let Some(stem) = state.stems.get(state.piece_index).or(state.stems.first()) else {
        return;
    };
    let name = stem_static(stem);
    let dressing = is_dressing_workflow(ws);
    let path = if dressing {
        glb_asset_path_in_kit(stem, SYNTH_KIT)
    } else {
        glb_asset_path(stem)
    };
    let scale = dressing_ghost_scale(ws);
    let y = if dressing {
        dressing_snap_y(stem, state.dressing_y_steps)
    } else {
        floor_y(ws.floor_level)
    };
    commands.spawn((
        SceneRoot(asset_server.load_with_settings(
            GltfAssetLabel::Scene(0).from_asset(path),
            |s: &mut GltfLoaderSettings| s.load_meshes = RenderAssetUsages::all(),
        )),
        Transform::from_xyz(state.snap.x, y, state.snap.z)
            .with_rotation(Quat::from_rotation_y(state.yaw))
            .with_scale(Vec3::splat(scale)),
        KenneyModule {
            name,
            collide: false,
            mesh_cutouts: kenney_pit::KenneyMeshCutouts::default(),
            group_id: None,
            floor: ws.floor_level,
            ceiling: false,
            kit: if dressing { Some(stem_static(SYNTH_KIT)) } else { None },
        },
        EditorGhost,
        Visibility::Inherited,
    ));
}

/// Spawns one ghost entity per piece of the currently selected module so the user
/// can see exactly what will be placed (with rotation) before clicking.
fn spawn_module_ghost(
    commands: &mut Commands,
    asset_server: &AssetServer,
    ws: &EditorWorkspace,
) {
    let Some(path) = &ws.selected_module else {
        return;
    };
    let Some(module) = shared::editor_map::ModuleDocument::load(path) else {
        return;
    };
    for p in &module.pieces {
        let yaw = quantize_yaw(p.yaw);
        let name = stem_static(&p.stem);
        let asset_path = glb_asset_path(&p.stem);
        commands.spawn((
            SceneRoot(asset_server.load_with_settings(
                GltfAssetLabel::Scene(0).from_asset(asset_path),
                |s: &mut GltfLoaderSettings| s.load_meshes = RenderAssetUsages::all(),
            )),
            Transform::default(),
            KenneyModule {
                name,
                collide: false,
                mesh_cutouts: kenney_pit::KenneyMeshCutouts::default(),
                group_id: p.group_id,
                floor: ws.floor_level + p.floor_level,
                ceiling: false,
                kit: None,
            },
            EditorGhost,
            EditorModuleGhostPiece {
                offset: Vec2::new(p.x, p.z),
                yaw,
            },
            Visibility::Hidden,
        ));
    }
}

fn stem_static(stem: &str) -> &'static str {
    Box::leak(stem.to_string().into_boxed_str())
}

fn load_pieces(
    commands: &mut Commands,
    asset_server: &AssetServer,
    state: &mut EditorState,
    ws: &EditorWorkspace,
) {
    for p in &ws.map.pieces {
        let id = state.next_id;
        spawn_piece_record_pub(
            commands,
            asset_server,
            p,
            PieceOwner::Map,
            id,
            ws.map.extraction_xz,
        );
        state.next_id += 1;
    }
    for p in &ws.module.pieces {
        let id = state.next_id;
        spawn_piece_record_pub(
            commands,
            asset_server,
            p,
            PieceOwner::Module,
            id,
            ws.map.extraction_xz,
        );
        state.next_id += 1;
    }
    for p in &ws.dressing.pieces {
        let id = state.next_id;
        spawn_piece_record_pub(
            commands,
            asset_server,
            p,
            PieceOwner::Dressing,
            id,
            None,
        );
        state.next_id += 1;
    }
}

fn spawn_piece_record(
    commands: &mut Commands,
    asset_server: &AssetServer,
    p: &PieceRecord,
    owner: PieceOwner,
    piece_id: u32,
    extraction_xz: Option<[f32; 2]>,
) {
    spawn_piece_record_pub(
        commands,
        asset_server,
        p,
        owner,
        piece_id,
        extraction_xz,
    );
}

fn ray_world_hit(
    window: &Window,
    camera: (&Camera, &GlobalTransform),
    plane_y: f32,
) -> Option<Vec3> {
    let (cam, cam_gt) = camera;
    let cursor = window.cursor_position()?;
    let ray = cam.viewport_to_world(cam_gt, cursor).ok()?;
    let dir = ray.direction.as_vec3();
    if dir.y.abs() < 1e-5 {
        return None;
    }
    let t = (plane_y - ray.origin.y) / dir.y;
    if t < 0.0 {
        return None;
    }
    Some(ray.origin + dir * t)
}

fn cycle_piece(
    commands: &mut Commands,
    asset_server: &AssetServer,
    state: &mut EditorState,
    ws: &EditorWorkspace,
    ghosts: &Query<Entity, With<EditorGhost>>,
    delta: i32,
) {
    let n = state.stems.len();
    if n == 0 {
        return;
    }
    let idx = state.piece_index as i32;
    state.piece_index = ((idx + delta).rem_euclid(n as i32)) as usize;
    state.dressing_y_steps = 0;
    update_snap(state, ws);
    respawn_ghost(commands, asset_server, state, ghosts, ws);
}

fn sync_pieces_from_world(
    placed: &Query<(Entity, &Transform, &KenneyModule, &EditorPlaced)>,
    owner: PieceOwner,
    prev: &[PieceRecord],
) -> Vec<PieceRecord> {
    placed
        .iter()
        .filter(|(_, _, _, ep)| ep.owner == owner)
        .map(|(_, tf, km, ep)| {
            // kit / tint / zone / tags are editor-map metadata NOT fully mirrored on
            // the entity (zone isn't on it at all). Recover them from the matching
            // existing piece so a quicksave/playtest round-trip doesn't wipe faction
            // zones (the bug where G turned every zone into plain default).
            let prior = prev.iter().find(|p| {
                p.stem == km.name
                    && p.floor_level == ep.floor_level
                    && (p.x - tf.translation.x).abs() < 0.05
                    && (p.z - tf.translation.z).abs() < 0.05
            });
            PieceRecord {
                stem: km.name.to_string(),
                x: tf.translation.x,
                z: tf.translation.z,
                yaw: quantize_yaw(tf.rotation.to_euler(EulerRot::YXZ).0),
                floor_level: ep.floor_level,
                scale: tf.scale.x,
                group_id: ep.group_id,
                ceiling: ep.ceiling,
                underside: ep.underside,
                kit: prior
                    .and_then(|p| p.kit.clone())
                    .or_else(|| km.kit.map(|s| s.to_string())),
                tint: prior.and_then(|p| p.tint),
                tags: prior.map(|p| p.tags.clone()).unwrap_or_default(),
                zone: prior.and_then(|p| p.zone.clone()),
                y: if owner == PieceOwner::Dressing {
                    Some(tf.translation.y)
                } else {
                    prior.and_then(|p| p.y)
                },
            }
        })
        .collect()
}

fn quicksave(
    ws: &mut EditorWorkspace,
    placed: &Query<(Entity, &Transform, &KenneyModule, &EditorPlaced)>,
    save_fb: &mut SaveFeedback,
    time: &Time,
) -> bool {
    let ok = match ws.workflow {
        EditorWorkflow::MapMaker => {
            let prev = std::mem::take(&mut ws.map.pieces);
            ws.map.pieces = sync_pieces_from_world(placed, PieceOwner::Map, &prev);
            let path = ws.active.quicksave_path_map(&ws.map);
            match ws.map.save(&path) {
                Ok(()) => {
                    ws.active.path = Some(path.clone());
                    ws.active.kind = ActiveDocKind::Map;
                    ws.dirty = false;
                    save_fb.message = format!(
                        "Saved map {} ({} pieces)",
                        path.display(),
                        ws.map.pieces.len()
                    );
                    save_fb.ok = true;
                    save_fb.hide_after = time.elapsed_secs() + 4.0;
                    true
                }
                Err(e) => {
                    save_fb.message = format!("Save failed: {e}");
                    save_fb.ok = false;
                    save_fb.hide_after = time.elapsed_secs() + 5.0;
                    false
                }
            }
        }
        EditorWorkflow::ModuleMaker => {
            let prev = std::mem::take(&mut ws.module.pieces);
            ws.module.pieces = sync_pieces_from_world(placed, PieceOwner::Module, &prev);
            match ws.module.save() {
                Ok(path) => {
                    ws.active.path = Some(path.clone());
                    ws.active.kind = ActiveDocKind::Module;
                    ws.dirty = false;
                    save_fb.message = format!(
                        "Saved module {} ({} pieces)",
                        path.display(),
                        ws.module.pieces.len()
                    );
                    save_fb.ok = true;
                    save_fb.hide_after = time.elapsed_secs() + 4.0;
                    true
                }
                Err(e) => {
                    save_fb.message = format!("Save failed: {e}");
                    save_fb.ok = false;
                    save_fb.hide_after = time.elapsed_secs() + 5.0;
                    false
                }
            }
        }
        EditorWorkflow::SynthDressing => {
            let prev = std::mem::take(&mut ws.dressing.pieces);
            ws.dressing.pieces = sync_pieces_from_world(placed, PieceOwner::Dressing, &prev);
            match ws.dressing.save() {
                Ok(path) => {
                    ws.active.path = Some(path.clone());
                    ws.active.kind = ActiveDocKind::Dressing;
                    ws.dirty = false;
                    save_fb.message = format!(
                        "Saved dressing {} ({} pieces)",
                        path.display(),
                        ws.dressing.pieces.len()
                    );
                    save_fb.ok = true;
                    save_fb.hide_after = time.elapsed_secs() + 4.0;
                    true
                }
                Err(e) => {
                    save_fb.message = format!("Save failed: {e}");
                    save_fb.ok = false;
                    save_fb.hide_after = time.elapsed_secs() + 5.0;
                    false
                }
            }
        }
    };
    if ok && ws.workflow == EditorWorkflow::MapMaker {
        let prev = std::mem::take(&mut ws.map.pieces);
        ws.map.pieces = sync_pieces_from_world(placed, PieceOwner::Map, &prev);
        ws.map.apply_hub_playtest_patches();
        let _ = ws.map.export_playtest_layout();
    }
    if ok && ws.workflow == EditorWorkflow::SynthDressing {
        let _ = ws.dressing.export_playtest_layout();
    }
    ok
}

fn autosave_on_exit(
    mut exit: MessageReader<AppExit>,
    mut ws: ResMut<EditorWorkspace>,
    placed: Query<(Entity, &Transform, &KenneyModule, &EditorPlaced)>,
    mut save_fb: ResMut<SaveFeedback>,
    time: Res<Time>,
) {
    if exit.read().next().is_none() || !ws.dirty {
        return;
    }
    let _ = quicksave(&mut ws, &placed, &mut save_fb, &time);
}

fn restore_editor_shell(
    commands: &mut Commands,
    asset_server: &AssetServer,
    state: &EditorState,
    ws: &EditorWorkspace,
    prefs: &UserEditorPrefs,
    cam: &EditorCam,
    toast_message: &str,
) {
    commands.spawn((
        Camera3d::default(),
        Exposure { ev100: 9.0 },
        EditorCamera,
        editor_cam_transform(cam),
    ));
    spawn_ghost(commands, asset_server, state, ws);
    spawn_editor_chrome(commands, ws, state, prefs);
    commands.spawn((
        SaveToastText,
        Text::new(toast_message),
        TextFont {
            font_size: 18.0,
            ..default()
        },
        TextColor(Color::srgb(0.35, 1.0, 0.5)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(24.0),
            width: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
    ));
}

fn launch_kenney_editor() {
    relaunch_fabled(&["--host", "--editor"]);
}

fn test_return_to_editor(keys: Res<ButtonInput<KeyCode>>, test: Option<Res<TestMode>>) {
    let Some(test) = test else {
        return;
    };
    if test.style != TestMapStyle::Kenney {
        return;
    }
    if keys.just_pressed(KeyCode::KeyG) {
        info!("returning to Kenney editor");
        launch_kenney_editor();
    }
}

fn editor_input(
    mut commands: Commands,
    mut state: ResMut<EditorState>,
    mut ws: ResMut<EditorWorkspace>,
    mut save_fb: ResMut<SaveFeedback>,
    mut history: ResMut<EditorHistory>,
    mut floor_session: ResMut<FloorPaintSession>,
    mut sel: ResMut<EditorSelection>,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut wheel: MessageReader<MouseWheel>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<EditorCamera>>,
    ghosts: Query<Entity, With<EditorGhost>>,
    placed: Query<(Entity, &Transform, &KenneyModule, &EditorPlaced)>,
) {
    if mouse.just_pressed(MouseButton::Forward) {
        state.yaw += std::f32::consts::FRAC_PI_2;
        update_snap(&mut state, &ws);
    }
    if mouse.just_pressed(MouseButton::Back) {
        state.yaw -= std::f32::consts::FRAC_PI_2;
        update_snap(&mut state, &ws);
    }
    if keys.just_pressed(KeyCode::Equal) || keys.just_pressed(KeyCode::NumpadAdd) {
        ws.floor_level += 1;
        ws.floor_dirty = true;
        update_snap(&mut state, &ws);
    }
    if keys.just_pressed(KeyCode::Minus) || keys.just_pressed(KeyCode::NumpadSubtract) {
        ws.floor_level -= 1;
        ws.floor_dirty = true;
        update_snap(&mut state, &ws);
    }
    if is_dressing_workflow(&ws) && ws.tool == EditorTool::PlaceGlb {
        if keys.just_pressed(KeyCode::KeyE) {
            state.dressing_y_steps += 1;
            update_snap(&mut state, &ws);
        }
        if keys.just_pressed(KeyCode::KeyQ) {
            state.dressing_y_steps -= 1;
            update_snap(&mut state, &ws);
        }
        if keys.just_pressed(KeyCode::KeyF) {
            update_snap(&mut state, &ws);
            let stem = current_stem(&state);
            let dx = state.hover_world.x - state.snap.x;
            let dz = state.hover_world.y - state.snap.z;
            state.yaw = synth_face_yaw(stem, dx, dz);
            update_snap(&mut state, &ws);
        }
    }

    // Gallery preview mode: all placement/ghost/scroll handled by gallery_controller_input.
    if ws.tool == EditorTool::GalleryPreview {
        for _ in wheel.read() {}
        return;
    }

    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    if !ws.sidebar_pointer_inside && !ctrl {
        for ev in wheel.read() {
            let delta = match ev.unit {
                MouseScrollUnit::Line => ev.y,
                MouseScrollUnit::Pixel => ev.y / 120.0,
            };
            if delta == 0.0 {
                continue;
            }
            let dir = if delta > 0.0 { -1 } else { 1 };
            if ws.tool == EditorTool::PlaceModule {
                ws.module_cycle_delta += dir;
            } else {
                cycle_piece(&mut commands, &asset_server, &mut state, &ws, &ghosts, dir);
            }
        }
    } else {
        for _ in wheel.read() {}
    }

    let grid = ws.grid();
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok(cam) = camera.single() else {
        return;
    };
    if let Some(hit) = ray_world_hit(window, cam, hover_plane_y(&ws)) {
        state.hover_world = Vec2::new(hit.x, hit.z);
        state.cell_sw = state.hover_world;
        update_snap(&mut state, &ws);

        if is_floor_tool(ws.tool) {
            let add = ws.tool == EditorTool::FloorAdd;
            ws.floor_paint_add = add;
            if mouse.just_pressed(MouseButton::Left) && !ws.pointer_over_ui {
                floor_session.active = true;
                floor_session.before = Some(clone_floor_mask(&ws));
                ws.floor_painting = true;
                if let Some((ix, iz)) = cell_index_from_world(hit.x, hit.z, &grid) {
                    ws.floor_paint_start = Some((ix, iz));
                    ws.floor_paint_preview = Some((ix, iz, ix, iz));
                    ws.floor_dirty = true;
                }
            }
            if mouse.pressed(MouseButton::Left) && ws.floor_painting {
                if let (Some((sx, sz)), Some((ix, iz))) = (
                    ws.floor_paint_start,
                    cell_index_from_world(hit.x, hit.z, &grid),
                ) {
                    let ix0 = sx.min(ix);
                    let ix1 = sx.max(ix);
                    let iz0 = sz.min(iz);
                    let iz1 = sz.max(iz);
                    ws.floor_paint_preview = Some((ix0, iz0, ix1, iz1));
                    ws.floor_dirty = true;
                }
            }
        }
    }
    if mouse.just_released(MouseButton::Left) {
        if floor_session.active {
            if let Some((ix0, iz0, ix1, iz1)) = ws.floor_paint_preview {
                let add = ws.floor_paint_add;
                paint_floor_rect(&mut ws, ix0, iz0, ix1, iz1, add);
            }
            if let Some((workflow, level, before)) = floor_session.before.take() {
                let (_, _, after) = clone_floor_mask(&ws);
                if before != after {
                    history.push(HistoryOp::FloorMask {
                        workflow,
                        level,
                        before,
                        after,
                    });
                }
            }
            floor_session.active = false;
        }
        ws.floor_painting = false;
        ws.floor_paint_preview = None;
        ws.floor_paint_start = None;
        ws.floor_dirty = true;
    }

    let owner = owner_for_workflow(ws.workflow);
    if ws.tool == EditorTool::PlaceGlb && mouse.just_pressed(MouseButton::Left) && !ws.pointer_over_ui {
        let Some(stem) = state.stems.get(state.piece_index).cloned() else {
            return;
        };
        let collide = kenney_catalog::piece(&stem)
            .map(|p| p.collide_default)
            .unwrap_or(true);
        let piece_id = state.next_id;
        let dressing = is_dressing_workflow(&ws);
        let (kit, scale, y) = if dressing {
            (
                Some(SYNTH_KIT),
                SYNTH_DRESSING_SCALE,
                Some(dressing_snap_y(&stem, state.dressing_y_steps)),
            )
        } else {
            (None, 1.0, None)
        };
        spawn_module(
            &mut commands,
            &asset_server,
            &stem,
            state.snap,
            state.yaw,
            scale,
            collide,
            ws.floor_level,
            state.cell_sw.x,
            state.cell_sw.y,
            owner,
            piece_id,
            None,
            false,
            false,
            kit,
            None,
            &[],
        );
        state.next_id += 1;
        ws.dirty = true;
        let record = PieceRecord {
            stem: stem.clone(),
            x: state.snap.x,
            z: state.snap.z,
            yaw: state.yaw,
            floor_level: ws.floor_level,
            scale,
            group_id: None,
            ceiling: false,
            underside: false,
            kit: kit.map(|k| k.to_string()),
            tint: None,
            tags: vec![],
            zone: None,
            y,
        };
        match ws.workflow {
            EditorWorkflow::MapMaker => ws.map.pieces.push(record.clone()),
            EditorWorkflow::ModuleMaker => ws.module.pieces.push(record.clone()),
            EditorWorkflow::SynthDressing => ws.dressing.pieces.push(record.clone()),
        }
        history.push(HistoryOp::Place(vec![snapshot_from_record(
            piece_id, owner, &record,
        )]));
    }

    if ws.workflow == EditorWorkflow::MapMaker
        && ws.tool == EditorTool::PlaceModule
        && mouse.just_pressed(MouseButton::Left)
        && !ws.pointer_over_ui
    {
        if let Some(path) = ws.selected_module.clone() {
            let (cx, cz) = snap_to_module_slot(state.hover_world.x, state.hover_world.y, &grid);
            place_module_on_map(
                &mut commands,
                &asset_server,
                &mut state,
                &mut ws,
                &mut history,
                &path,
                cx,
                cz,
            );
        }
    }

    if (ws.workflow == EditorWorkflow::MapMaker || is_dressing_workflow(&ws))
        && ws.tool == EditorTool::SetSpawn
        && mouse.just_pressed(MouseButton::Left)
        && !ws.pointer_over_ui
    {
        let xz = [state.hover_world.x, state.hover_world.y];
        if is_dressing_workflow(&ws) {
            ws.dressing.spawn_xz = Some(xz);
            ws.dressing.spawn_y = Some(SYNTH_DECK_Y);
        } else {
            ws.map.spawn_xz = Some(xz);
        }
        ws.dirty = true;
        ws.spawn_marker_dirty = true;
    }

    if ws.tool == EditorTool::Select {
        if keys.just_pressed(KeyCode::Delete) {
            let mut to_despawn: Vec<Entity> = Vec::new();
            let mut all_undo_snaps = Vec::new();
            let mut groups_to_remove: std::collections::HashSet<u32> =
                std::collections::HashSet::new();

            for id in &sel.selected {
                for (e, tf, km, ep) in &placed {
                    if ep.piece_id != *id || ep.owner != owner {
                        continue;
                    }
                    if let Some(gid) = ep.group_id {
                        groups_to_remove.insert(gid);
                    } else {
                        to_despawn.push(e);
                        all_undo_snaps
                            .push(snapshot_from_entity(ep.piece_id, km.name, tf, ep, km));
                    }
                    break;
                }
            }
            // Collect every member of groups that were hit.
            for (ge, gtf, gkm, gep) in &placed {
                if let Some(gid) = gep.group_id {
                    if groups_to_remove.contains(&gid) && gep.owner == owner {
                        to_despawn.push(ge);
                        all_undo_snaps
                            .push(snapshot_from_entity(gep.piece_id, gkm.name, gtf, gep, gkm));
                    }
                }
            }
            for e in to_despawn {
                commands.entity(e).despawn();
            }
            for gid in groups_to_remove {
                remove_group_from_document(&mut ws, owner, gid);
            }
            for snap in &all_undo_snaps {
                if snap.group_id.is_none() {
                    remove_piece_from_document(&mut ws, snap);
                }
            }
            if !all_undo_snaps.is_empty() {
                history.push(HistoryOp::Delete(all_undo_snaps));
                ws.dirty = true;
            }
            sel.selected.clear();
        }
    }

    if mouse.just_pressed(MouseButton::Right) && !ws.pointer_over_ui {
        let mut found_group: Option<u32> = None;
        if let Some(id) = pick_piece_at(state.hover_world, ws.floor_level, owner, &placed) {
            for (e, tf, km, ep) in &placed {
                if ep.piece_id != id || ep.floor_level != ws.floor_level || ep.owner != owner {
                    continue;
                }
                if let Some(gid) = ep.group_id {
                    remove_group_from_document(&mut ws, owner, gid);
                    found_group = Some(gid);
                } else {
                    let snap = snapshot_from_entity(ep.piece_id, km.name, tf, ep, km);
                    history.push(HistoryOp::Delete(vec![snap.clone()]));
                    remove_piece_from_document(&mut ws, &snap);
                    commands.entity(e).despawn();
                    if sel.selected.contains(&ep.piece_id) {
                        sel.selected.retain(|id| *id != ep.piece_id);
                    }
                }
                ws.dirty = true;
                break;
            }
        }
        // When a whole module group was right-clicked, despawn every entity in that group.
        if let Some(gid) = found_group {
            let mut undo_snaps = Vec::new();
            for (ge, gtf, gkm, gep) in &placed {
                if gep.group_id == Some(gid) && gep.owner == owner {
                    undo_snaps.push(snapshot_from_entity(gep.piece_id, gkm.name, gtf, gep, gkm));
                    commands.entity(ge).despawn();
                }
            }
            if !undo_snaps.is_empty() {
                history.push(HistoryOp::Delete(undo_snaps));
            }
        }
    }

    if keys.just_pressed(KeyCode::KeyF) || ws.file_save {
        ws.file_save = false;
        quicksave(&mut ws, &placed, &mut save_fb, &time);
    }
}

fn place_module_on_map(
    commands: &mut Commands,
    asset_server: &AssetServer,
    state: &mut EditorState,
    ws: &mut EditorWorkspace,
    history: &mut EditorHistory,
    path: &std::path::Path,
    center_x: f32,
    center_z: f32,
) {
    let Some(module) = shared::editor_map::ModuleDocument::load(path) else {
        return;
    };
    let group_id = state.next_id;
    state.next_id += 1;
    let mut undo_snaps = Vec::new();
    for p in &module.pieces {
        let piece_id = state.next_id;
        let record = PieceRecord {
            stem: p.stem.clone(),
            x: p.x + center_x,
            z: p.z + center_z,
            yaw: p.yaw,
            floor_level: p.floor_level + ws.floor_level,
            scale: p.scale,
            group_id: Some(group_id),
            ceiling: p.ceiling,
            underside: p.underside,
            kit: p.kit.clone(),
            tint: p.tint,
            tags: p.tags.clone(),
            zone: p.zone.clone(),
            y: p.y,
        };
        spawn_piece_record(
            commands,
            asset_server,
            &record,
            PieceOwner::Map,
            piece_id,
            ws.map.extraction_xz,
        );
        undo_snaps.push(snapshot_from_record(piece_id, PieceOwner::Map, &record));
        ws.map.pieces.push(record);
        state.next_id += 1;
    }
    shared::editor_map::ActiveDocument::bake_module_floor_on_map(
        &mut ws.map,
        &module,
        center_x,
        center_z,
        ws.floor_level,
    );
    ws.floor_dirty = true;
    if !undo_snaps.is_empty() {
        history.push(HistoryOp::Place(undo_snaps));
    }
    ws.dirty = true;
}

fn spawn_naming_ui(mut commands: Commands, mut ws: ResMut<EditorWorkspace>) {
    if !ws.spawn_naming_ui {
        return;
    }
    ws.spawn_naming_ui = false;
    if let Some(ref modal) = ws.naming_modal {
        spawn_naming_modal(&mut commands, &modal.buffer);
    }
}

fn clear_workflow_pieces(
    mut commands: Commands,
    mut ws: ResMut<EditorWorkspace>,
    placed: Query<(Entity, &EditorPlaced)>,
) {
    if ws.clear_module_pieces {
        ws.clear_module_pieces = false;
        for (e, ep) in &placed {
            if ep.owner == PieceOwner::Module {
                commands.entity(e).despawn();
            }
        }
        ws.set_workflow(EditorWorkflow::ModuleMaker);
        ws.sidebar_dirty = true;
    }
    if ws.clear_dressing_pieces {
        ws.clear_dressing_pieces = false;
        for (e, ep) in &placed {
            if ep.owner == PieceOwner::Dressing {
                commands.entity(e).despawn();
            }
        }
        ws.sidebar_dirty = true;
    }
}

fn file_menu_actions(
    mut commands: Commands,
    mut ws: ResMut<EditorWorkspace>,
    mut state: ResMut<EditorState>,
    mut history: ResMut<EditorHistory>,
    mut save_fb: ResMut<SaveFeedback>,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    placed: Query<(Entity, &Transform, &KenneyModule, &EditorPlaced)>,
    map_placed: Query<(Entity, &EditorPlaced)>,
    _ghosts: Query<Entity, With<EditorGhost>>,
) {
    if ws.file_save {
        ws.file_save = false;
        let _ = quicksave(&mut ws, &placed, &mut save_fb, &time);
    }

    if ws.file_save_as {
        ws.file_save_as = false;
        ws.open_naming_modal();
        ws.save_as_requested = true;
    }

    if ws.file_new {
        ws.file_new = false;
        match ws.workflow {
            EditorWorkflow::MapMaker => {
                ws.map = shared::editor_map::MapDocument::new_default();
                ws.active.path = None;
                for (e, ep) in &map_placed {
                    if ep.owner == PieceOwner::Map {
                        commands.entity(e).despawn();
                    }
                }
                ws.floor_dirty = true;
                ws.dirty = false;
                ws.refocus_camera = true;
            }
            EditorWorkflow::ModuleMaker => {
                // Auto-save current module before starting a new one so the
                // user doesn't accidentally overwrite it with the new name.
                if ws.dirty {
                    let prev_mod = std::mem::take(&mut ws.module.pieces);
                    ws.module.pieces = sync_pieces_from_world(&placed, PieceOwner::Module, &prev_mod);
                    if let Ok(path) = ws.module.save() {
                        ws.active.path = Some(path);
                        ws.dirty = false;
                        save_fb.message = "Auto-saved before new module".into();
                        save_fb.ok = true;
                        save_fb.hide_after = time.elapsed_secs() + 2.0;
                    }
                }
                ws.new_module_requested = true;
                ws.open_naming_modal();
            }
            EditorWorkflow::SynthDressing => {
                if ws.dirty {
                    let prev = std::mem::take(&mut ws.dressing.pieces);
                    ws.dressing.pieces =
                        sync_pieces_from_world(&placed, PieceOwner::Dressing, &prev);
                    if let Ok(path) = ws.dressing.save() {
                        ws.active.path = Some(path);
                        ws.dirty = false;
                        save_fb.message = "Auto-saved before new vignette".into();
                        save_fb.ok = true;
                        save_fb.hide_after = time.elapsed_secs() + 2.0;
                    }
                }
                ws.new_dressing_requested = true;
                ws.open_naming_modal();
            }
        }
    }

    if ws.file_discard {
        ws.file_discard = false;
        discard_all_content(
            &mut commands,
            &mut ws,
            &mut state,
            &mut history,
            &map_placed,
        );
        save_fb.message = "Discarded — cleared map and module".into();
        save_fb.ok = true;
        save_fb.hide_after = time.elapsed_secs() + 3.0;
    }

    if ws.file_load {
        ws.file_load = false;
        ws.pending_load_picker = true;
    }

    if let Some(path) = ws.pending_load_map.take() {
        let gen_load = ws.pending_map_gen_load;
        ws.pending_map_gen_load = false;
        if let Some(mut map) = shared::editor_map::MapDocument::load(&path) {
            if gen_load {
                map.apply_hub_playtest_patches();
                let _ = map.export_playtest_layout();
            }
            ws.map = map;
            ws.active.path = Some(path);
            for (e, ep) in &map_placed {
                if ep.owner == PieceOwner::Map {
                    commands.entity(e).despawn();
                }
            }
            for p in &ws.map.pieces.clone() {
                let id = state.next_id;
                spawn_piece_record_pub(
                    &mut commands,
                    &asset_server,
                    p,
                    PieceOwner::Map,
                    id,
                    ws.map.extraction_xz,
                );
                state.next_id += 1;
            }
            ws.floor_dirty = true;
            ws.refocus_camera = true;
            ws.spawn_marker_dirty = true;
            save_fb.message = if gen_load {
                "Procedural map loaded".into()
            } else {
                "Map loaded".into()
            };
            save_fb.ok = true;
            save_fb.hide_after = time.elapsed_secs() + 3.0;
        }
    }

    if let Some(path) = ws.pending_load_dressing.take() {
        if let Some(doc) = shared::editor_map::DressingDocument::load(&path) {
            ws.dressing = doc;
            ws.active.path = Some(path);
            ws.active.kind = ActiveDocKind::Dressing;
            for (e, ep) in &map_placed {
                if ep.owner == PieceOwner::Dressing {
                    commands.entity(e).despawn();
                }
            }
            for p in &ws.dressing.pieces.clone() {
                let id = state.next_id;
                spawn_piece_record_pub(
                    &mut commands,
                    &asset_server,
                    p,
                    PieceOwner::Dressing,
                    id,
                    None,
                );
                state.next_id += 1;
            }
            ws.floor_dirty = true;
            ws.spawn_marker_dirty = true;
            ws.refocus_camera = true;
            ws.dirty = false;
            save_fb.message = "Dressing vignette loaded".into();
            save_fb.ok = true;
            save_fb.hide_after = time.elapsed_secs() + 3.0;
        }
    }

    if let Some(name) = ws.pending_save_as_name.take() {
        if ws.new_module_requested {
            // "New module" path: clear everything and start fresh with the chosen name.
            ws.new_module_requested = false;
            for (e, ep) in &map_placed {
                if ep.owner == PieceOwner::Module {
                    commands.entity(e).despawn();
                }
            }
            let pool = ws.module.pool.clone();
            ws.module = shared::editor_map::ModuleDocument::new_named(name, pool);
            ws.active.path = None;
            ws.dirty = false;
            ws.floor_dirty = true;
            ws.refocus_camera = true;
            ws.sidebar_dirty = true;
            history.clear();
            save_fb.message = "New module created".into();
            save_fb.ok = true;
            save_fb.hide_after = time.elapsed_secs() + 2.0;
        } else if ws.new_dressing_requested {
            ws.new_dressing_requested = false;
            for (e, ep) in &map_placed {
                if ep.owner == PieceOwner::Dressing {
                    commands.entity(e).despawn();
                }
            }
            ws.begin_new_dressing(&name);
            history.clear();
            save_fb.message = "New dressing vignette".into();
            save_fb.ok = true;
            save_fb.hide_after = time.elapsed_secs() + 2.0;
        } else if ws.workflow == EditorWorkflow::SynthDressing {
            let prev = std::mem::take(&mut ws.dressing.pieces);
            ws.dressing.pieces = sync_pieces_from_world(&placed, PieceOwner::Dressing, &prev);
            ws.dressing.name = name.clone();
            ws.dressing.vignette = Some(name);
            match ws.dressing.save() {
                Ok(path) => {
                    ws.active.path = Some(path.clone());
                    ws.dirty = false;
                    ws.sidebar_dirty = true;
                    save_fb.message = format!("Saved as {}", path.display());
                    save_fb.ok = true;
                    save_fb.hide_after = time.elapsed_secs() + 4.0;
                }
                Err(e) => {
                    save_fb.message = format!("Save as failed: {e}");
                    save_fb.ok = false;
                    save_fb.hide_after = time.elapsed_secs() + 5.0;
                }
            }
        } else {
            // "Save As" path: save current content under a new name.
            let prev_mod = std::mem::take(&mut ws.module.pieces);
            ws.module.pieces = sync_pieces_from_world(&placed, PieceOwner::Module, &prev_mod);
            ws.module.name = name;
            match ws.module.save() {
                Ok(path) => {
                    ws.active.path = Some(path.clone());
                    ws.dirty = false;
                    ws.sidebar_dirty = true;
                    save_fb.message = format!("Saved as {}", path.display());
                    save_fb.ok = true;
                    save_fb.hide_after = time.elapsed_secs() + 4.0;
                }
                Err(e) => {
                    save_fb.message = format!("Save as failed: {e}");
                    save_fb.ok = false;
                    save_fb.hide_after = time.elapsed_secs() + 5.0;
                }
            }
        }
    }
}

fn sync_cam_on_workflow_change(
    ws: Res<EditorWorkspace>,
    mut cam: ResMut<EditorCam>,
    mut cam_state: ResMut<EditorCamState>,
    mut last: Local<Option<EditorWorkflow>>,
) {
    if last.is_none() {
        *last = Some(ws.workflow);
        return;
    }
    let from = last.unwrap();
    if from == ws.workflow {
        return;
    }
    cam_state.set(from, &cam);
    EditorCamState::apply(cam_state.get(ws.workflow), &mut cam);
    *last = Some(ws.workflow);
}

fn persist_module_on_map_switch(
    mut ws: ResMut<EditorWorkspace>,
    mut last: Local<Option<EditorWorkflow>>,
    placed: Query<(Entity, &Transform, &KenneyModule, &EditorPlaced)>,
    mut save_fb: ResMut<SaveFeedback>,
    time: Res<Time>,
) {
    if last.is_none() {
        *last = Some(ws.workflow);
        return;
    }
    let from = last.unwrap();
    if from == ws.workflow {
        return;
    }
    *last = Some(ws.workflow);
    if from == EditorWorkflow::ModuleMaker && ws.dirty {
        let prev_mod = std::mem::take(&mut ws.module.pieces);
        ws.module.pieces = sync_pieces_from_world(&placed, PieceOwner::Module, &prev_mod);
        if let Ok(path) = ws.module.save() {
            ws.active.path = Some(path.clone());
            ws.dirty = false;
            ws.sidebar_dirty = true;
            save_fb.message = format!("Module saved — visible in Modules list ({})", path.display());
            save_fb.ok = true;
            save_fb.hide_after = time.elapsed_secs() + 4.0;
        }
    }
    if from == EditorWorkflow::SynthDressing && ws.dirty {
        let prev = std::mem::take(&mut ws.dressing.pieces);
        ws.dressing.pieces = sync_pieces_from_world(&placed, PieceOwner::Dressing, &prev);
        if let Ok(path) = ws.dressing.save() {
            ws.active.path = Some(path.clone());
            ws.dirty = false;
            ws.sidebar_dirty = true;
            save_fb.message = format!("Dressing saved ({})", path.display());
            save_fb.ok = true;
            save_fb.hide_after = time.elapsed_secs() + 4.0;
        }
    }
}

fn maybe_respawn_ghost(
    mut commands: Commands,
    mut ws: ResMut<EditorWorkspace>,
    asset_server: Res<AssetServer>,
    mut state: ResMut<EditorState>,
    ghosts: Query<Entity, With<EditorGhost>>,
    cache: Res<SidebarCache>,
) {
    // Cycle selected module via scroll wheel (accumulated in editor_input).
    let module_delta = std::mem::replace(&mut ws.module_cycle_delta, 0);
    if module_delta != 0 && ws.tool == EditorTool::PlaceModule {
        let n = cache.modules.len();
        if n > 0 {
            let current_idx = cache
                .modules
                .iter()
                .position(|(_, p)| Some(p) == ws.selected_module.as_ref())
                .unwrap_or(0) as i32;
            let next = (current_idx + module_delta).rem_euclid(n as i32) as usize;
            ws.selected_module = cache.modules.get(next).map(|(_, p)| p.clone());
            ws.respawn_ghost = true;
            ws.sidebar_dirty = true;
        }
    }

    if !ws.respawn_ghost {
        return;
    }
    ws.respawn_ghost = false;

    if is_dressing_workflow(&ws) {
        state.stems = if ws.dressing_only {
            synth_dressing_stems_filtered(ws.dressing_category)
        } else {
            synth_dressing_stems()
        };
        if state.piece_index >= state.stems.len() {
            state.piece_index = 0;
        }
    }

    // Despawn all existing ghost entities.
    for e in &ghosts {
        commands.entity(e).despawn();
    }

    if ws.tool == EditorTool::PlaceModule {
        spawn_module_ghost(&mut commands, &asset_server, &ws);
    } else {
        update_snap(&mut state, &ws);
        spawn_ghost(&mut commands, &asset_server, &state, &ws);
    }
}

fn load_module_requested(
    mut commands: Commands,
    mut ws: ResMut<EditorWorkspace>,
    asset_server: Res<AssetServer>,
    mut state: ResMut<EditorState>,
    placed: Query<(Entity, &EditorPlaced)>,
) {
    let Some(path) = ws.load_module_path.take() else {
        return;
    };
    let Some(doc) = shared::editor_map::ModuleDocument::load(&path) else {
        return;
    };
    for (e, ep) in &placed {
        if ep.owner == PieceOwner::Module {
            commands.entity(e).despawn();
        }
    }
    ws.module = doc;
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        ws.module.name = stem.to_string();
    }
    ws.active.path = Some(path);
    ws.active.kind = shared::editor_map::ActiveDocKind::Module;
    ws.workflow = EditorWorkflow::ModuleMaker;
    ws.selected_module = None;
    if ws.tool == EditorTool::PlaceModule {
        ws.tool = EditorTool::PlaceGlb;
    }
    ws.respawn_ghost = true;
    ws.floor_dirty = true;
    ws.dirty = false;
    ws.refocus_camera = true;
    ws.sidebar_dirty = true;
    for p in &ws.module.pieces.clone() {
        let id = state.next_id;
        spawn_piece_record_pub(
            &mut commands,
            &asset_server,
            p,
            PieceOwner::Module,
            id,
            ws.map.extraction_xz,
        );
        state.next_id += 1;
    }
}

fn zoom_camera(
    mut wheel: MessageReader<MouseWheel>,
    keys: Res<ButtonInput<KeyCode>>,
    ws: Res<EditorWorkspace>,
    mut cam: ResMut<EditorCam>,
) {
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    if !ctrl || ws.sidebar_pointer_inside {
        for _ in wheel.read() {}
        return;
    }
    for ev in wheel.read() {
        let delta = match ev.unit {
            MouseScrollUnit::Line => ev.y,
            MouseScrollUnit::Pixel => ev.y / 120.0,
        };
        if delta == 0.0 {
            continue;
        }
        cam.height = (cam.height - delta * 4.0).clamp(ZOOM_MIN, ZOOM_MAX);
    }
}

fn respawn_ghost(
    commands: &mut Commands,
    asset_server: &AssetServer,
    state: &EditorState,
    ghosts: &Query<Entity, With<EditorGhost>>,
    ws: &EditorWorkspace,
) {
    for e in ghosts.iter() {
        commands.entity(e).despawn();
    }
    spawn_ghost(commands, asset_server, state, ws);
}

fn update_ghost(
    state: Res<EditorState>,
    ws: Res<EditorWorkspace>,
    mut glb_ghosts: Query<
        (&mut Transform, &mut Visibility),
        (With<EditorGhost>, Without<EditorModuleGhostPiece>),
    >,
    mut mod_ghosts: Query<
        (&mut Transform, &mut Visibility, &EditorModuleGhostPiece),
        With<EditorGhost>,
    >,
) {
    // --- GLB single-piece ghost ---
    let show_glb =
        ws.tool == EditorTool::PlaceGlb && !state.stems.is_empty() && !ws.pointer_over_ui;
    for (mut tf, mut vis) in &mut glb_ghosts {
        let scale = dressing_ghost_scale(&ws);
        *tf = Transform::from_translation(state.snap)
            .with_rotation(Quat::from_rotation_y(state.yaw))
            .with_scale(Vec3::splat(scale));
        *vis = if show_glb {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    // --- Module multi-piece ghost ---
    let show_mod =
        ws.tool == EditorTool::PlaceModule && ws.selected_module.is_some() && !ws.pointer_over_ui;
    let fy = floor_y(ws.floor_level);
    let grid = ws.grid();
    let (cx, cz) = snap_to_module_slot(state.hover_world.x, state.hover_world.y, &grid);
    for (mut tf, mut vis, gp) in &mut mod_ghosts {
        *tf = Transform::from_xyz(cx + gp.offset.x, fy, cz + gp.offset.y)
            .with_rotation(Quat::from_rotation_y(gp.yaw));
        *vis = if show_mod {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

fn update_piece_visibility(
    ws: Res<EditorWorkspace>,
    playtest: Option<Res<crate::editor_playtest::EditorPlaytestActive>>,
    mut placed: Query<(&EditorPlaced, &mut Visibility)>,
) {
    if playtest.is_some() {
        for (_, mut vis) in &mut placed {
            *vis = Visibility::Inherited;
        }
        return;
    }
    let owner = owner_for_workflow(ws.workflow);
    for (ep, mut vis) in &mut placed {
        *vis = if ep.floor_level == ws.floor_level && ep.owner == owner {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

fn refocus_camera(
    mut cam: ResMut<EditorCam>,
    mut ws: ResMut<EditorWorkspace>,
    mut cam_state: ResMut<EditorCamState>,
) {
    if !ws.refocus_camera {
        return;
    }
    ws.refocus_camera = false;
    let (cx, cz) = ws.grid().center_xz();
    cam.focus = Vec3::new(cx, 0.0, cz);
    cam.height = match ws.workflow {
        EditorWorkflow::MapMaker => 88.0,
        EditorWorkflow::ModuleMaker => 42.0,
        EditorWorkflow::SynthDressing => 220.0,
    };
    cam.orbit_yaw = 0.0;
    cam.orbit_pitch = DEFAULT_EDITOR_ELEVATION;
    cam_state.set(ws.workflow, &cam);
}

fn pan_camera(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    ws: Res<EditorWorkspace>,
    mut cam: ResMut<EditorCam>,
    mut cam_state: ResMut<EditorCamState>,
) {
    let mut delta = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        delta.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        delta.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        delta.x += 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        delta.x -= 1.0;
    }
    if delta == Vec2::ZERO {
        return;
    }
    let (forward, right) = editor_camera_pan_basis(&cam);
    let speed = 20.0;
    let step = speed * time.delta_secs();
    cam.focus.x += (forward.x * (-delta.y) + right.x * delta.x) * step;
    cam.focus.z += (forward.y * (-delta.y) + right.y * delta.x) * step;
    cam_state.set(ws.workflow, &cam);
}

fn orbit_camera(
    mouse: Res<ButtonInput<MouseButton>>,
    mut motion: MessageReader<MouseMotion>,
    ws: Res<EditorWorkspace>,
    mut cam: ResMut<EditorCam>,
    mut cam_state: ResMut<EditorCamState>,
) {
    if ws.pointer_over_ui {
        for _ in motion.read() {}
        return;
    }
    if !mouse.pressed(MouseButton::Middle) {
        for _ in motion.read() {}
        return;
    }
    for ev in motion.read() {
        cam.orbit_yaw -= ev.delta.x * 0.005;
        cam.orbit_pitch = (cam.orbit_pitch + ev.delta.y * 0.005).clamp(0.12, 1.52);
    }
    cam_state.set(ws.workflow, &cam);
}

fn update_editor_camera(cam: Res<EditorCam>, mut q: Query<&mut Transform, With<EditorCamera>>) {
    let Ok(mut tf) = q.single_mut() else {
        return;
    };
    *tf = editor_cam_transform(&cam);
}

/// Snap a world-space hover position to the nearest module slot centre.
fn snap_to_module_slot(hx: f32, hz: f32, grid: &shared::editor_map::GridSpec) -> (f32, f32) {
    let x0 = grid.world_x0();
    let z0 = grid.world_z0();
    let col = ((hx - x0) / KENNEY_MOD_M).floor() as i32;
    let row = ((hz - z0) / KENNEY_MOD_M).floor() as i32;
    let cx = x0 + col as f32 * KENNEY_MOD_M + KENNEY_MOD_M * 0.5;
    let cz = z0 + row as f32 * KENNEY_MOD_M + KENNEY_MOD_M * 0.5;
    (cx, cz)
}

fn draw_snap_gizmo(mut gizmos: Gizmos, state: Res<EditorState>, ws: Res<EditorWorkspace>) {
    if ws.pointer_over_ui {
        return;
    }
    let y = floor_y(ws.floor_level) + 0.05;
    if ws.tool == EditorTool::PlaceModule {
        let half = shared::editor_map::CELLS_PER_MODULE as f32 * KENNEY_CELL * 0.5;
        let grid = ws.grid();
        let (cx, cz) = snap_to_module_slot(state.hover_world.x, state.hover_world.y, &grid);
        let color = Color::srgb(0.55, 0.45, 1.0);
        let x0 = cx - half;
        let z0 = cz - half;
        let x1 = cx + half;
        let z1 = cz + half;
        gizmos.line(Vec3::new(x0, y, z0), Vec3::new(x1, y, z0), color);
        gizmos.line(Vec3::new(x1, y, z0), Vec3::new(x1, y, z1), color);
        gizmos.line(Vec3::new(x1, y, z1), Vec3::new(x0, y, z1), color);
        gizmos.line(Vec3::new(x0, y, z1), Vec3::new(x0, y, z0), color);
        return;
    }
    if ws.tool != EditorTool::PlaceGlb {
        return;
    }
    let stem = current_stem(&state);
    let (nx, nz) = kenney_catalog::piece_grid_size(stem);
    let (wx, wz) = rotated_grid_size(nx, nz, state.yaw);
    let sw = state.cell_sw;
    let y = state.snap.y + 0.05;
    let color = Color::srgb(0.2, 0.95, 0.45);
    let x0 = sw.x;
    let z0 = sw.y;
    let x1 = sw.x + wx * KENNEY_CELL;
    let z1 = sw.y + wz * KENNEY_CELL;
    gizmos.line(Vec3::new(x0, y, z0), Vec3::new(x1, y, z0), color);
    gizmos.line(Vec3::new(x1, y, z0), Vec3::new(x1, y, z1), color);
    gizmos.line(Vec3::new(x1, y, z1), Vec3::new(x0, y, z1), color);
    gizmos.line(Vec3::new(x0, y, z1), Vec3::new(x0, y, z0), color);
}

/// Keep the `ceiling` flag on placed pieces in sync with the loaded map so the
/// double-sided ceiling material is applied. No geometry change: a `template-floor`
/// already has a textured downward face, so it reads as a ceiling from below as-is.
fn sync_ceiling_piece_transforms(
    mut commands: Commands,
    ws: Res<EditorWorkspace>,
    mut placed: Query<(Entity, &mut EditorPlaced, &mut KenneyModule, &GlobalTransform)>,
) {
    const EPS: f32 = 0.05;
    for (entity, mut ep, mut module, gt) in &mut placed {
        let px = gt.translation().x;
        let pz = gt.translation().z;
        // Floor and ceiling slabs share stem + (x, z) on hub/landing levels — pick the
        // record that matches this entity's role, not whichever appears first in JSON.
        let want_ceiling = ep.ceiling || module.ceiling;
        let Some(p) = ws.map.pieces.iter().find(|p| {
            p.floor_level == ep.floor_level
                && p.ceiling == want_ceiling
                && (p.x - px).abs() < EPS
                && (p.z - pz).abs() < EPS
                && stem_static(&p.stem) == module.name
        }) else {
            continue;
        };
        if ep.ceiling != p.ceiling || module.ceiling != p.ceiling {
            ep.ceiling = p.ceiling;
            module.ceiling = p.ceiling;
            commands.entity(entity).remove::<EditorModuleReady>();
        }
    }
}

fn editor_apply_materials(
    mut commands: Commands,
    ws: Res<EditorWorkspace>,
    playtest: Option<Res<EditorPlaytestActive>>,
    cyber: Option<Res<CyberMaterial>>,
    cyber_ceiling: Option<Res<CyberMaterialCeiling>>,
    pink_ceiling: Option<Res<CyberMaterialPinkCeiling>>,
    cyber_industrial: Option<Res<CyberMaterialIndustrial>>,
    cyber_lasers: Option<Res<CyberLaserMaterial>>,
    priesthood: Option<Res<PriesthoodMaterial>>,
    synth: Option<Res<SynthMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    modules: Query<
        (
            Entity,
            &KenneyModule,
            &GlobalTransform,
            &EditorPlaced,
            Option<&EditorGhost>,
            Option<&PieceTint>,
        ),
        Without<EditorModuleReady>,
    >,
    children_q: Query<&Children>,
    mesh_q: Query<(&Mesh3d, &GlobalTransform)>,
) {
    let ghost_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.2, 0.95, 0.45, 0.32),
        emissive: LinearRgba::rgb(0.15, 0.55, 0.25),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    for (root, module, root_gt, placed, ghost, _tint) in &modules {
        let want_ceiling = placed.ceiling || module.ceiling;
        let px = root_gt.translation().x;
        let pz = root_gt.translation().z;
        let py = root_gt.translation().y;
        let pieces = match placed.owner {
            PieceOwner::Dressing => &ws.dressing.pieces,
            PieceOwner::Module => &ws.module.pieces,
            PieceOwner::Map => &ws.map.pieces,
        };
        let matched = pieces
            .iter()
            .filter(|p| {
                p.floor_level == placed.floor_level
                    && p.ceiling == want_ceiling
                    && (p.x - px).abs() < 0.12
                    && (p.z - pz).abs() < 0.12
                    && stem_static(&p.stem) == module.name
            })
            .min_by(|a, b| {
                let da = (a.y.unwrap_or(0.0) - py).abs();
                let db = (b.y.unwrap_or(0.0) - py).abs();
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .filter(|p| (p.y.unwrap_or(0.0) - py).abs() < 0.15);
        let zone = matched.and_then(|p| p.zone.as_deref());
        let kit = module
            .kit
            .as_deref()
            .or_else(|| matched.and_then(|p| p.kit.as_deref()));
        let slot = if ghost.is_some() {
            None
        } else {
            Some(kenney_material_slot(
                kit,
                zone,
                playtest.is_some(),
                want_ceiling,
                module.name,
            ))
        };
        let mesh_ents: Vec<Entity> = children_q
            .iter_descendants(root)
            .filter(|e| mesh_q.contains(*e))
            .collect();
        if mesh_ents.is_empty() {
            continue;
        }
        if mesh_ents.iter().any(|e| {
            let (m, _) = mesh_q.get(*e).unwrap();
            meshes.get(&m.0).is_none()
        }) {
            continue;
        }

        let piece_yaw = matched
            .map(|p| quantize_yaw(p.yaw))
            .unwrap_or_else(|| root_gt.rotation().to_euler(EulerRot::YXZ).0);
        let mesh_cutouts = if want_ceiling || placed.owner == PieceOwner::Dressing {
            kenney_pit::KenneyMeshCutouts::default()
        } else {
            kenney_pit::mesh_cutouts_for_piece(
                module.name,
                placed.floor_level,
                px,
                pz,
                piece_yaw,
                ws.map.extraction_xz.map(|[ex, ez]| Vec2::new(ex, ez)),
                ws.map.floors.get(&placed.floor_level),
                placed.ceiling,
            )
        };

        for e in &mesh_ents {
            let (mesh3d, gt) = mesh_q.get(*e).unwrap();
            let mesh_handle = if !want_ceiling && !mesh_cutouts.is_empty() {
                if let Some(mesh) = meshes.get(&mesh3d.0).cloned() {
                    meshes.add(cut_kenney_mesh(&mesh, gt, &mesh_cutouts))
                } else {
                    mesh3d.0.clone()
                }
            } else {
                mesh3d.0.clone()
            };
            if mesh_handle != mesh3d.0 {
                commands.entity(*e).insert(Mesh3d(mesh_handle));
            }
        }

        let mat = if ghost.is_some() {
            Some(ghost_mat.clone())
        } else if let Some(slot) = slot {
            match slot {
                KenneyMaterialSlot::NativeGlb => {
                    // Keep embedded GLB materials — removing MeshMaterial3d leaves meshes bare.
                    commands.entity(root).insert(EditorModuleReady);
                    continue;
                }
                KenneyMaterialSlot::Priesthood => {
                    let Some(priesthood) = priesthood.as_ref() else { continue };
                    Some(priesthood.0.clone())
                }
                KenneyMaterialSlot::SynthDeck => {
                    let Some(synth) = synth.as_ref() else { continue };
                    Some(synth.deck.clone())
                }
                KenneyMaterialSlot::SynthFloor => {
                    let Some(synth) = synth.as_ref() else { continue };
                    Some(synth.floor.clone())
                }
                KenneyMaterialSlot::SynthRail => {
                    let Some(synth) = synth.as_ref() else { continue };
                    Some(synth.rail.clone())
                }
                KenneyMaterialSlot::SynthProp => {
                    let Some(synth) = synth.as_ref() else { continue };
                    Some(synth.prop.clone())
                }
                KenneyMaterialSlot::Synth => {
                    let Some(synth) = synth.as_ref() else { continue };
                    Some(synth.base.clone())
                }
                KenneyMaterialSlot::Lasers => {
                    let Some(cyber_lasers) = cyber_lasers.as_ref() else { continue };
                    Some(cyber_lasers.0.clone())
                }
                KenneyMaterialSlot::Ceiling => {
                    let Some(cyber_ceiling) = cyber_ceiling.as_ref() else { continue };
                    Some(cyber_ceiling.0.clone())
                }
                KenneyMaterialSlot::CeilingPink => {
                    let Some(pink_ceiling) = pink_ceiling.as_ref() else { continue };
                    Some(pink_ceiling.0.clone())
                }
                KenneyMaterialSlot::SpaceIndustrial => {
                    let Some(cyber_industrial) = cyber_industrial.as_ref() else { continue };
                    Some(cyber_industrial.0.clone())
                }
                KenneyMaterialSlot::SpaceCyber => {
                    let Some(cyber) = cyber.as_ref() else { continue };
                    Some(cyber.0.clone())
                }
            }
        } else {
            continue;
        };

        if let Some(mat) = mat {
            for e in &mesh_ents {
                commands.entity(*e).insert(MeshMaterial3d(mat.clone()));
            }
        }
        commands.entity(root).insert(EditorModuleReady);
    }
}

fn update_save_toast(
    time: Res<Time>,
    fb: Res<SaveFeedback>,
    mut text: Query<(&mut Text, &mut TextColor), With<SaveToastText>>,
) {
    let Ok((mut t, mut color)) = text.single_mut() else {
        return;
    };
    if fb.message.is_empty() || time.elapsed_secs() > fb.hide_after {
        **t = String::new();
        return;
    }
    **t = fb.message.clone();
    *color = if fb.ok {
        TextColor(Color::srgb(0.35, 1.0, 0.5))
    } else {
        TextColor(Color::srgb(1.0, 0.35, 0.35))
    };
}

fn select_drag_system(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    ws: ResMut<EditorWorkspace>,
    sel: ResMut<EditorSelection>,
    history: ResMut<EditorHistory>,
    state: Res<EditorState>,
    placed: Query<(Entity, &mut Transform, &KenneyModule, &mut EditorPlaced)>,
) {
    select_drag_input(mouse, keys, ws, sel, history, placed, state.hover_world);
}

fn discard_all_content(
    commands: &mut Commands,
    ws: &mut EditorWorkspace,
    state: &mut EditorState,
    history: &mut EditorHistory,
    placed: &Query<(Entity, &EditorPlaced)>,
) {
    for (e, _) in placed {
        commands.entity(e).despawn();
    }
    ws.map = shared::editor_map::MapDocument::new_default();
    let pool = ws.module.pool.clone();
    ws.module = shared::editor_map::ModuleDocument::new_named("untitled", &pool);
    ws.dressing = shared::editor_map::DressingDocument::new_default();
    ws.active.path = None;
    ws.active.kind = match ws.workflow {
        EditorWorkflow::MapMaker => ActiveDocKind::Map,
        EditorWorkflow::ModuleMaker => ActiveDocKind::Module,
        EditorWorkflow::SynthDressing => ActiveDocKind::Dressing,
    };
    state.next_id = 1;
    history.clear();
    ws.floor_dirty = true;
    ws.dirty = false;
    ws.refocus_camera = true;
    ws.sidebar_dirty = true;
}

fn spawn_editor_sun(mut commands: Commands, editor: Option<Res<EditorMode>>) {
    if editor.is_none() {
        return;
    }
    // Single overhead fill — ambient-style light from above for piece readability.
    commands.spawn((
        EditorSunLight,
        DirectionalLight {
            color: Color::srgb(0.92, 0.94, 0.98),
            illuminance: 28_000.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    ));
}

fn maintain_editor_lighting(
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
    mut cam: Query<&mut Exposure, Or<(With<EditorCamera>, With<crate::editor_playtest::EditorPlaytestCamera>)>>,
) {
    ambient.color = Color::srgb(0.68, 0.72, 0.78);
    ambient.brightness = 2_200.0;
    clear.0 = Color::srgb(0.36, 0.40, 0.46);
    for mut exposure in &mut cam {
        exposure.ev100 = 9.0;
    }
}

#[derive(Resource, Default)]
struct SkipPlaytestExit(bool);

fn editor_playtest_enter(
    commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut ws: ResMut<EditorWorkspace>,
    mut save_fb: ResMut<SaveFeedback>,
    time: Res<Time>,
    placed: Query<(Entity, &Transform, &KenneyModule, &EditorPlaced)>,
    editor_cam_ent: Query<Entity, Or<(With<EditorCamera>, With<crate::editor_playtest::EditorPlaytestCamera>)>>,
    menu: Query<Entity, With<EditorMenuRoot>>,
    sidebar: Query<Entity, With<EditorSidebarRoot>>,
    ghosts: Query<Entity, With<EditorGhost>>,
    toast: Query<Entity, With<SaveToastText>>,
    floors: Query<Entity, With<FloorSlab>>,
    test_mode: ResMut<TestMode>,
    generation: ResMut<KenneyPlaytestGeneration>,
    window: Single<&mut CursorOptions, With<PrimaryWindow>>,
    mut skip_exit: ResMut<SkipPlaytestExit>,
) {
    if !keys.just_pressed(KeyCode::KeyG) {
        return;
    }
    if !quicksave(&mut ws, &placed, &mut save_fb, &time) {
        return;
    }
    ws.close_menus();
    let module_entities: Vec<Entity> = placed.iter().map(|(e, ..)| e).collect();
    enter_in_process_playtest(
        commands,
        editor_cam_ent,
        menu,
        sidebar,
        ghosts,
        toast,
        floors,
        module_entities,
        test_mode,
        generation,
        window,
    );
    skip_exit.0 = true;
    save_fb.message = format!("{} — in-process playtest (G return)", save_fb.message);
}

fn editor_playtest_exit(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut skip_exit: ResMut<SkipPlaytestExit>,
    mut ws: ResMut<EditorWorkspace>,
    asset_server: Res<AssetServer>,
    mut state: ResMut<EditorState>,
    prefs: Res<UserEditorPrefs>,
    cam: Res<EditorCam>,
    test_mode: ResMut<TestMode>,
    generation: ResMut<KenneyPlaytestGeneration>,
    window: Single<&mut CursorOptions, With<PrimaryWindow>>,
    player_vis: Query<&mut Visibility, With<crate::netplay::OwnPlayer>>,
    playtest_cam: Query<Entity, With<crate::editor_playtest::EditorPlaytestCamera>>,
    coords_hud: Query<Entity, With<crate::editor_playtest::PlaytestCoordsHud>>,
    map_placed: Query<(Entity, &EditorPlaced)>,
) {
    if !keys.just_pressed(KeyCode::KeyG) {
        return;
    }
    if skip_exit.0 {
        skip_exit.0 = false;
        return;
    }
    exit_in_process_playtest(
        &mut commands,
        test_mode,
        generation,
        window,
        player_vis,
        playtest_cam,
        coords_hud,
    );
    for (e, _) in &map_placed {
        commands.entity(e).remove::<EditorModuleReady>();
    }

    let dressing = is_dressing_workflow(&ws);
    let owner = if dressing {
        PieceOwner::Dressing
    } else {
        PieceOwner::Map
    };
    let pieces = if dressing {
        ws.dressing.pieces.clone()
    } else {
        ws.map.pieces.clone()
    };

    // The in-process playtest reuses and mutates/despawns the editor's placed-piece
    // entities (repositioning, mesh cutouts, hatch hiding). Rebuild from the saved
    // document so the editor returns to its exact saved state.
    for (e, ep) in &map_placed {
        if ep.owner == owner {
            commands.entity(e).despawn();
        }
    }
    for p in &pieces {
        let id = state.next_id;
        spawn_piece_record_pub(
            &mut commands,
            &asset_server,
            p,
            owner,
            id,
            if dressing { None } else { ws.map.extraction_xz },
        );
        state.next_id += 1;
    }

    ws.floor_dirty = true;
    ws.spawn_marker_dirty = true;
    ws.sidebar_dirty = true;
    restore_editor_shell(
        &mut commands,
        &asset_server,
        &state,
        &ws,
        &prefs,
        &cam,
        "Returned to editor",
    );
    info!("returned to editor from in-process playtest");
}

/// Hides spawn markers during playtest; shows them in the normal editor.
fn sync_spawn_marker_visibility(
    playtest: Option<Res<EditorPlaytestActive>>,
    mut markers: Query<&mut Visibility, With<SpawnMarker>>,
) {
    let vis = if playtest.is_some() {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    };
    for mut v in &mut markers {
        *v = vis;
    }
}

/// Spawns or replaces the character model marker at the editor spawn point.
pub fn sync_spawn_marker(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut ws: ResMut<EditorWorkspace>,
    existing: Query<Entity, With<SpawnMarker>>,
) {
    if !ws.spawn_marker_dirty {
        return;
    }
    ws.spawn_marker_dirty = false;

    for e in &existing {
        commands.entity(e).despawn();
    }

    let (spawn_xz, marker_y) = if is_dressing_workflow(&ws) {
        (
            ws.dressing
                .spawn_xz
                .or_else(|| Some(shared::editor_map::DressingDocument::default_spawn_xz())),
            ws.dressing.spawn_y.unwrap_or(SYNTH_DECK_Y),
        )
    } else {
        (ws.map.spawn_xz, floor_y(0))
    };

    let Some([sx, sz]) = spawn_xz else {
        return;
    };

    commands.spawn((
        SpawnMarker,
        SceneRoot(asset_server.load("models/Knight.glb#Scene0")),
        Transform::from_xyz(sx, marker_y, sz).with_scale(Vec3::splat(1.0)),
    ));
}
