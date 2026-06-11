//! Headless village simulation (V0 skeleton).
//!
//! Architecture: a bare Bevy `App` where **one `app.update()` = one sim
//! tick = one sim minute**. Wall-clock pacing lives in the runner loop
//! outside the ECS, so logic is identical at 60 ticks/sec or uncapped.
//! Everything observable goes through the JSONL event log and snapshots;
//! the stdin console gives orderly control (pause/speed/snapshot/stop).

pub mod brain;
pub mod clock;
pub mod control;
pub mod economy;
pub mod events;
pub mod housing;
pub mod npc;
pub mod params;
pub mod professions;
pub mod snapshot;
pub mod village;
pub mod weather;
pub mod world_map;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use clock::SimClock;
use control::Command;
use events::{EventLog, SimEvent};
use weather::{SimRng, Weather};

pub struct SimConfig {
    pub village: String,
    pub seed: u64,
    /// Initial pacing in ticks/second; 0 = as fast as possible.
    pub speed_tps: f64,
    /// Stop automatically after this many full sim days; 0 = run until `stop`.
    pub stop_after_days: u64,
    pub out_dir: PathBuf,
}

/// Identity of this run, used in snapshots and file names.
#[derive(Resource)]
pub struct SimMeta {
    pub village: String,
    pub seed: u64,
}

pub fn run(config: SimConfig) -> std::io::Result<()> {
    std::fs::create_dir_all(&config.out_dir)?;
    let wall_stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let log_path = config.out_dir.join(format!(
        "{}-seed{}-{}.jsonl",
        config.village, config.seed, wall_stamp
    ));
    let log = EventLog::create(log_path.clone())?;

    let mut app = App::new();
    app.insert_resource(SimMeta {
        village: config.village.clone(),
        seed: config.seed,
    })
    .insert_resource(SimClock::default())
    .insert_resource(SimRng(ChaCha8Rng::seed_from_u64(config.seed)))
    .insert_resource(Weather::default())
    .insert_resource(village::ForageBlight::default())
    .insert_resource(village::DailyStats::default())
    .insert_resource(economy::Ledger::default())
    .insert_resource(professions::Market::default())
    .insert_resource(professions::Roster::default())
    .insert_resource(log)
    .add_systems(Startup, npc::spawn_npcs)
    .add_systems(
        Update,
        (
            clock::advance_clock,
            // Summary first so "day N summary" precedes "day N+1 started".
            village::daily_summary,
            weather::day_start,
            brain::decay_memories,
            economy::fiscal_day,
            professions::tavern_restock,
            housing::improve_homes,
            npc::needs_tick,
            brain::emotions_tick,
            npc::act,
            economy::conservation_check,
        )
            .chain(),
    );

    // Day 1 begins at startup; later day boundaries come from `day_start`.
    {
        let world = app.world_mut();
        world.resource_scope(|world, mut log: Mut<EventLog>| {
            let clock = world.resource::<SimClock>();
            log.log(
                clock,
                &SimEvent::SimStarted {
                    village: config.village.clone(),
                    seed: config.seed,
                },
            );
        });
        world.resource_scope(|world, mut rng: Mut<SimRng>| {
            world.resource_scope(|world, mut weather: Mut<Weather>| {
                weather::roll_weather(&mut rng, &mut weather);
                world.resource_scope(|world, mut log: Mut<EventLog>| {
                    let clock = world.resource::<SimClock>();
                    log.log(
                        clock,
                        &SimEvent::DayStarted {
                            day: clock.day(),
                            weather: weather.kind.clone(),
                            temp_c: weather.temp_c,
                        },
                    );
                    log.flush();
                });
            });
        });
    }

    println!(
        "sim '{}' seed {} | 1 tick = 1 sim minute | log: {}",
        config.village,
        config.seed,
        log_path.display()
    );
    println!("{}", control::HELP);

    let commands = control::spawn_console();
    let mut paused = false;
    let mut speed_tps = config.speed_tps;
    let started = Instant::now();
    let mut next_tick = Instant::now();
    // Rolling window for "achieved tps" in status output.
    let mut window_ticks: u64 = 0;
    let mut window_start = Instant::now();

    let stop_reason: String = 'main: loop {
        while let Ok(command) = commands.try_recv() {
            match command {
                Command::Pause => {
                    paused = true;
                    println!("[paused] {}", app.world().resource::<SimClock>().stamp());
                }
                Command::Resume => {
                    paused = false;
                    next_tick = Instant::now();
                    println!("[resumed]");
                }
                Command::Speed(tps) => {
                    speed_tps = tps;
                    if tps == 0.0 {
                        println!("[speed] uncapped");
                    } else {
                        println!("[speed] {tps} ticks/sec");
                    }
                    next_tick = Instant::now();
                }
                Command::Status => {
                    let elapsed = window_start.elapsed().as_secs_f64();
                    let achieved = if elapsed > 0.0 {
                        window_ticks as f64 / elapsed
                    } else {
                        0.0
                    };
                    window_ticks = 0;
                    window_start = Instant::now();
                    let alive = app
                        .world_mut()
                        .query_filtered::<(), With<npc::Npc>>()
                        .iter(app.world())
                        .count();
                    let world = app.world();
                    let clock = world.resource::<SimClock>();
                    let weather = world.resource::<Weather>();
                    let log = world.resource::<EventLog>();
                    let market = world.resource::<professions::Market>();
                    let ledger = world.resource::<economy::Ledger>();
                    let blight = world.resource::<village::ForageBlight>().0;
                    let pacing = if speed_tps == 0.0 {
                        "max".to_string()
                    } else {
                        format!("{speed_tps} tps")
                    };
                    println!(
                        "[status] {} (tick {}) | weather {} {}C | alive {} | bread {} fish {} grain {}{} | treasury {}c | speed {} (achieved {:.0} tps) | events {} | {}",
                        clock.stamp(),
                        clock.tick,
                        weather.kind,
                        weather.temp_c,
                        alive,
                        market.bakery_bread + market.tavern_bread,
                        market.stall_fish + market.tavern_fish,
                        market.farm_grain + market.bakery_grain,
                        if blight { " (BLIGHT)" } else { "" },
                        ledger.treasury,
                        pacing,
                        achieved,
                        log.count,
                        if paused { "paused" } else { "running" },
                    );
                }
                Command::Snapshot => match snapshot::write_snapshot(app.world_mut(), &config.out_dir) {
                    Ok(path) => println!("[snapshot] {}", path.display()),
                    Err(error) => println!("[snapshot] failed: {error}"),
                },
                Command::Wealth => {
                    let ledger = app.world().resource::<economy::Ledger>();
                    let mut purses: Vec<(&String, &i64)> = ledger.accounts.iter().collect();
                    purses.sort_by_key(|(_, balance)| -**balance);
                    println!("[wealth] treasury: {}c", ledger.treasury);
                    for (name, balance) in purses {
                        println!("[wealth]   {name:<8} {balance:>6}c");
                    }
                }
                Command::SetBlight(active) => {
                    let world = app.world_mut();
                    world.resource_mut::<village::ForageBlight>().0 = active;
                    world.resource_scope(|world, mut log: Mut<EventLog>| {
                        log.log(
                            world.resource::<SimClock>(),
                            &SimEvent::BlightSet { active },
                        );
                    });
                    println!(
                        "[blight] {}",
                        if active { "active — foraging yields nothing" } else { "lifted" }
                    );
                }
                Command::Stop => break 'main "stop command".to_string(),
                Command::Help => println!("{}", control::HELP),
                Command::Unknown(line) => {
                    println!("[?] unrecognized: '{line}' — try 'help'");
                }
            }
        }

        if paused {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }

        app.update();
        window_ticks += 1;

        if config.stop_after_days > 0 {
            let clock = app.world().resource::<SimClock>();
            if clock.day() > config.stop_after_days {
                break 'main format!("day limit ({} days)", config.stop_after_days);
            }
        }

        if speed_tps > 0.0 {
            next_tick += Duration::from_secs_f64(1.0 / speed_tps);
            let now = Instant::now();
            if next_tick > now {
                std::thread::sleep(next_tick - now);
            } else {
                // Fell behind (or speed was just raised): don't try to catch
                // up with a burst, just continue from now.
                next_tick = now;
            }
        }
    };

    // Orderly shutdown: final snapshot, stop event, flush, report.
    let snapshot_path = snapshot::write_snapshot(app.world_mut(), &config.out_dir);
    let world = app.world_mut();
    world.resource_scope(|world, mut log: Mut<EventLog>| {
        let clock = world.resource::<SimClock>();
        log.log(
            clock,
            &SimEvent::SimStopped {
                reason: stop_reason.clone(),
                days_elapsed: clock.day() - 1,
            },
        );
        log.flush();
    });

    let clock = world.resource::<SimClock>();
    let events = world.resource::<EventLog>().count;
    let wall = started.elapsed().as_secs_f64();
    let achieved = if wall > 0.0 {
        clock.tick as f64 / wall
    } else {
        0.0
    };
    println!("--- final report ---");
    println!("reason:        {stop_reason}");
    println!("sim time:      {} ({} ticks, {} full days)", clock.stamp(), clock.tick, clock.day() - 1);
    println!("wall time:     {wall:.1}s ({achieved:.0} ticks/sec average)");
    println!("events logged: {events}");
    println!("event log:     {}", log_path.display());
    match snapshot_path {
        Ok(path) => println!("snapshot:      {}", path.display()),
        Err(error) => println!("snapshot:      FAILED: {error}"),
    }
    let ledger = world.resource::<economy::Ledger>();
    let mut purses: Vec<(&String, &i64)> = ledger.accounts.iter().collect();
    purses.sort_by_key(|(_, balance)| -**balance);
    println!("--- wealth ---");
    println!("treasury: {}c | total in circulation: {}c", ledger.treasury, ledger.total());
    for (name, balance) in purses {
        println!("  {name:<8} {balance:>6}c");
    }
    Ok(())
}
