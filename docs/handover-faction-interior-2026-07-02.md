# Handover — faction interior scaling, middle-zone furnish, seam walls, pipe runs (2026-07-02)

**Branch:** freeform/ceiling-roofs.
Continues [handover-faction-interior-2026-06-29.md](handover-faction-interior-2026-06-29.md) / [-29b](handover-faction-interior-2026-06-29b.md).
All changes are Python-side (tools/ + catalogs); **no Rust rebuild required** — Proc regen in the editor picks everything up.

---

## 1. Prop scaling — ROOT CAUSE FOUND + fixed (data-driven)

**Why everything was tiny:** every Kenney kit used here is authored on a normalized
grid where **wall stems are exactly 1.0 unit tall** (graveyard `brick-wall`=1.0,
retro_urban `wall-a`=1.0, space_station `wall`=1.0, retro_fantasy `column`=1.0) and one
map cell is 4 m. So props need **×4 — the same factor the user-approved synth interior
already used**. The old code placed all non-synth props at scale=1 (the June-29 scale
recommendations were never applied). The **factory kit is the exception**: authored
chunkier (machine = 1.3 units), it takes **×2**.

Implementation:
- `tools/probe_faction_catalog.py`: `KIT_BASE_SCALE` (factory 2.0, retro_fantasy 4.0),
  `FACTION_BASE_SCALE` (necropolis/urban/priesthood 4.0, industrial 2.0), and
  `compute_auto_scale()` which clamps per stem by `MAX_PROP_HEIGHT_M=3.4` (room is 4 m)
  and `MAX_PROP_FOOTPRINT_M=3.6` (one cell). Each stem now carries **`auto_scale`** in
  `placement_catalog.json` / `prop_catalog.json`; catalog headers carry the base scale.
- `tools/faction_interior.py`: `stem_scale(stem)` resolves catalog `auto_scale`
  (fallback: computes the same clamp); `prop()` / `prop_piece()` default to it. All
  hardcoded `scale=1.0` in setups removed; **all offsets (wall-snap depth, grave row
  step/inset, candle/gravestone offsets, conveyor step) multiply by the actual scale**.
- Catalogs were regenerated for necropolis/priesthood/urban/industrial only —
  **synth's catalog was intentionally NOT touched** (probe_synth_catalog output,
  user-approved).
- Tuning knob: change `KIT_BASE_SCALE`/`FACTION_BASE_SCALE` and re-run
  `python tools/probe_faction_catalog.py` (mind the synth exclusion — see §7).
- Priesthood columns keep explicit 4.0 via `PRIESTHOOD_STEM_SCALES` (full
  floor-to-ceiling; the height clamp would have shaved them to 3.4).

Resulting scales (examples): graves 2.9 (footprint-clamped), gravestones ~3.7,
candles 4.0, barrels 4.0, factory 2.0, trees ~1.5 (height-clamped).

## 2. Industrial middle zone was NEVER furnished — fixed

`gen_freeform.to_doc` only furnished `zone=="prev"` / `"next"`. The middle
(`default`, almost always industrial) zone got nothing — hence "industrial empty as
middle faction". Now `_furnish_zone("default", comp.default_faction)` runs too, and
`"next"` is furnished even when `prev==next` (both zones are disjoint cell sets).

## 3. Ground-faction transition doors get real walls (seam closure)

New `gen_freeform._emit_zone_seam_walls` (called in `to_doc` before
`_strip_walls_under_doors`), mirroring the synth envelope rule for ground factions:
- Walls **every** prev↔default / default↔next seam face (both cells walkable) with the
  zone faction's own wall stem via `_zone_wall_piece` (respects manifest
  stem/scale/yaw/**inset** — necropolis brick inset 1.4 — and the brick Y-stretch,
  extracted into `_apply_necropolis_wall_height`).
- The transition door stays as the entrance; **BFS reachability repair** converts a
  seam wall back into an extra faction door wherever sealing isolated a region.
- **Door jamb pass**: flank faces along the door's own wall line get a faction wall
  (reachability-safe); where a jamb would cut the sole path, a **second faction door**
  extends the gate line instead (wide gated threshold, never a free-standing frame).
- GOTCHA solved on the way: wall-piece positions must be **snapped to the cell-face
  line** before keying (necropolis inset walls sit 1.4 m off the face) — see
  `wall_face_key` / `snap_face`. Any future face-keyed lookup must do the same.

**Gate:** `python tools/test_zone_seams.py 15` — 5 compositions × 15 seeds @cells=25:
reachability (walls block / doors pass / stairs bridge 1.2 m), zero open seam faces,
zero free-standing ground doors. ALL PASS as of this handover.

## 4. Industrial pipe RUNS (connected, wall-to-wall) + purpose rework

Probed factory pipe geometry (port rings at the bbox X faces, tube centre y=0.5,
diameter 1.0 unit; `pipe-large-long` = 2 units = **exactly one 4 m cell at scale 2** so
segments at cell-centre spacing connect port-to-port; `pipe-large-junction` = same
straight + vertical branch whose top = 2.056 units ≈ **exactly roof height at ×2**).

`setup_industrial_factory_floor` now places:
- ① **pipe trunk** along a closed wall, wall-to-wall, ends embedded in the
  perpendicular walls; ~70% of runs swap one mid segment for a junction (riser tees
  into the roof). If the floor route would block an opening, the same run goes at
  **roof level** (y = 4 − pipe Ø − 0.05) instead.
- ② machine row against another closed wall (facing room), ③ conveyor line
  end-to-end through open floor (off-centre so the middle stays walkable),
  ④ screens wall-snapped facing the room, ⑤ hopper/box corner cluster,
  ⑥ accent (robot-arm/scanner), ⑦ warning sign.
Corner bends (`pipe-large-bend`, asymmetric ports probed) are NOT used yet — straight
runs only. Future polish: L-runs via the bend piece.

## 5. Placement infrastructure (all factions)

- **`resolve_overlaps` was scattering composed pieces** — it nudged touching pipe/
  conveyor segments and grave rows apart (this exact bug corrupted the first pipe runs:
  4 m spacing became 4.3–5.3 m with lateral jitter). Now only pieces tagged **`loose`**
  (free scatter, via `_loose()`) may be nudged; composed pieces (runs, rows,
  wall-snapped) are immovable. `_RoomPlacer.add_fixed()` bypasses the room-nudge for
  exact placements; `try_add(..., nudge=False)` for row members.
- **Room context** (`CURRENT_ROOM_CTX`, set per room in `furnish_faction_interior`):
  - `open_sides`: room sides with any open face (corridor mouth / zone seam) —
    wall-snapping (`_maybe_align_to_wall`, `_closed_sides()`, `_snap_to_side`) never
    targets them (no more benches "against" a doorway).
  - `mouth_rects`: keep-clear BBoxes seeded into every `_RoomPlacer` over corridor
    mouths, **door bands, the spawn cell and the extraction trap** — props can no
    longer block corridors, openings, doors, spawn, or extraction.
- `_room_span` now returns the **inner usable area** (cell centres + half cell −0.3 m);
  previously furniture was confined to the cell-centre hull, wasting a 2 m band and
  making true wall placement impossible.
- Grave rows rebuilt: gravestone at the wall, slab in front pointing into the room,
  candle at the foot (~45%), tight scale-aware spacing, rows only on closed long walls,
  facing rows only when a walkway remains, mourning bench on a remaining closed wall.
- `furnish_faction_interior` new params: `global_walkable`, `door_positions`
  (gen_freeform passes `fm.walkable`, all door pieces + spawn + extraction).

## 6. Synth "sometimes a wall piece missing" — investigated, NOT a current defect

Balcony-aware enclosure sweep over **90 maps** (synth→synth 40 seeds; outlaw→synth,
synth→necropolis, priesthood→synth 25 seeds each, all @cells=25): **0 real holes**.
The old `_diag_enclose.py` reports 2 "holes" on seed 5 — those are **intentional
balcony ledges**: `apply_perimeter_balconies` deliberately drops the exterior wall and
places a ledge tile (+ outer rails) there. Balcony tile centre sits **3.4 m** outside
the cell centre — a balcony-aware check must skip faces with a `balcony_floor` piece at
that offset. If the user reproduces a genuinely missing wall, capture the seed +
composition; the sweep in this doc's git history (or `_diag_enclose.py` + balcony
filter) localizes it immediately.

## 7. Verify / repro commands

```bash
python tools/test_zone_seams.py 15          # NEW gate: seams + reachability + doors
python tools/test_transition_stairs.py      # must pass
python tools/test_synth_transition.py       # must pass
python tools/test_synth_interior_rules.py   # 50 seeds
python tools/verify_synth_placement.py userinput/synth_dressing/interior_showcase.json
python tools/_render_props.py <seed> <prev> <mid> <next> [out.png]   # NEW: top-down incl. prop footprints
python tools/gen_maps.py --seed 11 --prev-faction necropolis --next-faction outlaw --preview
```

Re-probing catalogs (after GLB or scale-knob changes) — **exclude synth**:
```python
import sys; sys.path.insert(0,'tools')
import probe_faction_catalog as pc
pc.FACTIONS = ['necropolis','priesthood','urban','industrial']
pc.main()
```

## 8. Open / next

- User playtest of the new scales (they are one-knob tunable, §1).
- Pipe L-runs via `pipe-large-bend`; more ceiling runs (currently fallback-only).
- Priesthood room diversity still modest (kit folder is arch-heavy).
- Outlaw zone often sparse on small prev fractions — counts scale with room count.
- Editor G-playtest confirmation of collider behaviour for scaled-up props.
