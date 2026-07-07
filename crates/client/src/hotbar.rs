//! Client-side hotbar: shows YOUR inventory only. Contents arrive via
//! `InventoryUpdate` messages the server addresses solely to this client.
//! Pure presentation + input state; no gameplay logic.

use bevy::prelude::*;
use shared::config;
use shared::protocol::{Item, InventoryUpdate};
use shared::EditorMode;

use crate::ui_theme as theme;

pub struct HotbarPlugin;

impl Plugin for HotbarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OwnInventory>()
            .add_systems(Startup, setup_hotbar)
            .add_systems(
                Update,
                (sync_hotbar_visibility, receive_inventory, select_slot, refresh_hotbar)
                    .chain(),
            );
    }
}

#[derive(Component)]
struct HotbarRoot;

/// Visible during gameplay AND editor playtest; hidden in the editor itself.
fn sync_hotbar_visibility(
    editor: Option<Res<EditorMode>>,
    playtest: Option<Res<crate::editor_playtest::EditorPlaytestActive>>,
    mut root: Query<&mut Visibility, With<HotbarRoot>>,
) {
    let hidden = editor.is_some() && playtest.is_none();
    for mut vis in &mut root {
        *vis = if hidden {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
}

/// Local mirror of this client's server-side inventory.
#[derive(Resource)]
pub struct OwnInventory {
    pub slots: Vec<Option<Item>>,
    pub selected: usize,
}

impl Default for OwnInventory {
    fn default() -> Self {
        Self {
            slots: vec![None; config::INVENTORY_SLOTS],
            selected: 0,
        }
    }
}

#[derive(Component)]
struct HotbarSlot(usize);

/// Large colored item tag in the slot center (the v1 "icon").
#[derive(Component)]
struct HotbarSlotTag(usize);

/// Small item name under the tag.
#[derive(Component)]
struct HotbarSlotName(usize);

fn setup_hotbar(mut commands: Commands) {
    commands
        .spawn((
            HotbarRoot,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(18.0),
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                column_gap: Val::Px(6.0),
                ..default()
            },
        ))
        .with_children(|row| {
            for i in 0..config::INVENTORY_SLOTS {
                row.spawn((
                    HotbarSlot(i),
                    Node {
                        width: Val::Px(64.0),
                        height: Val::Px(64.0),
                        border: UiRect::all(Val::Px(1.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(2.0),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    BackgroundColor(theme::PANEL_BG),
                    BorderColor::all(theme::PANEL_BORDER),
                ))
                .with_children(|slot| {
                    // Key hint, pinned to the slot corner.
                    slot.spawn((
                        Text::new(format!("{}", i + 1)),
                        TextFont { font_size: 10.0, ..default() },
                        TextColor(theme::TEXT_DIM),
                        Node {
                            position_type: PositionType::Absolute,
                            top: Val::Px(2.0),
                            left: Val::Px(4.0),
                            ..default()
                        },
                    ));
                    slot.spawn((
                        HotbarSlotTag(i),
                        Text::new(""),
                        TextFont { font_size: 17.0, ..default() },
                        TextColor(theme::TEXT_DIM),
                    ));
                    slot.spawn((
                        HotbarSlotName(i),
                        Text::new(""),
                        TextFont { font_size: 9.0, ..default() },
                        TextColor(theme::TEXT_DIM),
                    ));
                });
            }
        });
}

fn receive_inventory(
    mut updates: MessageReader<InventoryUpdate>,
    mut inventory: ResMut<OwnInventory>,
) {
    if let Some(update) = updates.read().last() {
        inventory.slots = update.slots.clone();
    }
}

fn select_slot(
    keys: Res<ButtonInput<KeyCode>>,
    capture: Res<crate::netplay::InputCapture>,
    mut inventory: ResMut<OwnInventory>,
) {
    // Number keys belong to the dialogue/trade window while it's open.
    if capture.0 {
        return;
    }
    const SLOT_KEYS: [KeyCode; 4] = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
    ];
    for (i, key) in SLOT_KEYS.iter().enumerate().take(config::INVENTORY_SLOTS) {
        if keys.just_pressed(*key) {
            inventory.selected = i;
        }
    }
}

fn refresh_hotbar(
    inventory: Res<OwnInventory>,
    mut slots: Query<(&HotbarSlot, &mut BorderColor, &mut BackgroundColor)>,
    mut tags: Query<(&HotbarSlotTag, &mut Text, &mut TextColor), Without<HotbarSlotName>>,
    mut names: Query<(&HotbarSlotName, &mut Text), Without<HotbarSlotTag>>,
) {
    if !inventory.is_changed() {
        return;
    }
    for (slot, mut border, mut bg) in &mut slots {
        let selected = slot.0 == inventory.selected;
        *border = BorderColor::all(if selected { theme::ACCENT } else { theme::PANEL_BORDER });
        bg.0 = if selected {
            Color::srgba(0.06, 0.12, 0.15, 0.9)
        } else {
            theme::PANEL_BG
        };
    }
    for (tag, mut text, mut color) in &mut tags {
        match inventory.slots.get(tag.0).and_then(Option::as_ref) {
            Some(item) => {
                let (label, tint) = theme::item_style(item.id);
                text.0 = label.to_string();
                color.0 = tint;
            }
            None => {
                text.0 = "·".to_string();
                color.0 = theme::TEXT_DIM;
            }
        }
    }
    for (name, mut text) in &mut names {
        text.0 = inventory
            .slots
            .get(name.0)
            .and_then(Option::as_ref)
            .map(|item| item.name.clone())
            .unwrap_or_default();
    }
}
