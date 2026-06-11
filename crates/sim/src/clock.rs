//! Simulation time. One tick = one sim minute; the wall-clock pacing of
//! ticks is decided by the runner loop, never by game logic, so the same
//! run produces identical results at any speed.

use bevy::prelude::*;

/// Sim minutes per day (24h x 60m).
pub const MINUTES_PER_DAY: u64 = 24 * 60;

#[derive(Resource, Default)]
pub struct SimClock {
    /// Ticks elapsed since sim start. Tick 0 = day 1, 00:00.
    pub tick: u64,
}

impl SimClock {
    /// 1-based day number.
    pub fn day(&self) -> u64 {
        self.tick / MINUTES_PER_DAY + 1
    }

    pub fn minute_of_day(&self) -> u64 {
        self.tick % MINUTES_PER_DAY
    }

    pub fn hour(&self) -> u64 {
        self.minute_of_day() / 60
    }

    pub fn minute(&self) -> u64 {
        self.minute_of_day() % 60
    }

    /// Human-readable timestamp, e.g. "d003 14:25".
    pub fn stamp(&self) -> String {
        format!("d{:03} {:02}:{:02}", self.day(), self.hour(), self.minute())
    }
}

pub fn advance_clock(mut clock: ResMut<SimClock>) {
    clock.tick += 1;
}
