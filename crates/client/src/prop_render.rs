//! Client-side visuals for dynamic props: when an entity gains a
//! `PropShape` (spawned by the server locally in host mode, replicated
//! over the network from M3), attach a mesh and material to it.
//! Also attaches agent (enemy/NPC) models and drives their idle/walk/run
//! animation from actual horizontal motion.

use bevy::ecs::query::Or;
use bevy::gltf::GltfAssetLabel;
use bevy::math::Vec3Swizzles;
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;
use shared::props::PropShape;
use shared::protocol::{Enemy, EnemyAiMode, Item, Npc, Projectile};

pub struct PropRenderPlugin;

impl Plugin for PropRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AiMarkerAssets>().add_systems(
            Update,
            (
                attach_prop_visuals,
                attach_agent_visuals,
                attach_projectile_visuals,
                update_ai_markers,
                drive_agent_animation,
            ),
        );
    }
}

/// Shared mesh + one material per AI mode for the dev marker above enemies.
/// Index = `EnemyAiMode.0`: Patrol, Suspicious, Combat, Search, Cover.
#[derive(Resource)]
struct AiMarkerAssets {
    mesh: Handle<Mesh>,
    hand_mesh: Handle<Mesh>,
    hand_mat: Handle<StandardMaterial>,
    mats: [Handle<StandardMaterial>; 5],
}

impl FromWorld for AiMarkerAssets {
    fn from_world(world: &mut World) -> Self {
        let (mesh, hand_mesh) = {
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            (
                meshes.add(Sphere::new(0.18)),
                meshes.add(Cuboid::new(0.26, 0.03, 0.03)),
            )
        };
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let mut mat = |c: Color, boost: f32| {
            materials.add(StandardMaterial {
                base_color: c,
                emissive: LinearRgba::from(c) * boost,
                unlit: true,
                ..default()
            })
        };
        let hand_mat = mat(Color::srgb(0.05, 0.05, 0.05), 0.0);
        let mats = [
            mat(Color::srgb(0.92, 0.92, 0.92), 2.0), // Patrol — white
            mat(Color::srgb(1.0, 0.85, 0.15), 3.0),  // Suspicious — yellow
            mat(Color::srgb(1.0, 0.12, 0.08), 5.0),  // Combat — red
            mat(Color::srgb(1.0, 0.55, 0.12), 4.0),  // Search — orange
            mat(Color::srgb(0.25, 0.55, 1.0), 4.0),  // Cover — blue
        ];
        Self { mesh, hand_mesh, hand_mat, mats }
    }
}

/// Points from an enemy to its overhead mode-marker child.
#[derive(Component)]
pub struct EnemyMarkerRef(Entity);

/// The overhead mode-marker sphere (dev tool — hidden when dev tools are off).
#[derive(Component)]
pub struct AiMarker;

/// Clock-hand pivot on a marker: rotates a full circle as the enemy's mode
/// countdown runs (grace → Search, Search → Patrol, …); hidden when no timer.
#[derive(Component)]
pub struct AiMarkerHand;

fn attach_prop_visuals(
    mut commands: Commands,
    props: Query<(Entity, &PropShape, Has<Item>), Added<PropShape>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, shape, is_item) in &props {
        let (mesh, color) = match *shape {
            PropShape::Crate { size } => (
                meshes.add(Cuboid::from_size(size)),
                if is_item {
                    // Pickup items glow gold so they stand out.
                    Color::srgb(0.95, 0.78, 0.2)
                } else {
                    Color::srgb(0.65, 0.45, 0.25)
                },
            ),
            PropShape::Ball { radius } => (
                meshes.add(Sphere::new(radius)),
                Color::srgb(0.75, 0.3, 0.3),
            ),
        };
        commands.entity(entity).insert((
            Mesh3d(mesh),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                perceptual_roughness: 0.8,
                ..default()
            })),
        ));
    }
}

/// Clip nodes for one agent's animation graph, stored on the AGENT entity.
#[derive(Component)]
pub struct AgentAnim {
    player_entity: Entity,
    idle: AnimationNodeIndex,
    walk: AnimationNodeIndex,
    run: AnimationNodeIndex,
    current: AnimationNodeIndex,
}

/// Horizontal speed tracker (from Transform deltas — works in host and remote).
#[derive(Component, Default)]
pub struct AgentMotion {
    last: Option<Vec3>,
    speed: f32,
}

/// Spawn the model child for one agent and wire its animation graph
/// (Idle/Walk/Run) once the scene instance is ready.
fn spawn_agent_model(
    commands: &mut Commands,
    asset_server: &AssetServer,
    agent: Entity,
    path: &'static str,
) {
    // Approx scale so ~1.8m tall (models bbox ~3.13 tall)
    let model_scale: f32 = 1.8 / 3.13;
    // visual local y offset to align feet when root is at collider center
    let visual_y_offset: f32 = -0.85;

    let scene_h = asset_server.load(GltfAssetLabel::Scene(0).from_asset(path));
    let gltf_h: Handle<Gltf> = asset_server.load(path);
    let vis = commands
        .spawn((
            SceneRoot(scene_h),
            Transform::from_scale(Vec3::splat(model_scale))
                .with_translation(Vec3::new(0.0, visual_y_offset, 0.0)),
        ))
        .observe(
            move |ready: On<SceneInstanceReady>,
                  mut commands: Commands,
                  gltf_assets: Res<Assets<Gltf>>,
                  mut graphs: ResMut<Assets<AnimationGraph>>,
                  mut players: Query<&mut AnimationPlayer>,
                  children: Query<&Children>| {
                let Some(gltf) = gltf_assets.get(&gltf_h) else {
                    return;
                };
                let root = ready.entity;
                // find AnimationPlayer among descendants (or self)
                let player_e = children
                    .iter_descendants(root)
                    .find(|&e| players.contains(e))
                    .unwrap_or(root);
                let Ok(mut player) = players.get_mut(player_e) else {
                    return;
                };
                let clip_named = |name: &str| {
                    gltf.named_animations
                        .iter()
                        .find(|(n, _)| n.eq_ignore_ascii_case(name))
                        .or_else(|| {
                            gltf.named_animations
                                .iter()
                                .find(|(n, _)| n.to_lowercase().contains(&name.to_lowercase()))
                        })
                        .or_else(|| gltf.named_animations.iter().next())
                        .map(|(_, c)| c.clone())
                };
                let (Some(idle_c), Some(walk_c), Some(run_c)) =
                    (clip_named("Idle"), clip_named("Walk"), clip_named("Run"))
                else {
                    return;
                };
                let mut g = AnimationGraph::new();
                let idle = g.add_clip(idle_c, 1.0, g.root);
                let walk = g.add_clip(walk_c, 1.0, g.root);
                let run = g.add_clip(run_c, 1.0, g.root);
                let gh = graphs.add(g);
                commands.entity(player_e).insert(AnimationGraphHandle(gh));
                player.play(idle).repeat();
                commands.entity(agent).insert((
                    AgentAnim {
                        player_entity: player_e,
                        idle,
                        walk,
                        run,
                        current: idle,
                    },
                    AgentMotion::default(),
                ));
            },
        )
        .id();
    commands.entity(agent).add_child(vis);
}

fn attach_agent_visuals(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    marker_assets: Res<AiMarkerAssets>,
    enemies: Query<Entity, Added<Enemy>>,
    npcs: Query<Entity, Added<Npc>>,
) {
    // Enemy models from the provided pack (self-contained glTF with embedded buffers).
    let enemy_paths = [
        "models/glTF/Goblin_Male.gltf",
        "models/glTF/Zombie_Male.gltf",
        "models/glTF/Soldier_Male.gltf",
    ];
    let npc_paths = [
        "models/glTF/Casual_Male.gltf",
        "models/glTF/Worker_Male.gltf",
    ];

    for (i, entity) in enemies.iter().enumerate() {
        spawn_agent_model(
            &mut commands,
            &asset_server,
            entity,
            enemy_paths[i % enemy_paths.len()],
        );
        // Dev marker above the head showing the replicated AI mode
        // (starts as Patrol/white; update_ai_markers recolors it).
        let marker = commands
            .spawn((
                AiMarker,
                Mesh3d(marker_assets.mesh.clone()),
                MeshMaterial3d(marker_assets.mats[0].clone()),
                Transform::from_xyz(0.0, 1.2, 0.0),
            ))
            .id();
        // Clock hand: a pivot that rotates with the mode countdown, with the
        // hand mesh offset outward so it sweeps like a watch hand.
        let hand_pivot = commands
            .spawn((AiMarkerHand, Transform::IDENTITY, Visibility::Hidden))
            .id();
        let hand_mesh = commands
            .spawn((
                Mesh3d(marker_assets.hand_mesh.clone()),
                MeshMaterial3d(marker_assets.hand_mat.clone()),
                Transform::from_xyz(0.14, 0.24, 0.0),
            ))
            .id();
        commands.entity(hand_pivot).add_child(hand_mesh);
        commands.entity(marker).add_child(hand_pivot);
        commands.entity(entity).add_child(marker);
        commands.entity(entity).insert(EnemyMarkerRef(marker));
    }
    for (i, entity) in npcs.iter().enumerate() {
        spawn_agent_model(
            &mut commands,
            &asset_server,
            entity,
            npc_paths[i % npc_paths.len()],
        );
    }
}

/// Recolor each enemy's overhead marker when its replicated AI mode changes,
/// and sweep the clock hand while a mode countdown is running.
fn update_ai_markers(
    marker_assets: Res<AiMarkerAssets>,
    enemies: Query<
        (&EnemyAiMode, &EnemyMarkerRef),
        Or<(Changed<EnemyAiMode>, Added<EnemyMarkerRef>)>,
    >,
    mut markers: Query<&mut MeshMaterial3d<StandardMaterial>, With<AiMarker>>,
    children: Query<&Children>,
    mut hands: Query<(&mut Transform, &mut Visibility), With<AiMarkerHand>>,
) {
    for (mode, marker) in &enemies {
        let idx = (mode.mode as usize).min(marker_assets.mats.len() - 1);
        if let Ok(mut mat) = markers.get_mut(marker.0) {
            mat.0 = marker_assets.mats[idx].clone();
        }
        let Ok(kids) = children.get(marker.0) else { continue };
        for kid in kids {
            let Ok((mut tf, mut vis)) = hands.get_mut(*kid) else { continue };
            if mode.frac == 0 {
                *vis = Visibility::Hidden;
            } else {
                *vis = Visibility::Inherited;
                // frac counts DOWN (63 → 0); the hand sweeps a full circle
                // over the countdown, hitting 12 o'clock as the mode flips.
                let swept = 1.0 - (mode.frac as f32 / 63.0);
                tf.rotation = Quat::from_rotation_y(-swept * std::f32::consts::TAU);
            }
        }
    }
}

/// Glowing tracer for replicated projectiles: cyan = player fire, red = enemy.
fn attach_projectile_visuals(
    mut commands: Commands,
    projectiles: Query<(Entity, &Projectile), Added<Projectile>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, projectile) in &projectiles {
        let color = if projectile.friendly {
            Color::srgb(0.3, 0.95, 1.0)
        } else {
            Color::srgb(1.0, 0.35, 0.1)
        };
        // Elongated along +Z — the server orients the transform along the
        // flight direction, so the streak reads as a tracer round.
        let mesh = meshes.add(Cuboid::new(0.06, 0.06, 0.55));
        let mat = materials.add(StandardMaterial {
            base_color: color,
            emissive: LinearRgba::from(color) * 8.0,
            unlit: true,
            ..default()
        });
        commands.entity(entity).insert((Mesh3d(mesh), MeshMaterial3d(mat)));
    }
}

/// Switch agents between Idle / Walk / Run from their actual horizontal speed
/// (Transform deltas — server-driven in host mode, interpolated when remote).
fn drive_agent_animation(
    time: Res<Time>,
    mut agents: Query<(&Transform, &mut AgentMotion, &mut AgentAnim)>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let dt = time.delta_secs();
    if dt <= f32::EPSILON {
        return;
    }
    for (transform, mut motion, mut anim) in &mut agents {
        let pos = transform.translation;
        let inst = motion
            .last
            .map(|l| (pos - l).xz().length() / dt)
            .unwrap_or(0.0);
        motion.last = Some(pos);
        // Light smoothing so a single interpolation hiccup doesn't flicker clips.
        motion.speed = motion.speed * 0.8 + inst * 0.2;

        let want = if motion.speed > 3.2 {
            anim.run
        } else if motion.speed > 0.4 {
            anim.walk
        } else {
            anim.idle
        };
        if want != anim.current {
            if let Ok(mut player) = players.get_mut(anim.player_entity) {
                player.stop(anim.current);
                player.play(want).repeat();
                anim.current = want;
            }
        }
    }
}
