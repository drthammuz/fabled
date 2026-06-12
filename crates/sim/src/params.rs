//! All V1 gameplay tunables. Rates are expressed per sim hour for human
//! readability; systems divide by 60 to get the per-tick (per-minute) value.
//! Needs and health all live on a 0..100 scale.

/// Per-NPC hunger-rate multiplier range; desynchronizes schedules so the
/// village doesn't eat and sleep in lockstep.
pub const METABOLISM_MIN: f32 = 0.85;
pub const METABOLISM_MAX: f32 = 1.15;
/// Range for the 0..1 personality traits (sociability/diligence/frugality).
pub const TRAIT_MIN: f32 = 0.05;
pub const TRAIT_MAX: f32 = 0.95;

// --- Brain (V3) ---

/// Mood baseline and misery pressure (per hour).
pub const MOOD_BASELINE: f32 = 50.0;
pub const MOOD_MISERY_PER_HOUR: f32 = 3.0;
pub const MOOD_LONELY_PER_HOUR: f32 = 1.5;
/// Memories kept per NPC; salience multiplier per day; forget floor.
pub const MEMORY_CAP: usize = 32;
pub const MEMORY_DECAY_PER_DAY: f32 = 0.92;
pub const MEMORY_MIN_SALIENCE: f32 = 0.05;

// --- Social need (V3) ---

/// Base loneliness growth per hour, scaled by (0.5 + sociability).
pub const SOCIAL_PER_HOUR: f32 = 3.0;
/// Minutes spent socializing at the tavern/square.
pub const SOCIALIZE_MINUTES: u64 = 90;
/// Social relief: with a bought ale at the tavern vs. loitering for free.
/// Loitering is shallow on purpose — if free chitchat fully satisfied the
/// need, nobody would ever pay the tavern (verified: ale sales died).
pub const SOCIAL_RELIEF_ALE: f32 = 60.0;
pub const SOCIAL_RELIEF_LOITER: f32 = 12.0;
/// Extra relief when at least one other villager is socializing too.
pub const SOCIAL_FRIEND_BONUS: f32 = 15.0;
/// Mood bumps.
pub const MOOD_MEAL: f32 = 4.0;
pub const MOOD_SUPPER: f32 = 8.0;
pub const MOOD_SOCIAL_ALE: f32 = 12.0;
pub const MOOD_SOCIAL_LOITER: f32 = 6.0;

// Tavern ale (the social economy that keeps the keeper alive).
pub const PRICE_ALE: i64 = 8;
pub const TAVERN_TARGET_ALE: u32 = 12;
/// Grain brewed into one ale at restock.
pub const GRAIN_PER_ALE: u32 = 1;

// --- Need rates (per sim hour) ---

/// Hunger growth. ~11/h crosses the eat threshold about 3x per day.
pub const HUNGER_PER_HOUR: f32 = 11.0;
/// Metabolism slows in sleep: hunger growth multiplier while asleep.
/// Keeps a villager who ate supper asleep through the night; only those
/// who went to bed already hungry wake up for a midnight meal.
pub const HUNGER_ASLEEP_FACTOR: f32 = 0.35;
/// Energy drain while awake. ~4.7/h means a 06:00-21:00 day costs ~70
/// energy: tired by evening, but never forced into an afternoon nap on a
/// normal schedule. (At 6.25/h the village free-ran on a ~20h cycle with
/// mass 16:00 naps — verified in 30-day runs.)
pub const ENERGY_DRAIN_PER_HOUR: f32 = 4.7;
/// Energy recovery while sleeping; full recharge in ~8h.
pub const ENERGY_SLEEP_PER_HOUR: f32 = 12.5;
/// Outdoor warmth change is (temp_c - 10) * this, per hour: warm days
/// restore, cold days drain.
pub const WARMTH_OUTDOOR_FACTOR: f32 = 0.8;
/// Warmth recovery per hour while indoors (eat/sleep/idle).
pub const WARMTH_INDOOR_PER_HOUR: f32 = 10.0;
/// Warmth recovery per hour at the village fire (WarmUp action).
pub const WARMTH_FIRE_PER_HOUR: f32 = 25.0;

// --- Health (per sim hour) ---

/// Health drain while hunger is maxed (starving). Humans survive weeks
/// without food: 100 / 0.25 = 400h ≈ 17 days from empty stomach to death.
/// (The *urge* to eat still dominates decisions within hours — only dying
/// is slow.)
pub const STARVE_HEALTH_PER_HOUR: f32 = 0.25;
/// Health drain while warmth is zero (freezing).
pub const FREEZE_HEALTH_PER_HOUR: f32 = 5.0;
/// Health recovery while fed and warm.
pub const HEAL_PER_HOUR: f32 = 2.0;

// --- Actions ---

/// How much hunger one meal removes.
pub const MEAL_RELIEF: f32 = 70.0;
/// Hunger level at which eating becomes attractive.
pub const EAT_THRESHOLD: f32 = 50.0;
/// Forage yield range (food units per completed run), inclusive.
pub const FORAGE_YIELD_MIN: u32 = 2;
pub const FORAGE_YIELD_MAX: u32 = 4;

/// Action durations in sim minutes.
pub const EAT_MINUTES: u64 = 30;
pub const FORAGE_MINUTES: u64 = 180;
pub const WARMUP_MINUTES: u64 = 60;
pub const IDLE_MINUTES: u64 = 60;
pub const STROLL_MINUTES: u64 = 45;

/// A stroll around the village: light mood lift, no money involved.
/// Mostly an off-duty alternative to idling at home, so the streets
/// aren't empty. Social relief is kept near zero so strolling never
/// substitutes for the tavern (the service economy depends on lonely
/// people buying ale).
pub const MOOD_STROLL: f32 = 2.0;
pub const SOCIAL_RELIEF_STROLL: f32 = 2.0;

/// Daytime window: hour in [DAY_START, NIGHT_START) counts as day.
pub const DAY_START_HOUR: u64 = 6;
pub const NIGHT_START_HOUR: u64 = 22;

// --- Economy (V2). All money in copper coins. ---

/// Coins each villager starts with.
pub const STARTING_PURSE: i64 = 60;
/// Village treasury at sim start.
pub const TREASURY_START: i64 = 500;
/// Universal daily poll tax, collected at FISCAL_HOUR. Kept low: at 12c it
/// ate ~50% of a laborer's income, killed all discretionary spending, and
/// starved the service economy (the wealth tax funds the treasury instead).
pub const POLL_TAX: i64 = 5;
/// Daily wealth tax: 1/WEALTH_TAX_DIVISOR of the purse above the threshold.
/// Recirculates hoarded coin through the treasury back into salaries —
/// without it the economy demand-collapses (verified empirically, day 20).
pub const WEALTH_TAX_THRESHOLD: i64 = 50;
pub const WEALTH_TAX_DIVISOR: i64 = 10;
/// Hour of day when taxes are collected and salaries paid.
pub const FISCAL_HOUR: u64 = 9;
/// Daily public salaries / pension, paid from the treasury.
pub const SALARY_GUARD: i64 = 35;
pub const SALARY_MAYOR: i64 = 40;
pub const PENSION_ELDER: i64 = 24;
/// Farmhand wage, paid by the farmer per completed work hour.
pub const FARMHAND_WAGE_PER_HOUR: i64 = 5;
/// Cash the tavern keeper won't spend on stock (tax + food buffer —
/// without it one bad day starts an unrecoverable death spiral).
pub const KEEPER_CASH_RESERVE: i64 = 20;
/// Bulk prices the tavern pays when restocking. At retail (8 + 6 = 14c of
/// ingredients for a 15c supper) the supper trade was a 1c-margin business
/// whose stock the keeper also ate — the bankruptcy that kept killing him.
pub const WHOLESALE_BREAD: i64 = 5;
pub const WHOLESALE_FISH: i64 = 4;

// Prices (fixed in V2; dynamic pricing is a later milestone). Staples are
// kept cheap relative to wages so ordinary villagers have a small surplus —
// without discretionary income the service economy (tavern) starves.
pub const PRICE_GRAIN: i64 = 2;
pub const PRICE_BREAD: i64 = 8;
pub const PRICE_FISH: i64 = 6;
pub const PRICE_SUPPER: i64 = 15;
/// Purse level above which a villager treats the tavern as affordable.
pub const TAVERN_COMFORT_PURSE: i64 = 40;

// Production.
/// Grain consumed per loaf of bread.
pub const GRAIN_PER_BREAD: u32 = 2;
/// Grain produced per work hour (farmer and farmhand each).
pub const GRAIN_PER_HOUR: u32 = 4;
/// Loaves the baker can bake per work hour (grain permitting).
pub const BREAD_PER_HOUR: u32 = 4;
/// Fish caught per work hour, inclusive range (rng).
pub const FISH_PER_HOUR_MIN: u32 = 2;
pub const FISH_PER_HOUR_MAX: u32 = 3;
/// Tavern's target evening stock, bought from baker/fisher each morning.
pub const TAVERN_TARGET_BREAD: u32 = 3;
pub const TAVERN_TARGET_FISH: u32 = 3;
/// Hour the tavern keeper restocks.
pub const TAVERN_RESTOCK_HOUR: u64 = 10;
/// Supper serving window.
pub const TAVERN_OPEN_HOUR: u64 = 17;
pub const TAVERN_CLOSE_HOUR: u64 = 22;
/// The taproom serves drinks from late morning (a village tavern does not
/// survive on five hours of trade a day).
pub const TAVERN_DRINKS_FROM_HOUR: u64 = 11;

/// One work block in sim minutes; production lands on completion.
pub const WORK_MINUTES: u64 = 60;

// --- Space and travel (V4) ---

/// Walking speed: ~5 km/h.
pub const WALK_METERS_PER_MINUTE: f32 = 83.0;
/// Number of home positions on the ring around the square.
pub const HOME_RING_COUNT: usize = 8;
/// Work utility bonus when the purse is nearly empty (the broke hustle).
pub const BROKE_PURSE: i64 = 15;
pub const BROKE_WORK_BONUS: f32 = 12.0;

// --- Housing (V4) ---

/// Open floor cells (of 16) that feel right for one villager.
pub const HOUSE_IDEAL_OPEN: usize = 12;
/// Material costs (coins): per open cell, per wall cell, per insulation
/// point per cell.
pub const HOUSE_COST_FLOOR: i64 = 2;
pub const HOUSE_COST_WALL: i64 = 3;
pub const HOUSE_COST_INSULATION: i64 = 4;
/// Hour of day when villagers consider home improvements.
pub const HOUSE_PLANNING_HOUR: u64 = 14;
/// Mutated plans drafted per villager per day.
pub const HOUSE_VARIANTS_PER_DAY: usize = 6;
/// Minimum quality gain (0..1) worth a rebuild.
pub const HOUSE_MIN_IMPROVEMENT: f32 = 0.04;
/// Coins kept aside (food, taxes — and ale money: construction must not
/// crowd out the consumption the service economy lives on) before
/// spending on materials.
pub const HOUSE_BUILD_RESERVE: i64 = 70;
/// Each villager only considers a rebuild every N days (staggered), so
/// construction stays an occasional investment, not a daily money sink.
pub const HOUSE_PLANNING_INTERVAL_DAYS: u64 = 7;
/// How much of the indoor warmth benefit a zero-shelter hovel still gives.
pub const HOME_SHELTER_FLOOR: f32 = 0.25;

// Demand response: producers stop working when their stock is full.
/// Farmer/farmhand stay home when the farm holds this much grain.
pub const FARM_GRAIN_CAP: u32 = 80;
/// Baker stops baking at this bread stock.
pub const BAKERY_BREAD_CAP: u32 = 30;
