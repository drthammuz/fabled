# Procedural level generation — multi-faction manifest

**Status:** Living spec (return here before major procgen work)  
**Audience:** implementers, editor tooling, future faction content  
**Scope:** Raw **layout** and **building-system transitions** — not decoration, loot, or narrative scripting.

---

## 0. Agent handoff (read this first if you have no chat context)

**Assigned task:** Implement **Phase 1 only** (§7) — generic Kenney **mission graph** + **role-based tile synthesis**. Do **not** start faction profiles, industrial merge, or camp adjacency until Phase 1 exit criteria are met.

### What the user decided (summary)

1. **Stop using room GLBs** (`room-large`, etc.) for procgen — duplicate floors, limits layout freedom.
2. **Stop using pre-authored module pool** for map placement — synthesize each 5×5 module slot from 1×1 pieces (`template-floor`, `template-wall`, `corridor-*`).
3. **Long-term:** ~4–5 factions, multiple **building systems** (Kenney “people” vs industrial sewer/vent/rail), levels can mix factions with **logical transitions** (see §2–4). That is **later**; Phase 1 is Kenney-only layout quality.
4. **One thing at a time** — user will tune sliders in editor **after** generation feels purposeful; do not front-load all §5.1 parameters.

### Already implemented (do not redo)

| Area | State |
|------|--------|
| Tile synthesis default | `tools/gen_maps.py` — `synthesize_module()` calls `gen_modules.strat_planned(..., no_rooms=True, fixed_exit_cells=…)` |
| Module pool | Legacy only via `--use-pool`; default path needs no `userinput/modules/` |
| End / hub | Tiled rooms via `build_tiled_floor_room` / `build_end_extraction_room` — no `room-large` shells |
| Editor live regen | Sidebar **Proc** tab → `crates/client/src/editor_map_gen.rs` → `python tools/gen_maps.py --preview` |
| Generated map load | `MapDocument::load_generated()` in `crates/shared/src/editor_map.rs` |
| Preview output | `userinput/maps/_editor_preview.json` |

### Still wrong / Phase 1 target

| Problem | Location | Fix direction |
|---------|----------|---------------|
| Graph feels random | `design_high_level()` in `gen_maps.py` | Replace spanning tree + random chords + `target_avg_degree` with **spine spawn→end**, **branches off spine**, **slot roles** (`START`, `SPINE`, `HUB`, `CHAMBER`, `END`) |
| Every slot is “pipe fitting” | `synthesize_module` / `strat_planned` corridor-only | **Role-based:** dead-ends → full tiled chamber (`build_tiled_floor_room`); hubs → route exits to center; spine → narrow through-corridor |
| `target_avg_degree` knob | Proc panel + CLI | Keep temporarily; spine/branch params replace it in Phase 1–2 |

### Phase 1 exit criteria

- In editor **G** playtest, layout reads as **main path + optional side areas** without props.
- `python tools/gen_maps.py --seed N --probe` → mesh borders OK.
- `python tools/probe_layout_decor.py userinput/kenney_layout.json` → no duplicate floor overlaps on floor 0.
- Do **not** merge industrial `level.rs` `gen_grid` in Phase 1.

### Verification commands

```bash
python tools/gen_maps.py --seed 42 --attempts 30 --probe
python tools/probe_layout_decor.py userinput/kenney_layout.json
python tools/probe_map_geometry.py userinput/maps/gen_map_*.json
```

Hub/extraction work is **out of scope** unless a Phase 1 change breaks it — if touching hub, read `docs/hub-extraction-agent-failures.md` and run `python tools/probe_hub_tile_audit.py`.

### Key files

| File | Role |
|------|------|
| `tools/gen_maps.py` | Phase 1 graph + synthesis + CLI |
| `tools/gen_modules.py` | `strat_planned`, corridor/tile placement |
| `tools/probe_map_geometry.py` | Border opening validation |
| `tools/probe_layout_decor.py` | Duplicate floor / gate audit |
| `crates/client/src/editor_map_gen.rs` | Editor Proc panel |
| `docs/procgen-faction-manifest.md` | This document |
| `docs/hub-extraction-agent-failures.md` | Hub only — do not apply to generic procgen |

### Explicit non-goals for the next PR

- Faction profile JSON / camp adjacency / zone planner (Phase 2+)
- Kenney ↔ industrial transition geometry (Phase 4)
- Reintroducing `room-large` or module pool as default
- Global `stair_glue` in character movement (see hub failure log)
- Committing unless the user asks

---

## 1. Why this document exists

Map generation is not one algorithm for one art set. Fabled runs traverse **multiple architectural vocabularies** (Kenney “built environment”, industrial sewer/vent/rail, future faction kits) that must **connect logically** at camp boundaries and **inside** a single level when the run crosses factions.

This manifest defines:

- What a **building system** is vs a **faction**
- How levels **compose** and **transition**
- Which **parameters** each faction profile carries
- **Phased implementation** — generic layout first, faction knobs later, editor tuning last

**Rule for this stage:** one thing at a time. Do not wire all factions or all variables until the generic mission-graph + tile synthesis pipeline is trustworthy.

---

## 2. World model (runs, camps, factions)

| Concept | Meaning |
|--------|---------|
| **Camp / hub** | Safe meta layer between runs. Player chooses routes; camps have a **faction identity**. |
| **Faction** | Culture + gameplay tone + default **building system** + **procgen profile** (see §5). ~4–5 planned. |
| **Run / level** | One playable map from entry to extraction. May use **one or several** building systems. |
| **Stretch / route** | Graph of levels between camps (`shared/run.rs`). **Previous camp faction** and **next camp faction** influence the **current** level’s look and layout bias. |
| **Building system** | Concrete geometry + texture catalogue + piece rules (Kenney modular tiles, sewer arches, rusted rail, etc.). A faction *prefers* one; transitions may borrow another. |

**Camp adjacency drives variety:**

```
previous_camp.faction  ──►  current_level  ──►  next_camp.faction
         │                         │                      │
         └─ entry transition       ├─ interior mix         └─ exit transition
                                   └─ 0–3 factions in one level possible
```

Examples (illustrative, not final lore):

- **Priesthood / aristocratic** → Kenney clean modular halls (people-scale, planned grandeur).
- **Outlaws / scavengers** → rough sewer-metal, repetitive utility tunnels.
- **Military** → efficient grid, short paths, functional rooms.
- **Default industrial** → sewer / ventilation / subway (modular, repetitive, **infrastructure** not **architecture**).

**Default hypothesis (user correction):** industrial utility (sewer/vent/rail) is often the **baseline** substrate; Kenney “civilized” blocks are **grafted** into it and **release** back into industrial at boundaries — but **all combinations** remain valid (all Kenney, all industrial, 50/50, three-way split, etc.).

### 2.1 Ordering within a level (start → middle → end)

A faction's building tradition appears **on both sides of its camp** — you see it as you *leave* the previous camp and again as you *approach* the next one. For a typical level the default ordering along the main path (spawn → extract) is:

```
[ START ]            [ MIDDLE ]                 [ END ]
previous camp's  ──► default industrial   ──►  next camp's
faction buildings    substrate (sewer /         faction buildings
(you are leaving)    vent / subway)             (you are entering)

   exit transition         connective tissue        entry transition
```

- **Start** of the level = openings/exits **out of the previous camp's faction architecture** into the substrate.
- **Middle** = the **default industrial utility** system (sewer / ventilation / subway) as connective tissue between the two faction bookends. Length varies; in an all-one-faction level it can shrink to nothing.
- **End** = transition **into the next camp's faction architecture** before extraction.

This is the *default* shape, not a hard rule — §4.2 variety rules still allow all-one-system levels (no industrial middle), 50/50 blends, or three-way splits. But absent those overrides, **previous faction bookends the start, next faction bookends the end, industrial fills the gap.**

**Worked example — priesthood camp → synth camp:**

1. Level opens with the player **exiting priesthood-tradition buildings** (e.g. wide ceremonial Kenney halls — `organicness` high, `planning_vs_splendor` high).
2. A transition piece (collapsed wall / waste chute / service descent) drops them into the **default sewer / ventilation / subway substrate** for the bulk of the run.
3. Near the end, a second transition (sealed bulkhead / retrofitted section) brings them **into synth-tradition architecture**, which they traverse to the extraction in the synth camp.

(*"Synth" is used here illustratively — it is not yet one of the placeholder factions in §5.3; add a profile there when it is locked.*)

### 2.2 Industrial as substrate; camps as faction hubs

Industrial utility (sewer / vent / subway) is the **baseline layer** the level is
built on. The four non-industrial factions each own a **camp** — a safe station
between maps, rendered entirely in that faction's architecture.

| Layer | Role |
|-------|------|
| **Industrial substrate** | Default underground connective tissue for most of the run |
| **Faction camp** | Meta hub between levels; 100% that faction's buildings |
| **Faction bookends on a level** | Start = leaving previous camp's architecture; end = entering next camp's architecture |

**Camp adjacency loop:** player leaves camp A → level opens in A's tradition →
industrial middle → level closes in B's tradition → player enters camp B.

**Default fractions** along spawn→extract (prev / industrial / next): **15% /
60% / 35%** — industrial dominates; faction bookends are short. Preset in
§5.2 `mix_mode`; not equal thirds.

### 2.3 Transition entrances (not instant interiors)

**Problem today:** zone painting in `gen_freeform` / `level_composition.py`
swaps kit per spine fraction; the player walks from industrial **directly into
enclosed faction rooms** with no door or exterior read.

**Required model:**

1. **Industrial remains the structural floor plan**; faction geometry attaches
   at **transition nodes** via **entrances / doors**, not by replacing the whole
   interior grid at once.
2. **Transition path is per-faction** (profile field `transition_mode`):
   - `outdoor_then_indoor` — exterior buffer first (plaza, apron, courtyard).
     Favoured by **high-tech** factions (synth / space_station) that can afford
     engineered outdoor space.
   - `direct_on_substrate` — faction walls and doors sit **directly on dirt /
     industrial floor** with no landscaped outdoor. Favoured by **rogue /
     low-tier** factions (outlaw). Industrial tiles may **overlap** faction
     starts for several cells — substrate visibly continues under/around the
     new walls.
3. **Every industrial ↔ faction hand-off** emits an **entrance piece**: door
   frame + walkable threshold. Where the architecture kit provides them, add
   **small entrance stairs** to elevate the faction floor above the substrate
   (space_station: `stairs-small-*` straight and rounded — use at every
   entrance; catalogue in `docs/kenney_kits_catalogue.md` §space_station).
4. Lower-tier factions may show **industrial bleeding into the faction zone**
   for a few tiles after walls appear; high-tech factions transition cleaner
   via outdoor or raised platforms.

Implement in **Phase 4** (zone planner + transition spec) and **Phase 6**
(camp-driven variety). Do not fake this with kit painting alone.

---

## 3. Building systems (two families today)

### 3.1 Kenney space kit — *people buildings*

- **Grid:** 4 m cells, 20 m modules (`docs/kenney_space_kit.md`, `tools/gen_maps.py`).
- **Read:** offices, habitation, ritual halls, airlocks — **planned for humans**.
- **Layout character:** rooms, corridors, junctions; can be organic (mission graph) or stiff (grid).
- **Implementation today:** tile synthesis (`template-floor`, `template-wall`, `corridor-*`), editor Proc tab, `userinput/kenney_layout.json`.

### 3.2 Industrial utility — *infrastructure*

- **Read:** sewer water, rusted metal, ducts, rails, braces — **repetitive, modular, function-first**.
- **Layout character:** tunnel runs, shaft clusters, track beds; less “room decoration”, more **repeat units**.
- **Implementation today:** procedural modules in `crates/shared/src/level.rs` (`RoomKind`, `ConnType`, sewer/open builders); textures/atmosphere in client (`sewer_atmosphere`, water render).
- **Not the same procgen path as Kenney** — merging them is a **transition design** problem, not “use corridor-junction everywhere”.

### 3.3 Future faction kits

Each faction may define:

- Preferred building system (Kenney vs industrial vs bespoke).
- Overrides within that system (textures, piece subset, sizing rules).

**Available kits inventoried** (`docs/kenney_kits_catalogue.md`, `assets/models/kenney_kits_index.json`): 8 new Kenney kits (901 GLB) are extracted and prepared. Architecture-grade modular systems beyond the current `space/` kit: `dungeon/` (stone — *same modular grammar as `space/`*, ideal for a priesthood/order faction), `space_station/` (sleek high-tech — synth candidate), `building/` (urban brick), `retro_fantasy/` (medieval fort exterior). Dressing kits: `factory/` (industrial pipes/conveyors/machines for the industrial substrate), `furniture/` (interior props), `space_kit/` (exterior/rail/terrain). `prototype/` = blockout only. **None are wired into procgen yet** — each needs a mesh-measured catalogue (§ catalogue doc) before generation, cheapest first target being `dungeon/`.

---

## 4. Level composition & transitions

### 4.1 Logical structure of a level

A level is a **directed path** (entry → extraction) through **zones**. Each zone has:

| Field | Description |
|-------|-------------|
| `building_system` | `kenney` \| `industrial_sewer` \| `industrial_vent` \| `industrial_rail` \| … |
| `faction_profile_id` | Params from §5 |
| `fraction_of_level` | 0–1 along main path, or graph node count |
| `layout_role_hint` | `spine` \| `hub` \| `chamber` \| `transition` \| `extract` |

**Transition zones** are first-class — not hard cuts:

- **Kenney → industrial:** e.g. maintenance hatch, collapsed wall, “dumping station” opening into a **sewer lake**, service corridor descending from hab deck to utility tunnel. Player reads: *civilized ends here*.
- **Industrial → Kenney:** e.g. sealed bulkhead, renovated section, cult retrofit of a pump room — *something built here on purpose*.
- **Industrial → industrial:** change **subtype** (sewer → vent → rail) via shaft, ladder, grating — same “inorganic” family, different **catalogue + repetition pattern**.

Transitions must satisfy **gameplay**: walkable continuity, door alignment, floor masks, probe-clean borders (same bar as hub/extraction today).

### 4.1b Entrance-first transition geometry

A transition zone is not a kit swap — it is a **composed sequence**:

```
[industrial corridor] → [optional outdoor/apron] → [entrance stairs?] → [door frame] → [faction interior]
```

Rules:

- **Doors are mandatory** at every industrial ↔ faction boundary. No faction
  building opens directly into a sealed interior without a readable entrance.
- **Entrance stairs** (kit-specific small stair GLBs) raise faction floor level
  where the substrate is lower — especially space_station and other kits with
  `stairs-small-*` variants.
- **Outdoor buffer** is conditional on `transition_mode` (§2.3), not universal.
- **Substrate overlap:** for `direct_on_substrate` factions, the zone planner
  may assign `default` (industrial) cells inside the `next` fraction until a
  door threshold is crossed — visual + walkable industrial continues under /
  beside early faction walls.

### 4.2 Who decides the mix?

For a given run level:

```
level_plan = f(
  seed,
  previous_camp.faction,
  next_camp.faction,
  route_metadata,
  global_diversity_rules,
)
```

Output: ordered list of **zones** with `(building_system, faction_profile, length, transition_spec_at_start)`.

Variety rules (future):

- All one system (all Kenney, all sewer).
- Two-system blend (70/30, half/half).
- Three factions in one level (rare; more transition nodes).

---

## 5. Procgen variables (the editor slider spec)

There are **two variable sets**, and the editor exposes them **together** when you generate a level:

- **§5.1 Faction profile** — *per faction.* A transition level involves **two** factions (previous-camp and next-camp), so the editor shows **two faction blocks** at once.
- **§5.2 Level composition** — *per level.* The blend: how much of the level is previous-faction / default substrate / next-faction, plus transition and substrate choice.

§5.3 shows the combined editor panel. §5.4–5.5 are qualitative notes + concrete profiles.

> **Status legend** below: ✅ = already a real knob in `tools/gen_freeform.py`; 🟡 = partially driven by an existing knob; ⛔ = planned, **no generator support yet** (slider would be inert until implemented). This is the honest gap between the wishlist and the current free-form generator.

### 5.1 Faction profile variables (per faction)

Each faction stores its own values. Quantifiable = sliders/dropdowns; qualitative (§5.4) = notes.

| Variable | Type / range | Drives | `gen_freeform` knob | Status |
|----------|--------------|--------|---------------------|--------|
| `building_system` | enum (`dungeon`/`space_station`/`building`/`industrial_*`) | Which kit + generator backend | — (kit selection) | ⛔ |
| `prop_set` | enum (`retro_fantasy`/`furniture`/`factory`/…) | Decoration kit for dressing pass | — | ⛔ |
| `texture_catalogue` | id → asset set | Render-time skin; may constrain piece stems | — | ⛔ |
| `room_count_bias` | int / low-med-high | Number of rooms | `max_rooms` | ✅ |
| `room_size_bias` | min/max cells (or S/M/L) | Room footprint range | `room_min`, `room_max` | ✅ |
| `loop_count` | 0–N | Shortcut corridors reconnecting the graph | `loops` | ✅ |
| `main_path_length_bias` | short/med/long | Spawn→extract distance | `cells` (grid extent) | 🟡 |
| `density` | 0–1 | Filled-vs-empty floor ratio | derived (`max_rooms`×size / `cells`²) | 🟡 |
| `organicness` | 0–1 | Corridor windiness (clean L → jogged Z routes) | `organicness` | ✅ (corridor wander; room-edge jitter still TODO) |
| `planning_vs_splendor` | 0–1 | room scale + path directness (corridor width now its own knob) | composes `corridor_width` + `room_size` | 🟡 (building blocks exist; not yet a single mapped knob) |
| `corridor_width` | 1.0–2.0 | Lane width as a **fraction** of corridors that are 2-wide (1.3 = ~30% wide) | `corridor_width` | ✅ (2-wide emitted as room-style floor+walls) |
| `floor_preference` | single / multi | Vertical levels / shafts | — (single floor + hub levels) | ⛔ |
| `hidden_area_prevalence` | 0–1 | Secret side rooms, single entrance | `hidden_area_prevalence` | ✅ generation (dead-end rooms); 🟡 runtime open mechanic still TODO |
| `transition_mode` | enum (`outdoor_then_indoor` / `direct_on_substrate`) | How this faction meets industrial (§2.3) | — | ⛔ |
| `entrance_stem` | GLB stem | Door + optional small-stair kit at industrial boundary | — | ⛔ |
| `prop_setups` | list of setup defs | Named prop layouts (office cluster, cantina, …) — see §5.4b | — | ⛔ |

> **`hub_count` dropped** — it meant "extra central junction rooms," but in the free-form model that's **emergent** from `loop_count` + room degree, and "hub" collides with the *extraction* hub (trap → hub room → landings). Not a separate knob.

> **Hidden-area open mechanic (user spec):** a generated secret room is sealed by a **secret-door wall**; the player opens it by **left-clicking the wall**, and *both* the visual mesh and its collider **slide aside (side/up/down)** to reveal the room. This is a **runtime gameplay feature** (click raycast → identify secret-door entity → animate slide + toggle collider), paired with — but separate from — the generation that places the room and tags the wall. Build the generation half first; the interaction half is its own task.

**To make the remaining ⛔ rows real:** add to `gen_freeform.py` — a multi-floor pass, and room-edge jitter (finishing `organicness`); plus the runtime secret-door interaction in Rust (seal/open hidden rooms). Each is an isolated addition. Done so far: `organicness` ✅, `corridor_width` ✅, `hidden_area_prevalence` ✅ (generation half).

### 5.2 Level-composition variables (per level)

The blend across the run, parallel to (not inside) the faction profile. This is what answers "how much previous / default / next."

| Variable | Type / range | Meaning |
|----------|--------------|---------|
| `prev_faction` | enum (faction id) | Previous camp's faction — bookends the **start** (§2.1) |
| `next_faction` | enum (faction id) | Next camp's faction — bookends the **end** (§2.1) |
| `default_substrate` | enum (`sewer`/`vent`/`rail`) | Which industrial subtype fills the **middle** |
| `mix_mode` | preset (`all_one`/`two_blend`/`three_way`) | Sets the three fractions to sensible presets |
| `prev_fraction` | 0–1 | Share of level in previous-faction architecture |
| `default_fraction` | 0–1 | Share in the default industrial substrate |
| `next_fraction` | 0–1 | Share in next-faction architecture (3 fractions sum to 1) |
| *(preset)* | — | **Realistic default:** 0.15 / 0.60 / 0.35 (not equal thirds) |
| `transition_length` | cells / rooms | Span of each hand-off zone between systems |
| `transition_style` | enum per boundary (`hatch`/`flood`/`bulkhead`/`ritual_seal`) | Narrative read of each transition (§8) |
| `seed` | int | Reproducible generation |

> All ⛔ for now — there is no zone planner yet (§6.2 gap). `gen_freeform` builds a single-faction interior; the composition layer is Phase 4.

### 5.3 Editor panel grouping (how the sliders sit together)

When generating a **transition level** the Proc tab shows one panel:

```
┌── LEVEL COMPOSITION (§5.2) ───────────────────────────────┐
│ prev_faction ▼   default_substrate ▼   next_faction ▼     │
│ mix_mode ▼   [prev ░░░|default ░░░|next ░░░]  transition ─ │
│ seed ___                                                   │
├── FACTION A  = prev_faction  (§5.1) ──────────────────────┤
│ rooms ─  size ─  loops ─  length ─  organicness ─  …       │
├── FACTION B  = next_faction  (§5.1) ──────────────────────┤
│ rooms ─  size ─  loops ─  length ─  organicness ─  …       │
└────────────────────────────────────────────────────────────┘
                          [ Generate ]
```

All-one-faction level → composition collapses (one faction block, no default/next). The point: **both involved factions' knobs + the blend are tunable in a single generate pass.**

### 5.4 Qualitative (notes, not sliders)

- **Silhouette:** arches vs flat panels vs greeble density.
- **Verticality:** ladders, pits, stairs-as-story vs single plane.
- **Symmetry:** ceremonial axis vs outlaw asymmetry.
- **Lighting grammar:** neon strips vs torch niches (layout: where **chokes** and **reveals** go).
- **Transition vocabulary:** how this faction *meets* others (hatch, gate, flood, ritual seal).

Store as free-text `notes` on the profile until they become rules.

### 5.4b Faction structural archetypes (asset-gated, not sliders)

Beyond the numeric knobs, a faction may declare **named structural archetypes** — distinctive room/space setups that read as *that culture* and that the generator places **only when the faction owns the assets to dress them**. These are not continuous sliders; they're discrete, asset-gated layout patterns.

Each archetype carries:

| Field | Meaning |
|-------|---------|
| `id` | e.g. `inner_garden`, `ritual_apse`, `scrap_market`, `cryo_bay` |
| `structure` | the layout requirement (e.g. *enclosed courtyard: a room whose centre cells are open-air/floorless inside a full wall ring*) |
| `requires_assets` | prop/GLB set the archetype needs (e.g. garden/foliage props) — **archetype is skipped if the faction's `prop_set` lacks them** |
| `frequency` | 0–1 chance per eligible room, or a count cap per level |
| `placement_rule` | where it's eligible (dead-end chamber, hub, spine-adjacent, min size) |

Example: **priesthood `inner_garden`** — a room with a walled perimeter but a floorless/planted centre courtyard, placed only because `retro_fantasy` supplies trees/foliage/fence props. A faction without garden assets never gets it; an outlaw faction might instead declare `scrap_market` reusing `factory` salvage props in the same "open-centre room" structural slot.

**Props are not random scatter.** Each setup is a **typical arrangement** — e.g.
synth `office_cluster` = desk + computer facing corridor; `cantina_row` = tables
along a wall; industrial `pipe_run` = aligned factory props along a spine cell.
Setups declare:

| Field | Meaning |
|-------|---------|
| `props` | ordered GLB stems + relative offsets / yaw |
| `requires_room` | min footprint, shape tag (`rect`, `L`, `ring`), or `outdoor_courtyard` |
| `placement_rule` | `dead_end`, `hub_centre`, `corridor_mid`, `transition_apron`, … |
| `requires_archetype` | optional link to a structural archetype (courtyard must exist first) |

A faction may require a **specific outdoor shape** (open centre surrounded by
buildings) before an outdoor-only setup is eligible — the archetype creates the
space, the setup dresses it. Skipped automatically if assets or room shape fail.

This keeps lore-specific spaces **data-driven and self-disabling**: add the assets + an archetype entry, and the faction starts producing that space; no generator code per faction. Implement with the **props pass** (handover roster #1), after Phase 1 Kenney layout is trustworthy — Phase 2–3 for profile schema, Phase 4+ for transition-linked setups.

### 5.5 Concrete profiles (the three real factions + default)

Maps to prepared kits (see `docs/kenney_kits_catalogue.md`). `organicness`/`planning_vs_splendor` are target values for when those knobs land.

| ID | `building_system` | `prop_set` | organicness | planning_vs_splendor | room_size | notes |
|----|-------------------|-----------|-------------|----------------------|-----------|-------|
| `industrial_default` | `industrial_*` (procedural) | `factory` + `space_kit` (pipes/monorail) | 0.2 | 0.1 | small bays | Stale substrate; sewer/vent/rail. Metal PBR + sewer water. |
| `priesthood` | `dungeon` (+ `retro_fantasy` exterior) | `retro_fantasy` props | 0.7 | 0.9 | large | Wide stone halls; indoor + outdoor sets. |
| `synth` | `space_station` | `furniture` | 0.4 | 0.6 | medium | Sleek, polished; interior only so far. |
| `outlaw` | `building` (urban brick) | `furniture` / `factory` salvage | 0.6 | 0.2 | mixed | Improvised hab; barricades, gutters. |

*("synth" / "outlaw" are working names — final faction identities TBD.)*

---

## 6. Generation pipeline (architecture)

### 6.1 Layers (do not skip)

```
┌─────────────────────────────────────────────────────────┐
│ A. Mission graph (module slots) — roles, spine, branches│
├─────────────────────────────────────────────────────────┤
│ B. Zone assignment — building_system per graph region   │
├─────────────────────────────────────────────────────────┤
│ C. Geometry realization — per slot, per system          │
│    Kenney: tile synthesis / chambers / corridors        │
│    Industrial: level.rs module expansion                │
├─────────────────────────────────────────────────────────┤
│ D. Transition pieces — hand-authored or rule-based      │
├─────────────────────────────────────────────────────────┤
│ E. Validation — probes, floor masks, border alignment   │
└─────────────────────────────────────────────────────────┘
```

**Rooms are not step 1.** Graph + roles first; “rooms” are **chambers** assigned to branch leaves or faction policy.

### 6.2 Current codebase map

| Layer | Kenney | Industrial |
|-------|--------|------------|
| Graph | `tools/gen_maps.py` `design_high_level` (needs mission graph) | `level.rs` `gen_grid` (depth, dead-ends) |
| Realization | `strat_planned` / `build_tiled_floor_room` | `build_sewer`, `build_open`, … |
| Editor | Proc sidebar tab | not wired |
| Runtime layout | `kenney_layout.json`, `map_pool.rs` | `LevelDef` / stretch `build` fns |

**Gap:** No **zone planner**, no **transition spec**, no **faction profile** resource. Kenney and industrial are separate pipelines.

---

## 7. Phased delivery (one thing at a time)

### Phase 0 — This document ✓

Agree vocabulary before more code.

### Phase 1 — Generic Kenney mission graph *(current focus)*

**Goal:** Layouts that feel purposeful on Kenney alone.

- Replace random tree + avg degree with **spine + branches + roles** (§6.1 layer A).
- Role-based tile synthesis: chambers on dead-ends, hub routing to center (§6.1 layer C, Kenney only).
- Keep **no room GLBs**; tiled floors for chambers.
- Editor Proc tab: seed, attempts, synth retries — **no faction sliders yet**.
- Probes stay green.

**Exit criteria:** Playtesters describe routes as “main path + side areas” without props.

### Phase 2 — Faction profile schema (data only)

- Add `FactionProcgenProfile` struct (Rust + JSON in `userinput/factions/` or similar).
- Wire **one** test profile + default profile; generation still Kenney-only but reads e.g. `organicness` and `room_count_bias`.
- No camp adjacency yet — manual profile pick in editor.

### Phase 3 — Editor tuning for quantifiable params

- Extend Proc panel: profile dropdown + sliders from §5.1.
- Live regen (existing thread model).
- Save/load profile presets.

### Phase 4 — Zone planner + single transition

- Level = `[zone, zone, …]` along spine.
- Implement **one** transition template: Kenney zone → industrial zone (e.g. exit into sewer lake).
- `previous_camp` / `next_camp` stubbed to constants; then wired to run graph.

### Phase 5 — Industrial procgen parity

- Unify or coordinate `gen_grid` with module graph (same slot grid or explicit stitch points).
- Faction-specific industrial repetition from profile.

### Phase 6 — Full camp-driven variety

- `level_plan = f(previous, next, seed)` with multi-faction levels and 3-way splits.

**Do not start Phase 2+ until Phase 1 exit criteria met.**

---

## 8. Transition design checklist

Every transition between building systems must define:

- [ ] **Narrative read** (one sentence — e.g. “hab waste chute → sump”).
- [ ] **Entry piece(s)** — last Kenney slot openings match first industrial connector.
- [ ] **Floor continuity** — masks / physics agree (no phantom ledges).
- [ ] **Vertical offset** — if floor level changes, explicit stairs/ladder/hole (see hub extraction lessons in `docs/hub-extraction-agent-failures.md`).
- [ ] **Probe coverage** — border openings + decor audit where applicable.
- [ ] **Playtest path** — spawn → transition → extract without softlocks.

---

## 9. Editor UX (future, after Phase 1)

| Control | Phase |
|---------|-------|
| Seed, attempts, synth retries | 1 (done) |
| Spine length, branch count, hub count | 1–2 |
| Faction profile dropdown | 2 |
| organicness, planning_vs_splendor, … | 3 |
| Zone preview (A=Kenney, B=industrial) | 4 |
| Camp adjacency preview (mock prev/next) | 6 |

---

## 10. Open questions

1. **Single grid or stitched chunks?** Is the 5×5 Kenney module grid universal for all systems, or does industrial use a different cell size (12 m in `level.rs`) with explicit **stitch nodes**?
2. **Extraction placement** — always Kenney end module, or faction-specific extract (surface hatch vs rail terminal)?
3. **Multi-floor** — Kenney hub already uses floor levels; industrial verticality — same `FloorMask` model?
4. **Hidden areas** — separate graph layer (secret edges) or tagged branch roles on same graph?
5. **Authoring transitions** — JSON templates vs coded “transition modules” in `userinput/transitions/`?

Resolve during Phase 4; do not block Phase 1.

---

## 11. References

- Kenney space kit detail: `docs/kenney_space_kit.md`
- All Kenney kits inventory: `docs/kenney_kits_catalogue.md` (+ `assets/models/kenney_kits_index.json`)
- Hub / extraction pitfalls: `docs/hub-extraction-agent-failures.md`
- Kenney map gen: `tools/gen_maps.py`
- Industrial module gen: `crates/shared/src/level.rs` (`gen_grid`, `RoomKind`)
- Run / camps: `crates/shared/src/run.rs`, `update2.md`
- Editor live regen: `crates/client/src/editor_map_gen.rs`, Proc sidebar tab

---

*Last updated: 2026-06-22 — §2.2–2.3 industrial substrate + camp hubs; entrance-first transitions (doors, entrance stairs, outdoor vs direct-on-substrate); 15/60/35 default fractions; §5.4b prop setups (not random scatter).*
