//! Run HUD: credits, phase, shop and route prompts.

use bevy::prelude::*;
use shared::run::{RunPhase, RunState};

pub struct RunUiPlugin;

impl Plugin for RunUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_hud)
            .add_systems(Update, update_hud);
    }
}

#[derive(Component)]
struct RunHud;

fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        RunHud,
        Text::new(""),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        TextColor(Color::srgb(0.7, 0.95, 1.0)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            left: Val::Px(12.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
    ));
}

fn update_hud(run: Query<&RunState>, mut hud: Query<&mut Text, With<RunHud>>) {
    let Ok(state) = run.single() else { return };
    let Ok(mut text) = hud.single_mut() else { return };
    let phase = match state.phase {
        RunPhase::InStretch => "IN STRETCH".to_string(),
        RunPhase::InHub => state.hub_id.as_deref()
            .map(|h| format!("IN CAMP — {h}"))
            .unwrap_or_else(|| "IN CAMP".to_string()),
        RunPhase::RunOver => "RUN OVER — press R to restart".to_string(),
    };
    let display_id = state.hub_id.as_deref().unwrap_or(&state.level_id);
    let mut lines = vec![
        format!("{phase} · {display_id}"),
        format!("Credits: {} · Scrap: {}  |  seed: {}", state.credits, state.scrap, state.run_seed),
    ];
    if let Some(holder) = &state.map_holder {
        lines.push(format!("Map: {holder}"));
    }
    if state.phase == RunPhase::InHub {
        lines.push("Shop: 1=Flashlight 2=Bat 3=Map | Routes: 7/8/9".to_string());
        for (i, route) in state.route_options.iter().enumerate() {
            lines.push(format!("  {} — {} ({}c)", i + 7, route.label, route.cost));
        }
    }
    if state.phase == RunPhase::InStretch {
        lines.push("Reach the airlock together to extract".to_string());
        lines.push("Ctrl=crouch · F=flashlight · V=bat attack · E=pickup".to_string());
    }
    let next = lines.join("\n");
    if text.0 != next {
        text.0 = next;
    }
}
