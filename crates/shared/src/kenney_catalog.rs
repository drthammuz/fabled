//! Kenney Modular Space Kit catalogue — loaded from `kenney_catalog.json`.
//!
//! Regenerate: `python tools/generate_kenney_catalog.py`

use std::sync::OnceLock;

use bevy::prelude::Vec3;
use serde::Deserialize;

const CATALOG_JSON: &str = include_str!("../../../assets/models/space/kenney_catalog.json");

static CATALOG: OnceLock<KenneyCatalog> = OnceLock::new();

pub fn catalog() -> &'static KenneyCatalog {
    CATALOG.get_or_init(|| {
        serde_json::from_str(CATALOG_JSON).expect("assets/models/space/kenney_catalog.json must be valid")
    })
}

pub fn piece(stem: &str) -> Option<&'static KenneyPiece> {
    catalog().pieces.iter().find(|p| p.stem == stem)
}

#[derive(Debug, Deserialize)]
pub struct KenneyCatalog {
    pub version: u32,
    pub grid_unit_m: f32,
    pub opening_w_m: f32,
    pub opening_h_m: f32,
    pub ceiling_m: f32,
    #[serde(default)]
    pub floor_plane_y: f32,
    pub slot_l_m: f32,
    pub slot_c_m: f32,
    pub slot_r_m: f32,
    pub module_m: f32,
    pub pieces: Vec<KenneyPiece>,
}

#[derive(Debug, Deserialize)]
pub struct KenneyPiece {
    pub stem: String,
    pub file: String,
    pub category: String,
    pub role: String,
    pub footprint_m: Footprint,
    pub grid_units: GridUnits,
    pub bounds: Bounds,
    pub mesh_extent_m: MeshExtent,
    pub collide_default: bool,
    pub cell_grid: CellGrid,
    #[serde(default)]
    pub open_faces: Vec<String>,
    #[serde(default)]
    pub open_slots: Vec<String>,
    pub stairs: Option<StairsSpec>,
    pub variant_of: Option<String>,
    pub purpose: String,
}

#[derive(Debug, Deserialize)]
pub struct CellGrid {
    pub origin: String,
    pub axis: CellAxis,
    pub units_m: f32,
    pub floor_plane_y: f32,
    pub nx: u32,
    pub nz: u32,
    /// `[south → north][west → east]` — `floor` = y≈0 hit + clearance
    pub cells: Vec<Vec<String>>,
    pub edges: CellEdges,
    pub confidence: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Deserialize)]
pub struct CellAxis {
    pub x: String,
    pub z: String,
}

#[derive(Debug, Deserialize)]
pub struct CellEdges {
    pub south: Vec<String>,
    pub north: Vec<String>,
    pub west: Vec<String>,
    pub east: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Footprint {
    pub x: f32,
    pub z: f32,
}

#[derive(Debug, Deserialize)]
pub struct GridUnits {
    pub x: f32,
    pub z: f32,
}

#[derive(Debug, Deserialize)]
pub struct Bounds {
    pub x_min: f32,
    pub x_max: f32,
    pub z_min: f32,
    pub z_max: f32,
    pub y_min: f32,
    pub y_max: f32,
}

#[derive(Debug, Deserialize)]
pub struct MeshExtent {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Debug, Deserialize)]
pub struct StairsSpec {
    pub entry_z: f32,
    pub landing_z: f32,
    pub rise_m: f32,
    pub width_m: f32,
}

impl KenneyPiece {
    pub fn stem_str(&self) -> &str {
        &self.stem
    }
}

/// One Kenney grid cell in metres (matches `kenney_catalog.json` `grid_unit_m`).
pub const KENNEY_CELL: f32 = 4.0;

/// Largest Kenney kit module footprint (5×5 cells). Editor sandbox matches this.
pub const KENNEY_MOD_CELLS: u32 = 5;
pub const KENNEY_MOD_M: f32 = KENNEY_MOD_CELLS as f32 * KENNEY_CELL;

/// Snap rotation to 0°, 90°, 180°, or 270°.
pub fn quantize_yaw(yaw: f32) -> f32 {
    let deg = (yaw.to_degrees() / 90.0).round() * 90.0;
    deg.rem_euclid(360.0).to_radians()
}

pub fn piece_grid_size(stem: &str) -> (u32, u32) {
    piece(stem)
        .map(|p| (p.cell_grid.nx, p.cell_grid.nz))
        .unwrap_or((1, 1))
}

/// Footprint in cells after yaw (only 90° steps).
pub fn rotated_grid_size(nx: u32, nz: u32, yaw: f32) -> (f32, f32) {
    let steps = (quantize_yaw(yaw) / std::f32::consts::FRAC_PI_2).round() as i32;
    if steps.rem_euclid(2) != 0 {
        (nz as f32, nx as f32)
    } else {
        (nx as f32, nz as f32)
    }
}

/// South-west anchor so the hovered cell is the footprint centre (when possible).
pub fn anchor_sw_centered(hover_sw_x: f32, hover_sw_z: f32, stem: &str, yaw: f32) -> (f32, f32) {
    let yaw = quantize_yaw(yaw);
    let (nx, nz) = piece_grid_size(stem);
    let (wx, wz) = rotated_grid_size(nx, nz, yaw);
    let sw_x = hover_sw_x - (wx - 1.0) * KENNEY_CELL * 0.5;
    let sw_z = hover_sw_z - (wz - 1.0) * KENNEY_CELL * 0.5;
    (sw_x, sw_z)
}

/// Walls / doors sit on the south-west cell edge (grid-aligned), not centred.
pub fn uses_edge_anchor(stem: &str) -> bool {
    piece(stem).is_some_and(|p| {
        matches!(
            p.category.as_str(),
            "template_wall" | "gate" | "template"
        )
    })
}

/// Snap hover position to grid before computing anchor.
pub fn snap_hover(hit_x: f32, hit_z: f32, map_x0: f32, map_z0: f32, mode: crate::editor_map::SnapMode) -> (f32, f32) {
    match mode {
        crate::editor_map::SnapMode::FullCell => cell_sw_corner(hit_x, hit_z, map_x0, map_z0),
        crate::editor_map::SnapMode::HalfCell => {
            const H: f32 = 2.0;
            let sw_x = ((hit_x - map_x0) / H).floor() * H + map_x0;
            let sw_z = ((hit_z - map_z0) / H).floor() * H + map_z0;
            (sw_x, sw_z)
        }
        crate::editor_map::SnapMode::Free => (hit_x, hit_z),
    }
}

pub fn placement_for_hover(
    hover_x: f32,
    hover_z: f32,
    stem: &str,
    yaw: f32,
    y: f32,
    map_x0: f32,
    map_z0: f32,
    snap: crate::editor_map::SnapMode,
) -> (Vec3, f32, f32, f32) {
    let yaw = quantize_yaw(yaw);
    let (hover_sw_x, hover_sw_z) = snap_hover(hover_x, hover_z, map_x0, map_z0, snap);
    let (sw_x, sw_z) = if uses_edge_anchor(stem) || snap == crate::editor_map::SnapMode::Free {
        (hover_sw_x, hover_sw_z)
    } else {
        anchor_sw_centered(hover_sw_x, hover_sw_z, stem, yaw)
    };
    let (pos, yaw) = placement_at_sw(sw_x, sw_z, stem, yaw, y);
    (pos, yaw, sw_x, sw_z)
}

/// South-west corner of the grid cell under a world-space floor hit.
pub fn cell_sw_corner(hit_x: f32, hit_z: f32, map_x0: f32, map_z0: f32) -> (f32, f32) {
    let sw_x = ((hit_x - map_x0) / KENNEY_CELL).floor() * KENNEY_CELL + map_x0;
    let sw_z = ((hit_z - map_z0) / KENNEY_CELL).floor() * KENNEY_CELL + map_z0;
    (sw_x, sw_z)
}

/// Model origin for a piece whose south-west anchor cell is `(sw_x, sw_z)`.
pub fn placement_at_sw(sw_x: f32, sw_z: f32, stem: &str, yaw: f32, y: f32) -> (Vec3, f32) {
    let yaw = quantize_yaw(yaw);
    let (nx, nz) = piece_grid_size(stem);
    let (wx, wz) = rotated_grid_size(nx, nz, yaw);
    let cx = sw_x + wx * KENNEY_CELL * 0.5;
    let cz = sw_z + wz * KENNEY_CELL * 0.5;
    (Vec3::new(cx, y, cz), yaw)
}

/// South-west anchor recovered from a placed piece centre.
pub fn sw_from_placement(pos: Vec3, stem: &str, yaw: f32) -> (f32, f32) {
    let (nx, nz) = piece_grid_size(stem);
    let (wx, wz) = rotated_grid_size(nx, nz, yaw);
    (
        pos.x - wx * KENNEY_CELL * 0.5,
        pos.z - wz * KENNEY_CELL * 0.5,
    )
}
