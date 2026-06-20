# Fabled — Update 2

Single source of truth: game context, technical specs (water, gas, motes, walls), and implementation instructions (M0–M7).

**Audience:** implementer. Execute milestones in order. Each must compile and pass acceptance before the next.  
**Do not:** re-enable village plugins, use Salva/SPH water, import Tiny Glade code, commit unless asked.

---

# Part I — Game context

## What it is

**Fabled** is a **server-authoritative co-op extraction game** (Rust + Bevy 0.18), pivoted from a village sim (parked behind `#[cfg(feature = "village")]` and git tag `base`).

**Loop:** party drops into underground sewers → explores dark stretches → fights patrol enemies → collects credits → reaches extraction airlock → returns to **hub camps** → buys gear (flashlight, map, pipe bat) → picks routes → permadeath ends the run when everyone dies.

**Tone:** industrial cyberpunk sewer — arched tunnels, toxic side channels, neon accents, volumetric gas, sparse airborne motes, animated water, flashlight cone in darkness.

## Tech stack

- **Engine:** Bevy 0.18 (client renders; server owns gameplay)
- **Physics:** Avian3D (kinematic move-and-slide character controller)
- **Net:** `bevy_replicon` + `bevy_replicon_renet`
- **VFX:** `bevy_hanabi`, `FogVolume`, `VolumetricFog`
- **Run modes:** `--host` (listen server), `--client <ip>`, `--server` (headless)
- **Launch:** `host.bat` (kills `fabled.exe`, rebuilds, runs `--host`)

## Architecture

| Layer | Role |
|--------|------|
| `crates/shared` | `LevelDef`, `StaticDef`, protocol, config, `RunState`, stretch graph |
| `crates/server` | Physics, combat, run/permadeath, level collider spawn, player input |
| `crates/client` | Rendering, input send, camera, UI, atmosphere — **no gameplay logic** |

**Level data flow:** `LevelDef` → server spawns cuboid colliders from `statics` → client spawns matching visuals (skips colliders for `Neon`, `SewerWater`, `SewerPipe`, `SewerBrace`, `Gable`).

**Movement:** client sends `PlayerInput` (WASD, sprint, **Ctrl=crouch**, jump, grab/throw, interact, shop, routes, attack, flashlight). Server runs character controller at 30 Hz fixed tick.

## Current state

**Working (C0–C6 skeleton):**

- Village runtime disabled; sewer extraction active
- `RunState` + party credits + all-dead ends run
- Hub shop + route selection (7/8/9)
- Stretch graph (`crates/shared/src/run.rs`)
- Melee enemies, crouch under `SewerDuct`, B0001 crouch fix (`enter_crouch` / `try_stand_up`)
- Flat cuboid walls, flat green water planes, heavy green `FogVolume` over channels

**Not done:**

- Levels are **hand-authored** (`StretchNode.build: fn() -> LevelDef`), not seeded procgen
- Walls are perfect axis-aligned boxes — textures cannot fix flat silhouettes
- Water is cosmetic only — no ripples, buoyancy, or splash physics
- Gas reads as opaque green boxes (`density_factor: 0.5`)
- No sparse tunnel-wide airborne motes

**`test_level()`** exists for physics tests — **not** the design target.

## Controls

WASD move · Shift sprint · **Ctrl crouch** · Space jump · LMB grab · RMB throw · E pickup · Q drop · F flashlight · V melee · 1/2/3 shop · 7/8/9 routes · R restart after wipe

## Conventions

- No magic numbers — use `shared/src/config.rs`
- Gameplay on server only
- Minimize scope; match existing style
- Windows: `taskkill /IM fabled.exe /F` before rebuild if link fails
- Use `ParamSet` when `SpatialQuery` conflicts with mut `Collider` queries (see crouch fix)

## Key files

| Area | Path |
|------|------|
| Level geometry | `crates/shared/src/level.rs` |
| Stretch graph | `crates/shared/src/run.rs` |
| Run / shop / extraction | `crates/server/src/run.rs` |
| Colliders | `crates/server/src/level.rs` |
| Character + crouch | `crates/server/src/character.rs` |
| Client visuals | `crates/client/src/level_render.rs` |
| Atmosphere | `crates/client/src/sewer_atmosphere.rs`, `darkness.rs` |
| Camera / fog | `crates/client/src/fly_camera.rs` |
| Protocol | `crates/shared/src/protocol.rs` |

---

# Part II — North star: procedural levels

**Goal:** each stretch generated from a **seed**, not a fixed builder per graph node.

1. Keep `LevelDef` + `StaticDef` as server/client interchange format
2. Replace `fn() -> LevelDef` with `fn(seed: u64, params: StretchParams) -> LevelDef`
3. Compose from modules: tunnel segments, hubs, junctions, scatter items/enemies from seed
4. **Graph stays authored** (which camps connect); **geometry/content is procedural**
5. Store `run_seed` in `RunState`; derive per-stretch sub-seeds for determinism
6. Server generates on transition; client reloads when `RunState.level_id` changes (already wired)

**Physics invariants (never break):**

- `SewerFloor` under full tunnel width including water channels
- Player spawns clear of `SewerDuct` colliders
- Skip colliders: `Neon`, `SewerWater`, `SewerPipe`, `SewerBrace`, `Gable`
- `SewerDuct` **has** collider (crouch to pass)

```rust
fn stretch_seed(run_seed: u64, stretch_id: &str) -> u64;
fn segment_seed(stretch_seed: u64, index: u32) -> u64;
```

---

# Part III — Atmosphere: water, gas, motes

Three stacked layers. Gas and motes must **not** obscure gameplay (enemies, walkways, extraction).

```
┌────────────────────────────────────────────────────────────┐
│ LAYER 3 — Airborne motes (hanabi, tunnel volume)           │
│   Rare dust/spores catching flashlight beam                │
├────────────────────────────────────────────────────────────┤
│ LAYER 2 — Gas (FogVolume + 3D noise + VolumetricLight)     │
│   Low toxic haze; thicker near water; god-rays             │
├────────────────────────────────────────────────────────────┤
│ LAYER 1 — Water (bevy_water + ripples + server physics)    │
│   Animated surface; impact ripples; wade/buoyancy            │
└────────────────────────────────────────────────────────────┘
```

## III.A — Water

### Goal

Channels look and move like liquid. Walking in and throwing props causes visible and physical reaction.

### Stack (reject Salva/SPH)

| Piece | Technology | Owner |
|-------|------------|-------|
| Surface waves + reflections | [`bevy_water` 0.18](https://github.com/Neopallium/bevy_water) | Client |
| Impact ripples | Custom 2D GPU ripple sim per channel | Client |
| Float / wade / drag | Avian buoyancy + `WaterVolume` sensor | Server |
| Splashes | `bevy_hanabi` burst on `WaterImpact` | Client |

**Reject:** Salva3D / bevy-sph — Rapier coupling, too expensive, bad for multiplayer.

### Rendering (`bevy_water`)

```toml
# crates/client/Cargo.toml
bevy_water = { version = "0.18" }  # optional features = ["ssr"]
```

- `WaterTile` per `SewerWater` `StaticDef`, scaled to `def.size.x × def.size.z`
- Sewer tuning (not ocean):

| Parameter | Value |
|-----------|-------|
| Wave amplitude | 0.02–0.05 m |
| Wave speed | 0.3–0.8 |
| Base tint | toxic green `(0.05, 0.45, 0.18)` |

- `get_wave_point()` for client-side bob of floating props

### Ripples (per channel)

Pattern from [InteractiveWaterSystem](https://github.com/daothienphu/InteractiveWaterSystem):

1. 256² or 512² RG32F RT (height + velocity)
2. WGSL wave-equation propagation each frame
3. Inject circular impulse on `WaterImpact` at world→UV
4. Water shader samples ripple for normal/displacement

### Server physics

Port [coreh Avian liquids gist](https://gist.github.com/coreh/8fb96cc9684d1e16a5a93297554155ec):

- `WaterVolume` — thin sensor box at each `SewerWater` (replace “skip collider”)
- **Dynamic props:** buoyancy (`ExternalForce`) + velocity damping
- **Kinematic players:** `PLAYER_WADE_SPEED` ~50% walk, horizontal drag, no swim
- Emit `WaterImpact { channel_id, position, impulse }` on entry/splash

```rust
// shared/protocol.rs
pub struct WaterImpact {
    pub channel_id: u32,
    pub position: Vec3,
    pub impulse: f32,
}
```

### Splash VFX

Separate hanabi effect: short burst, impulse-scaled, green droplets.

### Water acceptance

- [ ] Surface animates; specular from neon/flashlight
- [ ] Thrown prop → ripples + splash + bob/slow
- [ ] Player wades slower; footfalls emit small ripples
- [ ] No falling through channels (`SewerFloor` stays)

### Water references

- [bevy_water](https://github.com/Neopallium/bevy_water)
- [Water in games is not real](https://tigerabrodi.blog/water-in-games-is-not-real)

---

## III.B — Gas (volumetric toxic atmosphere)

### Goal

Toxic **gas** in the air — thicker in pockets, lit by neon and flashlight, **not** a solid green fog cube.

### Technology

Bevy 0.18 requires **both**:

- `VolumetricFog` on **camera** (`fly_camera.rs`)
- `FogVolume` entities in scene (`sewer_atmosphere.rs`)

Reference: [Bevy volumetric fog example](https://bevy.org/examples/3d-rendering/volumetric-fog/)

**Current problem:** `density_factor: 0.5`, color `(0.12, 0.92, 0.38)` — cut density ~4×, desaturate.

### Two gas types

| | Channel steam | Tunnel haze |
|--|--------------|-------------|
| Placement | Above/near `SewerWater` | Along walkway ~20 m segments |
| Color | `(0.12, 0.55, 0.22)` | blue-grey `(0.35, 0.42, 0.48)` |
| `density_factor` | 0.12–0.22 | 0.04–0.08 |
| Height (scale.y) | 0.4–1.2 m | 2–3.5 m |

### `FogVolume` template

```rust
FogVolume {
    fog_color: Color::srgba(0.12, 0.55, 0.22, 1.0),
    density_factor: 0.15,
    absorption: 0.25,
    scattering: 0.55,
    scattering_asymmetry: 0.6,
    light_tint: Color::srgb(0.2, 0.9, 0.35),
    light_intensity: 0.8,
    density_texture: Some(noise_3d_handle),
    density_texture_offset: Vec3::ZERO,
    ..default()
}
```

### 3D density texture (required for quality)

`FogVolume.density_texture` = 3D voxel mask. Without it, fog is a homogeneous box.

1. **32³ or 64³** R8, Perlin/Simplex 3D noise
2. Bias low: `density *= (1.0 - uvw.y).powf(1.5)`
3. `ImageAddressMode::Repeat`
4. Animate: `density_texture_offset += Vec3(0.02, 0.0, 0.01) * dt`

Generate at startup (`noise` crate) or pre-bake `assets/textures/fog_noise_32.ktx2`.

### Volumetric lighting

```rust
commands.entity(light).insert(VolumetricLight);
```

- Flashlight `SpotLight` in `darkness.rs`
- Neon `PointLight` in `level_render.rs`
- Camera `VolumetricFog`: `ambient_intensity` 0.1–0.2, `step_count` 32–48

### Procgen gas zones

```rust
pub enum GasZoneKind { ChannelSteam, TunnelHaze, HubClear }
```

| Zone | Notes |
|------|-------|
| Walkway | Low blue-grey haze per ~20 m |
| Water channel | Green steam per `SewerWater` |
| Hub | Minimal haze |
| Extraction | Slight uplift |

### Gas acceptance

- [ ] Walkway readable 10+ m without flashlight
- [ ] Flashlight beam visible in haze
- [ ] Wispy gas (noise mask), not solid green box
- [ ] Neon tints nearby fog green-cyan

---

## III.C — Sparse airborne motes

### Goal

Occasional dust/spores in tunnel air — alive, not annoying. Flashlight catches specks; not snow.

### Separate from

- **Water boil** — rising green bubbles at channels (rate 15–25/s)
- **Splash bursts** — event-driven, short-lived
- **Air motes** — tunnel-wide, rare, neutral grey-green

### Targets

| Metric | Bad | Target |
|--------|-----|--------|
| Spawn rate | 48/s | **2–4/s** per zone |
| Capacity | 4096 | **512–768** |
| Lifetime | 2.2 s | **8–20 s** |
| Size | large | **0.008–0.025 m** |
| Peak alpha | 0.85 | **0.05–0.18** |

### `sewer_air_motes` hanabi effect

```rust
SetPositionBoxModifier { half_size: (12, 1.8, 35) }  // per segment
SetVelocitySphereModifier { speed: 0.08 }
SetAttributeModifier::LIFETIME = 14.0
SetAttributeModifier::SIZE = 0.015
LinearDragModifier(0.15)
AccelModifier(Vec3::new(0.0, 0.02, 0.0))
SpawnerSettings::rate(3.0)
FaceCamera billboards
```

Color gradient: grey-green, fade in/out, peak alpha 0.12.

**Placement:** one emitter per ~20 m walkway segment (not per water channel). ~3–4 emitters per 70 m tunnel.

### Motes acceptance

- [ ] Occasional specks in flashlight cone
- [ ] Never obstructs view
- [ ] 60 fps with motes + gas + water
- [ ] Distinct from green water bubbles

---

# Part IV — Procedural tunnel geometry (walls)

## Problem

Walls are `StaticDef` **axis-aligned cuboids** — perfectly flat silhouettes. Long 70 m single boxes. Arched vault approximated by a few rotated cuboids.

**Goal:** industrial metal conduit, collapsed rock, or patched transitions — without hand-modeling every stretch.

## Inspiration (not a dependency)

Anastasia Opara’s Bevy procedural wall work ([80.lv updated](https://80.lv/articles/bevy-engine-powered-procedural-wall-generator-updated), [original](https://80.lv/articles/a-procedural-wall-generator-made-with-bevy-engine)) → **Tiny Glade** ([interview](https://80.lv/articles/exclusive-tiny-glade-developers-discuss-bevy-proceduralism-publishers-cozy-games)). Use as **design reference** only — do not import her code.

| Opara pattern | Fabled mapping |
|---------------|----------------|
| Paths cross walls → arches | Centerline spline drives tunnel sweep |
| Walls stick to terrain | Floor/water level varies per segment |
| Modular tiles | Panel recipes (metal / rock / patch) |
| Openings cut walls | Ducts, doorways, collapse holes |

**No brick required.** Same pipeline, different surface recipes.

## Surface types

```rust
pub enum TunnelSurface {
    IndustrialMetal,   // riveted panels, ribs, pipe brackets
    CollapsedRock,     // noise-displaced cave
    Patchwork,         // metal + rock transition
    MaintainedConcrete,// hubs, newer sections
}
```

Pick per stretch from seed, depth, and `CampKind` (deeper → more rock).

## Pipeline (Tier 2 target)

```
1. Centerline polyline
2. Cross-section: Arch | Horseshoe | IrregularCave
3. Extrude mesh along centerline
4. Cut openings (ducts, doorways)
5. Surface recipe (metal panels vs rock noise)
6. Detail pass (pipes, neon, rubble)
7. Server: coarse cuboid colliders (≤ 8 per segment) — NOT render mesh
```

## Tier 1 — Fast win (panel breakup)

Keep server cuboid colliders. Client only:

| Technique | Metal | Rock |
|-----------|-------|------|
| Panel breakup | 2–4 m panels, ±0.01–0.03 m offset | Irregular chunks |
| Vertex displacement | Crease at seams | Fractal noise on normal |
| Props | Bolts, pipe clusters | Rubble, stalactites |

Split any `SewerWall` face > 3 m. Optional `tunnel_displace.wgsl`.

## Tier 2 — Segment sweeps

```rust
pub struct TunnelSegmentSpec {
    pub centerline: Vec<Vec3>,
    pub profile: TunnelProfile,
    pub surface: TunnelSurface,
    pub seed: u64,
    pub openings: Vec<TunnelOpening>,
}

pub enum TunnelProfile {
    Arch { radius: f32 },
    Horseshoe { floor_width: f32, arch_radius: f32 },
    IrregularCave { base_radius: f32, noise_amplitude: f32 },
}

pub struct TunnelSegmentDef {
    pub id: u32,
    pub spec: TunnelSegmentSpec,
    pub collider_statics: Vec<StaticDef>,
}
```

Extend `LevelDef`:

```rust
pub tunnel_segments: Vec<TunnelSegmentDef>,
pub statics: Vec<StaticDef>,  // floors, water, props
```

**Collider rule:** render mesh ≠ physics mesh. Server uses proxy cuboids only.

## Tier 3 — Hero SDF caves (optional)

One-off blown-out rooms only. Not for every corridor.

## Wall acceptance

- [ ] Tier 1: panel breaks visible; collision unchanged
- [ ] Tier 2: continuous arch; metal panel rhythm; rock feels lumpy
- [ ] Avian movement unchanged

## Wall references

- [80.lv procedural wall generator](https://80.lv/articles/bevy-engine-powered-procedural-wall-generator-updated)
- [Opara paths/arches thread](https://threadreaderapp.com/thread/1530473522224582656.html)

---

# Part V — Implementation milestones (M0–M7)

Execute in order. Verify with `host.bat` after each.

```
M0  Foundation (seed + procgen skeleton)
M1  Atmosphere quick wins (gas + motes)
M2  Tunnel Tier 1 (panel breakup)
M3  Procedural stretch generator (wired to run)
M4  Tunnel Tier 2 (segment sweeps)
M5  Water surface (bevy_water)
M6  Water interaction (ripples + server physics + splashes)
M7  Polish (lighting, textures, combat, multiplayer)
```

---

## M0 — Foundation: seed + procgen skeleton

1. Add `run_seed: u64` to `RunState`; init on new run
2. New `shared/src/procgen.rs`: `stretch_seed`, `segment_seed`, `StretchParams`, `generate_stretch`
3. Change `StretchNode.build` to `fn(u64, &StretchParams) -> LevelDef`
4. `server/src/run.rs`: pass seed on stretch transition
5. Enforce physics invariants (Part II)

**Acceptance:** same seed → same level; `--host` works; extraction/hubs work.

---

## M1 — Atmosphere: gas + motes

### M1a — Retune gas + volumetric lighting

**Files:** `sewer_atmosphere.rs`, `darkness.rs`, `level_render.rs`, `fly_camera.rs`

- Split effect handles: `WaterBoilEffect`, `AirMotesEffect`, `FogNoiseTexture`
- Apply gas values from Part III.B
- `VolumetricLight` on flashlight + neon
- Water boil rate **15–25/s**

### M1b — 3D fog density texture

- New `client/src/fog_noise.rs`
- 32³ noise texture, shared across `FogVolume`s
- Animate `density_texture_offset`

### M1c — Sparse airborne motes

- New `client/src/ambient_particles.rs`
- `sewer_air_motes` per Part III.C
- One emitter per ~20 m segment

**Acceptance:** Part III.B + III.C checklists.

---

## M2 — Tunnel Tier 1: panel breakup

- New `client/src/tunnel_mesh.rs` + plugin
- Split `SewerWall` into seeded panels in `level_render.rs`
- Optional `tunnel_displace.wgsl`
- **No server collider changes**

**Acceptance:** Part IV Tier 1 checklist.

---

## M3 — Procedural stretch generator

- Expand `procgen.rs`: grid/spine layout, jittered spawns, `GasZone` metadata
- `generate_hub(seed, CampKind)`
- All `stretch_graph()` nodes use seeded builder
- `TunnelSurface` pick per region

**Acceptance:** Part II procgen checklist.

---

## M4 — Tunnel Tier 2: segment sweeps

- New `shared/src/tunnel.rs`
- `LevelDef.tunnel_segments`
- `build_tunnel_mesh()` client; `to_colliders()` server
- Procgen emits segments instead of long wall cuboid chains

**Acceptance:** Part IV Tier 2 checklist.

---

## M5 — Water surface

- `bevy_water = "0.18"` in client
- New `client/src/water_render.rs`
- Replace flat `SewerWater` planes with `WaterTile`s (Part III.A)

**Acceptance:** water surface animates; no movement regression.

---

## M6 — Water interaction

### M6a — Server

- `WaterImpact` in protocol
- New `server/src/liquids.rs` (coreh gist)
- `WaterVolume` sensors in `server/level.rs`
- `PLAYER_WADE_SPEED` etc. in config
- `ParamSet` if query conflicts

### M6b — Client

- Per-channel ripple RT + WGSL
- Hanabi splash on `WaterImpact`
- `get_wave_point()` for prop bob

**Acceptance:** Part III.A water checklist.

---

## M7 — Polish

1. Lighting: dark stretches, brighter hubs, flashlight essential
2. Textures: `tools/fetch_cyberpunk_textures.ps1`; per-piece UVs
3. Combat: patrol paths, depth scaling
4. Hub shop stock by `CampKind`
5. Multiplayer: `--server` + `--client` smoke test
6. `GasZone` from procgen drives fog/mote placement

**Acceptance:** full run loop; remote client syncs; no panics/B0001.

---

# Part VI — File checklist

| File | Action |
|------|--------|
| `shared/src/procgen.rs` | **New** M0/M3 |
| `shared/src/tunnel.rs` | **New** M4 |
| `shared/src/run.rs` | `run_seed`, builder sig M0 |
| `shared/src/level.rs` | `tunnel_segments`, `GasZone` M3/M4 |
| `shared/src/protocol.rs` | `WaterImpact` M6 |
| `shared/src/config.rs` | All tunables |
| `server/src/run.rs` | Seed on transition M0 |
| `server/src/liquids.rs` | **New** M6 |
| `server/src/level.rs` | WaterVolume, segment colliders |
| `client/src/ambient_particles.rs` | **New** M1 |
| `client/src/fog_noise.rs` | **New** M1 |
| `client/src/tunnel_mesh.rs` | **New** M2/M4 |
| `client/src/water_render.rs` | **New** M5/M6 |
| `client/src/sewer_atmosphere.rs` | Refactor M1 |
| `client/src/level_render.rs` | Panels, lights, water delegate |
| `client/src/darkness.rs` | VolumetricLight M1 |
| `client/src/fly_camera.rs` | VolumetricFog tune M1 |
| `client/src/lib.rs` | Register plugins |
| `client/Cargo.toml` | `bevy_water`, optional `noise` |
| `assets/shaders/tunnel_displace.wgsl` | Optional M2 |
| `assets/textures/fog_noise_32.ktx2` | Optional M1 |

---

# Part VII — Performance budget

| System | Limit |
|--------|-------|
| `FogVolume` count | ≤ 12 |
| 3D fog texture | 32³ shared |
| Hanabi emitters | ≤ 6 motes + ≤ 4 boil + splashes |
| Ripple RTs | 512² × ≤ 4 channels |
| Tunnel segments / stretch | 4–12 |
| `VolumetricFog.step_count` | 32–48 |

---

# Part VIII — Explicit rejections

- Salva3D / bevy-sph for water
- Tiny Glade / Opara code import
- Render mesh as server trimesh for full tunnels
- Homogeneous `FogVolume` at high density without noise texture
- Mote spawn > 10/s per emitter
- Re-enabling village plugins without explicit request
- `test_level()` as production target

---

# Part IX — Testing

```bat
host.bat
```

```powershell
cargo run -- --server
cargo run -- --client 127.0.0.1
```

After each milestone: spawn → crouch duct → throw object → reach extraction.

---

# Summary

Implement **M0 → M7 in order**. This document is the only spec needed.

**One line:** Seeded procgen stretches + non-flat tunnel meshes (metal/rock) + layered atmosphere (gas, sparse motes, real water with server physics) on the existing server-authoritative sewer extraction loop.
