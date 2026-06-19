//! Spawn a new `fabled` process (editor ↔ playtest handoff) without a console flash.

use std::path::Path;
use std::process::Command;

/// Launch another mode and exit the current process on success.
pub fn relaunch_fabled(args: &[&str]) {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            bevy::log::warn!("could not find executable: {e}");
            return;
        }
    };
    match spawn_detached(&exe, args) {
        Ok(_) => {
            bevy::log::info!("launched {exe:?} {}", args.join(" "));
            std::process::exit(0);
        }
        Err(e) => bevy::log::warn!("failed to start {:?}: {e}", args),
    }
}

fn spawn_detached(exe: &Path, args: &[&str]) -> std::io::Result<std::process::Child> {
    let mut cmd = Command::new(exe);
    cmd.args(args);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Avoid a visible console window when handing off editor ↔ playtest.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
    }

    cmd.spawn()
}
