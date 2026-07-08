//! Raw glTF geometry loading for the headless dedicated server (`--server`).
//!
//! Bevy's normal glTF pipeline (`AssetServer` + `SceneRoot` + async trimesh
//! bake in `level::build_kenney_trimesh_colliders`) needs `bevy_pbr`'s
//! render-app machinery, which only exists behind a real GPU/`RenderPlugin`.
//! A headless server never creates one, so it can never load a single Kenney
//! piece through that path. This module bypasses Bevy's asset system
//! entirely: it parses `.glb` files directly with the `gltf` crate (plain
//! geometry, no rendering) and bakes the exact same avian3d trimesh colliders
//! `level::world_trimesh` builds for the windowed client, synchronously,
//! at spawn time instead of over several async frames.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use avian3d::prelude::Collider;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;

use shared::kenney_pit::KenneyMeshCutouts;

/// Absolute path to `assets/`, matching `game_asset_root()` in `src/main.rs`.
pub fn assets_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
}

struct GlbDoc {
    document: gltf::Document,
    buffers: Vec<gltf::buffer::Data>,
}

/// Caches parsed `.glb` documents by path — pool maps reuse the same handful
/// of module GLBs (walls, corridors, stairs...) hundreds of times over.
#[derive(Resource, Default)]
pub struct GlbGeometryCache {
    docs: HashMap<PathBuf, Option<GlbDoc>>,
}

impl GlbGeometryCache {
    fn load(&mut self, path: &Path) -> Option<&GlbDoc> {
        self.docs
            .entry(path.to_path_buf())
            .or_insert_with(|| {
                let gltf = gltf::Gltf::open(path)
                    .map_err(|e| warn!("headless gltf: failed to open {}: {e}", path.display()))
                    .ok()?;
                let base = path.parent();
                let buffers = gltf::import_buffers(&gltf.document, base, gltf.blob.clone())
                    .map_err(|e| warn!("headless gltf: failed to read buffers {}: {e}", path.display()))
                    .ok()?;
                Some(GlbDoc {
                    document: gltf.document,
                    buffers,
                })
            })
            .as_ref()
    }

    /// Bake world-space trimesh colliders for one placed Kenney piece,
    /// matching what the windowed client's `SceneRoot` + `world_trimesh`
    /// would produce for the same (path, transform, cutouts).
    pub fn colliders_for_piece(
        &mut self,
        path: &Path,
        world: Mat4,
        cutouts: &KenneyMeshCutouts,
    ) -> Vec<Collider> {
        let Some(doc) = self.load(path) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for scene in doc.document.scenes() {
            for node in scene.nodes() {
                walk_node(node, world, &doc.buffers, cutouts, &mut out);
            }
        }
        out
    }
}

fn walk_node(
    node: gltf::scene::Node,
    parent: Mat4,
    buffers: &[gltf::buffer::Data],
    cutouts: &KenneyMeshCutouts,
    out: &mut Vec<Collider>,
) {
    let local = Mat4::from_cols_array_2d(&node.transform().matrix());
    let world = parent * local;
    if let Some(mesh) = node.mesh() {
        for prim in mesh.primitives() {
            let reader = prim.reader(|b| buffers.get(b.index()).map(|d| d.0.as_slice()));
            let Some(positions) = reader.read_positions() else {
                continue;
            };
            let positions: Vec<[f32; 3]> = positions.collect();
            if positions.is_empty() {
                continue;
            }
            let indices: Vec<u32> = match reader.read_indices() {
                Some(idx) => idx.into_u32().collect(),
                None => (0..positions.len() as u32).collect(),
            };

            let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
            mesh.insert_attribute(
                Mesh::ATTRIBUTE_POSITION,
                VertexAttributeValues::Float32x3(positions),
            );
            mesh.insert_indices(Indices::U32(indices));

            let gt = GlobalTransform::from(world);
            if let Some(collider) = crate::level::world_trimesh(&mesh, &gt, cutouts) {
                out.push(collider);
            }
        }
    }
    for child in node.children() {
        walk_node(child, world, buffers, cutouts, out);
    }
}

/// Bake colliders for one placed Kenney piece (stem/kit + world transform),
/// resolving the same `models/<kit>/<stem>.glb` path the client's
/// `SceneRoot` loader uses.
pub fn bake_piece_colliders(
    cache: &mut GlbGeometryCache,
    stem: &str,
    kit: &str,
    transform: Transform,
    cutouts: &KenneyMeshCutouts,
) -> Vec<Collider> {
    let rel = shared::editor_catalog::glb_asset_path_in_kit(stem, kit);
    let abs = assets_root().join(rel);
    let world = Mat4::from_scale_rotation_translation(
        transform.scale,
        transform.rotation,
        transform.translation,
    );
    cache.colliders_for_piece(&abs, world, cutouts)
}
