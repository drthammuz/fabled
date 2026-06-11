//! Villagers: needs-driven utility AI, weighted by each NPC's brain
//! (traits, mood, memories — see `brain.rs`). V4: villagers exist in
//! space — actions happen at places, walking there takes time and exposes
//! you to the weather, and home quality decides how well you keep warm.

use bevy::prelude::*;
use rand::Rng;
use serde::Serialize;

use crate::brain::{self, Emotions, Memories, Traits};
use crate::clock::SimClock;
use crate::economy::Ledger;
use crate::events::{EventLog, SimEvent};
use crate::housing::{self, Home};
use crate::params;
use crate::professions::{self, Market, MealSource, Profession, Roster};
use crate::village::{DailyStats, ForageBlight};
use crate::weather::{SimRng, Weather};
use crate::world_map::{self, PlaceKind};

#[derive(Component)]
pub struct Npc {
    pub name: String,
}

/// Where the villager is (meters; updated on arrival).
#[derive(Component)]
pub struct Pos(pub Vec2);

#[derive(Component)]
pub struct Needs {
    /// 0 = stuffed, 100 = starving.
    pub hunger: f32,
    /// 100 = rested, 0 = exhausted.
    pub energy: f32,
    /// 100 = cozy, 0 = freezing.
    pub warmth: f32,
    /// 0 = fulfilled, 100 = desperately lonely.
    pub social: f32,
}

#[derive(Component)]
pub struct Health(pub f32);

/// Edge-detection flags so critical states are logged once per episode
/// instead of every tick.
#[derive(Component, Default)]
pub struct CriticalFlags {
    starving: bool,
    freezing: bool,
    hungry_broke: bool,
    lonely: bool,
    miserable: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    Sleep,
    Eat,
    Work,
    WarmUp,
    Socialize,
    Idle,
}

impl ActivityKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Sleep => "sleep",
            Self::Eat => "eat",
            Self::Work => "work",
            Self::WarmUp => "warm_up",
            Self::Socialize => "socialize",
            Self::Idle => "idle",
        }
    }
}

#[derive(Component)]
pub struct Activity {
    pub kind: ActivityKind,
    /// Where this happens; warmth exposure follows the venue.
    pub place: PlaceKind,
    /// Tick of arrival at the venue (walking until then, exposed outdoors).
    pub arrives: u64,
    /// Tick at which the activity completes. Sleep is open-ended
    /// (`u64::MAX`); waking is decided by energy/hunger/shift instead.
    pub until: u64,
    /// For Socialize: whether an ale was bought (changes relief and mood).
    pub paid: bool,
}

pub fn spawn_npcs(
    mut commands: Commands,
    mut rng: ResMut<SimRng>,
    mut ledger: ResMut<Ledger>,
    mut roster: ResMut<Roster>,
    clock: Res<SimClock>,
    mut log: ResMut<EventLog>,
) {
    for (index, (name, profession)) in professions::ROSTER.into_iter().enumerate() {
        // Slightly randomized starting needs so the villagers don't act in
        // lockstep from tick one.
        let needs = Needs {
            hunger: rng.0.random_range(20.0..60.0),
            energy: rng.0.random_range(60.0..100.0),
            warmth: rng.0.random_range(60.0..100.0),
            social: rng.0.random_range(10.0..50.0),
        };
        let traits = Traits {
            metabolism: rng
                .0
                .random_range(params::METABOLISM_MIN..params::METABOLISM_MAX),
            sociability: rng.0.random_range(params::TRAIT_MIN..params::TRAIT_MAX),
            diligence: rng.0.random_range(params::TRAIT_MIN..params::TRAIT_MAX),
            frugality: rng.0.random_range(params::TRAIT_MIN..params::TRAIT_MAX),
        };
        let genome = housing::initial_shack(&mut rng);
        let score = housing::score(&genome);
        let home_pos = world_map::home_pos(index);
        log.log(
            &clock,
            &SimEvent::NpcSpawned {
                npc: name.to_string(),
                profession: profession.name().to_string(),
                traits,
            },
        );
        commands.spawn((
            Npc {
                name: name.to_string(),
            },
            profession,
            needs,
            traits,
            Emotions::default(),
            Memories::default(),
            Health(100.0),
            CriticalFlags::default(),
            Pos(home_pos),
            Home {
                pos: home_pos,
                genome,
                score,
                planning_offset: index as u64,
            },
            Activity {
                kind: ActivityKind::Idle,
                place: PlaceKind::Home,
                arrives: 0,
                until: 0,
                paid: false,
            },
        ));
        ledger
            .accounts
            .insert(name.to_string(), params::STARTING_PURSE);
        roster.0.insert(profession, name.to_string());
    }
    ledger.treasury = params::TREASURY_START;
}

/// Per-tick need/health drift, expression of critical states, and death
/// (estate goes to the treasury). Warmth follows the venue: walking and
/// open-air places expose you to the weather; how much your home protects
/// you depends on how well it's built.
pub fn needs_tick(
    mut commands: Commands,
    clock: Res<SimClock>,
    weather: Res<Weather>,
    mut ledger: ResMut<Ledger>,
    mut roster: ResMut<Roster>,
    mut log: ResMut<EventLog>,
    mut npcs: Query<(
        Entity,
        &Npc,
        &Traits,
        &Emotions,
        &Home,
        &mut Needs,
        &mut Health,
        &mut CriticalFlags,
        &Activity,
    )>,
) {
    for (entity, npc, traits, emotions, home, mut needs, mut health, mut flags, activity) in
        &mut npcs
    {
        needs.hunger = (needs.hunger + params::HUNGER_PER_HOUR * traits.metabolism / 60.0)
            .clamp(0.0, 100.0);
        needs.social = (needs.social
            + params::SOCIAL_PER_HOUR * (0.5 + traits.sociability) / 60.0)
            .clamp(0.0, 100.0);

        let asleep = activity.kind == ActivityKind::Sleep && clock.tick >= activity.arrives;
        if asleep {
            needs.energy =
                (needs.energy + params::ENERGY_SLEEP_PER_HOUR / 60.0).clamp(0.0, 100.0);
        } else {
            needs.energy =
                (needs.energy - params::ENERGY_DRAIN_PER_HOUR / 60.0).clamp(0.0, 100.0);
        }

        let outdoor_delta =
            (weather.temp_c as f32 - 10.0) * params::WARMTH_OUTDOOR_FACTOR;
        let traveling = clock.tick < activity.arrives;
        let warmth_delta = if traveling {
            outdoor_delta
        } else {
            match activity.place {
                PlaceKind::Home => {
                    // A well-built home holds the indoor benefit; a hovel
                    // leaks half the weather straight through the walls.
                    let shelter = params::HOME_SHELTER_FLOOR
                        + (1.0 - params::HOME_SHELTER_FLOOR) * home.score.shelter;
                    if activity.kind == ActivityKind::WarmUp {
                        params::WARMTH_FIRE_PER_HOUR * (0.4 + 0.6 * shelter)
                    } else {
                        outdoor_delta * (1.0 - shelter) * 0.5
                            + params::WARMTH_INDOOR_PER_HOUR * shelter
                    }
                }
                place if place.outdoors() => outdoor_delta,
                _ => params::WARMTH_INDOOR_PER_HOUR,
            }
        };
        needs.warmth = (needs.warmth + warmth_delta / 60.0).clamp(0.0, 100.0);

        // Expressions: visible state transitions, logged once per episode.
        // These are the hooks a future LLM speech layer will render.
        let starving = needs.hunger >= 100.0;
        let freezing = needs.warmth <= 0.0;
        let lonely = needs.social >= 90.0;
        let miserable = emotions.mood <= 15.0;
        if starving && !flags.starving {
            log.log(&clock, &SimEvent::NpcStarving { npc: npc.name.clone() });
        }
        if freezing && !flags.freezing {
            log.log(&clock, &SimEvent::NpcFreezing { npc: npc.name.clone() });
        }
        if lonely && !flags.lonely {
            log.log(
                &clock,
                &SimEvent::Expression {
                    npc: npc.name.clone(),
                    what: "lonely".to_string(),
                },
            );
        }
        if miserable && !flags.miserable {
            log.log(
                &clock,
                &SimEvent::Expression {
                    npc: npc.name.clone(),
                    what: "miserable".to_string(),
                },
            );
        }
        flags.starving = starving;
        flags.freezing = freezing;
        flags.lonely = lonely;
        flags.miserable = miserable;

        if starving {
            health.0 -= params::STARVE_HEALTH_PER_HOUR / 60.0;
        }
        if freezing {
            health.0 -= params::FREEZE_HEALTH_PER_HOUR / 60.0;
        }
        if !starving && !freezing && needs.hunger < 60.0 && needs.warmth > 30.0 {
            health.0 = (health.0 + params::HEAL_PER_HOUR / 60.0).min(100.0);
        }

        if health.0 <= 0.0 {
            let cause = if starving { "starvation" } else { "freezing" };
            let estate = ledger.inherit(&npc.name);
            roster.0.retain(|_, holder| holder != &npc.name);
            log.log(
                &clock,
                &SimEvent::NpcDied {
                    npc: npc.name.clone(),
                    cause: cause.to_string(),
                    estate,
                },
            );
            log.flush();
            commands.entity(entity).despawn();
        }
    }
}

fn meal_place(source: MealSource) -> PlaceKind {
    match source {
        MealSource::Bakery => PlaceKind::Bakery,
        MealSource::FishStall => PlaceKind::Dock,
        MealSource::Tavern => PlaceKind::Tavern,
    }
}

/// Finish activities (applying their effects) and choose the next one,
/// walking to wherever it happens.
pub fn act(
    clock: Res<SimClock>,
    mut market: ResMut<Market>,
    mut ledger: ResMut<Ledger>,
    roster: Res<Roster>,
    blight: Res<ForageBlight>,
    mut stats: ResMut<DailyStats>,
    mut rng: ResMut<SimRng>,
    mut log: ResMut<EventLog>,
    mut npcs: Query<(
        &Npc,
        &Profession,
        &Traits,
        &Home,
        &mut Pos,
        &mut Emotions,
        &mut Memories,
        &mut Needs,
        &mut Activity,
        &mut CriticalFlags,
    )>,
) {
    let now = clock.tick;
    let hour = clock.hour();
    let is_night = !(params::DAY_START_HOUR..params::NIGHT_START_HOUR).contains(&hour);
    let is_evening = (16..params::TAVERN_CLOSE_HOUR).contains(&hour);
    let can_socialize = (8..23).contains(&hour);

    // Who is out socializing right now (for company bonuses and memories).
    let company: Vec<String> = npcs
        .iter()
        .filter(|(.., activity, _)| activity.kind == ActivityKind::Socialize)
        .map(|(npc, ..)| npc.name.clone())
        .collect();

    for (
        npc,
        profession,
        traits,
        home,
        mut pos,
        mut emotions,
        mut memories,
        mut needs,
        mut activity,
        mut flags,
    ) in &mut npcs
    {
        if activity.kind == ActivityKind::Sleep && now >= activity.arrives {
            // Wake when rested, dangerously hungry, or the shift starts
            // (only if reasonably rested — exhausted workers oversleep).
            let wake = needs.energy >= 95.0
                || needs.hunger >= 85.0
                || (profession.on_shift(hour) && needs.energy > 50.0);
            if !wake {
                continue;
            }
        } else if now < activity.until {
            continue;
        } else {
            // Completion effects.
            match activity.kind {
                ActivityKind::Eat => {
                    needs.hunger = (needs.hunger - params::MEAL_RELIEF).max(0.0);
                    stats.meals += 1;
                    emotions.mood = (emotions.mood + params::MOOD_MEAL).min(100.0);
                }
                ActivityKind::Work => {
                    professions::complete_work_hour(
                        &npc.name,
                        *profession,
                        &mut market,
                        &mut ledger,
                        &roster,
                        &mut rng,
                        blight.0,
                        &mut stats,
                        &clock,
                        &mut log,
                        &mut memories,
                    );
                }
                ActivityKind::Socialize => {
                    let mut relief = if activity.paid {
                        params::SOCIAL_RELIEF_ALE
                    } else {
                        params::SOCIAL_RELIEF_LOITER
                    };
                    let mood_bump = if activity.paid {
                        params::MOOD_SOCIAL_ALE
                    } else {
                        params::MOOD_SOCIAL_LOITER
                    };
                    // Company makes it better — and memorable.
                    let others: Vec<&String> =
                        company.iter().filter(|name| **name != npc.name).collect();
                    if !others.is_empty() {
                        relief += params::SOCIAL_FRIEND_BONUS;
                        let other = others[rng.0.random_range(0..others.len())].clone();
                        brain::remember(
                            &mut memories,
                            &npc.name,
                            "good_evening_with",
                            Some(&other),
                            0.4,
                            &clock,
                            &mut log,
                        );
                    }
                    needs.social = (needs.social - relief).max(0.0);
                    emotions.mood = (emotions.mood + mood_bump).min(100.0);
                }
                _ => {}
            }
        }

        let purse = ledger.balance(&npc.name);
        let meal =
            professions::meal_option(*profession, traits.frugality, hour, purse, &market, &roster);

        // "Hungry with no way to eat" is a key economic distress signal.
        if needs.hunger >= 80.0 && meal.is_none() {
            if !flags.hungry_broke {
                flags.hungry_broke = true;
                log.log(
                    &clock,
                    &SimEvent::CannotAffordMeal {
                        npc: npc.name.clone(),
                        purse,
                    },
                );
                brain::remember(
                    &mut memories,
                    &npc.name,
                    "went_hungry",
                    None,
                    -0.6,
                    &clock,
                    &mut log,
                );
            }
        }

        // Demand response: stay home when the warehouse is already full.
        let work_blocked = match profession {
            Profession::Farmer | Profession::Farmhand => {
                market.farm_grain >= params::FARM_GRAIN_CAP
            }
            Profession::Baker => market.bakery_bread >= params::BAKERY_BREAD_CAP,
            _ => false,
        };
        let on_shift = profession.on_shift(hour) && !work_blocked;
        let choice = decide(
            &needs,
            traits,
            emotions.mood,
            purse,
            meal.is_some(),
            on_shift,
            is_night,
            is_evening,
            can_socialize,
        );

        // A sleeper who re-picks sleep just keeps dozing; don't spam the log.
        if choice == ActivityKind::Sleep && activity.kind == ActivityKind::Sleep {
            continue;
        }

        // Venue and payment per choice.
        let tavern_social = (params::TAVERN_DRINKS_FROM_HOUR..params::TAVERN_CLOSE_HOUR)
            .contains(&hour)
            && roster.0.contains_key(&Profession::TavernKeeper);
        let mut paid = false;
        let (place, duration) = match choice {
            ActivityKind::Sleep => (PlaceKind::Home, u64::MAX),
            ActivityKind::Eat => {
                let (source, price) = meal.expect("eat chosen only when a meal exists");
                if !professions::buy_meal(
                    &npc.name, source, price, &mut market, &mut ledger, &roster,
                    &mut stats, &clock, &mut log,
                ) {
                    continue;
                }
                if matches!(source, MealSource::Tavern) && price > 0 {
                    emotions.mood = (emotions.mood + params::MOOD_SUPPER).min(100.0);
                    if let Some(keeper) = roster.0.get(&Profession::TavernKeeper) {
                        let keeper = keeper.clone();
                        brain::remember(
                            &mut memories,
                            &npc.name,
                            "supper_at_tavern",
                            Some(&keeper),
                            0.3,
                            &clock,
                            &mut log,
                        );
                    }
                }
                flags.hungry_broke = false;
                (meal_place(source), params::EAT_MINUTES)
            }
            ActivityKind::Work => (profession.workplace(), params::WORK_MINUTES),
            ActivityKind::WarmUp => (PlaceKind::Home, params::WARMUP_MINUTES),
            ActivityKind::Socialize => {
                let place = if tavern_social {
                    paid = professions::buy_ale(
                        &npc.name, *profession, traits.frugality, purse, &mut market,
                        &mut ledger, &roster, &mut stats, &clock, &mut log,
                    );
                    PlaceKind::Tavern
                } else {
                    PlaceKind::Square
                };
                (place, params::SOCIALIZE_MINUTES)
            }
            ActivityKind::Idle => (PlaceKind::Home, params::IDLE_MINUTES),
        };

        let target = if place == PlaceKind::Home {
            home.pos
        } else {
            place.pos()
        };
        let travel = world_map::travel_ticks(pos.0, target);
        pos.0 = target;
        activity.kind = choice;
        activity.place = place;
        activity.arrives = now + travel;
        activity.until = if duration == u64::MAX {
            u64::MAX
        } else {
            now + travel + duration
        };
        activity.paid = paid;
        log.log(
            &clock,
            &SimEvent::ActionStarted {
                npc: npc.name.clone(),
                action: choice.name().to_string(),
                place: place.name().to_string(),
                travel_min: travel,
            },
        );
    }
}

/// Utility scoring: highest score wins, weighted by personality.
/// - diligence raises work priority and resistance to dozing off on shift
/// - sociability (via the social need) pulls people to the tavern
/// - an empty purse pushes people to work; a rock-bottom mood saps it
#[allow(clippy::too_many_arguments)]
fn decide(
    needs: &Needs,
    traits: &Traits,
    mood: f32,
    purse: i64,
    meal_available: bool,
    on_shift: bool,
    is_night: bool,
    is_evening: bool,
    can_socialize: bool,
) -> ActivityKind {
    let mut best = (ActivityKind::Idle, 15.0_f32);
    let consider = |kind: ActivityKind, score: f32, best: &mut (ActivityKind, f32)| {
        if score > best.1 {
            *best = (kind, score);
        }
    };

    if meal_available && needs.hunger >= params::EAT_THRESHOLD {
        consider(ActivityKind::Eat, 40.0 + needs.hunger, &mut best);
    }

    let mut sleep = 100.0 - needs.energy;
    if is_night {
        sleep += 40.0;
    }
    if on_shift {
        // The diligent push through fatigue; the lazy doze off on duty.
        sleep -= 10.0 + 25.0 * traits.diligence;
    }
    consider(ActivityKind::Sleep, sleep, &mut best);

    if needs.warmth < 50.0 {
        consider(ActivityKind::WarmUp, 140.0 - needs.warmth, &mut best);
    }

    if on_shift {
        let mut work = 40.0 + 45.0 * traits.diligence;
        if purse < params::BROKE_PURSE {
            work += params::BROKE_WORK_BONUS;
        }
        if mood <= 20.0 {
            work -= 15.0;
        }
        consider(ActivityKind::Work, work, &mut best);
    }

    if can_socialize {
        let window = if is_evening { 1.0 } else { 0.25 };
        let social = needs.social * (0.4 + 0.8 * traits.sociability) * window;
        consider(ActivityKind::Socialize, social, &mut best);
    }

    best.0
}
