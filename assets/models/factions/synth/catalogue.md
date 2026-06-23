# Synth faction — GLB catalogue (dressing / interior)

Self-contained copy of the Kenney **space_station** kit under `assets/models/factions/synth/`.
Scale **4** in procgen and in the **Dressing** editor mode (`kit = factions/synth`).

This file is the **flat inventory** for the dressing sandbox (editor **Mode: Dressing**).
Per-stem placement rules, clusters, and room roles will be derived from user-authored
vignettes saved under `userinput/synth_dressing/`.

## Placement classes (procgen will use these later)

| Class | Meaning |
|-------|---------|
| **structure** | Build the room shell (floor @ y=0 block top 1.2 m, walls @ y=1.2 m) |
| **wall_swap** | Drop-in replacement for `wall` (same footprint, still solid) |
| **wall_decal** | Mount on inner wall face — **not** a floor prop |
| **floor_prop** | Sits on the 1.2 m deck |
| **balcony** | Perimeter open edge + rail (gated — far side must be void unless deliberate) |
| **skip** | Not for interior dressing |

---

## Structure — room shell

| Stem | Class |
|------|-------|
| `floor` | structure |
| `floor-corner` | structure |
| `floor-detail` | structure |
| `floor-panel` | structure |
| `floor-panel-corner` | structure |
| `floor-panel-end` | structure |
| `floor-panel-straight` | structure |
| `wall` | structure |
| `wall-corner` | structure |
| `wall-corner-round` | structure |
| `wall-door` | structure |
| `wall-door-banner` | structure |
| `wall-door-center` | structure |
| `wall-door-edge` | structure |
| `wall-door-edge-banner` | structure |
| `wall-door-wide` | structure |
| `wall-door-wide-banner` | structure |

---

## Wall swaps (drop-in on `wall`)

| Stem | Class |
|------|-------|
| `wall-banner` | wall_swap |
| `wall-pillar` | wall_swap |
| `wall-pillar-banner` | wall_swap |
| `wall-corner-banner` | wall_swap |
| `wall-corner-round-banner` | wall_swap |
| `wall-window` | wall_swap |
| `wall-window-banner` | wall_swap |
| `wall-window-frame` | wall_swap |
| `wall-window-shutters` | wall_swap |

**Void-facing windows:** OK when paired with `wall-window-shutters` (and optionally a black backing plane outside the shutters so gaps do not show the skybox).

**Structure walls:** on **4 m snap**, click the floor cell the wall belongs to and rotate (**R**) so it faces into the room — the editor shifts the piece **2 m** onto the cell edge (same rule as procgen). Use **2 m / 1 m / 0.5 m** snap to nudge manually.

---

## Wall decals (attach to wall — not floor scatter)

Mount on the **room-facing** side of a wall cell: offset = half wall thickness (0.6 m) + half decal depth along the piece yaw (+Z local faces into room). The dressing editor applies this automatically for the stems below.

**Facing:** props use Kenney **+Z = front** (same for chairs). **F** while placing rotates toward the cursor. Mouse back/forward still steps 90°.

**Beds:** snap marks the **pillow / head edge** on the wall line; only `bed-single` / `bed-double` use the back anchor (+2 m into the room). **`bed-*-cover`** stacks on the same origin as its bed — place the mattress first, then the cover on top (same x/z/yaw).

**Catalog:** stem bounds, snap rules, and front axis — `assets/models/factions/synth/placement_catalog.json` (regenerate: `python tools/probe_synth_catalog.py`).

**Balcony:** on **4 m snap**, click the **interior floor cell** at the edge; rotate so the **raised lip** (+z local) faces the drop — the editor shifts the tile **4 m outward** (replacing the wall line). Rails use deck height (1.2 m); floor tiles at 0.6 m origin (slab top flush with deck).

**Balcony orientation (probed — NOT symmetric visually):**

| Stem | Lip / facing |
|------|----------------|
| `balcony-floor-center` | Raised lip on **+z**; yaw = lip points outward (n=180°, s=0°, e=90°, w=270°) |
| `balcony-floor-corner` | Lip wraps **−x** and **+z**; yaw rotates lip-corner to outward diagonal |
| `rail` | 4 m barrier along local **x**; symmetric — placement from union boundary |
| `rail-narrow` | 2 m barrier; use on short outer edges / stubs |

Procgen/dressing generator: `tools/synth_interior.py` (`expected_balcony_floors`, `expected_balcony_rails`). Test map: `userinput/synth_dressing/balcony_test.json`.

| Stem | Class |
|------|-------|
| `display-wall` | wall_decal |
| `display-wall-wide` | wall_decal |
| `wall-detail` | wall_decal |
| `wall-switch` | wall_decal |

---

## Floor furniture

| Stem | Class |
|------|-------|
| `bed-single` | floor_prop |
| `bed-single-cover` | floor_prop |
| `bed-double` | floor_prop |
| `bed-double-cover` | floor_prop |
| `chair` | floor_prop |
| `chair-armrest` | floor_prop |
| `chair-armrest-headrest` | floor_prop |
| `chair-cushion` | floor_prop |
| `chair-cushion-headrest` | floor_prop |
| `chair-headrest` | floor_prop |
| `computer` | floor_prop |
| `computer-screen` | floor_prop |
| `computer-system` | floor_prop |
| `computer-wide` | floor_prop |
| `container` | floor_prop |
| `container-flat` | floor_prop |
| `container-flat-open` | floor_prop |
| `container-tall` | floor_prop |
| `container-wide` | floor_prop |
| `table` | floor_prop |
| `table-display` | floor_prop |
| `table-display-planet` | floor_prop |
| `table-display-small` | floor_prop |
| `table-inset` | floor_prop |
| `table-inset-small` | floor_prop |
| `table-large` | floor_prop |

---

## Balcony / rail

| Stem | Class |
|------|-------|
| `balcony-floor` | balcony |
| `balcony-floor-center` | balcony |
| `balcony-floor-corner` | balcony |
| `balcony-rail` | balcony |
| `balcony-rail-center` | balcony |
| `balcony-rail-corner` | balcony |
| `rail` | balcony |
| `rail-narrow` | balcony |

---

## Vertical / mezzanine (deferred in procgen)

| Stem | Class |
|------|-------|
| `structure` | skip |
| `structure-panel` | skip |
| `structure-barrier` | skip |
| `structure-barrier-high` | skip |
| `stairs` | skip |
| `stairs-corner` | skip |
| `stairs-corner-inner` | skip |
| `stairs-handrail` | skip |
| `stairs-handrail-single` | skip |
| `stairs-ramp` | skip |
| `stairs-small-center` | skip |
| `stairs-small-corner` | skip |
| `stairs-small-corner-inner` | skip |
| `stairs-small-corner-inner-r` | skip |
| `stairs-small-corner-r` | skip |
| `stairs-small-edge` | skip |
| `stairs-small-edge-handrail` | skip |
| `stairs-small-edge-r` | skip |
| `stairs-small-edges` | skip |
| `stairs-small-edges-handrail` | skip |

---

## Not for dressing sandbox

| Stem | Class |
|------|-------|
| `door-single` | skip |
| `door-single-closed` | skip |
| `door-single-half` | skip |
| `door-double` | skip |
| `door-double-closed` | skip |
| `door-double-half` | skip |
| `pipe` | skip |
| `pipe-bend` | skip |
| `pipe-bend-diagonal` | skip |
| `pipe-end` | skip |
| `pipe-end-colored` | skip |
| `pipe-ring` | skip |
| `pipe-ring-colored` | skip |
| `rocks` | skip |
| `skip` | skip |
| `skip-rocks` | skip |

---

## Dressing workflow (editor)

1. Run **`dressing.bat`** (or toolbar **Mode → Dressing** in a build started with `--dressing`).
2. Paint floor cells on the 1.2 m deck (Actions → Add/Remove floor). Ground `floor` tiles use a tiled scratch detail map (`Textures/floor_detail.png`).
3. Place walls, then decor from the sidebar (synth kit only, scale 4).
4. **File → Save** → `userinput/synth_dressing/<name>.json` (`vignette` field set from document name).
5. Name vignettes clearly (`bunk`, `lab`, `mess`, …) — see `userinput/synth_dressing/README.md`.

Machine-readable stem list: `shared::editor_catalog::synth_dressing_stems()`.
Placement catalog: `assets/models/factions/synth/placement_catalog.json` — regenerate after GLB changes: `python tools/probe_synth_catalog.py`. Rust: `shared::synth_placement`.

Docs: [docs/synth-master-plan.md](../../../docs/synth-master-plan.md), [docs/handover-synth-2026-06-23.md](../../../docs/handover-synth-2026-06-23.md) §7.
