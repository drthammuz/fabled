//! Client half of runtime hub map streaming (see `shared::proc_stream`).
//!
//! Owns: background generation of the two child maps (python gen_maps.py in
//! threads), child piece VISUALS while they wait under the hub, commit
//! detection (own player drops through a hub exit), and the commit swap:
//! load the chosen child into the editor workspace at the world origin,
//! re-export the playtest layout, shift the player by −offset, and bump
//! `KenneyPlaytestGeneration` so the server rebuilds colliders/agents/nav
//! through the normal reload path. The parent map and unchosen sibling are
//! despawned by that same reload. Host-mode solo is the supported v1 scope.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};

use bevy::gltf::GltfAssetLabel;
use bevy::prelude::*;
use shared::editor_map::MapDocument;
use shared::kenney_catalog::quantize_yaw;
use shared::level::MOD_H;
use shared::map_pool;
use shared::proc_stream::{GenKnobs, ProcChild, ProcStreamState, CHAIN_FACTIONS, CHILD_MAP_DIR};
use shared::protocol::NetTransform;
use shared::KenneyPlaytestGeneration;

use crate::editor_map_gen::{repo_root, MapGenSettings};
use crate::editor_playtest::EditorPlaytestActive;
use crate::editor_selection::{EditorPlaced, PieceOwner};
use crate::editor_state::EditorState;
use crate::editor_workspace::EditorWorkspace;
use crate::kenney_editor::spawn_piece_record_pub;
use crate::netplay::OwnPlayer;

pub struct ProcStreamPlugin;

impl Plugin for ProcStreamPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ProcStreamState>()
            .init_resource::<ProcStreamRuntime>()
            .add_systems(
                Update,
                (
                    stream_kickoff,
                    stream_poll,
                    stream_child_visuals,
                    stream_commit_detect,
                    stream_commit_apply,
                )
                    .chain()
                    .run_if(bevy::ecs::schedule::common_conditions::resource_exists::<
                        EditorPlaytestActive,
                    >),
            )
            .add_systems(Update, stream_reset_outside_playtest);
    }
}

/// Marks a visual spawned for a mounted (uncommitted) child map piece.
#[derive(Component)]
pub struct ProcChildVisual {
    pub instance_id: u32,
}

struct GenJob {
    exit_key: String,
    next_faction: String,
    out_path: PathBuf,
    round: u32,
    // Mutex-wrapped: Receiver is !Sync and resources must be Send+Sync
    // (same pattern as editor_map_gen::MapGenRuntime).
    rx: std::sync::Mutex<Receiver<Result<(), String>>>,
}

#[derive(Resource, Default)]
pub struct ProcStreamRuntime {
    jobs: Vec<GenJob>,
    /// KenneyPlaytestGeneration value the current round was kicked for.
    kicked_generation: Option<u32>,
    rng: u64,
}

impl ProcStreamRuntime {
    fn next_rand(&mut self) -> u64 {
        // xorshift64* — deterministic enough, seeded from wall clock at first use.
        if self.rng == 0 {
            self.rng = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x9E3779B97F4A7C15)
                | 1;
        }
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
}

fn knobs_from_settings(s: &MapGenSettings) -> GenKnobs {
    GenKnobs {
        cells: s.cells,
        rooms: s.rooms,
        loops: s.loops,
        attempts: s.attempts,
        organicness: s.organicness,
        corridor_width: s.corridor_width,
        hidden: s.hidden,
        prev_fraction: s.prev_fraction,
        default_fraction: s.default_fraction,
        next_fraction: s.next_fraction,
        enemy_count: s.enemy_count,
    }
}

/// Run `tools/gen_maps.py` for one child map (blocking; call from a thread).
fn run_python_child(
    knobs: &GenKnobs,
    seed: u32,
    prev_faction: &str,
    next_faction: &str,
    out: &PathBuf,
) -> Result<(), String> {
    let root = repo_root();
    let script = root.join("tools/gen_maps.py");
    if !script.exists() {
        return Err(format!("missing {}", script.display()));
    }
    if let Some(dir) = out.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let args = vec![
        script.to_string_lossy().into_owned(),
        "--preview".into(),
        "--no-layout-export".into(),
        "--seed".into(),
        seed.to_string(),
        "--attempts".into(),
        knobs.attempts.to_string(),
        "--cells".into(),
        knobs.cells.to_string(),
        "--rooms".into(),
        knobs.rooms.to_string(),
        "--loops".into(),
        knobs.loops.to_string(),
        "--organicness".into(),
        format!("{:.2}", knobs.organicness),
        "--corridor-width".into(),
        format!("{:.2}", knobs.corridor_width),
        "--hidden".into(),
        format!("{:.2}", knobs.hidden),
        "--mix-mode".into(),
        "transition".into(),
        "--faction-profile".into(),
        "industrial_default".into(),
        "--prev-faction".into(),
        prev_faction.into(),
        "--next-faction".into(),
        next_faction.into(),
        "--default-faction".into(),
        "industrial_default".into(),
        "--prev-fraction".into(),
        format!("{:.2}", knobs.prev_fraction),
        "--default-fraction".into(),
        format!("{:.2}", knobs.default_fraction),
        "--next-fraction".into(),
        format!("{:.2}", knobs.next_fraction),
        "--num-enemies".into(),
        knobs.enemy_count.to_string(),
        // NPC counts are placement rules in the generator, not a knob.
        "--out".into(),
        out.to_string_lossy().into_owned(),
    ];

    let try_run = |python: &str, extra: &[&str]| -> std::io::Result<std::process::Output> {
        let mut cmd = Command::new(python);
        cmd.args(extra).args(&args).current_dir(&root);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd.output()
    };
    let output = try_run("python", &[])
        .or_else(|_| try_run("py", &["-3"]))
        .or_else(|_| try_run("python3", &[]))
        .map_err(|e| format!("could not run python: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    if !out.exists() {
        return Err("generator produced no output file".into());
    }
    Ok(())
}

/// Start generating two children when a new active map begins (playtest enter
/// or a commit swap — both bump `KenneyPlaytestGeneration`).
fn stream_kickoff(
    generation: Res<KenneyPlaytestGeneration>,
    settings: Res<MapGenSettings>,
    mut state: ResMut<ProcStreamState>,
    mut runtime: ResMut<ProcStreamRuntime>,
) {
    if runtime.kicked_generation == Some(generation.0) {
        return;
    }
    runtime.kicked_generation = Some(generation.0);
    runtime.jobs.clear();
    state.children.clear();
    state.committed = None;
    state.round = state.round.wrapping_add(1);

    let layout = map_pool::play_layout(true);
    let exits = map_pool::hub_exits_for(&layout);
    if exits.is_empty() {
        info!("proc stream: active map has no hub exits — streaming idle");
        return;
    }
    if state.active_next_faction.is_empty() {
        state.active_next_faction = settings.next_faction.clone();
    }
    let prev = state.active_next_faction.clone();
    let knobs = knobs_from_settings(&settings);

    let mut keys: Vec<String> = exits.keys().cloned().collect();
    keys.sort();
    for key in keys.into_iter().take(2) {
        let next = CHAIN_FACTIONS[(runtime.next_rand() % CHAIN_FACTIONS.len() as u64) as usize];
        let seed = (runtime.next_rand() % 1_000_000) as u32 + 1;
        let out = repo_root().join(CHILD_MAP_DIR).join(format!(
            "child_r{}_e{}.json",
            state.round, key
        ));
        let (tx, rx) = mpsc::channel();
        let knobs2 = knobs.clone();
        let prev2 = prev.clone();
        let next2 = next.to_string();
        let out2 = out.clone();
        std::thread::spawn(move || {
            let _ = tx.send(run_python_child(&knobs2, seed, &prev2, &next2, &out2));
        });
        info!(
            "proc stream: generating child (exit {key}, {prev} → {next}, seed {seed})"
        );
        runtime.jobs.push(GenJob {
            exit_key: key,
            next_faction: next.to_string(),
            out_path: out,
            round: state.round,
            rx: std::sync::Mutex::new(rx),
        });
    }
}

/// Collect finished generation jobs and mount them under their hub exits.
fn stream_poll(
    mut state: ResMut<ProcStreamState>,
    mut runtime: ResMut<ProcStreamRuntime>,
) {
    if runtime.jobs.is_empty() {
        return;
    }
    let round = state.round;
    let mut done: Vec<(usize, Result<(), String>)> = Vec::new();
    for (i, job) in runtime.jobs.iter().enumerate() {
        let Ok(rx) = job.rx.lock() else {
            continue;
        };
        match rx.try_recv() {
            Ok(res) => done.push((i, res)),
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                done.push((i, Err("generator thread lost".into())))
            }
        }
    }
    for (i, res) in done.into_iter().rev() {
        let job = runtime.jobs.remove(i);
        if job.round != round {
            continue; // stale round (commit happened while generating)
        }
        match res {
            Err(e) => warn!("proc stream: child gen failed (exit {}): {e}", job.exit_key),
            Ok(()) => {
                let Some(doc) = MapDocument::load(&job.out_path) else {
                    warn!("proc stream: cannot load child doc {:?}", job.out_path);
                    continue;
                };
                let layout = doc.to_kenney_layout();
                let active = map_pool::play_layout(true);
                let exits = map_pool::hub_exits_for(&active);
                let Some(exit_spec) = exits.get(&job.exit_key) else {
                    warn!("proc stream: exit {} vanished", job.exit_key);
                    continue;
                };
                let offset = map_pool::mount_offset_world(exit_spec, &layout);
                let instance_id = 2 + state.children.len() as u32;
                info!(
                    "proc stream: child ready (exit {}, offset {:.1},{:.1},{:.1})",
                    job.exit_key, offset.x, offset.y, offset.z
                );
                state.children.push(ProcChild {
                    exit_key: job.exit_key,
                    doc_path: job.out_path,
                    layout,
                    offset,
                    instance_id,
                    next_faction: job.next_faction,
                    colliders_spawned: false,
                    visuals_spawned: false,
                });
            }
        }
    }
}

/// Spawn plain GLB visuals for mounted children (faction material slots apply
/// only after commit through the editor pipeline — pre-commit maps are only
/// glimpsed through the drop holes).
fn stream_child_visuals(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut state: ResMut<ProcStreamState>,
    existing: Query<(Entity, &ProcChildVisual)>,
) {
    let live: Vec<u32> = state.children.iter().map(|c| c.instance_id).collect();
    for (e, v) in &existing {
        if !live.contains(&v.instance_id) {
            commands.entity(e).despawn();
        }
    }
    for child in state.children.iter_mut().filter(|c| !c.visuals_spawned) {
        for p in &child.layout.pieces {
            let path = shared::editor_catalog::glb_asset_path_in_kit(
                &p.stem,
                p.kit.as_deref().unwrap_or("space"),
            );
            let sy = p.scale_y.unwrap_or(p.scale).max(0.01);
            commands.spawn((
                ProcChildVisual {
                    instance_id: child.instance_id,
                },
                SceneRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(path))),
                Transform::from_translation(Vec3::new(
                    p.x + child.offset.x,
                    p.floor as f32 * MOD_H + child.offset.y + p.y.unwrap_or(0.0) + 0.002,
                    p.z + child.offset.z,
                ))
                .with_rotation(shared::kenney_layout::placement_rotation(
                    quantize_yaw(p.yaw),
                    p.ceiling,
                ))
                .with_scale(Vec3::new(p.scale.max(0.01), sy, p.scale.max(0.01))),
            ));
        }
        child.visuals_spawned = true;
        info!(
            "proc stream: spawned visuals for child at exit {} ({} pieces)",
            child.exit_key,
            child.layout.pieces.len()
        );
    }
}

/// A player below the active hub's lowest floor has dropped through an exit
/// hole — lock in the nearest child.
fn stream_commit_detect(
    ws: Res<EditorWorkspace>,
    mut state: ResMut<ProcStreamState>,
    player: Query<&Transform, With<OwnPlayer>>,
) {
    if state.children.is_empty() || state.committed.is_some() {
        return;
    }
    let Ok(tf) = player.single() else {
        return;
    };
    let min_floor = ws.map.floors.keys().copied().min().unwrap_or(0);
    let threshold = min_floor as f32 * MOD_H - 1.5;
    let pos = tf.translation;
    if pos.y >= threshold {
        return;
    }
    let chosen = state
        .children
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let da = child_landing_xz(a).distance_squared(pos.xz());
            let db = child_landing_xz(b).distance_squared(pos.xz());
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i);
    if let Some(i) = chosen {
        let child = state.children.remove(i);
        info!(
            "proc stream: COMMIT — dropped into child at exit {} (y {:.1})",
            child.exit_key, pos.y
        );
        state.committed = Some(child);
    }
}

fn child_landing_xz(c: &ProcChild) -> Vec2 {
    let [sx, sz] = c.layout.spawn_xz.unwrap_or([0.0, 0.0]);
    Vec2::new(sx + c.offset.x, sz + c.offset.z)
}

/// Execute the swap: chosen child becomes the active map at the world origin.
fn stream_commit_apply(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut state: ResMut<ProcStreamState>,
    mut settings: ResMut<MapGenSettings>,
    mut ws: ResMut<EditorWorkspace>,
    mut editor_state: ResMut<EditorState>,
    mut generation: ResMut<KenneyPlaytestGeneration>,
    map_placed: Query<(Entity, &EditorPlaced)>,
    mut players: Query<(&mut Transform, Option<&mut NetTransform>), With<OwnPlayer>>,
) {
    let Some(child) = state.committed.take() else {
        return;
    };
    let Some(mut doc) = MapDocument::load(&child.doc_path) else {
        warn!("proc stream: committed child doc unreadable — aborting swap");
        return;
    };
    doc.apply_hub_playtest_patches();
    let _ = doc.export_playtest_layout();

    // Swap the editor workspace to the child (viewport visuals for the new
    // active map — same path the Proc panel uses after generating).
    ws.map = doc;
    ws.active.path = Some(child.doc_path.clone());
    for (e, ep) in &map_placed {
        if ep.owner == PieceOwner::Map {
            commands.entity(e).despawn();
        }
    }
    for p in ws.map.pieces.clone().iter() {
        let id = editor_state.next_id;
        spawn_piece_record_pub(
            &mut commands,
            &asset_server,
            p,
            PieceOwner::Map,
            id,
            ws.map.extraction_xz,
        );
        editor_state.next_id += 1;
    }
    ws.floor_dirty = true;
    ws.spawn_marker_dirty = true;

    // Re-base the player into the origin-centred child frame.
    for (mut tf, net) in &mut players {
        tf.translation -= child.offset;
        if let Some(mut net) = net {
            net.translation = tf.translation;
        }
    }

    // Faction chain: the child's start = old exit; its exit seeds the next round.
    settings.prev_faction = state.active_next_faction.clone();
    settings.next_faction = child.next_faction.clone();
    state.active_next_faction = child.next_faction.clone();
    state.children.clear();

    // Full reload through the normal pipeline (server colliders, agents, nav)
    // + next stream_kickoff round for the new active map's exits.
    generation.0 = generation.0.wrapping_add(1);
    info!(
        "proc stream: active map swapped (next faction chain → {})",
        state.active_next_faction
    );
}

/// Outside playtest: drop mounted children + visuals so the editor is clean.
fn stream_reset_outside_playtest(
    mut commands: Commands,
    playtest: Option<Res<EditorPlaytestActive>>,
    state: Option<ResMut<ProcStreamState>>,
    mut runtime: ResMut<ProcStreamRuntime>,
    existing: Query<Entity, With<ProcChildVisual>>,
) {
    if playtest.is_some() {
        return;
    }
    let Some(mut state) = state else {
        return;
    };
    if state.children.is_empty() && runtime.jobs.is_empty() && state.committed.is_none() {
        return;
    }
    state.children.clear();
    state.committed = None;
    runtime.jobs.clear();
    runtime.kicked_generation = None;
    for e in &existing {
        commands.entity(e).despawn();
    }
}
