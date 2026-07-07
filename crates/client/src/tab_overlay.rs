//! Tab overlay: sector map + inventory, shown while gameplay is active.
//! Owns the Tab toggle (the minimap grid renders inside this overlay's map
//! panel — see minimap.rs). Styled via `ui_theme` (Pass C).

use bevy::prelude::*;
use shared::run::RunState;
use shared::EditorMode;

use crate::hotbar::OwnInventory;
use crate::ui_theme as theme;

pub struct TabOverlayPlugin;

impl Plugin for TabOverlayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TabOverlayState>()
            .add_systems(Startup, spawn_overlay)
            .add_systems(Update, (toggle_overlay, refresh_inventory_list));
    }
}

#[derive(Resource, Default)]
pub struct TabOverlayState {
    pub visible: bool,
}

#[derive(Component)]
struct TabOverlayRoot;

/// The minimap grid is spawned as a child of this node (minimap.rs).
#[derive(Component)]
pub struct OverlayMapPanel;

/// Inventory rows are (re)built as children of this node.
#[derive(Component)]
struct InventoryList;

#[derive(Component)]
struct InventoryFooter;

fn spawn_overlay(mut commands: Commands) {
    commands
        .spawn((
            TabOverlayRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: Val::Px(14.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            Visibility::Hidden,
            GlobalZIndex(60),
        ))
        .with_children(|root| {
            // --- Map panel ---
            let (mut node, bg, border) = theme::panel();
            node.flex_direction = FlexDirection::Column;
            node.row_gap = Val::Px(6.0);
            node.padding = UiRect::all(Val::Px(12.0));
            root.spawn((node, bg, border)).with_children(|panel| {
                panel.spawn(theme::heading("SECTOR MAP"));
                panel.spawn((
                    OverlayMapPanel,
                    Node {
                        width: Val::Px(crate::minimap::MAP_PANEL_PX),
                        height: Val::Px(crate::minimap::MAP_PANEL_PX),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.0, 0.02, 0.06, 0.6)),
                ));
                panel.spawn(theme::label("· you    forward = up"));
            });

            // --- Inventory panel ---
            let (mut node, bg, border) = theme::panel();
            node.flex_direction = FlexDirection::Column;
            node.row_gap = Val::Px(6.0);
            node.padding = UiRect::all(Val::Px(12.0));
            node.width = Val::Px(300.0);
            root.spawn((node, bg, border)).with_children(|panel| {
                panel.spawn(theme::heading("INVENTORY"));
                panel.spawn((
                    InventoryList,
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(4.0),
                        ..default()
                    },
                ));
                panel.spawn((InventoryFooter, theme::label("")));
            });
        });
}

/// Tab toggles the overlay whenever the gameplay HUD is active (same rule as
/// the hotbar: gameplay or editor playtest, never the editor itself).
fn toggle_overlay(
    keys: Res<ButtonInput<KeyCode>>,
    editor: Option<Res<EditorMode>>,
    playtest: Option<Res<crate::editor_playtest::EditorPlaytestActive>>,
    capture: Res<crate::netplay::InputCapture>,
    mut state: ResMut<TabOverlayState>,
    mut root: Query<&mut Visibility, With<TabOverlayRoot>>,
) {
    let hud_hidden = (editor.is_some() && playtest.is_none()) || capture.0;
    if keys.just_pressed(KeyCode::Tab) && !hud_hidden {
        state.visible = !state.visible;
    }
    if hud_hidden {
        state.visible = false;
    }
    for mut vis in &mut root {
        *vis = if state.visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

/// Rebuilds the inventory rows when contents or selection change (rare).
fn refresh_inventory_list(
    inventory: Res<OwnInventory>,
    run: Query<&RunState>,
    list: Query<Entity, With<InventoryList>>,
    mut footer: Query<&mut Text, With<InventoryFooter>>,
    mut commands: Commands,
) {
    if !inventory.is_changed() {
        return;
    }
    let Ok(list) = list.single() else { return };

    commands.entity(list).despawn_children();
    commands.entity(list).with_children(|list| {
        for (i, slot) in inventory.slots.iter().enumerate() {
            let selected = i == inventory.selected;
            list.spawn((
                Node {
                    height: Val::Px(34.0),
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(8.0),
                    padding: UiRect::horizontal(Val::Px(6.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(if selected {
                    Color::srgba(0.06, 0.12, 0.15, 0.9)
                } else {
                    Color::srgba(0.05, 0.07, 0.10, 0.6)
                },),
                BorderColor::all(if selected {
                    theme::ACCENT
                } else {
                    Color::srgba(0.35, 0.55, 0.75, 0.15)
                }),
            ))
            .with_children(|row| {
                row.spawn((
                    Text::new(format!("{}", i + 1)),
                    TextFont { font_size: 11.0, ..default() },
                    TextColor(theme::TEXT_DIM),
                ));
                match slot {
                    Some(item) => {
                        let (tag, tint) = theme::item_style(item.id);
                        row.spawn((
                            Text::new(tag),
                            TextFont { font_size: 13.0, ..default() },
                            TextColor(tint),
                        ));
                        row.spawn((
                            Text::new(item.name.clone()),
                            TextFont { font_size: 13.0, ..default() },
                            TextColor(theme::TEXT),
                            Node { flex_grow: 1.0, ..default() },
                        ));
                        row.spawn((
                            Text::new(format!("{:.1} kg", item.weight)),
                            TextFont { font_size: 11.0, ..default() },
                            TextColor(theme::TEXT_DIM),
                        ));
                    }
                    None => {
                        row.spawn((
                            Text::new("empty"),
                            TextFont { font_size: 12.0, ..default() },
                            TextColor(Color::srgba(0.4, 0.45, 0.5, 0.5)),
                        ));
                    }
                }
            });
        }
    });

    let total: f32 = inventory
        .slots
        .iter()
        .flatten()
        .map(|item| item.weight)
        .sum();
    if let Ok(mut footer) = footer.single_mut() {
        let wallet = run
            .single()
            .map(|s| format!("Credits {} · Scrap {} · ", s.credits, s.scrap))
            .unwrap_or_default();
        footer.0 = format!("{wallet}Carrying {total:.1} kg");
    }
}
