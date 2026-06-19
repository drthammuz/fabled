# Fabled

Multiplayer game prototype (Rust / Bevy).

## Development

```bat
cargo build
cargo run -- --host --editor   REM Kenney editor
cargo run -- --host --test     REM test map
```

Kenney layout: `userinput/kenney_layout.json`. Hub branch maps: `userinput/maps/`.

### Hub / extraction geometry (known broken)

Editor **G** playtest around the extraction pit, hub floor −1, west door, and L2 stairs has **repeated visual vs physics mismatches**. Agent fix attempts are paused.

**Read first:** [docs/hub-extraction-agent-failures.md](docs/hub-extraction-agent-failures.md)  
**Agent rules:** [AGENTS.md](AGENTS.md)

**Bugbot:** Reviews **pull requests** on GitHub once the repo is connected in the [Cursor Bugbot dashboard](https://cursor.com/docs/bugbot). Open a PR (push a branch first); trigger manually with `bugbot run` on the PR if your setting is “only when mentioned”. PR rules: [`.cursor/BUGBOT.md`](.cursor/BUGBOT.md). Hub context: [docs/hub-extraction-agent-failures.md](docs/hub-extraction-agent-failures.md). Local pre-push: `/review-bugbot` in Cursor.

```bat
python tools/probe_hub_tile_audit.py
python tools/probe_hub_exits.py userinput/maps/level_stretch.json
```

Other known issues: [errors.md](errors.md).
