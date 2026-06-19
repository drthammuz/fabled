//! Run HUD: credits, phase, shop and route prompts.

use bevy::prelude::*;
use shared::kenney_hub;
use shared::kenney_layout::KenneyLayout;
use shared::run::{RunPhase, RunState};
use shared::EditorMode;

pub struct RunUiPlugin;

impl Plugin for RunUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_hud)
            .add_systems(Update, update_hud);
    }
}

#[derive(Component)]
struct RunHud;

fn spawn_hud(mut commands: Commands, editor: Option<Res<EditorMode>>) {
    if editor.is_some() {
        return;
    }
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

fn update_hud(
    run: Query<&RunState>,
    player: Query<&Transform, With<crate::netplay::OwnPlayer>>,
    mut hud: Query<&mut Text, With<RunHud>>,
) {
    let Ok(state) = run.single() else { return };
    let Ok(mut text) = hud.single_mut() else { return };
    let layout = KenneyLayout::load_from_disk();
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
        if let Some(exit) = state.hub_commit.chosen_exit {
            let n = state.hub_commit.player_exits.len();
            lines.push(format!("Exit locked: L{exit} — {n} committed"));
            for e in [2u8, 3, 4] {
                if state.hub_commit.is_exit_closed(e) {
                    lines.push(format!("  L{e} closed"));
                }
            }
            if state.hub_commit.l1_unloaded {
                lines.push("L1 stretch unloaded — in branch".to_string());
            } else {
                lines.push("All operators must reach the same exit".to_string());
            }
        } else {
            lines.push("Exits: centre pit | west corridor | west gate".to_string());
            if !state.map_stream.candidates.is_empty() {
                for (exit, id) in &state.map_stream.candidates {
                    lines.push(format!("  exit {exit}: {id} mounted below"));
                }
            } else {
                lines.push("Full maps stream in under hub holes (no loading screen)".to_string());
            }
        }
        if let Ok(tf) = player.single() {
            for (key, branch) in &layout.branch_levels {
                if kenney_hub::in_branch_destination(tf.translation, branch) {
                    lines.push(format!("Inside branch L{key}: {}", branch.label));
                }
            }
        }
        lines.push("Shop: 1=Flashlight 2=Bat 3=Map".to_string());
        if state.hub_commit.chosen_exit.is_none() {
            for (i, route) in state.route_options.iter().enumerate() {
                lines.push(format!("  {} — {} ({}c)", i + 7, route.label, route.cost));
            }
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
