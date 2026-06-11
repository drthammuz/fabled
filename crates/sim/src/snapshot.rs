//! Full-state snapshots. V0 state is tiny (clock + weather); every later
//! milestone extends this with its own state so a stopped sim can always
//! be inspected in full.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde::Serialize;

use std::collections::BTreeMap;

use crate::brain::{Emotions, Memories, Memory, Traits};
use crate::clock::SimClock;
use crate::economy::Ledger;
use crate::events::{EventLog, SimEvent};
use crate::housing::{Home, HouseScore};
use crate::npc::{Activity, Health, Needs, Npc, Pos};
use crate::professions::{Market, Profession};
use crate::village::ForageBlight;
use crate::weather::Weather;
use crate::SimMeta;

#[derive(Serialize)]
struct NpcSnapshot {
    name: String,
    profession: &'static str,
    purse: i64,
    hunger: f32,
    energy: f32,
    warmth: f32,
    social: f32,
    mood: f32,
    health: f32,
    activity: &'static str,
    place: &'static str,
    pos: [f32; 2],
    house: HouseScore,
    traits: Traits,
    /// Strongest memories, most salient first.
    memories: Vec<Memory>,
}

#[derive(Serialize)]
struct MarketSnapshot {
    farm_grain: u32,
    bakery_grain: u32,
    bakery_bread: u32,
    stall_fish: u32,
    tavern_bread: u32,
    tavern_fish: u32,
}

#[derive(Serialize)]
struct Snapshot<'a> {
    village: &'a str,
    seed: u64,
    tick: u64,
    time: String,
    day: u64,
    weather: &'a str,
    temp_c: i32,
    blight: bool,
    treasury: i64,
    total_money: i64,
    market: MarketSnapshot,
    events_logged: u64,
    npcs: Vec<NpcSnapshot>,
    /// Estates of the dead live in the treasury; purses of the living.
    purses: BTreeMap<String, i64>,
}

/// Writes a snapshot JSON next to the event log and logs the write itself.
pub fn write_snapshot(world: &mut World, out_dir: &Path) -> std::io::Result<PathBuf> {
    let ledger_balances: BTreeMap<String, i64> =
        world.resource::<Ledger>().accounts.clone();
    let npcs: Vec<NpcSnapshot> = world
        .query::<(
            &Npc,
            &Profession,
            &Needs,
            &Health,
            &Activity,
            &Traits,
            &Emotions,
            &Memories,
            &Pos,
            &Home,
        )>()
        .iter(world)
        .map(
            |(npc, profession, needs, health, activity, traits, emotions, memories, pos, home)| {
                let mut strongest: Vec<Memory> = memories.0.clone();
                strongest.sort_by(|a, b| b.salience.total_cmp(&a.salience));
                strongest.truncate(5);
                NpcSnapshot {
                    name: npc.name.clone(),
                    profession: profession.name(),
                    purse: ledger_balances.get(&npc.name).copied().unwrap_or(0),
                    hunger: needs.hunger,
                    energy: needs.energy,
                    warmth: needs.warmth,
                    social: needs.social,
                    mood: emotions.mood,
                    health: health.0,
                    activity: activity.kind.name(),
                    place: activity.place.name(),
                    pos: [pos.0.x, pos.0.y],
                    house: home.score,
                    traits: *traits,
                    memories: strongest,
                }
            },
        )
        .collect();

    let meta = world.resource::<SimMeta>();
    let clock = world.resource::<SimClock>();
    let weather = world.resource::<Weather>();
    let log = world.resource::<EventLog>();
    let ledger = world.resource::<Ledger>();
    let market = world.resource::<Market>();

    let snapshot = Snapshot {
        village: &meta.village,
        seed: meta.seed,
        tick: clock.tick,
        time: clock.stamp(),
        day: clock.day(),
        weather: &weather.kind,
        temp_c: weather.temp_c,
        blight: world.resource::<ForageBlight>().0,
        treasury: ledger.treasury,
        total_money: ledger.total(),
        market: MarketSnapshot {
            farm_grain: market.farm_grain,
            bakery_grain: market.bakery_grain,
            bakery_bread: market.bakery_bread,
            stall_fish: market.stall_fish,
            tavern_bread: market.tavern_bread,
            tavern_fish: market.tavern_fish,
        },
        events_logged: log.count,
        npcs,
        purses: ledger_balances,
    };
    let path = out_dir.join(format!(
        "{}-seed{}-tick{:08}.snapshot.json",
        meta.village, meta.seed, clock.tick
    ));
    std::fs::write(&path, serde_json::to_string_pretty(&snapshot)?)?;

    let event = SimEvent::SnapshotWritten {
        path: path.display().to_string(),
    };
    world.resource_scope(|world, mut log: Mut<EventLog>| {
        log.log(world.resource::<SimClock>(), &event);
        log.flush();
    });
    Ok(path)
}
