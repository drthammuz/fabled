//! Kenney map-pool streaming: spawn/despawn 5×5 maps under hub drop holes.

use avian3d::prelude::*;
use bevy::gltf::GltfAssetLabel;
use bevy::prelude::*;
use shared::kenney_catalog::{self, quantize_yaw};
use shared::kenney_layout::KenneyLayout;
use shared::level::MOD_H;
use shared::map_pool::{mount_offset_world, MountedMap, PoolIndex, PoolMapDocument};
use shared::protocol::{Player, PlayerAlive, PlayerName};
use shared::run::{RunPhase, RunState};
use shared::{TestMapStyle, TestMode, EditorMode};

use crate::level::{
    kenney_skip_piece_collider, KenneyColliderScene, KenneyFloorCell, KenneyFloorCellMeta,
    KenneyInstanceTag, KenneyLayoutCache, KenneyPieceMeta, LevelEntity,
};

#[derive(Resource)]
pub struct KenneyStreamWorld {
    pub pool: PoolIndex,
    pub active: MountedMap,
    pub candidates: Vec<MountedMap>,
    pub epoch: u32,
}

impl KenneyStreamWorld {
    pub fn all_instances(&self) -> impl Iterator<Item = &MountedMap> {
        std::iter::once(&self.active).chain(self.candidates.iter())
    }

    pub fn merged_layout(&self) -> KenneyLayout {
        let mut out = self.active.to_world_layout();
        for c in &self.candidates {
            let wl = c.to_world_layout();
            out.pieces.extend(wl.pieces);
            for (k, v) in wl.floors {
                out.floors.entry(k).or_insert(v);
            }
        }
        out
    }
}

#[derive(Component, Clone, Copy)]
pub struct KenneyMountHatch {
    pub exit: u8,
    pub candidate_id: u32,
}

pub struct KenneyStreamPlugin;

impl Plugin for KenneyStreamPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (mount_hub_candidates, stream_hub_commit).chain(),
        );
    }
}

pub fn stream_enabled(test: Option<&TestMode>) -> bool {
    test.is_some_and(|t| t.style == TestMapStyle::Kenney) && PoolIndex::load_from_disk().is_some()
}

pub fn init_kenney_stream(
    mut commands: Commands,
    editor: Option<Res<EditorMode>>,
    test: Option<Res<TestMode>>,
    mut run_q: Query<&mut RunState>,
    mut layout_cache: ResMut<KenneyLayoutCache>,
) {
    if editor.is_some() {
        return;
    }
    let Some(test) = test else {
        return;
    };
    if !stream_enabled(Some(&test)) {
        return;
    }
    let pool = PoolIndex::load_from_disk().expect("pool index");
    let start_id = pool.start_id().expect("pool start id").to_string();
    let entry = pool.entry(&start_id).expect("start map in pool");
    let doc = PoolMapDocument::load(entry).expect("load start map");
    let active = MountedMap::active(start_id.clone(), doc.layout);

    let Ok(mut run) = run_q.single_mut() else {
        warn!("kenney stream: RunState not ready — loading pool without run sync");
        layout_cache.0 = active.to_world_layout();
        commands.insert_resource(KenneyStreamWorld {
            pool,
            active,
            candidates: Vec::new(),
            epoch: 1,
        });
        info!("kenney stream: loaded active map {} (deferred run state)", start_id);
        return;
    };
    run.map_stream.active_pool_id = start_id.clone();
    run.map_stream.epoch = 1;

    layout_cache.0 = active.to_world_layout();
    commands.insert_resource(KenneyStreamWorld {
        pool,
        active,
        candidates: Vec::new(),
        epoch: 1,
    });
    info!("kenney stream: loaded active map {}", start_id);
}

fn mount_hub_candidates(
    mut commands: Commands,
    test: Option<Res<TestMode>>,
    mut stream: Option<ResMut<KenneyStreamWorld>>,
    mut run_q: Query<&mut RunState>,
    asset_server: Res<AssetServer>,
    scenes: Query<Entity, With<KenneyColliderScene>>,
    floors: Query<Entity, With<KenneyFloorCell>>,
    hatches: Query<Entity, With<KenneyMountHatch>>,
) {
    if !stream_enabled(test.as_deref()) {
        return;
    }
    let Some(mut world) = stream else {
        return;
    };
    let Ok(mut run) = run_q.single_mut() else {
        return;
    };
    if run.phase != RunPhase::InHub || !run.map_stream.candidates.is_empty() {
        return;
    }

    let mut used = run.map_stream.used_pool_ids.clone();
    used.push(run.map_stream.active_pool_id.clone());
    let available = world.pool.unused(&used);
    if available.is_empty() {
        return;
    }

    let exits: [u8; 3] = [3, 4, 2];
    let pick_n = available.len().min(3).max(1);
    let active_exits = world.active.world_hub_exits();

    let mut next_id = world.active.instance_id + 1;
    let mut candidates = Vec::new();
    let mut candidate_map = std::collections::HashMap::new();

    for (i, entry) in available.iter().take(pick_n).enumerate() {
        let exit = exits[i];
        let key = exit.to_string();
        let Some(exit_spec) = active_exits.get(&key) else {
            continue;
        };
        let Some(doc) = PoolMapDocument::load(entry) else {
            continue;
        };
        let offset = mount_offset_world(exit_spec, &doc.layout);
        let mounted = MountedMap::candidate(next_id, entry.id.clone(), doc.layout, offset, exit);
        candidate_map.insert(exit, entry.id.clone());
        candidates.push(mounted);
        next_id += 1;
    }

    if candidates.is_empty() {
        return;
    }

    for e in scenes.iter().chain(floors.iter()).chain(hatches.iter()) {
        commands.entity(e).despawn();
    }

    world.candidates = candidates;
    run.map_stream.candidates = candidate_map;
    run.map_stream.epoch += 1;
    world.epoch = run.map_stream.epoch;

    spawn_stream_geometries(&mut commands, &asset_server, &world);
    info!(
        "mounted {} hub candidates (epoch {})",
        world.candidates.len(),
        world.epoch
    );
}

fn stream_hub_commit(
    mut commands: Commands,
    test: Option<Res<TestMode>>,
    mut stream: Option<ResMut<KenneyStreamWorld>>,
    mut layout_cache: ResMut<KenneyLayoutCache>,
    mut run_q: Query<&mut RunState>,
    players: Query<(&Transform, &PlayerAlive, &PlayerName), With<Player>>,
    scenes: Query<Entity, With<KenneyColliderScene>>,
    floors: Query<Entity, With<KenneyFloorCell>>,
    hatches: Query<Entity, With<KenneyMountHatch>>,
    asset_server: Res<AssetServer>,
) {
    if !stream_enabled(test.as_deref()) {
        return;
    }
    let Some(mut world) = stream else {
        return;
    };
    let Ok(mut run) = run_q.single_mut() else {
        return;
    };
    if run.phase != RunPhase::InHub || run.hub_commit.l1_unloaded {
        return;
    }

    let active_exits = world.active.world_hub_exits();
    let extraction = world.active.world_extraction();

    let alive_names: Vec<String> = players
        .iter()
        .filter(|(_, alive, _)| alive.0)
        .map(|(_, _, name)| name.0.clone())
        .collect();

    for (transform, alive, name) in &players {
        if !alive.0 || run.hub_commit.player_committed(&name.0) {
            continue;
        }
        let pos = transform.translation;
        for candidate in &world.candidates {
            let Some(exit) = candidate.exit else {
                continue;
            };
            if run.hub_commit.is_exit_closed(exit) {
                continue;
            }
            let key = exit.to_string();
            let Some(exit_spec) = active_exits.get(&key) else {
                continue;
            };
            let spawn = candidate.layout.spawn_xz.unwrap_or([0.0, 0.0]);
            let branch = shared::editor_map::BranchLevel {
                x: spawn[0] + candidate.offset.x,
                z: spawn[1] + candidate.offset.z,
                floor: (candidate.offset.y / MOD_H).round() as i32,
                label: exit_spec.label.clone(),
            };
            if shared::kenney_hub::detects_branch_commit(pos, exit, &branch, extraction) {
                info!("{} committed to hub exit {exit} ({})", name.0, exit_spec.label);
                run.hub_commit.player_exits.push(shared::run::PlayerExitCommit {
                    player: name.0.clone(),
                    exit,
                });
                if run.hub_commit.chosen_exit.is_none() {
                    run.hub_commit.chosen_exit = Some(exit);
                    info!("party locked exit {exit}");
                }
                break;
            }
        }
    }

    if !run.hub_commit.all_alive_committed(&alive_names) {
        return;
    }

    let Some(chosen_exit) = run.hub_commit.chosen_exit else {
        return;
    };
    let Some(chosen) = world
        .candidates
        .iter()
        .find(|c| c.exit == Some(chosen_exit))
        .cloned()
    else {
        return;
    };

    let chosen_id = chosen.pool_id.clone();
    let prev_active = run.map_stream.active_pool_id.clone();
    run.map_stream.used_pool_ids.push(prev_active);
    run.map_stream.active_pool_id = chosen_id.clone();
    run.map_stream.candidates.clear();
    run.hub_commit = Default::default();
    run.phase = RunPhase::InStretch;
    run.hub_id = None;
    run.map_stream.epoch += 1;

    world.active = chosen;
    world.active.exit = None;
    world.active.instance_id = 1;
    world.candidates.clear();
    world.epoch = run.map_stream.epoch;

    for e in scenes.iter().chain(floors.iter()).chain(hatches.iter()) {
        commands.entity(e).despawn();
    }

    layout_cache.0 = world.active.to_world_layout();
    spawn_stream_geometries(&mut commands, &asset_server, &world);
    info!("stream transition → active {chosen_id} (epoch {})", world.epoch);
}

pub fn spawn_stream_geometries(
    commands: &mut Commands,
    asset_server: &AssetServer,
    world: &KenneyStreamWorld,
) {
    for inst in world.all_instances() {
        spawn_instance_pieces(commands, asset_server, inst);
        spawn_instance_floors(commands, inst);
        if inst.exit.is_some() {
            spawn_mount_hatch(commands, inst);
        }
    }
}

fn spawn_instance_pieces(commands: &mut Commands, asset_server: &AssetServer, inst: &MountedMap) {
    for p in &inst.layout.pieces {
        let collide = kenney_catalog::piece(&p.stem)
            .map(|x| x.collide_default)
            .unwrap_or(true);
        // Decide skip in the instance-local frame (the mask is origin-centred, pre-offset).
        if !collide || kenney_skip_piece_collider(p, &inst.layout) {
            continue;
        }
        if shared::kenney_layout::uses_floor_cell_collider(p) {
            continue;
        }
        let yaw = quantize_yaw(p.yaw);
        let path = shared::editor_catalog::glb_asset_path(&p.stem);
        let scale = p.scale.max(0.01);
        // Compute cutouts in the local frame (local mask + local extraction), then translate
        // the resulting world-space hole/opening centres by the instance offset.
        let mesh_cutouts = shared::kenney_pit::mesh_cutouts_for_piece(
            &p.stem,
            p.floor,
            p.x,
            p.z,
            p.yaw,
            inst.layout.extraction_xz.map(|[ex, ez]| Vec2::new(ex, ez)),
            inst.layout.floors.get(&p.floor),
            p.ceiling,
        )
        .translated(inst.offset.x, inst.offset.z);
        commands.spawn((
            LevelEntity,
            KenneyInstanceTag {
                instance_id: inst.instance_id,
            },
            KenneyColliderScene {
                stem: p.stem.clone(),
                mesh_cutouts,
                group_id: p.group_id,
                floor: p.floor,
            },
            KenneyPieceMeta {
                group_id: p.group_id,
                floor: p.floor,
            },
            SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(path))),
            Transform::from_translation(inst.piece_translation(p))
                .with_rotation(shared::kenney_layout::placement_rotation(yaw, p.ceiling))
                .with_scale(Vec3::splat(scale)),
        ));
    }
}

fn spawn_instance_floors(commands: &mut Commands, inst: &MountedMap) {
    let layout = &inst.layout;
    if layout.floors.is_empty() {
        return;
    }
    let cell = layout.grid_unit_m;
    for (level, mask) in &layout.floors {
        let y = *level as f32 * MOD_H + inst.offset.y;
        let x0 = mask.world_x0() + inst.offset.x;
        let z0 = mask.world_z0() + inst.offset.z;
        for iz in 0..mask.cells_z {
            for ix in 0..mask.cells_x {
                if !mask.get(ix, iz) {
                    continue;
                }
                let cx = x0 + (ix as f32 + 0.5) * cell;
                let cz = z0 + (iz as f32 + 0.5) * cell;
                if crate::level::kenney_mesh_covers_cell(layout, ix, iz, *level) {
                    continue;
                }
                commands.spawn((
                    LevelEntity,
                    KenneyInstanceTag {
                        instance_id: inst.instance_id,
                    },
                    KenneyFloorCell,
                    KenneyFloorCellMeta {
                        floor: *level,
                        map_ix: ix,
                        map_iz: iz,
                    },
                    RigidBody::Static,
                    Collider::cuboid(cell * 0.5, 0.12, cell * 0.5),
                    Transform::from_translation(Vec3::new(cx, y + 0.08, cz)),
                ));
            }
        }
    }
}

fn spawn_mount_hatch(commands: &mut Commands, inst: &MountedMap) {
    let [sx, sz] = inst.layout.spawn_xz.unwrap_or([0.0, 0.0]);
    let wx = sx + inst.offset.x;
    let wz = sz + inst.offset.z;
    let y = -MOD_H + inst.offset.y + 0.002;
    commands.spawn((
        LevelEntity,
        KenneyInstanceTag {
            instance_id: inst.instance_id,
        },
        KenneyMountHatch {
            exit: inst.exit.unwrap_or(0),
            candidate_id: inst.instance_id,
        },
        Transform::from_translation(Vec3::new(wx, y, wz)),
    ));
}
