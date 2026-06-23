# Handover — synth elevated-transition + dressing sandbox

**Last updated: 2026-06-24.** For the next agent. The user playtests synth maps in the editor and authors dressing vignettes; we fix **generation logic** and keep audits green. They are time-pressed — reproduce before claiming fixes, and don't hand things back "for re-test" when you can verify yourself.

**Start here for interior/balcony/mezz work:** [synth-master-plan.md](synth-master-plan.md). **Transition seam only:** [synth-transition-architecture.md](synth-transition-architecture.md).

---

## 0. READ FIRST — two things that will bite you

1. **The editor generates at `--cells 25`, NOT 40.** `gen_maps.py` default is 25; the editor "Proc regen" writes `userinput/maps/_editor_preview.json` at 25. Earlier "verified" sweeps ran at 40 and tested *different maps* than the user plays. **Always** reproduce/verify with `gen_freeform.generate_map(seed, cells=25, composition=comp)`. See [[reference_procgen_cells_default]] in memory.
2. **Single source of truth:** `docs/synth-transition-architecture.md`. It has §0a **OPEN DEFECTS table (D1–D11)** — keep it current; check rows off only when fixed AND verified. The user references it instead of re-explaining.

Composition for all repro:
```python
import gen_freeform as gf, level_composition as lc
comp = lc.LevelComposition(mix_mode="transition", prev_faction="synth",
                           next_faction="synth", default_faction="industrial_default")
```

---

## 1. Current defect status (from the doc's D-table)

| ID | What | Status |
|----|------|--------|
| D1 | Door spam (10+ at one spot) | FIXED — removed `emit_building_face_doors` |
| D2 | Door lodged in wall | FIXED — `_strip_walls_under_doors` post-pass |
| D3 | Inaccessible synth areas | FIXED — `_ensure_synth_accessibility` BFS repair |
| D4 | Transition without opening | FIXED — same repair + D7 |
| D5 | **Falling through world at spawn** | **FIX APPLIED, NEEDS USER REBUILD** (Rust) — see §3 |
| D6 | structure-barriers / holes | FIXED — disabled multi-floor (`_MULTI_FLOOR_ENABLED=False`) |
| D7 | **zone mislabel (the big one)** | FIXED — `zone = zone_lookup(faction_cell)` (was `"prev" if faction_id==prev_faction`, always "prev" when prev==next=="synth") |
| D8 | "Raised line / pixel too high" | FIXED — deck `y=0` vs synth blocks no-`y` (→ −0.005) = 5 mm step; pinned `y=0` in `_apply_synth_exterior_floors` |
| D9 | Floating / 2-tiles-back doors | FIXED — removed 2-tile setback, fixed door edge, added reachability-safe jambs |
| D10 | Corner stairs swapped | FIXED — swapped corner family in `_VARIANTS_BY_SIGN` |
| D11 | "Missing opening between factions" | NOT A BUG — user chose **one entrance per zone**; perimeters stay walled |

**Confirmed by user as fixed:** the raised line (D8). **User still saw:** spawn fall (D5) — but they had NOT rebuilt; the fix is Rust-side.

---

## 2. The single most important next action

**Ask whether they rebuilt (`host.bat`) before re-testing D5.** The spawn-fall fix is in `crates/shared/src/kenney_layout.rs` (`walkable_surface_y` / `infer_spawn_floor_y`) and only takes effect after a rebuild. Proc-regen alone won't include it.

If spawn STILL falls after a rebuild → the remaining suspect is the **collider path** for the elevated synth `floor` block (1.2 m). Colliders are trimeshes (`test_showcase.rs::world_trimesh`) so the top *should* be at 1.2 m; verify the server builds a collider for synth floor blocks at spawn time and that `play_spawn_y(editor_active)` returns 1.2 (saved JSON has `spawn_y=1.2`; the editor's in-memory regen path is the risk). Don't claim D5 fixed without the user confirming in-game.

---

## 3. Architecture in one breath

Synth is an **elevated** faction: industrial at y=0, the whole synth zone is ONE building on a **1.2 m platform**. Every synth cell = a solid `floor` block (GLB is 0.3 unit × scale 4 = 1.2 m tall) at y=0, top 1.2 m. Every synth wall sits at y=1.2 m. You enter via **stairs** (on the industrial substrate, bridging 0→1.2) + a **door** at the entrance. The synth footprint is fully walled (the "house keeps heat" rule); openings are only stairs/doors.

Pipeline in `gen_freeform.to_doc` (order matters):
`emit_pieces` (base floors+walls) → `te.emit_transition_pieces` (→ `st.emit_synth_boundary`: stairs, deck, main door, transition/integrity walls) → full-zone `interior_cells` → `_apply_synth_exterior_floors` (all synth floors → blocks, **y=0**) → `st.emit_synth_envelope_walls` (wall every synth↔walkable-non-synth seam; corridor crossings → door+stairs) → `_ensure_synth_accessibility` (BFS repair: open a stair+door into any sealed region) → `_attach_walls_to_doors` (reachability-safe door jambs) → `_strip_walls_under_doors` → roofs → `_apply_zone_elevation` (raise floor-0 non-block pieces; `_face_cells` max; `floor_level==0` guard).

Key files: `tools/synth_transition.py`, `tools/synth_deck.py`, `tools/transition_entrances.py`, `tools/gen_freeform.py` (orchestration + the post-passes above), `tools/level_composition.py` (zones, `make_elevation_lookup`), `tools/mirror_glb.py` + `repair_synth_glb.py` (chiral stair GLBs).

---

## 4. Verify commands (ALL at cells=25)

```bash
python tools/test_transition_stairs.py        # must pass
python tools/test_synth_transition.py         # must pass
python tools/_diag_synth.py                   # door stacks / doors-in-wall / barriers / spawn
python tools/_diag_access.py                  # reachability from spawn (note: was edited; the inline sweep below is canonical)
python tools/_diag_enclose.py                 # synth enclosure holes (should be 0)
python tools/_diag_baddoor.py                 # doors with open floor on BOTH flanks (should be ~0; 1 = corridor door, OK)
python tools/_diag_floors.py                  # synth floor-top uniformity (all 1.2)
python tools/_render_map.py <seed>            # TOP-DOWN PNG — use this to SEE the map (matplotlib)
python tools/_render_zoom.py <seed> x0 x1 z0 z1   # zoom with per-tile height numbers
```

**Lesson learned: when text diagnostics say "fine" but the user sees a defect, RENDER it** (`_render_map.py` / `_render_zoom.py`). That's how D8 (the 5 mm seam) was found — text checks all passed, the height numbers in the render exposed it. matplotlib is installed.

Canonical regression sweep (paste-run): BFS reachability honoring walls/doors/stairs+elevation must give `unreached=0`; also expect `door_stacks=0, doors_in_wall=0, barriers=0, nonflush=0` across seeds 1–39 @25. (The exact script is reproduced several times in the transcript; `_diag_baddoor.py` + `_diag_enclose.py` cover most of it.)

---

## 5. Gotchas / non-obvious facts

- **Chirality of end stairs is geometric** (`_oriented_end_stem`): pick the variant whose rail faces OUTWARD along the lateral axis, keyed on the **active span** (first/last non-None stem), using world yaw (`cos`/`-sin`). EDGE family follows rail-sign; **CORNER family is swapped** (its x-asymmetry doesn't track the visible corner) — `_VARIANTS_BY_SIGN`.
- **Mirrored stair GLBs must NOT flip UVs** — Kenney `colormap.png` is a colour atlas; flipping U samples the wrong swatch (white/odd stair). `mirror_glb.py` keeps UVs; re-run `repair_synth_glb.py` after. See [[reference_kenney_atlas_mirror]].
- `te._toward_substrate_side(fac, sub)` actually returns the toward-**faction** direction (misnamed). The synth door uses `OPPOSITE[toward]` to face the stairs.
- `default_placement_y(floor) = floor*MOD_H - 0.005`. Any floor-0 piece with NO `y` renders at −0.005 — that 5 mm bit us (D8). Synth blocks now pin `y=0`.
- Multi-floor (3.6 m / `structure-barrier`) is **disabled** (`_MULTI_FLOOR_ENABLED=False` in `synth_transition._plan_flights`) — it produced unsupported barriers + floor holes. Re-enable only behind a real second-floor impl.
- NEVER `git checkout`/`restore` — repo is mostly uncommitted (see top-level memory CRITICAL).

---

## 6. Open / next

- **D5 spawn fall** — confirm after user rebuild; chase collider path if it persists (§2).
- **Dressing sandbox — active (2026-06-24):** see §7 below. Balconies, mezzanine, room-first furnish, floor scratch texture, and playtest fixes are **done in dressing**; **not yet wired into live procgen**.
- **Interior decor on procgen maps:** first `furnish_synth_interior` scatter pass is structurally safe but visually wrong — replace with vignette-derived setups from `tools/synth_interior.py` once signed off.
- Possible polish (not user-blocking): wide-stair dead deck cells (seed 1); short stairs for compact mezzanine runs.
- Throwaway local outputs: `tools/_*.png`, `tools/_*.log` (gitignored). Keep referenced diagnostics: `tools/_diag_*.py`, `tools/_render_*.py`, `tools/audit_synth_scene.py`, `tools/verify_synth_placement.py`.

---

## 7. Dressing sandbox (2026-06-24)

### Launch

```bash
dressing.bat          # rebuild + --dressing shell (NOT editor.bat)
```

Window title shows `EDITOR_BUILD_TAG` (currently `2026-06-24b`) — if stale, rebuild failed or exe was locked.

### Key files

| Area | Path |
|------|------|
| Generator + geometry source of truth | `tools/synth_interior.py` (`expected_balcony_floors`, `expected_balcony_rails`, `mezzanine_plan`, `furnish_showcase_plan`) |
| Red-flag audit | `tools/audit_synth_scene.py` → `tools/verify_synth_placement.py` |
| Probed placement rules | `assets/models/factions/synth/placement_catalog.json` |
| Flat stem inventory | `assets/models/factions/synth/catalogue.md` |
| Saves | `userinput/synth_dressing/*.json` |
| Floor scratch texture | `assets/models/factions/synth/Textures/floor_detail.png` (`python tools/gen_floor_detail.py`) |
| Client materials | `crates/client/src/test_showcase.rs` (`SynthFloor`, `SynthProp`, `SynthDeck`) |
| Playtest sync (dressing must skip Kenney patch) | `crates/client/src/editor_playtest.rs` — early return when `EditorWorkflow::SynthDressing` |

### Regenerate + verify

```bash
python tools/gen_dressing_showcase.py
python tools/gen_balcony_test.py
python tools/gen_dressing_rating_map.py
python tools/verify_synth_placement.py userinput/synth_dressing/interior_showcase.json
```

### Balcony gotchas (learned the hard way)

- **Floor tiles are directional:** `balcony-floor-center` has a raised lip on **+z**; corner tile lip wraps **−x/+z**. Yaw must point lip **outward** (`FLOOR_EDGE_YAW`, `CORNER_FLOOR_YAW`) — using one yaw for n+s was the "correct in 2 dirs, wrong in 2" bug.
- **Rails:** trace the **outer boundary of the union of ledge tiles** (`expected_balcony_rails`), not one 4 m rail per exterior face — otherwise inner (concave) corners get perpendicular rails that cross. Audit: `check_rail_crossings`.
- **Visual test map:** `balcony_test.json` — reference row + 4-yaw rows + L-shaped room with concave corner.

### Mezzanine / playtest gotchas

- Elevated `floor` + `stairs` need `SynthProp` / base synth routing and positive `depth_bias` on props — otherwise z-fight in editor.
- **G playtest** used to collapse dressing piece Y via Kenney map sync — gated off for dressing workflow; if stairs/deck vanish in playtest again, check `sync_playtest_patched_pieces` / `sync_playtest_mesh_cutouts`.
- Chained stair flights get `mezz_stair_fill` floor blocks underneath in `add_command_mezzanine`.

### What's left

- Wire `furnish_showcase` logic into `tools/synth_transition.py` `furnish_synth_interior` (procgen maps still use old scatter).
- Optional: short stairs for mezzanine; balcony rules on live procgen footprint (currently dressing-only).
