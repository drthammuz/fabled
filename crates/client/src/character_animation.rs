//! Player character animations (remote AND own-player third-person model):
//! Idle / Walk driven by position delta.
//!
//! The character GLBs export two NLA strips named "Idle_Root" and "Walk_Root".
//! We build a per-class AnimationGraph once the GLTF is loaded, then wire it
//! up to each player's AnimationPlayer when their scene finishes spawning.

use std::collections::HashMap;
use std::time::Duration;

use bevy::gltf::Gltf;
use bevy::prelude::*;
use shared::classes::{ClassKind, ALL_CLASSES};
use shared::protocol::{Player, PlayerAlive, PlayerAttackAnim, PlayerHeldKind};


pub struct CharacterAnimationPlugin;

impl Plugin for CharacterAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CharacterAnimLib>()
            .add_systems(Startup, preload_anim_assets)
            // build_anim_graphs can stay in Update (mutates Assets, no entity commands).
            .add_systems(Update, build_anim_graphs)
            // Detection and wiring run in PostUpdate so that all Update command buffers
            // (model despawn/respawn on class change) have been flushed before we query
            // Added<AnimationPlayer>.  This prevents a panic when the rig entity is
            // despawned and re-inserted in the same flush.
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
    idle_gun: AnimationNodeIndex,
    idle_sword: AnimationNodeIndex,
    walk:  AnimationNodeIndex,
    run:   AnimationNodeIndex,
    gun_shoot: AnimationNodeIndex,
    run_shoot: AnimationNodeIndex,
    melee: AnimationNodeIndex,
    death: AnimationNodeIndex,
}

#[derive(Resource, Default)]
struct CharacterAnimLib(HashMap<ClassKind, ClassAnimEntry>);

fn preload_anim_assets(asset_server: Res<AssetServer>, mut lib: ResMut<CharacterAnimLib>) {
    for def in &ALL_CLASSES {
        lib.0.insert(def.kind, ClassAnimEntry {
            gltf:  asset_server.load(def.model_path),
            graph: None,
            idle:  AnimationNodeIndex::default(),
            idle_gun: AnimationNodeIndex::default(),
            idle_sword: AnimationNodeIndex::default(),
            walk:  AnimationNodeIndex::default(),
            run:   AnimationNodeIndex::default(),
            gun_shoot: AnimationNodeIndex::default(),
            run_shoot: AnimationNodeIndex::default(),
            melee: AnimationNodeIndex::default(),
            death: AnimationNodeIndex::default(),
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
        // Quaternius cyberpunk Character clip names (exact first, loose fallback).
        let Some(idle_clip) = exact("Idle_Neutral").or_else(|| contains("idle")) else {
            warn!("{kind:?}: no idle clip; available: {:?}",
                  gltf.named_animations.keys().collect::<Vec<_>>());
            continue;
        };
        let Some(walk_clip) = exact("Walk")
            .or_else(|| contains("walk"))
            .or_else(|| contains("run"))
        else {
            warn!("{kind:?}: no walk/run clip; available: {:?}",
                  gltf.named_animations.keys().collect::<Vec<_>>());
            continue;
        };
        let run_clip = exact("Run").unwrap_or_else(|| walk_clip.clone());
        let idle_gun = exact("Idle_Gun_Pointing").unwrap_or_else(|| idle_clip.clone());
        let idle_sword = exact("Idle_Sword").unwrap_or_else(|| idle_clip.clone());
        let gun_shoot = exact("Gun_Shoot").unwrap_or_else(|| idle_gun.clone());
        let run_shoot = exact("Run_Shoot").unwrap_or_else(|| gun_shoot.clone());
        let melee = exact("Sword_Slash")
            .or_else(|| exact("Punch_Right"))
            .unwrap_or_else(|| idle_clip.clone());
        let death = exact("Death").unwrap_or_else(|| idle_clip.clone());

        let mut graph = AnimationGraph::new();
        let root = graph.root;
        entry.idle  = graph.add_clip(idle_clip, 1.0, root);
        entry.idle_gun = graph.add_clip(idle_gun, 1.0, root);
        entry.idle_sword = graph.add_clip(idle_sword, 1.0, root);
        entry.walk  = graph.add_clip(walk_clip, 1.0, root);
        entry.run   = graph.add_clip(run_clip, 1.0, root);
        entry.gun_shoot = graph.add_clip(gun_shoot, 1.0, root);
        entry.run_shoot = graph.add_clip(run_shoot, 1.0, root);
        entry.melee = graph.add_clip(melee, 1.0, root);
        entry.death = graph.add_clip(death, 1.0, root);
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
                // (e.g. a class change despawned the model the same frame as Added<AnimationPlayer>).
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
    time: Res<Time>,
    mut players: Query<
        (
            Entity,
            &Transform,
            &PlayerRig,
            &PlayerAnimClass,
            &mut PlayerLastPos,
            Option<&PlayerAlive>,
            Option<&PlayerHeldKind>,
            Option<&PlayerAttackAnim>,
        ),
        With<Player>,
    >,
    new_players: Query<(Entity, &Transform), (With<PlayerRig>, Without<PlayerLastPos>, With<Player>)>,
    mut rigs: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    lib: Res<CharacterAnimLib>,
) {
    // Seed LastPos on newly rigged players.
    for (entity, transform) in &new_players {
        commands.entity(entity).insert(PlayerLastPos {
            pos: transform.translation,
            speed: 0.0,
            current: None,
            attack_seq: None,
            attack_until: 0.0,
            attack_kind: 0,
        });
    }

    let dt = time.delta_secs().max(1e-6);
    let now = time.elapsed_secs();
    for (_entity, transform, rig, anim_class, mut st, alive, held, attack) in &mut players {
        let inst = (transform.translation - st.pos).length() / dt;
        st.pos = transform.translation;
        // Light smoothing so interpolation hiccups don't flicker clips.
        st.speed = st.speed * 0.8 + inst.min(12.0) * 0.2;

        // One-shot attack overlay: seq bump starts a short window. Melee is sped
        // up so a swing is snappy/viable; Soldier swings a touch faster still.
        let soldier = anim_class.0 == ClassKind::Soldier;
        if let Some(a) = attack {
            if st.attack_seq != Some(a.seq) {
                let started = st.attack_seq.is_some();
                st.attack_seq = Some(a.seq);
                if started {
                    let window = match a.kind {
                        2 => if soldier { 0.27 } else { 0.33 }, // melee swing
                        _ => 0.38,                              // ranged recoil
                    };
                    st.attack_until = now + window;
                    st.attack_kind = a.kind;
                }
            }
        }

        let Some(entry) = lib.0.get(&anim_class.0) else { continue };
        let dead = alive.is_some_and(|a| !a.0);
        let moving = st.speed > 0.4;
        let held_kind = held.map(|h| h.0).unwrap_or(0);
        // Base walk speed is 8.4 and sprint 12.6, so split walk vs run near their
        // midpoint; crouch (4.5) rides the walk clip, slowed by the speed scaling.
        let sprinting = st.speed > 10.5;

        let (target, repeat) = if dead {
            (entry.death, false)
        } else if now < st.attack_until {
            match st.attack_kind {
                2 => (entry.melee, false),
                _ if moving => (entry.run_shoot, false),
                _ => (entry.gun_shoot, false),
            }
        } else if sprinting {
            (entry.run, true)
        } else if moving {
            (entry.walk, true)
        } else {
            match held_kind {
                1 => (entry.idle_gun, true),
                2 => (entry.idle_sword, true),
                _ => (entry.idle, true),
            }
        };

        // Clip playback speed: melee/attack sped up for snap; walk & run scaled to
        // the player's ACTUAL speed so footfalls read as crouch / walk / sprint.
        const WALK_REF: f32 = 8.4; // config::PLAYER_MOVE_SPEED
        const RUN_REF: f32 = 12.6; // config::PLAYER_SPRINT_SPEED
        let clip_speed = if now < st.attack_until {
            match st.attack_kind {
                2 => if soldier { 1.75 } else { 1.45 },
                _ => 1.3,
            }
        } else if target == entry.run {
            (st.speed / RUN_REF).clamp(0.7, 1.5)
        } else if target == entry.walk {
            (st.speed / WALK_REF).clamp(0.5, 1.6)
        } else {
            1.0
        };

        let Ok((mut player, mut transitions)) = rigs.get_mut(rig.0) else { continue };
        if st.current != Some(target) {
            st.current = Some(target);
            let active = transitions.play(&mut player, target, Duration::from_millis(120));
            active.set_speed(clip_speed);
            if repeat {
                active.repeat();
            }
        } else if let Some(active) = player.animation_mut(target) {
            // Same clip still playing — keep speed synced as movement changes.
            active.set_speed(clip_speed);
        }
    }
}

/// Per-player animation driver state (movement + attack overlay tracking).
#[derive(Component)]
struct PlayerLastPos {
    pos: Vec3,
    speed: f32,
    current: Option<AnimationNodeIndex>,
    attack_seq: Option<u32>,
    attack_until: f32,
    attack_kind: u8,
}
