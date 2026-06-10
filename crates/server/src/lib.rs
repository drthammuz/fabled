use bevy::prelude::*;
use shared::config;

/// Core server-side gameplay plugin. Added in both `--server` (headless)
/// and `--host` (listen server) modes, so gameplay always runs on the same
/// fixed-tick schedule regardless of how the app is presented.
pub struct ServerCorePlugin;

impl Plugin for ServerCorePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(config::SERVER_TICK_HZ))
            .init_resource::<ServerTick>()
            .add_systems(Startup, log_startup)
            .add_systems(FixedUpdate, advance_tick);
    }
}

/// Monotonic gameplay tick counter, advanced once per `FixedUpdate`.
#[derive(Resource, Default)]
pub struct ServerTick(pub u64);

fn log_startup() {
    info!(
        "server core running at fixed {} Hz tick",
        config::SERVER_TICK_HZ
    );
}

fn advance_tick(mut tick: ResMut<ServerTick>) {
    tick.0 += 1;
    // One log line per second is enough to prove the schedule is alive.
    if tick.0 % config::SERVER_TICK_HZ as u64 == 0 {
        info!("tick {}", tick.0);
    }
}
