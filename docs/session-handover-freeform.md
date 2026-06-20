# Session handover — procgen free-form pivot (2026-06-20)

## New generator: `tools/gen_freeform.py`
Rooms + corridors on a flat tile grid. **No modules, no `room-*` GLBs.** Tiles only:
`template-floor`, `template-wall`, `corridor-*`, `template-floor-hole`.
- 25×25 cell grid, ~11 variable rooms (3–7), MST + 3 loop corridors.
- Corridors connect rooms' nearest boundary cells → clean 1-cell doorways.
- Spawn/extraction = farthest room pair. ~0.01s/map. `validate()` = flood-fill reachability.

## Hub (new model — old stairs/gate/west logic abandoned)
Floor-0 extraction = trap door → 7×7 hub room (floor −1) → **2 exits** (each a
direct `trap` or `doorway`→corridor→trap) → 3×3 landing rooms (floor −2) = the two
next-level starts. Player commits to one, no way up. `build_hub()`; `hub_exits` tags kind.
Next levels are landing stubs for now.

## Wiring
`gen_maps.generate_map_report` → delegates to `gen_freeform.run()`, so editor **Proc tab**
(`gen_maps.py --preview`) and CLI both produce free-form. No Rust rebuild needed.
Loader (`editor_map.rs load_generated`) is dimension-agnostic.

## gen_maps.py teardown
Removed all module/pool logic: `--use-pool`/`--pool-batch`, `load_pool`, `compatible_variants`,
`generate_pool_batch`, etc. **Still orphaned/dead (TODO delete):** `design_high_level`,
`PlacementState`, `synthesize_*`, `build_map_json`, `apply_extraction_and_hub`/`apply_hub_*`,
`validate_map`/`audit_map`. `gen_modules.py` left intact (separate tool).

## Notes
- Old probes (`probe_map_geometry`, `probe_hub_tile_audit`) no longer apply.
- Tunables in `gen_freeform`: `generate_map`/`run`/argparse defaults must match (editor uses `run`).
- Open: delete dead slot code; turn floor-−2 stubs into real next-levels.

## Follow-up fixes (2026-06-20)
1. **Roofs:** `emit_roofs()` runs **last** after all functional floors. Adds normal `template-floor` one level up only when `_has_floor_at()` finds no floor-surface piece at that centre (0 px). Skips trap holes / open verticals. `ceiling: true` keeps playtest from hiding slabs over mask void. Faction roof height TBD.
2. **Extra hub holes / west stairs:** Legacy `patch_hub_branch_layout` injected L2/L3/L4 mask holes and west `stairs`. Freeform maps set `hub_model: "freeform_v1"`; patch is a no-op when `is_freeform_hub_layout()`.
3. **Disjoint hub landings:** `build_hub()` rejects exit pairs whose floor-−2 landing rects overlap; generator retries hub placement (up to 32 attempts per seed).

Full detail: memory `project_procgen_phase1.md`.
