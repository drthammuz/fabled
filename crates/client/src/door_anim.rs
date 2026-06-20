//! Secret-door animation prototype.
//!
//! Kenney `gate-door.glb` ships baked glTF clips `open` / `close` (node `door`).
//! Any placed `gate-door` piece (the free-form generator drops one at each hidden
//! room entrance) gets its `open` clip played when the camera comes near and
//! `close` when it leaves — proving the animated secret-door pipeline.
//!
//! Prototype scope: VISUAL only. `gate-door` has no collider (Kenney gate
//! category), so it does not yet *seal* the room — that's the next step
//! (toggle a collider with the animation + network the open state).

use std::time::Duration;

use bevy::gltf::Gltf;
use bevy::prelude::*;

use crate::test_showcase::KenneyModule;

const DOOR_GLB: &str = "models/space/gate-door.glb";
const OPEN_RADIUS: f32 = 5.0;
const CLOSE_RADIUS: f32 = 6.5; // hysteresis so it doesn't flap on the boundary

pub struct DoorAnimPlugin;

impl Plugin for DoorAnimPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DoorAnimAssets>()
            .add_systems(Startup, preload_door)
            .add_systems(Update, build_door_graph)
            // PostUpdate so the scene's AnimationPlayer has spawned (Added fires).
            .add_systems(PostUpdate, (wire_door_rigs, drive_doors).chain());
    }
}

#[derive(Resource, Default)]
struct DoorAnimAssets {
    gltf: Option<Handle<Gltf>>,
    graph: Option<Handle<AnimationGraph>>,
    open: AnimationNodeIndex,
    close: AnimationNodeIndex,
}

/// On a gate-door's AnimationPlayer entity: which door piece + current state.
#[derive(Component)]
struct DoorRig {
    door: Entity,
    open: bool,
}

fn preload_door(asset_server: Res<AssetServer>, mut assets: ResMut<DoorAnimAssets>) {
    assets.gltf = Some(asset_server.load(DOOR_GLB));
}

fn build_door_graph(
    mut assets: ResMut<DoorAnimAssets>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    if assets.graph.is_some() {
        return;
    }
    let Some(handle) = assets.gltf.clone() else { return };
    let Some(gltf) = gltfs.get(&handle) else { return };

    let clip = |needle: &str| {
        gltf.named_animations
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(needle))
            .map(|(_, c)| c.clone())
    };
    let (Some(open), Some(close)) = (clip("open"), clip("close")) else {
        warn!(
            "door anim: no open/close clips in {DOOR_GLB}; have {:?}",
            gltf.named_animations.keys().collect::<Vec<_>>()
        );
        return;
    };

    let mut graph = AnimationGraph::new();
    let root = graph.root;
    assets.open = graph.add_clip(open, 1.0, root);
    assets.close = graph.add_clip(close, 1.0, root);
    assets.graph = Some(graphs.add(graph));
    info!("door anim graph ready (open/close)");
}

fn wire_door_rigs(
    mut commands: Commands,
    assets: Res<DoorAnimAssets>,
    mut new_players: Query<(Entity, &mut AnimationPlayer), Added<AnimationPlayer>>,
    parents: Query<&ChildOf>,
    doors: Query<&KenneyModule>,
) {
    let Some(graph) = assets.graph.clone() else { return };
    for (rig, mut player) in &mut new_players {
        // Walk up the hierarchy from the AnimationPlayer to a gate-door piece.
        let mut current = rig;
        let mut door = None;
        loop {
            if let Ok(module) = doors.get(current) {
                if module.name == "gate-door" || module.name == "gate-door-window" {
                    door = Some(current);
                }
                break;
            }
            match parents.get(current) {
                Ok(p) => current = p.parent(),
                Err(_) => break,
            }
        }
        let Some(door) = door else { continue };

        // Start closed.
        let mut transitions = AnimationTransitions::new();
        transitions.play(&mut player, assets.close, Duration::ZERO);
        if let Ok(mut ec) = commands.get_entity(rig) {
            ec.insert((
                AnimationGraphHandle(graph.clone()),
                transitions,
                DoorRig { door, open: false },
            ));
        }
    }
}

fn drive_doors(
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    transforms: Query<&GlobalTransform>,
    mut rigs: Query<(&mut DoorRig, &mut AnimationPlayer, &mut AnimationTransitions)>,
    assets: Res<DoorAnimAssets>,
) {
    let Some(cam) = cameras.iter().next() else { return };
    let cam_pos = cam.translation();

    for (mut rig, mut player, mut transitions) in &mut rigs {
        let Ok(door_t) = transforms.get(rig.door) else { continue };
        let dist = door_t.translation().distance(cam_pos);

        if !rig.open && dist < OPEN_RADIUS {
            rig.open = true;
            transitions.play(&mut player, assets.open, Duration::from_millis(250));
        } else if rig.open && dist > CLOSE_RADIUS {
            rig.open = false;
            transitions.play(&mut player, assets.close, Duration::from_millis(250));
        }
    }
}
