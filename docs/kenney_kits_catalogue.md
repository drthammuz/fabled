# Kenney asset kits — master catalogue

**Status:** Inventory of every Kenney kit prepared for Bevy use.
**Purpose:** Decide which kits become **faction building systems** (see `docs/procgen-faction-manifest.md` §3) and which are pure dressing/props.
**Scope:** What we own and where it lives — *not* per-piece mesh measurements (those are generated later, per kit, when a kit is committed to procgen; see "Per-kit measured catalogue" below).

**Machine-readable index:** `assets/models/kenney_kits_index.json` (every kit, dest folder, texture scheme, all stems grouped).
Regenerate after dropping new kit zips in `assets/downloads/`:

```bash
python tools/extract_kenney_kits.py
```

---

## 0. The existing kit (already wired into procgen)

| Kit | Folder | Pieces | Notes |
|---|---|---|---|
| **Modular Space Kit** | `assets/models/space/` | 42 GLB | The ONLY kit currently used by gen_maps / editor / test showcase. Has a full mesh-measured catalogue (`assets/models/space/kenney_catalog.json`) and docs (`docs/kenney_space_kit.md`). 4 m grid, 4.25 m ceiling. |

Everything below is **newly prepared** by `tools/extract_kenney_kits.py` (2026-06-20) and is *not yet wired into any generator*.

---

## 1. How each kit is prepared

Kenney GLBs reference textures one of two ways. The extractor handles both:

- **External** — GLB references `Textures/colormap.png` (or named PNGs) *relative to the GLB*. The extractor copies the required PNG(s) into a sibling `Textures/` folder. **If that folder is missing the whole GLB fails to load in Bevy** (not just untextured — see space-kit gotcha in MEMORY).
- **Embedded** — GLTF-format kits bake texture/vertex-colour into the GLB. Self-contained; no `Textures/` folder.

| Folder | Kit | GLB | Texture scheme |
|---|---|---|---|
| `building/` | building-kit | 79 | external `colormap.png` |
| `factory/` | factory-kit 3.0 | 143 | external `colormap.png` |
| `furniture/` | furniture-kit | 140 | **embedded** |
| `dungeon/` | modular-dungeon-kit | 39 | external `colormap.png` |
| `prototype/` | prototype-kit | 145 | external `colormap.png` |
| `retro_fantasy/` | retro-fantasy-kit | 105 | external (10 named PNGs) |
| `space_kit/` | space-kit (original) | 153 | **embedded** |
| `space_station/` | space-station-kit | 97 | external `colormap.png` |

**Texture-only packs** (no models): `assets/textures/dev_essentials/` (15 prototype/dev PNGs), `assets/textures/retro_fantasy/` (117 fantasy surface PNGs).

**OBJ-only, staged, NOT game-ready:** `assets/staging/road_tiles/` (302 OBJ from 3d-road-tiles). Bevy has no built-in OBJ loader — needs OBJ→GLB conversion (Blender headless) before use.

> ⚠️ Naming: `space/` = **modular**-space-kit (the wired one). `space_kit/` = the **original** Kenney space kit (mostly exterior props/vehicles). Different kits.

---

## 2. Architecture systems vs dressing

Split by what the kit *is*, for faction planning:

### Modular ARCHITECTURE systems (walls + floors + corridors/rooms — a level can be generated from these)

| Folder | Read / vibe | Modular grammar | Faction candidate |
|---|---|---|---|
| `space/` *(existing)* | Clean utilitarian sci-fi | corridor/room/template/stairs/gate, 4 m grid | baseline Kenney "civilized" |
| **`dungeon/`** | Stone keep / catacomb | **Same stems as `space/`** (corridor, room, template-*, stairs, gate) — drop-in re-skin | **Priesthood / medieval** interior |
| `space_station/` | Sleek polished station | wall, floor, floor-panel, stairs (13!), door, balcony, structure | **Synth / high-tech** faction |
| `building/` | Urban brick block | wall (22), floor, roof, stairs, door, column, plating | Generic settlement / outlaw hab |
| `retro_fantasy/` | Medieval village + fort | wall (39), tower, battlement, roof, floor-stairs, dock | **Priesthood / fortress** exterior |
| `prototype/` | Greybox primitives | wall (25), floor, stairs, shape-*, column, door | Blockout only — not final art |

### Dressing / props (decorate a level; don't define its layout)

| Folder | Contents | Use |
|---|---|---|
| `factory/` | conveyor (38), pipe (18), structure (15), machine, catwalk, cog, hopper, crane, screen | **Industrial substrate** dressing (sewer/vent/rail rooms) + some `structure-*` walls |
| `furniture/` | beds, kitchen, lounge, bathroom, desks, lamps, rugs, plants, TVs | Interior set-dressing for any inhabited room |
| `space_kit/` | rockets, rovers, astronauts, monorail train+track, pipes, platforms, terrain, turrets, craft, satellite dishes | Exterior/surface scenes, **monorail/rail**, terrain ramps, sci-fi props |

---

## 3. Per-kit inventory

Grouped by leading name token. Counts in parens. Full stem lists in `kenney_kits_index.json`.

### `dungeon/` — Modular Dungeon Kit (39, stone) — *priesthood interior candidate*
Mirrors the existing `space/` modular system exactly, so it can reuse `gen_maps.py` + `kenney_catalog.json` tooling with a re-measure.
- **corridor** (11): corridor, -corner, -end, -intersection, -junction, -transition, -wide, -wide-corner, …
- **room** (7): room-small, -small-variation, -corner, -large, -large-variation, -wide, -wide-variation
- **template** (15): template-floor*, template-wall*, template-corner, template-detail (wall/floor/trim tiles)
- **gate** (4): gate, gate-door, gate-door-window, **gate-metal-bars** *(new vs space kit)*
- **stairs** (2): stairs, stairs-wide

### `space_station/` — Space Station Kit (97, clean) — *synth candidate*
- **wall** (21): wall, -banner, -corner, -corner-round, -detail, -door, -window, …
- **stairs** (13): stairs, -corner, -corner-inner, -handrail, -ramp, -small-*, …
- **floor** (7): floor, -corner, -detail, -panel, -panel-corner, -panel-end, -panel-straight
- **door** (6): door-single/-double (+ closed/half variants)
- **structure** (4): structure, -barrier, -barrier-high, -panel
- **pipe** (7), **balcony** (6), **rail** (2) — vertical/junction dressing
- Props: table (7), chair (6), bed (4), computer (4), container (5), display (2)

### `building/` — Building Kit (79, urban brick)
- **wall** (22): wall, -corner, -corner-column*, -corner-diagonal, -corner-round, …
- **border** (10), **roof** (7, flat), **floor** (5), **stairs** (8), **door** (8, rotating)
- **barricade** (6: doorway/window), **gutter** (5), **plating** (4), **column** (3), detail-pipe

### `retro_fantasy/` — Retro Fantasy Kit (105, medieval) — *priesthood/fort exterior*
- **wall** (39): wall, -detail, -door, -gate, -fortified*, … (the bulk — fortress walls)
- **tower** (6), **battlement** (4), **roof** (9), **floor**/floor-stairs (8), **structure** (6)
- **column** (5), **overhang** (4), **wood** floors (4), **fence** (3), **stairs** (3), **dock** (2)
- Props: barrels, bricks, ladder, water, tree (2), detail-crate*, pulley (2)

### `factory/` — Factory Kit (143, industrial) — *industrial substrate dressing*
- **conveyor** (38): the dominant set — belts, bars, slopes, fences, sides
- **pipe** (18): pipe-glass-large* + bends/cross/junction/curve
- **structure** (15): corner-inner/outer, doorway, doorway-wide, high/medium/short/tall *(usable as walls)*
- **machine** (7), **catwalk** (6), **screen** (8), **cog** (5), **hopper** (4), **piston** (4)
- Misc: crane (3), robot-arm (2), scanner (2), box (4), button (4), lever (2), warning (2), cone, oopi

### `prototype/` — Prototype Kit (145, greybox) — *blockout only*
- **wall** (25), **shape** (18: cube/cylinder/hexagon primitives), **floor** (8), **stairs** (8)
- **indicator** (17), **number** (20), **column** (6), **door** (6), **pipe** (6), **ladder** (3)
- Placeholder content: animal (3), vehicle (2), weapon (2), hat (2), figurine (4), crate, coin, flag

### `furniture/` — Furniture Kit (140, interior props)
Kitchen (cabinets/fridge/stove/sink/microwave), lounge (sofas/chairs/tables), bedroom (beds/cabinets), bathroom (sink/toilet/shower/tub), office (desk/computer/laptop), lamps, rugs, plants, TVs, washer/dryer, walls/floors/stairs/doorways (a few interior architecture bits too).

### `space_kit/` — Space Kit original (153, exterior sci-fi props)
- **terrain** (16): tiles, ramps, road pieces, cliffs — ground for exterior scenes
- **monorail** (13): track straight/corner/slope/support + train cars (box/cargo/passenger/front/end)
- **pipe** (20), **platform** (16), **rocket** (12 parts), **corridor** (15 modular interior), **rock/meteor/crater** (terrain scatter)
- **craft** (7 ships), **hangar** (8), **machine** (6), **rail** (4), **satelliteDish** (3), **turret** (2), astronauts (2), alien, rover, weapons (2)

---

## 4. Faction mapping (committed — feeds manifest §5.5)

Three faction architecture systems + the default substrate. Working names; final faction identities TBD. Each faction draws props from **its own** kit — props are **not** a shared pool (modern `furniture/` does NOT fit the stone faction).

| Faction (working name) | Interior architecture | Exterior architecture | Prop / decoration set | Indoor / outdoor |
|---|---|---|---|---|
| **Default industrial** | procedural sewer/vent/rail (`level.rs`) + `space/` modular | — | `factory/` (pipes, conveyors, catwalks, machines) + `space_kit/` (pipes, platforms, **monorail** track + train cars) | indoor (substrate) |
| **Priesthood / fantasy** | `dungeon/` (stone — same grammar as `space/`) | `retro_fantasy/` (towers, battlements, walls, docks) | `retro_fantasy/` props (barrels, crates, ladders, bricks, fences) | **both** |
| **Synth** | `space_station/` (sleek, banner walls) | — (none yet) | `furniture/` (modern domestic) + station props/computers/displays | indoor only |
| **Outlaw / settler** | `building/` (urban brick, roofs, barricades) | `building/` facades/roofs | `furniture/` or `factory/` salvage | semi-exterior, no landscape |

**Not factions:** `prototype/` = greybox blockout tool. `space_kit/` beyond monorail = exterior/surface set-pieces (hangars, terrain ramps, turrets, satellite dishes, craft). Texture packs = utility skins.

**Default-area materials already in place:** aged metal PBR (ambientCG MetalPlates013 + packed ORM) on walls, murky `bevy_water` sewer water. No dedicated *concrete* material yet; geometry is still procedural cuboids → the `factory/` + `space_kit/` GLBs are the "freshening up" pass.

**Gaps:** only Priesthood has a real outdoor/exterior arch set; Synth has no exterior. No generator places exterior GLBs yet — **no outdoor level generation exists** (see manifest §2.1 / Phase 4).

### 4.1 Faction procgen profiles (`userinput/factions/*.json`)

Machine-readable presets consumed by `tools/faction_profiles.py` and the editor Proc tab (`--faction-profile`). Each JSON file names the **architecture kit folder** under `assets/models/`, a **prop/dressing kit**, generation knob defaults, and the **hidden-room door** piece.

| Profile ID | Architecture kit (`building_system`) | Tile GLB path | Hidden door GLB | Door tint (sRGB) | Props kit | Notes |
|---|---|---|---|---|---|---|
| `space_default` | `space/` | `models/space/{stem}.glb` | `models/space/gate-door.glb` | cyan `(0.45, 0.82, 1.0)` | `factory/` | **Wired** — default procgen tiles + hidden doors |
| `priesthood` | `dungeon/` | `models/dungeon/{stem}.glb` | `models/dungeon/gate-door.glb` | amber `(0.95, 0.72, 0.35)` | `retro_fantasy/` | **Wired** — full modular tile emission + hidden doors |
| `synth` | `space_station/` | `models/space_station/{stem}.glb` | `models/space/gate-door.glb` *(fallback)* | teal `(0.55, 0.95, 0.88)` | `furniture/` | Station kit has no `gate-door`; hidden doors reuse space gate |
| `outlaw` | `building/` | `models/building/{stem}.glb` | `models/space/gate-door.glb` *(fallback)* | rust `(0.92, 0.55, 0.42)` | `factory/` salvage | Urban brick architecture; door fallback until `building/door-*` anim wired |
| `industrial_default` | `space/` + procedural sewer | `models/space/{stem}.glb` | `models/space/gate-door.glb` | grey `(0.65, 0.70, 0.75)` | `factory/` + `space_kit/` | Substrate = `level.rs` procgen; Kenney tiles optional overlay |

**Hidden door tag:** generated pieces carry `"tags": ["hidden_entrance"]`, `"kit"`, and `"tint"`. Runtime: tinted mesh (client), proximity open animation (client), closed-state seal collider (server — drops when player is near).

**List / inspect profiles:**

```bash
python tools/faction_profiles.py list
python tools/faction_profiles.py show priesthood
```

**Modular grammar kits** (same corridor/template/room stems, swap folder): `space/`, `dungeon/`. **Different grammar** (needs separate catalogue work): `space_station/`, `building/`, `retro_fantasy/`.

---

## 5. Per-kit measured catalogue (when wiring a kit into procgen)

The inventory above lists *what exists*. To actually generate levels from a kit (footprints, open faces, slot positions, cell grids) it needs a **mesh-measured** catalogue like the space kit's `kenney_catalog.json`, produced by `tools/generate_kenney_catalog.py`.

`generate_kenney_catalog.py` now takes a `--kit` arg (default `space`):

```bash
python tools/generate_kenney_catalog.py --kit dungeon   # -> assets/models/dungeon/kenney_catalog.json
```

**Done so far:**
- `space/` — `assets/models/space/kenney_catalog.json` (wired into gen_freeform / runtime).
- `dungeon/` ✅ — `assets/models/dungeon/kenney_catalog.json` (39 pieces, cell grids verified identical to the space grammar: room-small = 3×3 floor + C-slot openings, etc.). Generated but **not yet wired** into a generator/runtime — the Rust `shared::kenney_catalog` loader still hard-loads `space/`. Per-kit/per-faction loading is the next wiring step (manifest Phase 2).

For a non-modular kit (building/space_station/retro_fantasy) the 4 m-grid assumptions in the script (GRID_UNIT, slot positions, the MANUAL stem metadata) would need per-kit review first.

---

## 6. References
- Faction/procgen plan: `docs/procgen-faction-manifest.md`
- Existing space kit detail: `docs/kenney_space_kit.md`
- Extractor: `tools/extract_kenney_kits.py`
- Machine index: `assets/models/kenney_kits_index.json`

---

*Created 2026-06-20 — inventory of 8 new model kits (901 GLB) + 2 texture packs + 1 OBJ kit (staged). No kit beyond `space/` is wired into procgen yet.*
