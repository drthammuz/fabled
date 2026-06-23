//! Select / move / rotate / scale tools.

use bevy::prelude::*;
use shared::editor_map::EditorTool;
use shared::kenney_catalog::{self, quantize_yaw, rotated_grid_size, sw_from_placement, KENNEY_CELL};

use crate::editor_history::{snapshot_from_entity, EditorHistory, HistoryOp};
use crate::editor_state::floor_y;
use crate::editor_workspace::EditorWorkspace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceOwner {
    Map,
    Module,
    Dressing,
}

#[derive(Component, Clone, Copy)]
pub struct EditorPlaced {
    pub piece_id: u32,
    pub floor_level: i32,
    pub sw_x: f32,
    pub sw_z: f32,
    pub owner: PieceOwner,
    pub group_id: Option<u32>,
    pub ceiling: bool,
    pub underside: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PieceSnapshot {
    pub piece_id: u32,
    pub stem: String,
    pub x: f32,
    pub z: f32,
    pub yaw: f32,
    pub scale: f32,
    pub floor_level: i32,
    pub owner: PieceOwner,
    pub group_id: Option<u32>,
    pub sw_x: f32,
    pub sw_z: f32,
    pub y: Option<f32>,
    pub kit: Option<String>,
}

#[derive(Resource, Default)]
pub struct EditorSelection {
    pub selected: Vec<u32>,
    pub drag_active: bool,
    pub box_select: bool,
    pub drag_cursor_start: Vec2,
    pub drag_piece_start: Vec3,
    pub drag_before: Vec<PieceSnapshot>,
    pub drag_group: bool,
    pub box_start: Vec2,
    pub box_end: Vec2,
}

use crate::test_showcase::KenneyModule;

pub fn primary_selected(sel: &EditorSelection) -> Option<u32> {
    sel.selected.first().copied()
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum PickLayer {
    Floor = 0,
    Wall = 1,
    Prop = 2,
}

fn pick_layer(stem: &str) -> PickLayer {
    if stem.starts_with("floor") || stem.starts_with("balcony-floor") {
        PickLayer::Floor
    } else if stem.starts_with("wall") || stem.starts_with("balcony-rail") {
        PickLayer::Wall
    } else {
        PickLayer::Prop
    }
}

/// Half-extents in local X/Z (metres) for cursor hit-testing at piece centre.
fn piece_half_extents_xz(stem: &str, scale: f32) -> (f32, f32) {
    if let Some(ext) = shared::synth_placement::half_extents_xz(stem, scale) {
        return ext;
    }
    let sc = scale.abs().max(0.01);
    if let Some(p) = kenney_catalog::piece(stem) {
        return (p.footprint_m.x * 0.5 * sc, p.footprint_m.z * 0.5 * sc);
    }
    if stem.starts_with("floor") {
        (2.0 * sc, 2.0 * sc)
    } else if stem.starts_with("balcony-floor") || stem.starts_with("balcony-rail") {
        (2.0 * sc, 1.4 * sc)
    } else if stem.starts_with("wall") {
        (2.0 * sc, 0.65 * sc)
    } else if stem.starts_with("balcony") {
        (1.4 * sc, 1.4 * sc)
    } else if stem.starts_with("table") {
        (1.2 * sc, 1.2 * sc)
    } else if stem.starts_with("chair") {
        (0.9 * sc, 0.85 * sc)
    } else if stem.starts_with("bed-double") {
        (2.0 * sc, 2.0 * sc)
    } else if stem.starts_with("bed") {
        (1.0 * sc, 2.0 * sc)
    } else if stem.starts_with("container") {
        (1.2 * sc, 1.0 * sc)
    } else if stem.starts_with("display-wall") {
        (1.5 * sc, 0.4 * sc)
    } else {
        match stem {
            "computer-wide" | "computer-system" => (1.8 * sc, 1.2 * sc),
            "computer-screen" | "computer" => (0.85 * sc, 0.9 * sc),
            "table-large" => (2.8 * sc, 1.8 * sc),
            "table-display" | "table-display-small" | "table-display-planet" => (1.6 * sc, 1.2 * sc),
            _ => (1.0 * sc, 1.0 * sc),
        }
    }
}

fn piece_contains_world_xz(world_x: f32, world_z: f32, tf: &Transform, stem: &str) -> bool {
    let (hx, hz) = piece_half_extents_xz(stem, tf.scale.x);
    let dx = world_x - tf.translation.x;
    let dz = world_z - tf.translation.z;
    let yaw = tf.rotation.to_euler(EulerRot::YXZ).0;
    let cos = yaw.cos();
    let sin = yaw.sin();
    let lx = dx * cos + dz * sin;
    let lz = -dx * sin + dz * cos;
    lx.abs() <= hx + 0.08 && lz.abs() <= hz + 0.08
}

fn pick_best_at<'a>(
    world_x: f32,
    world_z: f32,
    floor_level: i32,
    owner: PieceOwner,
    pieces: impl Iterator<Item = (u32, &'a Transform, &'a str, &'a EditorPlaced)>,
) -> Option<u32> {
    let mut best: Option<(u32, PickLayer, f32, f32)> = None;

    for (id, tf, stem, ep) in pieces {
        if ep.floor_level != floor_level || ep.owner != owner {
            continue;
        }
        if !piece_contains_world_xz(world_x, world_z, tf, stem) {
            continue;
        }
        let layer = pick_layer(stem);
        let dx = tf.translation.x - world_x;
        let dz = tf.translation.z - world_z;
        let dist_sq = dx * dx + dz * dz;
        let elev = tf.translation.y;
        let better = match best {
            None => true,
            Some((_, bl, bd, be)) => {
                layer > bl
                    || (layer == bl
                        && (dist_sq < bd - 0.001
                            || ((dist_sq - bd).abs() < 0.001 && elev > be + 0.01)))
            }
        };
        if better {
            best = Some((id, layer, dist_sq, elev));
        }
    }
    best.map(|(id, _, _, _)| id)
}

pub fn pick_piece_at(
    hover: Vec2,
    floor_level: i32,
    owner: PieceOwner,
    placed: &Query<(Entity, &Transform, &KenneyModule, &EditorPlaced)>,
) -> Option<u32> {
    pick_best_at(
        hover.x,
        hover.y,
        floor_level,
        owner,
        placed
            .iter()
            .map(|(_, tf, km, ep)| (ep.piece_id, tf, km.name, ep)),
    )
}

pub fn pick_piece_at_mut(
    hover: Vec2,
    floor_level: i32,
    owner: PieceOwner,
    placed: &Query<(Entity, &mut Transform, &KenneyModule, &mut EditorPlaced)>,
) -> Option<u32> {
    pick_best_at(
        hover.x,
        hover.y,
        floor_level,
        owner,
        placed
            .iter()
            .map(|(_, tf, km, ep)| (ep.piece_id, &*tf, km.name, ep)),
    )
}

pub fn pick_pieces_in_rect(
    rect_min: Vec2,
    rect_max: Vec2,
    floor_level: i32,
    owner: PieceOwner,
    placed: &Query<(Entity, &mut Transform, &KenneyModule, &mut EditorPlaced)>,
) -> Vec<u32> {
    let x0 = rect_min.x.min(rect_max.x);
    let x1 = rect_min.x.max(rect_max.x);
    let z0 = rect_min.y.min(rect_max.y);
    let z1 = rect_min.y.max(rect_max.y);
    let mut out = Vec::new();
    for (_, tf, km, ep) in placed.iter() {
        if ep.floor_level != floor_level || ep.owner != owner {
            continue;
        }
        let yaw = tf.rotation.to_euler(EulerRot::YXZ).0;
        let (nx, nz) = kenney_catalog::piece_grid_size(km.name);
        let (wx, wz) = rotated_grid_size(nx, nz, yaw);
        let px0 = ep.sw_x;
        let pz0 = ep.sw_z;
        let px1 = px0 + wx * KENNEY_CELL * tf.scale.x;
        let pz1 = pz0 + wz * KENNEY_CELL * tf.scale.z;
        if px1 >= x0 && px0 <= x1 && pz1 >= z0 && pz0 <= z1 {
            out.push(ep.piece_id);
        }
    }
    out
}

pub fn piece_covers_cell(hover_sw: Vec2, stem: &str, yaw: f32, anchor_sw: Vec2) -> bool {
    let (nx, nz) = kenney_catalog::piece_grid_size(stem);
    let (wx, wz) = rotated_grid_size(nx, nz, yaw);
    hover_sw.x >= anchor_sw.x - 0.01
        && hover_sw.x < anchor_sw.x + wx * KENNEY_CELL - 0.01
        && hover_sw.y >= anchor_sw.y - 0.01
        && hover_sw.y < anchor_sw.y + wz * KENNEY_CELL - 0.01
}

pub fn collect_snapshots(
    ids: &[u32],
    group: bool,
    placed: &Query<(Entity, &mut Transform, &KenneyModule, &mut EditorPlaced)>,
) -> Vec<PieceSnapshot> {
    if group {
        if let Some(id) = ids.first() {
            return collect_snapshots_for_group(*id, placed);
        }
        return Vec::new();
    }
    ids.iter()
        .flat_map(|id| snapshot_for_id(*id, placed))
        .collect()
}

fn collect_snapshots_for_group(
    id: u32,
    placed: &Query<(Entity, &mut Transform, &KenneyModule, &mut EditorPlaced)>,
) -> Vec<PieceSnapshot> {
    let group_id = placed
        .iter()
        .find(|(_, _, _, ep)| ep.piece_id == id)
        .and_then(|(_, _, _, ep)| ep.group_id);
    placed
        .iter()
        .filter(|(_, _, _, ep)| ep.group_id.is_some() && ep.group_id == group_id)
        .map(|(_, tf, km, ep)| snapshot_from_entity(ep.piece_id, km.name, tf, ep, km))
        .collect()
}

fn snapshot_for_id(
    id: u32,
    placed: &Query<(Entity, &mut Transform, &KenneyModule, &mut EditorPlaced)>,
) -> Option<PieceSnapshot> {
    placed
        .iter()
        .find(|(_, _, _, ep)| ep.piece_id == id)
        .map(|(_, tf, km, ep)| snapshot_from_entity(ep.piece_id, km.name, tf, ep, km))
}

fn push_transform_if_changed(
    history: &mut EditorHistory,
    before: Vec<PieceSnapshot>,
    after: Vec<PieceSnapshot>,
) {
    if before == after {
        return;
    }
    history.push(HistoryOp::Transform { before, after });
}

fn update_sw(ep: &mut EditorPlaced, tf: &Transform, stem: &str) {
    let yaw = tf.rotation.to_euler(EulerRot::YXZ).0;
    let (sw_x, sw_z) = sw_from_placement(tf.translation, stem, yaw);
    ep.sw_x = sw_x;
    ep.sw_z = sw_z;
}

pub fn select_tool_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut ws: ResMut<EditorWorkspace>,
    mut sel: ResMut<EditorSelection>,
    mut history: ResMut<EditorHistory>,
    mut placed: Query<(Entity, &mut Transform, &KenneyModule, &mut EditorPlaced)>,
) {
    if ws.tool != EditorTool::Select {
        sel.drag_active = false;
        sel.box_select = false;
        return;
    }

    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if sel.selected.is_empty() {
        return;
    }

    if keys.just_pressed(KeyCode::KeyQ) {
        let before = collect_snapshots(&sel.selected, shift, &placed);
        for id in sel.selected.clone() {
            rotate_ids(&[id], std::f32::consts::FRAC_PI_2, shift, &mut placed);
        }
        let after = collect_snapshots(&sel.selected, shift, &placed);
        push_transform_if_changed(&mut history, before, after);
        ws.dirty = true;
    }
    if keys.just_pressed(KeyCode::KeyE) {
        let before = collect_snapshots(&sel.selected, shift, &placed);
        for id in sel.selected.clone() {
            rotate_ids(&[id], -std::f32::consts::FRAC_PI_2, shift, &mut placed);
        }
        let after = collect_snapshots(&sel.selected, shift, &placed);
        push_transform_if_changed(&mut history, before, after);
        ws.dirty = true;
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        let before = collect_snapshots(&sel.selected, shift, &placed);
        scale_ids(&sel.selected, 0.9, shift, &mut placed);
        let after = collect_snapshots(&sel.selected, shift, &placed);
        push_transform_if_changed(&mut history, before, after);
        ws.dirty = true;
    }
    if keys.just_pressed(KeyCode::BracketRight) {
        let before = collect_snapshots(&sel.selected, shift, &placed);
        scale_ids(&sel.selected, 1.1, shift, &mut placed);
        let after = collect_snapshots(&sel.selected, shift, &placed);
        push_transform_if_changed(&mut history, before, after);
        ws.dirty = true;
    }

    let nudge = nudge_delta(&keys);
    if nudge != Vec2::ZERO {
        let before = collect_snapshots(&sel.selected, shift, &placed);
        for id in sel.selected.clone() {
            move_ids(&[id], nudge, shift, &mut placed);
        }
        let after = collect_snapshots(&sel.selected, shift, &placed);
        push_transform_if_changed(&mut history, before, after);
        ws.dirty = true;
    }
}

fn nudge_delta(keys: &ButtonInput<KeyCode>) -> Vec2 {
    let mut d = Vec2::ZERO;
    if keys.just_pressed(KeyCode::ArrowLeft) {
        d.x -= 1.0;
    }
    if keys.just_pressed(KeyCode::ArrowRight) {
        d.x += 1.0;
    }
    if keys.just_pressed(KeyCode::ArrowUp) {
        d.y -= 1.0;
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        d.y += 1.0;
    }
    if d == Vec2::ZERO {
        return d;
    }
    d * KENNEY_CELL * 0.5
}

fn ids_to_move(ids: &[u32], group: bool, placed: &Query<(Entity, &mut Transform, &KenneyModule, &mut EditorPlaced)>) -> Vec<u32> {
    if !group {
        return ids.to_vec();
    }
    let mut out = Vec::new();
    for id in ids {
        let Some(snapshots) = snapshot_for_id(*id, placed) else {
            continue;
        };
        let group_id = snapshots.group_id;
        for (_, _, _, ep) in placed.iter() {
            if ep.group_id.is_some() && ep.group_id == group_id && !out.contains(&ep.piece_id) {
                out.push(ep.piece_id);
            }
        }
    }
    out
}

fn for_each_id(
    ids: &[u32],
    group: bool,
    placed: &mut Query<(Entity, &mut Transform, &KenneyModule, &mut EditorPlaced)>,
    mut f: impl FnMut(&mut Transform, &mut EditorPlaced, &KenneyModule),
) {
    let targets = ids_to_move(ids, group, placed);
    for (_, mut tf, km, mut ep) in placed.iter_mut() {
        if targets.contains(&ep.piece_id) {
            f(&mut tf, &mut ep, km);
        }
    }
}

fn rotate_ids(
    ids: &[u32],
    delta: f32,
    group: bool,
    placed: &mut Query<(Entity, &mut Transform, &KenneyModule, &mut EditorPlaced)>,
) {
    for_each_id(ids, group, placed, |tf, ep, km| {
        let yaw = quantize_yaw(tf.rotation.to_euler(EulerRot::YXZ).0 + delta);
        tf.rotation = Quat::from_rotation_y(yaw);
        update_sw(ep, tf, km.name);
    });
}

fn scale_ids(
    ids: &[u32],
    factor: f32,
    group: bool,
    placed: &mut Query<(Entity, &mut Transform, &KenneyModule, &mut EditorPlaced)>,
) {
    for_each_id(ids, group, placed, |tf, ep, km| {
        let s = (tf.scale.x * factor).clamp(0.25, 4.0);
        tf.scale = Vec3::splat(s);
        update_sw(ep, tf, km.name);
    });
}

fn move_ids(
    ids: &[u32],
    delta: Vec2,
    group: bool,
    placed: &mut Query<(Entity, &mut Transform, &KenneyModule, &mut EditorPlaced)>,
) {
    for_each_id(ids, group, placed, |tf, ep, km| {
        tf.translation.x += delta.x;
        tf.translation.z += delta.y;
        update_sw(ep, tf, km.name);
    });
}

pub fn select_drag_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut ws: ResMut<EditorWorkspace>,
    mut sel: ResMut<EditorSelection>,
    mut history: ResMut<EditorHistory>,
    mut placed: Query<(Entity, &mut Transform, &KenneyModule, &mut EditorPlaced)>,
    hover: Vec2,
) {
    if ws.tool != EditorTool::Select {
        return;
    }
    let owner = match ws.workflow {
        shared::editor_map::EditorWorkflow::MapMaker => PieceOwner::Map,
        shared::editor_map::EditorWorkflow::ModuleMaker => PieceOwner::Module,
        shared::editor_map::EditorWorkflow::SynthDressing => PieceOwner::Dressing,
    };
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

    if mouse.just_pressed(MouseButton::Left) {
        if let Some(id) = pick_piece_at_mut(hover, ws.floor_level, owner, &placed) {
            sel.selected = vec![id];
            sel.box_select = false;
            sel.drag_group = shift;
            sel.drag_before = collect_snapshots(&sel.selected, shift, &placed);
            if let Some((_, tf, _, _)) = placed.iter().find(|(_, _, _, ep)| ep.piece_id == id) {
                sel.drag_active = true;
                sel.drag_cursor_start = hover;
                sel.drag_piece_start = tf.translation;
            }
        } else {
            sel.selected.clear();
            sel.box_select = true;
            sel.box_start = hover;
            sel.box_end = hover;
            sel.drag_active = false;
        }
    }

    if sel.box_select && mouse.pressed(MouseButton::Left) {
        sel.box_end = hover;
    }

    if sel.drag_active && mouse.pressed(MouseButton::Left) {
        let delta = hover - sel.drag_cursor_start;
        let targets = ids_to_move(&sel.selected, sel.drag_group, &placed);
        for (_, mut tf, km, mut ep) in placed.iter_mut() {
            if !targets.contains(&ep.piece_id) {
                continue;
            }
            tf.translation.x = sel.drag_piece_start.x + delta.x;
            tf.translation.z = sel.drag_piece_start.z + delta.y;
            update_sw(&mut ep, &tf, km.name);
        }
    }

    if mouse.just_released(MouseButton::Left) {
        if sel.box_select {
            sel.selected = pick_pieces_in_rect(
                sel.box_start,
                sel.box_end,
                ws.floor_level,
                owner,
                &placed,
            );
            sel.box_select = false;
        }
        if sel.drag_active {
            let after = collect_snapshots(&sel.selected, sel.drag_group, &placed);
            push_transform_if_changed(&mut history, sel.drag_before.clone(), after);
            ws.dirty = true;
        }
        sel.drag_active = false;
        sel.drag_before.clear();
    }
}

pub fn draw_selection_gizmo(
    mut gizmos: Gizmos,
    sel: Res<EditorSelection>,
    ws: Res<EditorWorkspace>,
    placed: Query<(&Transform, &KenneyModule, &EditorPlaced)>,
) {
    if sel.box_select || sel.box_start != sel.box_end {
        let y = floor_y(ws.floor_level) + 0.06;
        let color = Color::srgba(0.4, 0.75, 1.0, 0.85);
        let x0 = sel.box_start.x.min(sel.box_end.x);
        let x1 = sel.box_start.x.max(sel.box_end.x);
        let z0 = sel.box_start.y.min(sel.box_end.y);
        let z1 = sel.box_start.y.max(sel.box_end.y);
        gizmos.line(Vec3::new(x0, y, z0), Vec3::new(x1, y, z0), color);
        gizmos.line(Vec3::new(x1, y, z0), Vec3::new(x1, y, z1), color);
        gizmos.line(Vec3::new(x1, y, z1), Vec3::new(x0, y, z1), color);
        gizmos.line(Vec3::new(x0, y, z1), Vec3::new(x0, y, z0), color);
    }

    for id in &sel.selected {
        for (tf, km, ep) in &placed {
            if ep.piece_id != *id || ep.floor_level != ws.floor_level {
                continue;
            }
            let yaw = tf.rotation.to_euler(EulerRot::YXZ).0;
            let (nx, nz) = kenney_catalog::piece_grid_size(km.name);
            let (wx, wz) = rotated_grid_size(nx, nz, yaw);
            let y = floor_y(ep.floor_level) + 0.1;
            let glow = Color::srgba(0.35, 0.85, 1.0, 0.55);
            let edge = Color::srgba(0.5, 0.95, 1.0, 0.9);
            let x0 = ep.sw_x;
            let z0 = ep.sw_z;
            let x1 = x0 + wx * KENNEY_CELL * tf.scale.x;
            let z1 = z0 + wz * KENNEY_CELL * tf.scale.z;
            let y_inner = y + 0.02;
            gizmos.line(Vec3::new(x0, y_inner, z0), Vec3::new(x1, y_inner, z0), glow);
            gizmos.line(Vec3::new(x1, y_inner, z0), Vec3::new(x1, y_inner, z1), glow);
            gizmos.line(Vec3::new(x1, y_inner, z1), Vec3::new(x0, y_inner, z1), glow);
            gizmos.line(Vec3::new(x0, y_inner, z1), Vec3::new(x0, y_inner, z0), glow);
            gizmos.line(Vec3::new(x0, y, z0), Vec3::new(x1, y, z0), edge);
            gizmos.line(Vec3::new(x1, y, z0), Vec3::new(x1, y, z1), edge);
            gizmos.line(Vec3::new(x1, y, z1), Vec3::new(x0, y, z1), edge);
            gizmos.line(Vec3::new(x0, y, z1), Vec3::new(x0, y, z0), edge);
        }
    }
}
