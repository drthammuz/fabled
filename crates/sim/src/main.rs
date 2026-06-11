use clap::Parser;
use std::path::PathBuf;

/// Headless village simulation. Runs without rendering or networking;
/// control it from this console (type 'help') and read the JSONL event
/// log + snapshots it writes to --out-dir.
#[derive(Parser)]
#[command(name = "sim")]
struct Args {
    /// RNG seed. Same seed = byte-identical run.
    #[arg(long, default_value_t = 42)]
    seed: u64,

    /// Village name, used in log/snapshot file names.
    #[arg(long, default_value = "greenfield")]
    village: String,

    /// Pacing in ticks/second (1 tick = 1 sim minute). 0 = as fast as possible.
    #[arg(long, default_value_t = 60.0)]
    speed: f64,

    /// Stop automatically after N full sim days. 0 = run until 'stop'.
    #[arg(long, default_value_t = 0)]
    days: u64,

    /// Directory for event logs and snapshots.
    #[arg(long, default_value = "sim_out")]
    out_dir: PathBuf,

    /// Evolution mode: run N villages in parallel for several generations,
    /// culling the worst policies and mutating the best.
    #[arg(long, default_value_t = false)]
    evolve: bool,

    /// Evolution: villages per generation.
    #[arg(long, default_value_t = 8)]
    villages: usize,

    /// Evolution: number of generations.
    #[arg(long, default_value_t = 10)]
    generations: u32,

    /// Run with a policy genome from a file (e.g. best_genome.json from an
    /// evolution run) instead of the built-in default policy.
    #[arg(long)]
    genome: Option<PathBuf>,
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();
    if args.evolve {
        return sim::evolve::run_evolution(sim::evolve::EvolveConfig {
            villages: args.villages.max(2),
            generations: args.generations.max(1),
            days: if args.days == 0 { 60 } else { args.days },
            seed: args.seed,
            out_dir: args.out_dir,
        });
    }
    let policy = match &args.genome {
        Some(path) => {
            let text = std::fs::read_to_string(path)?;
            let json: serde_json::Value = serde_json::from_str(&text)?;
            // Accept both a bare genome and the best_genome.json wrapper.
            let genome_value = json.get("genome").cloned().unwrap_or(json);
            let genome: sim::genome::Genome = serde_json::from_value(genome_value)
                .map_err(std::io::Error::other)?;
            println!("policy from {}: {}", path.display(), genome.brief());
            genome
        }
        None => sim::genome::Genome::default(),
    };
    sim::run(sim::SimConfig {
        village: args.village,
        seed: args.seed,
        speed_tps: args.speed,
        stop_after_days: args.days,
        out_dir: args.out_dir,
        policy,
    })
}
