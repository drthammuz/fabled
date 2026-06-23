# Handover — faction asset system (2026-06-22)

Self-contained context for continuing the **faction architecture** work with no
prior chat history. The user verified the current state in the editor and it
"looked fine" (5 factions render clean, no colour bleed).

## TL;DR — where things are

The procgen now builds levels from **per-faction asset folders**. A transition
map runs three composition zones (`prev` → `default` → `next`), each bound to a
**faction profile**; each faction supplies its own GLBs + colormap + material.
**5 factions exist and all generate + render** (editor-confirmed):

| Faction (profile id) | Asset folder | Source kit | Look | Material mode |
|---|---|---|---|---|
| `industrial_default` | `factions/industrial/` | dungeon (floor only) | gray space walls + brown **dirt floor** | floor = native_glb; walls = space `SpaceCyber` |
| `priesthood` | `factions/priesthood/` | dungeon | **stone** walls + corridors | explicit `Priesthood` material |
| `synth` | `factions/synth/` | space_station | sleek **station** walls (thick) | native_glb, scale 4 |
| `outlaw` *(="urban")* | `factions/urban/` | retro_urban | **cyberpunk** brick/asphalt | native_glb, scale 4 |
| `necropolis` | `factions/necropolis/` | graveyard | **gothic** brick walls | native_glb, scale 4 |

Editor default transition: **outlaw(urban) → industrial → priesthood**.
`space_default` (a legacy "pink" faction) was **retired** this session.

**Read these first** (the detailed, current source of truth):
- **`docs/faction_assets.md`** — the asset-folder + manifest system, schema, how
  the generator & client consume it, calibration, "adding a new faction".
- **`docs/faction_roster.md`** — the 5-faction sort across all 13 Kenney kits,
  scale notes, and the remaining build order.

## How it fits together (the pipeline)

1. **Manifest** `assets/models/factions/<id>/faction.json` (loader:
   `tools/faction_assets.py`) declares: which faction `profiles` it serves,
   `source_kit`, `scale`, `material.{mode,client_slot,…}`, and per-role
   (`floor`/`wall`/`corridor`) `stem` + calibration (`yaw_offset`, `inset`,
   `floorless_corner`) + `provides` (which roles route to this folder; unprovided
   roles fall back to the space grammar).
2. **Generator** `tools/gen_freeform.py::build_zone_kits(comp)` (called in
   `to_doc` before emit) resolves each zone's faction → per-role kit/stem/scale/
   yaw/inset into the `_ACTIVE_ZONE_*` globals. `_piece` + emit helpers
   (`add_floor`/`add_wall`/`add_corner`) read them. **So the look follows the
   faction** — reorder factions and the visuals move with them. A faction with no
   manifest falls back to its `building_system` kit (legacy whole-kit behaviour).
3. **Client** `crates/client/src/test_showcase.rs::kenney_material_slot(kit, …)`
   maps a piece's `kit` → `KenneyMaterialSlot`; the editor bake
   (`kenney_editor.rs::editor_apply_materials`) applies the matching material.
   Explicit materials are built in `init_editor_kenney_materials`.

Kit string = folder name: `glb_asset_path_in_kit(stem, "factions/urban")` →
`models/factions/urban/{stem}.glb`. Nested path works.

## Calibration (fixing alignment WITHOUT renders)

The newer kits are authored at **1-unit** scale vs space/dungeon's **4-unit**
cell, so faction manifests set `scale: 4`. Two per-piece knobs correct placement
(measure with `pygltflib`, no renders needed):
- **`inset`** (wall): if a wall's depth-axis bbox centre is off-zero by `c`, set
  `inset = -c * scale` to push it to the cell edge. Example: necropolis
  `brick-wall` z-centre −0.35 → `inset: 1.4`.
- **`yaw_offset`** (wall/corner, radians): if a piece faces wrong vs the space
  `template-wall`/`corridor-corner` the `WALL_YAW`/`CORNER_YAW` tables target,
  add a multiple of π/2. Example: synth `wall-corner` → `yaw_offset: 1.5708`.

Known-good reference: `space_station/wall` (depth centre 0) renders flush.

## Critical gotchas (these cost real time)

1. **Don't trust Blender-re-exported glTF materials** — Bevy often fails to
   resolve their texture binding → **white** pieces. Use an **explicit** material
   (load the folder colormap) for any re-exported GLB, not `NativeGlb`. Original
   (un-edited) Kenney GLBs are fine via `NativeGlb`. Tools:
   `tools/repair_priesthood_glb.py`, `tools/glb_externalize_texture.py`.
2. **Floor/roof audits are role-aware** — every piece carries a stamped `role`
   (`gen_freeform._piece`). Faction floors use custom stems (e.g. urban
   `road-asphalt-center`) that the OLD stem-prefix checks missed, causing
   hub-ceiling audit failures. `_piece_floor_surface(p)`/`_piece_solid_floor(p)`
   classify by role. `PieceRecord` has no `deny_unknown_fields`, so the extra
   `role` field is safe for the client.
3. **Colour must be FACTION-driven, never ZONE-driven.** The retired pink bug
   keyed materials on `zone == "prev"`, so any faction in the spawn zone went
   pink. `kenney_material_slot` now routes purely by `kit`. Ceilings + fallback
   floors are neutral gray for all zones.
4. **Floorless corner**: priesthood's `corridor-corner.glb` had its floor mesh
   removed; faction-folder corners (urban/synth/necropolis) likewise provide
   walls only. `floorless_corner: true` → the generator lays a `template-floor`
   tile under the corner. Floor-detection is kit-aware so this doesn't trip the
   duplicate-floor audit.
5. **Full `cargo build` needs cmd.exe/PowerShell (not Git Bash — `link.exe`
   conflict) AND the editor closed.** `cargo check -p client` works in either.

## Map model — user refinements (2026-06-22)

These override the naive "three equal faction zones" mental model. Full spec in
`docs/procgen-faction-manifest.md` §2.2–2.3, §4.3, §5.4b.

### Substrate vs camps

- **Industrial** (`industrial_default`) is the **default substrate** — sewers,
  ventilation, subway, anything that normally exists underground. It is not one
  of the four "faction camps"; it is connective tissue.
- The **four other factions** each have **camps** (stations between maps). The
  camp hub is built in that faction's architecture. When the player leaves a
  camp, the **next level starts in that faction's buildings**; when they
  approach the next camp, that faction's buildings appear again at the end.
- Realistic level fractions along spawn→extract are **~15% / 60% / 35%**
  (prev-faction / industrial / next-faction), not equal thirds. Current
  `level_composition.py` defaults are 25/50/25 — treat 15/60/35 as the target
  preset once the zone planner lands.

### Transitions must use entrances, not instant interiors

**Current behaviour (wrong):** `gen_freeform` paints prev/default/next kits
along the spine; the player steps from industrial straight into enclosed faction
rooms with no door read.

**Target behaviour:**

1. The level is **technically built on industrial**; faction architecture
   attaches at transition nodes through **doors / entrances**, not by swapping
   the whole grid interior-first.
2. Transition path is **faction-dependent:**
   - **Higher-tech** factions (e.g. synth / space_station) → more likely an
     **outdoor or exterior** buffer (plaza, station apron) before indoor halls.
   - **Lower-tier / rogue** factions (e.g. outlaw) → buildings **directly on
     dirt**; industrial substrate may **continue for several tiles** after
     faction walls begin (no landscaped outdoor).
3. Every faction entrance from industrial should include a **door frame** and,
   where the kit provides them, **small entrance stairs** (space_station:
   `stairs-small-*` straight and rounded variants) to **elevate** buildings
   above the substrate floor — learn these pieces for every hand-off.
4. Camps mirror this: approaching a camp, faction buildings appear; the camp
   itself is that faction's hub architecture, entered through the same entrance
   vocabulary.

### Props — not random scatter

Props are per-faction and must respect **setup patterns**, not uniform random
placement. Examples: office = desk + computer clusters; cantina = table groups;
some setups need **special room shapes** or **spine-centre corridor slots**;
outdoor setups may need a **specific courtyard shape** or **buildings
surrounding an open centre**. See manifest §5.4b (`structural archetypes`) and
§5.1 `prop_setups` — implement with the props pass, not before Phase 1 Kenney
layout is trustworthy.

Implemented (2026-06-22): `tools/transition_entrances.py` — doors + entrance
stairs at industrial↔faction spine boundaries; `transition` block on faction
profiles; substrate overlap (`substrate_overlap_cells`) for
`direct_on_substrate` factions; default zone weights 15/60/35 (normalized).

## Remaining work (roster build order)

1. **Props system** (biggest gap): faction-specific **setup patterns** (not random
   scatter) using each faction's dressing kit — `factory`/`space_kit`
   (industrial), `retro_fantasy`/`castle` (priesthood), `furniture`/computers
   (synth), `retro_urban` details/trucks (urban), `graveyard`
   gravestones/crypts/lanterns (necropolis). None placed yet.
2. **Castle pass** for Priesthood (fortress walls/towers/gates from `castle/`).
3. **Platformer slopes** as a shared verticality utility (`platformer/` ramps).
4. **Faction-driven ceilings**: ceilings are currently always neutral
   (kit=None). To give a faction a themed/coloured roof, stamp the faction on
   ceiling pieces and route in `kenney_material_slot`.
5. **Cleanup**: remove dormant pink materials (`SpaceIndustrial`/`CeilingPink`
   slots + `CyberMaterialIndustrial`/`CyberMaterialPinkCeiling`) — unused since
   pink retired; harmless but dead.
6. **Per-faction quirks** (flagged): space_station has multiple lower-stair
   variants needing advanced multi-piece stair logic; necropolis walls are short
   (~3/4 height — kit limitation, left as placeholder per user).
7. **Market pack** for the hub shop was requested but **no market zip is in
   `assets/downloads/`** — ask the user to (re)add it.

## Verify / build / run

```bash
# regenerate a transition map (any faction ordering)
python tools/gen_freeform.py --seed 1 --prev-faction outlaw \
  --next-faction priesthood --default-faction industrial_default \
  --out userinput/maps/_t.json --no-layout-export

python tools/validate_factions.py        # manifests vs files + profiles
python tools/faction_assets.py           # list loaded manifests
cargo check -p client                    # Git Bash OK
```
Editor: Map mode → **Proc** tab to regen; cycle a zone's faction (the editor
shells out to `gen_freeform.py --preview`). Faction list lives in
`editor_map_gen.rs::FACTION_PROFILES` (5 entries).

## Key files

- `tools/gen_freeform.py` — generator; `build_zone_kits`, `_piece`, emit helpers,
  role-aware floor audits, `FACTION_ZONE_*` resolution.
- `tools/faction_assets.py` / `tools/validate_factions.py` — manifest API +
  validator. `tools/extract_kenney_kits.py` — kit extractor (13 kits wired).
- `tools/bake_tinted_colormap.py`, `tools/recolor_priesthood.py` — colormap ops.
- `crates/client/src/test_showcase.rs` — `kenney_material_slot`,
  `init_editor_kenney_materials`, `KenneyMaterialSlot`.
- `crates/client/src/kenney_editor.rs` — `editor_apply_materials` (bake).
- `crates/client/src/editor_map_gen.rs` — `FACTION_PROFILES`, default settings.
- `assets/models/factions/<id>/{faction.json,catalogue.md,Textures/,*.glb}`.
- `userinput/factions/*.json` — faction procgen profiles (5).

## Conventions

- **Don't commit unless asked.** Match surrounding Rust/Bevy patterns.
- The 5 faction names/themes are **provisional** — the user sorts by available
  assets and may rename/move kits between factions later (the folder structure
  makes that cheap). `outlaw` profile currently drives the `urban` assets (name
  mismatch is intentional/provisional).
- The user cannot be assumed to want big refactors of working visuals; confirm
  before mass changes. They verify renders in the editor — flag what needs eyes.
