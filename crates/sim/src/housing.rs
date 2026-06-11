//! Parametric houses — the first "construction without LLMs" loop. A house
//! is a small genome: a 4x4 cell floor plan (open / interior wall), a door,
//! and an insulation level. It is scored on shelter (warmth retention),
//! navigability (BFS from the door — unreachable cells are wasted, and the
//! "can they get stuck?" question is answered by an actual path check),
//! and comfort (right-sized, some interior structure). Villagers improve
//! their homes by mutate-score-rebuild hill climbing when they can afford
//! it, so house quality climbs over the weeks — visibly faster for the rich.

use bevy::prelude::*;
use rand::Rng;
use serde::Serialize;

use crate::clock::SimClock;
use crate::economy::Ledger;
use crate::events::{EventLog, SimEvent};
use crate::npc::Npc;
use crate::params;
use crate::weather::SimRng;

pub const GRID: usize = 4;
pub const CELLS: usize = GRID * GRID;

#[derive(Clone, Serialize)]
pub struct HouseGenome {
    /// true = interior wall/furniture; false = open floor.
    pub blocked: [bool; CELLS],
    /// Door cell index, always on the perimeter and always open.
    pub door: usize,
    /// 0..1 — how well the house holds warmth.
    pub insulation: f32,
}

#[derive(Clone, Copy, Serialize)]
pub struct HouseScore {
    pub shelter: f32,
    pub navigability: f32,
    pub comfort: f32,
    /// Weighted total in 0..1.
    pub quality: f32,
    /// Material cost in coins to build this plan.
    pub cost: i64,
}

/// A villager's home: fixed position, evolving plan.
#[derive(Component)]
pub struct Home {
    pub pos: Vec2,
    pub genome: HouseGenome,
    pub score: HouseScore,
    /// Staggers planning days across the village.
    pub planning_offset: u64,
}

fn perimeter(cell: usize) -> bool {
    let (x, y) = (cell % GRID, cell / GRID);
    x == 0 || y == 0 || x == GRID - 1 || y == GRID - 1
}

/// Reachable open cells from the door (4-neighbor BFS).
fn reachable_cells(genome: &HouseGenome) -> usize {
    let mut seen = [false; CELLS];
    let mut queue = vec![genome.door];
    seen[genome.door] = true;
    let mut count = 0;
    while let Some(cell) = queue.pop() {
        count += 1;
        let (x, y) = (cell % GRID, cell / GRID);
        let mut push = |nx: i32, ny: i32| {
            if (0..GRID as i32).contains(&nx) && (0..GRID as i32).contains(&ny) {
                let neighbor = ny as usize * GRID + nx as usize;
                if !seen[neighbor] && !genome.blocked[neighbor] {
                    seen[neighbor] = true;
                    queue.push(neighbor);
                }
            }
        };
        push(x as i32 - 1, y as i32);
        push(x as i32 + 1, y as i32);
        push(x as i32, y as i32 - 1);
        push(x as i32, y as i32 + 1);
    }
    count
}

pub fn score(genome: &HouseGenome) -> HouseScore {
    let open = genome.blocked.iter().filter(|blocked| !**blocked).count();
    let walls = CELLS - open;

    let shelter = genome.insulation;
    let navigability = if open == 0 {
        0.0
    } else {
        reachable_cells(genome) as f32 / open as f32
    };
    // Comfort peaks at the ideal open size; some interior structure helps.
    let size_fit =
        1.0 - (open as f32 - params::HOUSE_IDEAL_OPEN as f32).abs() / params::HOUSE_IDEAL_OPEN as f32;
    let comfort = size_fit.clamp(0.1, 1.0);

    let quality = shelter * 0.5 + navigability * 0.3 + comfort * 0.2;
    let cost = open as i64 * params::HOUSE_COST_FLOOR
        + walls as i64 * params::HOUSE_COST_WALL
        + (genome.insulation * CELLS as f32 * params::HOUSE_COST_INSULATION as f32) as i64;
    HouseScore {
        shelter,
        navigability,
        comfort,
        quality,
        cost,
    }
}

/// The starting hovel: thin walls, randomly cluttered floor plan.
pub fn initial_shack(rng: &mut SimRng) -> HouseGenome {
    let mut blocked = [false; CELLS];
    for cell in &mut blocked {
        *cell = rng.0.random_range(0.0..1.0) < 0.25;
    }
    let door = loop {
        let cell = rng.0.random_range(0..CELLS);
        if perimeter(cell) {
            break cell;
        }
    };
    blocked[door] = false;
    HouseGenome {
        blocked,
        door,
        insulation: rng.0.random_range(0.10..0.30),
    }
}

/// One mutated variant: a few random edits to the plan.
fn mutate(genome: &HouseGenome, rng: &mut SimRng) -> HouseGenome {
    let mut variant = genome.clone();
    for _ in 0..rng.0.random_range(1..=3) {
        match rng.0.random_range(0..4) {
            0 | 1 => {
                // Toggle a non-door cell.
                let cell = rng.0.random_range(0..CELLS);
                if cell != variant.door {
                    variant.blocked[cell] = !variant.blocked[cell];
                }
            }
            2 => {
                // Move the door to another open perimeter cell.
                let cell = rng.0.random_range(0..CELLS);
                if perimeter(cell) && !variant.blocked[cell] {
                    variant.door = cell;
                }
            }
            _ => {
                variant.insulation = (variant.insulation
                    + rng.0.random_range(-0.08..0.15))
                .clamp(0.05, 0.95);
            }
        }
    }
    variant
}

/// Daily home improvement (one consideration per villager per day): draft a
/// few variants, keep the best, rebuild if it's clearly better and the
/// purse allows. Materials are bought from the village commons (treasury).
pub fn improve_homes(
    clock: Res<SimClock>,
    mut ledger: ResMut<Ledger>,
    mut rng: ResMut<SimRng>,
    mut log: ResMut<EventLog>,
    mut homes: Query<(&Npc, &mut Home)>,
) {
    if clock.minute_of_day() != params::HOUSE_PLANNING_HOUR * 60 {
        return;
    }
    for (npc, mut home) in &mut homes {
        if (clock.day() + home.planning_offset) % params::HOUSE_PLANNING_INTERVAL_DAYS != 0 {
            continue;
        }
        // Draft variants regardless of wealth (the rng stream must not
        // depend on purse state more than necessary, and dreaming is free).
        let mut best: Option<(HouseGenome, HouseScore)> = None;
        for _ in 0..params::HOUSE_VARIANTS_PER_DAY {
            let variant = mutate(&home.genome, &mut rng);
            let variant_score = score(&variant);
            if best
                .as_ref()
                .is_none_or(|(_, best_score)| variant_score.quality > best_score.quality)
            {
                best = Some((variant, variant_score));
            }
        }
        let Some((genome, new_score)) = best else {
            continue;
        };
        if new_score.quality < home.score.quality + params::HOUSE_MIN_IMPROVEMENT {
            continue;
        }
        let budget = ledger.balance(&npc.name) - params::HOUSE_BUILD_RESERVE;
        if budget < new_score.cost {
            continue;
        }
        ledger.collect_tax(&npc.name, new_score.cost); // materials from the commons
        log.log(
            &clock,
            &SimEvent::HouseImproved {
                npc: npc.name.clone(),
                quality_from: (home.score.quality * 100.0).round() / 100.0,
                quality_to: (new_score.quality * 100.0).round() / 100.0,
                shelter: (new_score.shelter * 100.0).round() / 100.0,
                navigability: (new_score.navigability * 100.0).round() / 100.0,
                cost: new_score.cost,
            },
        );
        home.genome = genome;
        home.score = new_score;
    }
}
