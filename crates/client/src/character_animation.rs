//! Remote player character animations: Idle / Walk driven by position delta.
//!
//! The character GLBs export two NLA strips named "Idle_Root" and "Walk_Root".
//! We build a per-class AnimationGraph once the GLTF is loaded, then wire it
//! up to each player's AnimationPlayer when their scene finishes spawning.

use std::collections::HashMap;
use std::time::Duration;

use bevy::gltf::Gltf;
use bevy::prelude::*;
use shared::classes::{ClassKind, ALL_CLASSES};
use shared::protocol::Player;

use crate::netplay::OwnPlayer;

pub struct CharacterAnimationPlugin;

impl Plugin for CharacterAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CharacterAnimLib>()
            .add_systems(Startup, preload_anim_assets)
            // build_anim_graphs can stay in Update (mutates Assets, no entity commands).
            .add_systems(Update, build_anim_graphs)
            // Detection and wiring run in PostUpdate so that all Update command buffers
            // (including remove_own_player_model's recursive despawn of the OwnPlayer rig)
            // have been flushed before we query Added<AnimationPlayer>.  This prevents a
            // panic when the rig entity is despawned and re-inserted in the same flush.
            .add_systems(PostUpdate, (
                detect_new_rigs,
                ApplyDeferred,
                wire_pending_rigs,
                drive_player_animations,
            ).chain());
    }
}

// ---------------------------------------------------------------------------
// Animation library (per class)
// ---------------------------------------------------------------------------

struct ClassAnimEntry {
    gltf:  Handle<Gltf>,
    graph: Option<Handle<AnimationGraph>>,
    idle:  AnimationNodeIndex,
    walk:  AnimationNodeIndex,
}

#[derive(Resource, Default)]
struct CharacterAnimLib(HashMap<ClassKind, ClassAnimEntry>);

fn preload_anim_assets(asset_server: Res<AssetServer>, mut lib: ResMut<CharacterAnimLib>) {
    for def in &ALL_CLASSES {
        lib.0.insert(def.kind, ClassAnimEntry {
            gltf:  asset_server.load(def.model_path),
            graph: None,
            idle:  AnimationNodeIndex::default(),
            walk:  AnimationNodeIndex::default(),
        });
    }
}

fn build_anim_graphs(
    mut lib:   ResMut<CharacterAnimLib>,
    gltf_assets: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    for (kind, entry) in lib.0.iter_mut() {
        if entry.graph.is_some() { continue; }
        let Some(gltf) = gltf_assets.get(&entry.gltf) else { continue };

        // Exact (case-insensitive) name match — KayKit GLBs have 76 clips, so
        // a loose `contains` would grab e.g. "2H_Melee_Idle" instead of "Idle".
        let exact = |needle: &str| {
            gltf.named_animations
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(needle))
                .map(|(_, clip)| clip.clone())
        };
        let contains = |needle: &str| {
            let n = needle.to_lowercase();
            gltf.named_animations
                .iter()
                .find(|(name, _)| name.to_lowercase().contains(&n))
                .map(|(_, clip)| clip.clone())
        };
        // KayKit idle = "Idle"; fall back to anything containing "idle".
        let Some(idle_clip) = exact("Idle").or_else(|| contains("idle")) else {
            warn!("{kind:?}: no idle clip; available: {:?}",
                  gltf.named_animations.keys().collect::<Vec<_>>());
            continue;
        };
        // KayKit walk = "Walking_A"; then any walk, then any run.
        let Some(walk_clip) = exact("Walking_A")
            .or_else(|| contains("walk"))
            .or_else(|| contains("run"))
        else {
            warn!("{kind:?}: no walk/run clip; available: {:?}",
                  gltf.named_animations.keys().collect::<Vec<_>>());
            continue;
        };

        let mut graph = AnimationGraph::new();
        let root = graph.root;
        entry.idle  = graph.add_clip(idle_clip, 1.0, root);
        entry.walk  = graph.add_clip(walk_clip, 1.0, root);
        entry.graph = Some(graphs.add(graph));
        info!("{kind:?}: animation graph ready (all clips: {:?})",
              gltf.named_animations.keys().collect::<Vec<_>>());
    }
}

// ---------------------------------------------------------------------------
// Per-instance wiring
// ---------------------------------------------------------------------------

/// Placed on the SceneRoot child entity linking it back to the player entity.
/// Needed so we can walk up the hierarchy from AnimationPlayer to the player.
#[derive(Component, Clone, Copy)]
pub struct PlayerSceneLink(pub Entity);

/// Stored on the player entity once its rig is wired up.
#[derive(Component)]
struct PlayerRig(Entity);

/// Per-player class recorded so we can look up the right animation entry.
#[derive(Component)]
struct PlayerAnimClass(ClassKind);

/// Marker added to a rig entity when its AnimationPlayer appears but the
/// animation graph for its class is not yet built. Retried every frame until
/// `wire_pending_rigs` can resolve it.
#[derive(Component)]
struct PendingRig {
    player_entity: Entity,
}

/// Step 1 — fires once per rig via `Added<AnimationPlayer>`.
/// Walks up the parent hierarchy from the AnimationPlayer entity to find
/// the `PlayerSceneLink` that connects it back to the player entity.
fn detect_new_rigs(
    mut commands: Commands,
    new_rigs: Query<Entity, Added<AnimationPlayer>>,
    parents: Query<&ChildOf>,
    scene_links: Query<&PlayerSceneLink>,
) {
    for rig_entity in &new_rigs {
        let mut current = rig_entity;
        let mut found = false;
        loop {
            if let Ok(link) = scene_links.get(current) {
                // Class is determined from the player entity inside wire_pending_rigs.
                // Use get_entity to silently skip if the rig was concurrently despawned
                // (e.g. remove_own_player_model fires the same frame as Added<AnimationPlayer>).
                if let Ok(mut ec) = commands.get_entity(rig_entity) {
                    ec.insert(PendingRig { player_entity: link.0 });
                    warn!("ANIM: new rig {rig_entity:?} → player {:?}", link.0);
                }
                found = true;
                break;
            }
            match parents.get(current) {
                Ok(p) => current = p.parent(),
                Err(_) => break,
            }
        }
        if !found {
            warn!("ANIM: AnimationPlayer {rig_entity:?} has no PlayerSceneLink ancestor — will not animate");
        }
    }
}

/// Step 2 — runs every frame on all pending rigs.
/// Wires up the animation graph + idle clip as soon as the graph is ready.
fn wire_pending_rigs(
    mut commands: Commands,
    mut pending: Query<(Entity, &PendingRig, &mut AnimationPlayer)>,
    player_classes: Query<Option<&shared::protocol::PlayerClass>>,
    lib: Res<CharacterAnimLib>,
) {
    for (rig_entity, pending, mut anim_player) in &mut pending {
        // Resolve class from the player entity (may not have arrived yet; fallback=Soldier).
        let kind = player_classes
            .get(pending.player_entity)
            .ok()
            .flatten()
            .map(|c| c.0)
            .unwrap_or(ClassKind::Soldier);

        // Find the first available graph — if this class isn't built yet, use Soldier.
        // All Kenney characters share the same skeleton so any graph works.
        let entry = lib.0.get(&kind)
            .or_else(|| lib.0.get(&ClassKind::Soldier))
            .filter(|e| e.graph.is_some());
        let Some(entry) = entry else { continue }; // no graph ready yet
        let graph = entry.graph.clone().unwrap();

        let mut transitions = AnimationTransitions::new();
        transitions
            .play(&mut anim_player, entry.idle, Duration::ZERO)
            .repeat();

        if let Ok(mut ec) = commands.get_entity(rig_entity) {
            ec.insert((AnimationGraphHandle(graph), transitions));
            ec.remove::<PendingRig>();
        } else {
            continue;
        }
        if let Ok(mut ec) = commands.get_entity(pending.player_entity) {
            ec.insert((PlayerRig(rig_entity), PlayerAnimClass(kind)));
        }
        warn!("ANIM: rig {rig_entity:?} wired ({kind:?}) idle={:?} walk={:?}",
              entry.idle, entry.walk);
    }
}

// ---------------------------------------------------------------------------
// Animation driving
// ---------------------------------------------------------------------------

fn drive_player_animations(
    mut commands: Commands,
    mut players: Query<
        (Entity, &Transform, &PlayerRig, &PlayerAnimClass, &mut PlayerLastPos),
        (With<Player>, Without<OwnPlayer>),
    >,
    new_players: Query<(Entity, &Transform), (With<PlayerRig>, Without<PlayerLastPos>, With<Player>)>,
    mut rigs: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    lib: Res<CharacterAnimLib>,
) {
    // Seed LastPos on newly rigged players.
    for (entity, transform) in &new_players {
        commands.entity(entity).insert(PlayerLastPos(transform.translation, false));
    }

    for (_entity, transform, rig, anim_class, mut last_pos) in &mut players {
        let delta = transform.translation.distance_squared(last_pos.0);
        let is_walking = delta > 0.0001; // ~1 cm moved per frame
        last_pos.0 = transform.translation;

        // Only switch when state changes to avoid restarting the clip every frame.
        if is_walking == last_pos.1 { continue; }
        last_pos.1 = is_walking;

        let Some(entry) = lib.0.get(&anim_class.0) else { continue };
        let Ok((mut player, mut transitions)) = rigs.get_mut(rig.0) else { continue };

        let target = if is_walking { entry.walk } else { entry.idle };
        transitions
            .play(&mut player, target, Duration::from_millis(200))
            .repeat();
    }
}

/// Cached last-frame position and walking state for movement detection.
#[derive(Component)]
struct PlayerLastPos(Vec3, bool);
