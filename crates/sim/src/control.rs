//! Operator console: a stdin reader thread feeding commands to the runner
//! loop, so a running sim can be paused, inspected, and stopped in an
//! orderly way (all logs flushed, final snapshot + report written) instead
//! of being killed with Ctrl+C.

use std::sync::mpsc::{self, Receiver};
use std::thread;

pub enum Command {
    Pause,
    Resume,
    /// New pacing in ticks/second; 0 = as fast as possible.
    Speed(f64),
    Status,
    Snapshot,
    /// Print every purse plus the treasury.
    Wealth,
    /// Toggle the blight (experiments): true = farm/fishing yield nothing.
    SetBlight(bool),
    Stop,
    Help,
    Unknown(String),
}

pub fn parse(line: &str) -> Option<Command> {
    let mut parts = line.split_whitespace();
    let word = parts.next()?;
    Some(match word {
        "pause" | "p" => Command::Pause,
        "resume" | "r" => Command::Resume,
        "speed" | "s" => match parts.next() {
            Some("max") => Command::Speed(0.0),
            Some(value) => match value.parse::<f64>() {
                Ok(tps) if tps >= 0.0 => Command::Speed(tps),
                _ => Command::Unknown(line.to_string()),
            },
            None => Command::Unknown(line.to_string()),
        },
        "status" | "st" => Command::Status,
        "snapshot" | "snap" => Command::Snapshot,
        "wealth" | "w" => Command::Wealth,
        "blight" => match parts.next() {
            Some("on") => Command::SetBlight(true),
            Some("off") => Command::SetBlight(false),
            _ => Command::Unknown(line.to_string()),
        },
        "stop" | "quit" | "q" => Command::Stop,
        "help" | "h" | "?" => Command::Help,
        _ => Command::Unknown(line.to_string()),
    })
}

pub const HELP: &str = "\
commands:
  status (st)      tick, sim time, speed, event count
  pause (p)        freeze the sim
  resume (r)       continue after pause
  speed <tps|max>  set pacing in ticks/sec (1 tick = 1 sim minute); max = uncapped
  snapshot (snap)  write a full state snapshot now
  wealth (w)       print every purse and the treasury
  blight <on|off>  while on, farming/fishing yield nothing (famine experiment)
  stop (q)         orderly shutdown: final snapshot, flush logs, report
  help (?)         this text";

/// Reads stdin lines on a background thread. If stdin closes (piped runs),
/// the channel disconnects and the runner just stops polling it.
pub fn spawn_console() -> Receiver<Command> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in std::io::stdin().lines() {
            let Ok(line) = line else { break };
            if let Some(command) = parse(&line) {
                if tx.send(command).is_err() {
                    break;
                }
            }
        }
    });
    rx
}
