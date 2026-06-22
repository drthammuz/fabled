//! Secret-door animation prototype.
//!
//! Kenney `gate-door.glb` ships baked glTF clips `open` / `close` (node `door`).
//! Hidden-room entrances tagged `hidden_entrance` get proximity open/close.
//!
//! Prototype scope: VISUAL only. `gate-door` has no collider (Kenney gate
//! category), so it does not yet *seal* the room — that's the next step
//! (toggle a collider with the animation + network the open state).

use std::collections::HashMap;
use std::time::Duration;

use bevy::gltf::Gltf;
use bevy::prelude::*;

use crate::netplay::OwnPlayer;
use crate::test_showcase::{KenneyModule, PieceKit};

const OPEN_RADIUS: f32 = shared::hidden_door::PROXIMITY_OPEN_M;
const CLOSE_RADIUS: f32 = shared::hidden_door::PROXIMITY_CLOSE_M;

/// Gate-door kits that ship open/close animation clips.
const DOOR_KITS: [&str; 2] = ["space", "dungeon"];

pub struct DoorAnimPlugin;

impl Plugin for DoorAnimPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DoorAnimAssets>()
            .add_systems(Startup, preload_doors)
            .add_systems(Update, build_door_graphs)
            .add_systems(PostUpdate, (wire_door_rigs, drive_doors).chain());
    }
}

#[derive(Clone)]
struct DoorKitGraph {
    graph: Handle<AnimationGraph>,
    open: AnimationNodeIndex,
    close: AnimationNodeIndex,
}

#[derive(Resource, Default)]
struct DoorAnimAssets {
    gltfs: HashMap<String, Handle<Gltf>>,
    kits: HashMap<String, DoorKitGraph>,
}

/// Marks a hidden-room entrance door (faction profile `hidden_entrance` tag).
#[derive(Component)]
pub struct HiddenEntranceDoor;

/// On a gate-door's AnimationPlayer entity: which door piece + current state.
#[derive(Component)]
struct DoorRig {
    door: Entity,
    kit: String,
    open: bool,
}

fn preload_doors(asset_server: Res<AssetServer>, mut assets: ResMut<DoorAnimAssets>) {
    for kit in DOOR_KITS {
        let path = format!("models/{kit}/gate-door.glb");
        assets.gltfs.insert(kit.to_string(), asset_server.load(path));
    }
}

fn build_door_graphs(
    mut assets: ResMut<DoorAnimAssets>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    for kit in DOOR_KITS {
        if assets.kits.contains_key(kit) {
            continue;
        }
        let Some(handle) = assets.gltfs.get(kit).cloned() else { continue };
        let Some(gltf) = gltfs.get(&handle) else { continue };

        let clip = |needle: &str| {
            gltf.named_animations
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(needle))
                .map(|(_, c)| c.clone())
        };
        let (Some(open), Some(close)) = (clip("open"), clip("close")) else {
            warn!(
                "door anim: no open/close clips in models/{kit}/gate-door.glb; have {:?}",
                gltf.named_animations.keys().collect::<Vec<_>>()
            );
            continue;
        };

        let mut graph = AnimationGraph::new();
        let root = graph.root;
        let open_idx = graph.add_clip(open, 1.0, root);
        let close_idx = graph.add_clip(close, 1.0, root);
        assets.kits.insert(
            kit.to_string(),
            DoorKitGraph {
                graph: graphs.add(graph),
                open: open_idx,
                close: close_idx,
            },
        );
    }
}

fn wire_door_rigs(
    mut commands: Commands,
    assets: Res<DoorAnimAssets>,
    mut new_players: Query<
        (Entity, &mut AnimationPlayer),
        (Added<AnimationPlayer>, Without<AnimationGraphHandle>),
    >,
    parents: Query<&ChildOf>,
    doors: Query<(&KenneyModule, Option<&PieceKit>)>,
) {
    for (rig, mut player) in &mut new_players {
        let mut current = rig;
        let mut door = None;
        let mut kit = "space".to_string();
        loop {
            if let Ok((module, piece_kit)) = doors.get(current) {
                if module.name == "gate-door" || module.name == "gate-door-window" {
                    door = Some(current);
                    kit = piece_kit
                        .map(|k| k.0.clone())
                        .or_else(|| module.kit.map(str::to_string))
                        .unwrap_or_else(|| "space".to_string());
                }
                break;
            }
            match parents.get(current) {
                Ok(p) => current = p.parent(),
                Err(_) => break,
            }
        }
        let Some(door) = door else { continue };
        let Some(graph) = assets.kits.get(&kit).cloned() else { continue };

        let mut transitions = AnimationTransitions::new();
        transitions.play(&mut player, graph.close, Duration::ZERO);
        if let Ok(mut ec) = commands.get_entity(rig) {
            ec.insert((
                AnimationGraphHandle(graph.graph.clone()),
                transitions,
                DoorRig {
                    door,
                    kit,
                    open: false,
                },
            ));
        }
    }
}

fn drive_doors(
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    players: Query<&GlobalTransform, With<OwnPlayer>>,
    transforms: Query<&GlobalTransform>,
    mut rigs: Query<(&mut DoorRig, &mut AnimationPlayer, &mut AnimationTransitions)>,
    assets: Res<DoorAnimAssets>,
) {
    for (mut rig, mut player, mut transitions) in &mut rigs {
        let Some(graph) = assets.kits.get(&rig.kit) else { continue };
        let Ok(door_t) = transforms.get(rig.door) else { continue };
        let door_pos = door_t.translation();
        let dist = cameras
            .iter()
            .map(|c| c.translation().distance(door_pos))
            .chain(players.iter().map(|p| p.translation().distance(door_pos)))
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(f32::MAX);

        if !rig.open && dist < OPEN_RADIUS {
            rig.open = true;
            transitions.play(&mut player, graph.open, Duration::from_millis(250));
        } else if rig.open && dist > CLOSE_RADIUS {
            rig.open = false;
            transitions.play(&mut player, graph.close, Duration::from_millis(250));
        }
    }
}
