//! Dev-tools overlay (B): AI mode markers, per-enemy awareness range
//! indicators, and the debug text HUDs — all toggleable at once.
//!
//! The range indicators are drawn from `shared::ai_tuning` (the same
//! constants the server AI uses) and are personalized to YOUR current
//! movement: the hearing ring grows when you sprint and shrinks when you
//! sneak, and the vision cone shortens while you crouch. Alerted enemies
//! (combat/search/cover) show boosted ranges, mirroring high-alert senses.
//! Line-of-sight still applies in the real perception check — the shapes
//! show range, not wall occlusion.

use bevy::prelude::*;
use shared::ai_tuning as tune;
use shared::protocol::{Enemy, EnemyAiMode};

use crate::editor_playtest::PlaytestCoordsHud;
use crate::prop_render::AiMarker;

pub struct DevToolsPlugin;

impl Plugin for DevToolsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DevTools>()
            .init_resource::<IndicatorAssets>()
            .add_systems(
                Update,
                (toggle_dev_tools, attach_range_indicators, update_range_indicators)
                    .chain(),
            );
    }
}

/// Master switch for dev overlays (markers, rings, debug text). B toggles.
#[derive(Resource)]
pub struct DevTools {
    pub enabled: bool,
}

impl Default for DevTools {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// Hearing-range ring around an enemy (radius = how far IT would hear YOU
/// moving the way you currently move).
#[derive(Component)]
struct HearRing(Entity);

/// Vision cone in the enemy's facing direction (length = how far it would
/// spot you, given your stance).
#[derive(Component)]
struct VisionCone(Entity);

#[derive(Resource)]
struct IndicatorAssets {
    ring: Handle<Mesh>,
    cone: Handle<Mesh>,
    ring_mat: Handle<StandardMaterial>,
    cone_mat: Handle<StandardMaterial>,
}

impl FromWorld for IndicatorAssets {
    fn from_world(world: &mut World) -> Self {
        let (ring, cone) = {
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            (
                // Unit-radius shapes; scaled per frame to the live range.
                meshes.add(Annulus::new(0.96, 1.0)),
                meshes.add(CircularSector::new(1.0, tune::VISION_CONE_HALF_RAD)),
            )
        };
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let mut mat = |c: Color| {
            materials.add(StandardMaterial {
                base_color: c,
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                double_sided: true,
                ..default()
            })
        };
        Self {
            ring_mat: mat(Color::srgba(0.25, 0.85, 1.0, 0.35)),
            cone_mat: mat(Color::srgba(1.0, 0.9, 0.2, 0.16)),
            ring,
            cone,
        }
    }
}

fn toggle_dev_tools(
    keys: Res<ButtonInput<KeyCode>>,
    mut dev: ResMut<DevTools>,
    capture: Res<crate::netplay::InputCapture>,
) {
    // B, not an F-key: the user's keyboard needs an Fn chord for F-keys.
    if !capture.0 && keys.just_pressed(KeyCode::KeyB) {
        dev.enabled = !dev.enabled;
        info!("dev tools: {}", if dev.enabled { "ON" } else { "OFF" });
    }
}

/// Give every new enemy a flat hearing ring + vision cone at its feet.
fn attach_range_indicators(
    mut commands: Commands,
    assets: Res<IndicatorAssets>,
    enemies: Query<Entity, Added<Enemy>>,
) {
    // Lay the 2D shapes flat: +Y (mesh "up") → +Z (enemy forward).
    let flat = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    for enemy in &enemies {
        let ring = commands
            .spawn((
                HearRing(enemy),
                Mesh3d(assets.ring.clone()),
                MeshMaterial3d(assets.ring_mat.clone()),
                Transform::from_xyz(0.0, -0.72, 0.0).with_rotation(flat),
            ))
            .id();
        let cone = commands
            .spawn((
                VisionCone(enemy),
                Mesh3d(assets.cone.clone()),
                MeshMaterial3d(assets.cone_mat.clone()),
                Transform::from_xyz(0.0, -0.70, 0.0).with_rotation(flat),
            ))
            .id();
        commands.entity(enemy).add_child(ring);
        commands.entity(enemy).add_child(cone);
    }
}

/// True while any alerted mode boosts the enemy's senses (approximates the
/// server's high-alert window client-side).
fn alerted(mode: u8) -> bool {
    matches!(mode, 2 | 3 | 4) // Combat | Search | Cover
}

/// Scale each enemy's indicators to the range at which it would actually
/// hear/see THIS player right now, and apply the master toggle to every
/// dev overlay (markers, rings, debug text HUDs).
#[allow(clippy::type_complexity)]
fn update_range_indicators(
    dev: Res<DevTools>,
    keys: Res<ButtonInput<KeyCode>>,
    modes: Query<&EnemyAiMode>,
    mut rings: Query<(&HearRing, &mut Transform, &mut Visibility)>,
    mut cones: Query<(&VisionCone, &mut Transform, &mut Visibility), Without<HearRing>>,
    mut markers: Query<
        &mut Visibility,
        (With<AiMarker>, Without<HearRing>, Without<VisionCone>),
    >,
    mut debug_texts: Query<
        &mut Visibility,
        (
            With<PlaytestCoordsHud>,
            Without<AiMarker>,
            Without<HearRing>,
            Without<VisionCone>,
        ),
    >,
) {
    let show = dev.enabled;
    for mut vis in &mut markers {
        *vis = if show { Visibility::Inherited } else { Visibility::Hidden };
    }
    for mut vis in &mut debug_texts {
        *vis = if show { Visibility::Inherited } else { Visibility::Hidden };
    }

    // My current movement mode — same key mapping netplay sends the server.
    let moving = keys.pressed(KeyCode::KeyW)
        || keys.pressed(KeyCode::KeyA)
        || keys.pressed(KeyCode::KeyS)
        || keys.pressed(KeyCode::KeyD);
    let sprint = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let crouch = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let hear_base = tune::hearing_radius(moving, sprint, crouch);

    for (ring, mut tf, mut vis) in &mut rings {
        let mode = modes.get(ring.0).map(|m| m.mode).unwrap_or(0);
        let mult = if alerted(mode) { tune::ALERT_SENSE_MULT } else { 1.0 };
        let r = hear_base * mult;
        if !show || r <= 0.01 {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Inherited;
        tf.scale = Vec3::new(r, r, 1.0);
    }
    for (cone, mut tf, mut vis) in &mut cones {
        let mode = modes.get(cone.0).map(|m| m.mode).unwrap_or(0);
        let mult = if alerted(mode) { tune::ALERT_SENSE_MULT } else { 1.0 };
        let crouch_mult = if crouch { tune::CROUCH_VIS_MULT } else { 1.0 };
        let r = tune::VISION_RANGE * mult * crouch_mult;
        *vis = if show { Visibility::Inherited } else { Visibility::Hidden };
        tf.scale = Vec3::new(r, r, 1.0);
    }
}
