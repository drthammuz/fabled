# Handover — playtest feedback fixes (2026-07-03)

**Branch:** freeform/ceiling-roofs. Continues [handover-faction-interior-2026-07-02.md](handover-faction-interior-2026-07-02.md).
Fixes for the user's editor/playtest feedback round (plan.txt). Mostly Python; TWO Rust files changed
(server/level.rs, client/test_showcase.rs) → **needs a rebuild** (exe was locked by the running game;
libs compile clean, `host.bat` relink pending).

**Repro note:** the editor's real defaults are fractions **0.14 / 0.55 / 0.32**
(MapGenSettings 15/60/35 normalized), NOT gen_maps' CLI defaults 0.25/0.5/0.25. Repro scripts must
pass these or you test different maps than the user plays.

## 1. Zone painting is now CONTIGUOUS (root cause of all "synth pieces in wrong zone")

`level_composition._demote_disconnected_zone_pockets` (called from `assign_zones_for_map`):
nearest-spine painting stranded pockets of prev/next deep inside the default zone. A lone synth
cell then became a bizarre mini-building (envelope walls at 1.2 m, stairs, elevated door — the
user's "levitating wall", "synth door on the floor", "balcony without floor in outlaw area",
"synth floor in necro room"). Keep the component containing the zone's spine anchor (spawn end
for prev, extraction end for next; largest as fallback), demote the rest to default.

## 2. Synth pipeline no longer assumes prev AND next are synth

`synth_transition.synth_zone_ids(comp)` returns the zone ids whose faction IS synth; threaded
through `emit_synth_envelope_walls`, `furnish_synth_interior`, `decorate_synth_walls`
(new `synth_zones` param, defaults to the old `{"prev","next"}`), and
`gen_freeform._apply_synth_exterior_floors` / `_ensure_synth_accessibility` / `to_doc`.
Before: on synth→outlaw maps the outlaw zone got synth envelope walls, exterior floor blocks,
balconies and interior props ("zone=next" synth pieces in outlaw territory).

## 3. Props snap to the wall SURFACE, not the cell-face line

Probe writes `wall_inner_offset_m` into each placement_catalog header
(`probe_faction_catalog.wall_inner_offset_m()`): how far the manifest wall stem's inner
**panel** surface protrudes into the room past the cell face (panel = z-slab whose verts cover
≥70% of the wall width — priesthood buttresses that stick 2 units in don't count).
necropolis 0.6 (brick slab 1.2 m thick centred on the face), priesthood 0.6, urban/industrial 0.
`faction_interior` reads it as `CURRENT_WALL_OFFSET`; `_wall_face()` shifts by it, and
`_maybe_align_to_wall` now uses `_wall_face`. Fixes gravestones (and everything wall-snapped)
half-buried in necropolis brick walls — verified: 15/15 stones clear.

## 4. Probed per-stem `front` axis — no more backwards screens/benches/dumpsters

`probe_stem` infers `front`: upper-half vertex-mass z-bias (backrests/mounts/back panels are the
BACK) with a topmost-band fallback (open dumpster lid sits at the hinge side). Flipped stems:
factory `screen-*`/`screen-panel-*` (mounts at +z), urban `detail-bench`,
`detail-dumpster-open/closed`. `faction_interior._facing_yaw(side, item)` =
`_INTO_ROOM_YAW + π if front == "-z"`; used by `_snap_to_side`, `_wall_yaw_and_offset`,
necro gravestone/bench/chapel-bench placements.

## 5. Corridors never dead-end into a zone seam wall

In `gen_freeform._emit_zone_seam_walls`: when the straight path continues through a corridor
cell on either side of a seam face, emit the zone faction's DOOR (tag `corridor_seam_door`)
instead of a wall. (necro→priesthood seed 8 at (-28,-10): industrial corridor now gets a
priesthood gate-door.)

## 6. Pipe runs must END in solid wall

`_emit_pipe_run`: roof-run END segments are rejected if they touch a keep-clear rect (doorway /
corridor mouth); floor runs were already checked via placer bbs. `setup_industrial_factory_floor`
now tries each closed side until a run fits. Swept 24 maps / 96 runs: 0 ends near a door.

## 7. Mezz upper wall tier stacked FLUSH (no overlapping double walls)

`synth_interior._mezz_upper_walls`: y = `DECK_Y + 4.0` (= 5.2, top of the base wall tier) instead
of `deck_top` (2.4) which coplanar-overlapped the base wall by 2.8 m (z-fighting). User rule:
double walls never overlap; use full height per tier.

## 8. Ceilings/roofs now COLLIDE (Rust — server/level.rs)

`kenney_skip_piece_collider` no longer skips ceiling slabs. On raised floors (synth deck 1.2,
mezz 2.4) a jump crossed the visual-only roof plane. Trap drops are unaffected (roof is one level
UP; landing cells still skip roof emission at gen time).

## 9. Synth floor = marble (Rust — client/test_showcase.rs + new texture)

`assets/models/factions/synth/Textures/floor_marble.png` — procedurally generated tileable veined
marble (numpy sin-band + periodic turbulence; regenerate with the snippet in git history of this
doc / session). `SynthFloor` material: marble texture, base_color 0.78/0.78/0.80 (was near-white
0.86), roughness 0.35, uv_transform (8,5) ≈ 1 repeat per 4 m tile.

## Verification (all green)

```
python tools/test_zone_seams.py 15         # ALL PASS (5 compositions)
python tools/test_transition_stairs.py
python tools/test_synth_transition.py
python tools/test_synth_interior_rules.py  # 50 seeds
python tools/verify_synth_placement.py userinput/synth_dressing/interior_showcase.json
cargo build -p server -p client            # clean (full link pending: exe locked by running game)
```

Catalogs re-probed (synth excluded as always: `pc.FACTIONS = ['necropolis','priesthood','urban','industrial']`).

## Open

- User visual pass on: marble floor look, mezz "tower" silhouette (upper tier now tops at 9.2 m),
  screen/bench/dumpster facings, corridor seam doors.
- The seed-1 outlaw "wall on door" coords ((-22,-8), (-30,36), (-21,-31)) reproduce as properly
  paired two-sided seam walls with no door overlap in current code — believed fixed by the
  previous round (backside copies + hardened `_strip_walls_under_doors`).
