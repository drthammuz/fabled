//! History apply helper.

use bevy::prelude::*;
use shared::editor_map::{EditorWorkflow, PieceRecord};
use shared::kenney_catalog::{quantize_yaw, sw_from_placement};

use crate::editor_history::{record_from_snapshot, HistoryOp, PendingHistoryApply};
use crate::editor_selection::{EditorPlaced, PieceOwner, PieceSnapshot};
use crate::editor_state::{floor_y, EditorState};
use crate::editor_workspace::EditorWorkspace;
use crate::kenney_editor::spawn_piece_record_pub;
use crate::test_showcase::KenneyModule;

pub fn apply_pending_history(
    mut commands: Commands,
    mut pending: ResMut<PendingHistoryApply>,
    asset_server: Res<AssetServer>,
    mut state: ResMut<EditorState>,
    mut ws: ResMut<EditorWorkspace>,
    mut placed: Query<(Entity, &mut Transform, &KenneyModule, &mut EditorPlaced)>,
) {
    let Some(op) = pending.apply.take() else {
        return;
    };
    match op {
        HistoryOp::Place(snaps) => {
            for s in &snaps {
                let record = record_from_snapshot(s);
                spawn_piece_record_pub(
                    &mut commands,
                    &asset_server,
                    &record,
                    s.owner,
                    s.piece_id,
                    ws.map.extraction_xz,
                );
                add_to_document(&mut ws, s.owner, record);
                state.next_id = state.next_id.max(s.piece_id + 1);
            }
            ws.dirty = true;
        }
        HistoryOp::Delete(snaps) => {
            let group_ids: std::collections::HashSet<u32> = snaps
                .iter()
                .filter_map(|s| s.group_id)
                .collect();
            for (e, _, _, ep) in &placed {
                let hit = snaps.iter().any(|s| s.piece_id == ep.piece_id)
                    || ep
                        .group_id
                        .is_some_and(|gid| group_ids.contains(&gid));
                if hit {
                    commands.entity(e).despawn();
                }
            }
            let mut groups_cleared = std::collections::HashSet::new();
            for s in &snaps {
                if let Some(gid) = s.group_id {
                    if groups_cleared.insert(gid) {
                        remove_group_from_document(&mut ws, s.owner, gid);
                    }
                } else {
                    remove_piece_from_document(&mut ws, s);
                }
            }
            ws.dirty = true;
        }
        HistoryOp::Transform { after, .. } => {
            for snap in after {
                for (e, mut tf, km, mut ep) in placed.iter_mut() {
                    if ep.piece_id != snap.piece_id {
                        continue;
                    }
                    tf.translation = Vec3::new(snap.x, floor_y(snap.floor_level), snap.z);
                    tf.rotation = Quat::from_rotation_y(quantize_yaw(snap.yaw));
                    tf.scale = Vec3::splat(snap.scale);
                    let (sw_x, sw_z) = sw_from_placement(tf.translation, km.name, snap.yaw);
                    ep.sw_x = sw_x;
                    ep.sw_z = sw_z;
                    let _ = e;
                    break;
                }
            }
            ws.dirty = true;
        }
        HistoryOp::FloorMask {
            workflow,
            level,
            after,
            ..
        } => {
            match workflow {
                EditorWorkflow::MapMaker => {
                    ws.map.floors.insert(level, after);
                }
                EditorWorkflow::ModuleMaker => {
                    *ws.module.floor_mask_for_mut(level) = after;
                }
            }
            ws.floor_dirty = true;
            ws.dirty = true;
        }
    }
}

fn document_pieces(ws: &mut EditorWorkspace, owner: PieceOwner) -> &mut Vec<PieceRecord> {
    match owner {
        PieceOwner::Map => &mut ws.map.pieces,
        PieceOwner::Module => &mut ws.module.pieces,
    }
}

fn add_to_document(ws: &mut EditorWorkspace, owner: PieceOwner, record: PieceRecord) {
    document_pieces(ws, owner).push(record);
}

pub fn remove_piece_from_document(ws: &mut EditorWorkspace, s: &PieceSnapshot) {
    let pieces = document_pieces(ws, s.owner);
    if let Some(i) = pieces.iter().position(|p| {
        p.stem == s.stem
            && (p.x - s.x).abs() < 0.05
            && (p.z - s.z).abs() < 0.05
            && p.floor_level == s.floor_level
    }) {
        pieces.remove(i);
    }
}

pub fn remove_group_from_document(ws: &mut EditorWorkspace, owner: PieceOwner, group_id: u32) {
    document_pieces(ws, owner).retain(|p| p.group_id != Some(group_id));
}
