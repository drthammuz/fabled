//! NPC dialogue + trade window (Pass D), built on the ui_theme framework.
//!
//! Opens when the server answers an E-interact on an NPC with `NpcDialogue`.
//! While open it sets `netplay::InputCapture` so the keyboard drives the
//! window instead of the player. All economy changes happen server-side
//! (`server::npc`): buy/sell requests travel as `PlayerInput::trade_buy` /
//! `trade_sell` via the `PendingTrade` queue, and the window re-renders from
//! the replicated results (OwnInventory + RunState credits).

use bevy::prelude::*;
use shared::items;
use shared::protocol::{NpcDialogue, PlayerAlive};
use shared::run::RunState;

use crate::hotbar::OwnInventory;
use crate::netplay::{InputCapture, OwnPlayer, PendingTrade};
use crate::ui_theme as theme;

/// The client closes the window past this distance from the NPC. Slightly
/// under the server's 4.5 m trade validation range so a queued action from
/// the final frame is still honored.
const CLOSE_RANGE: f32 = 4.25;
/// "E — talk" prompt shows within this range when facing an NPC.
const PROMPT_RANGE: f32 = 3.2;

pub struct DialoguePlugin;

impl Plugin for DialoguePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DialogueState>()
            .add_systems(Startup, (spawn_window, spawn_prompt))
            .add_systems(
                Update,
                (
                    receive_dialogue,
                    dialogue_keys,
                    auto_close,
                    rebuild_window,
                    sync_talk_prompt,
                )
                    .chain(),
            );
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Mode {
    Options,
    Trade,
}

/// Which trade column the cursor is in.
#[derive(PartialEq, Clone, Copy)]
enum Col {
    Buy,
    Sell,
}

struct OpenDialogue {
    data: NpcDialogue,
    mode: Mode,
    rumor_shown: bool,
    col: Col,
    buy_i: usize,
    sell_i: usize,
}

#[derive(Resource, Default)]
struct DialogueState {
    open: Option<OpenDialogue>,
    dirty: bool,
}

#[derive(Component)]
struct DialogueRoot;
#[derive(Component)]
struct DialoguePanel;
#[derive(Component)]
struct TalkPrompt;

fn spawn_window(mut commands: Commands) {
    // Full-screen invisible flex container centers the panel at any resolution.
    commands
        .spawn((
            DialogueRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Visibility::Hidden,
            GlobalZIndex(60),
        ))
        .with_children(|root| {
            let (node, bg, border) = theme::panel();
            root.spawn((
                DialoguePanel,
                Node {
                    width: Val::Px(560.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(8.0),
                    padding: UiRect::all(Val::Px(14.0)),
                    ..node
                },
                bg,
                border,
            ));
        });
}

fn spawn_prompt(mut commands: Commands) {
    commands.spawn((
        TalkPrompt,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            bottom: Val::Px(120.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        Visibility::Hidden,
        GlobalZIndex(40),
        children![(
            Text::new("[E]  talk"),
            TextFont { font_size: 14.0, ..default() },
            TextColor(theme::ACCENT),
        )],
    ));
}

fn receive_dialogue(
    mut messages: MessageReader<NpcDialogue>,
    mut state: ResMut<DialogueState>,
    mut capture: ResMut<InputCapture>,
) {
    let Some(msg) = messages.read().last() else {
        return;
    };
    state.open = Some(OpenDialogue {
        data: msg.clone(),
        mode: Mode::Options,
        rumor_shown: false,
        col: Col::Buy,
        buy_i: 0,
        sell_i: 0,
    });
    state.dirty = true;
    capture.0 = true;
}

fn close(state: &mut DialogueState, capture: &mut InputCapture) {
    state.open = None;
    state.dirty = true;
    capture.0 = false;
}

fn dialogue_keys(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    inventory: Res<OwnInventory>,
    mut state: ResMut<DialogueState>,
    mut capture: ResMut<InputCapture>,
    mut trade: ResMut<PendingTrade>,
) {
    let state = &mut *state;
    let Some(dlg) = state.open.as_mut() else {
        return;
    };
    let esc = keys.just_pressed(KeyCode::Escape);
    if esc {
        // Eat the press so fly_camera's Esc handler doesn't ungrab the cursor.
        keys.clear_just_pressed(KeyCode::Escape);
    }

    match dlg.mode {
        Mode::Options => {
            if keys.just_pressed(KeyCode::Digit1) {
                dlg.mode = Mode::Trade;
                state.dirty = true;
            } else if keys.just_pressed(KeyCode::Digit2) {
                dlg.rumor_shown = true;
                state.dirty = true;
            } else if esc || keys.just_pressed(KeyCode::KeyE) {
                close(state, &mut capture);
            }
        }
        Mode::Trade => {
            if esc {
                dlg.mode = Mode::Options;
                state.dirty = true;
                return;
            }
            let sell_slots: Vec<usize> = occupied_slots(&inventory);
            let (len, cursor) = match dlg.col {
                Col::Buy => (dlg.data.stock.len(), &mut dlg.buy_i),
                Col::Sell => (sell_slots.len(), &mut dlg.sell_i),
            };
            if keys.just_pressed(KeyCode::ArrowUp) && *cursor > 0 {
                *cursor -= 1;
                state.dirty = true;
            }
            if keys.just_pressed(KeyCode::ArrowDown) && *cursor + 1 < len.max(1) {
                *cursor += 1;
                state.dirty = true;
            }
            if keys.just_pressed(KeyCode::ArrowLeft) && dlg.col == Col::Sell {
                dlg.col = Col::Buy;
                state.dirty = true;
            }
            if keys.just_pressed(KeyCode::ArrowRight) && dlg.col == Col::Buy {
                dlg.col = Col::Sell;
                state.dirty = true;
            }
            if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) {
                match dlg.col {
                    Col::Buy => {
                        if dlg.buy_i < dlg.data.stock.len() {
                            trade.buy = Some(dlg.buy_i as u8);
                        }
                    }
                    Col::Sell => {
                        if let Some(slot) = sell_slots.get(dlg.sell_i) {
                            trade.sell = Some(*slot as u8);
                        }
                    }
                }
            }
        }
    }
}

/// Inventory slot indices that hold an item (the sell list).
fn occupied_slots(inventory: &OwnInventory) -> Vec<usize> {
    inventory
        .slots
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.is_some().then_some(i))
        .collect()
}

/// Walking away (getting shoved — movement itself is frozen) or dying closes
/// the window.
fn auto_close(
    player: Query<(&Transform, Option<&PlayerAlive>), With<OwnPlayer>>,
    mut state: ResMut<DialogueState>,
    mut capture: ResMut<InputCapture>,
) {
    let Some(dlg) = state.open.as_ref() else {
        return;
    };
    let Ok((tf, alive)) = player.single() else {
        close(&mut state, &mut capture);
        return;
    };
    let dead = alive.is_some_and(|a| !a.0);
    if dead || tf.translation.distance(dlg.data.npc_pos) > CLOSE_RANGE {
        close(&mut state, &mut capture);
    }
}

fn rebuild_window(
    mut commands: Commands,
    inventory: Res<OwnInventory>,
    run: Query<&RunState>,
    mut state: ResMut<DialogueState>,
    mut root: Query<&mut Visibility, With<DialogueRoot>>,
    panel: Query<Entity, With<DialoguePanel>>,
) {
    if !state.dirty && !inventory.is_changed() {
        return;
    }
    state.dirty = false;
    let Ok(mut vis) = root.single_mut() else {
        return;
    };
    let Ok(panel) = panel.single() else {
        return;
    };
    let Some(dlg) = state.open.as_mut() else {
        *vis = Visibility::Hidden;
        return;
    };
    *vis = Visibility::Visible;

    // Clamp cursors against the live lists (a sale shrinks the sell column).
    let sell_slots = occupied_slots(&inventory);
    dlg.buy_i = dlg.buy_i.min(dlg.data.stock.len().saturating_sub(1));
    dlg.sell_i = dlg.sell_i.min(sell_slots.len().saturating_sub(1));

    let credits = run.iter().next().map(|r| r.credits).unwrap_or(0);
    let d = &*dlg;

    commands.entity(panel).despawn_children();
    commands.entity(panel).with_children(|panel| {
        // Header: name left, wallet right.
        panel
            .spawn(Node {
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            })
            .with_children(|row| {
                row.spawn(theme::heading(&d.data.npc_name));
                let (t, f, _) = theme::label(&format!("credits {credits}"));
                row.spawn((t, f, TextColor(theme::WARN)));
            });

        // ASCII quotes: curly ones aren't in the default font subset (tofu).
        let line = if d.rumor_shown { &d.data.rumor } else { &d.data.greeting };
        panel.spawn(theme::body(&format!("\"{line}\"")));

        match d.mode {
            Mode::Options => {
                panel.spawn(Node { height: Val::Px(6.0), ..default() });
                for (key, label) in [("1", "Trade"), ("2", "Ask around"), ("Esc", "Leave")] {
                    panel
                        .spawn(Node { column_gap: Val::Px(8.0), ..default() })
                        .with_children(|row| {
                            let (t, f, _) = theme::body(&format!("[{key}]"));
                            row.spawn((t, f, TextColor(theme::ACCENT)));
                            row.spawn(theme::body(label));
                        });
                }
            }
            Mode::Trade => {
                panel
                    .spawn(Node {
                        column_gap: Val::Px(18.0),
                        align_items: AlignItems::FlexStart,
                        ..default()
                    })
                    .with_children(|cols| {
                        // BUY column — the NPC's stock.
                        cols.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(3.0),
                            flex_grow: 1.0,
                            ..default()
                        })
                        .with_children(|col| {
                            col.spawn(theme::heading("BUY"));
                            for (i, entry) in d.data.stock.iter().enumerate() {
                                let active = d.col == Col::Buy && i == d.buy_i;
                                trade_row(col, active, entry.item_id, &entry.name, entry.cost, credits >= entry.cost);
                            }
                        });
                        // SELL column — your inventory.
                        cols.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(3.0),
                            flex_grow: 1.0,
                            ..default()
                        })
                        .with_children(|col| {
                            col.spawn(theme::heading("SELL"));
                            if sell_slots.is_empty() {
                                col.spawn(theme::label("(nothing to sell)"));
                            }
                            for (i, slot) in sell_slots.iter().enumerate() {
                                let Some(item) = inventory.slots[*slot].as_ref() else {
                                    continue;
                                };
                                let active = d.col == Col::Sell && i == d.sell_i;
                                trade_row(col, active, item.id, &item.name, items::sell_price(item), true);
                            }
                        });
                    });
                panel.spawn(theme::label(
                    "\u{2190}\u{2192} column   \u{2191}\u{2193} select   Enter confirm   Esc back",
                ));
            }
        }
    });
}

/// One buy/sell line: colored item tag, name, price; cyan `>` cursor when
/// active, dimmed price when unaffordable.
fn trade_row(
    col: &mut ChildSpawnerCommands,
    active: bool,
    item_id: u32,
    name: &str,
    price: u32,
    affordable: bool,
) {
    let (tag, tag_color) = theme::item_style(item_id);
    col.spawn(Node {
        column_gap: Val::Px(6.0),
        ..default()
    })
    .with_children(|row| {
        let (t, f, _) = theme::body(if active { ">" } else { " " });
        row.spawn((t, f, TextColor(theme::ACCENT)));
        let (t, f, _) = theme::body(tag);
        row.spawn((t, f, TextColor(tag_color)));
        let (t, f, c) = theme::body(name);
        row.spawn((
            t,
            f,
            if active { TextColor(theme::ACCENT) } else { c },
        ));
        let (t, f, _) = theme::body(&format!("{price}c"));
        row.spawn((
            t,
            f,
            TextColor(if affordable { theme::WARN } else { theme::TEXT_DIM }),
        ));
    });
}

/// "[E] talk" when an alive player faces a nearby NPC and no window is open.
fn sync_talk_prompt(
    state: Res<DialogueState>,
    npcs: Query<&Transform, With<shared::protocol::Npc>>,
    player: Query<(&Transform, Option<&PlayerAlive>), With<OwnPlayer>>,
    camera: Query<&Transform, (With<crate::fly_camera::FlyCamera>, Without<OwnPlayer>)>,
    mut prompt: Query<&mut Visibility, With<TalkPrompt>>,
) {
    let Ok(mut vis) = prompt.single_mut() else {
        return;
    };
    let show = state.open.is_none()
        && player.single().is_ok_and(|(ptf, alive)| {
            alive.is_none_or(|a| a.0)
                && camera.single().is_ok_and(|cam| {
                    let fwd = *cam.forward();
                    npcs.iter().any(|npc| {
                        let to = npc.translation - ptf.translation;
                        to.length() < PROMPT_RANGE
                            && to.normalize_or_zero().dot(fwd) > 0.92
                    })
                })
        });
    *vis = if show { Visibility::Visible } else { Visibility::Hidden };
}
