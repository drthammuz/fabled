//! Client-only wall panel breakup — breaks flat cuboids into smaller panels.

use bevy::prelude::*;
use shared::config;
use shared::level::{StaticDef, StaticKind};

/// Wall kinds that get panelized for visual variety.
pub fn panelize_kind(kind: StaticKind) -> bool {
    matches!(
        kind,
        StaticKind::SewerWall
            | StaticKind::SewerDuct
            | StaticKind::SewerBrace
            | StaticKind::SewerArch
            | StaticKind::Building
            | StaticKind::Wall
    )
}

/// Split a wall cuboid into smaller panels. Returns (local center offset, size)
/// in the def's local space (before rotation).
pub fn wall_panels(def: &StaticDef, seed: u64, index: u32) -> Vec<(Vec3, Vec3)> {
    let max_panel = config::WALL_PANEL_MAX_M;
    let size = def.size;
    if size.x.max(size.y).max(size.z) <= max_panel {
        return vec![(Vec3::ZERO, size)];
    }

    // Split along the longest axis.
    if size.z >= size.x && size.z >= size.y {
        split_axis(size, Vec3::new(0.0, 0.0, 1.0), seed, index)
    } else if size.x >= size.y {
        split_axis(size, Vec3::new(1.0, 0.0, 0.0), seed, index)
    } else {
        split_axis(size, Vec3::new(0.0, 1.0, 0.0), seed, index)
    }
}

fn split_axis(size: Vec3, axis: Vec3, seed: u64, index: u32) -> Vec<(Vec3, Vec3)> {
    let max_panel = config::WALL_PANEL_MAX_M;
    let long = size.dot(axis).abs();
    let count = (long / max_panel).ceil().max(1.0) as u32;
    let panel_len = long / count as f32;
    let mut panels = Vec::with_capacity(count as usize);

    for i in 0..count {
        let t = (i as f32 + 0.5) / count as f32 - 0.5;
        let jitter = panel_jitter(seed, index, i);
        let mut panel_size = size;
        if axis.z > 0.5 {
            panel_size.z = panel_len;
        } else if axis.x > 0.5 {
            panel_size.x = panel_len;
        } else {
            panel_size.y = panel_len;
        }
        let offset = axis * (t * long) + jitter;
        panels.push((offset, panel_size));
    }
    panels
}

fn panel_jitter(seed: u64, wall: u32, panel: u32) -> Vec3 {
    let h = seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(wall as u64)
        .wrapping_add((panel as u64) << 32);
    let j = |n: u64| {
        let v = ((n ^ (n >> 15)).wrapping_mul(0x27D4_EB2D) >> 12) as f32 / 1048576.0 - 0.5;
        v * 0.08
    };
    Vec3::new(j(h), j(h.wrapping_add(1)), j(h.wrapping_add(2)))
}
