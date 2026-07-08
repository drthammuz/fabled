//! World-space placement of Kenney module pieces, shared between the
//! windowed client (which spawns `SceneRoot` visuals from these) and the
//! headless dedicated server (which bakes trimesh colliders from these via
//! raw glTF parsing, since it has no AssetServer/render app). Both MUST
//! agree on the exact same placements, or the server's collision and the
//! client's visuals/faction layout diverge.

use bevy::math::{Quat, Vec2, Vec3};

use crate::kenney_catalog::{self, quantize_yaw};
use crate::kenney_layout::KenneyLayout;
use crate::kenney_pit;
use crate::level::kenney_stairs_placement;
use crate::map_pool::{instances_from_stream_state, MountedMap, PoolIndex};
use crate::run::MapStreamState;
use crate::{TestMapStyle, TestMode};

#[derive(Clone)]
pub struct Placement {
    pub stem: &'static str,
    pub kit: Option<&'static str>,
    pub pos: Vec3,
    pub yaw: f32,
    pub scale: f32,
    pub scale_y: Option<f32>,
    pub collide: bool,
    pub mesh_cutouts: kenney_pit::KenneyMeshCutouts,
    pub group_id: Option<u32>,
    pub floor: i32,
    pub ceiling: bool,
}

pub fn leak_stem(stem: &str) -> &'static str {
    Box::leak(stem.to_string().into_boxed_str())
}

fn m(
    stem: &'static str,
    pos: Vec3,
    yaw: f32,
    scale: f32,
    collide: bool,
    mesh_cutouts: kenney_pit::KenneyMeshCutouts,
    group_id: Option<u32>,
    floor: i32,
    ceiling: bool,
) -> Placement {
    Placement {
        stem,
        kit: None,
        pos,
        yaw,
        scale,
        scale_y: None,
        collide,
        mesh_cutouts,
        group_id,
        floor,
        ceiling,
    }
}

/// Same rotation matrix `Transform::from_rotation(Quat::from_rotation_y(yaw))`
/// would produce, for callers building world matrices without a `Transform`.
pub fn placement_quat(p: &Placement) -> Quat {
    Quat::from_rotation_y(p.yaw)
}

pub fn placement_scale(p: &Placement) -> Vec3 {
    let sy = p.scale_y.unwrap_or(p.scale);
    Vec3::new(p.scale, sy, p.scale)
}

pub fn placements(style: TestMapStyle) -> Vec<Placement> {
    match style {
        TestMapStyle::Rusty => vec![],
        TestMapStyle::Kenney => kenney_placements(),
    }
}

pub fn kenney_placements() -> Vec<Placement> {
    if let Some(pool) = PoolIndex::load_from_disk() {
        let state = MapStreamState {
            active_pool_id: pool.start_id().unwrap_or("map_001").to_string(),
            ..Default::default()
        };
        if let Some((active, candidates)) = instances_from_stream_state(&state, &pool) {
            return placements_from_instances(&active, &candidates);
        }
    }
    placements_from_layout(&crate::map_pool::test_play_layout())
}

pub fn placements_from_layout(layout: &KenneyLayout) -> Vec<Placement> {
    let mut out: Vec<Placement> = Vec::new();
    let (ex_def, ez_def) = layout
        .extraction_xz
        .map(|[a, b]| (a, b))
        .unwrap_or((f32::INFINITY, f32::INFINITY));
    for p in &layout.pieces {
        let mask = layout.floors.get(&p.floor);
        if kenney_pit::hide_extraction_hatch_piece(&p.stem, p.floor, p.x, p.z, mask, p.ceiling) {
            continue;
        }
        let mut collide = kenney_catalog::piece(&p.stem)
            .map(|x| x.collide_default)
            .unwrap_or(false);
        if kenney_pit::skip_hub_passage_collider(&p.stem, p.floor, p.x, p.z, ex_def, ez_def, mask) {
            collide = false;
        }
        let mesh_cutouts = kenney_pit::mesh_cutouts_for_piece(
            &p.stem,
            p.floor,
            p.x,
            p.z,
            p.yaw,
            layout.extraction_xz.map(|[ex, ez]| Vec2::new(ex, ez)),
            mask,
            p.ceiling,
        );
        let mut pl = m(
            leak_stem(&p.stem),
            Vec3::new(p.x, p.world_y(), p.z),
            quantize_yaw(p.yaw),
            p.scale.max(0.01),
            collide,
            mesh_cutouts,
            p.group_id,
            p.floor,
            p.ceiling,
        );
        pl.kit = p.kit.as_deref().map(leak_stem);
        pl.scale_y = p.scale_y;
        out.push(pl);
    }

    if !out.iter().any(|p| p.stem == "stairs") {
        if let Some((pos, yaw)) = kenney_stairs_placement() {
            let collide = kenney_catalog::piece("stairs")
                .map(|p| p.collide_default)
                .unwrap_or(true);
            out.push(m(
                "stairs",
                pos,
                yaw,
                1.0,
                collide,
                kenney_pit::KenneyMeshCutouts::default(),
                None,
                0,
                false,
            ));
        }
    }
    out
}

pub fn placements_from_instances(active: &MountedMap, candidates: &[MountedMap]) -> Vec<Placement> {
    let mut out = placements_from_mounted(active);
    for c in candidates {
        out.extend(placements_from_mounted(c));
    }
    out
}

pub fn placements_from_mounted(inst: &MountedMap) -> Vec<Placement> {
    let mut out: Vec<Placement> = Vec::new();
    let (ex_def, ez_def) = inst
        .layout
        .extraction_xz
        .map(|[a, b]| (a, b))
        .unwrap_or((f32::INFINITY, f32::INFINITY));
    for p in &inst.layout.pieces {
        // All hub decisions run in the instance-local frame (mask is origin-centred).
        let mask = inst.layout.floors.get(&p.floor);
        if kenney_pit::hide_extraction_hatch_piece(&p.stem, p.floor, p.x, p.z, mask, p.ceiling) {
            continue;
        }
        let mut collide = kenney_catalog::piece(&p.stem)
            .map(|x| x.collide_default)
            .unwrap_or(false);
        if kenney_pit::skip_hub_passage_collider(&p.stem, p.floor, p.x, p.z, ex_def, ez_def, mask) {
            collide = false;
        }
        let mesh_cutouts = kenney_pit::mesh_cutouts_for_piece(
            &p.stem,
            p.floor,
            p.x,
            p.z,
            p.yaw,
            inst.layout.extraction_xz.map(|[ex, ez]| Vec2::new(ex, ez)),
            mask,
            p.ceiling,
        )
        .translated(inst.offset.x, inst.offset.z);
        let mut pl = m(
            leak_stem(&p.stem),
            inst.piece_translation(p),
            quantize_yaw(p.yaw),
            p.scale.max(0.01),
            collide,
            mesh_cutouts,
            p.group_id,
            p.floor,
            p.ceiling,
        );
        pl.kit = p.kit.as_deref().map(leak_stem);
        pl.scale_y = p.scale_y;
        out.push(pl);
    }
    out
}

/// True if this is a real-game session (`TestMode{Kenney}`, not editor) that
/// should render/collide the pool's faction map chain.
pub fn wants_pool_placements(test: Option<&TestMode>) -> bool {
    test.is_some_and(|t| t.style == TestMapStyle::Kenney)
}
