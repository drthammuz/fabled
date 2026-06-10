//! All tunable gameplay/engine values live here. No magic numbers in systems.

/// Fixed gameplay/physics tick rate of the server (Hz).
pub const SERVER_TICK_HZ: f64 = 30.0;

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

/// Horizontal move speed in m/s.
pub const PLAYER_MOVE_SPEED: f32 = 7.0;
/// Upward velocity on jump, m/s.
pub const PLAYER_JUMP_IMPULSE: f32 = 9.0;
/// Gravity acceleration, m/s² (negative = down). Stronger than earth for snappy falls.
pub const PLAYER_GRAVITY: f32 = -24.0;
/// Max downward speed from gravity, m/s (positive magnitude).
pub const PLAYER_TERMINAL_VELOCITY: f32 = 35.0;
/// Horizontal acceleration toward target speed, m/s².
pub const PLAYER_ACCELERATION: f32 = 55.0;
/// Exponential damping on horizontal velocity (higher = stops faster).
pub const PLAYER_MOVE_DAMPING: f32 = 14.0;
/// Fraction of ground acceleration applied while airborne.
pub const PLAYER_AIR_CONTROL: f32 = 0.2;
/// Capsule dimensions: radius and cylinder length (total height = length + 2r).
pub const PLAYER_CAPSULE_RADIUS: f32 = 0.4;
pub const PLAYER_CAPSULE_LENGTH: f32 = 1.0;
/// Player mass in kg (used when pushing dynamic props).
pub const PLAYER_MASS: f32 = 75.0;
/// Camera eye height above the capsule center.
pub const PLAYER_EYE_HEIGHT: f32 = 0.6;
/// Max distance below the capsule bottom that still counts as grounded.
pub const PLAYER_GROUND_PROBE: f32 = 0.15;

// --- Client-side presentation ---

/// How far in the past remote entities are rendered (seconds). Two server
/// ticks of delay gives the interpolator something to interpolate between.
pub const INTERP_DELAY: f64 = 0.1;
/// Mouse look sensitivity for the first-person camera.
pub const LOOK_SENSITIVITY: f32 = 0.002;

// --- Fly camera (debug/inspection, client-side only) ---

/// Base fly speed in m/s.
pub const FLY_CAM_SPEED: f32 = 12.0;
/// Speed multiplier while holding the fast key (Shift).
pub const FLY_CAM_FAST_MULT: f32 = 4.0;
/// Mouse look sensitivity in radians per pixel of mouse motion.
pub const FLY_CAM_SENSITIVITY: f32 = 0.002;
