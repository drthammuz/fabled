//! All tunable gameplay/engine values live here. No magic numbers in systems.

/// Fixed gameplay/physics tick rate of the server (Hz).
pub const SERVER_TICK_HZ: f64 = 30.0;

/// Live village pacing: real seconds per sim minute (default; +/- adjusts
/// One real minute is one game hour: a village day lasts 24 real minutes.
pub const SECS_PER_SIM_MINUTE: f64 = 1.0;

/// Villager commute speed in m/s, world space. Deliberately close to the
/// player's walk speed so NPCs read as people, not snails; visual walking
/// is decoupled from the sim's abstract travel times.
pub const VILLAGER_WALK_SPEED: f32 = 7.0;
/// Slow ambling speed for idle wandering around a venue, m/s.
pub const VILLAGER_AMBLE_SPEED: f32 = 2.2;

/// How often the headless server app loop wakes up (Hz). Must be >= tick
/// rate; the `FixedUpdate` accumulator consumes time at `SERVER_TICK_HZ`.
pub const SERVER_LOOP_HZ: f64 = 120.0;

/// Default UDP port for the game server.
pub const DEFAULT_PORT: u16 = 5000;

/// Netcode protocol id; both sides must match to connect.
pub const PROTOCOL_ID: u64 = 7;

/// Maximum simultaneous clients.
pub const MAX_CLIENTS: usize = 8;

// --- Player movement (server-side) ---

/// Walk speed in m/s (+20% over the original 7.0).
pub const PLAYER_MOVE_SPEED: f32 = 8.4;
/// Sprint speed in m/s (+50% over walk).
pub const PLAYER_SPRINT_SPEED: f32 = 12.6;
/// Upward velocity on jump, m/s.
pub const PLAYER_JUMP_IMPULSE: f32 = 9.0;
/// Gravity acceleration, m/s² (negative = down). Stronger than earth for snappy falls.
pub const PLAYER_GRAVITY: f32 = -24.0;
/// Max downward speed from gravity, m/s (positive magnitude).
pub const PLAYER_TERMINAL_VELOCITY: f32 = 35.0;
// Quake/Source-style ground movement: friction is always applied while
// grounded, then acceleration rebuilds speed toward the wish direction.
// This is what makes direction changes feel grippy instead of slidy.
/// Ground friction coefficient (1/s). Higher = grippier stops and turns.
pub const PLAYER_FRICTION: f32 = 9.0;
/// Ground acceleration as a multiple of wish speed (1/s).
/// 10 means full speed is reached in ~0.1 s.
pub const PLAYER_ACCEL_RATE: f32 = 10.0;
/// Air acceleration multiple — low: some steering, momentum preserved.
pub const PLAYER_AIR_ACCEL_RATE: f32 = 1.8;
/// Scale on the impulse players impart when walking into dynamic bodies
/// (applied on top of the physically-correct reduced-mass impulse).
pub const PLAYER_PUSH_STRENGTH: f32 = 0.5;
/// Character body width (full X/Z extent). Quake-style controllers use an AABB
/// hull; a box avoids capsule “rolling” off stair treads and trap-door lips.
pub const PLAYER_BODY_WIDTH: f32 = 0.6;
/// Standing body height (full Y extent).
pub const PLAYER_BODY_HEIGHT: f32 = 1.8;
/// Legacy capsule radius — kept for camera / spawn height math on the client.
pub const PLAYER_CAPSULE_RADIUS: f32 = 0.4;
/// Legacy capsule cylinder length — kept for camera / spawn height math.
pub const PLAYER_CAPSULE_LENGTH: f32 = 1.0;

/// `--city` player spawn (capsule centre). Colliders load async — may take a moment.
pub const CITY_SPAWN: bevy::prelude::Vec3 = bevy::prelude::Vec3::new(0.0, 18.0, 40.0);
/// Player mass in kg (used when pushing dynamic props).
pub const PLAYER_MASS: f32 = 75.0;
/// Camera eye height above the capsule center.
pub const PLAYER_EYE_HEIGHT: f32 = 0.6;
/// Crouched capsule cylinder length (total height = length + 2r).
pub const PLAYER_CROUCH_LENGTH: f32 = 0.45;
/// Eye height while crouched.
pub const PLAYER_CROUCH_EYE_HEIGHT: f32 = 0.25;
/// Walk speed while crouched, m/s.
pub const PLAYER_CROUCH_SPEED: f32 = 4.5;
/// Upward cast distance required before standing up from crouch.
pub const PLAYER_STAND_UP_CLEARANCE: f32 = 0.55;
/// Max height of a step/ledge the player climbs automatically (no jump).
/// Kenney `stairs` rise ~0.29 m per tread; 0.65 m matches Quake STEPSIZE headroom.
pub const PLAYER_STEP_HEIGHT: f32 = 0.65;
/// Short downward trace for grounded tests (Quake `PM_GroundTrace` ≈ 0.25 units).
/// Must stay small — a large value causes “suck to ground” while falling and pulls
/// players off narrow ledges.
pub const PLAYER_GROUND_TRACE_DIST: f32 = 0.08;
/// Relaxed walkable normal for stair tread snap during step-up only (not global ground).
pub const PLAYER_STAIR_WALK_NORMAL: f32 = 0.55;
/// Minimum surface normal Y for walkable ground (Quake `MIN_WALK_NORMAL` = 0.7).
pub const PLAYER_MIN_WALK_NORMAL: f32 = 0.7;
/// When moving up, ignore ground if velocity pushes away from the surface this fast
/// (Quake kickoff check, scaled to m/s).
pub const PLAYER_GROUND_KICKOFF_SPEED: f32 = 2.5;
/// Upward speed above which ground traces are skipped entirely (Quake: 180 ups).
pub const PLAYER_JUMP_GROUND_CUTOFF: f32 = 4.0;
/// Grace period after leaving the ground where a jump still counts as grounded.
pub const PLAYER_COYOTE_TIME: f32 = 0.1;
/// Wading: only count as grounded when feet are this close to the bed (not swimming).
pub const PLAYER_WADE_GROUND_PROBE: f32 = 0.12;
/// Max look pitch the server accepts, radians (just under straight up/down).
pub const PLAYER_MAX_PITCH: f32 = 1.55;

// --- Client-side presentation ---

/// How far in the past remote entities are rendered (seconds). Two server
/// ticks of delay gives the interpolator something to interpolate between.
pub const INTERP_DELAY: f64 = 0.1;
/// Mouse look sensitivity for the first-person camera.
pub const LOOK_SENSITIVITY: f32 = 0.002;

// --- Grab / throw (server-side, M4) ---

/// Max raycast distance to acquire a grab target.
pub const GRAB_RANGE: f32 = 4.0;
/// How far in front of the player the held object is pulled toward.
pub const GRAB_HOLD_DISTANCE: f32 = 1.6;
/// Spring strength pulling grabbed objects toward the hold point (1/s²).
pub const GRAB_SPRING: f32 = 38.0;
/// Velocity damping while grabbed (1/s).
pub const GRAB_DAMPING: f32 = 10.0;
/// Max force one player's grab can exert (N). One player cannot lift the
/// heavy boulder against gravity; two players together can.
pub const GRAB_MAX_FORCE: f32 = 3500.0;
/// Angular damping on dynamic props (reduces endless spinning).
pub const PROP_ANGULAR_DAMPING: f32 = 2.5;
/// Bounciness of dynamic props (0 = none, 1 = perfect bounce).
pub const PROP_RESTITUTION: f32 = 0.28;
/// Throw speed for objects up to THROW_REF_MASS, m/s.
pub const THROW_IMPULSE: f32 = 14.0;
/// Objects heavier than this get proportionally slower throws.
pub const THROW_REF_MASS: f32 = 40.0;
// --- Items / inventory (M5) ---

/// Number of inventory slots per player.
pub const INVENTORY_SLOTS: usize = 4;
/// Max distance for the interact (pickup) raycast.
pub const INTERACT_RANGE: f32 = 3.0;
/// Forward speed given to dropped items, m/s.
pub const ITEM_DROP_SPEED: f32 = 3.0;
/// World-item cube size (full extent), meters.
pub const ITEM_SIZE: f32 = 0.35;

/// Y-world offset for hub rooms. They generate this many metres below the
/// stretch floor so players physically fall from the extraction pit into the
/// hub without an abrupt teleport.
pub const HUB_Y_OFFSET: f32 = -22.0;

// --- Fly camera (debug/inspection, client-side only) ---

/// Base fly speed in m/s.
pub const FLY_CAM_SPEED: f32 = 12.0;
/// Speed multiplier while holding the fast key (Shift).
pub const FLY_CAM_FAST_MULT: f32 = 4.0;
/// Mouse look sensitivity in radians per pixel of mouse motion.
pub const FLY_CAM_SENSITIVITY: f32 = 0.002;

// --- Client atmosphere (M1) ---

/// Toxic steam above water channels.
pub const FOG_CHANNEL_DENSITY: f32 = 0.18;
/// General tunnel haze along walkways.
pub const FOG_TUNNEL_DENSITY: f32 = 0.06;
/// Hanabi spawn rate for water-channel boil bubbles (per second).
pub const WATER_BOIL_RATE: f32 = 20.0;
/// Hanabi spawn rate for sparse tunnel air motes (per second, per zone).
pub const AIR_MOTE_RATE: f32 = 5.0;
/// Max wall panel length before splitting (client visual only).
pub const WALL_PANEL_MAX_M: f32 = 3.5;

// --- Water (M5–M6) ---

/// World-space Y of the animated water surface.
pub const WATER_SURFACE_HEIGHT: f32 = 0.02;
/// Gerstner-style wave height for sewer channels, metres.
pub const WATER_WAVE_AMPLITUDE: f32 = 0.035;
/// Toxic green base tint (matches flat-water fallback).
pub const WATER_BASE_COLOR: (f32, f32, f32, f32) = (0.05, 0.45, 0.18, 0.92);
/// Shallow channel tint.
pub const WATER_SHALLOW_COLOR: (f32, f32, f32, f32) = (0.08, 0.55, 0.22, 1.0);
/// Deep channel tint.
pub const WATER_DEEP_COLOR: (f32, f32, f32, f32) = (0.02, 0.25, 0.10, 1.0);
/// Liquid density for buoyancy (kg/m³).
pub const WATER_DENSITY: f32 = 1000.0;
/// Walk speed multiplier while wading. Sewer channels are narrower than the
/// player capsule, so you almost always straddle a channel edge — a heavy
/// penalty here read as the whole tunnel being "sticky". Keep it subtle: a
/// mild slow that barely registers when brushing an edge but adds a little
/// weight in open water. (The old compounding per-tick water drag is gone.)
pub const PLAYER_WADE_SPEED_MULT: f32 = 0.85;
/// Minimum entry speed to emit a splash event, m/s.
pub const WATER_SPLASH_MIN_SPEED: f32 = 0.8;
/// Footfall ripple interval while wading on ground, seconds.
pub const WATER_FOOTFALL_INTERVAL: f32 = 0.45;
