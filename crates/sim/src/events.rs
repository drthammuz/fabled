//! Structured event log: one JSON object per line (JSONL). Events contain
//! only sim-deterministic fields (no wall-clock timestamps), so two runs
//! with the same seed produce byte-identical logs — that property is the
//! determinism test.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use bevy::prelude::*;
use serde::Serialize;

use crate::clock::SimClock;

#[derive(Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum SimEvent {
    SimStarted {
        village: String,
        seed: u64,
    },
    DayStarted {
        day: u64,
        weather: String,
        temp_c: i32,
    },
    SnapshotWritten {
        path: String,
    },
    SimStopped {
        reason: String,
        days_elapsed: u64,
    },
    NpcSpawned {
        npc: String,
        profession: String,
        traits: crate::brain::Traits,
    },
    /// A visible emotional/state expression — the hook a future LLM speech
    /// layer renders into words. Logged once per episode.
    Expression {
        npc: String,
        what: String,
    },
    MemoryFormed {
        npc: String,
        kind: String,
        about: Option<String>,
        valence: f32,
    },
    ActionStarted {
        npc: String,
        action: String,
        place: String,
        /// Walking time to get there (sim minutes).
        travel_min: u64,
    },
    HouseImproved {
        npc: String,
        quality_from: f32,
        quality_to: f32,
        shelter: f32,
        navigability: f32,
        cost: i64,
    },
    NpcStarving {
        npc: String,
    },
    NpcFreezing {
        npc: String,
    },
    NpcDied {
        npc: String,
        cause: String,
        /// Coins passed to the treasury.
        estate: i64,
    },
    DailySummary {
        day: u64,
        alive: usize,
        meals: u32,
        suppers: u32,
        ales_sold: u32,
        grain_grown: u32,
        fish_caught: u32,
        bread_baked: u32,
        farm_grain: u32,
        bakery_bread: u32,
        stall_fish: u32,
        avg_hunger: f32,
        avg_energy: f32,
        avg_warmth: f32,
        avg_health: f32,
        avg_mood: f32,
        avg_social: f32,
        avg_house_quality: f32,
    },
    WealthSummary {
        day: u64,
        treasury: i64,
        purses: std::collections::BTreeMap<String, i64>,
    },
    // --- Economy events ---
    Purchase {
        buyer: String,
        seller: String,
        good: String,
        price: i64,
    },
    WagePaid {
        from: String,
        to: String,
        amount: i64,
    },
    UnpaidWage {
        from: String,
        to: String,
    },
    SalaryPaid {
        npc: String,
        amount: i64,
    },
    UnpaidSalary {
        npc: String,
    },
    UnpaidTax {
        npc: String,
        owed: i64,
    },
    /// Hungry but no stocked, affordable meal exists — economic distress.
    CannotAffordMeal {
        npc: String,
        purse: i64,
    },
    /// Should never appear: the sum of all money changed. A bug, not lore.
    MoneyConservationViolated {
        expected: i64,
        actual: i64,
    },
    /// Operator intervention via console, kept in the log so a run's
    /// history stays complete.
    BlightSet {
        active: bool,
    },
}

#[derive(Serialize)]
struct EventRecord<'a> {
    tick: u64,
    time: String,
    #[serde(flatten)]
    event: &'a SimEvent,
}

#[derive(Resource)]
pub struct EventLog {
    /// None = counting only (evolution runs spin up dozens of villages and
    /// don't want dozens of log files).
    writer: Option<BufWriter<File>>,
    pub path: PathBuf,
    pub count: u64,
}

impl EventLog {
    pub fn create(path: PathBuf) -> std::io::Result<Self> {
        let file = File::create(&path)?;
        Ok(Self {
            writer: Some(BufWriter::new(file)),
            path,
            count: 0,
        })
    }

    pub fn disabled() -> Self {
        Self {
            writer: None,
            path: PathBuf::new(),
            count: 0,
        }
    }

    pub fn log(&mut self, clock: &SimClock, event: &SimEvent) {
        self.count += 1;
        let Some(writer) = &mut self.writer else {
            return;
        };
        let record = EventRecord {
            tick: clock.tick,
            time: clock.stamp(),
            event,
        };
        // Single-line JSON + newline = JSONL.
        if let Ok(line) = serde_json::to_string(&record) {
            let _ = writeln!(writer, "{line}");
        }
    }

    pub fn flush(&mut self) {
        if let Some(writer) = &mut self.writer {
            let _ = writer.flush();
        }
    }
}
