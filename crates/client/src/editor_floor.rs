//! Floor slab visuals, grid backdrop, and area floor-paint.

use bevy::prelude::*;
use shared::editor_map::{EditorTool, EditorWorkflow, FloorMask, GridSpec};
use shared::kenney_catalog::KENNEY_CELL;
use shared::level::MOD_H;

use crate::editor_workspace::{EditorWorkspace, FloorSlab, FloorSlabGrid, FloorSlabPainted, FloorSlabPreview};

fn extraction_pit_cell(ws: &EditorWorkspace, _ix: u32, _iz: u32, cx: f32, cz: f32) -> bool {
    if ws.workflow != EditorWorkflow::MapMaker || ws.floor_level != 0 {
        return false;
    }
    let Some([ex, ez]) = ws.map.extraction_xz else {
        return false;
    };
    (cx - ex).abs() < 0.25 && (cz - ez).abs() < 0.25
}

pub fn sync_floor_slabs(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ws: ResMut<EditorWorkspace>,
    existing: Query<Entity, Or<(With<FloorSlab>, With<FloorSlabGrid>, With<FloorSlabPainted>, With<FloorSlabPreview>)>>,
) {
    if !ws.floor_dirty {
        return;
    }
    ws.floor_dirty = false;
    for e in &existing {
        commands.entity(e).despawn();
    }

    let grid_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.16, 0.18, 0.22, 0.42),
        perceptual_roughness: 0.92,
        metallic: 0.02,
        ..default()
    });
    let paint_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.24, 0.27, 0.32),
        perceptual_roughness: 0.88,
        metallic: 0.05,
        ..default()
    });
    let preview_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.35, 0.75, 0.45, 0.45),
        emissive: LinearRgba::rgb(0.12, 0.35, 0.18),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    let preview_remove_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.85, 0.25, 0.25, 0.4),
        emissive: LinearRgba::rgb(0.35, 0.08, 0.08),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    let mesh = meshes.add(Cuboid::new(KENNEY_CELL, 0.08, KENNEY_CELL));
    let (mask, x0, z0, floor_y) = floor_context(&ws);
    let grid = ws.grid();

    for iz in 0..grid.cells_z {
        for ix in 0..grid.cells_x {
            let cx = x0 + ix as f32 * KENNEY_CELL + KENNEY_CELL * 0.5;
            let cz = z0 + iz as f32 * KENNEY_CELL + KENNEY_CELL * 0.5;
            if extraction_pit_cell(&ws, ix, iz, cx, cz) {
                continue;
            }
            let painted = mask.get(ix, iz);
            let in_preview = ws
                .floor_paint_preview
                .is_some_and(|(x0, z0, x1, z1)| ix >= x0 && ix <= x1 && iz >= z0 && iz <= z1);

            if in_preview {
                let mat = if ws.floor_paint_add {
                    preview_mat.clone()
                } else {
                    preview_remove_mat.clone()
                };
                commands.spawn((
                    FloorSlabPreview,
                    FloorSlab,
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(mat),
                    Transform::from_xyz(cx, floor_y - 0.04, cz),
                ));
                continue;
            }

            if painted {
                commands.spawn((
                    FloorSlabPainted,
                    FloorSlab,
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(paint_mat.clone()),
                    Transform::from_xyz(cx, floor_y - 0.05, cz),
                ));
            } else {
                commands.spawn((
                    FloorSlabGrid,
                    FloorSlab,
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(grid_mat.clone()),
                    Transform::from_xyz(cx, floor_y - 0.06, cz),
                ));
            }
        }
    }
}

fn floor_context(ws: &EditorWorkspace) -> (FloorMask, f32, f32, f32) {
    let grid = ws.grid();
    let floor_y = ws.floor_level as f32 * MOD_H;
    match ws.workflow {
        EditorWorkflow::MapMaker => {
            let mask = ws.map.floor_mask(ws.floor_level);
            (mask, grid.world_x0(), grid.world_z0(), floor_y)
        }
        EditorWorkflow::ModuleMaker => {
            let mask = ws.module.floor_mask_for(ws.floor_level);
            (mask, grid.world_x0(), grid.world_z0(), floor_y)
        }
        EditorWorkflow::SynthDressing => {
            let mask = ws.dressing.floor_mask.clone();
            (mask, grid.world_x0(), grid.world_z0(), floor_y)
        }
    }
}

pub fn clone_floor_mask(ws: &EditorWorkspace) -> (EditorWorkflow, i32, FloorMask) {
    match ws.workflow {
        EditorWorkflow::MapMaker => (
            EditorWorkflow::MapMaker,
            ws.floor_level,
            ws.map.floor_mask(ws.floor_level),
        ),
        EditorWorkflow::ModuleMaker => (
            EditorWorkflow::ModuleMaker,
            ws.floor_level,
            ws.module.floor_mask_for(ws.floor_level),
        ),
        EditorWorkflow::SynthDressing => (
            EditorWorkflow::SynthDressing,
            ws.floor_level,
            ws.dressing.floor_mask.clone(),
        ),
    }
}

pub fn paint_floor_rect(ws: &mut EditorWorkspace, raw_x0: u32, raw_z0: u32, raw_x1: u32, raw_z1: u32, add: bool) {
    let ix0 = raw_x0.min(raw_x1);
    let ix1 = raw_x0.max(raw_x1);
    let iz0 = raw_z0.min(raw_z1);
    let iz1 = raw_z0.max(raw_z1);
    let grid = ws.grid();
    match ws.workflow {
        EditorWorkflow::MapMaker => {
            let mask = ws.map.floor_mask_mut(ws.floor_level);
            for iz in iz0..=iz1 {
                for ix in ix0..=ix1 {
                    if ix < grid.cells_x && iz < grid.cells_z {
                        mask.set(ix, iz, add);
                    }
                }
            }
        }
        EditorWorkflow::ModuleMaker => {
            let level = ws.floor_level;
            let mask = ws.module.floor_mask_for_mut(level);
            for iz in iz0..=iz1 {
                for ix in ix0..=ix1 {
                    if ix < grid.cells_x && iz < grid.cells_z {
                        mask.set(ix, iz, add);
                    }
                }
            }
        }
        EditorWorkflow::SynthDressing => {
            for iz in iz0..=iz1 {
                for ix in ix0..=ix1 {
                    if ix < grid.cells_x && iz < grid.cells_z {
                        ws.dressing.floor_mask.set(ix, iz, add);
                    }
                }
            }
        }
    }
    ws.floor_dirty = true;
    ws.dirty = true;
}

pub fn cell_index_from_world(hit_x: f32, hit_z: f32, grid: &GridSpec) -> Option<(u32, u32)> {
    let x0 = grid.world_x0();
    let z0 = grid.world_z0();
    let ix = ((hit_x - x0) / KENNEY_CELL).floor() as i32;
    let iz = ((hit_z - z0) / KENNEY_CELL).floor() as i32;
    if ix < 0 || iz < 0 {
        return None;
    }
    let ix = ix as u32;
    let iz = iz as u32;
    if ix >= grid.cells_x || iz >= grid.cells_z {
        return None;
    }
    Some((ix, iz))
}

pub fn is_floor_tool(tool: EditorTool) -> bool {
    matches!(tool, EditorTool::FloorAdd | EditorTool::FloorRemove)
}
