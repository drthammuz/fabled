//! Professions and the goods economy: who works when, what their work
//! produces, and how meals are bought and sold. Sole proprietors — business
//! income goes straight into the owner's purse.

use bevy::prelude::*;
use rand::Rng;
use serde::Serialize;

use crate::clock::SimClock;
use crate::economy::Ledger;
use crate::events::{EventLog, SimEvent};
use crate::genome::Genome;
use crate::params;
use crate::village::DailyStats;
use crate::weather::SimRng;

#[derive(Component, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Profession {
    Farmer,
    Farmhand,
    Fisher,
    Baker,
    TavernKeeper,
    Guard,
    Mayor,
    Elder,
}

impl Profession {
    pub fn name(self) -> &'static str {
        match self {
            Self::Farmer => "farmer",
            Self::Farmhand => "farmhand",
            Self::Fisher => "fisher",
            Self::Baker => "baker",
            Self::TavernKeeper => "tavern_keeper",
            Self::Guard => "guard",
            Self::Mayor => "mayor",
            Self::Elder => "elder",
        }
    }

    /// Working this job exposes you to the weather.
    pub fn outdoors(self) -> bool {
        matches!(self, Self::Farmer | Self::Farmhand | Self::Fisher | Self::Guard)
    }

    /// Where this profession works.
    pub fn workplace(self) -> crate::world_map::PlaceKind {
        use crate::world_map::PlaceKind;
        match self {
            Self::Farmer | Self::Farmhand => PlaceKind::Farm,
            Self::Fisher => PlaceKind::Dock,
            Self::Baker => PlaceKind::Bakery,
            Self::TavernKeeper => PlaceKind::Tavern,
            // Patrol and town business happen on the square.
            Self::Guard | Self::Mayor | Self::Elder => PlaceKind::Square,
        }
    }

    /// Is `hour` within this profession's shift?
    pub fn on_shift(self, hour: u64) -> bool {
        match self {
            Self::Farmer | Self::Farmhand => (7..15).contains(&hour),
            Self::Fisher => (6..12).contains(&hour),
            Self::Baker => (5..11).contains(&hour),
            Self::TavernKeeper => {
                (params::TAVERN_OPEN_HOUR..params::TAVERN_CLOSE_HOUR).contains(&hour)
            }
            Self::Guard => hour >= 18 || hour < 2,
            Self::Mayor => (9..13).contains(&hour),
            Self::Elder => false,
        }
    }
}

/// Fixed V2 roster: (name, profession).
pub const ROSTER: [(&str, Profession); 8] = [
    ("Aldric", Profession::Farmer),
    ("Berta", Profession::Farmhand),
    ("Cole", Profession::Fisher),
    ("Dagny", Profession::Baker),
    ("Edwin", Profession::TavernKeeper),
    ("Frida", Profession::Guard),
    ("Gareth", Profession::Mayor),
    ("Hilda", Profession::Elder),
];

/// Who currently holds each job (sellers disappear when they die).
#[derive(Resource, Default)]
pub struct Roster(pub std::collections::BTreeMap<Profession, String>);

/// All goods stocks in the village.
#[derive(Resource)]
pub struct Market {
    pub farm_grain: u32,
    pub bakery_grain: u32,
    pub bakery_bread: u32,
    pub stall_fish: u32,
    pub tavern_bread: u32,
    pub tavern_fish: u32,
    pub tavern_ale: u32,
}

impl Default for Market {
    fn default() -> Self {
        Self {
            farm_grain: 10,
            bakery_grain: 4,
            bakery_bread: 8,
            stall_fish: 4,
            tavern_bread: 2,
            tavern_fish: 2,
            tavern_ale: 6,
        }
    }
}

/// Where a meal can come from, with its price and seller.
#[derive(Clone, Copy)]
pub enum MealSource {
    Bakery,
    FishStall,
    Tavern,
}

impl MealSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Bakery => "bread",
            Self::FishStall => "fish",
            Self::Tavern => "supper",
        }
    }
}

/// Picks where this villager would eat right now, if anywhere: producers
/// eat from their own stock (no coins move), everyone else goes to the
/// tavern in the evening when they feel wealthy, otherwise the cheapest
/// stocked option they can afford.
pub fn meal_option(
    profession: Profession,
    frugality: f32,
    hour: u64,
    purse: i64,
    market: &Market,
    roster: &Roster,
    genome: &Genome,
) -> Option<(MealSource, i64)> {
    match profession {
        Profession::Baker if market.bakery_bread > 0 => {
            return Some((MealSource::Bakery, 0));
        }
        Profession::Fisher if market.stall_fish > 0 => {
            return Some((MealSource::FishStall, 0));
        }
        Profession::TavernKeeper if market.tavern_bread + market.tavern_fish > 0 => {
            return Some((MealSource::Tavern, 0));
        }
        _ => {}
    }
    let tavern_open = (params::TAVERN_OPEN_HOUR..params::TAVERN_CLOSE_HOUR).contains(&hour)
        && roster.0.contains_key(&Profession::TavernKeeper)
        && market.tavern_bread + market.tavern_fish > 0;
    // The frugal need a much fuller purse before the tavern feels right.
    let comfort = (params::TAVERN_COMFORT_PURSE as f32 * (0.5 + 1.5 * frugality)) as i64;
    if tavern_open && purse >= comfort {
        return Some((MealSource::Tavern, genome.price_supper));
    }
    let fish_ok = market.stall_fish > 0
        && roster.0.contains_key(&Profession::Fisher)
        && purse >= genome.price_fish;
    if fish_ok {
        return Some((MealSource::FishStall, genome.price_fish));
    }
    let bread_ok = market.bakery_bread > 0
        && roster.0.contains_key(&Profession::Baker)
        && purse >= genome.price_bread;
    if bread_ok {
        return Some((MealSource::Bakery, genome.price_bread));
    }
    if tavern_open && purse >= genome.price_supper {
        // Not rich, but the tavern is the only stocked option left.
        return Some((MealSource::Tavern, genome.price_supper));
    }
    None
}

/// Executes the purchase chosen by `meal_option`: money to the seller,
/// goods out of stock. Returns false if the trade fell through.
pub fn buy_meal(
    buyer: &str,
    source: MealSource,
    price: i64,
    market: &mut Market,
    ledger: &mut Ledger,
    roster: &Roster,
    stats: &mut DailyStats,
    clock: &SimClock,
    log: &mut EventLog,
) -> bool {
    let seller_job = match source {
        MealSource::Bakery => Profession::Baker,
        MealSource::FishStall => Profession::Fisher,
        MealSource::Tavern => Profession::TavernKeeper,
    };
    let Some(seller) = roster.0.get(&seller_job) else {
        return false;
    };
    // Price 0 = producer eating own stock; no coins move.
    if price > 0 && !ledger.transfer(buyer, seller, price) {
        return false;
    }
    match source {
        MealSource::Bakery => market.bakery_bread -= 1,
        MealSource::FishStall => market.stall_fish -= 1,
        MealSource::Tavern => {
            if market.tavern_bread > 0 {
                market.tavern_bread -= 1;
            } else {
                market.tavern_fish -= 1;
            }
            if price > 0 {
                stats.suppers += 1;
            }
        }
    }
    log.log(
        clock,
        &SimEvent::Purchase {
            buyer: buyer.to_string(),
            seller: seller.clone(),
            good: source.label().to_string(),
            price,
        },
    );
    true
}

/// Buys an ale for a tavern visit (the keeper drinks his own for free).
/// Returns whether the visit is a "paid" one. Frugal villagers need a
/// comfortable margin in the purse before they'll spend on drink.
#[allow(clippy::too_many_arguments)]
pub fn buy_ale(
    buyer: &str,
    profession: Profession,
    frugality: f32,
    purse: i64,
    market: &mut Market,
    ledger: &mut Ledger,
    roster: &Roster,
    genome: &Genome,
    stats: &mut DailyStats,
    clock: &SimClock,
    log: &mut EventLog,
) -> bool {
    if market.tavern_ale == 0 {
        return false;
    }
    if profession == Profession::TavernKeeper {
        market.tavern_ale -= 1;
        return true;
    }
    let Some(keeper) = roster.0.get(&Profession::TavernKeeper) else {
        return false;
    };
    let buffer = (frugality * 20.0) as i64;
    if purse < genome.price_ale + buffer {
        return false;
    }
    if !ledger.transfer(buyer, keeper, genome.price_ale) {
        return false;
    }
    market.tavern_ale -= 1;
    stats.ales_sold += 1;
    stats.total_ales += 1;
    log.log(
        clock,
        &SimEvent::Purchase {
            buyer: buyer.to_string(),
            seller: keeper.clone(),
            good: "ale".to_string(),
            price: genome.price_ale,
        },
    );
    true
}

/// Production effects of one completed work hour.
#[allow(clippy::too_many_arguments)]
pub fn complete_work_hour(
    name: &str,
    profession: Profession,
    market: &mut Market,
    ledger: &mut Ledger,
    roster: &Roster,
    genome: &Genome,
    rng: &mut SimRng,
    blight: bool,
    stats: &mut DailyStats,
    clock: &SimClock,
    log: &mut EventLog,
    memories: &mut crate::brain::Memories,
) {
    match profession {
        Profession::Farmer => {
            if !blight {
                market.farm_grain += params::GRAIN_PER_HOUR;
                stats.grain_grown += params::GRAIN_PER_HOUR;
            }
        }
        Profession::Farmhand => {
            if !blight {
                market.farm_grain += params::GRAIN_PER_HOUR;
                stats.grain_grown += params::GRAIN_PER_HOUR;
            }
            // Wage from the farmer's purse, per completed hour.
            if let Some(farmer) = roster.0.get(&Profession::Farmer) {
                if ledger.transfer(farmer, name, genome.farmhand_wage) {
                    log.log(
                        clock,
                        &SimEvent::WagePaid {
                            from: farmer.clone(),
                            to: name.to_string(),
                            amount: genome.farmhand_wage,
                        },
                    );
                } else {
                    log.log(
                        clock,
                        &SimEvent::UnpaidWage {
                            from: farmer.clone(),
                            to: name.to_string(),
                        },
                    );
                    // Working unpaid is the kind of thing one remembers.
                    crate::brain::remember(
                        memories,
                        name,
                        "worked_unpaid",
                        Some(farmer.as_str()),
                        -0.7,
                        clock,
                        log,
                    );
                }
            }
        }
        Profession::Fisher => {
            if !blight {
                let caught = rng
                    .0
                    .random_range(params::FISH_PER_HOUR_MIN..=params::FISH_PER_HOUR_MAX);
                market.stall_fish += caught;
                stats.fish_caught += caught;
            }
        }
        Profession::Baker => {
            // Buy grain from the farm as needed, then bake.
            let want = params::BREAD_PER_HOUR * params::GRAIN_PER_BREAD;
            if market.bakery_grain < want {
                if let Some(farmer) = roster.0.get(&Profession::Farmer) {
                    let short = want - market.bakery_grain;
                    let affordable =
                        (ledger.balance(name) / genome.price_grain).max(0) as u32;
                    let amount = short.min(market.farm_grain).min(affordable);
                    if amount > 0
                        && ledger.transfer(name, farmer, amount as i64 * genome.price_grain)
                    {
                        market.farm_grain -= amount;
                        market.bakery_grain += amount;
                        log.log(
                            clock,
                            &SimEvent::Purchase {
                                buyer: name.to_string(),
                                seller: farmer.clone(),
                                good: format!("grain x{amount}"),
                                price: amount as i64 * genome.price_grain,
                            },
                        );
                    }
                }
            }
            let baked =
                params::BREAD_PER_HOUR.min(market.bakery_grain / params::GRAIN_PER_BREAD);
            market.bakery_grain -= baked * params::GRAIN_PER_BREAD;
            market.bakery_bread += baked;
            stats.bread_baked += baked;
        }
        // Service/public jobs produce no goods (crime and politics come later).
        Profession::TavernKeeper | Profession::Guard | Profession::Mayor | Profession::Elder => {}
    }
}

/// Tavern restock: each morning the keeper buys the evening's stock from
/// the baker and the fisher.
pub fn tavern_restock(
    clock: Res<SimClock>,
    genome: Res<Genome>,
    mut market: ResMut<Market>,
    mut ledger: ResMut<Ledger>,
    roster: Res<Roster>,
    mut log: ResMut<EventLog>,
) {
    if clock.minute_of_day() != params::TAVERN_RESTOCK_HOUR * 60 {
        return;
    }
    let Some(keeper) = roster.0.get(&Profession::TavernKeeper).cloned() else {
        return;
    };
    // Business sense: never spend the tax/food reserve, and buy highest-
    // margin stock first (ale: 2c in, 6c out) — restocking low-margin
    // supper goods first was how the first keeper went bankrupt and died.
    let spendable = |ledger: &Ledger| {
        (ledger.balance(&keeper) - params::KEEPER_CASH_RESERVE).max(0)
    };

    // 1) Ale: buy grain from the farm, one grain per ale.
    let want_ale = params::TAVERN_TARGET_ALE.saturating_sub(market.tavern_ale);
    if want_ale > 0 {
        if let Some(farmer) = roster.0.get(&Profession::Farmer) {
            let affordable = (spendable(&ledger) / genome.price_grain) as u32;
            let grain = (want_ale * params::GRAIN_PER_ALE)
                .min(market.farm_grain)
                .min(affordable * params::GRAIN_PER_ALE);
            if grain > 0
                && ledger.transfer(&keeper, farmer, grain as i64 * genome.price_grain)
            {
                market.farm_grain -= grain;
                market.tavern_ale += grain / params::GRAIN_PER_ALE;
                log.log(
                    &clock,
                    &SimEvent::Purchase {
                        buyer: keeper.clone(),
                        seller: farmer.clone(),
                        good: format!("grain x{grain} (brewing)"),
                        price: grain as i64 * genome.price_grain,
                    },
                );
            }
        }
    }

    // 2) Fish (cheaper supper ingredient than bread), at wholesale.
    let want_fish = params::TAVERN_TARGET_FISH.saturating_sub(market.tavern_fish);
    if want_fish > 0 {
        if let Some(fisher) = roster.0.get(&Profession::Fisher) {
            let affordable = (spendable(&ledger) / genome.wholesale_fish) as u32;
            let amount = want_fish.min(market.stall_fish).min(affordable);
            if amount > 0
                && ledger.transfer(&keeper, fisher, amount as i64 * genome.wholesale_fish)
            {
                market.stall_fish -= amount;
                market.tavern_fish += amount;
                log.log(
                    &clock,
                    &SimEvent::Purchase {
                        buyer: keeper.clone(),
                        seller: fisher.clone(),
                        good: format!("fish x{amount}"),
                        price: amount as i64 * genome.wholesale_fish,
                    },
                );
            }
        }
    }

    // 3) Bread last, at wholesale.
    let want_bread = params::TAVERN_TARGET_BREAD.saturating_sub(market.tavern_bread);
    if want_bread > 0 {
        if let Some(baker) = roster.0.get(&Profession::Baker) {
            let affordable = (spendable(&ledger) / genome.wholesale_bread) as u32;
            let amount = want_bread.min(market.bakery_bread).min(affordable);
            if amount > 0
                && ledger.transfer(&keeper, baker, amount as i64 * genome.wholesale_bread)
            {
                market.bakery_bread -= amount;
                market.tavern_bread += amount;
                log.log(
                    &clock,
                    &SimEvent::Purchase {
                        buyer: keeper.clone(),
                        seller: baker.clone(),
                        good: format!("bread x{amount}"),
                        price: amount as i64 * genome.wholesale_bread,
                    },
                );
            }
        }
    }
}