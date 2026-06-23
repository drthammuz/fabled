//! Server-side level instantiation: turns the shared `LevelDef` into
//! physics entities. Supports reload when the run transitions stretches.

use avian3d::{math::*, prelude::*};
use bevy::gltf::GltfAssetLabel;
use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy_replicon::prelude::*;
use shared::config;
use shared::items;
use shared::kenney_catalog::{self, quantize_yaw};
use shared::kenney_layout::KenneyLayout;
use shared::level::{self, LevelDef, PropDef, MOD_H};
use shared::props::{Grabbable, PropShape};
use shared::protocol::{Item, NetTransform};
use shared::{EditorMode, KenneyPlaytestGeneration, TestMapStyle, TestMode, CityViewMode};

use crate::combat::{EnemyBrain, Health};
use crate::liquids;
use shared::protocol::Enemy;

/// Marks geometry/props/items spawned by the current level (despawned on reload).
#[derive(Component)]
pub struct LevelEntity;

/// PostStartup level load finished (static colliders + Kenney mesh colliders).
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LevelReady;

/// Temporary scene instance while GLB meshes load for trimesh collider baking.
#[derive(Component, Clone)]
pub struct KenneyColliderScene {
    pub stem: String,
    pub mesh_cutouts: shared::kenney_pit::KenneyMeshCutouts,
    pub group_id: Option<u32>,
    pub floor: i32,
}

/// Baked Kenney trimesh metadata for selective hub culling.
#[derive(Component, Clone, Copy)]
pub struct KenneyPieceMeta {
    pub group_id: Option<u32>,
    pub floor: i32,
}

/// Marks baked Kenney floor cell colliders (rebuilt on playtest reload).
#[derive(Component)]
pub struct KenneyFloorCell;

/// Procedural stretch-level static colliders (sewer walls/floors). Hidden during
/// editor Kenney playtest — they overlap the procgen map near world origin.
#[derive(Component)]
pub struct StretchStaticCollider;

/// Generation stamp so async trimesh bakes ignore stale `KenneyColliderScene`s.
#[derive(Component, Clone, Copy)]
pub struct KenneyColliderEpoch(pub u32);

#[derive(Component, Clone, Copy)]
pub struct KenneyInstanceTag {
    pub instance_id: u32,
}

#[derive(Component, Clone, Copy)]
pub struct KenneyFloorCellMeta {
    pub floor: i32,
    pub map_ix: u32,
    pub map_iz: u32,
}
#[derive(Resource, Default)]
pub struct LoadedLevel {
    pub id: String,
}

#[derive(Resource, Clone, Default)]
pub struct KenneyLayoutCache(pub KenneyLayout);

#[derive(Resource, Default)]
struct KenneyColliderGeneration(u32);

#[derive(Resource, Clone)]
struct StretchLevelSource {
    id: String,
    seed: u64,
}

pub struct ServerLevelPlugin;

impl Plugin for ServerLevelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadedLevel>()
            .init_resource::<KenneyLayoutCache>()
            .init_resource::<KenneyColliderGeneration>()
            .init_resource::<KenneyPlaytestGeneration>()
            // PostStartup runs after Startup commands are flushed, so RunState
            // (spawned by RunPlugin at Startup) is available here.
            .add_systems(
                PostStartup,
                (
                    initial_load,
                    crate::map_stream::init_kenney_stream,
                    spawn_kenney_layout_colliders,
                    spawn_kenney_floor_colliders,
                    spawn_city_colliders,
                )
                    .chain()
                    .in_set(LevelReady),
            )
            // PostUpdate: reload before bake so despawn commands flush before we
            // walk `KenneyColliderScene` (Update→PostUpdate ordering).
            .add_systems(
                PostUpdate,
                (
                    reload_kenney_playtest,
                    build_kenney_trimesh_colliders.after(reload_kenney_playtest),
                    sync_hidden_door_seals.after(reload_kenney_playtest),
                )
                    .chain()
                    .after(TransformSystems::Propagate),
            );
    }
}

pub fn load_level(commands: &mut Commands, existing: &Query<Entity, With<LevelEntity>>, def: &LevelDef) {
    for entity in existing.iter() {
        commands.entity(entity).despawn();
    }
    spawn_level_content(commands, def);
}

fn initial_load(
    mut commands: Commands,
    existing: Query<Entity, With<LevelEntity>>,
    run: Query<&shared::run::RunState>,
) {
    let (id, seed) = run.single()
        .map(|r| (r.level_id.clone(), r.run_seed))
        .unwrap_or_else(|_| ("sewer_entry".to_string(), 0));
    commands.insert_resource(StretchLevelSource {
        id: id.clone(),
        seed,
    });
    let def = level::level_by_id(&id, seed);
    load_level(&mut commands, &existing, &def);
}

/// Loads Kenney GLB scenes for trimesh collider baking (`--test --kenney`).
fn spawn_kenney_layout_colliders(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    test: Option<Res<TestMode>>,
    editor: Option<Res<EditorMode>>,
    mut layout_cache: ResMut<KenneyLayoutCache>,
    stream: Option<Res<crate::map_stream::KenneyStreamWorld>>,
) {
    if test.as_ref().is_some_and(|t| t.style == TestMapStyle::Kenney) {
        if editor.is_some() {
            layout_cache.0 = shared::map_pool::play_layout(true);
            spawn_kenney_piece_scenes(&mut commands, &asset_server, test.as_deref(), editor.as_deref(), 0);
            return;
        }
        if let Some(world) = stream.as_ref() {
            layout_cache.0 = world.active.to_world_layout();
            crate::map_stream::spawn_stream_geometries(&mut commands, &asset_server, world);
            return;
        }
        if let Some((pool, active)) = shared::map_pool::bootstrap_active_map() {
            layout_cache.0 = active.to_world_layout();
            let world = crate::map_stream::KenneyStreamWorld {
                pool,
                active,
                candidates: Vec::new(),
                epoch: 1,
            };
            crate::map_stream::spawn_stream_geometries(&mut commands, &asset_server, &world);
            commands.insert_resource(world);
            return;
        }
        layout_cache.0 = shared::map_pool::test_play_layout();
    }
    spawn_kenney_piece_scenes(&mut commands, &asset_server, test.as_deref(), editor.as_deref(), 0);
}

fn spawn_kenney_floor_colliders(
    mut commands: Commands,
    test: Option<Res<TestMode>>,
    editor: Option<Res<EditorMode>>,
    stream: Option<Res<crate::map_stream::KenneyStreamWorld>>,
) {
    if editor.is_none() && stream.is_some() && stream_enabled(test.as_deref()) {
        return;
    }
    spawn_kenney_floor_cells(&mut commands, test.as_deref(), editor.as_deref(), 0);
}

fn stream_enabled(test: Option<&TestMode>) -> bool {
    crate::map_stream::stream_enabled(test)
}

pub fn kenney_mesh_covers_cell(
    layout: &KenneyLayout,
    map_ix: u32,
    map_iz: u32,
    floor_level: i32,
) -> bool {
    let cell = layout.grid_unit_m;
    let cx = layout.map_world_x0() + (map_ix as f32 + 0.5) * cell;
    let cz = layout.map_world_z0() + (map_iz as f32 + 0.5) * cell;
    // Below ground, any floor-bearing piece (room shell or template-floor tile) counts as
    // coverage. This prevents spawn_kenney_floor_cells from spawning a redundant cuboid
    // on top of a template-floor tile's own trimesh collider.
    if floor_level < 0 {
        return layout.pieces.iter().any(|p| {
            p.floor as i32 == floor_level
                && !kenney_skip_piece_collider(p, layout)
                && shared::kenney_pit::carves_floor(&p.stem)
                && kenney_piece_contains_xz(p, cx, cz)
        });
    }
    // RESTORED (b611031): any collidable, non-skipped piece covering this cell counts
    // as coverage — INCLUDING template-floor's own baked trimesh. That trimesh sits
    // flush at the floor surface and reliably catches the player; the cell-cuboid
    // path (template-floor routed to KenneyFloorCell) let players fall through solid
    // interior tiles. Cuboids now only fill BARE mask cells with no floor piece.
    layout.pieces.iter().any(|p| {
        if p.floor as i32 != floor_level {
            return false;
        }
        if kenney_skip_piece_collider(p, layout) {
            return false;
        }
        let collide = kenney_catalog::piece(&p.stem)
            .map(|c| c.collide_default)
            .unwrap_or(true);
        if !collide {
            return false;
        }
        kenney_piece_contains_xz(p, cx, cz)
    })
}

fn kenney_piece_contains_xz(p: &shared::kenney_layout::KenneyPlacement, cx: f32, cz: f32) -> bool {
    let (nx, nz) = kenney_catalog::piece_grid_size(&p.stem);
    let (wx, wz) = kenney_catalog::rotated_grid_size(nx, nz, p.yaw);
    let half_w = wx * kenney_catalog::KENNEY_CELL * 0.5;
    let half_d = wz * kenney_catalog::KENNEY_CELL * 0.5;
    (p.x - cx).abs() <= half_w + 0.05 && (p.z - cz).abs() <= half_d + 0.05
}

fn spawn_kenney_floor_cells(
    commands: &mut Commands,
    test: Option<&TestMode>,
    editor: Option<&EditorMode>,
    epoch: u32,
) {
    let Some(test) = test else {
        return;
    };
    if test.style != TestMapStyle::Kenney {
        return;
    }
    let layout = shared::map_pool::play_layout(editor.is_some());
    if layout.floors.is_empty() {
        return;
    }
    let cell = layout.grid_unit_m;
    // Walkable surface of every floor-bearing GLB (corridor, template-floor) sits at
    // the piece floor_y = level*MOD_H + 0.002 (their meshes have y=0 at the floor).
    // The cell cuboid rests its TOP flush with that surface (centre = surface - half),
    // so floor-tile cells line up with the corridor floor instead of standing proud.
    const FLOOR_HALF_H: f32 = 0.25;
    const FLOOR_SURFACE_Y: f32 = 0.002;
    let mut n = 0u32;
    for (level, mask) in &layout.floors {
        let y = *level as f32 * MOD_H;
        let x0 = mask.world_x0();
        let z0 = mask.world_z0();
        for iz in 0..mask.cells_z {
            for ix in 0..mask.cells_x {
                if !mask.get(ix, iz) {
                    continue;
                }
                if kenney_mesh_covers_cell(&layout, ix, iz, *level) {
                    continue;
                }
                let cx = x0 + (ix as f32 + 0.5) * cell;
                let cz = z0 + (iz as f32 + 0.5) * cell;
                commands.spawn((
                    LevelEntity,
                    KenneyFloorCell,
                    KenneyColliderEpoch(epoch),
                    KenneyFloorCellMeta {
                        floor: *level,
                        map_ix: ix,
                        map_iz: iz,
                    },
                    RigidBody::Static,
                    Collider::cuboid(cell * 0.5, FLOOR_HALF_H, cell * 0.5),
                    // Top flush with the floor surface: centre = surface - half-height.
                    Transform::from_translation(Vec3::new(
                        cx,
                        y + FLOOR_SURFACE_Y - FLOOR_HALF_H,
                        cz,
                    )),
                ));
                n += 1;
            }
        }
    }
    info!("kenney floor: {} cell colliders from {} levels", n, layout.floors.len());
}

fn reload_kenney_playtest(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    test: Option<Res<TestMode>>,
    editor: Option<Res<EditorMode>>,
    generation: Res<KenneyPlaytestGeneration>,
    stretch_src: Option<Res<StretchLevelSource>>,
    mut last: ResMut<KenneyColliderGeneration>,
    mut layout_cache: ResMut<KenneyLayoutCache>,
    kenney_collision: Query<
        Entity,
            Or<(
                With<KenneyPieceMeta>,
                With<KenneyColliderScene>,
                With<KenneyFloorCell>,
                With<HiddenDoorSeal>,
            )>,
    >,
    stretch_statics: Query<Entity, With<StretchStaticCollider>>,
) {
    let Some(test) = test else {
        return;
    };
    if last.0 == generation.0 {
        return;
    }
    last.0 = generation.0;
    let epoch = generation.0;

    for e in &kenney_collision {
        commands.entity(e).despawn();
    }

    if test.style != TestMapStyle::Kenney {
        // Leaving Kenney playtest — restore procedural stretch statics for editor.
        if editor.is_some() && stretch_statics.is_empty() {
            if let Some(src) = stretch_src.as_ref() {
                let def = level::level_by_id(&src.id, src.seed);
                spawn_stretch_static_colliders(&mut commands, &def);
            }
        }
        return;
    }

    // Entering Kenney playtest — hide stretch statics that overlap the procgen map.
    for e in &stretch_statics {
        commands.entity(e).despawn();
    }

    layout_cache.0 = shared::map_pool::play_layout(editor.is_some());
    spawn_kenney_floor_cells(&mut commands, Some(&test), editor.as_deref(), epoch);
    spawn_kenney_piece_scenes(
        &mut commands,
        &asset_server,
        Some(&test),
        editor.as_deref(),
        epoch,
    );
    spawn_hidden_door_seals(
        &mut commands,
        Some(&test),
        editor.as_deref(),
        epoch,
        &layout_cache.0,
    );
}

/// Closed-state physics slab for hidden-room gate-doors (removed while "open").
#[derive(Component)]
struct HiddenDoorSeal {
    open: bool,
    half: Vec3,
}

fn spawn_hidden_door_seals(
    commands: &mut Commands,
    test: Option<&TestMode>,
    editor: Option<&EditorMode>,
    epoch: u32,
    layout: &KenneyLayout,
) {
    let Some(test) = test else { return };
    if test.style != TestMapStyle::Kenney {
        return;
    }
    let _ = editor;
    let mut n = 0u32;
    for p in &layout.pieces {
        if !p.tags.iter().any(|t| t == "hidden_entrance") {
            continue;
        }
        if !matches!(p.stem.as_str(), "gate-door" | "gate-door-window") {
            continue;
        }
        let yaw = quantize_yaw(p.yaw);
        let (hx, hy, hz) = shared::hidden_door::seal_cuboid_half_extents(yaw);
        let half = Vec3::new(hx, hy, hz);
        let y = shared::hidden_door::seal_center_y(p.floor);
        commands.spawn((
            LevelEntity,
            HiddenDoorSeal { open: false, half },
            KenneyColliderEpoch(epoch),
            RigidBody::Static,
            Collider::cuboid(hx, hy, hz),
            Transform::from_translation(Vec3::new(p.x, y, p.z))
                .with_rotation(shared::kenney_layout::placement_rotation(yaw, false)),
        ));
        n += 1;
    }
    if n > 0 {
        info!("kenney layout reload: {n} hidden-door seal collider(s)");
    }
}

fn sync_hidden_door_seals(
    mut commands: Commands,
    players: Query<
        &Transform,
        Or<(
            With<crate::players::EditorPlaytestPlayer>,
            With<crate::character::CharacterController>,
        )>,
    >,
    mut seals: Query<(Entity, &Transform, &mut HiddenDoorSeal, Option<&Collider>)>,
) {
    for (entity, seal_tf, mut seal, collider) in &mut seals {
        let door_pos = seal_tf.translation;
        let min_dist = players
            .iter()
            .map(|p| p.translation.distance(door_pos))
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(f32::MAX);

        if !seal.open && min_dist < shared::hidden_door::PROXIMITY_OPEN_M {
            seal.open = true;
        } else if seal.open && min_dist > shared::hidden_door::PROXIMITY_CLOSE_M {
            seal.open = false;
        }

        let (hx, hy, hz) = (seal.half.x, seal.half.y, seal.half.z);
        if seal.open && collider.is_some() {
            commands.entity(entity).remove::<Collider>();
        } else if !seal.open && collider.is_none() {
            commands.entity(entity).insert(Collider::cuboid(hx, hy, hz));
        }
    }
}

fn spawn_kenney_piece_scenes(
    commands: &mut Commands,
    asset_server: &AssetServer,
    test: Option<&TestMode>,
    editor: Option<&EditorMode>,
    epoch: u32,
) {
    let Some(test) = test else {
        return;
    };
    if test.style != TestMapStyle::Kenney {
        return;
    }
    let layout = shared::map_pool::play_layout(editor.is_some());
    let mut n = 0u32;
    for p in &layout.pieces {
        let collide = kenney_catalog::piece(&p.stem)
            .map(|x| x.collide_default)
            .unwrap_or(true);
        if !collide || kenney_skip_piece_collider(p, &layout) {
            continue;
        }
        // RESTORED (b611031): walkable template-floor tiles bake their own flush trimesh
        // here (not a KenneyFloorCell cuboid). The cuboid path let players fall through
        // solid interior floor; the trimesh — same path corridors use — holds reliably.
        let yaw = quantize_yaw(p.yaw);
        let floor_y = p.world_y();
        let path = shared::editor_catalog::glb_asset_path_in_kit(
            &p.stem,
            p.kit.as_deref().unwrap_or("space"),
        );
        let scale = p.scale.max(0.01);
        let mesh_cutouts = shared::kenney_pit::mesh_cutouts_for_piece(
            &p.stem,
            p.floor,
            p.x,
            p.z,
            p.yaw,
            layout.extraction_xz.map(|[ex, ez]| Vec2::new(ex, ez)),
            layout.floors.get(&p.floor),
            p.ceiling,
        );
        commands.spawn((
            LevelEntity,
            KenneyColliderScene {
                stem: p.stem.clone(),
                mesh_cutouts,
                group_id: p.group_id,
                floor: p.floor,
            },
            KenneyColliderEpoch(epoch),
            KenneyPieceMeta {
                group_id: p.group_id,
                floor: p.floor,
            },
            SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(path))),
            Transform::from_translation(Vec3::new(p.x, floor_y, p.z))
                .with_rotation(shared::kenney_layout::placement_rotation(yaw, p.ceiling))
                .with_scale(Vec3::splat(scale)),
        ));
        n += 1;
    }
    info!("kenney layout reload: {} mesh colliders queued", n);
}

/// Skip colliders on floor-0 pieces covering the pit tile, except the hole frame
/// and room shells (those get a centre-floor cutout when baked).
pub fn kenney_skip_piece_collider(
    p: &shared::kenney_layout::KenneyPlacement,
    layout: &KenneyLayout,
) -> bool {
    // Ceiling / roof slabs are visual-only (template-floor one level above walkable).
    if shared::kenney_layout::is_ceiling_slab(p) {
        return true;
    }
    // The hole frame (template-floor-hole) is a raised rim with an open centre: it
    // SHOULD collide so you stand on the rim and fall through the middle. Never skip it.
    if matches!(
        p.stem.as_str(),
        "template-floor-hole" | "template-floor-layer-hole"
    ) {
        return false;
    }
    {
        let (ex, ez) = layout
            .extraction_xz
            .map(|[a, b]| (a, b))
            .unwrap_or((f32::INFINITY, f32::INFINITY));
        if shared::kenney_pit::skip_hub_passage_collider(
            &p.stem,
            p.floor,
            p.x,
            p.z,
            ex,
            ez,
            layout.floors.get(&p.floor),
        ) {
            return true;
        }
    }
    if p.floor != 0 {
        return false;
    }
    if shared::kenney_pit::is_room_shell(&p.stem) {
        return false;
    }
    let Some([ex, ez]) = layout.extraction_xz else {
        return false;
    };
    if p.stem.starts_with("template-wall") {
        return false;
    }
    let Some(cat) = kenney_catalog::piece(&p.stem) else {
        return false;
    };
    if !cat.collide_default {
        return false;
    }
    let (nx, nz) = kenney_catalog::piece_grid_size(&p.stem);
    let (wx, wz) = kenney_catalog::rotated_grid_size(nx, nz, p.yaw);
    let half_w = wx * kenney_catalog::KENNEY_CELL * 0.5;
    let half_d = wz * kenney_catalog::KENNEY_CELL * 0.5;
    let lx = p.x - ex;
    let lz = p.z - ez;
    let tile = kenney_catalog::KENNEY_CELL * 0.5;
    (lx - half_w) < tile
        && (lx + half_w) > -tile
        && (lz - half_d) < tile
        && (lz + half_d) > -tile
}

/// Trimesh colliders for the cyberpunk city GLB (`--city`).
fn spawn_city_colliders(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    city: Option<Res<CityViewMode>>,
) {
    if city.is_none() {
        return;
    }
    commands.spawn((
        LevelEntity,
        KenneyColliderScene {
            stem: "cyberpunk_city".into(),
            mesh_cutouts: shared::kenney_pit::KenneyMeshCutouts::default(),
            group_id: None,
            floor: 0,
        },
        SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(
            "models/misc/cyberpunk_city.glb",
        ))),
        Transform::IDENTITY,
    ));
    info!("city: cyberpunk_city.glb mesh colliders queued (loads async)");
}

/// Bake trimesh colliders from loaded Kenney meshes (matches visible geometry).
fn build_kenney_trimesh_colliders(
    mut commands: Commands,
    generation: Res<KenneyPlaytestGeneration>,
    meshes: Res<Assets<Mesh>>,
    scenes: Query<(Entity, &KenneyColliderScene, Option<&KenneyColliderEpoch>, &Children)>,
    children_q: Query<&Children>,
    mesh_q: Query<(&Mesh3d, &GlobalTransform)>,
) {
    let epoch = generation.0;
    for (scene, meta, scene_epoch, _) in &scenes {
        if scene_epoch.is_some_and(|e| e.0 != epoch) {
            commands.entity(scene).despawn();
            continue;
        }
        let mesh_ents: Vec<(Entity, &Mesh3d, &GlobalTransform)> = children_q
            .iter_descendants(scene)
            .filter_map(|e| mesh_q.get(e).ok().map(|(m, gt)| (e, m, gt)))
            .collect();
        if mesh_ents.is_empty() {
            continue;
        }
        if mesh_ents.iter().any(|(_, m, _)| meshes.get(&m.0).is_none()) {
            continue;
        }

        let mut collider_count = 0u32;
        for (_, mesh3d, gt) in &mesh_ents {
            let Some(mesh) = meshes.get(&mesh3d.0) else {
                continue;
            };
            let Some(collider) = world_trimesh(mesh, gt, &meta.mesh_cutouts) else {
                continue;
            };
            commands.spawn((
                LevelEntity,
                KenneyColliderEpoch(epoch),
                KenneyPieceMeta {
                    group_id: meta.group_id,
                    floor: meta.floor,
                },
                RigidBody::Static,
                collider,
                Transform::default(),
            ));
            collider_count += 1;
        }

        info!(
            "kenney collider '{}': {} trimesh(s) from mesh",
            meta.stem, collider_count
        );
        commands.entity(scene).despawn();
    }
}

fn world_trimesh(
    mesh: &Mesh,
    gt: &GlobalTransform,
    cutouts: &shared::kenney_pit::KenneyMeshCutouts,
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

    let mut indices = indices;

    if !cutouts.is_empty() {
        indices = shared::kenney_pit::apply_mesh_cutouts(&verts, &indices, cutouts);
    }

    if indices.is_empty() {
        return None;
    }

    Some(Collider::trimesh(verts, indices))
}

fn prop_collider(shape: &PropShape) -> Collider {
    match *shape {
        PropShape::Crate { size } => Collider::cuboid(size.x, size.y, size.z),
        PropShape::Ball { radius } => Collider::sphere(radius),
    }
}

fn spawn_stretch_static_colliders(commands: &mut Commands, level: &LevelDef) {
    let mut channel_id = 0u32;
    for def in &level.statics {
        if def.kind == level::StaticKind::SewerWater {
            liquids::spawn_water_volume(commands, def, channel_id);
            channel_id += 1;
            continue;
        }
        if matches!(def.kind, level::StaticKind::SewerPipe | level::StaticKind::SewerPipeBend) {
            let (radius, length, rotation) = match def.kind {
                level::StaticKind::SewerPipe => level::straight_pipe(def),
                _ => level::bend_pipe(def),
            };
            commands.spawn((
                LevelEntity,
                StretchStaticCollider,
                RigidBody::Static,
                Collider::cylinder(radius, length),
                Transform::from_translation(def.position).with_rotation(rotation),
            ));
            continue;
        }
        if matches!(
            def.kind,
            level::StaticKind::Gable
                | level::StaticKind::Neon
                | level::StaticKind::DoorMarker
                | level::StaticKind::SewerBrace
                | level::StaticKind::SewerArch
                | level::StaticKind::SewerWalkway
                | level::StaticKind::PipeBore
                | level::StaticKind::PipeRing
                | level::StaticKind::PipeElbow
        ) {
            continue;
        }
        commands.spawn((
            LevelEntity,
            StretchStaticCollider,
            RigidBody::Static,
            Collider::cuboid(def.size.x, def.size.y, def.size.z),
            Transform::from_translation(def.position).with_rotation(def.rotation),
        ));
    }
}

fn spawn_level_content(commands: &mut Commands, level: &LevelDef) {
    spawn_stretch_static_colliders(commands, level);

    for PropDef {
        shape,
        position,
        density,
    } in &level.props
    {
        commands.spawn((
            LevelEntity,
            Replicated,
            Grabbable,
            RigidBody::Dynamic,
            prop_collider(shape),
            ColliderDensity(*density),
            Friction::new(0.35),
            Restitution::new(config::PROP_RESTITUTION),
            AngularDamping(config::PROP_ANGULAR_DAMPING),
            *shape,
            NetTransform {
                translation: *position,
                rotation: Quat::IDENTITY,
            },
            Transform::from_translation(*position),
        ));
    }

    // Credit pickups scattered in the stretch.
    for (i, pos) in level.item_spawns.iter().enumerate() {
        let amount = 8 + (i as u32) * 4;
        spawn_world_item(commands, items::credits(amount), *pos, Vec3::ZERO, true);
    }

    for pos in &level.enemy_spawns {
        spawn_enemy(commands, *pos);
    }

    info!(
        "level '{}' spawned: {} statics, {} props, {} pickups, {} enemies",
        level.id,
        level.statics.len(),
        level.props.len(),
        level.item_spawns.len(),
        level.enemy_spawns.len()
    );
}

fn spawn_enemy(commands: &mut Commands, position: Vec3) {
    commands.spawn((
        LevelEntity,
        Replicated,
        Enemy,
        EnemyBrain::at(position),
        Health {
            current: 40.0,
            max: 40.0,
        },
        RigidBody::Kinematic,
        Collider::sphere(0.55),
        Transform::from_translation(position),
        NetTransform {
            translation: position,
            rotation: Quat::IDENTITY,
        },
    ));
}

/// Spawns a pickup item as a physics object in the world.
pub fn spawn_world_item(
    commands: &mut Commands,
    item: Item,
    position: Vec3,
    velocity: Vec3,
    level_owned: bool,
) -> Entity {
    let size = config::ITEM_SIZE;
    let mut entity = commands.spawn((
        Replicated,
        RigidBody::Dynamic,
        Collider::cuboid(size, size, size),
        ColliderDensity(80.0),
        Friction::new(0.5),
        Restitution::new(config::PROP_RESTITUTION),
        AngularDamping(config::PROP_ANGULAR_DAMPING),
        SweptCcd::default(),
        PropShape::Crate {
            size: Vec3::splat(size),
        },
        item,
        LinearVelocity(velocity.adjust_precision()),
        NetTransform {
            translation: position,
            rotation: Quat::IDENTITY,
        },
        Transform::from_translation(position),
    ));
    if level_owned {
        entity.insert(LevelEntity);
    }
    entity.id()
}

fn module_slot_ranges(col: u32, row: u32) -> (std::ops::RangeInclusive<u32>, std::ops::RangeInclusive<u32>) {
    let cells = shared::editor_map::CELLS_PER_MODULE;
    let x0 = col * cells;
    let z0 = row * cells;
    (x0..=x0 + cells - 1, z0..=z0 + cells - 1)
}

fn cull_branch_piece(commands: &mut Commands, meta: &KenneyPieceMeta, keep_gid: Option<u32>, entity: Entity) {
    let Some(gid) = meta.group_id else {
        return;
    };
    if !shared::kenney_hub::all_branch_gids().contains(&gid) {
        return;
    }
    if Some(gid) != keep_gid {
        commands.entity(entity).despawn();
    }
}

/// Despawn unchosen L2/L3/L4 branch colliders after the first physical commit.
pub fn cull_kenney_branch_groups(
    commands: &mut Commands,
    keep_exit: u8,
    pieces: &Query<(Entity, &KenneyPieceMeta)>,
    scenes: &Query<(Entity, &KenneyPieceMeta), With<KenneyColliderScene>>,
) {
    let keep_gid = shared::kenney_hub::branch_gid(keep_exit);
    for (entity, meta) in pieces.iter() {
        cull_branch_piece(commands, meta, keep_gid, entity);
    }
    for (entity, meta) in scenes.iter() {
        cull_branch_piece(commands, meta, keep_gid, entity);
    }
}

pub fn cull_kenney_branch_floors(
    commands: &mut Commands,
    keep_exit: u8,
    hub_slot: (u32, u32),
    west_slot: (u32, u32),
    floors: &Query<(Entity, &KenneyFloorCellMeta)>,
) {
    let (hub_x, hub_z) = module_slot_ranges(hub_slot.0, hub_slot.1);
    let (west_x, west_z) = module_slot_ranges(west_slot.0, west_slot.1);
    for (entity, meta) in floors.iter() {
        if meta.floor != -2 {
            continue;
        }
        let in_hub = hub_x.contains(&meta.map_ix) && hub_z.contains(&meta.map_iz);
        let in_west = west_x.contains(&meta.map_ix) && west_z.contains(&meta.map_iz);
        let keep = match keep_exit {
            2 => false,
            3 => in_hub,
            4 => in_west,
            _ => true,
        };
        if !keep {
            commands.entity(entity).despawn();
        }
    }
}

/// Despawn floor-0 stretch colliders after the whole party committed to an exit.
pub fn cull_kenney_l1(
    commands: &mut Commands,
    pieces: &Query<(Entity, &KenneyPieceMeta)>,
    scenes: &Query<(Entity, &KenneyPieceMeta), With<KenneyColliderScene>>,
    floors: &Query<(Entity, &KenneyFloorCellMeta)>,
) {
    for (entity, meta) in pieces.iter() {
        if meta.floor == 0 {
            commands.entity(entity).despawn();
        }
    }
    for (entity, meta) in scenes.iter() {
        if meta.floor == 0 {
            commands.entity(entity).despawn();
        }
    }
    for (entity, meta) in floors.iter() {
        if meta.floor == 0 {
            commands.entity(entity).despawn();
        }
    }
}
