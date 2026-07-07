//! Friendly-NPC interactions: E-to-talk dialogue and buy/sell trading.
//!
//! The server holds NO dialogue session state. Pressing E on an NPC sends the
//! owning client one `NpcDialogue` message (name, greeting, rumor, stock);
//! trade actions come back as `PlayerInput::trade_buy` / `trade_sell` and are
//! validated here against live NPC proximity, so a stale or forged action
//! after walking away simply does nothing.

use avian3d::{math::AdjustPrecision, prelude::*};
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use shared::config;
use shared::items::{self, FLASHLIGHT, MAP, MEDICAL_BAG, PIPE_BAT, SCRAP_PISTOL};
use shared::protocol::{Npc, NpcDialogue, Player, PlayerAlive, PlayerName, ShopEntry};

use crate::character::CharacterSystems;
use crate::items::Inventory;
use crate::players::{LatestInput, PlayerOwner};
use crate::run::RunEntity;

/// A trade action is honored while the player stands this close to any NPC
/// (a bit more than INTERACT_RANGE so stepping back mid-menu doesn't void it).
pub const NPC_TRADE_RANGE: f32 = 4.5;

pub struct ServerNpcPlugin;

impl Plugin for ServerNpcPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (npc_interact, npc_trade)
                .chain()
                .after(CharacterSystems)
                // npc_interact must see `interact` BEFORE pickup_items
                // consumes it (it only eats the press when the ray hits an NPC).
                .before(crate::items::pickup_items)
                .run_if(in_state(ClientState::Disconnected)),
        );
    }
}

/// Stable flavor identity, rolled once at spawn (wandering NPCs must not
/// change name between conversations).
#[derive(Component)]
pub struct NpcProfile {
    pub name: &'static str,
    pub greeting: &'static str,
    pub rumor: &'static str,
}

const NAMES: [&str; 10] = [
    "Vex", "Mara", "Old Serj", "Tunnel Kate", "Brick", "Whisper", "Doc Halden", "Rusty",
    "The Ledger", "Pip",
];

const GREETINGS: [&str; 6] = [
    "Looking to trade, or just passing through?",
    "Keep your voice down. The walls listen.",
    "Fresh from the tunnels? You look it.",
    "Credits or scrap, I don't judge.",
    "Don't touch the pallet. Ask first.",
    "Another runner. The sewers keep spitting you out.",
];

const RUMORS: [&str; 7] = [
    "Some doors down here aren't on any map. Walk the walls if you're curious.",
    "Patrols hear a sprint from across the hall. Crouch and they're half deaf.",
    "Lost a friend to a locked gate once. It opened for the thing chasing him.",
    "The machines further down still run. Nobody remembers switching them on.",
    "There's a whole floor below this one, they say. And another below that.",
    "Buy the map. Everyone thinks they don't need the map.",
    "Extraction pays for what you carry OUT, not for what you shot on the way.",
];

/// Deterministic profile for the `idx`-th NPC of a layout: the first ten get
/// distinct names, greeting/rumor mix in the spawn position for variety
/// across maps.
pub fn profile_for(idx: usize, pos: Vec3) -> NpcProfile {
    let h = (pos.x * 73.0).abs() as usize + (pos.z * 131.0).abs() as usize;
    NpcProfile {
        name: NAMES[idx % NAMES.len()],
        greeting: GREETINGS[(idx + h) % GREETINGS.len()],
        rumor: RUMORS[(idx * 3 + h / 5) % RUMORS.len()],
    }
}

/// What every trade NPC sells (id, cost, display name). Flat catalog v1;
/// per-faction stock differentiation is later material.
fn npc_stock() -> [(u32, u32, &'static str); 5] {
    [
        (PIPE_BAT, 10, "Pipe Bat"),
        (FLASHLIGHT, 12, "Flashlight"),
        (SCRAP_PISTOL, 45, "Scrap Pistol"),
        (MEDICAL_BAG, 30, "Medical Bag"),
        (MAP, 20, "Sector Map"),
    ]
}

fn stock_item(id: u32) -> Option<shared::protocol::Item> {
    Some(match id {
        FLASHLIGHT => items::flashlight(),
        MAP => items::map(),
        PIPE_BAT => items::pipe_bat(),
        SCRAP_PISTOL => items::scrap_pistol(),
        MEDICAL_BAG => items::medical_bag(),
        _ => return None,
    })
}

/// E on an NPC in view → open the dialogue on that client. Consumes the
/// interact press only when the ray actually lands on an NPC, so item pickup
/// (which runs after) keeps working everywhere else.
fn npc_interact(
    spatial: SpatialQuery,
    colliders: Query<&ColliderOf>,
    npcs: Query<(&Transform, Option<&NpcProfile>), With<Npc>>,
    mut players: Query<
        (Entity, &Transform, &mut LatestInput, &PlayerOwner, &PlayerAlive),
        With<Player>,
    >,
    mut writer: MessageWriter<ToClients<NpcDialogue>>,
) {
    for (player, transform, mut input, owner, alive) in &mut players {
        if !input.0.interact || !alive.0 {
            continue;
        }
        let eye = transform.translation + Vec3::Y * config::PLAYER_EYE_HEIGHT;
        let dir = Dir3::new(look_direction(input.0.yaw, input.0.pitch)).unwrap_or(Dir3::NEG_Z);
        let Some(hit) = spatial.cast_ray(
            eye.adjust_precision(),
            dir,
            config::INTERACT_RANGE,
            true,
            &SpatialQueryFilter::from_excluded_entities([player]),
        ) else {
            continue;
        };
        let body = colliders.get(hit.entity).map(|c| c.body).unwrap_or(hit.entity);
        let Ok((npc_tf, profile)) = npcs.get(body) else {
            continue;
        };
        input.0.interact = false;

        let fallback = profile_for(0, npc_tf.translation);
        let p = profile.unwrap_or(&fallback);
        writer.write(ToClients {
            targets: SendTargets::Single(owner.0),
            message: NpcDialogue {
                npc_pos: npc_tf.translation,
                npc_name: p.name.to_string(),
                greeting: p.greeting.to_string(),
                rumor: p.rumor.to_string(),
                stock: npc_stock()
                    .iter()
                    .map(|(id, cost, name)| ShopEntry {
                        item_id: *id,
                        name: (*name).to_string(),
                        cost: *cost,
                    })
                    .collect(),
            },
        });
    }
}

/// Apply buy/sell requests from players standing near an NPC. Credits are the
/// party wallet on `RunState` (same as the hub shop and credit pickups).
fn npc_trade(
    npcs: Query<&Transform, With<Npc>>,
    mut run_q: Query<&mut shared::run::RunState, With<RunEntity>>,
    mut players: Query<
        (&Transform, &mut LatestInput, &mut Inventory, &PlayerOwner, &PlayerName, &PlayerAlive),
        With<Player>,
    >,
    mut writer: MessageWriter<ToClients<shared::protocol::InventoryUpdate>>,
) {
    let Ok(mut run) = run_q.single_mut() else {
        return;
    };
    for (transform, mut input, mut inventory, owner, name, alive) in &mut players {
        let buy = input.0.trade_buy.take();
        let sell = input.0.trade_sell.take();
        if (buy.is_none() && sell.is_none()) || !alive.0 {
            continue;
        }
        let near_npc = npcs
            .iter()
            .any(|npc| npc.translation.distance(transform.translation) < NPC_TRADE_RANGE);
        if !near_npc {
            continue;
        }
        let mut changed = false;

        if let Some(idx) = buy {
            let stock = npc_stock();
            if let Some((id, cost, label)) = stock.get(idx as usize) {
                let map_taken = *id == MAP && run.map_holder.is_some();
                let free = inventory.0.iter().position(Option::is_none);
                if run.credits < *cost {
                    info!("trade: not enough credits for {label}");
                } else if map_taken {
                    info!("trade: someone already carries the map");
                } else if let (Some(slot), Some(item)) = (free, stock_item(*id)) {
                    if *id == MAP {
                        run.map_holder = Some(name.0.clone());
                    }
                    run.credits -= *cost;
                    inventory.0[slot] = Some(item);
                    changed = true;
                    info!("trade: {} bought {label} for {cost}c", name.0);
                }
            }
        }

        if let Some(slot) = sell {
            if let Some(item) = inventory
                .0
                .get_mut(slot as usize)
                .and_then(Option::take)
            {
                if items::is_map(&item) && run.map_holder.as_deref() == Some(name.0.as_str()) {
                    run.map_holder = None;
                }
                let price = items::sell_price(&item);
                run.credits += price;
                changed = true;
                info!("trade: {} sold {} for {price}c", name.0, item.name);
            }
        }

        if changed {
            writer.write(ToClients {
                targets: SendTargets::Single(owner.0),
                message: shared::protocol::InventoryUpdate {
                    slots: inventory.0.clone(),
                },
            });
        }
    }
}

fn look_direction(yaw: f32, pitch: f32) -> Vec3 {
    Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0) * -Vec3::Z
}
