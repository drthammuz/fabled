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
