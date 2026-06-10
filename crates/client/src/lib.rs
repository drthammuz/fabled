use bevy::prelude::*;

/// Address of the server this client should connect to (set from CLI).
/// Unused until netcode lands in M3; host mode connects to localhost.
#[derive(Resource, Debug, Clone)]
pub struct ServerAddress(pub String);

/// Core client-side plugin: rendering, input, UI. Never gameplay logic.
pub struct ClientCorePlugin;

impl Plugin for ClientCorePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, log_startup);
    }
}

fn log_startup(address: Option<Res<ServerAddress>>) {
    match address {
        Some(addr) => info!("client core running, server address: {}", addr.0),
        None => info!("client core running (local host mode)"),
    }
}
