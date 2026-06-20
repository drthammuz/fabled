# Kenney Modular Space Kit — Piece Catalogue

All measurements in metres.  
**Model origin** = floor surface (Y = 0).  
Floor slab extends **below** Y = 0 (don't count it as clearance).  
Ceiling top = **Y = 4.25** for all room/corridor pieces.  
**Grid unit = 4 m.**  All footprints are multiples of 4 m.

**Machine-readable catalogue:** `assets/models/space/kenney_catalog.json` (all 41 GLBs).  
Regenerate after adding GLBs: `python tools/generate_kenney_catalog.py`  
Rust API: `shared::kenney_catalog::piece("stairs")`.

---

## Catalogue schema (placement variables)

| Field | Type | Meaning |
|---|---|---|
| `stem` | string | GLB name without extension |
| `category` | enum | `corridor`, `corridor_wide`, `room`, `stairs`, `gate`, `template_wall`, `template_floor`, `template`, `prop` |
| `role` | enum | `connector`, `module`, `vertical`, `door`, `wall_tile`, `floor_tile`, `decoration`, … |
| `footprint_m` | `{x, z}` | Logical tile size for grid snapping |
| `grid_units` | `{x, z}` | Footprint ÷ 4 m |
| `bounds` | AABB | Measured mesh bounds (local space, yaw = 0) |
| `open_faces` | `south\|north\|east\|west`[] | Which faces have 4 m openings at yaw = 0 |
| `open_slots` | `L\|C\|R`[] | Room wall slot (2 m / 6 m / 10 m from edge on a 12 m wall) |
| `stairs` | object | `entry_z`, `landing_z`, `rise_m`, `width_m` — offsets from model origin |
| `cell_grid.cells` | 2D `[south→north][west→east]` — `floor`, `wall`, `void`, `stairs`, `door`, `prop`, `hole` |
| `cell_grid.edges` | Outer perimeter per unit: `open` or `wall` (checked at y≈1.8–3.2, not ceiling) |
| `cell_grid.confidence` | `mesh`, `mesh_variant`, or `review` |

Regenerate (re-measures GLBs with vertical rays at **y≈0** for floor):

```powershell
python tools/generate_kenney_catalog.py
```

---

## Master index (41 pieces)

| Category | Count | Stems |
|---|---|---|
| **corridor** (4×4 m) | 6 | `corridor`, `corridor-corner`, `corridor-end`, `corridor-intersection`, `corridor-junction`, `corridor-transition` |
| **corridor_wide** (8×8 m) | 5 | `corridor-wide`, `corridor-wide-corner`, `corridor-wide-end`, `corridor-wide-intersection`, `corridor-wide-junction` |
| **room** | 7 | `room-small`, `room-small-variation`, `room-corner`, `room-large`, `room-large-variation`, `room-wide`, `room-wide-variation` |
| **stairs** | 2 | `stairs` (4 m), `stairs-wide` (8 m) |
| **gate** | 5 | `gate`, `gate-door`, `gate-door-window`, `gate-lasers`, `gate-lasers-edited` |
| **template_wall** | 6 | `template-wall`, `template-wall-half`, `template-wall-corner`, `template-wall-detail-a`, `template-wall-stairs`, `template-wall-top` |
| **template_floor** | 7 | `template-floor`, `template-floor-big`, `template-floor-detail`, `template-floor-detail-a`, `template-floor-layer`, `template-floor-layer-raised`, `template-floor-layer-hole` |
| **template** (trim) | 2 | `template-corner`, `template-detail` |
| **prop** | 1 | `cables` |

### Quick reference — footprints (logical)

| Footprint | Pieces |
|---|---|
| 4 × 4 m | Standard corridors, `template-corner`, most `template-floor*` |
| 8 × 8 m | All `corridor-wide*`, `corridor-transition`, `template-floor-big`, `stairs-wide` |
| 4 × 8.2 m | `stairs` |
| 12 × 12 m | `room-small*`, `room-corner` |
| 20 × 12 m | `room-wide*` |
| 20 × 20 m | `room-large*` |
| 4 × 1 m (wall depth) | `template-wall`, `template-wall-half`, … |

> Full per-piece bounds, openings, and `purpose` strings: see `kenney_catalog.json`.

---

## Grid unit and slot system

```
One Kenney unit = 4 m.

A 12 m wall (3 units) has 3 natural connector positions:
  Slot L  →  2 m from left edge   (centre of unit 0–4 m)
  Slot C  →  6 m from left edge   (centre of unit 4–8 m)
  Slot R  → 10 m from left edge   (centre of unit 8–12 m)

Opening size that matches all Kenney pieces = 4 m wide × 4.25 m tall.
```

---

## 1. `corridor.glb`

| Property | Value |
|---|---|
| Footprint (X × Z) | 4 m × 4 m |
| Height (floor → ceiling) | 4.25 m |
| Mesh Y range | −2.069 → 4.25 |
| **Openings** | **South face (Z = −2)** and **North face (Z = +2)** — both 4 m wide × 4.25 m tall (full wall is open) |

**Purpose:** Straight one-unit corridor segment connecting two rooms or modules.

**Connections:**
- Both Z faces are fully open → connects to any piece with a matching 4 m wide opening.
- Place `+Z` face flush against a wall opening, OR chain multiple corridors.

**Rotation (yaw around Y):**
| Result | Yaw |
|---|---|
| N–S passage (default) | 0° |
| E–W passage | 90° (π/2) |

**Tiling:** 2 corridors end-to-end = 8 m straight run.  
3 corridors side-by-side across a 12 m wall = fills a full module width.

---

## 2. `corridor-corner.glb`

| Property | Value |
|---|---|
| Footprint (X × Z) | 4 m × 4 m |
| Height | 4.25 m |
| Mesh Y range | −1.707 → 4.25 |
| **Openings** | **South face (Z = −2)** and **East face (X = +2)** — both 4 m wide × 4.25 m tall |

**Purpose:** 90° L-shaped turn between two perpendicular corridors.

**Connections:**
- South Z = −2 face ↔ anything coming from the south.
- East X = +2 face ↔ anything coming from the east.

**Rotation (yaw) to create each corner type:**
| Corner direction | Yaw | Open faces after rotation |
|---|---|---|
| S → E (default) | 0° | Z = −2 (S) and X = +2 (E) |
| E → N | 90° | X = −2 (W rotated to S) … | 
| N → W | 180° (π) | Z = +2 (N) and X = −2 (W) |
| W → S | 270° (3π/2) | |

> Tip: rotate in 90° increments to reach all four L-turn orientations.

**Fitting with room-small:**  
Place at the corner of a 12 m room-small so the corridor-corner's opening aligns with the room's C-slot wall opening. Needs a short 4 m corridor bridge between the corner-piece and the room opening if positions don't align exactly.

---

## 3. `room-small.glb`

| Property | Value |
|---|---|
| Footprint (X × Z) | 12 m × 12 m (3 × 3 Kenney units) |
| Height | 4.25 m |
| Mesh Y range | −1.707 → 4.25 |
| **Openings** | **All 4 walls**, each with **one centred 4 m opening** at the mid-wall position (C-slot: 6 m from edge) |

**Purpose:** Self-contained square room. The fundamental large module piece.  
One `room-small` fills one 12 m × 12 m fabled module exactly.

**Connections (all 4 walls at C-slot, 6 m from each edge):**
| Face | Opening centre (local) |
|---|---|
| South (Z = −6) | X = 0 |
| North (Z = +6) | X = 0 |
| West (X = −6) | Z = 0 |
| East (X = +6) | Z = 0 |

Connect a `corridor` or `corridor-corner` to any of these openings.  
The room-small's opening is at the **C slot** of a 12 m module wall, never L or R.

**Rotation:** All 4 rotations are equivalent (symmetric room).

---

## 4. `stairs.glb`

| Property | Value |
|---|---|
| Footprint (X × Z) | 4 m × 8.2 m |
| Full mesh Y range | −2.069 → 8.75 (decorative side walls) |
| **Floor-to-landing height** | **≈ 4.35 m** |
| Entry face (Z local) | Z = −6.1 (bottom step, opening 4 m wide) |
| Landing face (Z local) | Z = +2.1 (top step / landing, Y ≈ 4.35 m) |
| Width | 4 m (X: −2 → +2) |

**Purpose:** Connects floor 0 to floor 1. Takes up one 4 m wide corridor slot.

**Key dimensions for placement:**
- The **decorative side walls** rise to 8.75 m — this is NOT the landing height.
- The **actual step landing** is at Y ≈ 4.35 m when bottom step is at Y = 0.
- Total depth from entry arch to landing = 8.2 m (6.1 + 2.1).

**Placement formula (yaw = 0, entry faces −Z):**

```
world_entry_z = model_origin_z - 6.1   (bottom step, ground level)
world_landing_z = model_origin_z + 2.1  (landing, upper floor)
```

**Ceiling clearance:** The stairwell needs the floor-1 slab to have a gap above the stair run.  
Suggested slab cutout: 4 m wide × 7 m deep (from entry to just past the landing).

**Connecting floor 1:** Place a `corridor` at world Z = landing_z, Y = 4.35 + 0.002.

---

## 5. `gate-door.glb`

| Property | Value |
|---|---|
| Footprint (X × Z) | 4.2 m × 1.4 m |
| Mesh Y range | −9.041 → 4.621 |
| Visible door height | 4.621 m (slightly above 4.25 m ceiling — frame overlaps) |
| Width of opening | ≈ 4 m (0.1 m frame on each side) |
| Depth | 1.4 m total (0.7 m each side of Z = 0) |

**Purpose:** A door/gate frame placed in a wall opening. The large downward Y range (−9 m) is the hidden door panel that slides underground when the door opens.

**Connections:**
- Centred on a wall opening (4 m wide).
- Both Z faces (+0.7 and −0.7) are open — place it exactly at the wall plane so it straddles the wall.
- Pairs with `gate-lasers` for a closed energy barrier.

**Placement:** World position = centre of the wall opening, Y = 0 (floor).  
Works at any wall; rotate yaw by 90° to place on E/W walls.

---

## 6. `gate-lasers.glb`

| Property | Value |
|---|---|
| Bar span (X) | ±1.125 m → total 2.25 m |
| Bar heights (Y) | 0.2, 0.8, 1.4, 2.0, 2.6, 3.2 m (6 bars) |
| Laser beams (Y offset) | +0.329 m above model origin |
| Depth (Z) | ≈ 0.4 m |

**Purpose:** Energy-barrier overlay for a `gate-door`. Six horizontal laser bars span the opening.

**Placement:** At the same world position as its `gate-door`, Y = 0 (floor).  
The bars sit inside the gate frame (2.25 m span vs 4 m opening — centred by default).

> **Node structure:** 12 bar-bracket nodes (Group, 6 pairs at X = ±1.125) + 1 laser-beams mesh.  
> For animation (gate open): hide/despawn the 6 bar pairs + laser-beams node.

---

## 7. `cables.glb`

| Property | Value |
|---|---|
| Size (X × Z) | 1.915 m × 2.102 m |
| Height | 0.16 m |
| Y range | 0 → 0.16 |

**Purpose:** Flat cable-bundle decoration. Cosmetic only — no collision needed.  
Place on floors or ceiling (Y = 0 for floor; Y = 4.25 − 0.16 for ceiling mount).

---

## 8. Extended corridors (`corridor-end`, `corridor-junction`, `corridor-intersection`, `corridor-transition`)

Same 4.25 m ceiling as `corridor.glb`. All are 4 m lane pieces except `corridor-transition` (8×8 m adapter to wide corridors).

| Stem | Footprint | Open faces (yaw = 0) | Purpose |
|---|---|---|---|
| `corridor-end` | 4×4 | north | Dead-end cap |
| `corridor-junction` | 4×4 | south, north, east | T-junction |
| `corridor-intersection` | 4×4 | all 4 | 4-way cross |
| `corridor-transition` | 8×8 | south, north, east | 4 m ↔ 8 m lane adapter |

---

## 9. Wide corridors (`corridor-wide*`)

8 m wide versions of the standard corridor set. Footprint **8 × 8 m** each.

| Stem | Open faces (yaw = 0) |
|---|---|
| `corridor-wide` | south, north |
| `corridor-wide-corner` | south, east |
| `corridor-wide-end` | north |
| `corridor-wide-junction` | south, north, east |
| `corridor-wide-intersection` | all 4 |

---

## 10. Extended rooms

| Stem | Footprint | Notes |
|---|---|---|
| `room-corner` | 12×12 | L-shaped room; openings south + east |
| `room-large` | 20×20 | 5×5 grid units; C-slot openings all walls |
| `room-large-variation` | 20×20 | Cosmetic variant of `room-large` |
| `room-wide` | 20×12 | Rectangular hall |
| `room-wide-variation` | 20×12 | Cosmetic variant of `room-wide` |
| `room-small-variation` | 12×12 | Cosmetic variant of `room-small` |

---

## 11. `stairs-wide.glb`

| Property | Value |
|---|---|
| Footprint | 8 m × 8.2 m |
| Width | 8 m (X: −4 → +4) |
| Entry / landing Z | Same as `stairs` (−6.1 / +2.1) |
| Rise | ≈ 4.35 m |

Use when a stairwell spans two 4 m lanes (wide corridor slot).

---

## 12. Gates (`gate.glb`, `gate-door-window.glb`)

| Stem | Notes |
|---|---|
| `gate` | Frame only (no sliding panel) |
| `gate-door-window` | `gate-door` with window insert |

Same 4.2 × 1.4 m footprint and wall-straddling placement as `gate-door`.

---

## 13. Template building blocks

Authored **wall/floor/trim tiles** for filling slots the prefab rooms don't cover (L/R positions, partial walls, mezzanine layers).

### Walls (`template-wall*`)

| Stem | Logical size | Role |
|---|---|---|
| `template-wall` | 4 × 1 m | Full-height wall panel |
| `template-wall-half` | 2 × 1 m | Half-width panel |
| `template-wall-corner` | 1 × 1 m | Corner filler |
| `template-wall-detail-a` | 4 × 1.37 m | Panel with surface greebles |
| `template-wall-stairs` | 4.2 × 0.78 m | Wall with stair opening |
| `template-wall-top` | 4.2 × 0.78 m | Railing / upper trim |

Place with **back face at −Z** (depth extends south); yaw 90° increments for E/W walls.

### Floors (`template-floor*`)

| Stem | Size | Role |
|---|---|---|
| `template-floor` | 4×4 | Flat floor tile |
| `template-floor-big` | 8×8 | Large floor tile |
| `template-floor-detail` | 4×4 | Shallow surface detail |
| `template-floor-detail-a` | 4×4 | Detail variant A |
| `template-floor-layer` | 4.2×4.2 | Thin layer slab (0.4 m) |
| `template-floor-layer-raised` | 4.2×4.2 | Raised platform (3.4 m) |
| `template-floor-layer-hole` | 4.2×4.2 | Layer with cutout (stairs/shafts) |

### Trim

| Stem | Size | Role |
|---|---|---|
| `template-corner` | 4×4 | Corner column / trim |
| `template-detail` | 1.56×1.56 | Small surface prop |

---

```
corridor (4×4)  ←──4m──→  corridor (4×4)
                ←──4m──→  corridor-corner (4×4) [on Z face]
                ←──4m──→  room-small wall opening (C-slot, 6m from edge)
                ←──4m──→  stairs entry/landing face

corridor-corner: one Z-face + one X-face (both 4m openings)

room-small has openings at C-slot (6m) on ALL 4 walls.
    → Slot L (2m) and R (10m) are NOT natively open — use sewer wall geometry.

gate-door: place at wall opening, straddles wall plane. Pairs with gate-lasers.

stairs: entry and landing are both 4m wide. Ceiling above stairs needs a slab gap.
```

---

## Module grid normalisation

| Module dimension | Kenney units | Fits |
|---|---|---|
| **12 m × 12 m** | 3 × 3 | 1 room-small; OR 3 × 3 corridors |
| 24 m × 24 m | 6 × 6 | 4 room-smalls; OR 6 × 6 corridors |

**Recommended module size = 12 m × 12 m.**  
Slots (L=2 m, C=6 m, R=10 m) all land on natural Kenney 4 m corridor positions.  
Two adjacent 12 m modules share a 12 m wall → the same L/C/R slots align on both sides.

---

## GLB preparation (all 42 models)

Kenney exports reference an **external** texture URI: `Textures/colormap.png` (relative to each `.glb`).  
This is **not** optional — Bevy and Windows 3D Viewer both need that file on disk:

```
assets/models/space/corridor.glb
assets/models/space/Textures/colormap.png   ← required sibling path
```

Only `gate-lasers.glb` / `gate-lasers-edited.glb` embed their texture (self-contained).

**One-time / after adding new GLBs:**

```powershell
powershell -ExecutionPolicy Bypass -File tools/prepare_kenney_glbs.ps1
```

This script:
1. Ensures `Textures/colormap.png` exists (copies from `colormap.png` if needed)
2. Regenerates `cyber_colormap*.png` for in-game material swap
3. Optionally writes `assets/models/space/viewer/*.glb` with embedded textures via `tools/embed_kenney_textures.py` (for Windows 3D Viewer)

**In-game modes (`--host --test`):**

| Flag | Walls | Stairs |
|---|---|---|
| `--test` or `--rusty` | Procedural from `wall_map.json` | Procedural ramp |
| `--test --kenney` | Same procedural walls | Kenney `stairs.glb` from `wall_map.json` `climb` direction |

---

## Full reproduction example: the 3 × 2 test grid

```
Wall connections at module boundary use C-slot (6m) by default → 
one corridor piece centred on the opening aligns perfectly.

For L (2m) or R (10m) slots → sewer procedural wall with opening, 
no Kenney piece needed at the wall itself (use sewer geometry for those).
```
