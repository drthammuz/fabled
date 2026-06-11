//! Village-level state: per-day statistics and the daily summary events —
//! the main "how is the village doing" signals in the log.

use bevy::prelude::*;

use crate::brain::Emotions;
use crate::clock::SimClock;
use crate::economy::Ledger;
use crate::events::{EventLog, SimEvent};
use crate::housing::Home;
use crate::npc::{Health, Needs, Npc};
use crate::professions::Market;

/// Experiment toggle (console: `blight on|off`): while active, farming and
/// fishing yield nothing — the way to watch a famine play out.
#[derive(Resource, Default)]
pub struct ForageBlight(pub bool);

/// Counters reset at each day boundary, reported in the daily summary.
/// The `total_*` fields are lifetime counters (never reset) used by the
/// evolution fitness function.
#[derive(Resource, Default)]
pub struct DailyStats {
    pub meals: u32,
    pub suppers: u32,
    pub ales_sold: u32,
    pub grain_grown: u32,
    pub fish_caught: u32,
    pub bread_baked: u32,
    pub total_meals: u64,
    pub total_ales: u64,
    pub total_starving_episodes: u64,
}

fn round1(value: f32) -> f32 {
    (value * 10.0).round() / 10.0
}

/// At every day boundary, log a summary of the day that just ended plus a
/// wealth report (every purse and the treasury).
pub fn daily_summary(
    clock: Res<SimClock>,
    market: Res<Market>,
    ledger: Res<Ledger>,
    mut stats: ResMut<DailyStats>,
    mut log: ResMut<EventLog>,
    npcs: Query<(&Needs, &Health, &Emotions, &Home), With<Npc>>,
) {
    if clock.minute_of_day() != 0 {
        return;
    }
    let alive = npcs.iter().count();
    let n = alive.max(1) as f32;
    let (mut hunger, mut energy, mut warmth, mut health) = (0.0, 0.0, 0.0, 0.0);
    let (mut mood, mut social, mut house_quality) = (0.0, 0.0, 0.0);
    for (needs, hp, emotions, home) in &npcs {
        hunger += needs.hunger;
        energy += needs.energy;
        warmth += needs.warmth;
        health += hp.0;
        mood += emotions.mood;
        social += needs.social;
        house_quality += home.score.quality;
    }
    log.log(
        &clock,
        &SimEvent::DailySummary {
            day: clock.day() - 1,
            alive,
            meals: stats.meals,
            suppers: stats.suppers,
            ales_sold: stats.ales_sold,
            grain_grown: stats.grain_grown,
            fish_caught: stats.fish_caught,
            bread_baked: stats.bread_baked,
            farm_grain: market.farm_grain,
            bakery_bread: market.bakery_bread,
            stall_fish: market.stall_fish,
            avg_hunger: round1(hunger / n),
            avg_energy: round1(energy / n),
            avg_warmth: round1(warmth / n),
            avg_health: round1(health / n),
            avg_mood: round1(mood / n),
            avg_social: round1(social / n),
            avg_house_quality: (house_quality / n * 100.0).round() / 100.0,
        },
    );
    log.log(
        &clock,
        &SimEvent::WealthSummary {
            day: clock.day() - 1,
            treasury: ledger.treasury,
            purses: ledger.accounts.clone(),
        },
    );
    log.flush();
    *stats = DailyStats {
        total_meals: stats.total_meals,
        total_ales: stats.total_ales,
        total_starving_episodes: stats.total_starving_episodes,
        ..Default::default()
    };
}
