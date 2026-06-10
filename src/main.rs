use std::time::Duration;

use bevy::app::ScheduleRunnerPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::window::PresentMode;
use clap::Parser;

use client::{ClientCorePlugin, ServerAddress};
use server::ServerCorePlugin;
use shared::config;

#[derive(Parser, Debug)]
#[command(
    name = "fabled",
    about = "Server-authoritative co-op physics game prototype"
)]
struct Cli {
    /// Run a headless dedicated server (no rendering)
    #[arg(long, conflicts_with_all = ["client", "host"])]
    server: bool,

    /// Connect as a client to a server at the given address (ip[:port])
    #[arg(long, value_name = "IP", conflicts_with = "host")]
    client: Option<String>,

    /// Run a listen server: server + local client in one process
    #[arg(long)]
    host: bool,
}

fn main() {
    let cli = Cli::parse();

    if cli.server {
        run_server();
    } else if let Some(address) = cli.client {
        run_client(address);
    } else if cli.host {
        run_host();
    } else {
        eprintln!("error: specify one of --server, --client <ip>, or --host");
        std::process::exit(2);
    }
}

/// Headless dedicated server: no windowing, no rendering, just the
/// schedule runner driving the fixed-tick gameplay core.
fn run_server() {
    App::new()
        .add_plugins((
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
                1.0 / config::SERVER_LOOP_HZ,
            ))),
            LogPlugin::default(),
            // Not part of MinimalPlugins, but required by physics.
            bevy::transform::TransformPlugin,
        ))
        .add_plugins(ServerCorePlugin)
        .run();
}

/// Remote client: window + rendering, connects to the given address.
fn run_client(address: String) {
    client_app("fabled - client")
        .insert_resource(ServerAddress(address))
        .run();
}

/// Listen server: full client app plus the server core in one process.
fn run_host() {
    client_app("fabled - host")
        .add_plugins(ServerCorePlugin)
        .run();
}

fn client_app(title: &str) -> App {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: title.into(),
            present_mode: PresentMode::AutoVsync,
            ..default()
        }),
        ..default()
    }))
    .add_plugins(ClientCorePlugin);
    app
}
