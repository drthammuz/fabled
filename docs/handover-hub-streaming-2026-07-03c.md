# Handover — hub streaming + follow-ups (2026-07-03, round 3)

**Branch:** freeform/ceiling-roofs. Continues handover-feedback-round-2026-07-03b.md.
Full `cargo build` OK. Backlog now lives in **docs/TODO.md** (persistent, prioritized,
with [S]=delegable-to-Sonnet/Opus flags — user request).

## 1. Runtime hub map streaming (the headline)

"Infinite grid, no loading screens": while playing the active map, TWO child maps
generate in background threads (`tools/gen_maps.py`, same knobs as the editor Proc
panel) and mount under the hub's two drop exits. Dropping through an exit commits:
the chosen child becomes the new active map, parent + unchosen sibling despawn, two
new children start generating. Only 3 maps live at once.

**Faction chain:** child prev-faction = parent's next-faction; child next-faction =
random from `CHAIN_FACTIONS` (outlaw/priesthood/synth/necropolis). The editor's
MapGenSettings prev/next are updated on commit so the panel stays truthful.

### Architecture (v1 = editor playtest G, solo host)
- `shared/proc_stream.rs` — `ProcStreamState` resource (children, committed, chain
  faction, round) shared by both sides of the listen server (one Bevy World).
- `client/proc_stream.rs` — generation threads (python), child piece VISUALS
  (plain `SceneRoot`s, tag `ProcChildVisual`), commit detection (OwnPlayer y below
  the active map's lowest hub floor − 1.5 → nearest child by landing XZ), and the
  COMMIT SWAP: load child doc into the EditorWorkspace (despawn Map-owned
  `EditorPlaced`, `spawn_piece_record_pub` fresh — `editor_apply_materials` runs in
  playtest because `EditorMode` persists), `apply_hub_playtest_patches` +
  `export_playtest_layout`, shift player Transform+NetTransform by −offset
  (**origin rebasing** — the child re-loads at origin, no float drift, floors mask
  stays origin-centred), bump `KenneyPlaytestGeneration`.
- `server/map_stream.rs::spawn_proc_child_colliders` — colliders + floor cells for
  mounted children via the (now pub) `spawn_instance_pieces/_floors` +
  `MountedMap::candidate`. The generation bump makes `reload_kenney_playtest`
  despawn everything and rebuild the new active through the NORMAL path — colliders,
  agents, enemy nav, materials all just work.
- Child docs land in `userinput/maps/stream/child_r{round}_e{exit}.json`.
- Freeform hub exits are keyed "0"/"1" (kinds "trap"/"doorway"); `mount_offset`
  places the child spawn one MOD_H below the exit floor (~y −12).

### Known v1 gaps (in TODO.md P2)
Void fall if you outrun generation; plain materials on uncommitted children;
host-only commit detection; legacy pool streaming still parallel.

## 2. Enemy spawn height hardened (user: "still spawned inside synth floor")

Baked data was already correct (gated); made runtime robust regardless of stale
layouts: `spawn_agents_from_layout` snaps floor-0 spawns to the NAV cell floor
(+0.15), and `enemy_locomotion` clamps agents to ≥ nav floor + half_h every tick —
elevated deck blocks are hollow trimeshes, so a ground ray cast from INSIDE finds
the substrate below and previously let agents walk embedded. Hub/sub-level spawns
(y < −0.5) keep their baked y.

## 3. NPCs only in hub + hidden rooms

`_place_agent_spawns` (now called AFTER `build_hub`): NPCs pick hidden-room cells
(hidden slider) and hub floor −1 cells (y −3.85, trap holes excluded); none on the
regular floor. Enemies unchanged. `_agent_spawn` keeps explicit non-0.15 y.
test_nav_grid check #4 exempts y < −0.5.

## 4. Industrial machine BANKS (user: machines are modules that fit together)

`setup_industrial_factory_floor` §②: 2–4 machines laid FLUSH edge-to-edge
(probed widths, centred on a closed wall, checked only against pre-existing boxes,
added as a unit). Verified: bank centre gaps == piece widths (2.4/2.37 m). Conveyor
lines still get end machines from the previous round.

## Verification (all green)

```
python tools/test_zone_seams.py 15 · test_nav_grid.py 4 · test_transition_stairs.py
       · test_synth_transition.py · test_synth_interior_rules.py · verify_synth_placement.py
cargo build   (full, exe relinked)
```

**Streaming itself needs a live playtest** (G → play → drop through a hub exit) —
first in-game validation is the user's next session; watch the log lines
`proc stream: generating child…`, `child ready`, `COMMIT`, `active map swapped`.
