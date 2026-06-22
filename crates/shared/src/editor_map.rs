//! Editor map / module documents, floor masks, and save paths.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::kenney_catalog::KENNEY_CELL;
use crate::kenney_layout::HubExit;

pub const CELLS_PER_MODULE: u32 = 5;
pub const DEFAULT_MAP_MODULES: u32 = 3;
pub const MAPS_DIR: &str = "userinput/maps";
pub const MODULES_DIR: &str = "userinput/modules";
pub const DEFAULT_POOL: &str = "default";
pub const PLAYTEST_LAYOUT_PATH: &str = "userinput/kenney_layout.json";
/// Fixed path written by `gen_maps.py --preview` for live editor regeneration.
pub const MAP_GEN_PREVIEW_PATH: &str = "userinput/maps/_editor_preview.json";

fn userinput_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../userinput")
}

pub fn maps_dir() -> PathBuf {
    userinput_root().join("maps")
}

pub fn pool_dir(pool: &str) -> PathBuf {
    userinput_root().join("modules").join(pool)
}

pub fn timestamp_name() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("map_{secs}")
}

pub fn sanitize_filename(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        "untitled".into()
    } else {
        s
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EditorWorkflow {
    #[default]
    MapMaker,
    ModuleMaker,
}

impl EditorWorkflow {
    pub fn label(self) -> &'static str {
        match self {
            EditorWorkflow::MapMaker => "Map",
            EditorWorkflow::ModuleMaker => "Module",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SnapMode {
    #[default]
    FullCell,
    HalfCell,
    Free,
}

impl SnapMode {
    pub const ALL: [SnapMode; 3] = [SnapMode::FullCell, SnapMode::HalfCell, SnapMode::Free];

    pub fn label(self) -> &'static str {
        match self {
            SnapMode::FullCell => "4 m snap",
            SnapMode::HalfCell => "2 m snap",
            SnapMode::Free => "Free",
        }
    }

    pub fn next(self) -> Self {
        match self {
            SnapMode::FullCell => SnapMode::HalfCell,
            SnapMode::HalfCell => SnapMode::Free,
            SnapMode::Free => SnapMode::FullCell,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EditorTool {
    #[default]
    PlaceGlb,
    Select,
    FloorAdd,
    FloorRemove,
    PlaceModule,
    SetSpawn,
    /// Gallery review mode: no ghost, no placement, work-area mouse routes to gallery.
    GalleryPreview,
}

impl EditorTool {
    pub fn label(self) -> &'static str {
        match self {
            EditorTool::PlaceGlb => "Place",
            EditorTool::Select => "Select",
            EditorTool::FloorAdd => "Add floor",
            EditorTool::FloorRemove => "Remove floor",
            EditorTool::PlaceModule => "Place",
            EditorTool::SetSpawn => "Set spawn",
            EditorTool::GalleryPreview => "Gallery",
        }
    }
}

#[derive(Clone, Debug)]
pub struct GridSpec {
    pub cells_x: u32,
    pub cells_z: u32,
}

impl GridSpec {
    pub fn for_workflow(workflow: EditorWorkflow, modules_x: u32, modules_z: u32) -> Self {
        match workflow {
            EditorWorkflow::ModuleMaker => Self {
                cells_x: CELLS_PER_MODULE,
                cells_z: CELLS_PER_MODULE,
            },
            EditorWorkflow::MapMaker => Self {
                cells_x: modules_x * CELLS_PER_MODULE,
                cells_z: modules_z * CELLS_PER_MODULE,
            },
        }
    }

    pub fn extent_m(&self) -> (f32, f32) {
        (
            self.cells_x as f32 * KENNEY_CELL,
            self.cells_z as f32 * KENNEY_CELL,
        )
    }

    pub fn world_x0(&self) -> f32 {
        -(self.cells_x as f32 * KENNEY_CELL) * 0.5
    }

    pub fn world_z0(&self) -> f32 {
        -(self.cells_z as f32 * KENNEY_CELL) * 0.5
    }

    pub fn center_xz(&self) -> (f32, f32) {
        let (ex, ez) = self.extent_m();
        (self.world_x0() + ex * 0.5, self.world_z0() + ez * 0.5)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct FloorMask {
    pub cells_x: u32,
    pub cells_z: u32,
    pub cells: Vec<bool>,
}

impl FloorMask {
    pub fn filled(cells_x: u32, cells_z: u32) -> Self {
        Self {
            cells_x,
            cells_z,
            cells: vec![true; (cells_x * cells_z) as usize],
        }
    }

    pub fn get(&self, ix: u32, iz: u32) -> bool {
        if ix >= self.cells_x || iz >= self.cells_z {
            return false;
        }
        self.cells[(iz * self.cells_x + ix) as usize]
    }

    pub fn set(&mut self, ix: u32, iz: u32, on: bool) {
        if ix < self.cells_x && iz < self.cells_z {
            self.cells[(iz * self.cells_x + ix) as usize] = on;
        }
    }

    pub fn world_x0(&self) -> f32 {
        -(self.cells_x as f32 * KENNEY_CELL) * 0.5
    }

    pub fn world_z0(&self) -> f32 {
        -(self.cells_z as f32 * KENNEY_CELL) * 0.5
    }
}

fn default_scale() -> f32 {
    1.0
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PieceRecord {
    pub stem: String,
    pub x: f32,
    pub z: f32,
    pub yaw: f32,
    pub floor_level: i32,
    #[serde(default = "default_scale")]
    pub scale: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<u32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ceiling: bool,
    /// Walkable space exists on the floor below — render floor slab double-sided (not a duplicate piece).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub underside: bool,
    /// Kenney kit folder under `assets/models/` (default `space`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kit: Option<String>,
    /// sRGB tint multiplier for recognisable variants (e.g. hidden-room doors).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<[f32; 3]>,
    /// Semantic tags (`hidden_entrance`, …) for gameplay systems.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Level-composition zone (`prev` / `default` / `next`) for material routing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModuleDocument {
    pub version: u32,
    pub name: String,
    pub pool: String,
    /// Floor-0 mask kept for backward compatibility with v1 files.
    pub floor_mask: FloorMask,
    /// Per-floor masks for floors other than 0 (floor 0 uses `floor_mask`).
    #[serde(default)]
    pub extra_floor_masks: std::collections::HashMap<i32, FloorMask>,
    pub pieces: Vec<PieceRecord>,
}

impl ModuleDocument {
    pub fn new_named(name: impl Into<String>, pool: impl Into<String>) -> Self {
        let name = name.into();
        let pool = pool.into();
        Self {
            version: 1,
            name: name.clone(),
            pool,
            floor_mask: FloorMask::filled(CELLS_PER_MODULE, CELLS_PER_MODULE),
            extra_floor_masks: std::collections::HashMap::new(),
            pieces: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty() && self.floor_mask.cells.iter().all(|&c| c)
    }

    /// Returns the floor mask for the given level (floor 0 = `floor_mask`).
    pub fn floor_mask_for(&self, level: i32) -> FloorMask {
        if level == 0 {
            self.floor_mask.clone()
        } else {
            self.extra_floor_masks
                .get(&level)
                .cloned()
                .unwrap_or_else(|| FloorMask::filled(CELLS_PER_MODULE, CELLS_PER_MODULE))
        }
    }

    /// Returns a mutable reference to the floor mask for the given level.
    pub fn floor_mask_for_mut(&mut self, level: i32) -> &mut FloorMask {
        if level == 0 {
            &mut self.floor_mask
        } else {
            self.extra_floor_masks
                .entry(level)
                .or_insert_with(|| FloorMask::filled(CELLS_PER_MODULE, CELLS_PER_MODULE))
        }
    }

    pub fn path_in_pool(&self) -> PathBuf {
        pool_dir(&self.pool).join(format!("{}.json", sanitize_filename(&self.name)))
    }

    pub fn save(&self) -> std::io::Result<PathBuf> {
        let path = self.path_in_pool();
        save_json(&path, self)?;
        Ok(path)
    }

    pub fn load(path: &Path) -> Option<Self> {
        read_json(path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchLevel {
    pub x: f32,
    pub z: f32,
    pub floor: i32,
    pub label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MapDocument {
    pub version: u32,
    pub name: String,
    pub modules_x: u32,
    pub modules_z: u32,
    #[serde(default)]
    pub floors: HashMap<i32, FloorMask>,
    pub pieces: Vec<PieceRecord>,
    /// Explicit spawn point [x, z] placed by the editor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spawn_xz: Option<[f32; 2]>,
    /// Extraction pit centre [x, z] on floor 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction_xz: Option<[f32; 2]>,
    /// Hub branch destinations keyed "2" | "3" | "4" (L2 stairs, L3 pit, L4 west drop).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub branch_levels: HashMap<String, BranchLevel>,
    /// Free-form hub exits keyed "0" | "1" (see `gen_freeform.py`).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub hub_exits: HashMap<String, HubExit>,
    /// `freeform_v1` skips legacy west-stairs hub patching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hub_model: Option<String>,
    /// Faction procgen profile id (`userinput/factions/*.json`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faction_profile: Option<String>,
    /// Modular architecture kit folder used for this map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub building_system: Option<String>,
}

impl Default for MapDocument {
    fn default() -> Self {
        Self::new_default()
    }
}

impl MapDocument {
    pub fn new_default() -> Self {
        let modules_x = DEFAULT_MAP_MODULES;
        let modules_z = DEFAULT_MAP_MODULES;
        let grid = GridSpec::for_workflow(EditorWorkflow::MapMaker, modules_x, modules_z);
        let mut floors = HashMap::new();
        floors.insert(0, FloorMask::filled(grid.cells_x, grid.cells_z));
        Self {
            version: 1,
            name: "untitled".into(),
            modules_x,
            modules_z,
            floors,
            pieces: Vec::new(),
            spawn_xz: None,
            extraction_xz: None,
            branch_levels: HashMap::new(),
            hub_exits: HashMap::new(),
            hub_model: None,
            faction_profile: None,
            building_system: None,
        }
    }

    pub fn resize_map(&mut self, modules_x: u32, modules_z: u32) {
        self.modules_x = modules_x.max(1).min(32);
        self.modules_z = modules_z.max(1).min(32);
        let grid = self.grid();
        for mask in self.floors.values_mut() {
            *mask = FloorMask::filled(grid.cells_x, grid.cells_z);
        }
        if !self.floors.contains_key(&0) {
            self.floors.insert(0, FloorMask::filled(grid.cells_x, grid.cells_z));
        }
    }

    pub fn grid(&self) -> GridSpec {
        GridSpec::for_workflow(EditorWorkflow::MapMaker, self.modules_x, self.modules_z)
    }

    pub fn floor_mask(&self, level: i32) -> FloorMask {
        self.floors
            .get(&level)
            .cloned()
            .unwrap_or_else(|| {
                let g = self.grid();
                FloorMask::filled(g.cells_x, g.cells_z)
            })
    }

    pub fn floor_mask_mut(&mut self, level: i32) -> &mut FloorMask {
        let g = self.grid();
        self.floors
            .entry(level)
            .or_insert_with(|| FloorMask::filled(g.cells_x, g.cells_z))
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        save_json(path, self)
    }

    pub fn load(path: &Path) -> Option<Self> {
        read_json(path).or_else(|| Self::load_generated(path))
    }

    /// Load a map emitted by `tools/gen_maps.py` (string floor keys, hub fields).
    pub fn load_generated(path: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(path).ok()?;
        let v: serde_json::Value = serde_json::from_str(&raw).ok()?;

        let mut floors: HashMap<i32, FloorMask> = HashMap::new();
        if let Some(obj) = v.get("floors").and_then(|f| f.as_object()) {
            for (k, mask) in obj {
                if let Ok(level) = k.parse::<i32>() {
                    if let Ok(m) = serde_json::from_value(mask.clone()) {
                        floors.insert(level, m);
                    }
                }
            }
        }

        let pieces: Vec<PieceRecord> = v
            .get("pieces")
            .and_then(|p| p.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| {
                        Some(PieceRecord {
                            stem: p.get("stem")?.as_str()?.to_string(),
                            x: p.get("x")?.as_f64()? as f32,
                            z: p.get("z")?.as_f64()? as f32,
                            yaw: p.get("yaw")?.as_f64()? as f32,
                            floor_level: p
                                .get("floor_level")
                                .or_else(|| p.get("floor"))
                                .and_then(|x| x.as_i64())
                                .unwrap_or(0) as i32,
                            scale: p
                                .get("scale")
                                .and_then(|x| x.as_f64())
                                .map(|x| x as f32)
                                .unwrap_or(1.0),
                            group_id: p.get("group_id").and_then(|x| x.as_u64()).map(|x| x as u32),
                            ceiling: p.get("ceiling").and_then(|x| x.as_bool()).unwrap_or(false),
                            underside: p.get("underside").and_then(|x| x.as_bool()).unwrap_or(false),
                            kit: p.get("kit").and_then(|x| x.as_str()).map(str::to_string),
                            tint: p.get("tint").and_then(|t| {
                                let arr = t.as_array()?;
                                if arr.len() != 3 {
                                    return None;
                                }
                                Some([
                                    arr[0].as_f64()? as f32,
                                    arr[1].as_f64()? as f32,
                                    arr[2].as_f64()? as f32,
                                ])
                            }),
                            tags: p
                                .get("tags")
                                .and_then(|x| x.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| v.as_str().map(str::to_string))
                                        .collect()
                                })
                                .unwrap_or_default(),
                            zone: p.get("zone").and_then(|x| x.as_str()).map(str::to_string),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let branch_levels: HashMap<String, BranchLevel> = v
            .get("branch_levels")
            .and_then(|b| serde_json::from_value(b.clone()).ok())
            .unwrap_or_default();

        let hub_exits: HashMap<String, HubExit> = v
            .get("hub_exits")
            .and_then(|h| serde_json::from_value(h.clone()).ok())
            .unwrap_or_default();

        let hub_model = v
            .get("hub_model")
            .and_then(|x| x.as_str())
            .map(str::to_string);

        Some(Self {
            version: v.get("version").and_then(|x| x.as_u64()).unwrap_or(1) as u32,
            name: v
                .get("name")
                .and_then(|x| x.as_str())
                .unwrap_or("generated")
                .to_string(),
            modules_x: v
                .get("modules_x")
                .and_then(|x| x.as_u64())
                .map(|x| x as u32)
                .unwrap_or(5),
            modules_z: v
                .get("modules_z")
                .and_then(|x| x.as_u64())
                .map(|x| x as u32)
                .unwrap_or(5),
            floors,
            pieces,
            spawn_xz: v
                .get("spawn_xz")
                .and_then(|s| serde_json::from_value(s.clone()).ok()),
            extraction_xz: v
                .get("extraction_xz")
                .and_then(|s| serde_json::from_value(s.clone()).ok()),
            branch_levels,
            hub_exits,
            hub_model,
            faction_profile: v
                .get("faction_profile")
                .and_then(|x| x.as_str())
                .map(str::to_string),
            building_system: v
                .get("building_system")
                .and_then(|x| x.as_str())
                .map(str::to_string),
        })
    }

    pub fn export_playtest_layout(&self) -> std::io::Result<()> {
        let layout =
            crate::kenney_hub::patch_hub_branch_layout(self.to_kenney_layout().resolve_for_playtest());
        let path = userinput_root().join("kenney_layout.json");
        save_json(&path, &layout)
    }

    pub fn to_kenney_layout(&self) -> crate::kenney_layout::KenneyLayout {
        use crate::kenney_layout::{KenneyLayout, KenneyPlacement};
        KenneyLayout {
            grid_unit_m: KENNEY_CELL,
            modules_x: self.modules_x,
            modules_z: self.modules_z,
            floors: self.floors.clone(),
            pieces: self
                .pieces
                .iter()
                .map(|p| KenneyPlacement {
                    stem: p.stem.clone(),
                    x: p.x,
                    z: p.z,
                    yaw: p.yaw,
                    floor: p.floor_level,
                    scale: p.scale,
                    group_id: p.group_id,
                    ceiling: p.ceiling,
                    underside: p.underside,
                    kit: p.kit.clone(),
                    tint: p.tint,
                    tags: p.tags.clone(),
                    zone: p.zone.clone(),
                })
                .collect(),
            spawn_xz: self.spawn_xz,
            extraction_xz: self.extraction_xz,
            hub_exits: self.hub_exits.clone(),
            hub_model: self.hub_model.clone(),
            branch_levels: self.branch_levels.clone(),
        }
    }

    /// Apply hub-branch fixes (stairs floor/yaw, missing L3 shell) to map pieces in place.
    pub fn apply_hub_playtest_patches(&mut self) {
        if self.extraction_xz.is_none() {
            self.extraction_xz = self
                .to_kenney_layout()
                .infer_extraction_xz();
        }
        let mut layout = self.to_kenney_layout();
        layout = crate::kenney_hub::patch_hub_branch_layout(layout);
        self.pieces = layout
            .pieces
            .iter()
            .map(|p| PieceRecord {
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
                zone: p.zone.clone(),
            })
            .collect();
        self.floors = layout.floors;
        self.extraction_xz = layout.extraction_xz;
        if self.branch_levels.is_empty() {
            self.branch_levels = layout.branch_levels;
        }
        if self.hub_exits.is_empty() {
            self.hub_exits = layout.hub_exits;
        }
        if self.hub_model.is_none() {
            self.hub_model = layout.hub_model;
        }
    }
}

fn save_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value)?;
    std::fs::write(path, json)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ActiveDocKind {
    #[default]
    Map,
    Module,
}

#[derive(Clone, Debug, Default)]
pub struct ActiveDocument {
    pub kind: ActiveDocKind,
    pub path: Option<PathBuf>,
    pub dirty: bool,
}

impl ActiveDocument {
    pub fn quicksave_path_map(&self, _map: &MapDocument) -> PathBuf {
        if let Some(p) = &self.path {
            if self.kind == ActiveDocKind::Map {
                return p.clone();
            }
        }
        maps_dir().join(format!("{}.json", timestamp_name()))
    }

    pub fn branch_path_map(&self) -> PathBuf {
        maps_dir().join(format!("{}_branch.json", timestamp_name()))
    }

    pub fn module_snapshot_dir(pool: &str) -> PathBuf {
        pool_dir(pool).join("_snapshots")
    }

    pub fn save_module_snapshot(doc: &ModuleDocument) -> std::io::Result<PathBuf> {
        let dir = Self::module_snapshot_dir(&doc.pool);
        let path = dir.join(format!("{}.json", timestamp_name()));
        save_json(&path, doc)?;
        Ok(path)
    }

    pub fn latest_module_snapshot(pool: &str) -> Option<PathBuf> {
        let dir = Self::module_snapshot_dir(pool);
        let mut best: Option<(PathBuf, std::time::SystemTime)> = None;
        let Ok(read) = std::fs::read_dir(&dir) else {
            return None;
        };
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
        best.map(|(p, _)| p)
    }

    /// Stamp module floor cells onto the map at `center_xz` (module-local origin = center).
    /// Handles all floor levels stored in the module (floor 0 + any extra floors).
    pub fn bake_module_floor_on_map(
        map: &mut MapDocument,
        module: &ModuleDocument,
        center_x: f32,
        center_z: f32,
        base_floor_level: i32,
    ) {
        let half = CELLS_PER_MODULE as f32 * KENNEY_CELL * 0.5;
        let mod_sw_x = center_x - half;
        let mod_sw_z = center_z - half;

        // Collect all (module_level, mask) pairs to bake.
        let mut levels: Vec<(i32, &FloorMask)> = vec![(0, &module.floor_mask)];
        for (mod_level, mask) in &module.extra_floor_masks {
            levels.push((*mod_level, mask));
        }

        let grid = map.grid();
        let map_x0 = grid.world_x0();
        let map_z0 = grid.world_z0();

        for (mod_level, src_mask) in levels {
            let map_level = base_floor_level + mod_level;
            let dst_mask = map.floor_mask_mut(map_level);
            for iz in 0..src_mask.cells_z {
                for ix in 0..src_mask.cells_x {
                    if !src_mask.get(ix, iz) {
                        continue;
                    }
                    let wx = mod_sw_x + ix as f32 * KENNEY_CELL;
                    let wz = mod_sw_z + iz as f32 * KENNEY_CELL;
                    let mix = ((wx - map_x0) / KENNEY_CELL).floor() as i32;
                    let miz = ((wz - map_z0) / KENNEY_CELL).floor() as i32;
                    if mix >= 0 && miz >= 0 {
                        let mix = mix as u32;
                        let miz = miz as u32;
                        if mix < dst_mask.cells_x && miz < dst_mask.cells_z {
                            dst_mask.set(mix, miz, true);
                        }
                    }
                }
            }
        }
    }
}
