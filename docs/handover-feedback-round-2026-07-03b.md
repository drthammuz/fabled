# Handover — feedback round 2 (2026-07-03, evening)

**Branch:** freeform/ceiling-roofs. Continues [handover-playtest-fixes-2026-07-03.md](handover-playtest-fixes-2026-07-03.md).
Python + Rust both changed; **full `cargo build` succeeded** (fabled.exe relinked, includes the
morning round's ceiling colliders + marble floor too).

## 1. Synth mezz stair gap (Python — synth_interior.py)

The short stair GLB is ~1.2 m deep, not a full cell; it was placed at the cell CENTRE leaving a
~1.6 m gap to the mezz deck. Now aligned like the zone-seam stairs: high edge flush with the
deck-cell boundary (`stair_ramp_footprint_m` + `STAIRS_SEAM_OVERLAP_M`). Verified: high edge at
1.998 m from deck centre (edge = 2.0).

## 2. Mezz rail inset (Python — synth_interior.py)

The mezz deck's open-edge rail was centred ON the tile edge (half its 0.4 m thickness overhanging).
Now inset by its probed half-thickness — sits fully on the deck tile. (Balcony rails already did this.)

## 3. Faction-correct door jambs (Python — gen_freeform.py)

`_attach_walls_to_doors` hardcoded `factions/synth` walls for EVERY door jamb — the user's
"synth walls at an industrial→priesthood transition" at (28,-9) seed 1 synth-industrial-priesthood.
Jambs now use the door's zone faction wall via the hoisted `_zone_wall_pieces_two_sided`
(urban gets its mirrored backside copy); synth wall remains only for synth zones / unzoned doors.
Verified: that jamb is now `factions/priesthood/template-wall`; 0 synth pieces outside synth zones.

## 4. Crouch stuck semi-crouched (Rust — server/character.rs)

`try_stand_up` cast the STANDING collider up from the CROUCHED centre — embedded in the floor, the
cast always hit ⇒ never stood while grounded (a jump freed it: the observed "jump to uncrouch").
Now sweeps the CROUCHED box up by `2*CROUCH_Y_SHIFT` (exactly the extra headroom standing needs),
excluding Liquid sensors like the ground traces.

## 5. Dropped jump inputs (Rust — server/character.rs)

Client sends `jump: just_pressed` every RENDER frame; `apply_player_inputs` overwrote LatestInput
per message, so the next frame's `false` erased a press before the 30 Hz tick read it. All one-frame
edges (jump, attack, shop_buy, route_select) are now OR-latched across messages within a tick.
NOT an air-jump buffer: `player_movement` consumes `jump` unconditionally every tick.
`player_attacks` consumes the press even when dead (no latched attack firing on respawn).

## 6. Enemy physics + navigation (the big one)

### Baked nav grid (Python — gen_freeform.build_nav_grid)
Every proc map doc now carries `"nav"`: walkable 4 m cells with floor Y (zone elevation) and
passable faces ("open": subset of NSEW) — derived from the SAME rules as player reachability
(walls block incl. inset necro walls via face-snap, doors/hidden doors pass, 1.2 m steps only via
stair cells). Agent spawns (`enemy_spawns`/`npc_spawns`) now get their cell's elevation + 0.15
(no more spawning inside the synth deck).

### Mass simulation gate (tools/test_nav_grid.py)
Per map: nav symmetry, edge re-derivation, BFS from spawn (0 unreachable), 500 random A* pairs with
path-validity check, spawn-height check. 24 maps × 4 compositions ALL PASS (~12k A* runs). Run with
a bigger arg for more seeds — it's pure CPU.

### Rust plumbing
`shared::kenney_layout::{NavCell, NavGrid}` (serde) → carried through MapDocument /
KenneyLayout / map_pool. `server/nav.rs`: `EnemyNav` resource (HashMap grid, A* with unit tests,
random-nearby-cell BFS for wandering). Built in `reload_kenney_playtest` from the layout
(log line: `enemy nav: N cells`). **Maps exported before this round have no nav** — regen from the
editor Proc panel to bake it; enemies fall back to local wander otherwise.

### Enemy behaviour (server/combat.rs rewrite)
- `enemy_navigate`: chase = A* to the player's cell (repath ≤ 0.7 s / on cell change), straight
  steering when close + same level; wander = random reachable cell ≤ 4 steps via BFS, pathed.
- `enemy_locomotion`: gravity (−20 m/s²) + downward ground probe (snap ≤ 0.6 m — walks ramps/stairs),
  chest-height blocked probe with axis-slide fallback, smoothed yaw facing motion.
  Enemies NO LONGER inherit the player's Y when chasing (that was the "flying" bug: chase target
  was the player's 3D position and movement lerped straight at it, no collision).
- NPC brain gets its own half-height (`EnemyBrain::sized`).

### Client (prop_render.rs rewrite)
Agents now switch **Idle/Walk/Run** by measured horizontal speed (smoothed, thresholds 0.4/3.2 m/s)
— previously only Idle ever played, which is why they "animated in an idle pose even when walking".
Model packs (Quaternius) each carry 17 clips incl. Punch/Death/RecieveHit for later. Bind pose
faces +Z (probed) — matches the server yaw convention, no flip needed.

## 7. Priesthood prop scale (probe + catalogs)

`KIT_BASE_SCALE["retro_fantasy"]` 4.0 → 4.8 (+20%): barrels/crates/bricks grow; columns, ladders,
pulleys are UNCHANGED (already capped by the 3.4 m height clamp — matches "pillars are fine").
Re-probed priesthood catalogs; `faction_interior.PROP_SCALE_DEFAULT` matched.

## 8. Industrial dressing upgrade (probe + faction_interior.py)

- Catalog additions: glass pipe family (`pipe-glass-large-*`), `pipe-large-valve` (+glass valve),
  conveyor part-end/middle stems, `catwalk-stairs`, and **cross-kit extras**: space-station
  `skip`, `skip-rocks`, `rocks` probed into the industrial prop catalog with per-stem `prop_kit`
  (probe `EXTRA_PROP_STEMS`; `prop_piece` honours per-stem kit).
- Pipe runs pick a material family per run (40% glass) and can take an inline **valve** slot
  (valve is 1 kit-unit — paired with a 1-unit plain pipe so the 2-unit slot stays gap-free).
- Conveyor lines are **machine-capped at both ends** (feeder/receiver facing the belt via probed
  `front`); a blocked end gives up one belt segment and retries. Verified seed 1: 4 lines,
  3 fully capped, 1 half (end abuts an opening).
- Storage corners can now place skips/rock piles.

### Deferred (next industrial session)
- Animated pistons + proximity thud sound — needs Rust: play GLB animations on placed kenney
  pieces + a piece-anchored audio emitter.
- Bigger industrial rooms with catwalk LEVELS (multi-tier, full-height double walls per tier) —
  needs a per-zone room-size knob in gen_maps + a catwalk planner.
- Machine-to-machine pipe networks (kit has `machine-connection-pipe/-hole` for this).

## Enemy AI — current state (user asked)

Wander near spawn + **hearing-only** aggro (8 m radius, 14 m if sprinting — through walls!), chase
while alert (decays), contact damage < 1.2 m. No patrol routes, no vision cone, no LOS check.
Sensible next steps: LOS raycast gate on aggro, patrol waypoints over the nav grid, a debug vision
cone (translucent arc mesh, dev-flag gated). Models have Punch/Death clips ready for combat anims.

## Verification (all green)

```
python tools/test_zone_seams.py 15        # ALL PASS
python tools/test_transition_stairs.py
python tools/test_synth_transition.py
python tools/test_synth_interior_rules.py # 50 seeds
python tools/verify_synth_placement.py userinput/synth_dressing/interior_showcase.json
python tools/test_nav_grid.py 6           # ALL PASS (24 maps, ~12k A* runs)
cargo test -p server nav                  # 3/3
cargo build                               # full link OK
```
