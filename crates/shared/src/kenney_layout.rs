//! Saved Kenney module layouts from the in-game editor.

use std::collections::HashMap;

use bevy::prelude::{Quat, Vec2};
use serde::{Deserialize, Serialize};

use crate::editor_map::{FloorMask, DEFAULT_MAP_MODULES};

pub use crate::editor_map::BranchLevel;
use crate::kenney_catalog::KENNEY_CELL;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubExit {
    pub x: f32,
    pub z: f32,
    pub floor: i32,
    pub label: String,
    #[serde(default = "default_hub_exit_kind")]
    pub kind: String,
}

fn default_hub_exit_kind() -> String {
    "drop".into()
}

pub const LAYOUT_PATH: &str = "userinput/kenney_layout.json";

fn layout_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(LAYOUT_PATH)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KenneyLayout {
    #[serde(default = "default_grid_unit")]
    pub grid_unit_m: f32,
    #[serde(default = "default_modules")]
    pub modules_x: u32,
    #[serde(default = "default_modules")]
    pub modules_z: u32,
    #[serde(default)]
    pub floors: HashMap<i32, FloorMask>,
    #[serde(default)]
    pub pieces: Vec<KenneyPlacement>,
    /// Optional explicit player spawn point [x, z] in world space.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spawn_xz: Option<[f32; 2]>,
    /// Extraction pit centre [x, z] on floor 0 (hub is one MOD_H below).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction_xz: Option<[f32; 2]>,
    /// Hub exit anchors keyed "0" | "1" (freeform) or legacy "2" | "3" | "4".
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub hub_exits: HashMap<String, HubExit>,
    /// `freeform_v1` = gen_freeform hub (no legacy west-stairs patch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hub_model: Option<String>,
    /// Legacy embedded branch destinations (deprecated).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub branch_levels: HashMap<String, BranchLevel>,
}

fn default_grid_unit() -> f32 {
    4.0
}

fn default_modules() -> u32 {
    DEFAULT_MAP_MODULES
}

impl Default for KenneyLayout {
    fn default() -> Self {
        Self {
            grid_unit_m: KENNEY_CELL,
            modules_x: DEFAULT_MAP_MODULES,
            modules_z: DEFAULT_MAP_MODULES,
            floors: HashMap::new(),
            pieces: Vec::new(),
            spawn_xz: None,
            extraction_xz: None,
            hub_exits: HashMap::new(),
            hub_model: None,
            branch_levels: HashMap::new(),
        }
    }
}

impl KenneyLayout {
    pub fn map_extent_m(&self) -> (f32, f32) {
        (
            self.modules_x as f32 * crate::editor_map::CELLS_PER_MODULE as f32 * self.grid_unit_m,
            self.modules_z as f32 * crate::editor_map::CELLS_PER_MODULE as f32 * self.grid_unit_m,
        )
    }

    pub fn map_center_xz(&self) -> (f32, f32) {
        let (ex, ez) = self.map_extent_m();
        (-ex * 0.5 + ex * 0.5, -ez * 0.5 + ez * 0.5)
    }

    pub fn map_world_x0(&self) -> f32 {
        let (ex, _) = self.map_extent_m();
        -ex * 0.5
    }

    pub fn map_world_z0(&self) -> f32 {
        let (_, ez) = self.map_extent_m();
        -ez * 0.5
    }

    /// World XZ of the main module cluster (ignores stray far-away placements).
    pub fn focus_xz(&self) -> Vec2 {
        if self.pieces.is_empty() {
            let (cx, cz) = self.map_center_xz();
            return Vec2::new(cx, cz);
        }
        let mut xs: Vec<f32> = self.pieces.iter().map(|p| p.x).collect();
        let mut zs: Vec<f32> = self.pieces.iter().map(|p| p.z).collect();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        zs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let med_x = xs[xs.len() / 2];
        let med_z = zs[zs.len() / 2];
        let (sum_x, sum_z, n) = self.pieces.iter().fold((0.0f32, 0.0f32, 0u32), |(sx, sz, n), p| {
            if (p.x - med_x).abs() <= 20.0 && (p.z - med_z).abs() <= 20.0 {
                (sx + p.x, sz + p.z, n + 1)
            } else {
                (sx, sz, n)
            }
        });
        if n == 0 {
            let n = self.pieces.len() as f32;
            let (sx, sz) = self
                .pieces
                .iter()
                .fold((0.0f32, 0.0f32), |(x, z), p| (x + p.x, z + p.z));
            return Vec2::new(sx / n, sz / n);
        }
        let n = n as f32;
        Vec2::new(sum_x / n, sum_z / n)
    }

    pub fn load_from_disk() -> Self {
        let path = layout_path();
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Infer extraction from pit markers when the exported layout omitted `extraction_xz`.
    pub fn infer_extraction_xz(&self) -> Option<[f32; 2]> {
        for p in &self.pieces {
            if matches!(
                p.stem.as_str(),
                "template-floor-hole" | "template-floor-layer-hole"
            ) && p.floor == 0
            {
                return Some([p.x, p.z]);
            }
        }
        for p in &self.pieces {
            if !crate::kenney_pit::is_room_shell(&p.stem) || p.floor != 0 {
                continue;
            }
            let has_hub_shell = self.pieces.iter().any(|q| {
                crate::kenney_pit::is_room_shell(&q.stem)
                    && q.floor == crate::kenney_pit::HUB_FLOOR_LEVEL
                    && (q.x - p.x).abs() < 0.5
                    && (q.z - p.z).abs() < 0.5
            });
            if has_hub_shell {
                return Some([p.x, p.z]);
            }
        }
        None
    }

    /// Fill metadata required for pit/hub playtest when the editor export is incomplete.
    pub fn resolve_for_playtest(mut self) -> Self {
        if self.extraction_xz.is_none() {
            self.extraction_xz = self.infer_extraction_xz();
        }
        if self.branch_levels.is_empty() && !crate::kenney_hub::is_freeform_hub_layout(&self) {
            if let Some([ex, ez]) = self.extraction_xz {
                self.branch_levels = crate::kenney_hub::default_branch_levels(ex, ez);
            }
        }
        self
    }

    /// True when the player is in the open extraction drop column (floor 0 → hub).
    pub fn in_extraction_shaft(&self, x: f32, y: f32, z: f32) -> bool {
        let Some([ex, ez]) = self.extraction_xz else {
            return false;
        };
        crate::kenney_pit::in_extraction_shaft(x, y, z, ex, ez)
    }

    pub fn save_to_disk(&self) -> std::io::Result<()> {
        let path = layout_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KenneyPlacement {
    pub stem: String,
    pub x: f32,
    pub z: f32,
    pub yaw: f32,
    /// Editor / `kenney_layout.json` use `floor`; pool maps use `floor_level`.
    #[serde(alias = "floor_level", default)]
    pub floor: i32,
    #[serde(default = "default_placement_scale")]
    pub scale: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<u32>,
    /// True for ceiling slabs (`template-floor` one level above walkable); not hidden over mask void.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ceiling: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub underside: bool,
}

fn default_placement_scale() -> f32 {
    1.0
}

/// World rotation for a placed piece (yaw only). Ceiling slabs are *not* flipped:
/// `template-floor` GLBs already carry a textured downward (−Y) face, so a slab
/// placed one level up reads as a ceiling from below without any rotation. The
/// `ceiling` flag is kept in the signature for call-site clarity.
pub fn placement_rotation(yaw: f32, _ceiling: bool) -> Quat {
    Quat::from_rotation_y(yaw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Vec3;

    #[test]
    fn placement_rotation_is_yaw_only() {
        // Ceiling slabs must not be flipped: a flat floor tile already shows a
        // textured face from below, and flipping only mirrors the texture.
        let flat = placement_rotation(0.0, false);
        let ceil = placement_rotation(0.0, true);
        assert!((flat * Vec3::Y).y > 0.9, "floor normal should point up");
        assert!((ceil * Vec3::Y).y > 0.9, "ceiling slab must not be flipped");
    }
}
