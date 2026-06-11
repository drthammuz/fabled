//! V5: multi-village evolution. N villages run in parallel, each under a
//! different policy genome (prices, taxes, wages). After a fixed horizon
//! every village is scored, the worst half is culled, and the survivors'
//! policies are mutated to refill the population. Within a generation every
//! village shares one world seed (same villagers, same weather), so fitness
//! differences are pure policy; the seed changes each generation, so a
//! policy must keep winning under new circumstances to stay in the pool.

use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use bevy::prelude::*;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::Serialize;

use crate::brain::Emotions;
use crate::economy::Ledger;
use crate::events::EventLog;
use crate::genome::Genome;
use crate::housing::Home;
use crate::npc::Npc;
use crate::build_app;
use crate::village::DailyStats;

pub struct EvolveConfig {
    pub villages: usize,
    pub generations: u32,
    /// Sim days each village runs per generation.
    pub days: u64,
    pub seed: u64,
    pub out_dir: PathBuf,
}

#[derive(Clone, Serialize)]
pub struct Outcome {
    pub score: f32,
    pub alive: usize,
    pub avg_mood: f32,
    pub avg_house_quality: f32,
    pub meals_per_day: f32,
    pub ales_per_day: f32,
    pub starving_per_day: f32,
    pub treasury: i64,
}

#[derive(Serialize)]
struct GenerationRecord<'a> {
    generation: u32,
    world_seed: u64,
    results: Vec<SlotRecord<'a>>,
}

#[derive(Serialize)]
struct SlotRecord<'a> {
    slot: usize,
    outcome: &'a Outcome,
    genome: &'a Genome,
}

/// Run one village to the horizon and score it.
///
/// Fitness: survival dominates (one death outweighs anything else a policy
/// can buy), then wellbeing (mood), housing progress, and a working economy
/// (meals eaten, ale sold), minus hunger crises.
pub fn run_village(seed: u64, policy: Genome, days: u64) -> Outcome {
    let mut app = build_app("evolve", seed, policy, EventLog::disabled());
    for _ in 0..days * 1440 {
        app.update();
    }

    let world = app.world_mut();
    let mut alive = 0usize;
    let (mut mood, mut house) = (0.0f32, 0.0f32);
    for (_, emotions, home) in world.query::<(&Npc, &Emotions, &Home)>().iter(world) {
        alive += 1;
        mood += emotions.mood;
        house += home.score.quality;
    }
    let n = alive.max(1) as f32;
    let stats = world.resource::<DailyStats>();
    let ledger = world.resource::<Ledger>();
    let days_f = days.max(1) as f32;

    let avg_mood = mood / n;
    let avg_house_quality = house / n;
    let meals_per_day = stats.total_meals as f32 / days_f;
    let ales_per_day = stats.total_ales as f32 / days_f;
    let starving_per_day = stats.total_starving_episodes as f32 / days_f;

    let score = alive as f32 * 200.0
        + avg_mood
        + avg_house_quality * 100.0
        + meals_per_day * 2.0
        + ales_per_day * 3.0
        - starving_per_day * 5.0;

    Outcome {
        score,
        alive,
        avg_mood: (avg_mood * 10.0).round() / 10.0,
        avg_house_quality: (avg_house_quality * 100.0).round() / 100.0,
        meals_per_day: (meals_per_day * 10.0).round() / 10.0,
        ales_per_day: (ales_per_day * 10.0).round() / 10.0,
        starving_per_day: (starving_per_day * 10.0).round() / 10.0,
        treasury: ledger.treasury,
    }
}

pub fn run_evolution(config: EvolveConfig) -> std::io::Result<()> {
    std::fs::create_dir_all(&config.out_dir)?;
    let wall_stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let log_path = config
        .out_dir
        .join(format!("evolution-seed{}-{}.jsonl", config.seed, wall_stamp));
    let mut log = std::io::BufWriter::new(std::fs::File::create(&log_path)?);

    println!(
        "evolution: {} villages x {} generations x {} days | master seed {}",
        config.villages, config.generations, config.days, config.seed
    );
    println!("fitness = alive*200 + mood + houses*100 + meals/d*2 + ales/d*3 - starving/d*5");

    // Generation 0: the hand-tuned default plus mutated variants of it.
    let mut population: Vec<Genome> = Vec::with_capacity(config.villages);
    population.push(Genome::default());
    let mut mutation_rng = ChaCha8Rng::seed_from_u64(config.seed ^ 0x5eed_f00d);
    while population.len() < config.villages {
        population.push(Genome::default().mutate(&mut mutation_rng));
    }

    let started = Instant::now();
    let mut champion: Option<(f32, Genome, Outcome)> = None;

    for generation in 1..=config.generations {
        // One shared world seed per generation: same villagers and weather
        // for every policy, so the comparison is apples to apples.
        let world_seed = config.seed.wrapping_mul(0x9e37_79b9).wrapping_add(generation as u64);

        let mut results: Vec<(usize, Outcome)> = std::thread::scope(|scope| {
            let handles: Vec<_> = population
                .iter()
                .enumerate()
                .map(|(slot, policy)| {
                    let policy = policy.clone();
                    let days = config.days;
                    scope.spawn(move || (slot, run_village(world_seed, policy, days)))
                })
                .collect();
            handles.into_iter().map(|h| h.join().expect("village thread panicked")).collect()
        });
        results.sort_by(|a, b| b.1.score.total_cmp(&a.1.score));

        println!("--- generation {generation} (world seed {world_seed}) ---");
        for (rank, (slot, outcome)) in results.iter().enumerate() {
            println!(
                "  #{:<2} score {:>7.1} | alive {} mood {:>5.1} houses {:.2} meals/d {:>4.1} ales/d {:>4.1} | {}",
                rank + 1,
                outcome.score,
                outcome.alive,
                outcome.avg_mood,
                outcome.avg_house_quality,
                outcome.meals_per_day,
                outcome.ales_per_day,
                population[*slot].brief(),
            );
        }

        let record = GenerationRecord {
            generation,
            world_seed,
            results: results
                .iter()
                .map(|(slot, outcome)| SlotRecord {
                    slot: *slot,
                    outcome,
                    genome: &population[*slot],
                })
                .collect(),
        };
        if let Ok(line) = serde_json::to_string(&record) {
            let _ = writeln!(log, "{line}");
        }
        log.flush()?;

        let (best_slot, best_outcome) = &results[0];
        if champion
            .as_ref()
            .is_none_or(|(best_score, ..)| best_outcome.score > *best_score)
        {
            champion = Some((
                best_outcome.score,
                population[*best_slot].clone(),
                best_outcome.clone(),
            ));
        }

        // Cull-and-reseed: top half survives unchanged, the bottom half is
        // replaced by mutated children of the survivors (round-robin).
        if generation < config.generations {
            let survivors = (config.villages / 2).max(1);
            let parents: Vec<Genome> = results[..survivors]
                .iter()
                .map(|(slot, _)| population[*slot].clone())
                .collect();
            let mut next: Vec<Genome> = parents.clone();
            let mut parent_index = 0usize;
            while next.len() < config.villages {
                next.push(parents[parent_index % parents.len()].mutate(&mut mutation_rng));
                parent_index += 1;
            }
            population = next;
        }
    }

    let (score, genome, outcome) = champion.expect("at least one generation ran");
    let best_path = config.out_dir.join("best_genome.json");
    std::fs::write(
        &best_path,
        serde_json::to_string_pretty(&serde_json::json!({
            "score": score,
            "outcome": outcome,
            "genome": genome,
        }))?,
    )?;

    println!("--- evolution complete ---");
    println!(
        "wall time:    {:.1}s ({} village-runs of {} days)",
        started.elapsed().as_secs_f64(),
        config.villages as u32 * config.generations,
        config.days
    );
    println!("champion:     score {score:.1} ({} alive, mood {:.1}, houses {:.2})",
        outcome.alive, outcome.avg_mood, outcome.avg_house_quality);
    println!("policy:       {}", genome.brief());
    println!("history:      {}", log_path.display());
    println!("best genome:  {}", best_path.display());
    Ok(())
}
