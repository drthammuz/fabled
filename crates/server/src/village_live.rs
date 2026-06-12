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
use shared::protocol::{NetTransform, VillageClock, Villager, VillagerState, VillagerStats};
use shared::village_map;

use sim::clock::SimClock;
use sim::events::EventLog;
use sim::npc::{Activity, ActivityKind, Npc};
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

        app.insert_resource(SimClock { tick: 0 })
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
                (
                    sim::npc::spawn_npcs,
                    warm_up_sim,
                    decorate_villagers,
                    spawn_clock_entity,
                )
                    .chain(),
            )
            .add_systems(
                FixedUpdate,
                (drive_sim, wander_villagers, move_villagers).chain(),
            );

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

/// Accumulates real time into sim minutes at the fixed pace
/// (`config::SECS_PER_SIM_MINUTE`).
#[derive(Resource, Default)]
struct SimPacing {
    accumulator: f64,
}

/// Where a villager is currently walking: a queue of world-space waypoints
/// (doors first, venue last). Visual movement is decoupled from the sim's
/// abstract travel times: villagers cover ground at a fixed human speed
/// regardless of time compression.
#[derive(Component)]
struct WorldWalk {
    /// Remaining waypoints, walked front to back.
    path: Vec<Vec3>,
    /// m/s in world space.
    speed: f32,
}

/// Cosmetic idling: when a villager has nowhere to walk, they occasionally
/// amble to a random spot around their current venue. Pure server-side
/// visual flavor — the economy sim never sees it.
#[derive(Component)]
struct Wander {
    anchor: Vec3,
    /// Venue name, for wander radius and door lookups.
    place: String,
    /// `Time::elapsed_secs` after which the next amble may start.
    next_at: f32,
}

/// How far villagers drift around each venue while there (meters). Walled
/// venues keep this small enough to stay inside their walls.
fn wander_radius(place: &str) -> f32 {
    match place {
        "farm" => 9.0,
        "dock" => 5.0,
        "square" => 6.0,
        "tavern" => 2.0,
        "bakery" => 1.6,
        _ => 1.2, // hut interiors
    }
}

/// Door waypoint for a venue, if it has walls.
fn venue_door(place: &str, home_index: usize) -> Option<Vec3> {
    if place == "home" {
        Some(village_map::home_door_pos(home_index))
    } else {
        village_map::place_door_pos(place)
    }
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

/// Live games join the village just before the workday ends. Rather than
/// teleporting fresh NPCs into an arbitrary hour (which made half of them
/// nap at 17:30 because they spawned "tired"), fast-forward the sim from
/// midnight so everyone arrives at 16:55 with a real day behind them.
fn warm_up_sim(world: &mut World) {
    const START_MINUTE: u64 = 16 * 60 + 55;
    for _ in 0..START_MINUTE {
        world.run_schedule(SimTick);
    }
    info!("village live: warmed up to day 1, 16:55");
}

/// After the sim spawns its NPCs, make them visible to the network layer.
fn decorate_villagers(
    mut commands: Commands,
    npcs: Query<(Entity, &Npc, &Profession, &Activity)>,
) {
    let mut count = 0;
    for (index, (entity, npc, profession, activity)) in npcs.iter().enumerate() {
        let here = world_pos_for(activity, index);
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
            VillagerStats {
                hunger: 0,
                energy: 0,
                warmth: 0,
                social: 0,
                mood: 0,
                purse: 0,
            },
            HomeIndex(index),
            NetTransform {
                translation: here,
                rotation: Quat::IDENTITY,
            },
            // The server-side Transform makes host mode render villagers
            // directly (and gives their model children a valid transform
            // chain). Networked clients get theirs from NetTransform.
            Transform::from_translation(here),
            WorldWalk {
                path: Vec::new(),
                speed: config::VILLAGER_WALK_SPEED,
            },
            Wander {
                anchor: here,
                place: activity.place.name().to_string(),
                next_at: 0.0,
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
    for _ in 0..8 {
        let due = world.resource::<SimPacing>().accumulator >= config::SECS_PER_SIM_MINUTE;
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
    time: Res<Time>,
    ledger: Res<sim::economy::Ledger>,
    mut villagers: Query<(
        &Activity,
        &HomeIndex,
        &Npc,
        &sim::npc::Needs,
        &sim::brain::Emotions,
        &mut WorldWalk,
        &mut Wander,
        &mut VillagerState,
        &mut VillagerStats,
    )>,
    mut clock_entity: Query<&mut VillageClock>,
) {
    for (activity, home, npc, needs, emotions, mut walk, mut wander, mut state, mut stats) in
        &mut villagers
    {
        let next_stats = VillagerStats {
            hunger: needs.hunger.round().clamp(0.0, 100.0) as u8,
            energy: needs.energy.round().clamp(0.0, 100.0) as u8,
            warmth: needs.warmth.round().clamp(0.0, 100.0) as u8,
            social: needs.social.round().clamp(0.0, 100.0) as u8,
            mood: emotions.mood.round().clamp(0.0, 100.0) as u8,
            purse: ledger.accounts.get(&npc.name).copied().unwrap_or(0),
        };
        if *stats != next_stats {
            *stats = next_stats;
        }
        let target = world_pos_for(activity, home.0);
        if wander.anchor != target {
            // New venue: commute there at full speed, then settle in.
            // Walled venues are entered and left through their doors.
            let mut path = Vec::new();
            if let Some(door_out) = venue_door(&wander.place, home.0) {
                path.push(door_out);
            }
            if let Some(door_in) = venue_door(activity.place.name(), home.0) {
                path.push(door_in);
            }
            path.push(target);
            wander.anchor = target;
            wander.place = activity.place.name().to_string();
            wander.next_at = time.elapsed_secs() + 4.0;
            walk.path = path;
            walk.speed = config::VILLAGER_WALK_SPEED;
        }
        let next = VillagerState {
            action: activity.kind.name().to_string(),
            place: activity.place.name().to_string(),
            walking: state.walking,
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

/// Cosmetic ambling: villagers who have arrived somewhere pick a random
/// nearby spot now and then instead of standing frozen. Sleepers and
/// diners stay put. Strollers and the guard on patrol roam much wider —
/// that's the visible street life of the village.
fn wander_villagers(
    time: Res<Time>,
    mut villagers: Query<(&Activity, &Profession, &mut WorldWalk, &mut Wander)>,
) {
    let now = time.elapsed_secs();
    for (activity, profession, mut walk, mut wander) in &mut villagers {
        if matches!(activity.kind, ActivityKind::Sleep | ActivityKind::Eat) {
            continue;
        }
        if !walk.path.is_empty() || now < wander.next_at {
            continue;
        }
        let stroll = activity.kind == ActivityKind::Stroll;
        let patrol =
            activity.kind == ActivityKind::Work && *profession == Profession::Guard;
        let (radius, speed, pause, outdoors) = if stroll {
            // Roam the streets between the square and the home ring.
            (16.0, 3.2, 0.5 + rand::random::<f32>() * 2.5, true)
        } else if patrol {
            // The guard makes rounds across the whole village core.
            (22.0, 3.2, 1.0 + rand::random::<f32>() * 3.0, true)
        } else {
            let radius = wander_radius(&wander.place);
            let outdoors = matches!(wander.place.as_str(), "square" | "farm" | "dock");
            (
                radius,
                config::VILLAGER_AMBLE_SPEED,
                2.0 + rand::random::<f32>() * 9.0,
                outdoors,
            )
        };
        // Outdoor targets must not land inside a building.
        for _ in 0..8 {
            let angle = rand::random::<f32>() * std::f32::consts::TAU;
            let dist = radius * rand::random::<f32>().sqrt();
            let target =
                wander.anchor + Vec3::new(angle.cos() * dist, 0.0, angle.sin() * dist);
            if outdoors && village_map::inside_any_building(target.xz(), 0.9) {
                continue;
            }
            walk.path = vec![target];
            walk.speed = speed;
            break;
        }
        wander.next_at = now + pause;
    }
}

/// Every server tick (30 Hz): step villagers toward their walk target at
/// constant speed, facing their direction of travel.
fn move_villagers(
    time: Res<Time>,
    mut villagers: Query<(
        &mut WorldWalk,
        &mut NetTransform,
        &mut Transform,
        &mut VillagerState,
    )>,
) {
    let delta = time.delta_secs();
    for (mut walk, mut net, mut transform, mut state) in &mut villagers {
        // Pop reached waypoints (more than one in a frame if they're close).
        while let Some(&next) = walk.path.first() {
            if transform.translation.distance_squared(next) < 0.04 {
                walk.path.remove(0);
            } else {
                break;
            }
        }
        let Some(&next) = walk.path.first() else {
            if state.walking {
                state.walking = false;
            }
            continue;
        };
        let offset = next - transform.translation;
        let distance = offset.length();
        let step = (walk.speed * delta).min(distance);
        let direction = offset / distance;
        let mut position = transform.translation + direction * step;
        // Hug the terrain (flat across the village, but commute corridors
        // can brush the edge of the hills).
        position.y = shared::terrain::height(position.x, position.z);
        let rotation = Quat::from_rotation_y(direction.x.atan2(direction.z));
        transform.translation = position;
        transform.rotation = rotation;
        net.translation = position;
        net.rotation = rotation;
        if !state.walking {
            state.walking = true;
        }
    }
}
