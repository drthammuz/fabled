# Handover — faction interior + synth fixes (2026-06-29 evening)

**Branch:** freeform/ceiling-roofs  
**Context:** Faction interior placement was just rewritten (probe_faction_catalog.py + faction_interior.py).
All 5 factions now place real props at correct scale and yaw. Visual test done in editor tonight.

Read first: [docs/handover-faction-interior-2026-06-29.md](handover-faction-interior-2026-06-29.md) (morning session context).

---

## User feedback from editor playtest (evening)

### 0 — Already done
Save this document before touching any code. ✓

---

### 1 — Brick pillars in synth rooms / missing stairs at synth transitions

**Brick pillars in synth:**  
Brick-wall2 (necropolis kit, `factions/necropolis`) is appearing inside synth rooms.
Cross-faction bleed — necropolis placement is leaking into synth zone cells.
User note: *"their placement is actually really good.. but shouldn't brick be in another faction?"* — the placement logic is sound, just wrong zone.
Fix: tighten zone filter in `gen_freeform.py` so `furnish_faction_interior` only receives cells
whose `zone_lookup(c)` strictly matches the current faction's zone tag ("prev"/"next").
Also audit synth's own `synth_interior.py` / `furnish_synth_interior` path — verify it
doesn't accidentally include cells from the adjacent faction.

**Missing stairs at synth transitions (synth is last faction, industrial is mid):**  
When synth is `next_faction` and industrial_default is between, the raised-floor stair
emission fails to produce stairs at the transition boundary.
This is likely the open defect O9/O11 from docs/synth-transition-architecture.md:
stair emission guard that skips cells without an adjacent lower deck neighbour.
Fix: ensure stair emission fires unconditionally for any raised synth deck cell that
borders a non-deck (ground-level) cell, regardless of faction order.

---

### 2 — Chair offset in front of synth screen

One chair is placed slightly to the side rather than centred in front of its screen.
Location: `synth_interior.py`, the `_place_computer_cluster` or equivalent function.
The chair offset is likely a `+x` jitter or half-width calculation that doesn't account
for the screen's actual centre x after wall-snap.
Fix: derive chair x from the screen's final `x` value (post wall-snap), not from a
separate random offset.

---

### 3 — Wrong doors at faction transitions

At prev↔next transition thresholds, the door GLB used is the industrial door
regardless of which factions are on either side.
Correct behaviour: use the door stem from the **first faction** (prev) for the prev-side
threshold and the **last faction** (next) for the next-side threshold.
Each faction's door stem is declared in its `faction.json` or `placement_catalog.json`
(e.g. necropolis → gate-door from dungeon kit; synth → gate-door from space_station kit).
Fix: look up door stem + kit from the faction profile rather than hard-coding industrial.

---

### 4 — Prop scale too small for ground factions

Current: all non-synth, non-column props use `scale=1.0`.
User verdict after playtest:
- **Industrial**: scale too small → try `scale=2.0`
- **Outlaw / Urban**: scale too small → try `scale=3.0`
- **Necropolis**: scale too small → try `scale=3.0`
- **Priesthood** (non-column): scale too small → try `scale=3.0`
  - Columns/pillars: keep at `scale=4.0` (user: *"pillars don't need enlargening"*)

Implementation:
- `faction_interior.py`: update `PROP_SCALE_DEFAULT` dict and/or `FACTION_SCALES` for each faction.
- `PRIESTHOOD_STEM_SCALES`: keep column/column-damaged/column-paint/column-wood/structure-pole at 4.0; reduce other structural to 3.0 or 2.0 as appropriate.
- After changing scales, re-verify overlap checker — bigger props need bigger gap or the
  `_RoomPlacer` will reject too many placements. May need to reduce counts slightly.
- Necropolis grave rows: at scale=3, `g_hx` and `g_hz` scale up proportionally.
  The `place_row` function derives step from `g_hx * scale`, so it should auto-adjust
  IF you pass the scale into the bounds calculation. Currently `g_hx`/`g_hz` are read
  from catalog at `bounds_scale1` — multiply by the new scale when computing step/inset.

Exact scale values are the user's best guess; tweak after seeing in editor.

---

### 5 — Two synth raised-floor bugs

**5a — Overlapping walls on raised-floor rooms (double walls):**  
When a synth room has a raised floor, the code adds extra wall segments around it to
close the gap between ground level and deck level. These extra walls are being placed at
the same x/z as the existing base walls → visual z-fighting / dark seam all around the room.
Fix: track which wall cells already have a base wall, and for raised-floor rooms do NOT
emit a second base wall. Instead, raise the *roof* of that room to `deck_y + cell_height`
so the existing walls are tall enough without duplication. In practice:
  - Detect raised-floor room cells (those in `deck_cells`).
  - For those rooms, skip the extra-wall emit.
  - Raise the roof pieces (or wall height) to span from ground to `deck_y + standard_height`.

**5b — Gap between short stairs and raised floor:**  
When a single short-stair piece is placed adjacent to a raised floor, there is a ~2m gap
between the top of the stair and the deck surface.
User also notes: a new `floor-half` GLB (2×4 unit) exists in `assets/models/space_station/`
that can be placed underneath a second short-stair to avoid the second stair levitating
when two stairs are stacked.
Fix:
  - Confirm stair placement x/z snaps flush to the deck edge cell (no cell-width offset).
  - For double-height transitions, use two short stairs with `floor-half` under the lower
    one (or one `stairs-small` + one `stairs-small-corner-inner` if geometry allows).
  - The `floor-half` stem is `floor-half` in the space_station kit — verify GLB exists
    at `assets/models/space_station/floor/floor-half.glb` (user said "floor-half i think").

---

## Priority order for next session

1. **Save this doc** (done) — user reviews tonight before starting
2. **Fix cross-faction bleed** (brick-wall2 in synth) — surgical zone filter
3. **Synth raised-floor bugs** (5a double walls, 5b stair gap) — existing open defects
4. **Scale adjustment** (#4) — single dict change, fast to test
5. **Missing synth stairs** at transition (#1b) — may be related to #5b
6. **Chair offset** (#2) — small tweak
7. **Faction-correct doors** (#3) — requires faction profile lookup

---

## Key files for next session

- `tools/faction_interior.py` — PROP_SCALE_DEFAULT, PRIESTHOOD_STEM_SCALES, setup_* functions
- `tools/gen_freeform.py` — zone filter for furnish calls; door stem lookup; stair emission
- `tools/synth_interior.py` — chair placement; raised-floor wall emit; stair snap
- `tools/synth_transition.py` — stair emission at transition boundary
- `assets/models/space_station/floor/` — verify floor-half.glb exists
- `docs/synth-transition-architecture.md` — OPEN DEFECTS table (O9, O11)
