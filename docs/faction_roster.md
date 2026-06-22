# Faction roster — asset-driven sort (5 factions)

Factions are defined by the Kenney assets we actually have (themes/names are
provisional and movable — the `factions/<id>/` folder structure makes copying or
relocating assets between factions cheap). 13 model kits extracted (1370 GLB).

## Scale note (critical for integration)

The original `space/` + `dungeon/` kits are authored at a **4-unit cell** (floor
4×4, wall 4 tall). **Every newer kit is ~1-unit** (castle/retro_urban/graveyard/
space_station walls ≈1 tall, floor/road tiles 1×1). They integrate into the
4-unit generator grid via the manifest `scale: 4` field (now supported). Vertical
flush-alignment + which-stem-per-role still need an editor pass per kit.

## The 5 factions

| # | Faction (provisional) | Architecture kit | Scale | Prop / dressing kits | Status |
|---|---|---|---|---|---|
| 1 | **Industrial** (default substrate) | `space/` (4-unit, wired) + procedural sewer; dirt floor from `dungeon/` | 1 | `factory/` (machinery, pipes) · `city_industrial/` (concrete buildings, chimneys — set-pieces) · `space_kit/` (monorail) | floor+walls live; props pending placement |
| 2 | **Priesthood / Order** (stone fortress) | `dungeon/` (4-unit stone, wired) + optional `castle/` (walls, towers, gates) | dungeon 1 / castle 4 | `retro_fantasy/` · `castle/` banners, flags, siege | live; castle is an upgrade pass |
| 3 | **Synth / Station** (clean hi-tech) | `space_station/` (1-unit → scale 4) | 4 | `furniture/` (modern domestic) · station computers/displays | **BUILT** (`factions/synth/`) — thick walls+corners; floor falls back to flush space floor; needs editor tuning (stairs quirk later) |
| 4 | **Urban / Cyber** (gritty cyberpunk city) | `retro_urban/` (1-unit → scale 4): 50 walls, roads, balconies, scaffolding, windows | 4 | `building/` (brick) · `retro_urban/` details, trucks | **BUILT** (`factions/urban/`) — data-complete, generates on all orderings; needs editor visual tuning (yaw/scale/material) |
| 5 | **Necropolis / Cult** (gothic death) | `graveyard/` (1-unit → scale 4): crypts, mausoleum, brick walls, iron fences, pillars | 4 | `graveyard/` coffins, lanterns, gravestones, candles, altars | **BUILT** (`factions/necropolis/`) — thick brick walls; floor falls back to flush space floor; rich props in-folder; needs editor tuning |

## Not factions (utility / shared)

- **`platformer/`** (153 GLB; block ×81, platforms, slopes, ladders, crates,
  spikes, conveyors) — terrain/verticality utility usable by ANY faction; the
  **slopes/ramps** answer the user's organic-geometry question. Not a faction.
- **`prototype/`** — greybox blockout only.
- **`space_kit/`** — exterior sci-fi props + monorail (Industrial props).
- **Hub / market** — a `market` pack was mentioned for the hub (purchasing) but
  **no market zip is in `assets/downloads/`** — needs (re)adding before wiring.

## Architectural quirks needing advanced procgen logic (flagged by user)

- `space_station/` has several **lower-stair** variants (`stairs-small-*`,
  `stairs-ramp`, `stairs-corner-*`) — faction-specific multi-piece stair logic.
- 1-unit kits need per-faction **scale** (have it) + **vertical offset** so their
  thinner/shorter floors sit flush with 4-unit zones at transition boundaries.
- Kits lacking a `corridor-corner` equivalent (space_station, retro_urban,
  graveyard) need a **corner-from-tiles** fallback in the generator (build the
  L-bend from a floor + two walls instead of a dedicated corner GLB).

## space_default retired (2026-06-22)

The legacy "pink" `space_default` faction has been **removed** — it was an
arbitrary tint placeholder from before real factions, and its zone-based pink was
bleeding onto whatever faction sat in the spawn zone. The 5 factions above are the
roster. Editor default transition is now **outlaw(urban) → industrial → priesthood**.

## Build order (each = folder + faction.json + colormap + material slot + editor tune)

1. **Urban/Cyber** (`retro_urban`) — best cyberpunk fit, richest wall set; proves
   the 1-unit-scale + corner-from-tiles path.
2. **Necropolis** (`graveyard`) — distinctive, similar integration to #1.
3. **Synth** (`space_station`) — adds the stair-quirk logic.
4. **Castle** pass for Priesthood; **city_industrial/factory** props for Industrial.
5. **Platformer** slopes as a shared verticality utility.
