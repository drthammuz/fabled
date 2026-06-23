//! Machine-readable synth dressing placement data (probed from GLBs).
//!
//! Source: `assets/models/factions/synth/placement_catalog.json`
//! Regenerate: `python tools/probe_synth_catalog.py`

use std::collections::HashMap;
use std::sync::OnceLock;

use bevy::prelude::Vec3;
use serde::Deserialize;

const CATALOG_JSON: &str = include_str!("../../../assets/models/factions/synth/placement_catalog.json");

#[derive(Clone, Debug, Deserialize)]
pub struct BoundsScale1 {
    pub x0: f32,
    pub x1: f32,
    pub y0: f32,
    pub y1: f32,
    pub z0: f32,
    pub z1: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct StemPlacement {
    pub class: String,
    pub bounds_scale1: BoundsScale1,
    pub half_x_m: f32,
    pub half_z_m: f32,
    pub height_m: f32,
    /// `+z` or `-z` — local axis that points "forward" / user-facing for props.
    pub front: String,
    pub snap: String,
    pub deck_y: String,
    #[serde(default)]
    pub back_anchor_local_m: Option<f32>,
    #[serde(default)]
    pub pillow_local_z: Option<f32>,
    #[serde(default)]
    pub stack_on: Option<String>,
    #[serde(default)]
    pub decal_depth_half_m: Option<f32>,
}

#[derive(Clone, Debug, Deserialize)]
struct CatalogFile {
    pub default_scale: f32,
    pub cell_m: f32,
    pub wall_face_offset_m: f32,
    pub deck_y: f32,
    pub wall_half_thickness_m: f32,
    pub stems: HashMap<String, StemPlacement>,
}

static CATALOG: OnceLock<CatalogFile> = OnceLock::new();

fn catalog() -> &'static CatalogFile {
    CATALOG.get_or_init(|| {
        serde_json::from_str(CATALOG_JSON).expect("placement_catalog.json parse")
    })
}

pub fn lookup(stem: &str) -> Option<&'static StemPlacement> {
    catalog().stems.get(stem)
}

pub fn default_scale() -> f32 {
    catalog().default_scale
}

pub fn cell_m() -> f32 {
    catalog().cell_m
}

pub fn wall_face_offset_m() -> f32 {
    catalog().wall_face_offset_m
}

pub fn deck_y() -> f32 {
    catalog().deck_y
}

pub fn wall_half_thickness_m() -> f32 {
    catalog().wall_half_thickness_m
}

/// Local +Z front = 1, local −Z front = −1.
pub fn front_sign(stem: &str) -> i8 {
    match lookup(stem).map(|s| s.front.as_str()) {
        Some("-z") => -1,
        _ => 1,
    }
}

/// Yaw so local front faces world direction `(dx, dz)`.
pub fn face_yaw(stem: &str, dx: f32, dz: f32) -> f32 {
    use std::f32::consts::PI;
    if dx.abs() < 1e-4 && dz.abs() < 1e-4 {
        return 0.0;
    }
    let base = dx.atan2(dz);
    // Seat/work props face +Z; headrest mass can mis-tag −Z in the catalog.
    let yaw = if stem.starts_with("chair") {
        base
    } else if front_sign(stem) < 0 {
        base + PI
    } else {
        base
    };
    crate::kenney_catalog::quantize_yaw(yaw)
}

pub fn half_extents_xz(stem: &str, scale: f32) -> Option<(f32, f32)> {
    let s = lookup(stem)?;
    let sc = scale.abs().max(0.01);
    Some((s.half_x_m * sc, s.half_z_m * sc))
}

pub fn uses_back_anchor(stem: &str) -> bool {
    lookup(stem).is_some_and(|s| s.snap == "back_z")
}

pub fn back_anchor_offset(stem: &str, yaw: f32) -> Option<Vec3> {
    let s = lookup(stem)?;
    if s.snap != "back_z" {
        return None;
    }
    let d = s.back_anchor_local_m?;
    Some(
        bevy::prelude::Quat::from_rotation_y(crate::kenney_catalog::quantize_yaw(yaw))
            * Vec3::new(0.0, 0.0, d),
    )
}

pub fn is_stack_cover(stem: &str) -> bool {
    lookup(stem).is_some_and(|s| s.snap == "stack")
}

pub fn stack_base_stem(stem: &str) -> Option<&'static str> {
    lookup(stem)?.stack_on.as_deref()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chair_faces_plus_z_at_zero_yaw() {
        assert_eq!(front_sign("chair"), 1);
        assert!(face_yaw("chair", 0.0, 1.0).abs() < 0.01);
        assert!((face_yaw("chair", 0.0, -1.0) - std::f32::consts::PI).abs() < 0.01);
    }

    #[test]
    fn bed_cover_has_no_back_anchor() {
        assert!(!uses_back_anchor("bed-single-cover"));
        assert!(uses_back_anchor("bed-single"));
    }
}
