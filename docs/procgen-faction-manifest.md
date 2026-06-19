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

## 5. Faction procgen profile

Each faction has a **profile** (data, not code). Quantifiable fields are editor-tunable later; qualitative fields guide implementers.

### 5.1 Quantifiable (editor sliders / dropdowns later)

| Parameter | Range / type | Effect on layout |
|-----------|--------------|------------------|
| `organicness` | 0 = grid / repetitive modules, 1 = winding mission graph, irregular chambers | Industrial → low; priesthood → high on Kenney |
| `planning_vs_splendor` | 0 = shortest functional paths (military), 1 = long/wide ceremonial routes (priesthood) | Spine length, corridor width, hub size |
| `room_size_bias` | small / medium / large (or metres) | Tiled chamber radius in modules; industrial “bay” length |
| `room_count_bias` | low / medium / high | Branch count off main path |
| `texture_catalogue` | id → asset set | Visual only at render; may constrain piece stems |
| `floor_preference` | single / multi / mixed | Kenney floor levels; industrial vertical shafts |
| `hidden_area_prevalence` | 0–1 | Optional branches, secret tiles, duct bypasses |
| `loop_count` | 0–3 | Shortcuts reconnecting to spine |
| `hub_count` | 0–2 | Degree-3/4 junctions on spine |
| `main_path_length_bias` | short / medium / long | Steps spawn→extract on module graph |
| `building_system` | enum | Primary generator backend |
| `piece_subset` | list | Allowed GLBs / module defs |

### 5.2 Qualitative (manifest notes, not sliders)

- **Silhouette:** arches vs flat panels vs greeble density.
- **Verticality:** ladders, pits, stairs-as-story vs single plane.
- **Symmetry:** military axis vs outlaw asymmetry.
- **Lighting grammar:** neon strips vs torch niches (layout: where **chokes** and **reveals** go).
- **Transition vocabulary:** how this faction *meets* others (hatch, gate, flood, ritual seal).

Store as free-text `notes` on the profile until they become rules.

### 5.3 Example profiles (placeholder IDs)

| ID | Building system | organicness | planning_vs_splendor | room_size | notes |
|----|---------------|-------------|----------------------|-----------|-------|
| `industrial_default` | sewer/vent/rail | 0.2 | 0.1 | small bays | Repetitive tunnel modules |
| `kenney_habitation` | Kenney | 0.5 | 0.5 | medium | Current Proc gen target |
| `priesthood` | Kenney | 0.8 | 0.9 | large | Wide halls, long spine |
| `military` | Kenney or industrial | 0.3 | 0.0 | small | Shortest graph, few branches |
| `outlaw` | industrial | 0.6 | 0.2 | mixed | Hidden ducts, irregular loops |

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

- Kenney kit: `docs/kenney_space_kit.md`
- Hub / extraction pitfalls: `docs/hub-extraction-agent-failures.md`
- Kenney map gen: `tools/gen_maps.py`
- Industrial module gen: `crates/shared/src/level.rs` (`gen_grid`, `RoomKind`)
- Run / camps: `crates/shared/src/run.rs`, `update2.md`
- Editor live regen: `crates/client/src/editor_map_gen.rs`, Proc sidebar tab

---

*Last updated: 2026-06-19 — Phase 0 manifest + §0 agent handoff for Claude Code / fresh sessions.*
