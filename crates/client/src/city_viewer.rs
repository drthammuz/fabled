//! Cyberpunk city GLB viewer (`--city`): renders the scene; collision is server-side.

use bevy::prelude::*;
use shared::CityViewMode;

pub struct CityViewerPlugin;

impl Plugin for CityViewerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_city);
    }
}

fn spawn_city(mut commands: Commands, asset_server: Res<AssetServer>, city: Option<Res<CityViewMode>>) {
    if city.is_none() {
        return;
    }
    let scene = asset_server.load("models/misc/cyberpunk_city.glb#Scene0");
    commands.spawn((
        SceneRoot(scene),
        Transform::IDENTITY,
        Name::new("cyberpunk_city"),
    ));
    info!(
        "cyberpunk city — WASD move | mouse look (LMB) | Space jump | Shift sprint | colliders load async"
    );
}
