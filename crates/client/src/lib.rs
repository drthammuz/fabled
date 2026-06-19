use bevy::prelude::*;

pub mod audio;
pub mod character_animation;
pub mod city_viewer;
pub mod class_select;
pub mod display_settings;
pub mod process_spawn;
pub mod darkness;
pub mod floor_debug;
pub mod fog_noise;
pub mod fly_camera;
pub mod hotbar;
pub mod level_render;
pub mod minimap;
pub mod netplay;
pub mod prop_render;
pub mod run_ui;
pub mod sewer_atmosphere;
pub mod editor_floor;
pub mod editor_ops;
pub mod editor_history;
pub mod editor_playtest;
pub mod editor_selection;
pub mod editor_sidebar;
pub mod editor_state;
pub mod editor_ui;
pub mod editor_workspace;
pub mod kenney_editor;
pub mod test_showcase;
pub mod tunnel_mesh;
pub mod water_render;
// Village intermezzo parked at git tag `base`; not wired into the game.
#[cfg(feature = "village")]
pub mod terrain_render;
#[cfg(feature = "village")]
pub mod villager_ui;
#[cfg(feature = "village")]
pub mod villagers;

/// Address of the server this client should connect to (set from CLI).
/// Unused until netcode lands in M3; host mode connects to localhost.
#[derive(Resource, Debug, Clone)]
pub struct ServerAddress(pub String);

/// Core client-side plugin: rendering, input, UI. Never gameplay logic.
pub struct ClientCorePlugin;

impl Plugin for ClientCorePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            class_select::ClassSelectPlugin,
            display_settings::DisplaySettingsPlugin,
            fog_noise::FogNoisePlugin,
            water_render::WaterRenderPlugin,
            level_render::LevelRenderPlugin,
            sewer_atmosphere::SewerAtmospherePlugin,
            darkness::DarknessPlugin,
            fly_camera::FlyCameraPlugin,
            prop_render::PropRenderPlugin,
            netplay::NetPlayPlugin,
            hotbar::HotbarPlugin,
            run_ui::RunUiPlugin,
            audio::GameAudioPlugin,
            minimap::MinimapPlugin,
            character_animation::CharacterAnimationPlugin,
        ))
        .add_plugins((
            test_showcase::TestShowcasePlugin,
            city_viewer::CityViewerPlugin,
            editor_playtest::EditorPlaytestPlugin,
            kenney_editor::KenneyEditorPlugin,
            floor_debug::FloorDebugPlugin,
        ))
        .add_systems(Startup, log_startup);
    }
}

fn log_startup(address: Option<Res<ServerAddress>>) {
    match address {
        Some(addr) => info!("client core running, server address: {}", addr.0),
        None => info!("client core running (local host mode)"),
    }
}
