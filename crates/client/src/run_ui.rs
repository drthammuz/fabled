//! Run HUD: status panel (phase/credits), contextual info, health bar,
//! death banner, and a damage vignette. Styled via `ui_theme` (Pass C).

use bevy::prelude::*;
use shared::run::{RunPhase, RunState};
use shared::EditorMode;

use crate::ui_theme as theme;

pub struct RunUiPlugin;

impl Plugin for RunUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_hud).add_systems(
            Update,
            (
                sync_hud_visibility,
                update_status_text,
                update_health_bar,
                update_damage_vignette,
            ),
        );
    }
}

/// Marker for every top-level HUD node (visibility is driven as a group).
#[derive(Component)]
struct RunHud;

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct ContextText;

#[derive(Component)]
struct HealthBarFill;

#[derive(Component)]
struct HealthBarText;

#[derive(Component)]
struct DeathBanner;

#[derive(Component)]
struct DamageVignette;

/// Last-seen own HP, for detecting damage ticks (drives the vignette).
#[derive(Resource, Default)]
struct LastHp(Option<f32>);

fn spawn_hud(mut commands: Commands) {
    commands.init_resource::<LastHp>();

    // Top-left: status (phase · location · credits) + contextual info below.
    commands
        .spawn((
            RunHud,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(10.0),
                left: Val::Px(12.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                ..default()
            },
        ))
        .with_children(|col| {
            let (node, bg, border) = theme::panel();
            col.spawn((node, bg, border)).with_children(|panel| {
                panel.spawn((StatusText, theme::body("")));
            });
            let (mut node, bg, border) = theme::panel();
            node.padding = UiRect::all(Val::Px(6.0));
            col.spawn((node, bg, border)).with_children(|panel| {
                let (text, mut font, _) = theme::label("");
                font.font_size = 12.0;
                panel.spawn((ContextText, text, font, TextColor(theme::TEXT_DIM)));
            });
        });

    // Bottom-left: health bar.
    let (mut node, bg, border) = theme::panel();
    node.position_type = PositionType::Absolute;
    node.left = Val::Px(12.0);
    node.bottom = Val::Px(18.0);
    node.width = Val::Px(230.0);
    node.flex_direction = FlexDirection::Column;
    node.row_gap = Val::Px(4.0);
    commands.spawn((RunHud, node, bg, border)).with_children(|panel| {
        panel
            .spawn(Node {
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            })
            .with_children(|row| {
                row.spawn(theme::heading("HP"));
                row.spawn((HealthBarText, theme::label("--")));
            });
        // Bar track + fill (rounded, so the bar reads as a modern meter).
        panel
            .spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(14.0),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(7.0)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.06, 0.08, 0.11, 0.95)),
                BorderColor::all(theme::ACCENT_DIM),
            ))
            .with_children(|track| {
                track.spawn((
                    HealthBarFill,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        border_radius: BorderRadius::all(Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(theme::GOOD),
                ));
            });
    });

    // Center death banner.
    commands
        .spawn((
            RunHud,
            DeathBanner,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(38.0),
                ..default()
            },
            Visibility::Hidden,
            GlobalZIndex(45),
        ))
        .with_children(|banner| {
            banner.spawn((
                Text::new("DEAD — respawning…"),
                TextFont { font_size: 34.0, ..default() },
                TextColor(theme::DANGER),
                Node {
                    // Center on the anchor: shift back by half the text width.
                    position_type: PositionType::Absolute,
                    left: Val::Px(-160.0),
                    ..default()
                },
            ));
        });

    // Full-screen damage vignette (alpha driven by recent HP loss).
    commands.spawn((
        RunHud,
        DamageVignette,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.6, 0.05, 0.05, 0.0)),
        GlobalZIndex(30),
    ));
}

/// The HUD shows during gameplay AND editor playtest, but not in the editor
/// itself (previously it was skipped entirely in editor mode, so playtests
/// had no HP/damage feedback at all).
fn sync_hud_visibility(
    editor: Option<Res<EditorMode>>,
    playtest: Option<Res<crate::editor_playtest::EditorPlaytestActive>>,
    mut hud: Query<&mut Visibility, (With<RunHud>, Without<DeathBanner>)>,
) {
    let hidden = editor.is_some() && playtest.is_none();
    for mut vis in &mut hud {
        *vis = if hidden {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
}

fn update_status_text(
    run: Query<&RunState>,
    mut status: Query<&mut Text, (With<StatusText>, Without<ContextText>)>,
    mut context: Query<&mut Text, (With<ContextText>, Without<StatusText>)>,
) {
    let Ok(mut status) = status.single_mut() else { return };
    let Ok(mut context) = context.single_mut() else { return };

    let Ok(state) = run.single() else {
        // Editor playtest: no run entity. Keep the panels minimal.
        set_if_changed(&mut status, "PLAYTEST".to_string());
        set_if_changed(
            &mut context,
            "Ctrl=crouch · F=flashlight · LMB=fire/swing · E=pickup · Tab=map".to_string(),
        );
        return;
    };

    let phase = match state.phase {
        RunPhase::InStretch => "IN STRETCH".to_string(),
        RunPhase::InHub => state
            .hub_id
            .as_deref()
            .map(|h| format!("IN CAMP — {h}"))
            .unwrap_or_else(|| "IN CAMP".to_string()),
        RunPhase::RunOver => "RUN OVER — press R to restart".to_string(),
    };
    let display_id = state.hub_id.as_deref().unwrap_or(&state.level_id);
    set_if_changed(
        &mut status,
        format!(
            "{phase} · {display_id}\nCredits {} · Scrap {}",
            state.credits, state.scrap
        ),
    );

    let mut lines: Vec<String> = Vec::new();
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
        } else if !state.map_stream.candidates.is_empty() {
            // Real pool game: next sectors are mounted under the exit holes.
            lines.push("Drop through an exit hole to the next sector".to_string());
            for (exit, id) in &state.map_stream.candidates {
                lines.push(format!("  exit {exit}: {id} mounted below"));
            }
        }
        // Legacy stretch-graph routes (keys 7+) — only the old sewer game
        // populates `route_options`; the pool game leaves it empty.
        if state.hub_commit.chosen_exit.is_none() {
            for (i, route) in state.route_options.iter().enumerate() {
                lines.push(format!("  {} — {} ({}c)", i + 7, route.label, route.cost));
            }
        }
    }
    if state.phase == RunPhase::InStretch {
        lines.push("Reach the airlock together to extract".to_string());
        lines.push("Ctrl=crouch · F=flashlight · LMB=fire/swing · E=pickup · Tab=map".to_string());
    }
    set_if_changed(&mut context, lines.join("\n"));
}

fn set_if_changed(text: &mut Text, next: String) {
    if text.0 != next {
        text.0 = next;
    }
}

fn update_health_bar(
    health: Query<&shared::protocol::PlayerHealth, With<crate::netplay::OwnPlayer>>,
    editor: Option<Res<EditorMode>>,
    playtest: Option<Res<crate::editor_playtest::EditorPlaytestActive>>,
    mut fill: Query<(&mut Node, &mut BackgroundColor), With<HealthBarFill>>,
    mut label: Query<&mut Text, With<HealthBarText>>,
    mut banner: Query<&mut Visibility, With<DeathBanner>>,
) {
    let Ok((mut fill_node, mut fill_color)) = fill.single_mut() else { return };
    let Ok(mut label) = label.single_mut() else { return };

    let (frac, text, dead) = match health.single() {
        Ok(hp) => (
            (hp.current / hp.max).clamp(0.0, 1.0),
            format!("{:.0} / {:.0}", hp.current, hp.max),
            hp.current <= 0.5,
        ),
        Err(_) => (1.0, "--".to_string(), false),
    };
    fill_node.width = Val::Percent(frac * 100.0);
    fill_color.0 = if frac > 0.5 {
        theme::GOOD
    } else if frac > 0.25 {
        theme::WARN
    } else {
        theme::DANGER
    };
    set_if_changed(&mut label, text);

    let hud_hidden = editor.is_some() && playtest.is_none();
    for mut vis in &mut banner {
        *vis = if dead && !hud_hidden {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

/// Red screen edge flash on taking damage: alpha spikes with the HP drop and
/// exponentially fades. Server-authoritative (keyed off replicated HP), so it
/// also fires for fall damage etc., not just enemy hits.
fn update_damage_vignette(
    time: Res<Time>,
    health: Query<&shared::protocol::PlayerHealth, With<crate::netplay::OwnPlayer>>,
    mut last: ResMut<LastHp>,
    mut vignette: Query<&mut BackgroundColor, With<DamageVignette>>,
) {
    let Ok(mut color) = vignette.single_mut() else { return };
    let current = health.single().map(|h| h.current).ok();

    let mut alpha = color.0.alpha();
    if let (Some(prev), Some(now)) = (last.0, current) {
        let drop = prev - now;
        if drop > 0.01 {
            alpha = (alpha + drop * 0.015 + 0.12).min(0.4);
        }
    }
    alpha *= f32::exp(-3.0 * time.delta_secs());
    if alpha < 0.005 {
        alpha = 0.0;
    }
    color.0.set_alpha(alpha);
    last.0 = current;
}
