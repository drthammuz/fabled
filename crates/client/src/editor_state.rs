//! Placement picker state (filtered GLB list, yaw, snap).

use bevy::prelude::*;
use shared::editor_catalog;
use shared::editor_map::{MapDocument, SnapMode};
use shared::kenney_catalog::{placement_for_hover, KENNEY_CELL};

#[derive(Resource)]
pub struct EditorState {
    pub piece_index: usize,
    pub yaw: f32,
    /// South-west anchor for placement / gizmo.
    pub cell_sw: Vec2,
    pub snap: Vec3,
    pub stems: Vec<String>,
    pub next_id: u32,
    /// Raw cursor hit on floor (module placement center).
    pub hover_world: Vec2,
}

impl Default for EditorState {
    fn default() -> Self {
        let stems = editor_catalog::filtered_stems(&Default::default());
        let grid = MapDocument::new_default().grid();
        let (cx, cz) = grid.center_xz();
        let stem = stems.first().map(|s| s.as_str()).unwrap_or("corridor");
        let (snap, yaw, sw_x, sw_z) = placement_for_hover(
            cx,
            cz,
            stem,
            0.0,
            floor_y(0),
            grid.world_x0(),
            grid.world_z0(),
            SnapMode::FullCell,
        );
        Self {
            piece_index: 0,
            yaw,
            cell_sw: Vec2::new(sw_x, sw_z),
            snap,
            stems,
            next_id: 1,
            hover_world: Vec2::new(cx, cz),
        }
    }
}

pub fn floor_y(floor: i32) -> f32 {
    // Keep visual slabs a few mm below y=0 so their top surface doesn't
    // z-fight with GLB floor geometry that sits at exactly y=0.
    floor as f32 * shared::level::MOD_H - 0.005
}

pub fn current_stem(state: &EditorState) -> &str {
    state
        .stems
        .get(state.piece_index)
        .map(|s| s.as_str())
        .unwrap_or("corridor")
}

pub fn module_footprint_sw(hover_x: f32, hover_z: f32) -> (f32, f32) {
    let half = (shared::editor_map::CELLS_PER_MODULE as f32 * KENNEY_CELL) * 0.5;
    (hover_x - half, hover_z - half)
}
