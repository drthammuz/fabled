# Fabled

A co-op multiplayer extraction game prototype (Rust / Bevy). You and up to 7
friends drop into procedurally generated faction sectors, fight through, and
extract.

> **Building this yourself? AI agents:** all contributor/agent rules, the doc
> index, and the "update `docs/TODO.md` every pass" workflow live in
> **[AGENTS.md](AGENTS.md)**. Current status & backlog: **[docs/TODO.md](docs/TODO.md)**.

---

## Play with friends (the short version)

**You (the host):**
1. In **cmd.exe** at the repo root: `serve_and_play_random.bat` — builds, hosts,
   and drops you into a random level.
2. To let friends join over the internet, the easy path is **Tailscale** (free);
   give them the `100.x.y.z` address it shows you.
3. To give friends the game without them installing anything: run
   **`pack_client.bat`**. It makes `fabled_client.zip` (game + art + a `PLAY.bat`)
   and, if you've set up the GitHub CLI once, uploads it to your repo's
   **Releases → "playtest"** so there's always one download link.

**Your friends:**
1. Download `fabled_client.zip`, unzip it anywhere.
2. Double-click **`PLAY.bat`**, type the host's address, press Enter. Done — no
   code, no GitHub, no Rust.

**Full step-by-step (with firewall/Tailscale help and troubleshooting):**
**[docs/playtest-online-2026-07-07.md](docs/playtest-online-2026-07-07.md).**

---

## Running from source (dev)

Build and run from **cmd.exe** (Git Bash's `link.exe` breaks the final link):

```bat
cargo build
serve_and_play_random.bat        REM real game, host + play, random map
cargo run -- --host --editor     REM Kenney map editor
cargo run -- --host --test       REM flat test map
join.bat <host-ip>               REM build + join a host from source
```

Maps: `userinput/maps/pool/` (real game, built by `tools/build_map_pool.py`);
editor layout `userinput/kenney_layout.json`.

Known issues: [errors.md](errors.md).
