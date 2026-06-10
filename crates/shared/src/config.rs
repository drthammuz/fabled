//! All tunable gameplay/engine values live here. No magic numbers in systems.

/// Fixed gameplay/physics tick rate of the server (Hz).
pub const SERVER_TICK_HZ: f64 = 30.0;

/// How often the headless server app loop wakes up (Hz). Must be >= tick
/// rate; the `FixedUpdate` accumulator consumes time at `SERVER_TICK_HZ`.
pub const SERVER_LOOP_HZ: f64 = 120.0;

/// Default UDP port for the game server.
pub const DEFAULT_PORT: u16 = 5000;

// --- Fly camera (debug/inspection, client-side only) ---

/// Base fly speed in m/s.
pub const FLY_CAM_SPEED: f32 = 12.0;
/// Speed multiplier while holding the fast key (Shift).
pub const FLY_CAM_FAST_MULT: f32 = 4.0;
/// Mouse look sensitivity in radians per pixel of mouse motion.
pub const FLY_CAM_SENSITIVITY: f32 = 0.002;
