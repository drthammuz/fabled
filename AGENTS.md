# Agent instructions (Fabled)

## Procedural map generation — read before touching `gen_maps`, tile synthesis, or editor Proc tab

Kenney map procgen is mid-refactor: **tile synthesis only**, **no room GLBs**, **mission graph not yet implemented**.

**Before changing `tools/gen_maps.py`, `tools/gen_modules.py` (strat_planned / synthesis), `editor_map_gen.rs`, or Kenney layout generation:**

1. Read **[docs/procgen-faction-manifest.md](docs/procgen-faction-manifest.md)** — start at **§0 Agent handoff** if you have no prior chat context. It defines phased delivery, current vs target state, and the **Phase 1** task (spine + branches + role-based synthesis).
2. **Scope:** Phase 1 = Kenney layout quality only. Do not wire faction profiles, industrial merge, or camp transitions until Phase 1 exit criteria in the manifest are met.
3. After map gen changes, run:
   - `python tools/gen_maps.py --seed 42 --probe`
   - `python tools/probe_layout_decor.py userinput/kenney_layout.json`
4. Editor: Map mode → sidebar **Proc** tab for live regen (`python tools/gen_maps.py --preview`).

Long-term procgen (multi-faction, sewer/Kenney transitions) is spec’d in the manifest §2–6; **do not implement Phase 2+ while Phase 1 is open** unless the user explicitly redirects.

## Hub / extraction — read before touching Kenney hub geometry

The hub, extraction pit, west corridor, and L2 stairs area has had **many failed agent iterations** with visual vs physical mismatches in editor **G** playtest.

**Before changing `kenney_pit`, `kenney_hub`, `editor_playtest`, floor cutouts, hatch pieces, or playtest sync:**

1. Read **[docs/hub-extraction-agent-failures.md](docs/hub-extraction-agent-failures.md)** — summary of the last five user prompts, reported symptoms, and why prior agent work lost trust.
2. Run **`python tools/probe_hub_tile_audit.py`** and verify **in-game G playtest** on the tiles listed in that doc. Probes passing alone is not sufficient.
3. Prefer **Bugbot review** on hub-related diffs before merge (see below).

Procgen Phase 1 work should **not** modify hub/extraction unless probes regress — keep hub changes separate.

## Bugbot

**GitHub PR reviews (the product Bugbot):** Enabled in [Cursor → Bugbot dashboard](https://cursor.com/docs/bugbot) after connecting the repo. It runs on **pull requests**, not on local commits alone.

1. Connect GitHub repo in Cursor dashboard → enable Bugbot for this repository.
2. Push a branch and **open a PR** (or draft PR).
3. Bugbot reviews automatically on each push **unless** your personal/team setting is “Run only when mentioned” — then comment on the PR: `bugbot run` or `cursor review`.
4. Project rules for PR reviews live in **[`.cursor/BUGBOT.md`](.cursor/BUGBOT.md)** (always included). Hub context: **[docs/hub-extraction-agent-failures.md](docs/hub-extraction-agent-failures.md)**. Procgen context: **[docs/procgen-faction-manifest.md](docs/procgen-faction-manifest.md)**.

**Local pre-push review:** `/review-bugbot` in Cursor agent reviews branch changes before you push; if the diff matches the PR, GitHub Bugbot may skip duplicate work (see [Cursor Bugbot docs](https://cursor.com/docs/bugbot#run-in-your-agent)).

For hub / extraction / Kenney playtest visual–physics work, Bugbot should use the failure log above. Do not merge hub fixes without a PR review and probe commands listed in that doc.

## General

- Do not commit unless the user asks.
- Match existing Rust / Bevy patterns in surrounding code.
- Probes in `tools/` are the intended pre-flight checks for map and hub geometry.
- **One thing at a time** on procgen: manifest phases are ordered; do not skip ahead.

## Doc index (quick)

| Topic | Doc |
|-------|-----|
| Procgen / factions / Phase 1 task | [docs/procgen-faction-manifest.md](docs/procgen-faction-manifest.md) |
| Hub / extraction failures | [docs/hub-extraction-agent-failures.md](docs/hub-extraction-agent-failures.md) |
| Kenney GLB catalogue | [docs/kenney_space_kit.md](docs/kenney_space_kit.md) |
| Game loop / milestones | [update2.md](update2.md) |
