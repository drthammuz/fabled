//! Undo / redo for editor operations.

use bevy::prelude::*;
use shared::editor_map::{EditorWorkflow, FloorMask, PieceRecord};

use crate::editor_selection::{PieceOwner, PieceSnapshot};

#[derive(Clone, Debug)]
pub enum HistoryOp {
    Place(Vec<PieceSnapshot>),
    Delete(Vec<PieceSnapshot>),
    Transform {
        before: Vec<PieceSnapshot>,
        after: Vec<PieceSnapshot>,
    },
    FloorMask {
        workflow: EditorWorkflow,
        level: i32,
        before: FloorMask,
        after: FloorMask,
    },
}

#[derive(Resource)]
pub struct EditorHistory {
    undo: Vec<HistoryOp>,
    redo: Vec<HistoryOp>,
    max_depth: usize,
}

impl Default for EditorHistory {
    fn default() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            max_depth: 64,
        }
    }
}

impl EditorHistory {
    pub fn push(&mut self, op: HistoryOp) {
        self.undo.push(op);
        if self.undo.len() > self.max_depth {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn take_undo(&mut self) -> Option<HistoryOp> {
        let op = self.undo.pop()?;
        self.redo.push(op.clone());
        Some(op)
    }

    pub fn take_redo(&mut self) -> Option<HistoryOp> {
        let op = self.redo.pop()?;
        self.undo.push(op.clone());
        Some(op)
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

pub fn snapshot_from_entity(
    piece_id: u32,
    stem: &str,
    tf: &Transform,
    ep: &crate::editor_selection::EditorPlaced,
) -> PieceSnapshot {
    PieceSnapshot {
        piece_id,
        stem: stem.to_string(),
        x: tf.translation.x,
        z: tf.translation.z,
        yaw: tf.rotation.to_euler(EulerRot::YXZ).0,
        scale: tf.scale.x,
        floor_level: ep.floor_level,
        owner: ep.owner,
        group_id: ep.group_id,
        sw_x: ep.sw_x,
        sw_z: ep.sw_z,
    }
}

pub fn snapshot_from_record(id: u32, owner: PieceOwner, record: &PieceRecord) -> PieceSnapshot {
    PieceSnapshot {
        piece_id: id,
        stem: record.stem.clone(),
        x: record.x,
        z: record.z,
        yaw: record.yaw,
        scale: record.scale,
        floor_level: record.floor_level,
        owner,
        group_id: record.group_id,
        sw_x: record.x,
        sw_z: record.z,
    }
}

pub fn record_from_snapshot(s: &PieceSnapshot) -> PieceRecord {
    PieceRecord {
        stem: s.stem.clone(),
        x: s.x,
        z: s.z,
        yaw: s.yaw,
        scale: s.scale,
        floor_level: s.floor_level,
        group_id: s.group_id,
        ceiling: false,
        underside: false,
        kit: None,
        tint: None,
        tags: vec![],
    }
}

pub fn undo_redo_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut history: ResMut<EditorHistory>,
    mut pending: ResMut<PendingHistoryApply>,
) {
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    if !ctrl {
        return;
    }
    if keys.just_pressed(KeyCode::KeyZ) {
        if let Some(op) = history.take_undo() {
            pending.apply = Some(invert_op(&op));
        }
    } else if keys.just_pressed(KeyCode::KeyY) {
        if let Some(op) = history.take_redo() {
            pending.apply = Some(op);
        }
    }
}

fn invert_op(op: &HistoryOp) -> HistoryOp {
    match op {
        HistoryOp::Place(snaps) => HistoryOp::Delete(snaps.clone()),
        HistoryOp::Delete(snaps) => HistoryOp::Place(snaps.clone()),
        HistoryOp::Transform { before, after } => HistoryOp::Transform {
            before: after.clone(),
            after: before.clone(),
        },
        HistoryOp::FloorMask {
            workflow,
            level,
            before,
            after,
        } => HistoryOp::FloorMask {
            workflow: *workflow,
            level: *level,
            before: after.clone(),
            after: before.clone(),
        },
    }
}

#[derive(Resource, Default)]
pub struct PendingHistoryApply {
    pub apply: Option<HistoryOp>,
}

#[derive(Resource, Default)]
pub struct FloorPaintSession {
    pub active: bool,
    pub before: Option<(EditorWorkflow, i32, FloorMask)>,
}
