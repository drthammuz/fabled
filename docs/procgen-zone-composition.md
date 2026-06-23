# Procgen zone composition — current state, options, recommendations

**Status:** Research summary (2026-06-23)  
**Audience:** Agents working on `gen_freeform`, `level_composition`, Proc tab, faction transitions  
**Related:** [procgen-faction-manifest.md](procgen-faction-manifest.md) §6.1 (layer order), [handover-factions-2026-06-22.md](handover-factions-2026-06-22.md) (entrance-first target), [synth-transition-architecture.md](synth-transition-architecture.md) (seam width / deck rules)

---

## 1. Current situation

### Pipeline today (editor Proc → `gen_maps.py` → `gen_freeform.py`)

```text
1. Layout     — rooms + corridors on N×N grid (default 25×25); one global knob set
2. Spine      — BFS shortest path: spawn room → hub trap
3. Zone paint — each walkable cell projected onto spine → t ∈ [0,1] → prev / default / next
4. Buffer     — edge-adjacent prev↔next demoted to default (4-connect only)
5. Emit       — faction kit per zone; transition pieces at spine boundaries
```

**Layout is faction-blind.** `LevelComposition` is passed in but not used during room/corridor placement. Faction profiles define per-faction `room_min`, `room_max`, `organicness`, `corridor_width`, etc., and `apply_generation_defaults()` exists in `tools/faction_profiles.py` — but **nothing calls it**. The Proc panel exposes **one** organicness / corridor-width for the whole map, not per prev/next faction (manifest §5.3 target: two faction blocks).

### How percentages work

Sliders (`prev_fraction`, `default_fraction`, `next_fraction`) control **share of spine arc-length**, not share of floor tiles. Side rooms inherit zone from **projection onto the spine**, not map position. Actual tile counts often diverge heavily from slider values (e.g. target 15/60/35 vs ~30/32/38 by cell count on sample seeds).

### Known problems

| Issue | Cause |
|-------|--------|
| prev and next **corner-touch** | `enforce_industrial_between_factions` blocks 4-neighbor edges only; diagonal prev↔next pairs are common |
| Transition misalignment | `zone_for_cell` uses arc-length **t**; `find_zone_boundaries` uses spine index **i/n** |
| Too-narrow synth seams | Transitions adapt to layout (`scan_seam_strip`); 1-wide substrate corridors → width-1 stair runs |
| Wrong spatial “feel” | Priesthood wants large halls; industrial wants small bays; synth wants wider corridors — all share one carved graph |
| Interior-first read | Whole zones get faction kits; handover target is industrial substrate + **entrances** grafted at boundaries |

### vs manifest intent (§6.1)

```text
A. Mission graph → B. Zone assignment → C. Geometry per zone/system → D. Transitions → E. Validation
```

Current free-form is roughly **C before B** (monolithic geometry, then zone paint as a skin). Manifest Phase 4 zone planner is **partial** (`level_composition.py` only).

### Key files

| File | Role |
|------|------|
| `tools/gen_freeform.py` | Layout, emit, hub |
| `tools/level_composition.py` | Spine, zone paint, buffer, kit/zone lookups |
| `tools/faction_profiles.py` | Per-faction spatial knobs (unused at layout time) |
| `tools/transition_entrances.py` | Boundary doors/stairs; spine-index boundaries |
| `tools/synth_transition.py` | Elevated deck; seam width from layout |
| `crates/client/src/editor_map_gen.rs` | Proc tab; global layout knobs |
| `tools/test_synth_transition.py` | prev/next 4-neighbor test only |

---

## 2. Options

### A. Patch zone buffer (smallest)

Extend `enforce_industrial_between_factions`: 8-connectivity, minimum default band on spine, morphological BFS until prev/next components separated by ≥W cells.

| Pros | Cons |
|------|------|
| Hours–1 day; low risk for A1 (8-connect) | Does not fix narrow corridors or room scale |
| Fixes diagonal corner-touch | Shrinks faction areas unpredictably at wide bands |
| Aligns with synth doc “hard rule” | Band-aid on paint-on-top model |

### B. Unify spine parameterization

Use one **t** definition everywhere (arc-length or spine-index — pick one) for zone paint and `find_zone_boundaries`.

| Pros | Cons |
|------|------|
| Small change; doors align with painted edges | Alone: no separation or spatial fixes |

### C. Flood-fill from spine

Assign zones on spine cells from fractions; off-spine cells inherit nearest spine cell’s zone (not global projection).

| Pros | Cons |
|------|------|
| Side rooms follow local route neighborhood | Loops can still connect distant regions geographically |
| More intuitive “bands along path” | Still one global layout grammar |

### D. Graph-distance / watershed zones

Boundary indices on spine from fractions; multi-source BFS; default wins middle; hard prev/next separation.

| Pros | Cons |
|------|------|
| Stronger zone separation | Fractions = seed span, not tile area |
| Closest to Phase 4 “level = [zone, …] along spine” | 2–4 days; rebalance transition tests |

### E. Substrate-first + bounded faction footprints *(manifest / handover target)*

Full map = industrial layout. At transition nodes: stairs → door → faction interior only **behind door** (BFS depth cap). Rest stays industrial.

| Pros | Cons |
|------|------|
| Matches camp/substrate story | Faction areas feel small unless fractions large |
| Synth deck scoped naturally | Two geometry models in one map |
| Avoids whole-map kit swap | Multi-PR refactor of `to_doc` / emit |

### F. Zone-first, generate per zone

Plan zones on spine; run room/corridor placement **per zone** with that faction’s profile; stitch at boundaries.

| Pros | Cons |
|------|------|
| Each faction gets its corridor/room knobs | Stitch walkability at boundaries is hard |
| Archetypes (§5.4b) can target shapes | Three layout passes + validation |
| Manifest-native layer order | Largest layout refactor |

### G. Layout-first + adaptation pass

Keep current layout → paint zones → widen corridors, expand rooms, reserve transition aprons; reject seed if constraints fail.

| Pros | Cons |
|------|------|
| Incremental from today | Fighting the layout; many failed attempts |
| Can enforce min seam width for synth | Poor fit for archetype shapes |

### H. Constraint probes (enabler)

Audit: 8-neighbor prev/next, min default width, min seam width at boundaries, tile-% vs spine-%, room size in faction zones.

| Pros | Cons |
|------|------|
| Cheap; documents requirements | Does not fix root cause alone |

### I. Wire faction profiles into layout (partial)

Call `apply_generation_defaults` per zone or use blended/global max of prev/default/next profile knobs; split Proc panel per faction (§5.3).

| Pros | Cons |
|------|------|
| Uses existing profile data | Single global layout still compromises all factions |
| Editor honesty | Does not fix paint-on-top or touch without A–D |

---

## 3. Recommendations

### Phasing (respect manifest unless user redirects)

| Priority | Action |
|----------|--------|
| **Now (stabilize)** | **A1** (8-connect buffer) + **B** (unify **t**) + **H** (zone adjacency / seam-width probes) |
| **Short-term** | **G** + **H** if staying on paint-on-top; **I** (per-faction Proc knobs via profiles) |
| **Directionally correct** | **E** (substrate + entrance footprints) or **F** (zone-first multi-realization) — Phase 4 |
| **Later** | Structural archetypes (§5.4b) inside faction footprints only; props pass after layout trustworthy |

### Decision guide

| If the goal is… | Lean toward… |
|-----------------|--------------|
| Stop corner-touch this week | A + H |
| Align doors with zone edges | B |
| Sliders match floor area | C or D (+ report actual tile %) |
| Synth/priesthood spatial requirements | E or F; not kit swap alone |
| Trustworthy generic layout first | Finish manifest Phase 1; defer heavy zone work |

### Explicit non-goals for a zone-only PR

- Reintroducing room GLBs or module pool as default
- Merging industrial `level.rs` `gen_grid` (Phase 5)
- Hub/extraction changes unless probes regress

---

## 4. Verification (when implementing)

```bash
python tools/gen_freeform.py --seed 42 --mix-mode transition --prev-faction priesthood --next-faction synth
python tools/test_synth_transition.py
python tools/probe_layout_decor.py userinput/kenney_layout.json
```

Add zone-specific probes as options H lands (prev/next 8-connect, min seam width at boundaries).

---

*Last updated: 2026-06-23 — consolidates agent research on zone paint order, faction spatial requirements, and touch/adjacency bugs.*
