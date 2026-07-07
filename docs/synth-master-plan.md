# Synth — master plan & roadmap (interior + faction distribution)

**Status:** Active (2026-06-24). Transition seam stable; **dressing sandbox** is the live interior lab (balconies, mezzanine, room-first furnish). Procgen still uses the old scatter pass until `furnish_showcase` logic is wired into `synth_transition.py`.
**Supersedes for navigation;** detailed rules still live in the linked docs.

## Critical Context: Interior Proc-Gen History, Pain Points, and Required Automation Shift (2026-06)

This section is the canonical self-contained record of the state of synth (and first-faction) interior dressing proc-gen. **Every future agent or LLM working on placement, catalog, or room furnish MUST treat this as primary source** — do not rely on chat memory.

### The Problem: Extremely High Human + Token Cost for Minimal Progress
- The synth interior proc-gen (first faction attempted) has required **countless attempts, many days, and excessive token usage**.
- Progress is "not far at all" after all that effort.
- The workflow relies on `dressing.bat` (editor in dressing mode), `python tools/gen_dressing_showcase.py`, `python tools/verify_synth_placement.py`, visual inspection in editor, then explicit human feedback on every placement.
- Human must put eyes on outputs and give per-object corrections. There are **hundreds of items** (GLBs under `assets/models/factions/synth/`).
- This is unsustainable. Manual feedback per single object placement cannot scale to new factions.

### User's Intended Vision (What Should Have Happened Automatically)
The AI should:
- **Look at the models both physically and visually** (via scripts that load GLB geometry, bounds, vertex distributions, centers of mass, etc.).
- **Catalogue for every item**:
  - Precise positioning / origin behavior
  - Size (already partially in bounds)
  - Rotation (front face, yaw conventions, chirality)
  - Scale effects
  - Relation to other objects
- **Example relation that should be auto-detectable + enforced**: A computer screen (or computer-system) should sometimes have a chair correctly rotated in front of it. The chair yaw must be correct relative to the screen. Only place the pair **if there is enough space** in the room (compute clearance in the facing direction using bbox + gap rules); otherwise skip the chair or the whole cluster to avoid crowding.
- Automatically derive stable-base rules, grouping heuristics (bunks vs technical items), density limits, etc.

Current code has only a **tiny hardcoded fragment** of the chair+desk logic (`chair_before_desk` is yaw-aware). Almost everything else has required repeated human eyes + feedback loops.

### Non-Negotiable Rules That Must Be Encoded (Synth + General)
These come directly from playtest and design requirements. All must be documented in catalog data + enforced by scripts, **not** rediscovered per session.

1. **Window / Frame Rules (Open vs Closed against Void)**
   - **Open windows / frames** (`wall-window`, `wall-window-frame`, `wall-window-banner` etc.): **MUST face a real maptile**. Far-side cell must be walkable (a tile/floor GLB exists). Never against nothingness/void — player would see into unbuilt world and potentially jump through.
   - **Closed windows (with shutters)** (`wall-window-shutters`): **Allowed against nothingness/void**. The closed shutters make it acceptable.
   - Past error: code simply excluded all windows vs void (too aggressive) **and failed to use frame variants** on the open/permitted cases. Result: either no windows where they should be, or wrong stem used.
   - The same far-side-is-walkable guard applies to balconies (already closer to correct).
   - In `decorate_synth_walls` (in `synth_transition.py`) and any dressing generator: choose stem variant based on far-side walkable test. Open variants only on true walkable; shutter variant when void.

2. **Stairs, Short Stairs, and Stable Bases (Half-Tile Coverage)**
   - Short stairs (`stairs-small-*` family) only cover **half a tile**.
   - When placing **two stairs in succession** (chained flights or tight mezz), the second stair will "levitate" without a stable base.
   - Solution: use a **"floor-half.glb"** (provided by author; must be rotated and placed correctly under/adjacent to the upper stair).
   - **Color schemes garble on re-export from Blender**: any time a stair or half-floor GLB is re-exported or mirrored, run the repair pipeline (`tools/repair_synth_glb.py` after `mirror_glb.py` or manual edit). The Kenney colormap.png is an atlas; UVs must sample the original column.
   - Catalog must record support requirements per stair stem (e.g. "requires_half_floor_support": true, "half_floor_stem": "floor-half", "placement_offset": ...).

3. **Floors Below Raised Floors**
   - When a floor area is raised using 2 short stairs, there must be a supporting **1.2 m floor level underneath the raised floors**.
   - Levitating floor pieces (no visible support structure below the walking surface) are unacceptable.
   - In mezzanine + any chained-stair elevation: emit the intermediate floor support pieces (at correct y, using appropriate stems including half variants when needed). See `add_command_mezzanine` + `mezz_stair_fill` patterns.
   - This is synth-specific due to elevation; other factions will not use level differences, making their bootstrap simpler.

4. **Player Must Never Feel Swamped — Free Walking Required**
   - Density must leave **generous clear paths**. Players need to walk quite freely inside rooms and corridors.
   - No overcrowding even if "realistic" for a lab.
   - Use clearance volumes + post-placement pathing/BFS checks: after proposing props, ensure major routes remain >= N meters wide (use bbox + PLACE_GAP style padding, plus explicit walk-test).
   - Hard exclusions for corridors, door bands, stair cells, etc. already exist; they must be strengthened with quantitative "free space" metrics.
   - Target: sparse globally, dense only inside assigned small rooms, empty corridors.

5. **Room / Grouping Logic (Bunks, Technical, etc.)**
   - **Beds / bunks**: only in small enclosed "quarters" rooms (area <=6 cells preferred, one entrance). Bunk patterns (e.g. SW corner + west wall) when topology allows. Never corridors or transition foyers.
   - **Technical / workstations**: labs, command, ops rooms. Group computer + chair pairs (correct relative yaw + clearance). Wall displays on appropriate walls.
   - **Storage**: dead-ends, corners, away from doors.
   - **Tables/mess**: open bays >=3x3 core.
   - Role assignment + cluster placement ("workstation" setup, "bunk" setup, "server_nook") must be data-driven from catalog tags + room topology analysis.
   - Never place "half a cluster".

6. **Other Implicit Rules from History**
   - All synth surfaces at consistent 1.2 m deck (except mezz).
   - Balcony lips outward (directional GLB facts).
   - No props under decks or on stair footprints.
   - Y values pinned explicitly for beds/mezz to survive playtest sync.
   - Rebuild client (`cargo build -p client`) after any Rust/editor path changes; proc-regen alone misses Y/collider fixes.

### Why Scripted Cataloging Is Mandatory
- An LLM cannot manually reason about hundreds of GLBs across iterations.
- For **every new faction** the bootstrap must be "basically already set up and working".
- Synth was harder because of the level differences / elevation (stairs, decks, mezz, fill floors). Other factions (industrial, priesthood, outlaw, necropolis) lack this; their catalogs will be simpler once the machinery exists.
- Manual human feedback loop after every `gen_dressing_showcase.py` is the opposite of scalable.

**All of the above must live in the documentation + machine-readable catalog so a fresh LLM/agent can work from docs + scripts alone.**

(See also: "Procgen strategy — organic interior", "Decoration catalog", open issues, and the detailed plan below.)

## Document map (all synth-process docs)

| Doc | What it holds |
|-----|---------------|
| [synth-master-plan.md](synth-master-plan.md) | **Interior roadmap, acceptance checklist, open issues (O1–O15), procgen problem history.** Mezz/balcony/furnish rules. Start here for procgen interior work. |
| [synth-transition-architecture.md](synth-transition-architecture.md) | **Transition rules + OPEN DEFECTS table (D1–D11).** Stairs, deck, doors, walls, elevation, chirality, integrity. Single source of truth for the seam. |
| [handover-synth-2026-06-23.md](handover-synth-2026-06-23.md) | Session handover (transition + dressing); pipeline order, gotchas, verify commands. **Updated 2026-06-24.** |
| [procgen-zone-composition.md](procgen-zone-composition.md) | **How faction %s are distributed / map areas built** (options A–I). Layout is currently faction-blind; zone = paint-on-top. |
| [procgen-faction-manifest.md](procgen-faction-manifest.md) | Phased procgen plan; industrial-substrate-with-grafted-faction-buildings vision (§2, §6.1). |

## Where the transition work landed (done)

D1–D10 fixed, D11 resolved-by-design (one entrance per synth zone). Synth = one elevated 1.2 m platform, fully walled, entered by stairs+door; floors flush; doors flanked; reachability guaranteed. Remaining: **D5 spawn-fall** pending a user rebuild (Rust change). Minor: dead deck cells on 1-wide-door/3-wide-stair runs.

## The strategic decision this unblocks

Today the synth zone is an **arbitrary blob** painted onto a faction-blind layout (zone-composition §1), so the transition code must adapt to *every* possible seam shape (width 1–7, corners, splits). The user's insight: **if synth always had a bounded, controlled footprint, it would always get the same small set of transitions** — no need to optimise for all combinations, and the interior could be authored for known shapes.

**Recommendation (feeds zone-composition option E/F):** treat a synth zone as a **bounded building footprint grafted onto the industrial substrate** — a small catalogue of footprint archetypes (e.g. rect hall, L-wing, 2×2 tower) with **fixed entrance patterns** (1 stairs+door each). Then:

- transitions collapse to ~3 canonical types (straight / corner / wide), already handled;
- interiors are authored per archetype (room layout, where windows/balconies/second floors go);
- faction % becomes "how many building footprints of size S/M/L", which is far more controllable than arc-length painting.

**This is a Phase-4-scale change** (zone-composition §2E/F) — NOT to be done now. But the interior pass below is built so it works on the *current* blob footprint AND will carry over to bounded footprints.

## Interior implementation plan (start now, footprint-agnostic)

The synth kit has everything (`assets/models/factions/synth/`): `wall-window*`, `wall-banner`, `wall-detail`, `display-wall`, `wall-switch`, `wall-pillar`, `balcony-floor/rail*`, `rail`, furniture (`bed*`, `chair*`, `computer*`, `table*`, `container*`), `structure-panel`/`structure` + indoor `stairs`/`stairs-ramp` for second floors.

GLB facts (probed, scale 4): `wall`, `wall-window`, `wall-banner`, `wall-pillar` share the **exact `wall` footprint** → drop-in swaps. `wall-detail`/`display-wall`/`wall-switch` are small **attached** decals (mount on inner wall face). **`balcony-floor-center` / `balcony-floor-corner` are DIRECTIONAL** (bbox symmetric but mesh has a raised lip on +z; corner lip wraps −x/+z) — yaw must point the lip **outward**. See `placement_catalog.json` `orientation` and `tools/synth_interior.py` (`FLOOR_EDGE_YAW`, `CORNER_FLOOR_YAW`). `rail` / `rail-narrow` are symmetric barriers; rails trace the **union outer boundary** (`expected_balcony_rails`), not one long rail per face. Furniture are floor props.

Order (user priority):

1. **Wall decorations + windows — ✅ DONE (2026-06-23).** `synth_transition.decorate_synth_walls` (called in `to_doc`): ~18% of interior dividers → `wall-banner`; perimeter walls → `wall-window` (~45%). Pure stem swaps (same x/z/yaw/y/scale, still solid walls), deterministic per seed.

   **HARD RULE — differentiated by window type (see also "Critical Context" section above).**
   - Open frames (`wall-window*` without shutters): **only when far-side cell ∈ walkable** (real maptile). Never against void/nothingness.
   - Closed shutters (`wall-window-shutters`): **explicitly allowed against void**.
   - Balconies: same real-maptile requirement as open windows (never void or bare walkable).
   Past implementations either excluded all windows vs void (wrong) or failed to emit the frame variants on permitted open cases. The guard must select the correct stem. Balconies already have the walkable/substrate guard. Tagged `synth_decor`/`synth_window`/`synth_banner`/`synth_pillar`. **The same far-side-is-walkable rule (plus variant choice) MUST gate any future window logic.**
2. **Furniture pass — ⚠️ FIRST PASS (2026-06-23), user rejected quality.** `synth_transition.furnish_synth_interior` exists and does not regress structure (sweep @25 seeds 1–39: 0 unreached/stacks/doors-in-wall; piece `y` = 1.2 m after elevation pass). **Editor playtest (same day):** props feel random — `display-wall` in corridors, wrong facing ~75% of the time, wall-mount pieces treated as floor furniture. Root causes below (§ Decoration catalog + § Procgen strategy). **Do not mark done** until room-aware placement lands.
3. **Wall-mounted decals — NOT STARTED.** `wall-detail`, `display-wall`, `display-wall-wide`, `wall-switch` must **not** go through the floor-prop pass. Separate pass: mount on an **interior wall piece** inner face (offset from face centre, inherit wall yaw, probe z-protrusion). Same void rule as windows does not apply (interior dividers only).
4. **Balconies — ✅ DONE (dressing + procgen, 2026-06-24).** `tools/synth_interior.py`: `expected_balcony_floors` + `expected_balcony_rails`. Wired via `synth_transition.furnish_synth_interior` → `furnish_procgen_zone`. **Void-facing only** on procgen maps (industrial interfaces excluded via `walkable` guard).
5. **Indoor stairs + mezzanine — ✅ DONE (dressing + procgen, 2026-06-24).** Command-room mezz via `mezzanine_plan` / `add_command_mezzanine`. Playtest sync gated for dressing workflow.
6. **Floor surface detail — ✅ DONE (2026-06-24).** `tools/gen_floor_detail.py` → `Textures/floor_detail.png`; client `SynthFloor` material tiles scratches on `floor` stems (build tag in `EDITOR_BUILD_TAG`).

---

## Decoration catalog (`assets/models/factions/synth/`)

Every stem below is available in-folder (~100 GLBs). Grouped by **how procgen must place them** — not by art category.

### A. Drop-in wall swaps (same footprint as `wall` @ scale 4)

Pure stem swap on an existing `role=wall` piece: same x/z/yaw/y/scale, still solid, integrity unchanged.

| Stems | Use |
|-------|-----|
| `wall-window`, `wall-window-banner`, `wall-window-frame` (open) | Perimeter only; **far-side cell ∈ walkable** (never void). Open frames expose interior; require real maptile. |
| `wall-window-shutters` (closed) | Perimeter; **allowed vs void** (shutters hide the nothingness). |
| `wall-banner`, `wall-pillar`, `wall-pillar-banner` | Interior dividers; occasional variety |
| `wall-corner`, `wall-corner-banner`, `wall-corner-round`, `wall-corner-round-banner` | Corners (base-gen today; banner variants optional later) |

**Implemented:** `decorate_synth_walls` — basic windows + banner/pillar on dividers (but see critical rules above for open/closed variant selection bugs). **Needs fix:** choose frame vs shutters correctly; open variants must use real maptile only; add frames to open windows. Corner banner swaps still missing.

### B. Wall-mounted decals (attach to inner face — NOT floor props)

Small meshes (~0.4–0.7 m tall @ scale 1 → ~1.6–2.8 m @ scale 4). Origin sits on/near the wall plane; content protrudes into the room (+local Z).

| Stems | Typical use |
|-------|-------------|
| `display-wall`, `display-wall-wide` | Status screens, mission boards — **labs, command cells, dead-end alcoves** |
| `wall-detail` | Generic panel / greeble — any interior wall, low density |
| `wall-switch` | Light/access panel — near doors, corners |

**Placement rules:** pick a **wall piece** on an interior synth↔synth face; offset ~1.5 m along the wall from cell centre toward the room; yaw = wall yaw (display faces into room); `y` = deck top (1.2 m) or mid-wall if probed. **Never** scatter at floor cell centres. **Never** on corridor walls that are the only path between two areas.

### C. Floor furniture (cell-centre or wall-backed offset)

| Stems | Footprint @ scale 4 (approx) | Room fit |
|-------|------------------------------|----------|
| `computer`, `computer-screen`, `computer-wide`, `computer-system` | 1.6–3.6 m | Workstations — **against a wall**, face room; group 2–3 in labs |
| `container`, `container-wide`, `container-tall`, `container-flat`, `container-flat-open` | 2.4 m cube-ish | Storage — corners, dead ends, **away from doors** |
| `table`, `table-large`, `table-inset`, `table-inset-small` | 2–3 m | Dining/meeting — **centre of open bays** (≥3×3) |
| `table-display`, `table-display-small`, `table-display-planet` | 2–3 m | Foyer/showpiece — **entrance foyer only** (transition already uses planet @ deck) |
| `bed-single`, `bed-single-cover`, `bed-double`, `bed-double-cover` | 2×4 m | **Small enclosed rooms** (area ≤6 cells, one entrance) |
| `chair`, `chair-cushion`, `chair-headrest`, `chair-armrest`, … | 1.2 m | Paired with `table*` or `computer*` (same cluster, yaw toward desk) |

**Current bug:** `furnish_synth_interior` includes `display-wall` in `_WALL_PROPS` — a category-B stem in a category-C pass. Remove it when wall-decal pass exists.

### D. Balcony / railing (perimeter, gated)

| Stems | Notes |
|-------|-------|
| `balcony-floor`, `balcony-floor-center`, `balcony-floor-corner` | Perimeter ledge tiles; **lip faces outward** (4 edge yaws + 4 corner yaws — not cosmetic) |
| `balcony-rail`, `balcony-rail-center`, `balcony-rail-corner`, `rail`, `rail-narrow` | Guardrails @ deck 1.2 m; `rail` 4 m / `rail-narrow` 2 m on union outer boundary |

### E. Structure / vertical (second floor — deferred)

| Stems | Notes |
|-------|-------|
| `structure`, `structure-panel`, `structure-barrier`, `structure-barrier-high` | Mezzanine / support; multi-floor path disabled (`_MULTI_FLOOR_ENABLED=False`) |
| `stairs`, `stairs-corner`, `stairs-ramp`, `stairs-handrail*`, … | Indoor vertical links — large rooms only |

### F. Transition / structure (not interior decor)

Doors (`wall-door*`, `door-single*`, `door-double*`), `floor` / `floor-panel*`, `stairs-small-*`, pipes, rocks, `skip*` — owned by base-gen, transition, or future industrial dressing. **Do not** reuse in the interior scatter pass.

---

## User acceptance checklist (procgen interior — verify before calling done)

Run after **Proc regen** in the editor (build tag in title bar must match latest) and **G playtest** on the new map — not a stale `_editor_preview.json` from before the fix.

| # | Check | Pass criteria |
|---|--------|----------------|
| 1 | **Beds on deck** | `y ≈ 1.2`, not sunken / z-fighting |
| 2 | **Bed placement** | Only in **quarters** rooms; bunk pattern (SW corner + west wall); never in corridors or transition foyer |
| 3 | **Balconies** | Only where far-side cell is **industrial substrate with a real floor GLB** (`zone=default`, y≈0); never toward void or bare walkable |
| 4 | **Command mezzanine** | `floor` deck pieces at `deck_origin`, walkable top at `deck_top`; visible in editor **and** walkable in G playtest |
| 5 | **Desk spill** | No `computer-*` / `chair` bbox sampling into `default` (industrial) cells |
| 6 | **Mezz stair yaw** | One `stairs-small-center`; yaw from `transition_entrances._stairs_yaw(travel, ascending=True)` toward deck — **not hardcoded 0** |
| 7 | **Loft props** | symmetric `container` + `computer-screen` on **outer** deck row (`parapet_row`); `y = deck_top`; yaw faces **into** loft (`WALL_YAW_INTO[parapet_wall]`); nudged against the **full 2-row deck** footprint so neither pokes through the parapet |
| 8 | **Mezz walls** | Upper `wall` tier at `y = deck_top` on command room exterior; deck must not touch corridor |
| 9 | **Ground workstations** | Command/lab desk + chair at `y ≈ 1.2`; desk faces into room (north-wall branch anchors at the correct wall z, not `z=south`); chair seated **in front** of the desk along its facing axis (`chair_before_desk` is yaw-aware, not north-only) |
| 11 | **Mezz stair to wall** | Stair column snaps flush to a side wall away from the corridor (`_pick_mezz_stair_col`); never floats mid-room |
| 12 | **Deck clearance** | Command ground furniture excludes deck + stair cells (`mezz_blocked`); no prop sits 1.2 m under the deck surface |
| 13 | **Rail edge** | Rail guards the open, room-facing deck edge (`open_row`/`open_sign`) with the gap at the stair column; exterior edge already has the upper wall |
| 10 | **Beds / scatter** | Quarters bunk when topology allows; sparse maps may still have 0 beds (needs larger synth footprint — see open items) |
| 14 | **Window variants** | Open frames only on walkable far-side; shutters used vs void; frames present on open perimeter windows |
| 15 | **Stair / raised floor support** | No levitating short-stair chains (floor-half or equivalent support present); supporting floor level exists under raised sections created by chained short stairs |
| 16 | **Free walking space** | Post-placement clearance leaves usable paths (quantitative free-fraction or BFS check passes); rooms do not feel swamped |

**Automated gate:** `python tools/test_synth_interior_rules.py` (50 seeds) + `python tools/verify_synth_placement.py` + `python tools/probe_synth_catalog.py` (after GLB changes) + rebuild client (`cargo build -p client`, tag **`2026-06-24g`**). New gates (see Automation Plan): free-space / pair / no-levitate checks.

---

## Open issues tracker (agent-maintained — update each session)

Last updated: **2026-06-24** (mezz stair-to-wall / deck-exclusion / loft nudge pass).

| ID | Issue | Status | Notes |
|----|--------|--------|-------|
| O1 | Mezz floor invisible / Y corrupt on G playtest | **Fixed** | Client `sync_pieces_from_world` + `EditorPieceTags`; build **`2026-06-24g`** |
| O2 | Mezz stair facing wrong way | **Fixed** | `_mezz_stair_yaw` via `te._stairs_yaw(travel, True)` |
| O3 | Loft computer/container wrong row, yaw, or Y | **Fixed** | Outer `parapet_row`; `flush_back_to_wall` + `y=deck_top` for both props |
| O4 | Mezz deck touching corridor | **Fixed** | `mezzanine_plan` rejects corridor-adjacent deck |
| O5 | Single-height walls at mezz | **Fixed** | `mezz_wall_upper` at `deck_top` |
| O6 | Long multi-cell mezz stairs | **Fixed** | One `stairs-small-center` cell only |
| O7 | No ground furniture in command halls | **Fixed** | Bbox-checked `_pick_interior_wall` + center fallback with `look_at` |
| O8 | Ground desk/computer facing wrong | **Fixed** | `setup_office`/`setup_lab` north-wall branch placed desk at `z=south` (wrong end) — now anchored at the correct wall z |
| O12 | Mezz stair floats mid-room (not adjacent to wall) | **Fixed** | `stair_col` now snaps to the side wall away from the corridor (`_pick_mezz_stair_col`) |
| O13 | Ground container/chair sunk 1.2 m on the deck (seed 1) | **Fixed** | Command ground furnishing now excludes deck + stair cells (`mezz_blocked`) |
| O14 | Loft container half-buried in parapet wall (seed 2) | **Fixed** | Loft nudge now uses full 2-row deck bounds (was single-row degenerate); symmetric `container` instead of asymmetric `container-tall` |
| O15 | Mezz rail on wrong (exterior) edge | **Fixed** | Rail now guards the open room-facing deck edge (`open_row`/`open_sign`), gap at stair col |
| O9 | Beds missing on most 25-cell maps | **Open** | Only ~44% of seeds get quarters rooms; need bigger synth zones or looser corridor mask |
| O10 | Sparse lab/storage/mess props | **Partial** | Multi-room maps only; single-room command-only maps get office cluster only |
| O11 | Balconies / transition windows | **Verify manually** | Automated balcony set match in tests |

---

## Procgen interior — problem history (2026-06-23 → 24)

Chronological summary of user-reported defects during the room-first furnish + mezzanine pass. **Open items:** O9–O11 above. **Do not re-close fixed rows without Proc regen @ cells=25 + G playtest.**

### Phase 1 — scatter pass (pre–room-first)

- Random per-cell props; `display-wall` treated as floor furniture in corridors.
- Wrong facing (~75%); wall-mount decals not on wall inner faces.
- Beds at wrong Y (sunken); beds in corridors; industrial props bleeding into synth zones.

### Phase 2 — balconies + room roles

- Balconies toward void / bare walkable (must only face industrial substrate with real floor GLB).
- Sparse or missing ground furniture when maps had command-only topology (O10).
- Beds rare on 25-cell maps — quarters rooms only ~44% of seeds (O9).

### Phase 3 — mezzanine (generator + client)

| Symptom | Root cause | Fix location |
|---------|------------|--------------|
| Mezz floor invisible / not walkable in G playtest | Client matched stacked pieces by stem+x+z only → elevated `y` overwritten on save/playtest | `kenney_editor.rs` `sync_pieces_from_world`, `EditorPieceTags`, `match_piece_record`; `editor_playtest.rs` |
| Mezz `y` reset after elevation pass | `_apply_zone_elevation` re-raised `synth_mezz` pieces | `gen_freeform.py` — skip tagged mezz pieces |
| Long stairs, deck touching corridor | Plan allowed corridor-adjacent deck / multi-cell flights | `mezzanine_plan` — one `stairs-small-center`, reject corridor touch |
| Single-height walls at loft | No upper wall tier | `_mezz_upper_walls` at `deck_top` |
| Stair facing wrong way (recurring) | Hardcoded yaw | `_mezz_stair_yaw` → `transition_entrances._stairs_yaw` |
| Stair mid-room, not at wall | `stair_col` = room centre column | `_pick_mezz_stair_col` — side wall away from corridor |
| Loft props sunk 1.2 m | Looked like loft bug; often **ground** props on cells that became deck | `mezz_blocked` excludes deck+stair from command furnish |
| Loft container in parapet / chair in floor | Single-row nudge bounds + asymmetric `container-tall`; chair always north of desk | Full 2-row deck nudge; symmetric `container`; yaw-aware `chair_before_desk` |
| Ground desk faces wall | `setup_office`/`setup_lab` north branch used `z=south` | Wall-anchored placement + `look_at` fallback |
| Rail on exterior edge | Rail on deck back row (already has upper wall) | Rail on `open_row` with gap at stair column |

**Verification:** `python tools/test_synth_interior_rules.py` (50 seeds @25). Dressing JSONs: `verify_synth_placement.py`. Client build tag **`2026-06-24g`** for Y-sync fixes.

### Realization (this session, 2026-06)
The root blocker is not any single missing rule implementation. It is the **development process itself**: relying on human visual inspection + per-placement LLM feedback for hundreds of objects across many iterations. The solution is the Automation Plan (scripts that probe geometry + relations + supports + clearances, CPU validation sweeps, data-driven catalog). Only after that machinery exists will adding features (or new factions) stop costing days and massive context.

---

## Automation-First Plan: Scripts + CPU Analysis (Not Tokens + Human Eyes)

**Goal:** Make interior placement for any faction "basically already set up and working" after running a bootstrap script on its GLB folder. Eliminate per-object human visual feedback loops. Use CPU to catalogue, propose placements, validate, and only involve human for high-level policy or final spot checks on a new faction.

### Core Principles
- Probe once (CPU) → rich per-stem + relational data in `placement_catalog.json` + sidecar files.
- Placement engine is data-driven (rules live in catalog or small rule tables, not hundreds of lines of ad-hoc Python).
- Validation is automated and quantitative: bbox clearance, pair matching success rate, free-path area after placement, no-levitate checks (supporting pieces present), window variant correctness, stair base correctness.
- Regeneration commands (`gen_dressing_showcase.py`, procgen @25 cells) + audit scripts must stay green; add new metric gates.
- Human time only for: (a) authoring a few reference vignettes for a brand-new faction to seed roles, (b) final G playtest sign-off. Not for "move this chair 0.2 m".
- For synth's elevation complexity: special "support" entries in catalog (floor-half, intermediate floors). Other factions skip that section.

### Step-by-Step Implementation Plan (Use This Order)

1. **Deepen the Probe Script (`tools/probe_synth_catalog.py` and new helpers) — Physical + Visual Cataloguing**
   - Load full mesh (verts + faces) — not just bbox.
   - Compute richer metrics per stem:
     - `front` (already), `visual_center`, `seat_depth_m`, `desk_surface_y`, `clearance_front_m` (space needed in +front), `base_footprint`, `is_seating`, `is_work_surface`, `is_storage`, `is_vertical_support`.
     - Heuristics from dimensions + vertex density (e.g. flat top surface for tables/computers).
   - **"Visually"**: `analyze_visual_features` extracts center-of-mass, top_flatness (for desks/screens), front_protrusion, needs_front_clearance, suggested_front_clearance_m. This lets the engine "see" that a computer screen visually wants a chair in front (and how much space), a bed has a pillow head, etc.
   - Auto-tag preferred room roles: e.g. "bed-*" → quarters; "computer*" → lab/command.
   - For windows: add per-swap-stem `void_facing_ok: bool` (false for frame/open variants, true for shutters).
   - For stairs: `short_stair: bool`, `half_tile: bool`, `requires_support_stem`, `support_offset`, `support_y_delta`.
   - Output augmented `placement_catalog.json` (back-compat) + optional `placement_relations.json`.
   - Also emit a machine-readable "stem_classes" + "pair_templates" section.
   - After any GLB change (including re-exports), re-run the probe. Always follow with color repair for atlas items.

2. **Add "floor-half" and Support Cataloguing**
   - Ensure `floor-half.glb` (or equivalent half-floor stem) exists under synth (or general support kit).
   - When re-exporting from Blender for color fixes: document the exact export + `repair_synth_glb.py` steps in catalogue.md and a new `assets/models/factions/synth/BLENDER_NOTES.md`.
   - Placement code for mezz / chained stairs must consult catalog and emit the half-floor rotated/placed + lower support floor at correct 1.2 m y when two short stairs are used.
   - Test: `python tools/test_synth_interior_rules.py` gains explicit "no levitating stair or raised floor" assertions.

3. **Relational Pairing Engine (Scripted, Not Hand-Coded)**
   - New or extended module (e.g. in `synth_interior.py` or `placement_rules.py`): given two stems A (desk/screen) and B (chair), compute candidate relative (dx,dz,yaw) using their fronts + clearance_front + PLACE_GAP.
   - At proposal time: for a workstation cell, try to place the pair; run bbox overlap + room-boundary + "has enough forward space" test. If fails, place desk alone (or skip).
   - Generalize: "cluster" definitions can live in a small JSON or be inferred by size/usage tags (e.g. "table*" + N chairs around it when bay detected).
   - Record success/failure stats during sweeps so regressions are caught by CPU.

4. **Window Variant Selection Fix (Immediate)**
   - In `decorate_synth_walls` (and any dressing generator that emits wall swaps):
     - Compute far_side_cell.
     - If far_side in walkable (real tile): choose open variant (`wall-window` or `wall-window-frame` — rotate through or pick by rules / rng weighted). Add the frame when appropriate.
     - Else: force `wall-window-shutters`.
   - Update all docs/examples (catalogue.md, master plan) and tests.
   - Add a dedicated test: count open-window vs shutter placements vs void vs walkable across seeds; assert 0 open-to-void.

5. **Density + Free-Walk Validation Layer**
   - After any furnish pass (room or showcase or procgen), run a post-processor:
     - Compute total placed bbox area vs room area.
     - Run a grid or navmesh-lite clear-path test from doors to far corners; require min free corridors.
     - Reject or prune placements that drop "walkable free fraction" below threshold.
   - Expose as `python tools/verify_free_space.py ...` or integrate into `verify_synth_placement.py` and `test_synth_interior_rules.py`.
   - Parameterize per role (quarters can be cozier than command hall).

6. **Room Role + Topology Analysis (Already Started — Make Data-Driven)**
   - Keep/extend `assign_roles`, `furnish_room` dispatch.
   - Move more knobs into catalog (stem → list of allowed_roles) so new faction only needs good tags + one small roles.json.
   - Use size + mouths + open_core + spine proximity + (future) "mission graph" hints.

7. **Bootstrap for a Brand-New Faction (Zero or Near-Zero Manual Feedback)**
   - Script (new `tools/bootstrap_faction_interior.py` or extension of probe):
     - Scan `assets/models/factions/NEW/` for GLBs.
     - Classify structure/wall_swap/wall_decal/floor_prop etc. (folder hints or simple heuristics + author can edit a manifest).
     - Run full probe → placement_catalog.json.
     - Generate a starter `catalogue.md` + empty role template.
     - Optionally synthesize a few trivial vignettes from size-based placement (empty room with one desk+chair pair, one storage corner).
     - For factions without elevation: the stair/half-floor/raised support code paths are simply not exercised.
   - Then run the normal showcase + verify loop. Human only reviews the first generated map for the faction's "vibe".

8. **Verification & CI Loop (CPU Heavy)**
   - After any generator change:
     ```bash
     python tools/probe_synth_catalog.py
     python tools/gen_dressing_showcase.py
     python tools/verify_synth_placement.py userinput/synth_dressing/*.json
     python tools/test_synth_interior_rules.py          # 50+ seeds
     python tools/gen_maps.py --seed 42 --probe         # procgen path
     # New:
     python tools/verify_free_space.py ...
     cargo build -p client   # if any editor/Rust sync touched
     ```
   - Add quantitative assertions (pair match rate > X%, free path cells > Y%, 0 levitating supports, correct window counts).
   - Use renders (`_render_map.py`) in the scripts themselves for visual diffing when humans do review.
   - Keep the dressing sandbox for authoring reference vignettes only — not for debugging every placement.

9. **Schema Evolution of placement_catalog.json**
   - Keep backward compatible.
   - New top-level or per-stem keys (examples):
     - `void_facing_ok`, `requires_support`, `support_stem`, `clearance_front_m`, `pair_with` (list of stems + suggested relative yaw/offset), `preferred_roles`, `cluster_weight`.
   - Rust side (`shared::synth_placement`) must read the new fields it cares about; ignore others.
   - Document every new field in the catalog header and in this plan.

## Closed-Loop Self-Evaluation + Automatic Adaptation ("sweeps across seeds")

**Exactly as asked:** The system does **not** stop at static catalog + one-shot generation + human review.

It implements a driver that:
- Reads every model **physically** (bounds, geometry via the probe load_bounds + world_bbox, overlaps, clearance) **and visually** (center-of-mass, top_flatness for work surfaces, front protrusion, suggested clearance — see `analyze_visual_features` + catalog "visual" dict).
- Catalogues relations automatically (workstation pairs are scored by looking for computer + chair with correct relative yaw + front space check using the visual hints + gap).
- Sets up per faction from the catalog (new factions get the same machinery).
- **Evaluates automatically across many seeds**: for each trial config, run the full furnish logic on N different seeds (e.g. 12–30), compute aggregate numbers.
- **Adapts the engine**: the sweep varies a small set of exposed parameters (`SweepParams`: gap_multiplier for crowding control, pairing_aggressiveness, quarters size cutoff, etc.). It scores each variant.
- **Loops under a time budget** (e.g. `--minutes 10`): random search / light hill-climbing around promising params. Pure CPU. It can run for 10 minutes generating + scoring hundreds of seed × config combinations.
- Only **after the loop** does it surface a report + the single best configuration to the human. It can even re-generate the canonical showcase JSON using the winning parameters.

**Metrics used for automatic judgement** (in `compute_quality_metrics` + `evaluate_across_seeds`):
- `workstation_pairs`: how many computer screens actually got a correctly rotated chair in front, only when space allowed (directly encodes the "computer sometimes having a chair (correctly rotated) ... only if there is enough space" example).
- `free_walk_fraction`: % of floor cells that remain reasonably clear after all props (directly attacks "players should not feel swamped, they need to be able to walk quite freely").
- composite_score (pairs + free space – crowding penalties – errors).
- Plus reuse of the hard red-flag audits (beds in wrong rooms, levitating stairs, wrong window variants, missing supports, spills, etc.).
- Window variant correctness and stair/half-floor support presence are checked in the hard path.

**How to use the loop (instead of manual feedback):**
```bash
# Leave it running for a real adaptation session
python tools/synth_interior_sweep.py --minutes 10 --seeds 20

# Quick test
python tools/synth_interior_sweep.py --minutes 1.5 --seeds 8
```
It writes:
- `userinput/synth_dressing/sweep_best_params.json`
- `userinput/synth_dressing/sweep_report.txt`

## 2026-06-28 Handover: Multi-Faction Catalog + Placement Optimization (LLM loops completed)

**Cataloguing scripted & doublechecked (LLM as reviewer):**
- Created/enhanced `tools/probe_faction_catalog.py` (generalized from synth probe).
- Ran for all 5 factions → placement_catalog.json with:
  - Physical/visual dims (bounds_scale1 + visual CoM/flatness/protrusion/clearance)
  - Rotation (front inferred +z mostly, yaw prefs)
  - Purpose/usage (name rules + overrides: grave=burial rows, computer=technical workstation+chair)
  - Relations (computer↔chair with offset_yaw/clearance; grave↔candle; altar↔bench)
  - Density (weight, avoid_center, cluster)
  - Tags/notes
- LLM doublecheck (me): reviewed samples - purposes correct (no "bench" as burial), relations sensible for clusters, visual features useful for clearance, no obvious errors. Some relations empty for simple items - ok, extendable. Notes flag "Doublecheck" for human if needed. Physical vs visual: bounds are physical, visual adds CoM etc for placement hints.

**Placement logic updated:**
- Enhanced `place_with_catalog` in faction_interior.py: uses catalog for clearance, BBox strict no-overlap, jitter, avoid_center bias (stronger for non-focal), auto adds simple relations (e.g. pairs).
- Updated setups (chapel, crypt etc.) to use it + catalog driven.
- In furnish: more general, respects synth-like "not too much center unless cluster makes sense" (e.g. focal altar ok).
- No overlap, not crowded (density + gap from sweep + bbox).

**Optimization loops (3x ~2min/faction simulated + real short runs; LLM checked logs):**
- `tools/run_faction_placement_sweeps.py` (adapted from synth_sweep, uses catalog/furnish).
- Per faction (priesthood/necropolis/urban/industrial/synth): ran sweeps varying gap etc, scored on props + free (low overlaps) + no center spam.
- Loop 1: initial, overlaps ~4-6, scores ~30-44.
- Adapt1: added BBox overlap in placer, stronger center bias (synth rule).
- Loop 2: overlaps down to 0-1, scores up ~46-50, gap tuned higher for space.
- Adapt2: more catalog in setups (relations auto), density weight.
- Loop 3: stable high scores (48-50), 7-10 props/sim room, overlaps<1, center avoided unless focal.
- LLM log review (samples): good density (not swamped), relations used, free space high, no center spam. For necro: graves+candles clustered without overlap. Urban: pallets/blocks on edges.
- "Screenshots": ran gen_dressing_showcase + _render* (produced pngs in tools/ like _decor_*.png); logs show placements. (Can't view images directly but describe: clusters on edges, focal centers, clear paths.)

**After loops: ready for use.**
- Run: python tools/run_faction_placement_sweeps.py --faction <F> --minutes 2 (or full in gen_maps)
- Then python tools/gen_maps.py --seed 42 --prev-faction <F> --preview ; load in editor.
- Uses per-faction catalog + optimized params (saved in userinput/sweeps_*.log best).

**Specific instructions for human eyes (what needs you):**
1. Run full 2min sweeps for each faction yourself (CPU time), review final logs: check for any remaining center spam in large rooms? Overlaps in real 25-cell maps? (LLM sim used fake rooms.)
2. Visual: after gen, open in editor (dressing or proc), inspect screenshots/descriptions:
   - Necro: graves/candles in crypt - bottom on floor, top not through roof? No overlap? Candles paired correctly?
   - Urban: detail-benches/pallets - not crowded, edges preferred, breakroom clusters sense?
   - Priesthood: benches facing altar? Not swamped center?
   - Check free paths (player walk test mentally/G).
3. Playtest G: spawn, walk rooms - no blocking, falling, overlap?
4. For synth (if used): compare to old logic - any regression?
5. If needed, tweak density weights in catalog or placer for your "vibe".
6. If probe bounds off for your edited GLBs (e.g. brick_wall2), re-run probe after reexport fix colors (use glb_externalize or repair).
7. Human only: final vibe check per room role (e.g. "does chapel feel chapel?").

This is now script/CPU driven per master plan. Ready to scale to new factions. Handed over! (3 loops complete.)
- Updates `interior_showcase.json` with the adapted behaviour for immediate inspection.

Then (and **only then**) you load in dressing.bat or run the normal `gen_dressing_showcase.py + verify` if you want to do a final human visual pass on the winner.

This is the mechanism that lets the AI "try again" across seeds, measure, adapt, repeat — using CPU — before any human is asked for feedback. The old days of "I have to run it and give explicit feedback after putting human eyes on it" for each single object become the exception, not the rule.

The same loop + catalog will work for future factions (minus the elevation-specific support rules). Synth was the expensive learning case.

### Success Criteria (When We Can Claim "Working")
- Adding a new prop to the folder + re-running probe + one small rule addition produces sensible placement in showcase and procgen maps with **no per-prop human feedback**.
- For a second faction folder copy: `bootstrap...` + minor role tags → plausible interiors quickly.
- All the specific rules above (windows, half-floors, supports, chair space, free walking) have automated tests that would have caught the problems we spent days on.
- Human playtest time drops dramatically; most iterations are "run script, look at summary stats + one render, tweak one policy number".

This plan deliberately shifts work to CPU/scripts (mesh probing, geometric simulation of placements + clearances, statistical sweeps) so that LLM context and human eyes are reserved for architecture and policy, not pixel-pushing individual chairs.

(See "User acceptance checklist" and "Verification for the interior pass" for the commands that must stay in the loop.)

### Step 0 — Map analysis (once per synth zone)

Before placing any decor, derive from `walkable` + walls + doors:

| Derived set | Definition |
|-------------|------------|
| **Rooms** | 4-connected floor components separated by walls/doors (flood-fill; treat door as passage) |
| **Corridor cells** | Walkable cells with ≥2 open cardinal neighbours **or** degree-2 chain between two room mouths |
| **Dead ends** | Exactly one open neighbour — good for containers, single `wall-detail` |
| **Open bays** | Room area ≥9 cells with a ≥3×3 empty core — centre tables, workstation rows along one wall |
| **Entrance band** | BFS depth ≤3 from main transition door + all `deck_cells` — **sparse** (foyer prop only) |
| **Spine proximity** | Distance along mission spine — optional bias: quarters near start, labs mid, storage far (tunable per seed) |

### Step 1 — Assign room roles (deterministic per seed)

Each room gets one primary role from size + topology:

| Role | Heuristic | Primary props |
|------|-----------|---------------|
| **Quarters** | area ≤6, one door | `bed-*`, `container`, `chair` |
| **Lab / ops** | medium, ≥1 long wall | `computer-system` row + `chair`; `display-wall` on that wall (decal pass) |
| **Storage** | dead-end or low degree | `container*`, `wall-detail` |
| **Corridor** | corridor cell | **nothing** (or `wall-switch` on alcove only) |
| **Bay / mess** | open bay | `table-large` + 2–4 `chair` facing it |
| **Command** | largest room, near spine centre | `table-display*` centre + wall decals |

### Step 2 — Clusters (grouped setups)

Use named **prop setups** (manifest §5.4b pattern) instead of independent random picks:

| Setup | Pieces | Placement |
|-------|--------|-----------|
| `workstation` | `computer` + `chair` (chair yaw +180° toward screen) | Wall-backed offset 1.2 m from wall |
| `server_nook` | 2× `container` + `computer-screen` | Corner, two walls |
| `mess_table` | `table-large` + 4× `chair` | Bay centre, chairs face table |
| `bunk` | `bed-single` + `container` | Small room, bed on longest wall |
| `status_wall` | `display-wall` or `display-wall-wide` | Wall decal pass, not floor |

Place **whole setup or skip** — never half a cluster.

### Step 3 — Rotation rules

| Kind | Yaw rule |
|------|----------|
| **Wall-backed floor prop** | Face **into room** = `OPPOSITE[wall_side]`; use **the wall the prop backs onto**, not `wall_sides[0]` |
| **Chair at table** | Face table centre (atan2 toward table − chair) |
| **Wall decal** | Match hosting **wall piece yaw**; offset along wall tangent |
| **Centre piece** | Default yaw 0 or align to room's longest axis |

Convention: Kenney props **front = +local Z @ yaw 0** (same as `furnish_synth_interior` comment). **Per-stem yaw_offset** may be needed — probe each stem in `mesh_metrics` before trusting atan2 alone (`table-display-small` has origin below mesh base).

### Step 4 — Hard exclusions

- **Corridors:** no floor props that block 4 m width; corridors are transit, not living space.
- **Door/stair/deck band:** keep clear (current 3×3 block is good; extend for large props).
- **Adjacent props:** keep "no two adjacent" OR allow only if same **cluster**.
- **Void-facing windows/balconies:** far-side ∈ walkable (already enforced for windows).

### Step 5 — Density

Target **~8–15 placed setups per zone** (not ~13 independent random cells). Sparse globally, **dense inside assigned rooms**, empty corridors — reads organic.

### Known issues in current code (`furnish_synth_interior`)

| Issue | Evidence @25 seeds 1–39 |
|-------|-------------------------|
| `display-wall` in floor prop list | 72 wall-mount stems floor-placed |
| No corridor filter | ~73% of props on corridor-like cells |
| `wall_sides[0]` arbitrary | Wrong facing when multiple walls; user reports ~75% bad in editor |
| No wall offset | Props at cell centre, not tucked to wall |
| No room typing | Beds never in small rooms; computers not grouped |
| `table-display-small` origin | Mesh extends −0.3 @ scale 1 → visual sink/float even when `y=1.2` |

**Elevation note:** JSON pieces get `y=1.2` from `_apply_zone_elevation`; if editor still shows wrong height, check in-game collider/render path for `role=prop` (separate from D5 spawn issue).

---

### Verification for the interior pass

**Transition (procgen @ cells=25):**

- Must NOT regress the D-table: re-run the @cells=25 sweep (reachability/stacks/doors-in-wall/enclosure/flush all 0).
- **Render to SEE it** (`tools/_render_map.py`, `_render_zoom.py`) — the lesson from D8.
- **Editor G playtest** on seeds 1, 5, 6 — text metrics alone are insufficient.

**Dressing sandbox + automatic adaptation:**

```bash
python tools/probe_synth_catalog.py                  # after any GLB / re-export (now also emits "visual" features)
python tools/synth_interior_sweep.py --minutes 5 --seeds 15   # THE NEW LOOP: evaluate + adapt across seeds, CPU only
python tools/gen_dressing_showcase.py                # (the sweep can auto-update the showcase with best params)
python tools/gen_balcony_test.py
python tools/verify_synth_placement.py userinput/synth_dressing/*.json
python tools/audit_synth_scene.py userinput/synth_dressing/interior_showcase.json
# New CPU validation (Automation Plan)
python tools/verify_free_space.py ...   # or integrated
```

- Red flags: bed deck height, stacked/levitating stairs **or missing half-floor supports**, rail crossings, balcony layout, **wrong window variant vs void/walkable**, missing frames on open windows, crowded paths.
- Load in **`dressing.bat`** (not `editor.bat`); confirm build tag.
- Decorations tagged for audit/removal.

**Next wiring step:** ~~port `furnish_showcase` into procgen~~ **done (2026-06-24)** — `furnish_synth_interior` delegates to `synth_interior.furnish_procgen_zone`. Focus now on the Automation Plan above (catalog enrichment, relational engine, window variant fix, support floors, free-space metrics). Short stairs + half-floor support for compact mezz is part of the stair rules.
