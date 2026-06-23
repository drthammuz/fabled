//! Pre-generated Kenney map pool (dev/test) and runtime instance mounting.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::prelude::Vec3;
use serde::{Deserialize, Serialize};

use crate::editor_map::FloorMask;
use crate::kenney_layout::{HubExit, KenneyLayout, KenneyPlacement};
use crate::level::MOD_H;

pub const POOL_INDEX_PATH: &str = "userinput/maps/pool/index.json";

fn repo_relative(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(path)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolMapEntry {
    pub id: String,
    pub path: String,
    pub spawn_xz: [f32; 2],
    #[serde(default)]
    pub extraction_xz: Option<[f32; 2]>,
    #[serde(default)]
    pub hub_exits: HashMap<String, HubExit>,
    #[serde(default = "default_modules")]
    pub modules_x: u32,
    #[serde(default = "default_modules")]
    pub modules_z: u32,
}

fn default_modules() -> u32 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolIndex {
    pub version: u32,
    #[serde(default = "default_modules")]
    pub modules: u32,
    #[serde(default)]
    pub grid_unit_m: f32,
    #[serde(default)]
    pub start_id: Option<String>,
    pub maps: Vec<PoolMapEntry>,
}

#[derive(Debug, Clone)]
pub struct PoolMapDocument {
    pub pool_id: String,
    pub layout: KenneyLayout,
}

impl PoolIndex {
    pub fn load_from_disk() -> Option<Self> {
        let path = repo_relative(POOL_INDEX_PATH);
        if !path.exists() {
            return None;
        }
        let raw = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    pub fn start_id(&self) -> Option<&str> {
        self.start_id
            .as_deref()
            .or_else(|| self.maps.first().map(|m| m.id.as_str()))
    }

    pub fn entry(&self, id: &str) -> Option<&PoolMapEntry> {
        self.maps.iter().find(|m| m.id == id)
    }

    pub fn unused<'a>(&'a self, used: &'a [String]) -> Vec<&'a PoolMapEntry> {
        self.maps
            .iter()
            .filter(|m| !used.contains(&m.id))
            .collect()
    }

    /// Load the pool's start map document.
    pub fn start_map(&self) -> Option<PoolMapDocument> {
        let id = self.start_id()?.to_string();
        let entry = self.entry(&id)?;
        PoolMapDocument::load(entry)
    }
}

impl PoolMapDocument {
    pub fn load(entry: &PoolMapEntry) -> Option<Self> {
        let path = repo_relative(&format!("userinput/maps/{}", entry.path));
        let raw = std::fs::read_to_string(&path).ok()?;
        let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
        let pool_id = v
            .get("pool_id")
            .and_then(|x| x.as_str())
            .unwrap_or(&entry.id)
            .to_string();

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

        let pieces: Vec<KenneyPlacement> = v
            .get("pieces")
            .and_then(|p| serde_json::from_value(p.clone()).ok())
            .unwrap_or_default();
        if pieces.is_empty() {
            if let Some(arr) = v.get("pieces").and_then(|p| p.as_array()) {
                if !arr.is_empty() {
                    eprintln!(
                        "map_pool: failed to deserialize {} pieces from {} — check floor/floor_level fields",
                        arr.len(),
                        entry.path
                    );
                }
            }
        }

        let hub_exits: HashMap<String, HubExit> = v
            .get("hub_exits")
            .and_then(|h| serde_json::from_value(h.clone()).ok())
            .unwrap_or_default();

        let branch_levels = v
            .get("branch_levels")
            .and_then(|b| serde_json::from_value(b.clone()).ok())
            .unwrap_or_default();

        let layout = KenneyLayout {
            grid_unit_m: v
                .get("grid_unit_m")
                .and_then(|x| x.as_f64())
                .map(|x| x as f32)
                .unwrap_or(4.0),
            modules_x: v
                .get("modules_x")
                .and_then(|x| x.as_u64())
                .map(|x| x as u32)
                .unwrap_or(entry.modules_x),
            modules_z: v
                .get("modules_z")
                .and_then(|x| x.as_u64())
                .map(|x| x as u32)
                .unwrap_or(entry.modules_z),
            floors,
            pieces,
            spawn_xz: v
                .get("spawn_xz")
                .and_then(|s| serde_json::from_value(s.clone()).ok())
                .or(Some(entry.spawn_xz)),
            spawn_y: v
                .get("spawn_y")
                .and_then(|s| serde_json::from_value(s.clone()).ok()),
            extraction_xz: v
                .get("extraction_xz")
                .and_then(|s| serde_json::from_value(s.clone()).ok())
                .or(entry.extraction_xz),
            hub_exits,
            hub_model: v
                .get("hub_model")
                .and_then(|m| m.as_str())
                .map(str::to_owned),
            branch_levels,
        };

        Some(Self { pool_id, layout })
    }
}

/// World offset to mount a child map so its spawn sits under a hub exit.
pub fn mount_offset(exit: &HubExit, child: &KenneyLayout) -> Vec3 {
    let [sx, sz] = child.spawn_xz.unwrap_or([0.0, 0.0]);
    let hub_y = exit.floor as f32 * MOD_H;
    let child_floor_y = match exit.kind.as_str() {
        "walk" => hub_y,
        _ => hub_y - MOD_H,
    };
    Vec3::new(exit.x - sx, child_floor_y, exit.z - sz)
}

/// Hub exits for a layout — prefers `hub_exits`, falls back to legacy `branch_levels`.
pub fn hub_exits_for(layout: &KenneyLayout) -> HashMap<String, HubExit> {
    if !layout.hub_exits.is_empty() {
        return layout.hub_exits.clone();
    }
    layout
        .branch_levels
        .iter()
        .map(|(k, b)| {
            (
                k.clone(),
                HubExit {
                    x: b.x,
                    z: b.z,
                    floor: b.floor,
                    label: b.label.clone(),
                    kind: if k == "2" { "walk".into() } else { "drop".into() },
                },
            )
        })
        .collect()
}

/// One mounted pool map in the world (local layout + world offset).
#[derive(Clone)]
pub struct MountedMap {
    pub instance_id: u32,
    pub pool_id: String,
    pub offset: Vec3,
    pub layout: KenneyLayout,
    pub exit: Option<u8>,
}

impl MountedMap {
    pub fn active(pool_id: impl Into<String>, layout: KenneyLayout) -> Self {
        Self {
            instance_id: 1,
            pool_id: pool_id.into(),
            offset: Vec3::ZERO,
            // Bake hub holes into the local mask so floor visuals + physics share one source.
            // No-op for non-hub maps (patch returns early without an extraction point).
            layout: crate::kenney_hub::patch_hub_branch_layout(layout),
            exit: None,
        }
    }

    pub fn candidate(
        instance_id: u32,
        pool_id: impl Into<String>,
        layout: KenneyLayout,
        offset: Vec3,
        exit: u8,
    ) -> Self {
        Self {
            instance_id,
            pool_id: pool_id.into(),
            offset,
            layout: crate::kenney_hub::patch_hub_branch_layout(layout),
            exit: Some(exit),
        }
    }

    pub fn piece_world_y(&self, floor: i32) -> f32 {
        floor as f32 * MOD_H + self.offset.y
    }

    pub fn piece_translation(&self, p: &KenneyPlacement) -> Vec3 {
        Vec3::new(
            p.x + self.offset.x,
            p.floor as f32 * MOD_H + self.offset.y + 0.002,
            p.z + self.offset.z,
        )
    }

    pub fn world_spawn(&self) -> Option<[f32; 2]> {
        self.layout
            .spawn_xz
            .map(|[x, z]| [x + self.offset.x, z + self.offset.z])
    }

    pub fn world_extraction(&self) -> Option<[f32; 2]> {
        self.layout
            .extraction_xz
            .map(|[x, z]| [x + self.offset.x, z + self.offset.z])
    }

    pub fn world_hub_exits(&self) -> HashMap<String, HubExit> {
        hub_exits_for(&self.layout)
            .into_iter()
            .map(|(k, mut e)| {
                e.x += self.offset.x;
                e.z += self.offset.z;
                (k, e)
            })
            .collect()
    }

    pub fn to_world_layout(&self) -> KenneyLayout {
        let mut layout = self.layout.clone();
        for p in &mut layout.pieces {
            p.x += self.offset.x;
            p.z += self.offset.z;
        }
        if let Some([x, z]) = layout.spawn_xz {
            layout.spawn_xz = Some([x + self.offset.x, z + self.offset.z]);
        }
        if let Some([x, z]) = layout.extraction_xz {
            layout.extraction_xz = Some([x + self.offset.x, z + self.offset.z]);
        }
        for exit in layout.hub_exits.values_mut() {
            exit.x += self.offset.x;
            exit.z += self.offset.z;
        }
        for exit in layout.branch_levels.values_mut() {
            exit.x += self.offset.x;
            exit.z += self.offset.z;
        }
        layout
    }
}

/// Reconstruct active + candidate instances from replicated run state (client + server).
pub fn instances_from_stream_state(
    state: &crate::run::MapStreamState,
    pool: &PoolIndex,
) -> Option<(MountedMap, Vec<MountedMap>)> {
    if state.active_pool_id.is_empty() {
        return None;
    }
    let entry = pool.entry(&state.active_pool_id)?;
    let doc = PoolMapDocument::load(entry)?;
    let active = MountedMap::active(state.active_pool_id.clone(), doc.layout);
    let active_exits = active.world_hub_exits();

    let mut candidates = Vec::new();
    let mut next_id = active.instance_id + 1;
    for (exit, pool_id) in &state.candidates {
        let entry = pool.entry(pool_id)?;
        let doc = PoolMapDocument::load(entry)?;
        let key = exit.to_string();
        let exit_spec = active_exits.get(&key)?;
        let offset = mount_offset_world(exit_spec, &doc.layout);
        candidates.push(MountedMap::candidate(
            next_id,
            pool_id.clone(),
            doc.layout,
            offset,
            *exit,
        ));
        next_id += 1;
    }
    Some((active, candidates))
}

pub fn mount_offset_world(exit: &HubExit, child: &KenneyLayout) -> Vec3 {
    mount_offset(exit, child)
}

/// Kenney `--test` layout: pool start map when `pool/index.json` exists.
pub fn test_play_layout() -> KenneyLayout {
    PoolIndex::load_from_disk()
        .and_then(|p| p.start_map())
        .map(|d| d.layout)
        .unwrap_or_else(KenneyLayout::load_from_disk)
}

/// Kenney layout for the current play mode.
/// Editor playtest (G) uses the saved `kenney_layout.json`; standalone `--test --kenney` uses the pool.
pub fn play_layout(editor_active: bool) -> KenneyLayout {
    let layout = if editor_active {
        KenneyLayout::load_from_disk()
    } else {
        test_play_layout()
    };
    crate::kenney_hub::patch_hub_branch_layout(layout.resolve_for_playtest())
}

/// World XZ spawn with per-player spread.
pub fn play_spawn_xz(editor_active: bool, player_index: usize) -> Option<[f32; 2]> {
    let [sx, sz] = play_layout(editor_active).spawn_xz?;
    let spread_x = (player_index % 2) as f32 * 2.0 - 1.0;
    let spread_z = (player_index / 2) as f32 * 2.0;
    Some([sx + spread_x, sz + spread_z])
}

/// Walkable surface Y at the spawn marker (0 = industrial substrate).
pub fn play_spawn_y(editor_active: bool) -> f32 {
    play_layout(editor_active).spawn_floor_y()
}

/// World XZ spawn for standalone pool test play.
pub fn test_spawn_xz(player_index: usize) -> Option<[f32; 2]> {
    play_spawn_xz(false, player_index)
}

/// Bootstrap the active map instance from the pool (no candidates).
pub fn bootstrap_active_map() -> Option<(PoolIndex, MountedMap)> {
    let pool = PoolIndex::load_from_disk()?;
    let doc = pool.start_map()?;
    let id = doc.pool_id.clone();
    Some((pool, MountedMap::active(id, doc.layout)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_map_pieces_load_floor_level_field() {
        let pool = PoolIndex::load_from_disk().expect("pool index");
        let doc = pool.start_map().expect("start map");
        assert!(
            !doc.layout.pieces.is_empty(),
            "pool pieces must deserialize (floor_level → floor)"
        );
        assert_eq!(doc.layout.pieces[0].stem, "room-large");
        assert_eq!(doc.layout.spawn_xz, Some([-40.0, -40.0]));
    }

    #[test]
    fn hub_drop_zones_are_three_separate_tiles_off_the_landing() {
        use crate::kenney_pit::{
            hub_l3_drop_centre, hub_west_drop_centre, in_hub_l3_drop_zone, in_hub_west_drop_zone,
        };
        let ex = 20.0;
        let ez = 20.0;
        let west = hub_west_drop_centre(ex, ez);
        // SW corner (ex-8, ez+8) — off the centre row to the gate / stairs.
        assert_eq!(west, bevy::prelude::Vec2::new(12.0, 28.0));
        assert!(in_hub_west_drop_zone(12.0, 28.0, ex, ez));
        // The (ex,ez) landing tile is SOLID — not any drop zone.
        assert!(!in_hub_west_drop_zone(20.0, 20.0, ex, ez));
        assert!(!in_hub_l3_drop_zone(20.0, 20.0, ex, ez));
        // L3 drop sits on its own tile north of the landing.
        let l3 = hub_l3_drop_centre(ex, ez);
        assert_eq!(l3, bevy::prelude::Vec2::new(20.0, 12.0));
        assert!(in_hub_l3_drop_zone(l3.x, l3.y, ex, ez));
        use crate::kenney_pit::{hub_stairs_opening, hub_stairs_opening_cells};
        let stair = hub_stairs_opening(ex, ez);
        // Cell-aligned (west centre 0 + 4), not the off-grid +6 that desynced mask vs mesh.
        assert_eq!(stair, bevy::prelude::Vec2::new(4.0, 20.0));
        // The stairs span two cells (stair-top + one west); the door threshold (8,20) stays solid.
        let cells = hub_stairs_opening_cells(ex, ez);
        assert_eq!(cells, [bevy::prelude::Vec2::new(4.0, 20.0), bevy::prelude::Vec2::new(0.0, 20.0)]);
        assert!(crate::kenney_pit::in_hub_stairs_opening(0.0, 20.0, ex, ez));
        assert!(crate::kenney_pit::in_hub_stairs_opening(4.0, 20.0, ex, ez));
        assert!(!crate::kenney_pit::in_hub_stairs_opening(8.0, 20.0, ex, ez));
    }

    #[test]
    fn patch_infers_missing_extraction_from_floor_hole() {
        let mut layout = KenneyLayout::load_from_disk();
        if layout
            .pieces
            .iter()
            .any(|p| p.stem == "template-floor-hole" && p.floor == 0)
        {
            layout.extraction_xz = None;
            layout.branch_levels.clear();
            let patched = crate::kenney_hub::patch_hub_branch_layout(layout);
            assert!(
                patched.extraction_xz.is_some(),
                "playtest layout must infer extraction_xz from floor-0 pit marker"
            );
            assert!(!patched.branch_levels.is_empty());
        }
    }
}
