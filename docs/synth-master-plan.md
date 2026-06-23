# Synth — master plan & roadmap (interior + faction distribution)

**Status:** Active (2026-06-24). Transition seam stable; **dressing sandbox** is the live interior lab (balconies, mezzanine, room-first furnish). Procgen still uses the old scatter pass until `furnish_showcase` logic is wired into `synth_transition.py`.
**Supersedes for navigation;** detailed rules still live in the linked docs.

## Document map (all synth-process docs)

| Doc | What it holds |
|-----|---------------|
| [synth-transition-architecture.md](synth-transition-architecture.md) | **Transition rules + OPEN DEFECTS table (D1–D11).** Stairs, deck, doors, walls, elevation, chirality, integrity. Single source of truth for the seam. |
| [handover-synth-2026-06-23.md](handover-synth-2026-06-23.md) | Session handover (transition + dressing); pipeline order, gotchas, verify commands. **Updated 2026-06-24.** |
| [procgen-zone-composition.md](procgen-zone-composition.md) | **How faction %s are distributed / map areas built** (options A–I). Layout is currently faction-blind; zone = paint-on-top. |
| [procgen-faction-manifest.md](procgen-faction-manifest.md) | Phased procgen plan; industrial-substrate-with-grafted-faction-buildings vision (§2, §6.1). |

## Where the transition work landed (done)

D1–D10 fixed, D11 resolved-by-design (one entrance per synth zone). Synth = one elevated 1.2 m platform, fully walled, entered by stairs+door; floors flush; doors flanked; reachability guaranteed. Remaining: **D5 spawn-fall** pending a user rebuild (Rust change). Minor: dead deck cells on 1-wide-door/3-wide-stair runs.

## The strategic decision this unblocks

Today the synth zone is an **arbitrary blob** painted onto a faction-blind layout (zone-composition §1), so the transition code must adapt to *every* possible seam shape (width 1–7, corners, splits). The user's insight: **if synth always had a bounded, controlled footprint, it would always get the same small set of transitions** — no need to optimise for all combinations, and the interior could be authored for known shapes.

**Recommendation (feeds zone-composition option E/F):** treat a synth zone as a **bounded building footprint grafted onto the industrial substrate** — a small catalogue of footprint archetypes (e.g. rect hall, L-wing, 2×2 tower) with **fixed entrance patterns** (1 stairs+door each). Then:

- transitions collapse to ~3 canonical types (straight / corner / wide), already handled;
- interiors are authored per archetype (room layout, where windows/balconies/second floors go);
- faction % becomes "how many building footprints of size S/M/L", which is far more controllable than arc-length painting.

**This is a Phase-4-scale change** (zone-composition §2E/F) — NOT to be done now. But the interior pass below is built so it works on the *current* blob footprint AND will carry over to bounded footprints.

## Interior implementation plan (start now, footprint-agnostic)

The synth kit has everything (`assets/models/factions/synth/`): `wall-window*`, `wall-banner`, `wall-detail`, `display-wall`, `wall-switch`, `wall-pillar`, `balcony-floor/rail*`, `rail`, furniture (`bed*`, `chair*`, `computer*`, `table*`, `container*`), `structure-panel`/`structure` + indoor `stairs`/`stairs-ramp` for second floors.

GLB facts (probed, scale 4): `wall`, `wall-window`, `wall-banner`, `wall-pillar` share the **exact `wall` footprint** → drop-in swaps. `wall-detail`/`display-wall`/`wall-switch` are small **attached** decals (mount on inner wall face). **`balcony-floor-center` / `balcony-floor-corner` are DIRECTIONAL** (bbox symmetric but mesh has a raised lip on +z; corner lip wraps −x/+z) — yaw must point the lip **outward**. See `placement_catalog.json` `orientation` and `tools/synth_interior.py` (`FLOOR_EDGE_YAW`, `CORNER_FLOOR_YAW`). `rail` / `rail-narrow` are symmetric barriers; rails trace the **union outer boundary** (`expected_balcony_rails`), not one long rail per face. Furniture are floor props.

Order (user priority):

1. **Wall decorations + windows — ✅ DONE (2026-06-23).** `synth_transition.decorate_synth_walls` (called in `to_doc`): ~18% of interior dividers → `wall-banner`; perimeter walls → `wall-window` (~45%). Pure stem swaps (same x/z/yaw/y/scale, still solid walls), deterministic per seed.

   **HARD RULE — windows/balconies only face a real map tile, never void.** A window onto void = the player sees into (and can jump through to) the unbuilt world. A perimeter wall becomes `wall-window` ONLY when the **far-side cell is in `walkable`** (a tile/floor exists there) — i.e. where synth abuts the industrial substrate. Verified @25 seeds 1–39: **0 windows face void**; sweep unchanged. Synth blobs are mostly surrounded by void, so real windows are sparse (~5/map) on synth↔industrial interfaces — correct. Tagged `synth_decor`/`synth_window`/`synth_banner`/`synth_pillar`. **The same far-side-is-walkable rule MUST gate balconies (step 4).**
2. **Furniture pass — ⚠️ FIRST PASS (2026-06-23), user rejected quality.** `synth_transition.furnish_synth_interior` exists and does not regress structure (sweep @25 seeds 1–39: 0 unreached/stacks/doors-in-wall; piece `y` = 1.2 m after elevation pass). **Editor playtest (same day):** props feel random — `display-wall` in corridors, wrong facing ~75% of the time, wall-mount pieces treated as floor furniture. Root causes below (§ Decoration catalog + § Procgen strategy). **Do not mark done** until room-aware placement lands.
3. **Wall-mounted decals — NOT STARTED.** `wall-detail`, `display-wall`, `display-wall-wide`, `wall-switch` must **not** go through the floor-prop pass. Separate pass: mount on an **interior wall piece** inner face (offset from face centre, inherit wall yaw, probe z-protrusion). Same void rule as windows does not apply (interior dividers only).
4. **Balconies — ✅ DONE in dressing sandbox (2026-06-24).** `tools/synth_interior.py`: `expected_balcony_floors` + `expected_balcony_rails` (single source of truth; generator + `verify_synth_placement.py`). Perimeter ledges on exterior room edges in the **Dressing** workflow (`interior_showcase.json`, `balcony_test.json`). Lip yaw per face/diagonal; rails follow merged ledge boundary (no inner-corner crossing). **Not yet wired into live procgen** (`synth_transition.py`) — dressing-only until user confirms on bounded footprints.
5. **Indoor stairs + mezzanine — ✅ DONE in dressing sandbox (2026-06-24).** Command hall in `interior_showcase.json`: stair-up mezzanine (`mezzanine_plan`, `add_command_mezzanine`), stair fill blocks, playtest sync gated for dressing workflow. Short stairs for compact runs still optional polish.
6. **Floor surface detail — ✅ DONE (2026-06-24).** `tools/gen_floor_detail.py` → `Textures/floor_detail.png`; client `SynthFloor` material tiles scratches on `floor` stems (build tag in `EDITOR_BUILD_TAG`).

---

## Decoration catalog (`assets/models/factions/synth/`)

Every stem below is available in-folder (~100 GLBs). Grouped by **how procgen must place them** — not by art category.

### A. Drop-in wall swaps (same footprint as `wall` @ scale 4)

Pure stem swap on an existing `role=wall` piece: same x/z/yaw/y/scale, still solid, integrity unchanged.

| Stems | Use |
|-------|-----|
| `wall-window`, `wall-window-banner`, `wall-window-frame`, `wall-window-shutters` | Perimeter only; **far-side cell ∈ walkable** (never void) |
| `wall-banner`, `wall-pillar`, `wall-pillar-banner` | Interior dividers; occasional variety |
| `wall-corner`, `wall-corner-banner`, `wall-corner-round`, `wall-corner-round-banner` | Corners (base-gen today; banner variants optional later) |

**Implemented:** `decorate_synth_walls` — windows + banner/pillar on dividers. **Not implemented:** window shutter/frame variants; corner banner swaps.

### B. Wall-mounted decals (attach to inner face — NOT floor props)

Small meshes (~0.4–0.7 m tall @ scale 1 → ~1.6–2.8 m @ scale 4). Origin sits on/near the wall plane; content protrudes into the room (+local Z).

| Stems | Typical use |
|-------|-------------|
| `display-wall`, `display-wall-wide` | Status screens, mission boards — **labs, command cells, dead-end alcoves** |
| `wall-detail` | Generic panel / greeble — any interior wall, low density |
| `wall-switch` | Light/access panel — near doors, corners |

**Placement rules:** pick a **wall piece** on an interior synth↔synth face; offset ~1.5 m along the wall from cell centre toward the room; yaw = wall yaw (display faces into room); `y` = deck top (1.2 m) or mid-wall if probed. **Never** scatter at floor cell centres. **Never** on corridor walls that are the only path between two areas.

### C. Floor furniture (cell-centre or wall-backed offset)

| Stems | Footprint @ scale 4 (approx) | Room fit |
|-------|------------------------------|----------|
| `computer`, `computer-screen`, `computer-wide`, `computer-system` | 1.6–3.6 m | Workstations — **against a wall**, face room; group 2–3 in labs |
| `container`, `container-wide`, `container-tall`, `container-flat`, `container-flat-open` | 2.4 m cube-ish | Storage — corners, dead ends, **away from doors** |
| `table`, `table-large`, `table-inset`, `table-inset-small` | 2–3 m | Dining/meeting — **centre of open bays** (≥3×3) |
| `table-display`, `table-display-small`, `table-display-planet` | 2–3 m | Foyer/showpiece — **entrance foyer only** (transition already uses planet @ deck) |
| `bed-single`, `bed-single-cover`, `bed-double`, `bed-double-cover` | 2×4 m | **Small enclosed rooms** (area ≤6 cells, one entrance) |
| `chair`, `chair-cushion`, `chair-headrest`, `chair-armrest`, … | 1.2 m | Paired with `table*` or `computer*` (same cluster, yaw toward desk) |

**Current bug:** `furnish_synth_interior` includes `display-wall` in `_WALL_PROPS` — a category-B stem in a category-C pass. Remove it when wall-decal pass exists.

### D. Balcony / railing (perimeter, gated)

| Stems | Notes |
|-------|-------|
| `balcony-floor`, `balcony-floor-center`, `balcony-floor-corner` | Perimeter ledge tiles; **lip faces outward** (4 edge yaws + 4 corner yaws — not cosmetic) |
| `balcony-rail`, `balcony-rail-center`, `balcony-rail-corner`, `rail`, `rail-narrow` | Guardrails @ deck 1.2 m; `rail` 4 m / `rail-narrow` 2 m on union outer boundary |

### E. Structure / vertical (second floor — deferred)

| Stems | Notes |
|-------|-------|
| `structure`, `structure-panel`, `structure-barrier`, `structure-barrier-high` | Mezzanine / support; multi-floor path disabled (`_MULTI_FLOOR_ENABLED=False`) |
| `stairs`, `stairs-corner`, `stairs-ramp`, `stairs-handrail*`, … | Indoor vertical links — large rooms only |

### F. Transition / structure (not interior decor)

Doors (`wall-door*`, `door-single*`, `door-double*`), `floor` / `floor-panel*`, `stairs-small-*`, pipes, rocks, `skip*` — owned by base-gen, transition, or future industrial dressing. **Do not** reuse in the interior scatter pass.

---

## Procgen strategy — organic interior (target design)

The first furniture pass is **cell-random** (14% Bernoulli per eligible floor cell). That cannot look lived-in. Target: **room-first, role-second, scatter-last**.

### Step 0 — Map analysis (once per synth zone)

Before placing any decor, derive from `walkable` + walls + doors:

| Derived set | Definition |
|-------------|------------|
| **Rooms** | 4-connected floor components separated by walls/doors (flood-fill; treat door as passage) |
| **Corridor cells** | Walkable cells with ≥2 open cardinal neighbours **or** degree-2 chain between two room mouths |
| **Dead ends** | Exactly one open neighbour — good for containers, single `wall-detail` |
| **Open bays** | Room area ≥9 cells with a ≥3×3 empty core — centre tables, workstation rows along one wall |
| **Entrance band** | BFS depth ≤3 from main transition door + all `deck_cells` — **sparse** (foyer prop only) |
| **Spine proximity** | Distance along mission spine — optional bias: quarters near start, labs mid, storage far (tunable per seed) |

### Step 1 — Assign room roles (deterministic per seed)

Each room gets one primary role from size + topology:

| Role | Heuristic | Primary props |
|------|-----------|---------------|
| **Quarters** | area ≤6, one door | `bed-*`, `container`, `chair` |
| **Lab / ops** | medium, ≥1 long wall | `computer-system` row + `chair`; `display-wall` on that wall (decal pass) |
| **Storage** | dead-end or low degree | `container*`, `wall-detail` |
| **Corridor** | corridor cell | **nothing** (or `wall-switch` on alcove only) |
| **Bay / mess** | open bay | `table-large` + 2–4 `chair` facing it |
| **Command** | largest room, near spine centre | `table-display*` centre + wall decals |

### Step 2 — Clusters (grouped setups)

Use named **prop setups** (manifest §5.4b pattern) instead of independent random picks:

| Setup | Pieces | Placement |
|-------|--------|-----------|
| `workstation` | `computer` + `chair` (chair yaw +180° toward screen) | Wall-backed offset 1.2 m from wall |
| `server_nook` | 2× `container` + `computer-screen` | Corner, two walls |
| `mess_table` | `table-large` + 4× `chair` | Bay centre, chairs face table |
| `bunk` | `bed-single` + `container` | Small room, bed on longest wall |
| `status_wall` | `display-wall` or `display-wall-wide` | Wall decal pass, not floor |

Place **whole setup or skip** — never half a cluster.

### Step 3 — Rotation rules

| Kind | Yaw rule |
|------|----------|
| **Wall-backed floor prop** | Face **into room** = `OPPOSITE[wall_side]`; use **the wall the prop backs onto**, not `wall_sides[0]` |
| **Chair at table** | Face table centre (atan2 toward table − chair) |
| **Wall decal** | Match hosting **wall piece yaw**; offset along wall tangent |
| **Centre piece** | Default yaw 0 or align to room's longest axis |

Convention: Kenney props **front = +local Z @ yaw 0** (same as `furnish_synth_interior` comment). **Per-stem yaw_offset** may be needed — probe each stem in `mesh_metrics` before trusting atan2 alone (`table-display-small` has origin below mesh base).

### Step 4 — Hard exclusions

- **Corridors:** no floor props that block 4 m width; corridors are transit, not living space.
- **Door/stair/deck band:** keep clear (current 3×3 block is good; extend for large props).
- **Adjacent props:** keep "no two adjacent" OR allow only if same **cluster**.
- **Void-facing windows/balconies:** far-side ∈ walkable (already enforced for windows).

### Step 5 — Density

Target **~8–15 placed setups per zone** (not ~13 independent random cells). Sparse globally, **dense inside assigned rooms**, empty corridors — reads organic.

### Known issues in current code (`furnish_synth_interior`)

| Issue | Evidence @25 seeds 1–39 |
|-------|-------------------------|
| `display-wall` in floor prop list | 72 wall-mount stems floor-placed |
| No corridor filter | ~73% of props on corridor-like cells |
| `wall_sides[0]` arbitrary | Wrong facing when multiple walls; user reports ~75% bad in editor |
| No wall offset | Props at cell centre, not tucked to wall |
| No room typing | Beds never in small rooms; computers not grouped |
| `table-display-small` origin | Mesh extends −0.3 @ scale 1 → visual sink/float even when `y=1.2` |

**Elevation note:** JSON pieces get `y=1.2` from `_apply_zone_elevation`; if editor still shows wrong height, check in-game collider/render path for `role=prop` (separate from D5 spawn issue).

---

### Verification for the interior pass

**Transition (procgen @ cells=25):**

- Must NOT regress the D-table: re-run the @cells=25 sweep (reachability/stacks/doors-in-wall/enclosure/flush all 0).
- **Render to SEE it** (`tools/_render_map.py`, `_render_zoom.py`) — the lesson from D8.
- **Editor G playtest** on seeds 1, 5, 6 — text metrics alone are insufficient.

**Dressing sandbox:**

```bash
python tools/gen_dressing_showcase.py    # regenerates interior_showcase.json + audit
python tools/gen_balcony_test.py         # orientation + L-room concave-corner test
python tools/verify_synth_placement.py userinput/synth_dressing/*.json
python tools/audit_synth_scene.py userinput/synth_dressing/interior_showcase.json
```

- Red flags: bed deck height, stacked stairs, **rail crossings** (`check_rail_crossings`), balcony floor/rail layout vs `expected_balcony_*`.
- Load in **`dressing.bat`** (not `editor.bat`); confirm build tag in window title (`EDITOR_BUILD_TAG`).
- Decorations tagged (`synth_balcony`, `synth_mezz`, `synth_prop`, …) for audit/removal.

**Next wiring step:** port `furnish_showcase` / balcony / mezz from `synth_interior.py` into `synth_transition.furnish_synth_interior` once dressing vignettes are signed off.
