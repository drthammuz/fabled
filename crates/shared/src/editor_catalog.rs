//! GLB picker filters — Kenney subcategories + misc assets.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::kenney_catalog;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FilterGroup {
    KenneyRooms,
    KenneyCorridors,
    KenneyWalls,
    KenneyFloors,
    KenneyDoors,
    KenneyStairs,
    KenneyMisc,
    OtherMisc,
}

impl FilterGroup {
    pub const ALL: [FilterGroup; 8] = [
        FilterGroup::KenneyRooms,
        FilterGroup::KenneyCorridors,
        FilterGroup::KenneyWalls,
        FilterGroup::KenneyFloors,
        FilterGroup::KenneyDoors,
        FilterGroup::KenneyStairs,
        FilterGroup::KenneyMisc,
        FilterGroup::OtherMisc,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FilterGroup::KenneyRooms => "Rooms",
            FilterGroup::KenneyCorridors => "Corridors",
            FilterGroup::KenneyWalls => "Walls",
            FilterGroup::KenneyFloors => "Floors",
            FilterGroup::KenneyDoors => "Doors/Gates",
            FilterGroup::KenneyStairs => "Stairs",
            FilterGroup::KenneyMisc => "Kenney misc",
            FilterGroup::OtherMisc => "Other GLBs",
        }
    }
}

#[derive(Clone, Debug)]
pub struct PieceFilters {
    pub enabled: [bool; 8],
}

impl Default for PieceFilters {
    fn default() -> Self {
        Self {
            enabled: [true; 8],
        }
    }
}

impl PieceFilters {
    pub fn set(&mut self, group: FilterGroup, on: bool) {
        self.enabled[group_index(group)] = on;
    }

    pub fn get(&self, group: FilterGroup) -> bool {
        self.enabled[group_index(group)]
    }

    pub fn set_all(&mut self, on: bool) {
        self.enabled = [on; 8];
    }

    pub fn toggle(&mut self, group: FilterGroup) {
        let i = group_index(group);
        self.enabled[i] = !self.enabled[i];
    }
}

fn group_index(g: FilterGroup) -> usize {
    match g {
        FilterGroup::KenneyRooms => 0,
        FilterGroup::KenneyCorridors => 1,
        FilterGroup::KenneyWalls => 2,
        FilterGroup::KenneyFloors => 3,
        FilterGroup::KenneyDoors => 4,
        FilterGroup::KenneyStairs => 5,
        FilterGroup::KenneyMisc => 6,
        FilterGroup::OtherMisc => 7,
    }
}

pub fn classify_kenney(category: &str) -> FilterGroup {
    match category {
        "room" => FilterGroup::KenneyRooms,
        "corridor" | "corridor_wide" => FilterGroup::KenneyCorridors,
        "template_wall" => FilterGroup::KenneyWalls,
        "template_floor" => FilterGroup::KenneyFloors,
        "gate" => FilterGroup::KenneyDoors,
        "stairs" => FilterGroup::KenneyStairs,
        _ => FilterGroup::KenneyMisc,
    }
}

/// All placeable stems after applying filters.
pub fn filtered_stems(filters: &PieceFilters) -> Vec<String> {
    let mut out = Vec::new();
    for piece in kenney_catalog::catalog().pieces.iter() {
        let group = classify_kenney(&piece.category);
        if filters.get(group) {
            out.push(piece.stem.clone());
        }
    }
    if filters.get(FilterGroup::OtherMisc) {
        for stem in misc_stems() {
            if !out.iter().any(|s| s == &stem) {
                out.push(stem);
            }
        }
    }
    out.sort();
    out
}

/// Non-Kenney GLBs under `assets/models/misc/` (excludes huge city scene).
pub fn misc_stems() -> Vec<String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/models/misc");
    let mut stems = Vec::new();
    let Ok(read) = std::fs::read_dir(&root) else {
        return stems;
    };
    for ent in read.flatten() {
        let path = ent.path();
        if path.extension().and_then(|e| e.to_str()) != Some("glb") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if stem == "cyberpunk_city" {
            continue;
        }
        stems.push(stem.to_string());
    }
    stems.sort();
    stems
}

/// Asset path for a stem (Kenney or misc) in the default `space` kit.
pub fn glb_asset_path(stem: &str) -> String {
    glb_asset_path_in_kit(stem, "space")
}

/// Asset path for a stem in a specific Kenney kit folder.
pub fn glb_asset_path_in_kit(stem: &str, kit: &str) -> String {
    if kit == "space" && kenney_catalog::piece(stem).is_some() {
        format!("models/space/{stem}.glb")
    } else {
        format!("models/{kit}/{stem}.glb")
    }
}

/// Resolve GLB path from a placed piece record.
pub fn glb_asset_path_for_piece(p: &crate::editor_map::PieceRecord) -> String {
    glb_asset_path_in_kit(&p.stem, p.kit.as_deref().unwrap_or("space"))
}

/// Metadata for one entry in a `gen_index.json` gallery file.
#[derive(Clone, Debug, serde::Deserialize, Default)]
pub struct GalleryMeta {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub score: f32,
    #[serde(default)]
    pub entrances: usize,
    #[serde(default)]
    pub sides: Vec<String>,
    #[serde(default)]
    pub strategy: String,
    #[serde(default)]
    pub total_pieces: usize,
}

/// Load `gen_index.json` for a pool.  Returns an empty vec if the file is absent.
pub fn load_gallery_index(pool: &str) -> Vec<GalleryMeta> {
    let path = crate::editor_map::pool_dir(pool).join("gen_index.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<GalleryMeta>>(&text).unwrap_or_default()
}

/// All pools that have a `gen_index.json` (generated pools).
pub fn list_gallery_pools() -> Vec<String> {
    list_pools()
        .into_iter()
        .filter(|p| crate::editor_map::pool_dir(p).join("gen_index.json").exists())
        .collect()
}

/// Metadata files that live in pool directories but are not loadable modules.
const POOL_METADATA_STEMS: &[&str] = &["gen_index", "gallery_ratings"];

pub fn list_modules_in_pool(pool: &str) -> Vec<(String, PathBuf)> {
    let dir = crate::editor_map::pool_dir(pool);
    let mut out = Vec::new();
    let Ok(read) = std::fs::read_dir(&dir) else {
        return out;
    };
    for ent in read.flatten() {
        let path = ent.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        if POOL_METADATA_STEMS.contains(&name.as_str()) {
            continue;
        }
        out.push((name, path));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

pub fn list_pools() -> Vec<String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../userinput/modules");
    let mut pools = vec!["default".into()];
    let Ok(read) = std::fs::read_dir(&root) else {
        return pools;
    };
    for ent in read.flatten() {
        if ent.path().is_dir() {
            if let Some(name) = ent.file_name().to_str() {
                if !pools.iter().any(|p| p == name) {
                    pools.push(name.to_string());
                }
            }
        }
    }
    pools.sort();
    pools
}

/// Category swatch color for sidebar thumbnails.
pub fn stem_swatch_color(stem: &str) -> (f32, f32, f32) {
    let group = kenney_catalog::piece(stem)
        .map(|p| classify_kenney(&p.category))
        .unwrap_or(FilterGroup::OtherMisc);
    match group {
        FilterGroup::KenneyRooms => (0.35, 0.55, 0.85),
        FilterGroup::KenneyCorridors => (0.45, 0.72, 0.55),
        FilterGroup::KenneyWalls => (0.72, 0.55, 0.38),
        FilterGroup::KenneyFloors => (0.55, 0.55, 0.58),
        FilterGroup::KenneyDoors => (0.85, 0.45, 0.35),
        FilterGroup::KenneyStairs => (0.65, 0.48, 0.78),
        FilterGroup::KenneyMisc => (0.42, 0.68, 0.78),
        FilterGroup::OtherMisc => (0.78, 0.62, 0.32),
    }
}

pub fn cycle_pool(current: &str) -> String {
    let pools = list_pools();
    let i = pools.iter().position(|p| p == current).unwrap_or(0);
    pools[(i + 1) % pools.len()].clone()
}

pub fn suggest_module_name(pool: &str) -> String {
    let existing: HashSet<String> = list_modules_in_pool(pool)
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    for i in 1..9999 {
        let name = format!("module_{i:02}");
        if !existing.contains(&name) {
            return name;
        }
    }
    "module_new".into()
}

/// Kenney synth kit folder for dressing / procgen decor.
pub const SYNTH_KIT: &str = "factions/synth";
/// Uniform scale for space_station-derived synth pieces (1-unit GLBs → 4 m cells).
pub const SYNTH_DRESSING_SCALE: f32 = 4.0;
/// Walkable deck height for elevated synth interiors.
pub const SYNTH_DECK_Y: f32 = 1.2;

/// Sidebar filter for the dressing shell piece list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DressingCatFilter {
    #[default]
    All,
    Structure,
    WallSwap,
    WallDecal,
    FloorProp,
    Balcony,
}

impl DressingCatFilter {
    pub const SELECTABLE: [DressingCatFilter; 6] = [
        DressingCatFilter::All,
        DressingCatFilter::Structure,
        DressingCatFilter::WallSwap,
        DressingCatFilter::WallDecal,
        DressingCatFilter::FloorProp,
        DressingCatFilter::Balcony,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DressingCatFilter::All => "All",
            DressingCatFilter::Structure => "Structure",
            DressingCatFilter::WallSwap => "Walls",
            DressingCatFilter::WallDecal => "Decals",
            DressingCatFilter::FloorProp => "Furniture",
            DressingCatFilter::Balcony => "Balcony",
        }
    }
}

const SYNTH_CAT_STRUCTURE: &[&str] = &[
    "floor", "floor-corner", "floor-detail", "floor-panel", "floor-panel-corner",
    "floor-panel-end", "floor-panel-straight", "wall", "wall-corner", "wall-corner-round",
    "wall-door", "wall-door-banner", "wall-door-center", "wall-door-edge",
    "wall-door-edge-banner", "wall-door-wide", "wall-door-wide-banner",
];

const SYNTH_CAT_WALL_SWAP: &[&str] = &[
    "wall-banner", "wall-pillar", "wall-pillar-banner", "wall-corner-banner",
    "wall-corner-round-banner", "wall-window", "wall-window-banner", "wall-window-frame",
    "wall-window-shutters",
];

const SYNTH_CAT_WALL_DECAL: &[&str] = &[
    "display-wall", "display-wall-wide", "wall-detail", "wall-switch",
];

/// Synth `wall` GLB half-thickness @ scale 4 (0.3 u × 4 m/u ÷ 2).
const SYNTH_WALL_HALF_THICK_M: f32 = 0.6;

pub fn is_synth_wall_decal(stem: &str) -> bool {
    SYNTH_CAT_WALL_DECAL.contains(&stem)
}

/// Straight synth wall pieces (not decals) — edge-mounted on a 4 m floor cell face.
pub fn is_synth_structure_wall(stem: &str) -> bool {
    stem.starts_with("wall") && !is_synth_wall_decal(stem)
}

pub fn is_synth_balcony_stem(stem: &str) -> bool {
    stem.starts_with("balcony-") || stem == "rail" || stem == "rail-narrow"
}

/// Floor / balcony deck — should win depth over coplanar rails (dressing z-fight).
pub fn is_synth_deck_occluder_stem(stem: &str) -> bool {
    stem.starts_with("floor") || stem.starts_with("balcony-floor")
}

/// Substrate blocks keep embedded GLB materials (floor + stair flights).
pub fn is_synth_substrate_glb_stem(stem: &str) -> bool {
    matches!(
        stem,
        "floor"
            | "floor-corner"
            | "floor-detail"
            | "floor-panel"
            | "floor-panel-corner"
            | "floor-panel-end"
            | "floor-panel-straight"
    ) || stem.starts_with("stairs")
}

/// Props that sit on the 1.2 m deck and must render above floor tiles.
pub fn is_synth_deck_prop_stem(stem: &str) -> bool {
    matches!(
        stem,
        "bed-single"
            | "bed-single-cover"
            | "bed-double"
            | "bed-double-cover"
            | "chair"
            | "chair-armrest"
            | "chair-armrest-headrest"
            | "chair-cushion"
            | "chair-cushion-headrest"
            | "chair-headrest"
            | "computer"
            | "computer-screen"
            | "computer-system"
            | "computer-wide"
            | "container"
            | "container-flat"
            | "container-flat-open"
            | "container-tall"
            | "container-wide"
            | "table"
            | "table-display"
            | "table-display-planet"
            | "table-display-small"
            | "table-inset"
            | "table-inset-small"
            | "table-large"
    )
}

/// Railing with no walkable mesh — may overlap deck at the same XZ.
pub fn is_synth_rail_stem(stem: &str) -> bool {
    stem.starts_with("balcony-rail") || stem == "rail" || stem == "rail-narrow"
}

/// Balcony tiles sit one cell **outside** the wall line (2× the wall face offset).
pub fn synth_balcony_outward_offset(yaw: f32) -> bevy::prelude::Vec3 {
    synth_wall_face_offset(yaw) * 2.0
}

pub fn is_synth_chair_stem(stem: &str) -> bool {
    stem.starts_with("chair")
}

/// Yaw so a prop's front faces world direction `(dx, dz)` (see `placement_catalog.json`).
pub fn synth_face_yaw(stem: &str, dx: f32, dz: f32) -> f32 {
    crate::synth_placement::face_yaw(stem, dx, dz)
}

/// Wall origin when the hovered point is the **floor cell centre** (4 m snap).
/// Matches procgen ``add_wall`` / ``_cell_face_pose`` (CELL × 0.5) and hand-placed synth2 layouts.
pub fn synth_wall_face_offset(yaw: f32) -> bevy::prelude::Vec3 {
    use bevy::prelude::Vec3;
    use std::f32::consts::{FRAC_PI_2, PI};

    let yaw = crate::kenney_catalog::quantize_yaw(yaw);
    const H: f32 = crate::kenney_catalog::KENNEY_CELL * 0.5;
    if yaw.abs() < 0.01 {
        Vec3::new(0.0, 0.0, -H)
    } else if (yaw - PI).abs() < 0.01 {
        Vec3::new(0.0, 0.0, H)
    } else if (yaw - FRAC_PI_2).abs() < 0.01 {
        Vec3::new(H, 0.0, 0.0)
    } else if (yaw - PI - FRAC_PI_2).abs() < 0.01 || (yaw + FRAC_PI_2).abs() < 0.01 {
        Vec3::new(-H, 0.0, 0.0)
    } else {
        Vec3::ZERO
    }
}

fn synth_wall_decal_depth_half_m(stem: &str) -> f32 {
    match stem {
        "display-wall" | "display-wall-wide" => 0.765,
        "wall-detail" => 0.52,
        "wall-switch" => 0.05,
        _ => 0.5,
    }
}

/// Offset from wall-cell centre so a decal sits on the room-facing side (Kenney +Z @ yaw).
pub fn synth_wall_decal_mount_offset(stem: &str, yaw: f32) -> bevy::prelude::Vec3 {
    use bevy::prelude::{Quat, Vec3};
    if !is_synth_wall_decal(stem) {
        return Vec3::ZERO;
    }
    let yaw = crate::kenney_catalog::quantize_yaw(yaw);
    let dist = SYNTH_WALL_HALF_THICK_M + synth_wall_decal_depth_half_m(stem);
    Quat::from_rotation_y(yaw) * Vec3::new(0.0, 0.0, dist)
}

/// Head / back edge sits on the snap point; offset moves the piece origin into the room.
pub fn synth_back_anchor_offset(stem: &str, yaw: f32) -> Option<bevy::prelude::Vec3> {
    crate::synth_placement::back_anchor_offset(stem, yaw)
}

const SYNTH_CAT_FLOOR_PROP: &[&str] = &[
    "bed-single", "bed-single-cover", "bed-double", "bed-double-cover", "chair",
    "chair-armrest", "chair-armrest-headrest", "chair-cushion", "chair-cushion-headrest",
    "chair-headrest", "computer", "computer-screen", "computer-system", "computer-wide",
    "container", "container-flat", "container-flat-open", "container-tall", "container-wide",
    "table", "table-display", "table-display-planet", "table-display-small", "table-inset",
    "table-inset-small", "table-large",
];

const SYNTH_CAT_BALCONY: &[&str] = &[
    "balcony-floor", "balcony-floor-center", "balcony-floor-corner", "balcony-rail",
    "balcony-rail-center", "balcony-rail-corner", "rail", "rail-narrow",
];

fn synth_stems_for_category(cat: DressingCatFilter) -> &'static [&'static str] {
    match cat {
        DressingCatFilter::All => &[],
        DressingCatFilter::Structure => SYNTH_CAT_STRUCTURE,
        DressingCatFilter::WallSwap => SYNTH_CAT_WALL_SWAP,
        DressingCatFilter::WallDecal => SYNTH_CAT_WALL_DECAL,
        DressingCatFilter::FloorProp => SYNTH_CAT_FLOOR_PROP,
        DressingCatFilter::Balcony => SYNTH_CAT_BALCONY,
    }
}

/// Stems shown in editor **Dressing** mode sidebar (see `assets/models/factions/synth/catalogue.md`).
pub fn synth_dressing_stems() -> Vec<String> {
    DressingCatFilter::SELECTABLE
        .iter()
        .filter(|&&c| c != DressingCatFilter::All)
        .flat_map(|&c| synth_stems_for_category(c).iter().copied())
        .map(str::to_string)
        .collect()
}

pub fn synth_dressing_stems_filtered(cat: DressingCatFilter) -> Vec<String> {
    if cat == DressingCatFilter::All {
        return synth_dressing_stems();
    }
    synth_stems_for_category(cat)
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

/// How a synth GLB origin lines up with the 1.2 m walkable deck (scale 4).
#[derive(Clone, Copy, Debug, PartialEq)]
enum SynthDressingPlacement {
    /// Full 1.2 m block: origin at ground, walkable top at deck height.
    SubstrateBlock,
    /// Thin tile: mesh top flush with deck (`y = deck - mesh_height`).
    DeckTopFlush(f32),
    /// Prop / wall / rail: origin bottom on deck surface.
    DeckBase,
}

fn synth_dressing_placement(stem: &str) -> SynthDressingPlacement {
    match stem {
        "floor" | "floor-corner" | "floor-detail" | "floor-panel" | "floor-panel-corner"
        | "floor-panel-end" | "floor-panel-straight" => SynthDressingPlacement::SubstrateBlock,
        "stairs" | "stairs-ramp" | "stairs-corner" | "stairs-corner-inner" => {
            SynthDressingPlacement::SubstrateBlock
        }
        "balcony-floor" | "balcony-floor-center" | "balcony-floor-corner" => {
            SynthDressingPlacement::DeckTopFlush(0.6)
        }
        _ => SynthDressingPlacement::DeckBase,
    }
}

/// Default absolute Y for a dressing placement (mesh-aware @ scale 4).
pub fn synth_dressing_default_y(stem: &str) -> Option<f32> {
    Some(match synth_dressing_placement(stem) {
        SynthDressingPlacement::SubstrateBlock => 0.0,
        SynthDressingPlacement::DeckTopFlush(h) => SYNTH_DECK_Y - h,
        SynthDressingPlacement::DeckBase => SYNTH_DECK_Y,
    })
}

/// Stem default Y plus optional 1.2 m steps (dressing editor Q/E).
pub fn synth_dressing_placement_y(stem: &str, extra_deck_steps: i32) -> f32 {
    synth_dressing_default_y(stem).unwrap_or(SYNTH_DECK_Y)
        + extra_deck_steps as f32 * SYNTH_DECK_Y
}

/// Backfill kit / scale / deck height on dressing vignette pieces (legacy saves).
pub fn normalize_synth_dressing_piece(p: &mut crate::editor_map::PieceRecord) {
    if p.kit.is_none() {
        p.kit = Some(SYNTH_KIT.to_string());
    }
    if p.scale < 0.01 {
        p.scale = SYNTH_DRESSING_SCALE;
    }
    if p.y.is_none() {
        p.y = synth_dressing_default_y(&p.stem);
    }
}

pub fn suggest_dressing_name() -> String {
    let existing: std::collections::HashSet<String> = crate::editor_map::list_dressing_files()
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    for base in ["bunk", "lab", "mess", "storage", "command", "corridor_alcove", "vignette"] {
        if !existing.contains(base) {
            return base.into();
        }
    }
    for i in 1..9999 {
        let name = format!("vignette_{i}");
        if !existing.contains(&name) {
            return name;
        }
    }
    "vignette_new".into()
}
