//! All tunable gameplay/engine values live here. No magic numbers in systems.

/// Fixed gameplay/physics tick rate of the server (Hz).
pub const SERVER_TICK_HZ: f64 = 30.0;

/// Live village pacing: real seconds per sim minute. 2.0 means a village
/// day lasts 48 real minutes.
pub const SECS_PER_SIM_MINUTE: f64 = 2.0;

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
/// Capsule dimensions: radius and cylinder length (total height = length + 2r).
pub const PLAYER_CAPSULE_RADIUS: f32 = 0.4;
pub const PLAYER_CAPSULE_LENGTH: f32 = 1.0;
/// Player mass in kg (used when pushing dynamic props).
pub const PLAYER_MASS: f32 = 75.0;
/// Camera eye height above the capsule center.
pub const PLAYER_EYE_HEIGHT: f32 = 0.6;
/// Max distance below the capsule bottom that still counts as grounded.
pub const PLAYER_GROUND_PROBE: f32 = 0.15;
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

// --- Fly camera (debug/inspection, client-side only) ---

/// Base fly speed in m/s.
pub const FLY_CAM_SPEED: f32 = 12.0;
/// Speed multiplier while holding the fast key (Shift).
pub const FLY_CAM_FAST_MULT: f32 = 4.0;
/// Mouse look sensitivity in radians per pixel of mouse motion.
pub const FLY_CAM_SENSITIVITY: f32 = 0.002;
