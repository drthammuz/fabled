# Agent instructions (Fabled)

## ⛳ Update docs/TODO.md EVERY pass (not optional)

`docs/TODO.md` is the single hand-off between sessions. The human relies on it
to know it is safe to clear the chat/cache. At the **end of every pass**:

1. Move anything you finished into a **"Done (recent)"** section with a one-line
   what + where (file/fn), newest first.
2. Update **"In progress / next"** so the next agent can start cold — what is
   left, the exact files, and any decision already made.
3. Keep it **skimmable**: short bullets, group by area, prune stale/duplicate
   lines. Detailed narrative belongs in a `docs/handover-*.md`, not TODO.
4. If you leave work unfinished, say so explicitly under "In progress" with the
   next concrete step.

The player also keeps design notes in **`docs/fromuser/`** — read them before
class/gameplay work and reflect decisions into TODO.

## Game vision — read before player-facing features (roles, quests, rep, chapters)

Author-stated design (professions, faction camps, reputation, prologue → campaign → open world): **[docs/game-pitch.md](docs/game-pitch.md)**. Implementation still largely ahead of this doc; use it for intent, not current behaviour.

## Synth dressing sandbox — read before interior / balcony / mezzanine work

**Master plan (START HERE):** [docs/synth-master-plan.md](docs/synth-master-plan.md) — roadmap, **open issues O1–O15**, **procgen problem history**, acceptance checklist, **Critical Context section (full history of manual feedback cost + all specific rules for windows/stairs/floors/relations/density)**, and **Automation-First Plan** (script/CPU approach). **Handover:** [docs/handover-synth-2026-06-23.md](docs/handover-synth-2026-06-23.md) §7 (now references the automation context). **Transition seam only:** [docs/synth-transition-architecture.md](docs/synth-transition-architecture.md) (D-table).

**Before touching synth_interior.py, placement_catalog, probe_synth_catalog.py (or probe_faction_catalog.py), decorate, furnish, or faction_interior.py:** read the Critical Context + Automation Plan in synth-master-plan.md so the expensive manual-per-object loop is never repeated. All future catalog/relation work must be script-driven. For all-faction interiors see the 2026-06-29 handover.

**Dressing workflow** — launch **`dressing.bat`** (not `editor.bat`). Saves under `userinput/synth_dressing/`. The same automation principles (probe + script rules + sweeps) were extended to all factions (see handover 2026-06-29). After generator or catalog changes (and always after GLB edits):

```bash
python tools/probe_synth_catalog.py
python tools/gen_dressing_showcase.py
python tools/verify_synth_placement.py userinput/synth_dressing/*.json
# plus free-space / rule checks per Automation Plan
```

**Live procgen interior** (editor Map → Proc tab, cells=25) — same placement rules in `tools/synth_interior.py`. After changes:

```bash
python tools/test_synth_interior_rules.py
python tools/gen_maps.py --seed 42 --probe
```

Rebuild client after Rust editor/playtest changes (`cargo build -p client`; tag **`2026-06-24g`**). Proc regen alone does not pick up Y-sync fixes.

Placement source of truth: `tools/synth_interior.py` + `assets/models/factions/synth/placement_catalog.json` (enriched by probe + relations) + the full rules in synth-master-plan.md#critical-context. The catalog + rules must be script-probed so hundreds of items do not require manual per-object feedback.

## Faction asset system — read FIRST if continuing faction / per-faction architecture work

**Latest handover (2026-06-22): [docs/handover-factions-2026-06-22.md](docs/handover-factions-2026-06-22.md)** — self-contained context with no prior chat needed. Covers the per-faction asset-folder pipeline, the 5 current factions (industrial, priesthood, synth, outlaw/urban, necropolis), calibration (scale/yaw_offset/inset), critical gotchas (Blender white-bug, role-aware floor audits, faction-driven colour), and the remaining roster (props system, castle pass, slopes). Detailed refs: [docs/faction_assets.md](docs/faction_assets.md), [docs/faction_roster.md](docs/faction_roster.md).

**Faction interior placement automation handover (this session, 2026-06-29):** [docs/handover-faction-interior-2026-06-29.md](docs/handover-faction-interior-2026-06-29.md) — user's initial automation goals (script-driven catalog + relations + no manual loops), what was built (probe, unified placer, sweeps, bats, wall/corridor attempts), recurring complaints (overlaps, bench facing/backrest, roadblocks in corridors, grave rows, priesthood ruins logic, cross-faction, lack of probe auto-detection), and what remained unfixed. Read after the 2026-06-22 handover. Always re-read synth-master-plan Critical Context + Automation Plan before touching placement/catalog code.

## Procedural map generation — read before touching `gen_maps`, tile synthesis, or editor Proc tab

Kenney map procgen is mid-refactor: **tile synthesis only**, **no room GLBs**, **mission graph not yet implemented**.

**Before changing `tools/gen_maps.py`, `tools/gen_modules.py` (strat_planned / synthesis), `editor_map_gen.rs`, `level_composition.py`, `gen_freeform.py`, `tools/synth_interior.py`, `tools/synth_transition.py`, or Kenney layout generation:**

1. Read **[docs/procgen-faction-manifest.md](docs/procgen-faction-manifest.md)** — start at **§0 Agent handoff** if you have no prior chat context. It defines phased delivery, current vs target state, and the **Phase 1** task (spine + branches + role-based synthesis).
2. For **multi-faction zone paint, prev/next adjacency, or per-faction corridor/room requirements**, read **[docs/procgen-zone-composition.md](docs/procgen-zone-composition.md)** — current pipeline vs manifest layer order, options, and recommendations.
3. For **synth interior furnish, balconies, or mezzanine**, read **[docs/synth-master-plan.md](docs/synth-master-plan.md)** (open issues + problem history) — not the manifest Phase 1 scope alone.
3. **Scope:** Phase 1 = Kenney layout quality only. Do not wire faction profiles, industrial merge, or camp transitions until Phase 1 exit criteria in the manifest are met.
4. After map gen changes, run:
   - `python tools/gen_maps.py --seed 42 --probe`
   - `python tools/probe_layout_decor.py userinput/kenney_layout.json`
5. Editor: Map mode → sidebar **Proc** tab for live regen (`python tools/gen_maps.py --preview`).

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
| **Synth master plan & interior roadmap (START HERE for synth)** | [docs/synth-master-plan.md](docs/synth-master-plan.md) — incl. open issues + procgen problem history + **Critical Context (pain + rules)** + **Automation Plan (scripts/CPU)** |
| Synth handover (transition D-table + dressing §7) | [docs/handover-synth-2026-06-23.md](docs/handover-synth-2026-06-23.md) |
| Synth transition & elevated deck (rules + D-table) | [docs/synth-transition-architecture.md](docs/synth-transition-architecture.md) |
| **Faction asset system (latest handover, 2026-06-22)** | [docs/handover-factions-2026-06-22.md](docs/handover-factions-2026-06-22.md) |
| Faction interior placement automation (this session) | [docs/handover-faction-interior-2026-06-29.md](docs/handover-faction-interior-2026-06-29.md) |
| Faction asset folders / manifest schema | [docs/faction_assets.md](docs/faction_assets.md) |
| Faction roster (5 factions × 13 kits) | [docs/faction_roster.md](docs/faction_roster.md) |
| Procgen / factions / Phase 1 task | [docs/procgen-faction-manifest.md](docs/procgen-faction-manifest.md) |
| **Zone composition (paint order, adjacency, spatial options)** | [docs/procgen-zone-composition.md](docs/procgen-zone-composition.md) |
| **Game pitch (author vision: roles, camps, rep, release chapters)** | [docs/game-pitch.md](docs/game-pitch.md) |
| Hub / extraction failures | [docs/hub-extraction-agent-failures.md](docs/hub-extraction-agent-failures.md) |
| Kenney GLB catalogue | [docs/kenney_space_kit.md](docs/kenney_space_kit.md) |
| Game loop / milestones | [update2.md](update2.md) |
