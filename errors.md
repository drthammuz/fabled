# Known Bugs & Visual Issues

> **Hub / extraction (2026-06):** Editor G playtest pit → hub → stairs area has unresolved visual/physics mismatches after multiple agent iterations. **Do not hotfix without reading [docs/hub-extraction-agent-failures.md](docs/hub-extraction-agent-failures.md).** Bugbot should review related diffs — see [AGENTS.md](AGENTS.md).

Status legend: [VERIFIED] = root cause proven + fix confirmed in data/build.
[NEEDS EYES] = correct fix applied + compiles, but only the user can confirm
the on-screen result (no GUI/headless way to verify rendering here).

---

## ACTIVE — pipe / water / dev-map iteration (remove each line once confirmed in-game)

Run with `host.bat` (passes `--host --test`) or `cargo run -- --test`. Remove
`--test` from host.bat to restore the real game (class-select + procgen + hub).
Bypass itself is CONFIRMED working (log shows `level 'testmap'` + auto Soldier).

- **KENNEY-VIS** [NEEDS EYES]: models invisible. TWO bugs: (1) the GLBs reference
  `Textures/colormap.png` (a subfolder) — it was placed next to the glb, so the
  texture 404'd and the whole glTF load failed → empty scene. FIX: texture now at
  `assets/models/space/Textures/colormap.png`. (2) The player spawns looking +Z
  (`LookAngles` default yaw=π) but the showcase was at −Z (behind them), AND
  `spawn_player` used the hardcoded sewer_entry spawns, ignoring the testmap's.
  FIX: testmap flipped to the +Z side; `spawn_player` takes the testmap spawns
  when in TestMode.
- **GRATE** [NEEDS EYES]: bars overshot the wall by sub-cm. FIX: chord clip
  radius reduced by half a bar-thickness (`rc = r - bar_d/2`) so the square bar's
  corner sits flush. Pipes now share the grate's rusted-steel material.
- **WATER** [NEEDS EYES]: looked like a dark static surface with a separate light
  wavy one under it. Real cause: bevy_water does its OWN depth-based see-through
  via `clarity` and IGNORES `AlphaMode::Opaque` — so the dark channel bed showed
  through under the slime surface. FIX: `clarity = 0.0` (truly opaque) on both the
  per-tile material AND `sewer_water_settings`, plus one uniform slime colour
  (all colours equal, edge_scale 0). Waves are geometry, so it still ripples.
- **PIPE-THICKNESS** [NEEDS EYES]: culvert pipe was a zero-thickness single
  surface. FIX: `pipe_tube_mesh` (inner wall + outer wall + rim annulus); inner
  radius unchanged (= stream width), 5 cm thickness added OUTSIDE, visible rim at
  the opening.
- **BEND** [VERIFY]: replaced the segmented elbow (gaps between pieces) with a
  single gap-free SWEPT-TUBE mesh per `bentpipe.txt` (circular profile swept
  along the quarter-arc + stub) — `PipeElbow` kind + `pipe_elbow_mesh`. Done
  procedurally in Rust (not a baked Blender asset) so it adapts to each bend's
  radius without distorting the circle.
- **WALLBARS** [NEEDS EYES]: bars stopped short of the roof. FIX: sewer arch
  side-posts run full height to the top bar; test-map bars span floor→ceiling.

---

## MODELS: switched to KayKit Adventurers (known-good)

The Kenney re-export *played* animation but deformed the mesh (bind-pose mismatch
from importing model + animations as separate FBXs). Rather than fight Blender
retargeting, the four classes now use the CC0 **KayKit Adventurers** GLBs already
in `assets/models/`:
- Soldier → Knight, Medic → Mage, Scout → Rogue, Tech → Barbarian

Each has 76 animations baked in with matching bind poses (so no deformation),
including exact "Idle" and "Walking_A". `CHAR_SCALE` 0.478 → 0.78 (model ~2.3
units tall → 1.8 m). The old Kenney `character_*.glb` + `export_characters.py`
are left in place but unused. Facing keeps the PI rotation; flag if backwards.

---

## ANIM-01 / ANIM-02: original Kenney clips were 2 frames [ROOT CAUSE — see MODELS above]

**Root cause (proven by inspecting the GLB binary):** every animation in every
character GLB contained only **2 keyframes spanning 0.04 seconds**. The Blender
export had selected the wrong action. Kenney's `idle.fbx` imports as TWO actions:
- `Root|Root|0.Targeting Pose` — 2 frames (a calibration pose, set as *active*)
- `Root|Root|Idle` — **33 frames** (the real animation)

Both the previous bot and the first attempt grabbed `armature.animation_data.action`
(the active one = the 2-frame pose). No wiring code could ever fix this — the
animation data was essentially empty.

**Fix:** `tools/export_characters.py` re-exports all four GLBs, selecting the
action with the **most keyframes** per FBX, retargeting onto the shared skeleton
via NLA tracks, and baking each over its own full range.

**Verified result (re-inspected the new GLBs):**
- Idle: 33 keyframes, 1.38 s (was 2 / 0.08 s)
- Walk: 17 keyframes, 0.71 s (was 2 / 0.08 s)
- Each GLB keeps its per-class skin texture.

Animation names are still "Idle"/"Walk", so the existing `find("idle")`/`find("walk")`
graph wiring matches unchanged.

---

## WALLS-01: Walls invisible / "5-10% visible, see-through" [FIXED — root cause found via runtime log]

**Actual root cause (proven by running the game and logging panel offsets):**
`panel_jitter` in `tunnel_mesh.rs` was missing a 20-bit mask. It computed
`(hash >> 12) as f32 / 1048576.0` — but `hash >> 12` is still a ~52-bit number,
so dividing by 2^20 produced values in the HUNDREDS OF MILLIONS instead of [0,1).
Every multi-panel (large) wall got a jitter offset of ~251,000,000 m and was
flung off the map → invisible. Only small walls that fit in ONE panel (early
return with `Vec3::ZERO` offset, no jitter) rendered — that was the "5-10%".

Material/metallic/lighting were red herrings; months of those tweaks couldn't fix
a geometry-position bug.

**Fix:** mask to 20 bits before the float divide:
`((hash >> 12) & 0xF_FFFF) as f32 / 1048576.0`. Verified jitter is now ±0.04 m.
Also gave walls a modest texture-modulated `emissive` self-glow so they read in
the dark sewer (metallic 0, no normal map).

---

## GAS-01: Gas was a green box, then invisible [NEEDS EYES]

**Root cause:** the volumetric `FogVolume` density was either too high (looked
like a solid colored box) or, after the last tweak, too low (0.04) *combined* with
a density texture that squared its noise toward zero → invisible.

**Fix:** density `0.22` with a rebuilt 2-octave noise texture that has a solid
floor (0.35) and smooth variation up to 1.0 — present everywhere, wispy structure,
never a flat box. This is true volumetric fog (Bevy `FogVolume` + camera
`VolumetricFog` + `VolumetricLight` lights), the real-games technique for gas.

---

## AIR-01: Dust was hard geometric squares [NEEDS EYES]

**Root cause:** Hanabi billboards with no texture are hard squares. Shrinking them
to 3 mm just made them vanish.

**Fix:** generate a soft radial alpha texture (`make_soft_dot`) and apply it via
`ParticleTextureModifier { ModulateOpacityFromR }` + `EffectMaterial`. Motes are
now 4 cm soft round glows that drift slowly — the TLOU2 / Stranger Things look,
not coded squares.

---

## WATER-01: Water flat / buried, only wave crests visible [NEEDS EYES]

**Root cause 1:** `spawn_channel_water` bypassed `bevy_water` with a flat plane.
**Fix:** real `WaterTile` + `StandardWaterMaterial` (animated Gerstner waves + PBR).

**Root cause 2 (the "too far down, only crests show"):** the surface Y was hardcoded
to `WATER_SURFACE_HEIGHT` (0.02) with amplitude 0.07 — wave troughs dipped to −0.05,
below the channel floor, so the alpha-blended water was occluded except at crests.
**Fix:** surface placed at `def.position.y + size.y/2 + 0.05` (above the floor) and
amplitude cut to 0.02 so the whole gentle ripple stays visible.

## DUST: still rendered as squares — render-modifier order [NEEDS EYES]

**Root cause:** `ParticleTextureModifier` ran BEFORE `ColorOverLifetimeModifier`
(Overwrite/RGBA), so the color modifier overwrote the alpha the texture had just
set — erasing the round shape, leaving a hard square.
**Fix:** reordered (color first, texture second so `ModulateOpacityFromR` multiplies
the final alpha) + explicit `AlphaMode::Blend`.

## GAS: never visible — thin floor-level slab [NEEDS EYES]

The `FogVolume`s were 1 m tall at ground level — a faint strip below standing eye
height. Setup (camera `VolumetricFog` + `VolumetricLight` lights) is correct.
**Fix:** fog volumes raised and grown to ~3 m tall so the green steam envelops the
player, density 0.22 with a 2-octave noise floor for wispy (non-box) structure.

---

## CHAR-01: Character faced wrong direction [VERIFIED FIXED — user confirmed]

PI rotation on the SceneRoot in both spawn sites in `netplay.rs`.

---

## DOUBLE-MODEL: remote player shows two overlapping models [FIXED]

**Root cause:** `attach_remote_player_visuals` (on `Added<PlayerName>`) and
`update_class_model` (on `Changed<PlayerClass>`, which fires on initial
replication too) BOTH spawned a SceneRoot child in the same frame. The despawn
in `update_class_model` couldn't see the other system's not-yet-committed child,
so two models survived — one got its animation rig wired (animated), the other
didn't (bind pose). Both followed the entity since both are children.
**Fix:** `update_class_model` is now the sole model owner; `attach_remote_player_visuals`
only spawns the name tag.

## CROUCH-STICK: model stays crouched/sunk after standing up [FIXED]

**Root cause:** the character's `move_and_slide` is a kinematic sweep with no
penetration recovery. Crouching settles the short capsule ~0.275 m lower; on
standing, `try_stand_up` swaps back to the tall capsule, which is then embedded
in the floor with nothing to push it out — so the body (and the fixed-offset
model) stayed low.
**Fix:** `enter_crouch`/`try_stand_up` now explicitly shift the body Y by half
the capsule-length difference (`CROUCH_Y_SHIFT`), keeping the feet planted and
restoring standing height deterministically.
