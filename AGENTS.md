# Agent instructions (Fabled)

## Hub / extraction — read before touching Kenney hub geometry

The hub, extraction pit, west corridor, and L2 stairs area has had **many failed agent iterations** with visual vs physical mismatches in editor **G** playtest.

**Before changing `kenney_pit`, `kenney_hub`, `editor_playtest`, floor cutouts, hatch pieces, or playtest sync:**

1. Read **[docs/hub-extraction-agent-failures.md](docs/hub-extraction-agent-failures.md)** — summary of the last five user prompts, reported symptoms, and why prior agent work lost trust.
2. Run **`python tools/probe_hub_tile_audit.py`** and verify **in-game G playtest** on the tiles listed in that doc. Probes passing alone is not sufficient.
3. Prefer **Bugbot review** on hub-related diffs before merge (see below).

## Bugbot

**GitHub PR reviews (the product Bugbot):** Enabled in [Cursor → Bugbot dashboard](https://cursor.com/docs/bugbot) after connecting the repo. It runs on **pull requests**, not on local commits alone.

1. Connect GitHub repo in Cursor dashboard → enable Bugbot for this repository.
2. Push a branch and **open a PR** (or draft PR).
3. Bugbot reviews automatically on each push **unless** your personal/team setting is “Run only when mentioned” — then comment on the PR: `bugbot run` or `cursor review`.
4. Project rules for PR reviews live in **[`.cursor/BUGBOT.md`](.cursor/BUGBOT.md)** (always included). Hub context: **[docs/hub-extraction-agent-failures.md](docs/hub-extraction-agent-failures.md)**.

**Local pre-push review:** `/review-bugbot` in Cursor agent reviews branch changes before you push; if the diff matches the PR, GitHub Bugbot may skip duplicate work (see [Cursor Bugbot docs](https://cursor.com/docs/bugbot#run-in-your-agent)).

For hub / extraction / Kenney playtest visual–physics work, Bugbot should use the failure log above. Do not merge hub fixes without a PR review and probe commands listed in that doc.

## General

- Do not commit unless the user asks.
- Match existing Rust / Bevy patterns in surrounding code.
- Probes in `tools/` are the intended pre-flight checks for map and hub geometry.
