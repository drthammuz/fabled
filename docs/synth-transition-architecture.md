# Synth transition & elevated architecture — implementation plan

**Status:** Active (2026-06-22). Single source of truth for synth boundary work.  
**Scope:** Industrial ↔ elevated synth transitions only. Other factions follow once this exits.

This document collects **every user rule and correction** from the synth-transition thread (2026-06). Read it before touching `tools/synth_transition.py`, `tools/synth_deck.py`, `tools/transition_entrances.py`, or elevated emit in `tools/gen_freeform.py`.

---

## 0a. OPEN DEFECTS — playtest feedback (do NOT remove a row until fixed + verified)

User playtested synth–industrial–synth, seeds 1–10 (reported 2026-06-23). Coordinates are editor/world coords. **Each row stays until fixed and verified; reference this table instead of re-asking the user.**

**CRITICAL — the editor generates at `--cells 25` (gen_maps default), NOT 40.** All earlier "verified" sweeps ran at cells=40 and tested *different maps* than the user plays. Always reproduce playtest defects at **cells=25**.

| ID | Seeds | Defect | Status |
|----|-------|--------|--------|
| D1 | 2 (~10), 3, 6 (8), 7, 10 (~9) | **Many doors stacked at one location** | **FIXED 2026-06-23** — root cause `emit_building_face_doors` dropped a door on *every* synth cell facing the deck (a whole column). Call removed; deck is interior (open). Doors now 2–7/map, 0 stacks across seeds 1–39 @25. |
| D2 | 6 (26,21), 7, 9 (-8,-22) | **Door lodged inside a wall** | **FIXED** — `_strip_walls_under_doors` post-pass deletes any floor-0 wall sharing a door's face. 0 across seeds 1–39 @25. |
| D3 | 3 (×3), 8 (×2), 9, 10 | **Inaccessible synth areas** | **FIXED** — `_ensure_synth_accessibility` BFS-repair opens a stair+door entrance into any region the envelope walls sealed. 0 unreached cells seeds 1–39 @25. |
| D4 | 3, 4, 8, 9, 10 | **Transition without an opening** | **FIXED** — same reachability repair guarantees a passable entrance to every region (incl. default pockets between the two synth zones). |
| D5 | 5, 7 | **Falling through world at spawn** | **FIX APPLIED, re-test** — `infer_spawn_floor_y` (kenney_layout.rs, the editor's in-memory fallback) used the synth `floor` block's *origin* (0), not its 1.2 m *top*, so the player spawned inside the block and dropped through. Added `walkable_surface_y` (origin + 0.3·scale for synth `floor` blocks). Saved-JSON path already carried `spawn_y=1.2`; this fixes the in-memory regen path. shared crate compiles. |
| D6 | 10 (~28,24) | **~9 doors, ~22 `structure-barrier` supporting nothing, floor holes** | **FIXED** — disabled the half-baked multi-floor (3.6 m / `structure-barrier`) path (`_MULTI_FLOOR_ENABLED=False`). 0 barriers seeds 1–39 @25. |

### Round 2 feedback (2026-06-23, seeds 2–5) — root cause found

| ID | Report | Status |
|----|--------|--------|
| **D7** | **`zone` mislabel — THE big one.** `plan_transition` did `zone = "prev" if faction_id == prev_faction else "next"`; with prev==next=="synth" it **always** returned "prev", so the NEXT zone's deck/door/elevation (all keyed on `plan.zone`) broke → `deck=[]`, door stuck on the substrate cell = **"transition without an opening"** (seed 4 ~-36,-6). This also forced extra accessibility-repair doors ("illogical doors", seed 3 -4,42). | **FIXED** — `zone = zone_lookup(faction_cell)`. Next-zone transitions now have proper deck+door. |
| D8 | **"Raised line, 2 tiles a pixel too high"** (seed 2, 3, 4, 5). | **FIXED 2026-06-23** — root cause: deck pieces had explicit `y=0.0` (top 1.2 m) but converted synth floor blocks had **no `y`**, so the client used `default_placement_y = -0.005` → synth floor sat **5 mm below** the deck tiles. The 2 deck tiles read as "a pixel too high". Now `_apply_synth_exterior_floors` pins `y=0.0`; all synth surfaces flush (0 non-flush blocks seeds 1–39). Found by **rendering** the map (`tools/_render_map.py`, `_render_zoom.py`) — height numbers per tile exposed the 5 mm. |
| D9 | "Doorway with no connected walls" / "door 2 tiles from stairs" / "door far inside, illogical" (seeds 1,2,3,4,5). | **FIXED 2026-06-23** — two causes: (1) the **2-tile door setback** (old retracted directive) pushed the door deep into open floor — removed (`min_door_depth=1`, door at the entrance cell); (2) `toward_sub` used `te._toward_substrate_side` which returns the toward-*faction* dir, placing the door on the cell's *interior* edge — now `OPPOSITE[toward]` (faces the stairs); (3) added `_attach_walls_to_doors` — a **reachability-safe** jamb pass that walls each open door flank (never the sole path). Bad doors **36 → 1** (the 1 is a corridor door, legitimately open); reachability preserved. |
| D10 | Seed 5 ≈-12,-22: corner stairs' **outer corners on the wrong side** (left/right swapped). | **FIXED** — corner GLB's x-asymmetry doesn't track its outer-corner direction, so the rail-sign rule chose wrong. Swapped corner family in `_VARIANTS_BY_SIGN`. |
| D11 | "Missing opening between factions" (seed 3 ≈-31,-1, seed 4 ≈-32,-6). | **NOT A BUG — RESOLVED by design (user, 2026-06-23): ONE entrance per synth zone.** These spots are perimeter edges of a synth platform that is already reachable via its single main entrance (spine doesn't cross here; 0 prev-next adjacencies). Perimeters stay walled by design — do not add per-edge transitions. |

Verified @cells=25, seeds 1–39: 0 door stacks, 0 doors-in-walls, 0 barriers, 0 floor overlaps, 0 synth walls < 1.2 m, **0 unreachable cells, 0 enclosure holes, 0 double-floors**. Diagnostics (all @25): `tools/_diag_synth.py`, `_diag_access.py`, `_diag_enclose.py`, `_diag_floors.py`, `_diag_trans.py`, `_diag_spot.py`.

---

## 0. Agent handoff

> **Latest session handover: [`docs/handover-synth-2026-06-23.md`](handover-synth-2026-06-23.md)** — read it first. Defects D1–D10 fixed, D11 resolved-by-design, **D5 (spawn fall) pending user rebuild**. Reminder: editor generates at **cells=25**, not 40.


### Verify commands

```bash
python tools/test_transition_stairs.py
python tools/test_synth_transition.py
python tools/_diag_synth.py     # door stacks / doors-in-wall / barriers / spawn  (cells=25)
python tools/_diag_access.py    # reachability from spawn                          (cells=25)
# Editor preview uses cells=25 by DEFAULT — match it:
python tools/gen_maps.py --preview --no-layout-export --seed 1 --prev-faction synth --next-faction synth
python tools/gen_maps.py --preview --no-layout-export --seed 6 --prev-faction synth --next-faction synth
```

Editor: Proc regen → **G** playtest both boundaries. **Inspect seeds before replying** — do not guess layout from code alone.

### Reference screenshot (seed 1, synth–industrial–synth)

User annotated two **missing walls** (red vertical lines) at the **north and south corners** of a wide stair opening on the elevated deck, where side doors open into corridor/rooms beside the landing. A **stair-edge piece** (red circle) is still **one cell short** of the end of the lateral run; arrow shows it must cap the **last** substrate column.

Wide layouts (width ≥ 2) are the critical case. **Confirmed: procgen `seed 1` (cells=40) is width-1 on both boundaries** — the user's wide annotations come from the saved preview maps (`userinput/maps/_synth_*.json`). Reproduce wide cases on procgen seeds 9/30 (w=7), 21/17 (w=5). The rules apply whenever the seam is wide.

> **Note for a fresh agent:** screenshots the user pastes in chat are *not* always present in your context — capture their content here (this §0) the first time so it survives. Always `tools/_inspect_integrity.py`/`_scan_wide.py` to ground yourself; do not infer layout from code alone.

---

## 1. Map model (context)

| Zone | Role |
|------|------|
| **default** (`industrial_default`) | Underground substrate — sewers, vents, subway (~60% of level) |
| **prev / next** (factions) | Faction camps at transitions (~15% / ~35%) |

**Hard rule:** `prev` and `next` must **never** share an edge without `default` between them. Fixed in `level_composition.py` via arc-length spine projection + `enforce_industrial_between_factions`.

Synth is a **high-tech elevated** faction: industrial at **y = 0**, synth deck at **1.2 m** after one flight of stairs.

Kit path: **`factions/synth`** (not `space_station`). Stairs/floor/walls use explicit **SynthMaterial** in the Rust client.

---

## 2. Spatial sequence (along approach)

Each boundary is built **in this order only**:

```
stairs (substrate, y=0) → floor deck (faction, y=0 mesh top @ 1.2 m) → foyer → door → interior
```

| Step | Where | Pieces |
|------|-------|--------|
| 1 | Default substrate at seam | `stairs-small-*` @ y=0, one module per substrate column |
| 2 | Faction cells from **stair landing** (seam row) through **door row** | **`floor` only** @ y=0 (not `floor-panel-*` for now) |
| 3 | Foyer between landing and door | optional **one** prop max (`table-display-planet`); omit piece `y` — zone elevation applies |
| 4 | Door cell | `wall-door` on **substrate-facing edge** of tile |
| 5 | Approach path (stairs → door) | **industrial** `template-wall` @ y=0 on deck **lateral** faces toward default |
| 6 | Behind door | synth `wall` @ 1.2 m — normal base-gen room walls |

### What is NOT the synth building

The **elevated deck landing and approach** (stairs → door) are **outdoor station platform**, not enclosed synth rooms.

- **Industrial walls** run along the approach until the **door** — the synth **building starts at the door**.
- **No synth walls** on the approach deck before the door.
- **No industrial walls** inside synth room interiors.
- **No balconies** (removed — were illogical).
- **No zone-wide deck replacement** — `floor` only on `deck_cells` for this transition.

### Side doors (ground corridors → deck footprint)

When an exterior synth corridor meets the transition deck footprint, emit a **building-face door** (`emit_building_face_doors`). These are **entrances** — the tile behind the door is a **room** and must obey **building integrity** (§6).

Exterior synth corridors use ground-level `floor` GLB (`factions/synth`), **not** raised `template-floor`. Only **building interior** (BFS behind main door) gets zone elevation uplift.

---

## 3. Deck & elevation

| Deck height | Walkable surface | Support piece | Placement Y |
|-------------|------------------|---------------|-------------|
| **1.2 m** (one flight) | Mesh top @ 1.2 m | **`floor`** only (for now) | **0** |
| **3.6 m** (three flights) | Mesh top @ 3.6 m | `structure-barrier` only | **0** |

**Do not use:** `structure-panel` (not 1.2 m — user rejected).  
**Do not use:** `structure-barrier` unless deck ≥ 3.6 m.  
**Do not use:** elevated `template-floor` + y offset on whole synth zone (double surface / z-fight).

**The whole synth zone is ONE elevated building — every synth cell sits at 1.2 m, so every synth wall is at 1.2 m.** (User rule, 2026‑06‑22: *"can you think of any situation where a synth wall should not be at 1.2 m? Treat every synth wall below 1.2 m as a bug and find the cause."*)

### Root cause of synth walls stuck at y = 0 (SOLVED 2026-06-22)

`interior_cells` (the elevated set) used to be `interior_cells_for_plan` — a **1‑D walk** straight back from the door along `toward`. Any synth cell that branched sideways was never elevated, so its base‑gen wall stayed at y = 0 (143–236 such walls per map). Fixes in `gen_freeform.to_doc` / `_apply_zone_elevation`:

1. `interior_cells = { every walkable prev/next cell }` — the full footprint elevates.
2. **Robust wall height:** a wall sits on a cell **face**; the float can round to either side. `_face_cells` returns *both* cells sharing the face and elevation takes the **max**, so a synth↔default seam wall (which belongs to the synth side) is correctly raised.
3. **`floor_level == 0` guard:** never raise the underground hub floors/walls (`floor_level -1/-2`) that happen to sit under a synth cell (a bug the whole‑zone change exposed).
4. Every synth floor is a thick `floor` **block** at y = 0 (top @ 1.2 m) — `_apply_synth_exterior_floors` now converts the **whole** zone (skip = deck cells only), not just "exterior" cells. Blocks stay at y = 0 (they *are* the 1.2 m support); walls sit on top at 1.2 m. No floating thin floors, no fall‑through (floor‑overlap audit clean on seeds 1–39).

Walls on elevated tiles sit at **y = 1.2 m** (top of deck), not y = 0 overlapping the floor mesh.

---

## 4. Stairs (`stairs-small-*`)

### Rule A — edge caps on both ends

Every lateral run terminates in **edge** or **corner** pieces. Single column → `stairs-small-edges` (double edge).

Edge caps must sit on the **first and last substrate cells** of the **full** lateral seam run — not one cell short. Root cause when wrong: `scan_seam_strip` truncated the run (anchor-based contiguous walk dropped cells beyond a gap). Fix: derive strip from the **contiguous faction seam row**, one substrate cell per faction seam cell.

### Rule B — width layouts (lateral opening width 1–5)

| Width | Allowed layouts |
|-------|-----------------|
| 1 | `stairs-small-edges` |
| 2 | Single `stairs-small-edges` off-centre |
| 3 | 3× straight (edge–center–edge) **or** corner–center–corner |
| 4 | 4× straight **or** corner–center–center–corner (never lone solo) |
| 5 | 1 / 3 / 5 straight modes; solo centre only at width 5 with corner variants |

Width > 5: centre a width-5 pattern, empty flanks.

### Rule C — placement & scale

- One module per substrate cell; ramp lip at zone seam (`mesh_metrics`, 2 mm overlap).
- **Uniform scale 4** for all stems in a run — do **not** per-stem scale (middle stair looked bigger; edges shrank to 3 m).
- Stairs are 1-unit modules @ scale 4 → one full 4 m cell each.

### Rule D — asymmetric edge GLBs

`stairs-small-edge` is asymmetric (x_mean ≈ −0.11; railing on local −x). Mirrored variants live in `assets/models/factions/synth/`:

- `stairs-small-edge-r`
- `stairs-small-corner-r`
- `stairs-small-corner-inner-r`

Created via `tools/mirror_glb.py`; repair materials with `tools/repair_synth_glb.py`.

**Chirality is geometric, not a static table (`_oriented_end_stem`).** Two bugs caused the wrong-facing rails (issue 1.png):

1. **Active-end keying:** the old code keyed the mirror on raw `slot 0 / width-1`, but padded patterns (e.g. width-5 `[None, edge, center, edge, None]`) put the real end pieces at slots 1 & 3. So padded runs were never mirrored and *both* ends kept the same variant → the high end's rail faced inward. Fix: key on the **active span** (`_active_span` = first/last non-`None` slot).
2. **Yaw dependence:** the outward direction is set by the run's world **yaw**, not by enter/exit. (seed 33 yaw 0° and seed 17 yaw 180° are both "exit" yet need opposite variants.)

`_oriented_end_stem` computes it directly: rail world-lateral component = `local_x_sign * factor`, `factor = cos(yaw)` (x-run) or `-sin(yaw)` (z-run); the low end wants that component negative, the high end positive, so `required_local_sign = outward * factor`. It then picks the variant with that rail sign (`_RAIL_LOCAL_SIGN` / `_VARIANTS_BY_SIGN` — note the **corner** family's sign is opposite the edge family). **No magic constant, no editor guess** — verified geometrically: 22/22 end pieces across the wide seeds face outward (`tools/_verify_rails.py`).

#### Root cause of the "white / wrong-colour flipped stair" (SOLVED 2026-06-22)

The Kenney `colormap.png` is a **colour atlas**: each face's UVs point at one solid swatch; there is no tiling surface texture. SynthMaterial (client `init_editor_kenney_materials`) samples that atlas by the mesh's **raw UV** with no `uv_transform`. The old `mirror_glb.py` flipped U (`u → 1−u`) when mirroring, which moved every face to a **mirrored column of the atlas → a different colour**. Verified: plain `stairs-small-edge` UVs sit at `u ∈ [0.219, 0.344]` (same column as `floor`); the bad `-r` was at `u ∈ [0.656, 0.781]`.

**Fix:** `mirror_glb.py` no longer flips TEXCOORD_0 — it mirrors geometry/normals/winding only and keeps UVs, so the `-r` samples the same swatches. Re-mirror + `repair_synth_glb.py` after any geometry change. This is NOT a Rust-side material-slot issue: `kenney_material_slot` already returns `Synth` for every `factions/synth` stem (incl. `-r`).

### Rule E — multi-floor (optional, deprioritized)

When depth ≥ 3 and width ≥ 2 (~28%): one column **3 flights** (→ 3.6 m). `structure-barrier` only on 3.6 m columns.

### Rule F — deferred

`stairs-ramp` — future vehicle/garage paths. Regular stair + handrail family for very wide openings — later.

### Known stair bugs (user reports)

| Symptom | Cause / fix |
|---------|-------------|
| Edges at z=38 not z=42 when run is 30–42 | Strip truncated — §4 Rule A / `scan_seam_strip` |
| Middle stair bigger, gaps between pieces | Per-stem scale — use uniform scale 4 |
| Both/one edge facing wrong way | Chirality keyed on padded slot vs active end, and is yaw-dependent — now geometric (`_oriented_end_stem`, §4 Rule D) |
| Flipped edge white/wrong color | `mirror_glb.py` flipped UVs → wrong atlas swatch. Fixed: no UV flip; re-mirror + repair (§4 Rule D root cause) |

---

## 5. Door & foyer

| Rule | Detail |
|------|--------|
| **Depth** | **2 cells** from stair landing (seam row = depth 1). User corrected from 4. |
| **Position** | On **substrate-facing edge** of door tile (`_cell_face_pose`), **not** cell centre only |
| **Yaw** | Substrate-facing (`toward_substrate` wall yaw) |
| **Props** | Max **1** per entrance; no explicit `y` on prop (zone elevation) |
| **Depth bug** | `_depth_into_faction` must step **+toward_faction**, not back toward default |

---

## 6. Building integrity (critical)

User analogy: *if a house has no walls, it can't keep heat; if it has missing walls, it can rain in.*

### Formal rule

Every **synth room tile** must be closed on all **4 cardinal sides** by exactly one of:

1. Another **room tile** (same enclosed space),
2. A **wall**, or
3. A **door**.

If a side opens to **outside** (void, industrial default at deck level, or open stair well), the building **does not maintain integrity**.

### Entrance ⇒ room ⇒ closed (user's exact articulation, 2026-06-22)

If you place a door at the side after the stairs, that **is an entrance**; the tile behind it is a **room**; that room must be closed on all 4 sides (room / wall / door). If none of the three holds on a side, the cell is **directly connected to the outside** and the building loses integrity — *"if a house has missing walls, it can rain in."* The industrial floor/walls are the map's **default**; until the actual synth **building** starts (the door, ~2 tiles past the stairs) the approach walls are **industrial**. Synth doors+walls begin at the building and must form a closed shell.

### Root cause of the missing walls (SOLVED 2026-06-22)

Base-gen (`gen_freeform.emit_pieces`) walls a side **only when its neighbour is the void** (non-walkable). It **never** walls a synth cell against a *walkable* `default` (industrial) cell — it treats adjacent walkable cells as connected open space (a doorway). So every synth/`default` **walkable seam leaks**: the cells flanking a wide stair run, and any synth corridor that abuts a sewer, have no wall. That is the user's "missing walls next to the stairs" and the general integrity holes.

**Inspection ground truth:** procgen `seed 1` (cells=40) produces only **width-1** transitions; the wide stair runs / side-door layouts the user annotates ("22,38–22,42", "24,44") come from the larger **saved preview maps** (`userinput/maps/_synth_*.json`), not procgen seed 1. Wide cases reproduce on procgen seeds 9, 30 (w=7), 21, 17 (w=5). Use `tools/_inspect_integrity.py <seed>` (open-side audit) and `tools/_scan_wide.py` to find them.

### Fix: `emit_synth_envelope_walls` (synth_transition.py, called once in `to_doc`)

Closes the footprint: for every synth (`prev`/`next`) cell, for each side facing a **walkable non-synth** cell (skip void faces — base-gen owns those; skip stair mouths; skip deck cells), close the seam one of two ways:

- **Corridor crossing** (either the synth cell or its `default` neighbour is a corridor cell): it is an **intended path**, so emit a **synth door** on the synth face (y = 1.2 m) **plus ascending stairs on the `default` cell** (0 → 1.2 m) — *never* a blocking wall. This satisfies the user rule **"wherever you put a door, there must be a stair down before you reach the next faction's level."** Stairs dedupe per `default` cell. Verified: crossing-stair lips align to their own cell seam (0.0 m error).
- **Otherwise** (room edge): a synth `wall` at 1.2 m.

Connectivity (every synth wall/door touching another) is then an **emergent** property of closing the footprint, **not** an imposed "must touch a wall" rule. Verified across seeds 1–39: 0 synth walls < 1.2 m, 0 floor overlaps, 0 open holes (only stair mouths / crossings stay passable).

> **Earlier failure mode (fixed):** the first envelope pass walled corridor crossings too, *blocking passages*. Corridor crossings must be door+stairs, not walls.

### Consequences for transitions

- **Main door** on deck → room behind it must be enclosed by base-gen + transition + envelope walls.
- **Side doors** on deck landing (corridors meeting footprint) → room behind **each** side door enclosed by the envelope pass.
- At the lateral ends of a **wide stair opening**, the flanking side cells meet the open stair well — closed by `emit_room_integrity_walls` (deck-height corner) and/or `emit_synth_envelope_walls` (flanking room cell). Verify in editor.

### What NOT to do

- **`emit_synth_building_walls` (reverted)** — spawned disconnected `building_wall` pieces everywhere. User explicitly asked to revert. The replacement `emit_synth_envelope_walls` is *not* that: it only walls **existing synth↔walkable-non-synth seams**, so walls land on the real footprint outline (connected by construction), never free-floating.
- Do **not** set a naive global post-pass “every synth wall must touch another wall” — achieve connectivity by **correct local placement** (close the footprint seams; connectivity is emergent).
- Do **not** block the **stair opening** toward default (intentionally open).
- Do **not** wall across a **door** face.

### Wall types recap

| Location | Wall | Height |
|----------|------|--------|
| Substrate lateral to stair block | `template-wall` (industrial) | y = 0 |
| Deck lateral toward default (approach) | `template-wall` (industrial) | y = 0 |
| Deck corner beside wide stairs + side door | `wall` (synth) | y = 1.2 m |
| Room interior / behind main door | `wall` (synth, base-gen) | y = 1.2 m on elevated cells |

---

## 7. Issue log (chronological user feedback)

| # | Report | Resolution |
|---|--------|------------|
| 1 | Stairs facing wrong way | Fixed yaw via travel direction + ascend/descend |
| 2 | Stairs should be **before** door; both boundaries need stairs | Sequence §2; enter + exit both emit stairs |
| 3 | Fall through world when starting on synth | Spawn Y from elevation lookup |
| 4 | Gap between stair lip and deck | `mesh_metrics` seam alignment |
| 5 | Duplicate floor under doorway | No deck on door cell; no double template-floor |
| 6 | 5-wide edges don't wrap 3 centres | Uniform scale 4; full seam strip width |
| 7 | Door too close to stairs | Door depth = **2** cells |
| 8 | Door at cell centre vs edge | Edge placement on substrate-facing side |
| 9 | 4-wide with solo stair | Width ≥ 4 forbids solo |
| 10 | structure-barrier / structure-panel everywhere | Use **`floor`** @ 1.2 m; barrier only @ 3.6 m |
| 11 | Structures above tiles | Deck/support at **y = 0** under player |
| 12 | Door rotated not repositioned | Fix position, keep wall yaw |
| 13 | Wrong wall types / walls inside rooms | Industrial only on approach; synth from door onward |
| 14 | User corrected: use **floor** GLBs not structure-panel | §3 |
| 15 | seed 1: edges at 22,38 not 22,42 | §4 Rule A — strip truncation |
| 16 | Middle stair bigger / gaps | Uniform scale 4 |
| 17 | Planet prop 1.2 m below floor | Omit prop `y`; max 1 prop |
| 18 | Balconies illogical | **Removed** |
| 19 | Extra walls at 18,30–22,30 (nonsense) | Reverted blanket wall pass |
| 20 | Door 4 tiles away — wanted 2 | min_door_depth = 2 |
| 21 | Mixed floor stems — use **`floor` only** | `synth_deck.pick_deck_stem` |
| 22 | Walls at y=0 on elevated tiles | Walls @ 1.2 m on deck |
| 23 | Mirrored edge GLB needed | `mirror_glb.py` + material repair |
| 24 | Industrial walls must reach door; no synth walls before building | §2, §6 |
| 25 | Exterior corridors: ground floor, side door at deck | `emit_building_face_doors` + exterior floors |
| 26 | prev/next touching without industrial | `enforce_industrial_between_factions` |
| 27 | Side doors need closing walls | §6 integrity corners |
| 28 | Blanket wall emit wrong — **revert** | Use targeted corner walls only |
| 29 | seed 1 screenshot: 2 missing corner walls + stair-edge still short | §0, §4, §6 |
| 30 | "you flipped the wrong stair — both edges wrong way" | Chirality is orientation-dependent; `_stem_for_lateral_slot` now takes `ascending`; flip `_LOW_END_MIRRORED_WHEN_DESCENDING` if still inverted (§4 Rule D) |
| 31 | Flipped stair white / wrong colour | `mirror_glb.py` flipped UVs → wrong atlas swatch; now keeps UVs; re-mirrored + repaired (§4 Rule D root cause) |
| 32 | Walls next to stairs missing; "house with holes" | Base-gen only walls void faces, not synth↔default-walkable seams; added `emit_synth_envelope_walls` (§6) |
| 33 | "you did something very wrong with the walls — revert" | Reverted blanket pass; envelope pass walls only real footprint seams (connected by construction) |
| 34 | Side door ⇒ room behind ⇒ must be closed 4 sides | Envelope pass + entrance/room articulation (§6) |
| 35 | Rightmost stair edge rail faces middle (1.png) | Chirality keyed on padded slot 0/width-1, not the **active** end; also yaw-dependent. Now geometric: `_oriented_end_stem` picks the variant whose rail faces outward (§4 Rule D). Verified 22/22 end pieces. |
| 36 | "any synth wall not at ≥1.2 m is an error" | Whole synth zone elevates (was 1-D BFS line); `_face_cells` max; `floor_level==0` guard (§3). 0 low walls seeds 1–39. |
| 37 | Integrity walls block corridors — want doors | Corridor crossings → synth door + ascending stairs (0→1.2), not a wall (§6) |
| — | "seed 1" wide layouts not in procgen seed 1 | procgen seed 1 = width-1; wide cases are saved preview maps / seeds 9/30/21 (§6) |

---

## 8. File map

| File | Responsibility |
|------|----------------|
| `tools/synth_transition.py` | Plan + emit stairs (orientation-aware chirality), deck, door, foyer, approach walls, integrity walls, side doors, **envelope walls** |
| `tools/_inspect_integrity.py` / `_scan_wide.py` | Audit open synth seams per seed / find wide transitions |
| `tools/synth_deck.py` | Transition deck `floor` @ y=0 |
| `tools/transition_entrances.py` | Route elevated factions to synth assembler |
| `tools/mesh_metrics.py` | GLB probes (stairs, floor tops) |
| `tools/mirror_glb.py` | Mirror asymmetric stair GLBs |
| `tools/repair_synth_glb.py` | Fix colormap on edited GLBs |
| `tools/level_composition.py` | Zone paint, elevation lookup, prev/next buffer |
| `tools/gen_freeform.py` | Orchestration, exterior floors, zone elevation |
| `assets/models/factions/synth/` | Kit GLBs + `faction.json` |
| `userinput/factions/synth.json` | `elevation_rise: 1.2`, door/stair specs |

---

## 9. Remaining / later

- [ ] Per-cell elevation map for split 1.2 m / 3.6 m columns on one boundary
- [ ] Re-enable `floor-panel-*` stem selection once `floor`-only pass is stable
- [ ] Regular (non-small) stair family + handrails for very wide openings
- [ ] Port pattern to priesthood / outlaw / necropolis elevated profiles
- [ ] `stairs-ramp` for vehicle ramps

---

## 10. Exit criteria (synth transition v1)

- [ ] No duplicate walkable surfaces at transitions
- [ ] Deck visibly 1.2 m from `floor` blocks, not floating template-floor
- [ ] Stair edge caps on **first and last** substrate column of full seam run
- [ ] Uniform stair scale 4; lips meet deck
- [ ] Door **2** cells deep; correct edge position/yaw
- [ ] Industrial approach walls only; synth walls from door / integrity corners
- [x] Wide stair + side doors: footprint closed — no synth↔default-walkable holes (`emit_synth_envelope_walls`; verified seeds 9/30/21). **Still verify in editor.**
- [x] prev/next never adjacent without industrial (`test_no_prev_next_adjacency`)
- [x] Mirrored stair colour correct (UV atlas not flipped)
- [ ] Stair end-railing chirality correct in editor for BOTH enter & exit (flip `_LOW_END_MIRRORED_WHEN_DESCENDING` if not)
- [x] seed 1 + seed 6 procgen generate without error
- [x] Tests green (`test_transition_stairs.py`, `test_synth_transition.py`)
