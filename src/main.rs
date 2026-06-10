use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bevy::app::ScheduleRunnerPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::window::PresentMode;
use bevy_replicon::prelude::*;
use bevy_replicon_renet::netcode::{
    ClientAuthentication, NetcodeClientTransport, NetcodeServerTransport, ServerAuthentication,
    ServerConfig,
};
use bevy_replicon_renet::renet::ConnectionConfig;
use bevy_replicon_renet::{RenetChannelsExt, RenetClient, RenetServer, RepliconRenetPlugins};
use clap::Parser;

use client::ClientCorePlugin;
use server::ServerCorePlugin;
use shared::config;
use shared::protocol::ProtocolPlugin;

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
            // Not part of MinimalPlugins, but required by physics/replicon.
            bevy::transform::TransformPlugin,
            StatesPlugin,
        ))
        .add_plugins((RepliconPlugins, RepliconRenetPlugins, ProtocolPlugin))
        .add_plugins(ServerCorePlugin)
        .add_systems(Startup, open_server)
        .run();
}

/// Remote client: window + rendering, connects to the given address.
fn run_client(address: String) {
    client_app("fabled - client")
        .insert_resource(ServerAddress(address))
        .add_systems(Startup, connect_client)
        .run();
}

/// Listen server: full client app plus the server core in one process.
fn run_host() {
    client_app("fabled - host")
        .add_plugins(ServerCorePlugin)
        .add_systems(Startup, open_server)
        .add_systems(PostStartup, server::players::spawn_local_player)
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
    .add_plugins((RepliconPlugins, RepliconRenetPlugins, ProtocolPlugin))
    .add_plugins(ClientCorePlugin);
    app
}

#[derive(Resource)]
struct ServerAddress(String);

fn open_server(channels: Res<RepliconChannels>, mut commands: Commands) {
    let server = RenetServer::new(connection_config(&channels));
    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), config::DEFAULT_PORT);
    let socket = UdpSocket::bind(bind_addr).expect("failed to bind server UDP socket");
    let server_config = ServerConfig {
        current_time: unix_time(),
        max_clients: config::MAX_CLIENTS,
        protocol_id: config::PROTOCOL_ID,
        public_addresses: vec![bind_addr],
        authentication: ServerAuthentication::Unsecure,
    };
    let transport = NetcodeServerTransport::new(server_config, socket)
        .expect("failed to create server transport");
    commands.insert_resource(server);
    commands.insert_resource(transport);
    info!("server listening on udp port {}", config::DEFAULT_PORT);
}

fn connect_client(
    channels: Res<RepliconChannels>,
    address: Res<ServerAddress>,
    mut commands: Commands,
) {
    let server_addr = parse_address(&address.0);
    let client = RenetClient::new(connection_config(&channels));
    // The netcode client id only needs to be unique per server session.
    let client_id = unix_time().as_nanos() as u64;
    let authentication = ClientAuthentication::Unsecure {
        client_id,
        protocol_id: config::PROTOCOL_ID,
        server_addr,
        user_data: None,
    };
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).expect("failed to bind UDP socket");
    let transport = NetcodeClientTransport::new(unix_time(), authentication, socket)
        .expect("failed to create client transport");
    commands.insert_resource(client);
    commands.insert_resource(transport);
    info!("connecting to {server_addr}");
}

fn connection_config(channels: &RepliconChannels) -> ConnectionConfig {
    ConnectionConfig {
        server_channels_config: channels.server_configs(),
        client_channels_config: channels.client_configs(),
        ..Default::default()
    }
}

fn parse_address(input: &str) -> SocketAddr {
    let with_port = if input.contains(':') {
        input.to_string()
    } else {
        format!("{input}:{}", config::DEFAULT_PORT)
    };
    with_port
        .parse()
        .unwrap_or_else(|_| panic!("invalid server address: {input}"))
}

fn unix_time() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
}
