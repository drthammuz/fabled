//! Money. One ledger holds every purse plus the treasury; all transfers go
//! through it, which makes the conservation invariant checkable: the sum of
//! all accounts never changes except by death-inheritance (also internal).
//! If the total ever drifts, there is a bug — the sim logs it loudly once.

use std::collections::BTreeMap;

use bevy::prelude::*;

use crate::clock::SimClock;
use crate::events::{EventLog, SimEvent};
use crate::genome::Genome;
use crate::npc::Npc;
use crate::params;
use crate::professions::Profession;

/// All money in the village. BTreeMap so iteration order is deterministic.
#[derive(Resource, Default)]
pub struct Ledger {
    pub accounts: BTreeMap<String, i64>,
    pub treasury: i64,
    /// Expected total, captured on the first conservation check.
    baseline: Option<i64>,
    violated: bool,
}

impl Ledger {
    pub fn total(&self) -> i64 {
        self.accounts.values().sum::<i64>() + self.treasury
    }

    pub fn balance(&self, name: &str) -> i64 {
        self.accounts.get(name).copied().unwrap_or(0)
    }

    /// Person-to-person payment. Fails (no-op) if the payer can't afford it.
    pub fn transfer(&mut self, from: &str, to: &str, amount: i64) -> bool {
        if self.balance(from) < amount {
            return false;
        }
        *self.accounts.entry(from.to_string()).or_insert(0) -= amount;
        *self.accounts.entry(to.to_string()).or_insert(0) += amount;
        true
    }

    /// Tax payment; collects at most what the payer has. Returns paid amount.
    pub fn collect_tax(&mut self, from: &str, amount: i64) -> i64 {
        let paid = amount.min(self.balance(from)).max(0);
        *self.accounts.entry(from.to_string()).or_insert(0) -= paid;
        self.treasury += paid;
        paid
    }

    /// Treasury payment. Fails (no-op) if the treasury can't afford it.
    pub fn pay_from_treasury(&mut self, to: &str, amount: i64) -> bool {
        if self.treasury < amount {
            return false;
        }
        self.treasury -= amount;
        *self.accounts.entry(to.to_string()).or_insert(0) += amount;
        true
    }

    /// Death: the estate goes to the treasury (money must not evaporate).
    pub fn inherit(&mut self, name: &str) -> i64 {
        let estate = self.accounts.remove(name).unwrap_or(0);
        self.treasury += estate;
        estate
    }
}

/// Every tick: total money must equal the baseline. Logs one loud error
/// event on the first violation instead of killing a long run.
pub fn conservation_check(
    mut ledger: ResMut<Ledger>,
    clock: Res<SimClock>,
    mut log: ResMut<EventLog>,
) {
    let total = ledger.total();
    match ledger.baseline {
        None => ledger.baseline = Some(total),
        Some(expected) => {
            if total != expected && !ledger.violated {
                ledger.violated = true;
                log.log(
                    &clock,
                    &SimEvent::MoneyConservationViolated { expected, actual: total },
                );
                log.flush();
                println!(
                    "[BUG] money conservation violated: expected {expected}, found {total}"
                );
            }
        }
    }
}

/// Daily fiscal event at FISCAL_HOUR: poll tax from everyone, then public
/// salaries and the pension. Shortfalls are logged, not papered over —
/// unpaid guards and broke treasuries are exactly the signals we watch for.
pub fn fiscal_day(
    clock: Res<SimClock>,
    genome: Res<Genome>,
    mut ledger: ResMut<Ledger>,
    mut log: ResMut<EventLog>,
    npcs: Query<(&Npc, &Profession)>,
) {
    if clock.minute_of_day() != params::FISCAL_HOUR * 60 {
        return;
    }

    for (npc, _) in &npcs {
        // Poll tax plus a wealth tax on large purses. The wealth tax is what
        // keeps coins circulating instead of pooling at the best earner.
        let wealth_tax = (ledger.balance(&npc.name) - genome.wealth_tax_threshold)
            .max(0)
            / genome.wealth_tax_divisor;
        let owed = genome.poll_tax + wealth_tax;
        let paid = ledger.collect_tax(&npc.name, owed);
        if paid < owed {
            log.log(
                &clock,
                &SimEvent::UnpaidTax {
                    npc: npc.name.clone(),
                    owed: owed - paid,
                },
            );
        }
    }

    for (npc, profession) in &npcs {
        let salary = match profession {
            Profession::Guard => genome.salary_guard,
            Profession::Mayor => genome.salary_mayor,
            Profession::Elder => genome.pension_elder,
            _ => continue,
        };
        if ledger.pay_from_treasury(&npc.name, salary) {
            log.log(
                &clock,
                &SimEvent::SalaryPaid {
                    npc: npc.name.clone(),
                    amount: salary,
                },
            );
        } else {
            log.log(&clock, &SimEvent::UnpaidSalary { npc: npc.name.clone() });
        }
    }
}
