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
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();
    sim::run(sim::SimConfig {
        village: args.village,
        seed: args.seed,
        speed_tps: args.speed,
        stop_after_days: args.days,
        out_dir: args.out_dir,
    })
}
