//! The village policy genome: the economic knobs that were hand-tuned
//! through V2-V4 (prices, taxes, wages), now made evolvable. Every village
//! in an evolution run carries one of these; selection keeps the policies
//! whose villages thrive and mutates them for the next generation.

use bevy::prelude::*;
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use crate::params;

#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct Genome {
    pub price_grain: i64,
    pub price_bread: i64,
    pub price_fish: i64,
    pub price_ale: i64,
    pub price_supper: i64,
    pub wholesale_bread: i64,
    pub wholesale_fish: i64,
    pub poll_tax: i64,
    pub wealth_tax_threshold: i64,
    pub wealth_tax_divisor: i64,
    pub farmhand_wage: i64,
    pub salary_guard: i64,
    pub salary_mayor: i64,
    pub pension_elder: i64,
}

impl Default for Genome {
    /// The hand-tuned V4 policy — the baseline evolution has to beat.
    fn default() -> Self {
        Self {
            price_grain: params::PRICE_GRAIN,
            price_bread: params::PRICE_BREAD,
            price_fish: params::PRICE_FISH,
            price_ale: params::PRICE_ALE,
            price_supper: params::PRICE_SUPPER,
            wholesale_bread: params::WHOLESALE_BREAD,
            wholesale_fish: params::WHOLESALE_FISH,
            poll_tax: params::POLL_TAX,
            wealth_tax_threshold: params::WEALTH_TAX_THRESHOLD,
            wealth_tax_divisor: params::WEALTH_TAX_DIVISOR,
            farmhand_wage: params::FARMHAND_WAGE_PER_HOUR,
            salary_guard: params::SALARY_GUARD,
            salary_mayor: params::SALARY_MAYOR,
            pension_elder: params::PENSION_ELDER,
        }
    }
}

/// (accessor, min, max) for every evolvable field.
const FIELDS: [(fn(&mut Genome) -> &mut i64, i64, i64); 14] = [
    (|g| &mut g.price_grain, 1, 8),
    (|g| &mut g.price_bread, 2, 24),
    (|g| &mut g.price_fish, 2, 20),
    (|g| &mut g.price_ale, 2, 24),
    (|g| &mut g.price_supper, 4, 40),
    (|g| &mut g.wholesale_bread, 1, 24),
    (|g| &mut g.wholesale_fish, 1, 20),
    (|g| &mut g.poll_tax, 0, 20),
    (|g| &mut g.wealth_tax_threshold, 20, 400),
    (|g| &mut g.wealth_tax_divisor, 2, 40),
    (|g| &mut g.farmhand_wage, 1, 15),
    (|g| &mut g.salary_guard, 5, 80),
    (|g| &mut g.salary_mayor, 5, 80),
    (|g| &mut g.pension_elder, 5, 60),
];

impl Genome {
    /// A child policy: nudge a few knobs by up to ~±30% (at least 1 coin).
    pub fn mutate(&self, rng: &mut ChaCha8Rng) -> Self {
        let mut child = self.clone();
        for _ in 0..rng.random_range(1..=3) {
            let (field, min, max) = FIELDS[rng.random_range(0..FIELDS.len())];
            let value = field(&mut child);
            let factor: f64 = rng.random_range(0.7..1.3);
            let mut next = (*value as f64 * factor).round() as i64;
            if next == *value {
                next += if rng.random_range(0..2) == 0 { -1 } else { 1 };
            }
            *value = next.clamp(min, max);
        }
        // Wholesale must stay at or below retail or the tavern trade
        // becomes a money pump for the suppliers.
        child.wholesale_bread = child.wholesale_bread.min(child.price_bread);
        child.wholesale_fish = child.wholesale_fish.min(child.price_fish);
        child
    }

    /// One-line summary for evolution reports.
    pub fn brief(&self) -> String {
        format!(
            "bread {} fish {} ale {} supper {} | tax {} wealth>{}÷{} | wage {} guard {} mayor {} pension {}",
            self.price_bread,
            self.price_fish,
            self.price_ale,
            self.price_supper,
            self.poll_tax,
            self.wealth_tax_threshold,
            self.wealth_tax_divisor,
            self.farmhand_wage,
            self.salary_guard,
            self.salary_mayor,
            self.pension_elder,
        )
    }
}
