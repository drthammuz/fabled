//! Center-screen crosshair, shown while a gun is the selected hotbar item.
//! Works in first AND third person: the client casts the camera ray through
//! screen center each frame (`fly_camera::CrosshairAim`) and the server fires
//! toward that point, so shots land on the crosshair in both modes. Pure
//! presentation here.

use bevy::prelude::*;
use shared::protocol::PlayerAlive;
use shared::EditorMode;

use crate::class_select::SelectState;
use crate::editor_playtest::EditorPlaytestActive;
use crate::hotbar::OwnInventory;
use crate::netplay::OwnPlayer;

pub struct CrosshairPlugin;

impl Plugin for CrosshairPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_crosshair)
            .add_systems(Update, sync_crosshair);
    }
}

#[derive(Component)]
struct CrosshairRoot;

/// The four gun bars, grouped so they can hide independently of the always-on
/// center dot.
#[derive(Component)]
struct CrosshairBars;

const BAR_THICKNESS: f32 = 2.0;
const BAR_LENGTH: f32 = 7.0;
const BAR_GAP: f32 = 3.0;
const BAR_COLOR: Color = Color::srgba(0.95, 0.95, 0.95, 0.9);
const DOT_SIZE: f32 = 3.0;
const DOT_COLOR: Color = Color::srgba(0.95, 0.95, 0.95, 0.85);

fn spawn_crosshair(mut commands: Commands) {
    // Zero-size anchor at screen center; children hang off it with absolute
    // pixel offsets, so no flexbox math can drift the center.
    commands
        .spawn((
            CrosshairRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(50.0),
                width: Val::Px(0.0),
                height: Val::Px(0.0),
                ..default()
            },
            Visibility::Hidden,
            GlobalZIndex(40),
        ))
        .with_children(|anchor| {
            // Always-on center dot: an aim/interaction point shown whenever the
            // player is in the world, even with no weapon out (pickups, etc.).
            anchor.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(-DOT_SIZE * 0.5),
                    top: Val::Px(-DOT_SIZE * 0.5),
                    width: Val::Px(DOT_SIZE),
                    height: Val::Px(DOT_SIZE),
                    border_radius: BorderRadius::all(Val::Px(DOT_SIZE * 0.5)),
                    ..default()
                },
                BackgroundColor(DOT_COLOR),
            ));
            // Gun bars: shown only when a gun is the selected item.
            anchor
                .spawn((
                    CrosshairBars,
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Px(0.0),
                        height: Val::Px(0.0),
                        ..default()
                    },
                    Visibility::Inherited,
                ))
                .with_children(|bars_root| {
                    let half = BAR_THICKNESS * 0.5;
                    let bars = [
                        // (left, top, width, height)
                        (-half, -(BAR_GAP + BAR_LENGTH), BAR_THICKNESS, BAR_LENGTH),
                        (-half, BAR_GAP, BAR_THICKNESS, BAR_LENGTH),
                        (-(BAR_GAP + BAR_LENGTH), -half, BAR_LENGTH, BAR_THICKNESS),
                        (BAR_GAP, -half, BAR_LENGTH, BAR_THICKNESS),
                    ];
                    for (left, top, width, height) in bars {
                        bars_root.spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                left: Val::Px(left),
                                top: Val::Px(top),
                                width: Val::Px(width),
                                height: Val::Px(height),
                                ..default()
                            },
                            BackgroundColor(BAR_COLOR),
                        ));
                    }
                });
        });
}

/// Visible while alive in gameplay (normal run OR editor playtest) with a gun
/// in the selected slot; hidden otherwise (editor proper, class select, bat
/// out, dead).
fn sync_crosshair(
    inventory: Res<OwnInventory>,
    editor: Option<Res<EditorMode>>,
    playtest: Option<Res<EditorPlaytestActive>>,
    select: Res<State<SelectState>>,
    capture: Res<crate::netplay::InputCapture>,
    player: Query<Option<&PlayerAlive>, With<OwnPlayer>>,
    mut root: Query<&mut Visibility, (With<CrosshairRoot>, Without<CrosshairBars>)>,
    mut bars: Query<&mut Visibility, (With<CrosshairBars>, Without<CrosshairRoot>)>,
) {
    let gun_out = inventory
        .slots
        .get(inventory.selected)
        .and_then(Option::as_ref)
        .is_some_and(shared::items::is_gun);
    let in_game = playtest.is_some()
        || (editor.is_none() && *select.get() == SelectState::Playing);
    let alive = player
        .single()
        .map(|alive| alive.is_none_or(|a| a.0))
        .unwrap_or(false);
    // Center dot shows whenever the player is in the world; the gun bars only
    // when a gun is selected.
    let show_dot = in_game && alive && !capture.0;
    for mut vis in &mut root {
        *vis = if show_dot { Visibility::Visible } else { Visibility::Hidden };
    }
    for mut vis in &mut bars {
        *vis = if gun_out { Visibility::Inherited } else { Visibility::Hidden };
    }
}
