//! Client-side hotbar: shows YOUR inventory only. Contents arrive via
//! `InventoryUpdate` messages the server addresses solely to this client.
//! Pure presentation + input state; no gameplay logic.

use bevy::prelude::*;
use shared::config;
use shared::protocol::{Item, InventoryUpdate};

pub struct HotbarPlugin;

impl Plugin for HotbarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OwnInventory>()
            .add_systems(Startup, setup_hotbar)
            .add_systems(
                Update,
                (receive_inventory, select_slot, refresh_hotbar).chain(),
            );
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

#[derive(Component)]
struct HotbarSlotText(usize);

const SLOT_BG: Color = Color::srgba(0.1, 0.1, 0.12, 0.75);
const SLOT_BG_SELECTED: Color = Color::srgba(0.35, 0.3, 0.1, 0.9);

fn setup_hotbar(mut commands: Commands) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(18.0),
            width: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|row| {
            for i in 0..config::INVENTORY_SLOTS {
                row.spawn((
                    HotbarSlot(i),
                    Node {
                        width: Val::Px(110.0),
                        height: Val::Px(44.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        flex_direction: FlexDirection::Column,
                        ..default()
                    },
                    BackgroundColor(SLOT_BG),
                ))
                .with_children(|slot| {
                    slot.spawn((
                        HotbarSlotText(i),
                        Text::new(format!("{}", i + 1)),
                        TextFont {
                            font_size: 13.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.8, 0.8, 0.8)),
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

fn select_slot(keys: Res<ButtonInput<KeyCode>>, mut inventory: ResMut<OwnInventory>) {
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
    mut slots: Query<(&HotbarSlot, &mut BackgroundColor)>,
    mut texts: Query<(&HotbarSlotText, &mut Text)>,
) {
    if !inventory.is_changed() {
        return;
    }
    for (slot, mut bg) in &mut slots {
        bg.0 = if slot.0 == inventory.selected {
            SLOT_BG_SELECTED
        } else {
            SLOT_BG
        };
    }
    for (slot, mut text) in &mut texts {
        let label = match inventory.slots.get(slot.0).and_then(Option::as_ref) {
            Some(item) => format!("{} {}", slot.0 + 1, item.name),
            None => format!("{}", slot.0 + 1),
        };
        text.0 = label;
    }
}
