//! Developer-only: arranges Kenney GLBs from `userinput/kenney_layout.json`.
//!
//! Swaps in the cyberpunk atlas once meshes load. Physics boxes are spawned on
//! the server (`server::level::spawn_kenney_layout_colliders`).

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::gltf::GltfLoaderSettings;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::math::{Affine2, Vec2};
use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;
use shared::editor_catalog::glb_asset_path;
use shared::kenney_catalog::{self, quantize_yaw};
use shared::kenney_hub;
use shared::kenney_layout::KenneyLayout;
use shared::kenney_pit;
use shared::level::{kenney_stairs_placement, MOD_H};
use shared::map_pool::{instances_from_stream_state, MountedMap, PoolIndex};
use shared::run::RunState;
use shared::{TestMapStyle, TestMode};
use shared::EditorMode;

pub struct TestShowcasePlugin;

impl Plugin for TestShowcasePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, spawn_showcase)
            .add_systems(Update, (build_modules, hub_cull_showcase, sync_stream_showcase).chain());
    }
}

/// Shared cyberpunk materials for Kenney GLBs (test showcase + editor).
#[derive(Resource, Clone)]
pub struct CyberMaterial(pub Handle<StandardMaterial>);

#[derive(Resource, Clone)]
pub struct CyberMaterialUnderside(pub Handle<StandardMaterial>);

#[derive(Resource, Clone)]
pub struct CyberMaterialCeiling(pub Handle<StandardMaterial>);

/// Double-sided pink (industrial-tint) ceiling for the pink zone roof.
#[derive(Resource, Clone)]
pub struct CyberMaterialPinkCeiling(pub Handle<StandardMaterial>);

/// Industrial middle zone — muddy brown substrate atlas (placeholder until sewer procgen).
#[derive(Resource, Clone)]
pub struct CyberMaterialIndustrial(pub Handle<StandardMaterial>);

#[derive(Resource, Clone)]
pub struct CyberLaserMaterial(pub Handle<StandardMaterial>);

/// Priesthood faction stone atlas — applied explicitly in the editor because
/// Blender re-exports often produce glTF materials Bevy fails to texture.
#[derive(Resource, Clone)]
pub struct PriesthoodMaterial(pub Handle<StandardMaterial>);

/// Synth / space_station atlas — deck + rail variants for z-fight control in dressing.
#[derive(Resource, Clone)]
pub struct SynthMaterial {
    pub base: Handle<StandardMaterial>,
    /// Slightly biased toward camera so deck wins over coplanar rails.
    pub deck: Handle<StandardMaterial>,
    /// Slightly biased away so rails sit under deck when overlapping.
    pub rail: Handle<StandardMaterial>,
    /// Floor props on the 1.2 m deck — wins over coplanar floor substrate.
    pub prop: Handle<StandardMaterial>,
    /// Ground floor tiles — base look plus a tiled scratch detail map so the floor
    /// reads as a surface instead of one flat colour.
    pub floor: Handle<StandardMaterial>,
}


/// Default space kit, or an explicit ``space`` kit tag.
pub fn uses_space_cyber_materials(kit: Option<&str>) -> bool {
    kit.is_none() || kit == Some("space")
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum KenneyMaterialSlot {
    /// Embedded GLB materials — dungeon stone, playtest space architecture, etc.
    NativeGlb,
    /// ``factions/priesthood`` — shared external colormap (Blender-safe).
    Priesthood,
    /// ``factions/synth`` — space_station colormap (Blender-safe).
    Synth,
    /// Synth floor / balcony deck — depth bias wins over rails.
    SynthDeck,
    /// Synth ground floor tiles — base look + tiled scratch detail map.
    SynthFloor,
    /// Synth railing — depth bias loses to deck when overlapping.
    SynthRail,
    /// Synth floor props (beds, desks) — depth bias wins over floor tiles.
    SynthProp,
    SpaceCyber,
    SpaceIndustrial,
    Ceiling,
    CeilingPink,
    Lasers,
}

/// Per-piece material routing: one consistent look within each kit + composition zone.
pub fn kenney_material_slot(
    kit: Option<&str>,
    _zone: Option<&str>,
    _playtest: bool,
    ceiling: bool,
    stem: &str,
) -> KenneyMaterialSlot {
    if stem == "gate-lasers" {
        return KenneyMaterialSlot::Lasers;
    }
    // Faction look comes from the FACTION (its own kit), never the zone position.
    if kit == Some("factions/priesthood") {
        return KenneyMaterialSlot::Priesthood;
    }
    if kit == Some("factions/synth") {
        // Stairs sit ON a floor block (coplanar base): bias them toward the camera so
        // the ramp wins instead of z-fighting / vanishing into the floor.
        if stem == "stairs" {
            return KenneyMaterialSlot::SynthProp;
        }
        if stem.starts_with("balcony-floor") {
            return KenneyMaterialSlot::SynthDeck;
        }
        if shared::editor_catalog::is_synth_rail_stem(stem) {
            return KenneyMaterialSlot::SynthRail;
        }
        if shared::editor_catalog::is_synth_deck_prop_stem(stem) {
            return KenneyMaterialSlot::SynthProp;
        }
        // Floor blocks (ground + mezzanine deck) get the tiled scratch detail map.
        if stem == "floor" {
            return KenneyMaterialSlot::SynthFloor;
        }
        // Walls use the base synth colormap.
        return KenneyMaterialSlot::Synth;
    }
    // Non-space kits (dungeon stone, urban/necropolis native) keep their own material.
    if !uses_space_cyber_materials(kit) {
        return KenneyMaterialSlot::NativeGlb;
    }
    // Space-grammar fallback pieces (ceilings everywhere; floors of factions that
    // don't provide their own). NEUTRAL for every zone — pink no longer bleeds onto
    // whatever faction happens to sit in the "prev" zone.
    if ceiling {
        return KenneyMaterialSlot::Ceiling;
    }
    KenneyMaterialSlot::SpaceCyber
}

pub const EDITOR_BUILD_TAG: &str = "2026-06-24b";

pub fn init_kenney_materials(
    asset_server: &AssetServer,
    materials: &mut Assets<StandardMaterial>,
) -> (CyberMaterial, CyberLaserMaterial) {
    let base = asset_server.load("models/space/cyber_colormap.png");
    let emissive = asset_server.load("models/space/cyber_colormap_emissive.png");
    let mr = asset_server.load_with_settings(
        "models/space/cyber_colormap_mr.png",
        |s: &mut ImageLoaderSettings| s.is_srgb = false,
    );
    let cyber = materials.add(StandardMaterial {
        base_color: Color::srgb(1.35, 1.35, 1.4),
        base_color_texture: Some(base.clone()),
        metallic_roughness_texture: Some(mr.clone()),
        metallic: 0.05,
        perceptual_roughness: 0.72,
        emissive_texture: Some(emissive.clone()),
        emissive: LinearRgba::rgb(5.0, 5.0, 5.5),
        ..default()
    });
    let cyber_lasers = materials.add(StandardMaterial {
        base_color_texture: Some(base),
        metallic_roughness_texture: Some(mr),
        metallic: 1.0,
        perceptual_roughness: 1.0,
        emissive_texture: Some(emissive),
        emissive: LinearRgba::rgb(3.0, 0.0, 0.0),
        ..default()
    });
    (CyberMaterial(cyber), CyberLaserMaterial(cyber_lasers))
}

/// Editor PBR materials — low metallic so overhead fill light reads on pieces.
pub fn init_editor_kenney_materials(
    asset_server: &AssetServer,
    materials: &mut Assets<StandardMaterial>,
) -> (
    CyberMaterial,
    CyberLaserMaterial,
    CyberMaterialCeiling,
    CyberMaterialIndustrial,
    CyberMaterialPinkCeiling,
    PriesthoodMaterial,
    SynthMaterial,
) {
    let base = asset_server.load("models/space/cyber_colormap.png");
    let emissive = asset_server.load("models/space/cyber_colormap_emissive.png");
    let mr = asset_server.load_with_settings(
        "models/space/cyber_colormap_mr.png",
        |s: &mut ImageLoaderSettings| s.is_srgb = false,
    );
    let cyber = materials.add(StandardMaterial {
        base_color: Color::srgb(0.82, 0.84, 0.88),
        base_color_texture: Some(base.clone()),
        metallic_roughness_texture: Some(mr.clone()),
        metallic: 0.12,
        perceptual_roughness: 0.78,
        emissive_texture: Some(emissive.clone()),
        emissive: LinearRgba::rgb(0.2, 0.2, 0.22),
        ..default()
    });
    let industrial = materials.add(StandardMaterial {
        base_color: Color::srgb(0.68, 0.55, 0.42),
        base_color_texture: Some(base.clone()),
        metallic_roughness_texture: Some(mr.clone()),
        metallic: 0.10,
        perceptual_roughness: 0.82,
        emissive_texture: Some(emissive.clone()),
        emissive: LinearRgba::rgb(0.12, 0.10, 0.08),
        uv_transform: Affine2::from_scale_angle_translation(
            Vec2::splat(2.2),
            0.0,
            Vec2::new(0.42, 0.58),
        ),
        ..default()
    });
    let cyber_lasers = materials.add(StandardMaterial {
        base_color_texture: Some(base),
        metallic_roughness_texture: Some(mr),
        metallic: 0.1,
        perceptual_roughness: 0.7,
        emissive_texture: Some(emissive),
        emissive: LinearRgba::rgb(2.5, 0.4, 0.4),
        ..default()
    });
    let mut ceiling_mat = materials
        .get(&cyber)
        .cloned()
        .unwrap_or_default();
    ceiling_mat.cull_mode = None;
    let cyber_ceiling = materials.add(ceiling_mat);
    let mut pink_ceiling_mat = materials.get(&industrial).cloned().unwrap_or_default();
    pink_ceiling_mat.cull_mode = None;
    let pink_ceiling = materials.add(pink_ceiling_mat);
    let priesthood_colormap =
        asset_server.load("models/factions/priesthood/Textures/colormap.png");
    let mut priesthood_mat = StandardMaterial {
        base_color_texture: Some(priesthood_colormap),
        metallic: 0.0,
        perceptual_roughness: 1.0,
        ..default()
    };
    priesthood_mat.cull_mode = None;
    let priesthood = materials.add(priesthood_mat);
    let synth_colormap = asset_server.load("models/factions/synth/Textures/colormap.png");
    let mut synth_mat = StandardMaterial {
        base_color: Color::srgb(0.86, 0.84, 0.80),
        base_color_texture: Some(synth_colormap),
        metallic: 0.06,
        perceptual_roughness: 0.48,
        uv_transform: Affine2::from_scale_angle_translation(
            Vec2::splat(0.35),
            0.0,
            Vec2::new(0.12, 0.18),
        ),
        ..default()
    };
    synth_mat.cull_mode = None;
    let synth = materials.add(synth_mat.clone());
    let mut synth_deck_mat = synth_mat.clone();
    synth_deck_mat.depth_bias = 0.75;
    let synth_deck = materials.add(synth_deck_mat);
    let mut synth_rail_mat = synth_mat.clone();
    synth_rail_mat.depth_bias = -0.75;
    let synth_rail = materials.add(synth_rail_mat);
    // Ground floor: keep the synth colour but multiply in a tiled scratch/scuff map so the
    // floor isn't one flat colour. The floor GLB samples a tiny colormap swatch, so the
    // detail texture is set to Repeat and uv_transform tiles it ~2x across each 4 m tile
    // (24/15 ratio compensates the non-square swatch so scratches read square).
    let floor_detail = asset_server.load_with_settings(
        "models/factions/synth/Textures/floor_detail.png",
        |s: &mut ImageLoaderSettings| {
            s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                ..default()
            });
        },
    );
    let mut synth_floor_mat = synth_mat.clone();
    synth_floor_mat.base_color = Color::srgb(0.86, 0.84, 0.80);
    synth_floor_mat.base_color_texture = Some(floor_detail);
    synth_floor_mat.uv_transform =
        Affine2::from_scale_angle_translation(Vec2::new(16.0, 10.0), 0.0, Vec2::ZERO);
    let synth_floor = materials.add(synth_floor_mat);
    // Props + stairs sit ON a floor block; positive bias makes them win the coplanar
    // contact so beds/desks don't sink and stairs don't vanish into the deck.
    let mut synth_prop_mat = synth_mat;
    synth_prop_mat.depth_bias = 0.75;
    let synth_prop = materials.add(synth_prop_mat);
    (
        CyberMaterial(cyber),
        CyberLaserMaterial(cyber_lasers),
        CyberMaterialCeiling(cyber_ceiling),
        CyberMaterialIndustrial(industrial),
        CyberMaterialPinkCeiling(pink_ceiling),
        PriesthoodMaterial(priesthood),
        SynthMaterial {
            base: synth,
            deck: synth_deck,
            rail: synth_rail,
            prop: synth_prop,
            floor: synth_floor,
        },
    )
}

#[derive(Component)]
struct ModuleReady;

#[derive(Component, Clone, Copy)]
pub struct PieceTint(pub [f32; 3]);

#[derive(Component, Clone)]
pub struct PieceKit(pub String);

#[derive(Component)]
pub struct KenneyModule {
    pub name: &'static str,
    pub collide: bool,
    pub mesh_cutouts: kenney_pit::KenneyMeshCutouts,
    pub group_id: Option<u32>,
    pub floor: i32,
    pub ceiling: bool,
    pub kit: Option<&'static str>,
}

#[derive(Component)]
struct BranchBeacon;

#[derive(Component)]
struct HubCulled;

fn spawn_branch_beacons(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    layout: &KenneyLayout,
) {
    let mesh = meshes.add(Cuboid::new(0.35, MOD_H * 0.85, 0.35));
    for (key, branch) in &layout.branch_levels {
        let y = branch.floor as f32 * MOD_H + MOD_H * 0.42;
        let color = match key.as_str() {
            "2" => Color::srgb(0.3, 0.9, 0.45),
            "3" => Color::srgb(0.35, 0.75, 1.0),
            "4" => Color::srgb(1.0, 0.65, 0.25),
            _ => Color::srgb(0.9, 0.9, 0.9),
        };
        let mat = materials.add(StandardMaterial {
            base_color: color,
            emissive: LinearRgba::from(color) * 4.0,
            unlit: true,
            ..default()
        });
        commands.spawn((
            BranchBeacon,
            Mesh3d(mesh.clone()),
            MeshMaterial3d(mat),
            Transform::from_xyz(branch.x, y, branch.z),
        ));
        // Vertical guide from hub band down/up to the branch room centre.
        let guide = meshes.add(Cylinder::new(0.12, MOD_H * 2.0));
        let guide_y = y - MOD_H;
        commands.spawn((
            BranchBeacon,
            Mesh3d(guide),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color.with_alpha(0.35),
                emissive: LinearRgba::from(color) * 2.0,
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_xyz(branch.x, guide_y, branch.z),
        ));
    }
}

struct Placement {
    stem: &'static str,
    pos: Vec3,
    yaw: f32,
    scale: f32,
    collide: bool,
    mesh_cutouts: kenney_pit::KenneyMeshCutouts,
    group_id: Option<u32>,
    floor: i32,
    ceiling: bool,
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
        pos,
        yaw,
        scale,
        collide,
        mesh_cutouts,
        group_id,
        floor,
        ceiling,
    }
}

fn placements(style: TestMapStyle) -> Vec<Placement> {
    match style {
        TestMapStyle::Rusty => vec![],
        TestMapStyle::Kenney => kenney_placements(),
    }
}

fn kenney_placements() -> Vec<Placement> {
    if let Some(pool) = PoolIndex::load_from_disk() {
        let state = shared::run::MapStreamState {
            active_pool_id: pool.start_id().unwrap_or("map_001").to_string(),
            ..Default::default()
        };
        if let Some((active, candidates)) = instances_from_stream_state(&state, &pool) {
            return placements_from_instances(&active, &candidates);
        }
    }
    placements_from_layout(&shared::map_pool::test_play_layout())
}

fn placements_from_layout(layout: &KenneyLayout) -> Vec<Placement> {
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
        out.push(m(
            leak_stem(&p.stem),
            Vec3::new(p.x, p.world_y(), p.z),
            quantize_yaw(p.yaw),
            p.scale.max(0.01),
            collide,
            mesh_cutouts,
            p.group_id,
            p.floor,
            p.ceiling,
        ));
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

fn placements_from_instances(active: &MountedMap, candidates: &[MountedMap]) -> Vec<Placement> {
    let mut out = placements_from_mounted(active);
    for c in candidates {
        out.extend(placements_from_mounted(c));
    }
    out
}

fn placements_from_mounted(inst: &MountedMap) -> Vec<Placement> {
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
        out.push(m(
            leak_stem(&p.stem),
            inst.piece_translation(p),
            quantize_yaw(p.yaw),
            p.scale.max(0.01),
            collide,
            mesh_cutouts,
            p.group_id,
            p.floor,
            p.ceiling,
        ));
    }
    out
}

fn sync_stream_showcase(
    mut commands: Commands,
    test: Option<Res<TestMode>>,
    run: Query<&RunState>,
    modules: Query<Entity, With<KenneyModule>>,
    beacons: Query<Entity, With<BranchBeacon>>,
    mut last_epoch: Local<u32>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(test) = test else { return };
    if test.style != TestMapStyle::Kenney {
        return;
    }
    let Some(pool) = PoolIndex::load_from_disk() else {
        return;
    };
    let Ok(state) = run.single() else { return };
    if *last_epoch == 0 {
        *last_epoch = state.map_stream.epoch.max(1);
        return;
    }
    if state.map_stream.epoch == *last_epoch {
        return;
    }
    *last_epoch = state.map_stream.epoch;
    let Some((active, candidates)) = instances_from_stream_state(&state.map_stream, &pool) else {
        return;
    };
    for e in modules.iter().chain(beacons.iter()) {
        commands.entity(e).despawn();
    }
    let list = placements_from_instances(&active, &candidates);
    let layout = active.to_world_layout();
    for p in &list {
        commands.spawn((
            SceneRoot(asset_server.load_with_settings(
                GltfAssetLabel::Scene(0).from_asset(glb_asset_path(p.stem)),
                |s: &mut GltfLoaderSettings| s.load_meshes = RenderAssetUsages::all(),
            )),
            Transform::from_translation(p.pos)
                .with_rotation(Quat::from_rotation_y(p.yaw))
                .with_scale(Vec3::splat(p.scale)),
            KenneyModule {
                name: p.stem,
                collide: p.collide,
                mesh_cutouts: p.mesh_cutouts.clone(),
                group_id: p.group_id,
                floor: p.floor,
                ceiling: p.ceiling,
                kit: None,
            },
        ));
    }
    spawn_branch_beacons(&mut commands, &mut meshes, &mut materials, &layout);
    info!(
        "stream showcase rebuilt epoch {} ({} modules)",
        state.map_stream.epoch,
        list.len()
    );
}

fn leak_stem(stem: &str) -> &'static str {
    Box::leak(stem.to_string().into_boxed_str())
}

fn spawn_showcase(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    test: Option<Res<TestMode>>,
) {
    let Some(test) = test else {
        return;
    };
    if test.style != TestMapStyle::Kenney {
        return;
    }

    let (cyber, cyber_lasers, cyber_ceiling, cyber_industrial, pink_ceiling, priesthood, synth) =
        init_editor_kenney_materials(&asset_server, &mut materials);
    commands.insert_resource(cyber);
    commands.insert_resource(cyber_lasers);
    commands.insert_resource(cyber_ceiling);
    commands.insert_resource(cyber_industrial);
    commands.insert_resource(pink_ceiling);
    commands.insert_resource(priesthood);
    commands.insert_resource(synth);

    let list = placements(test.style);
    let layout = shared::map_pool::test_play_layout();
    for p in &list {
        commands.spawn((
            SceneRoot(asset_server.load_with_settings(
                GltfAssetLabel::Scene(0).from_asset(glb_asset_path(p.stem)),
                |s: &mut GltfLoaderSettings| s.load_meshes = RenderAssetUsages::all(),
            )),
            Transform::from_translation(p.pos)
                .with_rotation(Quat::from_rotation_y(p.yaw))
                .with_scale(Vec3::splat(p.scale)),
            KenneyModule {
                name: p.stem,
                collide: p.collide,
                mesh_cutouts: p.mesh_cutouts.clone(),
                group_id: p.group_id,
                floor: p.floor,
                ceiling: p.ceiling,
                kit: None,
            },
        ));
    }

    for (_key, branch) in &layout.branch_levels {
        let y = branch.floor as f32 * MOD_H + 3.5;
        commands.spawn((
            PointLight {
                intensity: 4_000_000.0,
                range: 40.0,
                shadows_enabled: false,
                color: Color::srgb(0.85, 0.92, 1.0),
                ..default()
            },
            Transform::from_xyz(branch.x, y, branch.z),
        ));
    }

    spawn_branch_beacons(&mut commands, &mut meshes, &mut materials, &layout);

    let floors: std::collections::HashMap<i32, usize> =
        list.iter().fold(std::collections::HashMap::new(), |mut m, p| {
            *m.entry(p.floor).or_insert(0) += 1;
            m
        });
    info!(
        "test showcase (kenney): {} modules — floors {:?} — branch_levels {}",
        list.len(),
        floors,
        layout.branch_levels.len()
    );
}

fn build_modules(
    mut commands: Commands,
    test: Option<Res<TestMode>>,
    editor: Option<Res<EditorMode>>,
    cyber: Option<Res<CyberMaterial>>,
    cyber_ceiling: Option<Res<CyberMaterialCeiling>>,
    cyber_lasers: Option<Res<CyberLaserMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    modules: Query<(Entity, &KenneyModule), Without<ModuleReady>>,
    children_q: Query<&Children>,
    mesh_q: Query<(&Mesh3d, &GlobalTransform)>,
) {
    if editor.is_some() {
        return;
    }
    let is_test = test.is_some();

    for (root, module) in &modules {
        let mesh_ents: Vec<Entity> = children_q
            .iter_descendants(root)
            .filter(|e| mesh_q.contains(*e))
            .collect();
        if mesh_ents.is_empty() {
            continue;
        }
        if mesh_ents.iter().any(|e| {
            let (m, _) = mesh_q.get(*e).unwrap();
            meshes.get(&m.0).is_none()
        }) {
            continue;
        }

        let mat = if module.name == "gate-lasers" {
            let Some(cyber_lasers) = cyber_lasers.as_ref() else { continue };
            cyber_lasers.0.clone()
        } else if module.ceiling {
            let Some(cyber_ceiling) = cyber_ceiling.as_ref() else { continue };
            cyber_ceiling.0.clone()
        } else {
            let Some(cyber) = cyber.as_ref() else { continue };
            cyber.0.clone()
        };

        for e in &mesh_ents {
            let (mesh3d, gt) = mesh_q.get(*e).unwrap();
            let mesh_handle = if !module.ceiling && !module.mesh_cutouts.is_empty() {
                if let Some(mesh) = meshes.get(&mesh3d.0).cloned() {
                    meshes.add(cut_kenney_mesh(&mesh, gt, &module.mesh_cutouts))
                } else {
                    mesh3d.0.clone()
                }
            } else {
                mesh3d.0.clone()
            };
            if mesh_handle != mesh3d.0 {
                commands.entity(*e).insert(Mesh3d(mesh_handle.clone()));
            }
            commands.entity(*e).insert(MeshMaterial3d(mat.clone()));

            // Ceiling slabs are visual-only: never give them a collider.
            if module.collide && !module.ceiling && !is_test {
                if let Some(mesh) = meshes.get(&mesh_handle) {
                    if let Some(collider) = world_trimesh(mesh, gt, &module.mesh_cutouts) {
                        commands.spawn((RigidBody::Static, collider, Transform::default()));
                    }
                }
            }
        }
        commands.entity(root).insert(ModuleReady);
    }
}

/// Hide branch / L1 modules when the server replicates hub commit state.
fn hub_cull_showcase(
    mut commands: Commands,
    run: Query<&RunState>,
    modules: Query<(Entity, &KenneyModule), Without<HubCulled>>,
) {
    let Ok(state) = run.single() else {
        return;
    };
    let commit = &state.hub_commit;
    // Keep all branch geometry visible while exploring the hub; cull only on departure.
    if !commit.l1_unloaded {
        return;
    }
    for (entity, module) in &modules {
        let mut cull = false;
        if let Some(keep) = commit.chosen_exit {
            if let Some(gid) = module.group_id {
                if kenney_hub::all_branch_gids().contains(&gid)
                    && Some(gid) != kenney_hub::branch_gid(keep)
                {
                    cull = true;
                }
            }
        }
        if module.floor == 0 {
            cull = true;
        }
        if cull {
            commands.entity(entity).despawn();
        }
    }
}

pub fn apply_room_shell_mesh_cutouts(
    commands: &mut Commands,
    root: Entity,
    stem: &str,
    floor: i32,
    root_gt: &GlobalTransform,
    extraction: Option<Vec2>,
    mask: Option<&shared::editor_map::FloorMask>,
    meshes: &mut Assets<Mesh>,
    children_q: &Query<&Children>,
    mesh_q: &Query<(&Mesh3d, &GlobalTransform)>,
) -> bool {
    if !kenney_pit::is_room_shell(stem) {
        return false;
    }
    let yaw = root_gt.rotation().to_euler(EulerRot::YXZ).0;
    let cutouts = kenney_pit::mesh_cutouts_for_piece(
        stem,
        floor,
        root_gt.translation().x,
        root_gt.translation().z,
        yaw,
        extraction,
        mask,
        false,
    );
    if cutouts.is_empty() {
        return false;
    }

    let mesh_ents: Vec<Entity> = children_q
        .iter_descendants(root)
        .filter(|e| mesh_q.contains(*e))
        .collect();
    if mesh_ents.is_empty() {
        return false;
    }
    if mesh_ents.iter().any(|e| {
        let (m, _) = mesh_q.get(*e).unwrap();
        meshes.get(&m.0).is_none()
    }) {
        return false;
    }

    for e in &mesh_ents {
        let (mesh3d, gt) = mesh_q.get(*e).unwrap();
        let mesh_handle = if let Some(mesh) = meshes.get(&mesh3d.0).cloned() {
            meshes.add(cut_kenney_mesh(&mesh, gt, &cutouts))
        } else {
            mesh3d.0.clone()
        };
        if mesh_handle != mesh3d.0 {
            commands.entity(*e).insert(Mesh3d(mesh_handle));
        }
    }
    true
}

pub fn cut_kenney_mesh(
    mesh: &Mesh,
    gt: &GlobalTransform,
    cutouts: &kenney_pit::KenneyMeshCutouts,
) -> Mesh {
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return mesh.clone();
    };
    let affine = gt.affine();
    let world: Vec<Vec3> = positions
        .iter()
        .map(|p| affine.transform_point3(Vec3::new(p[0], p[1], p[2])))
        .collect();
    let indices = mesh_triangle_indices(mesh);
    let kept = kenney_pit::apply_mesh_cutouts(&world, &indices, cutouts);
    let mut out = mesh.clone();
    out.insert_indices(Indices::U32(
        kept.iter().flat_map(|tri| tri.iter().copied()).collect(),
    ));
    out
}

pub fn cut_floor_mesh(mesh: &Mesh, gt: &GlobalTransform, cutouts: &[Vec2], floor: i32) -> Mesh {
    let ops = kenney_pit::KenneyMeshCutouts {
        floor,
        floor_holes: cutouts.to_vec(),
        ..Default::default()
    };
    cut_kenney_mesh(mesh, gt, &ops)
}

#[allow(dead_code)]
pub fn cut_pit_floor_mesh(mesh: &Mesh, gt: &GlobalTransform, ex: f32, ez: f32) -> Mesh {
    cut_floor_mesh(mesh, gt, &[Vec2::new(ex, ez)], 0)
}

fn mesh_triangle_indices(mesh: &Mesh) -> Vec<[u32; 3]> {
    match mesh.indices() {
        Some(Indices::U32(idx)) => idx.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect(),
        Some(Indices::U16(idx)) => idx
            .chunks_exact(3)
            .map(|c| [c[0] as u32, c[1] as u32, c[2] as u32])
            .collect(),
        None => (0..mesh.count_vertices() as u32)
            .collect::<Vec<_>>()
            .chunks_exact(3)
            .map(|c| [c[0], c[1], c[2]])
            .collect(),
    }
}

fn world_trimesh(
    mesh: &Mesh,
    gt: &GlobalTransform,
    cutouts: &kenney_pit::KenneyMeshCutouts,
) -> Option<Collider> {
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return None;
    };
    let affine = gt.affine();
    let verts: Vec<Vec3> = positions
        .iter()
        .map(|p| affine.transform_point3(Vec3::new(p[0], p[1], p[2])))
        .collect();

    let indices: Vec<[u32; 3]> = match mesh.indices() {
        Some(Indices::U32(idx)) => idx.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect(),
        Some(Indices::U16(idx)) => idx
            .chunks_exact(3)
            .map(|c| [c[0] as u32, c[1] as u32, c[2] as u32])
            .collect(),
        None => (0..verts.len() as u32)
            .collect::<Vec<_>>()
            .chunks_exact(3)
            .map(|c| [c[0], c[1], c[2]])
            .collect(),
    };

    if verts.is_empty() || indices.is_empty() {
        return None;
    }

    let indices = if cutouts.is_empty() {
        indices
    } else {
        kenney_pit::apply_mesh_cutouts(&verts, &indices, cutouts)
    };

    if indices.is_empty() {
        return None;
    }

    Some(Collider::trimesh(verts, indices))
}
