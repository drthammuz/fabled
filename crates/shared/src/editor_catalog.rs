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
