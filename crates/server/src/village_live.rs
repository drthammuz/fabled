//! The live village: the same simulation that runs headless in the `sim`
//! crate, embedded in the game server. One sim tick (= one sim minute)
//! fires every `SECS_PER_SIM_MINUTE` real seconds via a custom schedule;
//! between ticks this plugin moves villagers smoothly through the world
//! and replicates their positions and activities to clients.
//!
//! All NPC logic stays server-side: clients only ever see replicated state.

use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use shared::config;
use shared::protocol::{NetTransform, VillageClock, Villager, VillagerState};
use shared::village_map;

use sim::clock::SimClock;
use sim::events::EventLog;
use sim::npc::{Activity, Npc};
use sim::professions::Profession;

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct SimTick;

pub struct VillageLivePlugin;

impl Plugin for VillageLivePlugin {
    fn build(&self, app: &mut App) {
        // The live village writes the same JSONL event log as headless runs,
        // so everything that happens in front of you is also on disk.
        let log = std::fs::create_dir_all("sim_out")
            .and_then(|_| {
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                EventLog::create(format!("sim_out/live-village-{stamp}.jsonl").into())
            })
            .unwrap_or_else(|_| EventLog::disabled());

        // Live games start mid-morning, not at 00:00 in the dark.
        app.insert_resource(SimClock { tick: 8 * 60 })
            .insert_resource(sim::weather::SimRng(
                <rand_chacha::ChaCha8Rng as rand::SeedableRng>::seed_from_u64(42),
            ))
            .insert_resource(sim::weather::Weather::default())
            .insert_resource(sim::genome::Genome::default())
            .insert_resource(sim::village::ForageBlight::default())
            .insert_resource(sim::village::DailyStats::default())
            .insert_resource(sim::economy::Ledger::default())
            .insert_resource(sim::professions::Market::default())
            .insert_resource(sim::professions::Roster::default())
            .insert_resource(log)
            .insert_resource(SimPacing::default())
            .init_schedule(SimTick)
            .add_systems(
                Startup,
                (sim::npc::spawn_npcs, decorate_villagers, spawn_clock_entity).chain(),
            )
            .add_systems(FixedUpdate, (drive_sim, move_villagers).chain());

        app.edit_schedule(SimTick, |schedule| {
            schedule.add_systems(
                (
                    sim::clock::advance_clock,
                    sim::village::daily_summary,
                    sim::weather::day_start,
                    sim::brain::decay_memories,
                    sim::economy::fiscal_day,
                    sim::professions::tavern_restock,
                    sim::housing::improve_homes,
                    sim::npc::needs_tick,
                    sim::brain::emotions_tick,
                    sim::npc::act,
                    sim::economy::conservation_check,
                    sync_villager_state,
                    flush_log_hourly,
                )
                    .chain(),
            );
        });
    }
}

/// Accumulates real time into sim minutes.
#[derive(Resource, Default)]
struct SimPacing {
    accumulator: f64,
}

/// Where a villager is walking from/to, in world space and sim-tick time.
#[derive(Component)]
struct WorldWalk {
    from: Vec3,
    to: Vec3,
    /// Sim ticks (fractional progress comes from the pacing accumulator).
    depart_tick: u64,
    arrive_tick: u64,
}

/// Marker entity carrying the replicated village clock.
#[derive(Component)]
struct ClockEntity;

fn world_pos_for(activity: &Activity, home_index: usize) -> Vec3 {
    let place = activity.place.name();
    if place == "home" {
        village_map::home_world_pos(home_index)
    } else {
        village_map::place_world_pos(place)
    }
}

/// Stable home slot per villager (sim spawns them in roster order).
#[derive(Component)]
struct HomeIndex(usize);

/// After the sim spawns its NPCs, make them visible to the network layer.
fn decorate_villagers(
    mut commands: Commands,
    npcs: Query<(Entity, &Npc, &Profession, &Activity)>,
) {
    let mut count = 0;
    for (index, (entity, npc, profession, activity)) in npcs.iter().enumerate() {
        let home = village_map::home_world_pos(index);
        commands.entity(entity).insert((
            Replicated,
            Villager {
                name: npc.name.clone(),
                profession: profession.name().to_string(),
            },
            VillagerState {
                action: activity.kind.name().to_string(),
                place: activity.place.name().to_string(),
                walking: false,
            },
            HomeIndex(index),
            NetTransform {
                translation: home,
                rotation: Quat::IDENTITY,
            },
            WorldWalk {
                from: home,
                to: home,
                depart_tick: 0,
                arrive_tick: 0,
            },
        ));
        count += 1;
    }
    info!("village live: {count} villagers replicated");
}

fn spawn_clock_entity(mut commands: Commands) {
    commands.spawn((
        Replicated,
        ClockEntity,
        VillageClock {
            day: 1,
            minute_of_day: 0,
        },
    ));
}

/// Live servers want tail-able logs; headless runs flush at day boundaries
/// only, which is too coarse here.
fn flush_log_hourly(clock: Res<SimClock>, mut log: ResMut<EventLog>) {
    if clock.minute_of_day() % 60 == 0 {
        log.flush();
    }
}

/// Fires sim ticks at the configured pace.
fn drive_sim(world: &mut World) {
    let delta = world.resource::<Time<Fixed>>().delta_secs_f64();
    world.resource_mut::<SimPacing>().accumulator += delta;
    // Catch up at most a few ticks per frame; pacing hiccups must not
    // freeze the server in a sim-tick loop.
    for _ in 0..4 {
        let due = {
            let pacing = world.resource::<SimPacing>();
            pacing.accumulator >= config::SECS_PER_SIM_MINUTE
        };
        if !due {
            break;
        }
        world.resource_mut::<SimPacing>().accumulator -= config::SECS_PER_SIM_MINUTE;
        world.run_schedule(SimTick);
    }
}

/// After each sim tick: pick up new activities (walk targets + state) and
/// update the replicated clock.
fn sync_villager_state(
    clock: Res<SimClock>,
    mut villagers: Query<(
        &Activity,
        &HomeIndex,
        &NetTransform,
        &mut WorldWalk,
        &mut VillagerState,
    )>,
    mut clock_entity: Query<&mut VillageClock>,
) {
    for (activity, home, net, mut walk, mut state) in &mut villagers {
        let target = world_pos_for(activity, home.0);
        if walk.to != target {
            walk.from = net.translation;
            walk.to = target;
            walk.depart_tick = clock.tick;
            // The sim's travel time is its own (bigger) map; in the world we
            // walk the compressed distance over the same duration.
            walk.arrive_tick = activity.arrives.max(clock.tick);
        }
        let walking = clock.tick < activity.arrives && walk.to != walk.from;
        let next = VillagerState {
            action: activity.kind.name().to_string(),
            place: activity.place.name().to_string(),
            walking,
        };
        if *state != next {
            *state = next;
        }
    }
    if let Ok(mut village_clock) = clock_entity.single_mut() {
        let next = VillageClock {
            day: clock.day(),
            minute_of_day: clock.minute_of_day(),
        };
        if *village_clock != next {
            *village_clock = next;
        }
    }
}

/// Every server tick (30 Hz): move villagers along their walks with
/// fractional sim-time progress, facing their direction of travel.
fn move_villagers(
    clock: Res<SimClock>,
    pacing: Res<SimPacing>,
    mut villagers: Query<(&WorldWalk, &Activity, &mut NetTransform, &mut VillagerState)>,
) {
    let fraction = (pacing.accumulator / config::SECS_PER_SIM_MINUTE).clamp(0.0, 1.0) as f32;
    let now = clock.tick as f32 + fraction;
    for (walk, activity, mut net, mut state) in &mut villagers {
        if walk.arrive_tick <= walk.depart_tick {
            continue;
        }
        let span = (walk.arrive_tick - walk.depart_tick) as f32;
        let progress = ((now - walk.depart_tick as f32) / span).clamp(0.0, 1.0);
        let position = walk.from.lerp(walk.to, progress);
        let direction = walk.to - walk.from;
        let rotation = if direction.length_squared() > 0.01 && progress < 1.0 {
            Quat::from_rotation_y(direction.x.atan2(direction.z))
        } else {
            net.rotation
        };
        let next = NetTransform {
            translation: position,
            rotation,
        };
        if *net != next {
            *net = next;
        }
        // Flip the walking flag off as soon as the walk visually completes,
        // even between sim ticks, so animations switch promptly.
        if progress >= 1.0 && state.walking && clock.tick >= activity.arrives {
            state.walking = false;
        }
    }
}
