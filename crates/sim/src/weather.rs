//! Daily weather — the first consumer of the seeded RNG. In V0 its job is
//! to prove determinism (same seed = same weather sequence) and give the
//! event log a heartbeat; later it feeds building scores (wind/sun cover).

use bevy::prelude::*;
use rand::Rng;
use rand_chacha::ChaCha8Rng;

use crate::clock::SimClock;
use crate::events::{EventLog, SimEvent};

/// The single deterministic RNG for all sim logic. Systems must never use
/// any other randomness source, or reproducibility is lost.
#[derive(Resource)]
pub struct SimRng(pub ChaCha8Rng);

#[derive(Resource, Default)]
pub struct Weather {
    pub kind: String,
    pub temp_c: i32,
}

const KINDS: [&str; 5] = ["clear", "overcast", "rain", "windy", "fog"];

pub fn roll_weather(rng: &mut SimRng, weather: &mut Weather) {
    weather.kind = KINDS[rng.0.random_range(0..KINDS.len())].to_string();
    weather.temp_c = rng.0.random_range(-5..28);
}

/// Fires at every day boundary (00:00). Day 1 is rolled at startup instead.
pub fn day_start(
    clock: Res<SimClock>,
    mut rng: ResMut<SimRng>,
    mut weather: ResMut<Weather>,
    mut log: ResMut<EventLog>,
) {
    if clock.minute_of_day() != 0 {
        return;
    }
    roll_weather(&mut rng, &mut weather);
    log.log(
        &clock,
        &SimEvent::DayStarted {
            day: clock.day(),
            weather: weather.kind.clone(),
            temp_c: weather.temp_c,
        },
    );
    // A day boundary is a natural flush point: logs stay readable while
    // the sim runs without paying a syscall per event.
    log.flush();
}
