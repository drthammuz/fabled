# Handover — faction interior placement automation (2026-06-29)

**Date / branch:** 2026-06-29, freeform/ceiling-roofs.  
This session extended interior dressing automation from synth-specific to all 5 factions (industrial, priesthood, synth, outlaw/urban, necropolis). Focus moved to per-faction placement catalogs, room-role logic, overlap/relation rules, wall alignment, corridor handling, and optimization sweeps.

**Read first (automation mindset):**  
- [docs/synth-master-plan.md](synth-master-plan.md) — especially the **Critical Context** (history of manual feedback cost + rules for relations, density, no-overlap, center bias, wall/floor/stair) and **Automation-First Plan** (script/CPU over human-per-object loops). All catalog and placement work must stay script-driven.  
- [docs/handover-synth-2026-06-23.md](handover-synth-2026-06-23.md) (synth dressing context).  
- [docs/faction_roster.md](faction_roster.md) and [docs/faction_assets.md](faction_assets.md) (what props belong where per docs).  
- Previous: [docs/handover-factions-2026-06-22.md](handover-factions-2026-06-22.md) (asset folders).  

**Key files for placement:**  
- `tools/probe_faction_catalog.py` (auto-catalog from GLBs)  
- `assets/models/factions/*/placement_catalog.json` (per-faction, enriched)  
- `tools/faction_interior.py` (unified furnish, place_with_catalog, ROLE_SETUPS, wall align, blockers, resolve)  
- `tools/run_faction_placement_sweeps.py`  
- `tools/gen_freeform.py` (integration for zones)  
- Short bats: `run.bat`, `s.bat`, `g.bat`, `e.bat` (user tools).  

---

## 1. User's initial intentions (from session start and prior context)

The user wanted to escape the costly manual feedback loop documented in synth-master-plan (repeated "try this, user reports bad overlap/facing, fix, repeat").

Core goals:
- **Full script-driven automation** for all faction interiors.
- Auto-probe every GLB in faction folders: physical/visual bounds (min/max, half sizes, height, CoM, flatness, protrusion, clearance), purpose/usage (inferred + rules), relations (e.g. grave+candle or bench+wall with yaw/offset/clearance), density hints, tags.
- Placement that automatically satisfies:
  - No overlaps/crowding (strict BBox + gap).
  - Free paths.
  - Center avoidance unless focal (altar-like).
  - Benches/seating placed against walls with **correct facing** (back to wall; use probe to locate backrest precisely).
  - Roadblocks/barriers used specifically to block corridor entrances (and allowed inside corridors); nothing else in corridors.
  - Graves in logical structured rows **by walls, next to each other** (modeled on synth bed/wall placement logic), with bench/candle nearby.
  - Diverse, purposeful items per faction docs/roster (factory/machinery for industrial, retro_fantasy props for priesthood, graveyard variety for necro, furniture for synth/urban, etc.). Not just "rubble" or "random pillars".
  - Priesthood decor should resemble logical "ruins" (not random overlapping walls/pillars/doors).
  - No cross-faction bleed (e.g. priesthood pillars appearing in synth areas).
  - Synth-specific issues fixed (raised floors always with stairs, no pollution).
- Workflow: 2-min sweeps per faction (all 5 ~10min via `run`), 3 loops. AI reviews logs + adapts code/scripts between. User primarily does `run` + editor (Proc tab + G playtest) for validation. AI owns p/v (probe/verify) and script edits.
- After initial "walls/height/elevation" fixes, pivot to "decoration items" with catalog + relations + automation so user gives feedback once, not repeatedly.
- Keep within existing limits (no arbitrary new GLB copies unless per docs/roster; use probe + existing in factions/).

The explicit instruction was to use sweeps more for "script-edit dependent" improvement than blind re-runs, and to make probes detect things like wrong bench facing automatically.

---

## 2. What was done / attempted in this session

- Extended from wall-height fixes (non-uniform scale_y, brick-wall2 for necro, y=0 pins for ground factions like urban/outlaw, elevation in level_composition/gen_freeform) to full interior automation.
- Built `probe_faction_catalog.py`: iterates factions, loads GLB verts, computes bounds, visual CoM, flatness, protrusion, clearance; applies PURPOSE_RULES / RELATION_RULES / DENSITY_HINTS; writes per-faction `placement_catalog.json`.
- `faction_interior.py`:
  - `set_faction_context`, CURRENT_CATALOG, BBox with rotation-aware overlap + padded gap (from SweepParams).
  - `place_with_catalog`: jitter from mid, avoid_center (density), BBox no-overlap within call, nudge, basic relations (offset_z or fallback array).
  - Room analysis + role assignment (chapel/crypt/storage/breakroom/ruins/quarters) via classifiers using area/mouths.
  - ROLE_SETUPS + setup_* functions with per-faction logic.
  - `_maybe_align_to_wall`: edge bias + yaw for benches/pallets using catalog purpose/usage/"against wall" (updated multiple times with 180° flips).
  - Corridor mouth detection (`opens_to_corridor`) + explicit blockers for urban/outlaw at entrances.
  - Extra scatter routed through placer for safety; added `resolve_overlaps` (iterative nudge) + calls in furnish.
  - `_get_faction_decor_stems` expanded to pull from catalog where possible + keywords from docs (factory, etc.); fallbacks updated.
  - Integration hooks, sweep support, synth generic fallback.
- `run_faction_placement_sweeps.py`: synthetic rooms, vary gap_multiplier, score (props + free space - center - overlaps), logs.
- Bats refined for user (run for full cycle, s for sweeps, g/e for editor, safe CMD, no auto e after sweeps).
- Catalog fixes, synth load (wall_face_offset shim + inject), lower counts, structured rows attempt for graves, relation handling.
- Sweeps run (multiple times), logs reviewed, code adapted between (yaw, resolve, blockers, counts, catalog pull, rows vs jitter).
- Some cross-checks against docs (roster, kenney_kits_catalogue, handover-factions) for prop mapping (factory for industrial, retro_fantasy for priesthood, etc.).

Early session also covered wall height/scale fixes (non-uniform for necro using user's brick-wall2, outlaw/urban y=0 pins) and elevation to stop synth-bleed into ground factions.

Live testing via editor Proc tab (cells=25) + G.

---

## 3. Summary of user's complaints (especially repeated ones)

Recurring themes across the session (user had to restate specifics):

- **Overlaps everywhere (repeated multiple times):** Pallets stacked/same position; benches + roadblocks/blocks/concrete cubes overlapping; ramp overlapping pallet; graves overlapping each other and benches; general crowding. "how can you not solve this automatically?"
- **Bench placement against walls (repeated, core complaint):** Attempts made them "even worse"; wrong facing (not back to wall); not close enough to wall; "facing away from wall". User had to repeat "backs against walls", "locate the back rest" "more than 1 time". "if i tell you a specific thing, and you ignore it..."
- **Roadblocks / barriers (repeated):** Placed worse; not at entrances blocking paths; "i see no roadblock inside corridor"; "the only thing that i allow you to put in corridor entrances and in corridors".
- **Graves / necropolis (repeated):** "way too many graveyards"; spread out / bunched in middle instead of "placed next to each other"; overlapping; "should be placed similarly to bed placement logic in synth, by walls"; bench overlapping graves; "bench at least facing away from wall, but not close enough".
- **Priesthood (repeated):** "same illogical random placement of doors and pillars"; "overlapping, and not resembling anything that could have been a building before"; "don't know why you're still placing random pillars and walls"; "the placement is not only within priesthood"; only walls/pillars, no diversity.
- **Lack of automation / probe use (repeated):** "you should be able to probe the bench extensively to locate the back rest, and position it against a wall, without me having to tell you more than 1 time"; "these are all things that you should be able to fix automatically"; "with probes probably???"; "the fact that we still have overlapping items, how can you not solve this automatically?"
- **Diversity / docs adherence:** Industrial "only rubble now? maybe the odd pillar"; "aren't there really good industrial decorations from a specific kenney pack"; "have you checked the documentations from before"; priesthood not using full available props; synth "some new bugs".
- **Cross-faction / scope bleed:** "the placement is not only within priesthood"; priesthood pillars in synth areas; "random pillars (but that is from priesthood i think, being placed within the synth area?)".
- **Synth specific:** "raised floor without staircase"; new bugs after changes.
- **Overall process frustration:** "what was the last 15 minute loop about? what's the point?"; "completely pointless"; loops not reducing the need for repeated specific instructions.

User emphasized base structure (catalog + furnish skeleton + bats + sweeps) is now working; focus on quality of placement logic and true automation so feedback is given once.

---

## 4. What was unable to fix / remaining gaps (honest assessment)

Despite multiple edit-sweep-review loops:
- **Bench facing / wall proximity:** Align logic added/iterated (edge bias, yaw flips +pi, catalog purpose check), but results were reported worse or still incorrect (wrong direction, not close). Probe catalogs capture front/usage but not precise backrest geometry, visual orientation, or "how close is close enough" for seating. No full back-detection using verts/CoM like synth bed_anchor. 180° attempts didn't satisfy.
- **Overlaps:** place_with_catalog has BBox + CURRENT_PLACE_GAP + resolve_overlaps (nudge passes) added late; post-room resolve + shared bbs in relations helped in tests but not enough in editor (pallets, graves, mixed items still overlapped). Rotation bbox, multiple placer/manual calls in one room, jitter vs exact spacing, small items (pallets) with loose gaps, and ramp items not fully constrained. Not "automatic" at the level of probe-verified zero-overlap.
- **Roadblocks/corridors:** Mouth blockers implemented for urban at entrances using opens_to_corridor + bias. But not appearing "inside corridor", not reliably blocking, and placement "worse". No full corridor-cell population or orientation rules for narrow spaces. User intent (only allowed item type there) only partially realized.
- **Graves / structured rows:** Switched from pure jitter to wall-edge rows + candles + occasional bench (inspired by synth bed logic). Still "too many", "spread out", "bunched", overlapping. No true grid/adjacent pairing or clearance between graves + bench. resolve didn't catch all.
- **Priesthood "ruins" logic:** Reduced counts, added gate variety, edge bias, catalog pull. Still "illogical random", "overlapping", "not resembling building before". No higher-level "ruins" structure (e.g. aligned fallen pillars, logical wall remnants vs scatter). Limited props in folder → mostly template-detail looking pillar-like. Cross-zone bleed not fully prevented.
- **Diversity per docs:** _get and catalog pull updated with keywords from faction_roster/kenney_kits_catalogue (factory pipes/conveyors/machines for industrial, retro_fantasy for priesthood, etc.). But factions/*/ folders are incomplete (industrial mostly floors only; priesthood heavy on arch not props). Placement stayed limited/rubble-like in practice. No auto-copy from source kits.
- **Cross-faction / synth pollution:** Guards in gen_freeform (skip synth zones for fi) and kit setting; still reported priesthood items in synth areas. Separate synth_interior path vs unified not airtight for all seeds/Proc paths. "raised floor without staircase" surfaced as new after changes (stair emission / elevation interaction).
- **True probe-based auto-detection:** Catalogs enriched with bounds/purpose/relations. No integrated "probe the bench for backrest and verify facing/overlap in loop" (beyond basic catalog + sweep scoring). verify tools more synth-oriented. User still needed to describe facing/overlap specifics repeatedly.
- **Other:** Roadblock "inside corridor" never materialized; industrial diversity not achieved within folder limits; bench "pointless" after attempts.

The automation reduced some manual cost but did not reach the "tell me once, probe detects + fixes" threshold for facing, exact adjacency, corridor usage, or ruins-like structure. Over-reliance on jitter + per-call checks + late resolve, plus incomplete prop sets, left quality issues.

---

## 5. Recommendations for next work (automation-first, no repeat loops)

- **Deepen probing for orientation:** Extend probe_faction_catalog (or new visual probe) to detect backrest (highest point cluster or flat back vs seat), front/back vectors from mesh, suggested wall inset from clearance. Store "back_local_offset", "preferred_wall_yaw" in catalog. Use in _maybe_align_to_wall exactly like synth's back_anchor + wall_inner.
- **Robust overlap / adjacency:** Make place_with_catalog take/return shared bbs list for whole room (all calls + relations + extras share state). Integrate resolve earlier (before append). Add min-distance per catalog size + "adjacent ok for graves" grid mode. Post-pass verification function (probe-style) that flags facing errors / overlaps / center spam / corridor violations — fail sweep if bad.
- **Specialized placement modes:**
  - Wall-row placer (benches + graves) modeled 1:1 on synth bed_origin_at_wall + multiple along face.
  - Corridor-specific: populate narrow corridor cells with oriented roadblocks only (no other stems); use as blockers at mouths + interior.
  - "Ruins" mode for priesthood: constrained scatter (fallen yaw, clustered but gapped pillars against remnant walls, logical door placements as openings).
- **Diversity:** Populate factions/industrial/ etc from factory/ city_industrial/ space_kit/ per roster/kenney_kits_catalogue (scripted copy + probe refresh). Update _get + ROLE_SETUPS to prefer doc-mapped props (machinery for industrial, barrels/crates for priesthood ruins feel). Add purpose-driven variety (not just one stem).
- **Cross-faction / zone hygiene:** Strengthen filters in gen_freeform (walkable per exact prev/next vs synth dedicated). After placement pass, strip or re-kit any piece whose kit doesn't match zone faction. Audit in Proc.
- **Synth / stairs:** Audit _apply_synth_exterior_floors + _ensure_synth_accessibility for cases producing raised floors without stair connection. Make stair emission mandatory for any deck cell.
- **Verification & workflow:** Build / extend probe-based checker (e.g. generalize verify_synth_placement + add bench_facing_test, grave_row_test, corridor_blocker_test, no_cross_faction). Integrate into sweeps + dressing workflow. Always reproduce with editor cells=25 + G. Update synth-master-plan open issues with faction equivalents.
- **Process:** Before any placement edit, re-read synth-master-plan Critical Context + Automation Plan. After each code change: probe → sweep (short or full) → log review → verify render / editor test yourself. User feedback only after you confirm no regression on stated items (facing, overlaps, corridor use, rows, ruins logic).
- Run order for validation: `python tools/probe_faction_catalog.py` (if GLBs changed); sweeps; `python tools/gen_maps.py --seed 42 --probe`; editor Proc + G.

Keep bats user-simple (run + editor). Do not require user to run p/v.

---

## 6. Current state snapshot (as of this handover)

- Base (catalog, furnish skeleton, sweeps, bats, zone integration): working.
- Specific quality (correct bench facing + proximity, roadblock corridor usage, non-overlapping graves in rows, priesthood ruins logic, full doc diversity, zero cross-bleed, automatic detection without repeats): not yet at user expectation.
- Synth dedicated path still has isolated bugs (stairs, possible bleed).
- Props limited by what's actually in factions/ folders vs full roster/docs.

Next agent: treat this as the new "Critical Context" for faction interiors. Prioritize probe improvements for facing/adjacency + shared robust placer + corridor rules before more sweeps. Verify in editor yourself.

Links:
- Master automation rules: [docs/synth-master-plan.md](synth-master-plan.md)
- Faction prop mapping: [docs/faction_roster.md](faction_roster.md), [docs/kenney_kits_catalogue.md](kenney_kits_catalogue.md)
- This handover: the current doc.

Do not repeat the manual loop. Script everything.
