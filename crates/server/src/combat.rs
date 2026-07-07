//! Player health, enemies, combat, and enemy perception/AI.
//!
//! Enemy AI is a small state machine driven by two senses evaluated per
//! enemy × per alive player (multiplayer-correct):
//! - VISION: a facing cone + line-of-sight raycast against level geometry.
//! - HEARING: radius tiered by how the player moves (sprint > walk > sneak),
//!   plus one-shot noises (gunshots, impacts) routed through [`PendingNoises`].
//!
//! Modes: Patrol → Suspicious (investigate noise) → Combat (seen; ranged fire)
//! → Search (lost contact; hold last-known position on high alert) → Patrol.
//! Combat can briefly divert to Cover when shots land close. On first contact
//! an enemy shout-alerts non-engaged allies nearby. The current mode is
//! replicated as [`EnemyAiMode`] for the dev marker above each enemy.

use avian3d::{math::*, prelude::*};
use bevy::ecs::query::Or;
use bevy::math::Vec3Swizzles;
use bevy::prelude::Dir3;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use shared::config;
use shared::items;
use shared::protocol::{
    Enemy, EnemyAiMode, NetTransform, Npc, Player, PlayerAlive, PlayerHealth, PlayerName,
    Projectile,
};

use crate::character::CharacterSystems;
use crate::items::Inventory;
use crate::level::LevelEntity;
use crate::nav::EnemyNav;
use crate::players::LatestInput;
use crate::run::RunEntity;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EnemyNav>()
            .init_resource::<PendingNoises>()
            .add_systems(
                FixedUpdate,
                (
                    player_attacks,
                    projectile_sim,
                    enemy_perceive_and_think,
                    enemy_navigate,
                    enemy_locomotion,
                    enemy_fire,
                    enemy_damage_players,
                    sync_ai_mode,
                    sync_player_health,
                )
                    .chain()
                    .after(CharacterSystems)
                    .run_if(in_state(ClientState::Disconnected)),
            );
    }
}

const AGENT_GRAVITY: f32 = -20.0;
/// How far below the feet the ground probe still snaps (stairs/ramps downhill).
const GROUND_SNAP: f32 = 0.6;
const REPATH_INTERVAL: f32 = 0.7;
const WAYPOINT_REACHED: f32 = 0.9;

// ── Perception tuning — SHARED with the client dev-tools indicators ─────────
// (edit in shared/src/ai_tuning.rs so the debug rings stay truthful)
use shared::ai_tuning::{
    hearing_radius, ALERT_SENSE_MULT, CROUCH_VIS_MULT, GUNSHOT_NOISE, VISION_CONE_COS,
    VISION_RANGE,
};

// ── State machine timers (seconds) ──────────────────────────────────────────
/// Combat is held this long after losing sight before dropping to Search.
const COMBAT_GRACE: f32 = 1.5;
const SEARCH_SECS: f32 = 10.0;
const SUSPICIOUS_SECS: f32 = 6.0;
/// Senses stay boosted this long after last contact.
const HIGH_ALERT_SECS: f32 = 20.0;
const COVER_MIN_SECS: f32 = 1.2;
/// Minimum gap between two cover dives.
const COVER_COOLDOWN: f32 = 4.0;
const ALARM_RADIUS: f32 = 14.0;

// ── Weapons ──────────────────────────────────────────────────────────────────
const ENEMY_FIRE_RANGE: f32 = 20.0;
const ENEMY_FIRE_COOLDOWN: f32 = 1.1;
const ENEMY_PROJ_SPEED: f32 = 22.0;
const ENEMY_PROJ_DAMAGE: f32 = 12.0;
/// Aim jitter (radians) so enemies are dangerous but dodgeable.
const ENEMY_AIM_SPREAD: f32 = 0.05;
const PLAYER_PROJ_SPEED: f32 = 45.0;
const PLAYER_PROJ_DAMAGE: f32 = 12.0;
/// Player muzzle in look-space relative to the eye (x=right, y=up, z: -=fwd).
const PLAYER_MUZZLE_OFFSET: Vec3 = Vec3::new(0.22, -0.25, -0.45);
const PROJECTILE_TTL: f32 = 2.5;
/// A friendly projectile impacting within this range makes an enemy consider cover.
const NEAR_MISS_RADIUS: f32 = 4.0;

/// AI mode of one enemy. Mirrored to clients as `EnemyAiMode(u8)`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AiMode {
    #[default]
    Patrol,
    Suspicious,
    Combat,
    Search,
    Cover,
}

impl AiMode {
    fn as_u8(self) -> u8 {
        match self {
            AiMode::Patrol => 0,
            AiMode::Suspicious => 1,
            AiMode::Combat => 2,
            AiMode::Search => 3,
            AiMode::Cover => 4,
        }
    }
}

/// One-shot noises (gunshots, projectile impacts) heard by every enemy in
/// radius on the next perception tick. Cleared after consumption.
#[derive(Resource, Default)]
pub struct PendingNoises(pub Vec<(Vec3, f32)>);

#[derive(Component)]
pub struct EnemyBrain {
    home: Vec3,
    /// Current world-space steering point (waypoint or wander spot).
    steer: Vec3,
    /// Remaining A* waypoints (cell indices), front = next.
    path: Vec<(i32, i32)>,
    /// Cell the current path was computed toward.
    goal_cell: Option<(i32, i32)>,
    repath: f32,
    cooldown: f32,
    /// Vertical velocity (simple gravity — agents never jump).
    vy: f32,
    /// Body half height (feet-to-centre) for ground probes.
    half_h: f32,
    // ── AI state ────────────────────────────────────────────────────────────
    pub mode: AiMode,
    /// Time remaining in the current mode (meaning depends on mode).
    mode_timer: f32,
    /// Full duration the current countdown started from (for the marker clock).
    mode_total: f32,
    /// Last position a player was seen or heard at.
    last_known: Option<Vec3>,
    /// True this tick if the combat target is directly visible.
    sees_target: bool,
    /// Senses stay boosted while > 0.
    high_alert: f32,
    /// Set when a player projectile lands nearby: where it came from.
    threat_from: Option<Vec3>,
    cover_cd: f32,
    fire_cd: f32,
    /// Baked patrol waypoints (world). Empty = wander around `home`.
    patrol: Vec<Vec3>,
    patrol_i: usize,
    /// Idle pause at a reached patrol waypoint.
    patrol_wait: f32,
}

impl EnemyBrain {
    pub fn at(position: Vec3) -> Self {
        Self::sized(position, 0.85)
    }

    pub fn sized(position: Vec3, half_h: f32) -> Self {
        Self {
            home: position,
            steer: position,
            path: Vec::new(),
            goal_cell: None,
            repath: 0.0,
            cooldown: 0.0,
            vy: 0.0,
            half_h,
            mode: AiMode::Patrol,
            mode_timer: 0.0,
            mode_total: 0.0,
            last_known: None,
            sees_target: false,
            high_alert: 0.0,
            threat_from: None,
            cover_cd: 0.0,
            fire_cd: 0.0,
            patrol: Vec::new(),
            patrol_i: 0,
            patrol_wait: 0.0,
        }
    }

    pub fn with_patrol(mut self, patrol: Vec<Vec3>) -> Self {
        self.patrol = patrol;
        self
    }
}

#[derive(Component)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            current: 100.0,
            max: 100.0,
        }
    }
}

/// Server-simulated bullet: straight flight, ray-swept each tick.
#[derive(Component)]
pub struct ProjectileSim {
    vel: Vec3,
    ttl: f32,
    damage: f32,
    shooter: Entity,
    /// Where it was fired from (cover direction for threatened enemies).
    origin: Vec3,
}

/// Facing-cone check: `to_target` need not be normalized.
fn in_vision_cone(forward: Vec3, to_target: Vec3, range: f32) -> bool {
    let dist = to_target.length();
    if dist > range || dist < 1e-3 {
        // Point-blank always counts (standing inside someone's face).
        return dist < 1e-3;
    }
    forward.dot(to_target / dist) > VISION_CONE_COS
}

/// Sense players and run the mode state machine for every enemy.
#[allow(clippy::too_many_arguments)]
fn enemy_perceive_and_think(
    time: Res<Time>,
    spatial: SpatialQuery,
    nav: Res<EnemyNav>,
    mut noises: ResMut<PendingNoises>,
    liquids: Query<Entity, With<crate::liquids::Liquid>>,
    agents: Query<Entity, Or<(With<Enemy>, With<Npc>)>>,
    players: Query<(Entity, &Transform, &LatestInput, &PlayerAlive), With<Player>>,
    mut enemies: Query<(Entity, &Transform, &mut EnemyBrain), With<Enemy>>,
) {
    let dt = time.delta_secs();
    // LOS rays only care about level geometry: skip liquid sensors and every
    // agent/player body so a crowd can't "block" vision.
    let mut los_excluded: Vec<Entity> = liquids.iter().collect();
    los_excluded.extend(agents.iter());
    los_excluded.extend(players.iter().map(|(e, ..)| e));
    let los_filter = SpatialQueryFilter::from_excluded_entities(los_excluded);

    // Alarm shouts collected during the loop, applied after (can't alias the query).
    let mut alarms: Vec<(Vec3, Vec3)> = Vec::new(); // (shouter pos, last_known)

    for (_e, etransform, mut brain) in &mut enemies {
        brain.high_alert = (brain.high_alert - dt).max(0.0);
        brain.cover_cd = (brain.cover_cd - dt).max(0.0);
        let alert_mult = if brain.high_alert > 0.0 { ALERT_SENSE_MULT } else { 1.0 };
        let eye = etransform.translation + Vec3::Y * (brain.half_h * 0.7);
        let forward = etransform.rotation * Vec3::Z;

        // ── Senses ──
        let mut seen: Option<Vec3> = None;
        let mut seen_dist = f32::MAX;
        let mut heard: Option<Vec3> = None;
        let mut heard_dist = f32::MAX;
        for (_pe, ptransform, input, alive) in &players {
            if !alive.0 {
                continue;
            }
            let ppos = ptransform.translation;
            let to = ppos - etransform.translation;
            let dist = to.length();

            let mut vis_range = VISION_RANGE * alert_mult;
            if input.0.crouch {
                vis_range *= CROUCH_VIS_MULT;
            }
            if in_vision_cone(forward, to, vis_range) && dist < seen_dist {
                // Eye-to-eye ray against geometry only.
                let target_eye = ppos + Vec3::Y * 0.4;
                let ray = target_eye - eye;
                let blocked = Dir3::new(ray.normalize_or_zero()).ok().is_some_and(|dir| {
                    spatial
                        .cast_ray(
                            eye.adjust_precision(),
                            dir,
                            (ray.length() - 0.1).max(0.0) as Scalar,
                            true,
                            &los_filter,
                        )
                        .is_some()
                });
                if !blocked {
                    seen = Some(ppos);
                    seen_dist = dist;
                }
            }

            let moving = input.0.move_dir.length_squared() > 0.01;
            let hear = hearing_radius(moving, input.0.sprint, input.0.crouch) * alert_mult;
            if dist < hear && dist < heard_dist {
                heard = Some(ppos);
                heard_dist = dist;
            }
        }
        for &(npos, radius) in &noises.0 {
            let dist = etransform.translation.distance(npos);
            if dist < radius * alert_mult && dist < heard_dist {
                heard = Some(npos);
                heard_dist = dist;
            }
        }

        // ── Transitions ──
        brain.sees_target = seen.is_some();
        if let Some(ppos) = seen {
            let first_contact =
                !matches!(brain.mode, AiMode::Combat | AiMode::Cover);
            brain.last_known = Some(ppos);
            brain.high_alert = HIGH_ALERT_SECS;
            if brain.mode != AiMode::Cover {
                brain.mode = AiMode::Combat;
                brain.mode_timer = COMBAT_GRACE;
                brain.mode_total = COMBAT_GRACE;
            }
            if first_contact {
                alarms.push((etransform.translation, ppos));
            }
        } else {
            match brain.mode {
                AiMode::Combat => {
                    if let Some(h) = heard {
                        brain.last_known = Some(h);
                        brain.mode_timer = COMBAT_GRACE;
                        brain.mode_total = COMBAT_GRACE;
                    } else {
                        brain.mode_timer -= dt;
                        if brain.mode_timer <= 0.0 {
                            brain.mode = AiMode::Search;
                            brain.mode_timer = SEARCH_SECS;
                            brain.mode_total = SEARCH_SECS;
                        }
                    }
                }
                AiMode::Search | AiMode::Suspicious => {
                    if let Some(h) = heard {
                        brain.last_known = Some(h);
                        brain.mode_timer = brain.mode_timer.max(3.0);
                    }
                    brain.mode_timer -= dt;
                    if brain.mode_timer <= 0.0 {
                        brain.mode = AiMode::Patrol;
                        brain.mode_total = 0.0;
                        brain.last_known = None;
                    }
                }
                AiMode::Patrol => {
                    if let Some(h) = heard {
                        brain.mode = AiMode::Suspicious;
                        brain.mode_timer = SUSPICIOUS_SECS;
                        brain.mode_total = SUSPICIOUS_SECS;
                        brain.last_known = Some(h);
                    }
                }
                AiMode::Cover => {
                    brain.mode_timer -= dt;
                    if brain.mode_timer <= 0.0 {
                        brain.mode = if brain.last_known.is_some() {
                            AiMode::Combat
                        } else {
                            AiMode::Patrol
                        };
                        brain.mode_timer = COMBAT_GRACE;
                        brain.mode_total = COMBAT_GRACE;
                    }
                }
            }
        }

        // ── Cover: dive when shots landed close and a walled cell is near ──
        if let Some(threat) = brain.threat_from.take() {
            if brain.mode == AiMode::Combat && brain.cover_cd <= 0.0 && !nav.is_empty() {
                if let Some(cur) = nav.nearest_cell(etransform.translation) {
                    if let Some(cell) = nav.cover_cell_from(cur, threat, 2) {
                        brain.mode = AiMode::Cover;
                        let seed = (etransform.translation.x * 13.0
                            + etransform.translation.z * 7.0) as u32;
                        brain.mode_timer = COVER_MIN_SECS + pseudo_rand(seed) * 1.2;
                        brain.mode_total = brain.mode_timer;
                        brain.cover_cd = COVER_COOLDOWN;
                        brain.steer = nav.world_of(cell);
                        brain.path.clear();
                        brain.goal_cell = Some(cell);
                    }
                }
            }
        }
    }

    // Shout: allies near a first-contact enemy converge on the sighting
    // (Patrol/Suspicious only — engaged enemies keep their own target).
    for (shouter, last_known) in alarms {
        for (_e, etransform, mut brain) in &mut enemies {
            if !matches!(brain.mode, AiMode::Patrol | AiMode::Suspicious) {
                continue;
            }
            if etransform.translation.distance(shouter) > ALARM_RADIUS {
                continue;
            }
            brain.mode = AiMode::Search;
            brain.mode_timer = SEARCH_SECS;
            brain.mode_total = SEARCH_SECS;
            brain.last_known = Some(last_known);
            brain.high_alert = HIGH_ALERT_SECS;
        }
    }

    noises.0.clear();
}

/// Decide WHERE each agent is heading based on its mode: patrol waypoints,
/// investigate/search points, chase paths, or nav-aware wandering.
fn enemy_navigate(
    time: Res<Time>,
    nav: Res<EnemyNav>,
    mut enemies: Query<(&Transform, &mut EnemyBrain), Or<(With<Enemy>, With<Npc>)>>,
) {
    let dt = time.delta_secs();
    for (transform, mut brain) in &mut enemies {
        brain.cooldown = (brain.cooldown - dt).max(0.0);
        brain.repath = (brain.repath - dt).max(0.0);
        let pos = transform.translation;
        // The nav grid covers floor 0 only. Agents on other levels (hub NPCs
        // at y −4, hidden shafts) must NOT path toward floor-0 cells — that
        // steered hub NPCs across the drop holes and off the edge.
        let off_grid = nav.is_empty()
            || nav
                .nearest_cell(pos)
                .map_or(true, |c| (nav.world_of(c).y - pos.y).abs() > 2.0);

        match brain.mode {
            AiMode::Combat => {
                if let Some(target) = brain.last_known {
                    // Ranged: hold ground once the target is visible and in
                    // range; close distance otherwise.
                    let dist = (target - pos).xz().length();
                    if brain.sees_target && dist < ENEMY_FIRE_RANGE * 0.8 {
                        brain.path.clear();
                        brain.goal_cell = None;
                        brain.steer = pos;
                    } else {
                        navigate_toward(&nav, off_grid, pos, target, &mut brain);
                    }
                    continue;
                }
            }
            AiMode::Suspicious => {
                if let Some(target) = brain.last_known {
                    navigate_toward(&nav, off_grid, pos, target, &mut brain);
                    continue;
                }
            }
            AiMode::Search => {
                if let Some(target) = brain.last_known {
                    let arrived = (target - pos).xz().length() < 1.5;
                    if !arrived {
                        navigate_toward(&nav, off_grid, pos, target, &mut brain);
                        continue;
                    }
                    // At the last-known spot: sweep the area instead of
                    // standing still — short hops between nearby cells.
                    // Repick strictly on the cooldown, never on arrival alone:
                    // a hop that lands (or fails) near the agent would otherwise
                    // re-roll a new random direction EVERY frame (time-seeded),
                    // making the agent flicker left/right in place.
                    if brain.cooldown <= 0.0 {
                        let seed = (pos.x * 23.0 + pos.z * 41.0
                            + time.elapsed_secs() * 11.0) as u32;
                        if !off_grid {
                            if let Some(anchor) = nav.nearest_cell(target) {
                                if let Some(goal) = nav.random_nearby_cell(anchor, 2, seed) {
                                    if let Some(cur) = nav.nearest_cell(pos) {
                                        if let Some(path) = nav.find_path(cur, goal) {
                                            brain.path = path;
                                            brain.goal_cell = Some(goal);
                                        }
                                    }
                                }
                            }
                        } else {
                            let r1 = pseudo_rand(seed);
                            let r2 = pseudo_rand(seed.wrapping_add(5));
                            let a = r1 * std::f32::consts::TAU;
                            let d = 1.0 + r2 * 2.0;
                            brain.steer = target + Vec3::new(a.cos() * d, 0.0, a.sin() * d);
                        }
                        brain.cooldown = 1.2 + pseudo_rand(brain.patrol_i as u32 ^ seed) * 1.5;
                    }
                    advance_waypoints(&nav, pos, &mut brain);
                    continue;
                }
            }
            AiMode::Cover => {
                // Steer point was set when the dive started; just walk there.
                advance_waypoints(&nav, pos, &mut brain);
                continue;
            }
            AiMode::Patrol => {
                if !brain.patrol.is_empty() {
                    let wp = brain.patrol[brain.patrol_i % brain.patrol.len()];
                    let arrived = (wp - pos).xz().length() < 1.2;
                    if arrived {
                        brain.patrol_wait -= dt;
                        if brain.patrol_wait <= 0.0 {
                            let n = brain.patrol.len();
                            brain.patrol_i = (brain.patrol_i + 1) % n;
                            let seed = (pos.x * 17.0 + pos.z * 31.0) as u32;
                            brain.patrol_wait = 2.0 + pseudo_rand(seed) * 2.5;
                        }
                        brain.steer = pos;
                    } else {
                        navigate_toward(&nav, off_grid, pos, wp, &mut brain);
                    }
                    continue;
                }
            }
        }

        // Wander (patrol without a route / NPCs): pick a new nearby spot when
        // the timer lapses. (Cooldown-only for the same reason as Search —
        // "arrived" can hold true forever and re-roll a direction per frame.)
        if brain.cooldown <= 0.0 {
            let seed = (pos.x * 17.0 + pos.z * 31.0 + time.elapsed_secs() * 7.0) as u32;
            let r1 = pseudo_rand(seed);
            if !off_grid {
                if let Some(cur) = nav.nearest_cell(pos) {
                    if let Some(goal) = nav.random_nearby_cell(cur, 4, seed) {
                        if let Some(path) = nav.find_path(cur, goal) {
                            brain.path = path;
                            brain.goal_cell = Some(goal);
                        }
                    }
                }
            } else {
                // Off-grid (hub/testmap): tight home-radius wander so agents
                // idle around their post instead of roaming toward edges.
                let r2 = pseudo_rand(seed.wrapping_add(99));
                let angle = r1 * std::f32::consts::TAU;
                let dist = 1.0 + r2 * 1.5;
                brain.steer =
                    brain.home + Vec3::new(angle.cos() * dist, 0.0, angle.sin() * dist);
            }
            brain.cooldown = 2.5 + r1 * 2.5;
        }
        advance_waypoints(&nav, pos, &mut brain);
    }
}

/// Plan/refresh a path toward a world target and set the current steer point.
fn navigate_toward(nav: &EnemyNav, off_grid: bool, pos: Vec3, target: Vec3, brain: &mut EnemyBrain) {
    if off_grid {
        // Direct pursuit off the baked grid — horizontal only (gravity owns Y).
        brain.steer = Vec3::new(target.x, pos.y, target.z);
        return;
    }
    let close_xz = (target - pos).xz().length_squared() < (6.0f32).powi(2);
    let same_level = (target.y - pos.y).abs() < 1.0;
    if close_xz && same_level {
        // Same room & level: steer straight at the target, no grid detour.
        brain.path.clear();
        brain.goal_cell = None;
        brain.steer = Vec3::new(target.x, pos.y, target.z);
        return;
    }
    let (Some(cur), goal_c) = (nav.nearest_cell(pos), nav.cell_of(target)) else {
        return;
    };
    let goal = if nav.cells.contains_key(&goal_c) {
        goal_c
    } else if let Some(n) = nav.nearest_cell(target) {
        n
    } else {
        return;
    };
    let stale = brain.goal_cell != Some(goal) || brain.path.is_empty();
    if stale && brain.repath <= 0.0 {
        if let Some(path) = nav.find_path(cur, goal) {
            brain.path = path;
            brain.goal_cell = Some(goal);
        }
        brain.repath = REPATH_INTERVAL;
    }
    advance_waypoints(nav, pos, brain);
}

/// Pop reached waypoints and aim `steer` at the next one (cell centre at floor Y).
fn advance_waypoints(nav: &EnemyNav, pos: Vec3, brain: &mut EnemyBrain) {
    if nav.is_empty() || brain.path.is_empty() {
        // Cover dives set `goal_cell` without a path — keep that steer point.
        return;
    }
    while let Some(&next) = brain.path.first() {
        let w = nav.world_of(next);
        if (w - pos).xz().length() < WAYPOINT_REACHED {
            brain.path.remove(0);
        } else {
            brain.steer = w;
            return;
        }
    }
    // Path exhausted: hold position (goal cell centre).
    if let Some(goal) = brain.goal_cell.take() {
        brain.steer = nav.world_of(goal);
    }
}

/// Move agents toward their steer point with wall checks, gravity and ground
/// snapping — they walk ON floors and ramps instead of gliding through them.
fn enemy_locomotion(
    time: Res<Time>,
    spatial: SpatialQuery,
    nav: Res<EnemyNav>,
    liquids: Query<Entity, With<crate::liquids::Liquid>>,
    players: Query<Entity, With<Player>>,
    mut enemies: Query<
        (Entity, &mut Transform, &mut EnemyBrain, &mut NetTransform),
        Or<(With<Enemy>, With<Npc>)>,
    >,
) {
    let dt = time.delta_secs();
    let mut excluded_base: Vec<Entity> = liquids.iter().collect();
    excluded_base.extend(players.iter());

    for (entity, mut transform, mut brain, mut net) in &mut enemies {
        let mut excluded = excluded_base.clone();
        excluded.push(entity);
        let filter = SpatialQueryFilter::from_excluded_entities(excluded);

        let speed = match brain.mode {
            AiMode::Combat | AiMode::Cover | AiMode::Search => 4.2,
            AiMode::Suspicious => 3.0,
            AiMode::Patrol => 2.2,
        };
        let to_steer = (brain.steer - transform.translation).xz();
        let dir2 = if to_steer.length_squared() > 0.04 {
            to_steer.normalize()
        } else {
            Vec2::ZERO
        };
        let dir = Vec3::new(dir2.x, 0.0, dir2.y);
        let step_len = speed * dt;

        // Horizontal move with a chest-height blocked probe; if the direct move
        // is blocked (prop/wall), try sliding along each axis.
        let chest = transform.translation + Vec3::Y * (brain.half_h * 0.4);
        let clear = |d: Vec3| -> bool {
            if d.length_squared() < 1e-6 {
                return false;
            }
            let Ok(ray_dir) = Dir3::new(d.normalize()) else {
                return false;
            };
            spatial
                .cast_ray(
                    chest.adjust_precision(),
                    ray_dir,
                    (step_len + 0.35) as Scalar,
                    true,
                    &filter,
                )
                .is_none()
        };
        let mut moved = Vec3::ZERO;
        if dir != Vec3::ZERO {
            if clear(dir) {
                moved = dir * step_len;
            } else if clear(Vec3::new(dir.x, 0.0, 0.0)) {
                moved = Vec3::new(dir.x, 0.0, 0.0) * step_len;
            } else if clear(Vec3::new(0.0, 0.0, dir.z)) {
                moved = Vec3::new(0.0, 0.0, dir.z) * step_len;
            }
        }
        transform.translation += moved;

        // Gravity + ground snap: probe down from the centre for the floor top.
        brain.vy += AGENT_GRAVITY * dt;
        let fall = (-brain.vy * dt).max(0.0);
        let probe_len = brain.half_h + fall + GROUND_SNAP;
        let hit = spatial.cast_ray(
            transform.translation.adjust_precision(),
            Dir3::NEG_Y,
            probe_len as Scalar,
            true,
            &filter,
        );
        if let Some(hit) = hit {
            let ground_y = transform.translation.y - hit.distance as f32;
            transform.translation.y = ground_y + brain.half_h;
            brain.vy = 0.0;
        } else {
            transform.translation.y += brain.vy * dt;
        }

        // The baked nav floor is authoritative on the grid: an agent must never
        // sink below its cell's floor surface. (Elevated deck blocks are hollow
        // trimeshes — a ray cast from INSIDE finds the substrate underneath and
        // would leave the agent walking embedded in the block; agents above the
        // grid, e.g. hub floors below y=0, keep normal gravity.)
        if !nav.is_empty() && transform.translation.y > -0.5 {
            if let Some(cell) = nav.cells.get(&nav.cell_of(transform.translation)) {
                let min_y = cell.y + brain.half_h;
                if transform.translation.y < min_y - 0.02 {
                    transform.translation.y = min_y;
                    brain.vy = 0.0;
                }
            }
        }

        // Facing: engaged enemies track their target; others face the motion
        // (fallback: face the steer point while blocked).
        let face = if matches!(brain.mode, AiMode::Combat | AiMode::Cover) {
            brain
                .last_known
                .map(|t| t - transform.translation)
                .filter(|v| v.xz().length_squared() > 1e-6)
                .unwrap_or(if moved.length_squared() > 1e-6 { moved } else { dir })
        } else if moved.length_squared() > 1e-6 {
            moved
        } else {
            dir
        };
        if face.xz().length_squared() > 1e-6 {
            let target_yaw = face.x.atan2(face.z);
            let target = Quat::from_rotation_y(target_yaw);
            transform.rotation = transform.rotation.slerp(target, (10.0 * dt).min(1.0));
        }

        net.translation = transform.translation;
        net.rotation = transform.rotation;
    }
}

/// Spawn one server-simulated tracer round.
fn spawn_projectile(
    commands: &mut Commands,
    pos: Vec3,
    dir: Vec3,
    speed: f32,
    damage: f32,
    friendly: bool,
    shooter: Entity,
) {
    let rotation = Quat::from_rotation_arc(Vec3::Z, dir.normalize_or_zero());
    commands.spawn((
        LevelEntity,
        Replicated,
        Projectile { friendly },
        ProjectileSim {
            vel: dir.normalize_or_zero() * speed,
            ttl: PROJECTILE_TTL,
            damage,
            shooter,
            origin: pos,
        },
        Transform::from_translation(pos).with_rotation(rotation),
        NetTransform {
            translation: pos,
            rotation,
        },
    ));
}

/// Enemies in Combat with line of sight fire tracers at their target.
fn enemy_fire(
    mut commands: Commands,
    time: Res<Time>,
    mut noises: ResMut<PendingNoises>,
    mut enemies: Query<(Entity, &Transform, &mut EnemyBrain), With<Enemy>>,
) {
    let dt = time.delta_secs();
    for (entity, transform, mut brain) in &mut enemies {
        brain.fire_cd = (brain.fire_cd - dt).max(0.0);
        if brain.mode != AiMode::Combat || !brain.sees_target || brain.fire_cd > 0.0 {
            continue;
        }
        let Some(target) = brain.last_known else { continue };
        let muzzle = transform.translation + Vec3::Y * (brain.half_h * 0.6);
        let aim_point = target + Vec3::Y * 0.3;
        let to = aim_point - muzzle;
        if to.length() > ENEMY_FIRE_RANGE {
            continue;
        }
        // Deterministic-ish spread from position + time.
        let seed = (transform.translation.x * 91.0
            + transform.translation.z * 53.0
            + time.elapsed_secs() * 977.0) as u32;
        let jx = (pseudo_rand(seed) - 0.5) * 2.0 * ENEMY_AIM_SPREAD;
        let jy = (pseudo_rand(seed.wrapping_add(7)) - 0.5) * 2.0 * ENEMY_AIM_SPREAD;
        let dir = (to.normalize_or_zero() + Vec3::new(jx, jy, 0.0)).normalize_or_zero();
        spawn_projectile(
            &mut commands,
            muzzle,
            dir,
            ENEMY_PROJ_SPEED,
            ENEMY_PROJ_DAMAGE,
            false,
            entity,
        );
        brain.fire_cd = ENEMY_FIRE_COOLDOWN
            + pseudo_rand(seed.wrapping_add(31)) * ENEMY_FIRE_COOLDOWN * 0.5;
        // Gunfire is loud: allies converge even through walls.
        noises.0.push((transform.translation, GUNSHOT_NOISE));
    }
}

/// Fly projectiles, sweep the path with a ray, and apply hits.
#[allow(clippy::too_many_arguments)]
fn projectile_sim(
    mut commands: Commands,
    time: Res<Time>,
    spatial: SpatialQuery,
    mut noises: ResMut<PendingNoises>,
    liquids: Query<Entity, With<crate::liquids::Liquid>>,
    colliders: Query<&ColliderOf>,
    mut projectiles: Query<(
        Entity,
        &mut Transform,
        &mut NetTransform,
        &mut ProjectileSim,
        &Projectile,
    )>,
    mut enemies: Query<(&Transform, &mut Health, &mut EnemyBrain), (With<Enemy>, Without<ProjectileSim>, Without<Player>)>,
    enemy_ids: Query<Entity, With<Enemy>>,
    npc_ids: Query<Entity, With<Npc>>,
    player_ids: Query<Entity, With<Player>>,
    mut players: Query<
        (&mut Health, &mut PlayerAlive, &PlayerName),
        (With<Player>, Without<Enemy>),
    >,
) {
    let dt = time.delta_secs();
    for (entity, mut transform, mut net, mut sim, kind) in &mut projectiles {
        sim.ttl -= dt;
        if sim.ttl <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let start = transform.translation;
        let step = sim.vel * dt;
        let Ok(dir) = Dir3::new(sim.vel.normalize_or_zero()) else {
            commands.entity(entity).despawn();
            continue;
        };
        // Never hit the shooter, liquid sensors, or same-team bodies.
        let mut excluded: Vec<Entity> = liquids.iter().collect();
        excluded.push(sim.shooter);
        if kind.friendly {
            excluded.extend(player_ids.iter());
        } else {
            excluded.extend(enemy_ids.iter());
            excluded.extend(npc_ids.iter());
        }
        let filter = SpatialQueryFilter::from_excluded_entities(excluded);
        let hit = spatial.cast_ray(
            start.adjust_precision(),
            dir,
            (step.length() + 0.02) as Scalar,
            true,
            &filter,
        );
        let Some(hit) = hit else {
            transform.translation = start + step;
            net.translation = transform.translation;
            continue;
        };

        let impact = start + *dir * hit.distance as f32;
        let body = colliders.get(hit.entity).map(|c| c.body).unwrap_or(hit.entity);
        if kind.friendly {
            if let Ok((_t, mut health, _b)) = enemies.get_mut(body) {
                health.current -= sim.damage;
                if health.current <= 0.0 {
                    commands.entity(body).despawn();
                }
            }
            // Enemies near a player bullet impact consider diving to cover,
            // and the crack draws attention.
            let origin = sim.origin;
            for (etransform, _h, mut brain) in &mut enemies {
                if etransform.translation.distance(impact) < NEAR_MISS_RADIUS {
                    brain.threat_from = Some(origin);
                }
            }
            noises.0.push((impact, 12.0));
        } else if let Ok((mut health, mut alive, name)) = players.get_mut(body) {
            health.current -= sim.damage;
            if health.current <= 0.0 && alive.0 {
                alive.0 = false;
                info!("player {} died (shot)", name.0);
            }
        }
        commands.entity(entity).despawn();
    }
}

fn player_attacks(
    mut commands: Commands,
    spatial: SpatialQuery,
    mut noises: ResMut<PendingNoises>,
    colliders: Query<&ColliderOf>,
    mut enemies: Query<(Entity, &mut Health, &Transform)>,
    mut players: Query<
        (Entity, &Transform, &mut LatestInput, &Inventory, &PlayerAlive, &PlayerName),
        With<Player>,
    >,
) {
    for (player, transform, mut input, inventory, alive, _) in &mut players {
        // Consume the press even when dead, so a latched attack can't fire on respawn.
        if !input.0.attack {
            continue;
        }
        input.0.attack = false;
        if !alive.0 {
            continue;
        }
        let eye = transform.translation + Vec3::Y * config::PLAYER_EYE_HEIGHT;
        let look = Quat::from_euler(EulerRot::YXZ, input.0.yaw, input.0.pitch, 0.0);
        let forward = look * -Vec3::Z;
        // The gun sits in the right hand — right of and below the eye. In
        // first person the tracer reads as a handheld shot instead of leaving
        // the camera lens; in third person it starts at the model's gun side.
        let muzzle = eye + look * PLAYER_MUZZLE_OFFSET;
        // Fire toward the client's crosshair aim point (camera ray), not down
        // the raw eye ray: in third person the camera sits 0.55 m right of the
        // eye, and eye-ray shots landed left of the crosshair at close range.
        let dir_v = aim_direction(input.0.aim, muzzle, forward);

        // The weapon in the SELECTED hotbar slot decides the attack — a gun
        // elsewhere in the inventory must not fire while the bat is out.
        let held = inventory
            .0
            .get(input.0.selected_slot as usize)
            .and_then(|slot| slot.as_ref());
        if held.is_some_and(items::is_gun) {
            spawn_projectile(
                &mut commands,
                muzzle,
                dir_v,
                PLAYER_PROJ_SPEED,
                PLAYER_PROJ_DAMAGE,
                true,
                player,
            );
            noises.0.push((transform.translation, GUNSHOT_NOISE));
            continue;
        }

        if !held.is_some_and(items::is_bat) {
            continue;
        }
        let dir = Dir3::new(dir_v).unwrap_or(Dir3::NEG_Z);
        let Some(hit) = spatial.cast_ray(
            eye.adjust_precision(),
            dir,
            2.5,
            true,
            &SpatialQueryFilter::from_excluded_entities([player]),
        ) else {
            continue;
        };
        let body = colliders
            .get(hit.entity)
            .map(|c| c.body)
            .unwrap_or(hit.entity);
        let Ok((enemy, mut health, _)) = enemies.get_mut(body) else {
            continue;
        };
        health.current -= 25.0;
        if health.current <= 0.0 {
            commands.entity(enemy).despawn();
        }
    }
}

/// Close-range fallback damage: an engaged enemy standing on top of a player
/// claws/strikes (ranged fire handles everything beyond arm's length).
fn enemy_damage_players(
    time: Res<Time>,
    run: Query<&shared::run::RunState, With<RunEntity>>,
    mut players: Query<
        (Entity, &Transform, &mut Health, &mut PlayerAlive, &PlayerName),
        With<Player>,
    >,
    enemies: Query<(&Transform, &EnemyBrain), With<Enemy>>,
) {
    if run
        .single()
        .is_ok_and(|r| r.phase == shared::run::RunPhase::RunOver)
    {
        return;
    }
    let dt = time.delta_secs();
    for (_player, transform, mut health, mut alive, name) in &mut players {
        if !alive.0 {
            continue;
        }
        for (etransform, brain) in &enemies {
            if brain.mode != AiMode::Combat {
                continue;
            }
            let dist = transform.translation.distance(etransform.translation);
            if dist < 1.2 {
                health.current -= 30.0 * dt;
            }
        }
        if health.current <= 0.0 && alive.0 {
            alive.0 = false;
            info!("player {} died", name.0);
        }
    }
}

/// Mirror each enemy's brain mode + countdown fraction into the replicated
/// `EnemyAiMode` marker. The fraction drives the marker's clock hand and is
/// quantized to 64 steps to keep replication churn low. Combat shows the
/// countdown only while contact is lost (grace before dropping to Search).
fn sync_ai_mode(mut enemies: Query<(&EnemyBrain, &mut EnemyAiMode), With<Enemy>>) {
    for (brain, mut mode) in &mut enemies {
        let timed = match brain.mode {
            AiMode::Combat => !brain.sees_target,
            AiMode::Search | AiMode::Suspicious | AiMode::Cover => true,
            AiMode::Patrol => false,
        };
        let frac = if timed && brain.mode_total > 0.01 {
            ((brain.mode_timer / brain.mode_total).clamp(0.0, 1.0) * 63.0).round() as u8
        } else {
            0
        };
        mode.set_if_neq(EnemyAiMode {
            mode: brain.mode.as_u8(),
            frac,
        });
    }
}

/// Mirror server Health into the replicated `PlayerHealth` for HUD display.
fn sync_player_health(mut players: Query<(&Health, &mut PlayerHealth), With<Player>>) {
    for (health, mut replicated) in &mut players {
        replicated.set_if_neq(PlayerHealth {
            current: health.current.max(0.0),
            max: health.max,
        });
    }
}

/// Direction from the muzzle to the client's crosshair aim point, falling
/// back to the raw look ray when the point is absent or degenerate (default
/// input, point-blank wall = aim at/behind the muzzle) or implausibly far off
/// the look ray (stale or hostile input).
fn aim_direction(aim: Vec3, muzzle: Vec3, forward: Vec3) -> Vec3 {
    if !aim.is_finite() || aim == Vec3::ZERO {
        return forward;
    }
    let to = aim - muzzle;
    if to.length_squared() < 0.05 {
        return forward;
    }
    let dir = to.normalize();
    // Camera ray and look ray agree to well under 60° at any range that
    // matters (they converge past ~1 m); reject anything wider.
    if dir.dot(forward) < 0.5 {
        return forward;
    }
    dir
}

fn pseudo_rand(seed: u32) -> f32 {
    let mut x = seed;
    x ^= x.wrapping_mul(0x85eb_ca6b);
    x ^= x >> 13;
    x ^= x.wrapping_mul(0xc2b2_ae35);
    x ^= x >> 16;
    (x & 0x00ff_ffff) as f32 / 16_777_216.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hearing_tiers_ordered() {
        let sneak = hearing_radius(true, false, true);
        let walk = hearing_radius(true, false, false);
        let sprint = hearing_radius(true, true, false);
        assert!(sneak < walk && walk < sprint, "{sneak} {walk} {sprint}");
        assert_eq!(hearing_radius(false, true, false), 0.0, "still = silent");
    }

    #[test]
    fn aim_direction_converges_and_falls_back() {
        let muzzle = Vec3::new(0.22, 1.35, -0.45);
        let forward = -Vec3::Z;
        // Sane aim point ahead: shot converges on it (not on the raw ray).
        let aim = Vec3::new(1.0, 1.6, -8.0);
        let dir = aim_direction(aim, muzzle, forward);
        assert!((dir - (aim - muzzle).normalize()).length() < 1e-5);
        // Default/degenerate/behind inputs all fall back to the look ray.
        assert_eq!(aim_direction(Vec3::ZERO, muzzle, forward), forward);
        assert_eq!(aim_direction(Vec3::NAN, muzzle, forward), forward);
        assert_eq!(aim_direction(muzzle + Vec3::Z * 5.0, muzzle, forward), forward);
        assert_eq!(aim_direction(muzzle + forward * 0.1, muzzle, forward), forward);
    }

    #[test]
    fn vision_cone_only_forward() {
        let fwd = Vec3::Z;
        assert!(in_vision_cone(fwd, Vec3::new(0.0, 0.0, 10.0), 22.0));
        // 45° off-axis is inside a 110° cone.
        assert!(in_vision_cone(fwd, Vec3::new(7.0, 0.0, 7.0), 22.0));
        // Directly behind is never visible.
        assert!(!in_vision_cone(fwd, Vec3::new(0.0, 0.0, -5.0), 22.0));
        // 90° to the side is outside the cone.
        assert!(!in_vision_cone(fwd, Vec3::new(10.0, 0.0, 0.0), 22.0));
        // Beyond range.
        assert!(!in_vision_cone(fwd, Vec3::new(0.0, 0.0, 30.0), 22.0));
    }

    #[test]
    fn ai_mode_u8_roundtrip_values() {
        assert_eq!(AiMode::Patrol.as_u8(), 0);
        assert_eq!(AiMode::Suspicious.as_u8(), 1);
        assert_eq!(AiMode::Combat.as_u8(), 2);
        assert_eq!(AiMode::Search.as_u8(), 3);
        assert_eq!(AiMode::Cover.as_u8(), 4);
    }
}
