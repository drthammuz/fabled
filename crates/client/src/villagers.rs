//! Villager visuals: KayKit character models (CC0) per profession, with a
//! profession color tint and idle/walk animations driven by the replicated
//! `VillagerState`. Positions come from `NetTransform` and are smoothed by
//! the same interpolation buffer the players use.

use std::collections::HashMap;
use std::time::Duration;

use bevy::gltf::Gltf;
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;
use shared::protocol::{Villager, VillagerState};

pub struct VillagersPlugin;

impl Plugin for VillagersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CharacterLibrary>()
            .add_systems(Startup, load_character_library)
            .add_systems(
                Update,
                (
                    build_animation_graphs,
                    attach_villager_visuals,
                    hook_animation_players,
                    drive_animations,
                ),
            );
    }
}

/// Which model and tint a profession uses.
fn profession_style(profession: &str) -> (&'static str, Color) {
    match profession {
        "farmer" => ("models/Barbarian.glb", Color::srgb(0.55, 0.75, 0.4)),
        "farmhand" => ("models/Barbarian.glb", Color::srgb(0.8, 0.65, 0.35)),
        "fisher" => ("models/Rogue.glb", Color::srgb(0.45, 0.6, 0.75)),
        "baker" => ("models/Rogue.glb", Color::srgb(0.9, 0.85, 0.7)),
        "tavern_keeper" => ("models/Barbarian.glb", Color::srgb(0.85, 0.4, 0.35)),
        "guard" => ("models/Knight.glb", Color::srgb(0.6, 0.65, 0.8)),
        "mayor" => ("models/Mage.glb", Color::srgb(0.7, 0.5, 0.8)),
        "elder" => ("models/Rogue_Hooded.glb", Color::srgb(0.6, 0.6, 0.6)),
        _ => ("models/Rogue.glb", Color::WHITE),
    }
}

struct CharacterEntry {
    gltf: Handle<Gltf>,
    scene: Handle<Scene>,
    /// Built once the gltf is loaded.
    graph: Option<Handle<AnimationGraph>>,
    idle: AnimationNodeIndex,
    walk: AnimationNodeIndex,
}

#[derive(Resource, Default)]
struct CharacterLibrary(HashMap<&'static str, CharacterEntry>);

fn load_character_library(asset_server: Res<AssetServer>, mut library: ResMut<CharacterLibrary>) {
    for path in [
        "models/Barbarian.glb",
        "models/Knight.glb",
        "models/Mage.glb",
        "models/Rogue.glb",
        "models/Rogue_Hooded.glb",
    ] {
        library.0.insert(
            path,
            CharacterEntry {
                gltf: asset_server.load(path),
                scene: asset_server.load(GltfAssetLabel::Scene(0).from_asset(path)),
                graph: None,
                idle: AnimationNodeIndex::default(),
                walk: AnimationNodeIndex::default(),
            },
        );
    }
}

/// Once each gltf is loaded, build an Idle/Walk animation graph from its
/// named clips.
fn build_animation_graphs(
    mut library: ResMut<CharacterLibrary>,
    gltf_assets: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    for (path, entry) in library.0.iter_mut() {
        if entry.graph.is_some() {
            continue;
        }
        let Some(gltf) = gltf_assets.get(&entry.gltf) else {
            continue;
        };
        let pick = |needles: &[&str]| -> Option<Handle<AnimationClip>> {
            for needle in needles {
                if let Some((_, clip)) = gltf
                    .named_animations
                    .iter()
                    .find(|(name, _)| name.as_ref() == *needle)
                {
                    return Some(clip.clone());
                }
            }
            // Fallback: substring match.
            for needle in needles {
                if let Some((_, clip)) = gltf
                    .named_animations
                    .iter()
                    .find(|(name, _)| name.to_lowercase().contains(&needle.to_lowercase()))
                {
                    return Some(clip.clone());
                }
            }
            None
        };
        let idle = pick(&["Idle", "Idle_A"]);
        let walk = pick(&["Walking_A", "Walking_B", "Walk"]);
        let (Some(idle), Some(walk)) = (idle, walk) else {
            let names: Vec<&str> = gltf.named_animations.keys().map(|k| k.as_ref()).collect();
            warn!("{path}: no idle/walk clips found; available: {names:?}");
            continue;
        };
        let mut graph = AnimationGraph::new();
        let root = graph.root;
        let idle_node = graph.add_clip(idle, 1.0, root);
        let walk_node = graph.add_clip(walk, 1.0, root);
        entry.idle = idle_node;
        entry.walk = walk_node;
        entry.graph = Some(graphs.add(graph));
        info!("{path}: animation graph ready");
    }
}

/// Tracks which model a villager uses, for graph lookups.
#[derive(Component)]
struct VillagerModel(&'static str);

/// The AnimationPlayer inside this villager's scene instance.
#[derive(Component)]
struct VillagerRig(Entity);

/// Marks the scene child so we can find the villager root from rig events.
#[derive(Component)]
struct SceneOf(Entity);

fn attach_villager_visuals(
    mut commands: Commands,
    villagers: Query<(Entity, &Villager), Added<Villager>>,
    library: Res<CharacterLibrary>,
) {
    for (entity, villager) in &villagers {
        let (path, tint) = profession_style(&villager.profession);
        let Some(entry) = library.0.get(path) else {
            continue;
        };
        // The KayKit rigs stand ~2.3 units tall in scene space (the raw
        // mesh bounds are taller, but the skeleton pulls the legs up and
        // plants the feet at y=0). Scale to ~1.7 m, just under the player.
        const SCALE: f32 = 1.7 / 2.3;
        let scene = commands
            .spawn((
                SceneRoot(entry.scene.clone()),
                Transform::from_scale(Vec3::splat(SCALE)),
                SceneOf(entity),
            ))
            .observe(
                move |ready: On<SceneInstanceReady>,
                      mut commands: Commands,
                      children: Query<&Children>,
                      mesh_materials: Query<&MeshMaterial3d<StandardMaterial>>,
                      mut materials: ResMut<Assets<StandardMaterial>>| {
                    // Tint every material in the freshly spawned scene.
                    for descendant in children.iter_descendants(ready.entity) {
                        if let Ok(material_handle) = mesh_materials.get(descendant) {
                            if let Some(material) = materials.get(&material_handle.0) {
                                let mut tinted = material.clone();
                                tinted.base_color = tint;
                                let handle = materials.add(tinted);
                                commands
                                    .entity(descendant)
                                    .insert(MeshMaterial3d(handle));
                            }
                        }
                    }
                },
            )
            .id();
        commands
            .entity(entity)
            .insert((VillagerModel(path), Visibility::default()))
            .add_child(scene);
        info!("villager visual attached: {} ({})", villager.name, villager.profession);
    }
}

/// When the scene instance spawns its AnimationPlayer, wire it to the
/// model's graph and start idling.
fn hook_animation_players(
    mut commands: Commands,
    mut new_players: Query<(Entity, &mut AnimationPlayer), Added<AnimationPlayer>>,
    parents: Query<&ChildOf>,
    scene_links: Query<&SceneOf>,
    models: Query<&VillagerModel>,
    library: Res<CharacterLibrary>,
) {
    for (rig, mut player) in &mut new_players {
        // Walk up to find the scene child (SceneOf), then the villager root.
        let mut current = rig;
        let mut villager = None;
        loop {
            if let Ok(link) = scene_links.get(current) {
                villager = Some(link.0);
                break;
            }
            match parents.get(current) {
                Ok(parent) => current = parent.parent(),
                Err(_) => break,
            }
        }
        let Some(villager) = villager else { continue };
        let Ok(model) = models.get(villager) else { continue };
        let Some(entry) = library.0.get(model.0) else { continue };
        let Some(graph) = entry.graph.clone() else { continue };

        let mut transitions = AnimationTransitions::new();
        transitions
            .play(&mut player, entry.idle, Duration::ZERO)
            .repeat();
        commands
            .entity(rig)
            .insert((AnimationGraphHandle(graph), transitions));
        commands.entity(villager).insert(VillagerRig(rig));
    }
}

/// Switch idle/walk when the replicated state changes.
fn drive_animations(
    changed: Query<(&VillagerState, &VillagerModel, &VillagerRig), Changed<VillagerState>>,
    library: Res<CharacterLibrary>,
    mut rigs: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    for (state, model, rig) in &changed {
        let Some(entry) = library.0.get(model.0) else { continue };
        let Ok((mut player, mut transitions)) = rigs.get_mut(rig.0) else {
            continue;
        };
        let node = if state.walking { entry.walk } else { entry.idle };
        transitions
            .play(&mut player, node, Duration::from_millis(250))
            .repeat();
    }
}
