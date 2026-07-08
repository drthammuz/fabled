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
use shared::classes::ClassKind;
use shared::items::{self, FLASHLIGHT, MAP, MEDICAL_BAG, PIPE_BAT, SCRAP_PISTOL};
use shared::protocol::{Npc, NpcDialogue, Player, PlayerAlive, PlayerClass, PlayerName, ShopEntry};

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
            (npc_interact, npc_trade, tech_terminal_hack)
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
    /// Hidden-room keeper: a reclusive vendor behind a secret door. Sells only
    /// bandages and trades in deeper lore instead of street rumors.
    pub keeper: bool,
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
        keeper: false,
    }
}

/// Hidden-room keepers: hermits who found the secret rooms first and stayed.
const KEEPER_NAMES: [&str; 6] = [
    "The Hermit", "Old Wick", "Sister Ash", "The Cartographer", "Grim", "Mother Kell",
];

const KEEPER_GREETINGS: [&str; 4] = [
    "You found the door. Few do. Sit — but keep your hands where I can see them.",
    "A visitor. It's been... I've stopped counting. What do you need?",
    "Quiet, out there. Quieter in here. That's how I like it.",
    "You've got the look of someone still counting on getting out. Cute.",
];

/// Deeper lore, revealed via "Ask around". Keepers know the old story.
const KEEPER_LORE: [&str; 6] = [
    "This whole sector was a shelter once. Then the shelter needed shelter from what it kept.",
    "The factions up top are children fighting over a corpse's pockets. The body's still warm below.",
    "Every hidden room connects, if you know the old maintenance codes. I've walked the whole ring.",
    "The machines didn't wake. They were never asleep. We just stopped being worth their attention.",
    "There's a floor they sealed with people still on it. On some nights, the seals still knock.",
    "Extraction isn't up. Never was. They just point you at the light so you stop digging.",
];

pub fn keeper_profile(idx: usize, pos: Vec3) -> NpcProfile {
    let h = (pos.x * 51.0).abs() as usize + (pos.z * 89.0).abs() as usize;
    NpcProfile {
        name: KEEPER_NAMES[idx % KEEPER_NAMES.len()],
        greeting: KEEPER_GREETINGS[(idx + h) % KEEPER_GREETINGS.len()],
        rumor: KEEPER_LORE[(idx + h / 3) % KEEPER_LORE.len()],
        keeper: true,
    }
}

/// Vendor stock (id, cost, display name) for an NPC. Hub traders carry the flat
/// gear catalog; hidden-room keepers deal only in bandages (their real value is
/// lore, not loot).
fn stock_for(keeper: bool) -> Vec<(u32, u32, &'static str)> {
    if keeper {
        vec![(items::BANDAGE, 6, "Bandage")]
    } else {
        vec![
            (PIPE_BAT, 10, "Pipe Bat"),
            (FLASHLIGHT, 12, "Flashlight"),
            (SCRAP_PISTOL, 45, "Scrap Pistol"),
            (MEDICAL_BAG, 30, "Medical Bag"),
            (items::ARMOR, 60, "Body Armor"),
            (MAP, 20, "Sector Map"),
        ]
    }
}

fn stock_item(id: u32) -> Option<shared::protocol::Item> {
    items::by_id(id)
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
                stock: stock_for(p.keeper)
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
    npcs: Query<(&Transform, &NpcProfile), With<Npc>>,
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
        // Validate against the NEAREST in-range NPC's own stock, so a buy index
        // resolves against exactly what that NPC's dialogue showed (keeper vs hub).
        let nearest = npcs
            .iter()
            .map(|(npc, prof)| (npc.translation.distance(transform.translation), prof))
            .filter(|(d, _)| *d < NPC_TRADE_RANGE)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let Some((_, prof)) = nearest else {
            continue;
        };
        let mut changed = false;

        if let Some(idx) = buy {
            let stock = stock_for(prof.keeper);
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

/// Per-Tech cooldown so `hack` can't be spammed for infinite credits.
#[derive(Component, Default)]
pub struct HackCooldown(pub f32);

const HACK_REWARD_CREDITS: u32 = 40;
const HACK_COOLDOWN_SECS: f32 = 20.0;

/// Tech-only terminal perk: running `hack` in a terminal grants party credits
/// (and a Data Shard to sell) on a cooldown. Purely for the "Tech feels useful"
/// loop — no real hacking logic yet. Non-Tech requests are ignored.
fn tech_terminal_hack(
    time: Res<Time>,
    mut run_q: Query<&mut shared::run::RunState, With<RunEntity>>,
    mut players: Query<
        (
            &mut LatestInput,
            &PlayerClass,
            &mut Inventory,
            &PlayerOwner,
            &mut HackCooldown,
            &PlayerAlive,
        ),
        With<Player>,
    >,
    mut inv_writer: MessageWriter<ToClients<shared::protocol::InventoryUpdate>>,
) {
    let Ok(mut run) = run_q.single_mut() else {
        return;
    };
    let dt = time.delta_secs();
    for (mut input, class, mut inv, owner, mut cd, alive) in &mut players {
        cd.0 = (cd.0 - dt).max(0.0);
        let requested = std::mem::take(&mut input.0.terminal_hack);
        if !requested || !alive.0 || class.0 != ClassKind::Tech || cd.0 > 0.0 {
            continue;
        }
        cd.0 = HACK_COOLDOWN_SECS;
        run.credits += HACK_REWARD_CREDITS;
        // Drop a Data Shard (sellable) if there's a free slot.
        if let Some(slot) = inv.0.iter().position(Option::is_none) {
            inv.0[slot] = Some(items::data_shard());
            inv_writer.write(ToClients {
                targets: SendTargets::Single(owner.0),
                message: shared::protocol::InventoryUpdate { slots: inv.0.clone() },
            });
        }
        info!("tech hack: +{HACK_REWARD_CREDITS}c + data shard");
    }
}

fn look_direction(yaw: f32, pitch: f32) -> Vec3 {
    Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0) * -Vec3::Z
}
