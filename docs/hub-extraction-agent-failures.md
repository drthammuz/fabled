# Hub / extraction — agent iteration failure log

**Status:** UNRESOLVED — user blocked further agent fix attempts (2026-06-17).  
**Next step:** Open a **GitHub pull request** so [Cursor Bugbot](https://cursor.com/docs/bugbot) can review the diff (enable repo in Bugbot dashboard; comment `bugbot run` on the PR if reviews are manual-only). See [AGENTS.md](../AGENTS.md) and [`.cursor/BUGBOT.md`](../.cursor/BUGBOT.md).

This document summarizes the **last five user prompts** in the hub / extraction / editor-G-playtest thread, what was reported, what the agent attempted, and why trust was withdrawn.

---

## Prompt 5 (earliest of the five)

**User:** Diagonal visual opening at the entrance to the stairs-room; tiles above the stairs are neither visually nor physically open. Asked whether probes were being used, whether something is fundamentally wrong with the agent’s understanding of the code, and whether a different LLM model is needed.

**Reported symptoms:**
- Unexplained **diagonal visual wedge** near the west-module / stairs entrance.
- **Floor above L2 stairs** (hub band, ~`(6, 20)` world) appeared **solid on both channels** — could not walk down.

**Note (added later):** In-game confirmation of whether that tile is physically open may be **impossible with current movement**: step-up / stair height logic keeps the player at stair elevation when they walk up a few steps and then walk backwards off the stairs — they **hover at that altitude** and do not fall to hub floor level, so absence of a hole cannot be distinguished from “open but unfallable.” Probes are the reliable check for physical openness there; playtest feel alone is inconclusive.

**Agent response pattern:** More mesh-cutout / layout-patch tweaks; added or adjusted probe scripts; claimed architectural “dual pipeline” understanding (client visuals vs server physics).

**Outcome:** Problems persisted into later prompts.

---

## Prompt 4

**User:** Cannot fall down the trap anymore — “how come there’s always a new problem introduced?”

**Reported symptoms:**
- Extraction pit at floor 0 **physically closed** (regression after prior “fixes”).

**Agent response pattern:** Collider skip rules, floor mask, grounding suppression, shaft landing nudge — each fix risked breaking the previous state.

**Outcome:** Partially addressed in next prompt (trap physically open again) but visual channel still wrong.

---

## Prompt 3

**User:** Still cannot drop down to hub.

**Reported symptoms:**
- Pit / shaft path from floor 0 to hub floor −1 not traversable.

**Agent response pattern:** `patch_hub_branch_layout`, `infer_extraction_xz`, floor-cell skip logic, playtest layout export.

**Outcome:** Physics improved for some tiles; visual vs physical split remained.

---

## Prompt 2

**User:** Four concrete mismatches; demanded **systematic** evaluation of **all relevant tiles** (visual **and** physical) before returning; root-cause analysis, not hotfixes.

**Reported symptoms:**

| # | Location / issue | Physical | Visual |
|---|------------------|----------|--------|
| 1 | Floor 0 trap → hub | Open | **Closed** |
| 1b | Hub −1 tile below pit | Open (fall 2 floors) | **Closed** |
| 2 | Floor above stairs | **Closed** (user report; see step-up caveat below) | **Closed** |
| 3 | Diagonal opening (in door, over stairs, ~45°) | Closed | **Open (artifact)** |
| 4 | Hub −1 tile just outside door | **Closed** | **Open** |

**User requirement:** Probe every relevant tile; explain **why**, not patch symptoms.

**Agent response pattern:**
- Identified **dual pipeline**: editor visuals baked once (`editor_apply_materials` / stale meshes); server rebuilds from `play_layout(true)` with cutouts.
- Added `sync_playtest_mesh_cutouts`, `hide_extraction_hatch_piece` despawn, `floor_tile_half()`, `tools/probe_hub_tile_audit.py`.
- Ran Python probe claiming PASS on key tiles — **did not match in-game user experience**.

**Outcome:** User still saw widespread visual/physics disagreement.

---

## Prompt 1 (latest — agent forbidden from further fixes)

**User:** Agent has **spent its trust** and is **not allowed to try again**.

**Reported symptoms (current game state):**
1. **Floor above stairs:** user reported physically **open**, visually **still closed** — but **physical openness at `(6, 20)` cannot be verified in playtest** while step-up logic holds the player at stair height when backing off the stairs (hover, no drop). Treat physical state as **probe-only** until movement is fixed or a dedicated fall test exists.
2. **Trap door:** visually **open**, but agent **removed `template-floor-hole.glb`** — user wanted the hole **frame** prop, not only mesh cutouts.
3. **Diagonal visual hole** outside the door and over the stairs **still present**.

**User instruction:**
- Write this summary in an error doc.
- Link from `AGENTS.md` and `README.md`.
- State that **Bugbot should read this** before attempting hub/extraction fixes.

---

## Resolution plan (locked with user, 2026-06-19)

**Root cause:** open/closed of a tile was decided independently by ~6 functions across two pipelines (client visual carving vs server physics), using hub-specific geometric heuristics. They could not stay in sync.

**Fix (single source of truth = `floor_mask` per level):**
- A floor exists at a cell **iff `floor_mask.get(ix,iz)` is true**, for both visual mesh carving and physics.
- Delete hub-specific geometric zone functions for floors (`in_hub_drop_column`, `hub_stairs_opening`, `in_hub_l3/west_drop_zone`, `pit_shaft`, geometric `suppress_extraction_grounding`). Replace with one mask lookup keyed by player floor level (`y/MOD_H` rounded).
- Floor-hole carving on shells is uniform and mask-driven (remove floor tris whose cell is masked-false), identical code for client mesh and server trimesh.
- Per-cell physics cuboids spawned for masked-true cells (unchanged), no geometric skips.

**Locked hub vertical layout (user decisions):**
- Floor-0 trap at `(ex,ez)` → **land on hub floor −1** (the `(ex,ez)` tile on −1 is **SOLID**).
- Hub floor −1 has **three separate openings away from the landing tile**: stairs (L2), pit drop (L3), west drop (L4).
- Floor visuals rendered from the mask (kit floor tiles acceptable); correctness over exact baked look.

**Verification:** `probe_hub_tile_audit.py` asserts visual==physical==mask per tile, **and** an in-engine debug overlay shows per-tile floor state in G playtest (walking can't fall-test stairs due to step-up).

---

## Cross-cutting technical themes (for Bugbot)

1. **Two pipelines:** Client `EditorPlaced` meshes vs server `play_layout(true)` colliders — cutouts, hatch pieces, and generation bumps must stay in sync.
2. **`template-floor-hole.glb`:** Despawning it removes the visible frame; mesh cutouts alone may not match art intent. Diagonal wedge may be the hatch mesh itself or incorrect cutout math.
3. **Room-shell floor cutouts:** Centroid vs AABB vs multi-cell floor tris — probes passed while editor playtest did not.
4. **Probe false confidence:** `probe_hub_tile_audit.py` reported PASS; user in **G** playtest still sees mismatches — probes may not model playtest sync timing, hatch despawn, or material/mesh handle caching.
5. **Regression churn:** Each fix on physics (open stairs floor) broke visuals (trap frame removed) or reintroduced adjacent-tile bleed.
6. **Stairs tile not fall-testable in playtest:** Step-up / stair snap keeps Y elevated after walking up and backing off the stairs, so the player **does not drop** to hub floor height over the opening. User cannot tell from walking whether `(6, 20)` is physically open or closed — only mesh probes, collider debug, or fixed movement can answer that. Do not treat “I didn’t fall” as proof the floor is solid.

---

## Key files touched in this thread

| Area | Files |
|------|--------|
| Cutouts / drops | `crates/shared/src/kenney_pit.rs`, `crates/shared/src/kenney_hub.rs` |
| Playtest sync | `crates/client/src/editor_playtest.rs`, `crates/client/src/test_showcase.rs` |
| Physics | `crates/server/src/level.rs`, `crates/server/src/character.rs` (step-up / stair height — affects ability to fall-test stairs opening) |
| Layout | `crates/shared/src/map_pool.rs`, `userinput/kenney_layout.json` |
| Probes | `tools/probe_hub_tile_audit.py`, `tools/probe_hub_exits.py`, `tools/probe_extraction.py` |

---

## Verification commands (for humans / Bugbot)

```bat
python tools/probe_hub_tile_audit.py
python tools/probe_hub_exits.py userinput/maps/level_stretch.json
cargo test -p shared kenney_pit
```

Editor: `--host --editor`, quicksave, **G** playtest — compare pit, hub −1 drops, west door threshold `(8,20)`, stairs opening `(6,20)`.

**Stairs opening `(6, 20)`:** Do not rely on walking backwards off the stairs to test physical openness; step-up logic may leave you airborne at stair height. Use probes or explicit collider checks.

**Do not declare fixed until in-game G playtest matches probes on all listed tiles** (except stairs physical open/closed, which requires probes or movement fix).

---

## 2026-06-19 follow-up (tiled-floor architecture + Quake controller)

After the move to per-cell `template-floor` / `template-wall` rooms and a single `FloorMask`
source of truth, the user reported two remaining issues. Root causes and fixes:

1. **Missing `template-floor-hole` frames** (extraction trap + the two hub drop holes).
   - *Cause:* `kenney_pit::floor_prop_on_hole` returned `true` for any `*hole*` stem, so the
     frame was removed from the playtest layout (`patch_hub_branch_layout` retain) and hidden
     by `hide_extraction_hatch_piece` — the "legacy diagonal wedge" decision. With carving gone
     the raw GLB is a clean raised rim, so that suppression was no longer warranted.
   - *Fix:* `floor_prop_on_hole` now returns `false` for the hole *frame* (renders it) and only
     suppresses *solid* `template-floor` tiles over a mask hole. The frame's collider is still
     skipped by `kenney_skip_piece_collider` (floor < 0 / the extraction tile), so the hole stays
     physically open. `gen_maps.py` now places a `template-floor-hole` at the two hub drop holes
     (`build_tiled_floor_room` `frame_holes`). Probe mirrors both in `floor_prop_on_hole` +
     `skip_physics_collider`. Stair-opening cells get **no** frame (the stairs piece occupies them).

2. **Can't walk up stairs / float at the top / can't jump there.**
   - *Cause:* `over_open_hole` forced `on_ground=false` on any cell that is a mask hole at the
     player's floor band. The stair-opening cells are holes in the `-1` mask (so you can descend),
     and `floor_level_at_y` maps the **upper half** of the staircase to level `-1` — so the whole
     top half of the stairs was grounding-suppressed: no step-up (walk), no jump at the top.
   - *Fix:* removed the `over_open_hole` checks from `categorize_position` and `stay_on_ground`
     (and the now-dead `extraction_xz` helper + imports). With tiled floors, real holes have **no
     collider** (no tile, no cuboid), so the downward probe finds nothing and the body falls
     naturally; stairs are a collider that passes *through* the mask hole, so they stay groundable.

3. **West drop blocked the path to the stairs.** It sat on the hub centre row `(ex-8, ez)`
   between the landing and the west gate, so you had to cross it to reach the stairs. Per the
   user's choice it was relocated to the SW corner `hub_west_drop_centre = (ex-8, ez+8)`, which
   still drops into the same room below; the centre-row cell `(ex-8, ez)` is now solid. Updated
   in `kenney_pit::hub_west_drop_centre` (single source), `kenney_hub::in_west_drop_commit` /
   mask cut, `gen_maps.py` (`WEST_DROP_DZ`), the `map_pool` test, and the probe (incl. a new
   "gate path SOLID" expectation).

Verified: `cargo build` (libs compile), `python tools/gen_maps.py --seed 42`,
`python tools/probe_hub_tile_audit.py` → PASS on all tiles incl. framed holes and both stair cells.

### 2026-06-19 round 2 (collision + step-up)

1. **Hole frames had no collision.** The frame is a raised rim with an open centre and
   should collide (stand on the rim, fall through the middle). `kenney_skip_piece_collider`
   now never skips `template-floor-hole`; probe mirrors it. Centre stays open (probe still
   PASS) because the rim only occupies the cell edges.
2. **Invisible double collider in L3 (room below hub).** `apply_hub_branches` only stripped
   the hub module's `-1` band, leaving a leftover `room-large` shell at `-2` overlapping the
   tiled L3 room (visible as a phantom collider near the centre). Now strips `{-1, -2}` for
   the hub module. The Rust fallback also no longer re-adds a `room-large` when L3 already has
   floor coverage (`has_floor_coverage` replaces `has_room_shell`).
3. **Couldn't walk up stairs (only jump).** The Kenney `stairs` mesh rises only **0.29 m per
   step** (the old `0.62 m` comment was wrong), well under the 0.65 m step height — so this was
   a step-up *logic* bug, not a height limit. A capsule's rounded base rolls partway up a low
   step during the flat slide, inflating `flat_dist` so the `step_dist > flat_dist` gate never
   fired. `move_character` now also takes the stepped path when it lands on a *higher* walkable
   surface than the flat move (`climbed > min_climb`); walls are still excluded because their
   down-trace finds no walkable top.
4. **Stair-top ~0.3 m gap:** left for re-test — with walking step-up restored, the remaining
   gap is one normal step; revisit stair Y/scale only if it still reads wrong in playtest.
