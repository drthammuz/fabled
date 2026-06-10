//! All tunable gameplay/engine values live here. No magic numbers in systems.

/// Fixed gameplay/physics tick rate of the server (Hz).
pub const SERVER_TICK_HZ: f64 = 30.0;

/// How often the headless server app loop wakes up (Hz). Must be >= tick
/// rate; the `FixedUpdate` accumulator consumes time at `SERVER_TICK_HZ`.
pub const SERVER_LOOP_HZ: f64 = 120.0;

/// Default UDP port for the game server.
pub const DEFAULT_PORT: u16 = 5000;
