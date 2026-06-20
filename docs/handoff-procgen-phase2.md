# Session handoff — procgen Phase 2 (knobs + editor UI) & Kenney kit library

**Date:** 2026-06-20
**Branch:** `freeform/ceiling-roofs`
**Context:** read this + `docs/procgen-faction-manifest.md` (the living spec) + `docs/kenney_kits_catalogue.md` before continuing. Memory index: `~/.claude/.../memory/MEMORY.md`.

---

## What happened this session (7 commits)

| Commit | Summary |
|--------|---------|
| `c8a4ee3` | **Floor-collider fall-through FIX** + 8-kit Kenney library (901 GLB) + faction/procgen spec |
| `4740e84` | `generate_kenney_catalog.py --kit`; generated `dungeon/` catalogue |
| `62bcc33` | `gen_freeform` **organicness** knob + §5.4b faction archetypes note |
| `562ed1f` | `gen_freeform` **corridor_width** knob (1.0–2.0 fractional) |
| `84539e5` | `gen_freeform` **hidden_area_prevalence** knob (generation half) |
| `c2faa3b` | `gen_maps.py --preview` forwards the real knobs (was dropping them) |
| `f9d3841` | **Editor Proc panel** rewired to real knobs + slider-style fill bars |

Working tree is clean; transient generator dumps (`gen_map_*.json`, crash logs) are now gitignored.

---

## 1. Kenney asset library (done, usable)

8 new kits extracted + prepared under `assets/models/` (`building`, `dungeon`, `factory`, `furniture`, `prototype`, `retro_fantasy`, `space_kit`, `space_station`) via `tools/extract_kenney_kits.py` (idempotent; re-run after dropping zips in `assets/downloads/`). Index: `assets/models/kenney_kits_index.json`. Full inventory + the committed **3-faction mapping**: `docs/kenney_kits_catalogue.md`.

- **GOTCHA:** `space/` = the wired modular-space-kit; `space_kit/` = the *original* (exterior props) — different kits.
- `dungeon/` shares the space modular grammar → it has a mesh-measured catalogue already: `assets/models/dungeon/kenney_catalog.json`. **Not yet wired into runtime** (`shared::kenney_catalog` still hard-loads `space/`).
- Factions (working names, manifest §5.5): **priesthood** = `dungeon` + `retro_fantasy`; **synth** = `space_station` + `furniture`; **outlaw** = `building`; **default industrial** = procedural sewer + `factory`/`space_kit` dressing. Props are **per-faction**, not shared (modern furniture ≠ stone faction).

## 2. The floor-collider fall-through fix (don't regress this)

Symptom: player fell through solid interior floor tiles in editor playtest. Root cause: the ceiling work rerouted walkable `template-floor` tiles from a **baked trimesh** collider to the **KenneyFloorCell cuboid** path; cuboids had full coverage but the avian SpatialQuery the server character casts against never caught the player (suspected registration/timing vs `PostUpdate`-baked trimeshes). **Fix = restored the b611031 trimesh path** (the one corridors use). Kept the ceiling handling (`is_ceiling_slab` → no collider). Detail: `memory/reference_kenney_floor_colliders.md`. **Lesson:** when a collider provably exists+positioned-right but the player passes through, suspect physics registration, not geometry — prefer the proven path.

## 3. Procgen knobs (gen_freeform.py) — implemented + editor-wired

All RNG-safe (default value = byte-identical to old output). CLI: `python tools/gen_freeform.py --seed N --show [--organicness X --corridor-width Y --hidden Z --rooms R --loops L --cells C]`.

| Knob | Range | Effect | Status |
|------|-------|--------|--------|
| cells / rooms / loops | ints | grid / room count / shortcut count | ✅ pre-existing |
| `organicness` | 0–1 | clean L → jogged Z corridors | ✅ |
| `corridor_width` | 1.0–2.0 | fraction of corridors that are 2-wide (1.3 = ~30%); wide = room-style floor+walls | ✅ |
| `hidden_area_prevalence` | 0–1 | single-entrance dead-end rooms | ✅ **generation only** |

Editor: **Proc** tab → Layout / Feel / Secrets / Advanced groups; `−/+` buttons with slider-style fill bars. Chain: panel → `tools/gen_maps.py --preview` → `gen_freeform.run`. UI is `−/+` buttons (not drag-sliders — deliberate; build drag-sliders only with the editor running to iterate).

## 4. Catalogue generalised

`tools/generate_kenney_catalog.py --kit <folder>` (default `space`). Run on `dungeon` → catalogue verified to match the space grammar. `space` output byte-identical.

---

## NEXT / open backlog (priority-ish)

1. **Hidden-room secret-door mechanic (RUNTIME, Rust)** — user-flagged: the generated hidden room is *open*, so not actually secret. Spec: left-click the entrance wall → both visual mesh and collider **slide aside** to seal/reveal.

   **ATTEMPTED + REVERTED (commit `a049b65`, reverted by `445ee61`).** What was learned (re-use this; the reverted commit is in history if you want the diff):
   - **Kenney doors have baked glTF animations.** `gate-door.glb` / `gate-door-window.glb` (space + dungeon), `factory/door.glb`, `building/door-rotate-*` all ship clips named **`open` / `close`** (space uses `open-close`) on a child node `door`. Drive them with bevy `AnimationPlayer` exactly like `crates/client/src/character_animation.rs` (build an `AnimationGraph` from `gltf.named_animations`, `Added<AnimationPlayer>` → walk up `ChildOf` to the piece, `AnimationTransitions::play`). The prototype matched the placed scene because the graph loads the *same* GLB (`glb_asset_path("gate-door")` = `models/space/gate-door.glb`).
   - **Approach that worked visually:** generator emitted a `gate-door` piece at each hidden entrance (`place_hidden_rooms` computed doorway world pos + yaw `0` if N/S corridor else `π/2`; `to_doc` appended the piece). A `door_anim.rs` plugin opened it on camera proximity. User confirmed it looked good (minor: door landed 1 tile off / inside a wall on some seeds — the entrance-cell/yaw calc needs hardening).
   - **WHY REVERTED — UNRESOLVED:** with doors present, **invisible colliders appeared in "many locations"** (like the earlier ceiling-collider class of bug, see `memory/reference_kenney_floor_colliders.md`). Cause NOT identified. Puzzle: `gate-door` has `collide_default=false` in the catalog so it should get **no** collider, and the only changes were (a) emitting `gate-door` pieces and (b) the client-only `door_anim` plugin (no colliders). Suspects to check FIRST next time: does the runtime actually read `collide_default=false` for `gate-door` (else it bakes the door's huge −9 m→+4.6 m trimesh)? does emitting a non-walkable piece at a wall-edge perturb `kenney_mesh_covers_cell` / the floor-cuboid pass in `level.rs`? Reproduce with `--hidden 0.6`, then bisect: doors-but-no-anim-plugin vs anim-plugin-but-no-doors.
   - **Collider-seal is still TODO regardless:** `gate-door` has no collider, so even working it wouldn't *seal* the room — you'd need to add a closed-state collider that's removed on open (and network the state).
2. **room-edge jitter** — finishes `organicness` (irregular non-rectangular rooms). Connectivity-risky; keep corridor attach cells intact. Python.
3. **multi-floor** (`floor_preference`) — vertical levels/shafts. Bigger. Python + collider/ceiling care.
4. **Faction profile binding** (manifest Phase 2/3) — `FactionProcgenProfile` struct/JSON holding the §5.1 knob values; wire `dungeon`/`default` profiles; per-kit catalogue runtime loading (un-hardcode `space/`).
5. **Editor tuning UI polish** — user wants a properly responsive UI (`memory/feedback_procgen_ui.md`); true drag-sliders + the per-faction / level-composition panel from manifest §5.3 (two faction blocks + blend). Build with the editor running.
6. **§5.4b asset-gated faction archetypes** (e.g. priesthood inner-garden) — later, data-driven.

## How to run / verify

- **Editor:** `editor.bat` (always rebuilds; kills running `fabled.exe` first — full build fails if the editor is open). Press **G** to playtest the current map.
- **Generator CLI (fast, no rebuild):** `python tools/gen_freeform.py --seed 7 --organicness 0.8 --corridor-width 1.5 --hidden 0.5 --show`
- **Builds:** `cargo build -p server` / `-p client` work in Git Bash; full `cargo build` needs cmd.exe/PowerShell (Git's `link.exe` conflict) AND the editor closed.
- Don't `git checkout` files with uncommitted user work (repo is mostly uncommitted) — see MEMORY CRITICAL.

## Key files
`tools/gen_freeform.py` (generator), `tools/gen_maps.py` (--preview bridge), `crates/client/src/editor_map_gen.rs` (Proc panel), `crates/server/src/level.rs` (floor/ceiling colliders), `docs/procgen-faction-manifest.md` (spec), `docs/kenney_kits_catalogue.md` (assets).
