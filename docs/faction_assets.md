# Faction asset system

Scalable, self-contained per-faction asset folders. Each faction the procgen can
build from owns a folder under `assets/models/factions/<id>/` containing its GLB
copies, its colormap(s), a machine-readable manifest (`faction.json`), and human
notes (`catalogue.md`). This decouples factions from the shared Kenney kits and
from each other, so a faction can be **duplicated** (copy the folder) or an item
**relocated** (its role entry records stem + facing + flags).

## Folder layout

```
assets/models/factions/<id>/
  faction.json          # machine manifest — source of truth (schema below)
  catalogue.md          # human notes
  Textures/colormap.png # this faction's own colormap (recolored / baked / copied)
  <stem>.glb ...         # GLB copies (own geometry; Blender-editable in isolation)
```

Current factions: `space_default` (pink), `industrial` (dirt floor), `priesthood`
(stone). Planned: `synth` (space_station), `outlaw` (building).

## `faction.json` (schema 1)

Loaded by `tools/faction_assets.py`. Key fields:

| Field | Meaning |
|---|---|
| `id` | == folder name; kit string is `factions/<id>` |
| `profiles` | faction-profile ids this asset serves (`userinput/factions/*.json`) |
| `source_kit` | Kenney kit the GLBs were copied from |
| `colormaps.primary` | the faction's colormap (folder-relative) |
| `material.mode` | `explicit` (editor builds a StandardMaterial) or `native_glb` (keep the GLB's own material) |
| `material.client_slot` | the `KenneyMaterialSlot` the client uses (cross-ref to test_showcase.rs) |
| `material.*` | albedo / base_color / metallic / roughness / emissive(+map) / mr_map / uv_transform / double_sided |
| `roles.<role>` | per-role usage: `stem` (the GLB the generator emits for this role — overrides the default `template-*`), `yaw_offset` (radians added to placement yaw — corrects a kit's native facing), `inset` (wall only: world units pushing the wall out along its side normal — corrects an off-centre depth anchor), `collide`, `placement`, `faces_at_yaw0`; corridors carry `corner_stem`, `floorless_corner`, `built_from` |

**Calibrating a new kit's walls/corners (no renders needed):** measure the piece
bbox (`pygltflib` accessor min/max). A wall's depth (thin) axis CENTRE should be
~0 like `space_station/wall` (renders flush); if it's off-centre `c`, set
`inset = -c * scale`. If a corner/wall faces the wrong way vs the space
`corridor-corner`/`template-wall` the `CORNER_YAW`/`WALL_YAW` tables target, set
`yaw_offset` (usually a multiple of π/2). Examples in `factions/necropolis`
(brick-wall inset 1.4) and `factions/synth` (wall-corner yaw_offset π/2).
| `provides` | which roles (`floor`/`wall`/`corridor`) route to THIS folder; unprovided roles fall back to the space grammar |
| `scale` | uniform spawn scale for this faction's pieces (default `1.0`). Use when a kit is authored at a different unit size than the 4-unit generator cell (e.g. space_station is 1-unit → `scale: 4`). |

The generator emits each faction's `roles.*.stem` at its `scale` (resolved per
zone in `build_zone_kits`), so a faction can supply its own non-`template-*`
geometry. Existing factions use the default stems at scale 1 → unchanged.

A faction with **no** manifest falls back to its `building_system` kit for all
roles (legacy whole-kit behaviour) — that is how `synth`/`outlaw` work today.

## How it drives the generator

`tools/gen_freeform.py::build_zone_kits(comp)` resolves each composition zone
(`prev`/`default`/`next`) to its faction profile → asset manifest → per-role kit,
and the set of floorless-corner kits. So the **look follows the faction**: reorder
the factions in a transition and the visuals move with them (verified). `_piece`
reads the resolved `_ACTIVE_ZONE_KITS[zone][role]`.

## How it drives the client (editor)

`crates/client/src/test_showcase.rs::kenney_material_slot` routes `kit` →
`KenneyMaterialSlot`; the editor bake (`kenney_editor.rs::editor_apply_materials`)
applies the matching material. Explicit materials are built in
`init_editor_kenney_materials`. **Explicit (not NativeGlb) is preferred** — Blender
re-exports often produce glTF materials Bevy fails to texture (renders white).

## Tools

| Tool | Purpose |
|---|---|
| `faction_assets.py` | load manifests; `python faction_assets.py [show <id>]` |
| `validate_factions.py` | check manifests vs files + profiles (run after edits) |
| `bake_tinted_colormap.py` | bake an sRGB tint into a colormap (linear-correct) — for tint-based looks like pink |
| `recolor_priesthood.py` | hue-targeted recolor (brown→gray) for the priesthood colormap |
| `repair_priesthood_glb.py` | normalize a Blender-edited GLB to the external-colormap pattern |
| `glb_externalize_texture.py` | repoint an embedded texture to the external colormap |

## Adding a new faction (e.g. synth)

1. `assets/models/factions/synth/` — copy the kit's GLBs (or the subset of roles
   it provides) + a `Textures/colormap.png`.
2. Write `faction.json`: `profiles: ["synth"]`, material mode + slot, roles +
   `provides`. If the kit's pieces face differently, set per-role `yaw_offset`.
3. If the kit is a NEW grammar (space_station/building are not the modular
   space/dungeon grammar), it first needs a measured catalogue
   (`generate_kenney_catalog.py`) before the generator can place it.
4. If a new client material is needed, add a `KenneyMaterialSlot` + resource +
   route in `test_showcase.rs` and bake it in `kenney_editor.rs`.
5. `python tools/validate_factions.py` → PASS, then generate a map.

## Known coupling to revisit

The pink accent **ceiling** is currently bound to the composition zone `prev`
(client `kenney_material_slot`), not to the `space_default` faction. With the
default ordering this is correct; if you reorder so a non-pink faction is in
`prev`, the ceiling stays pink. Make ceilings faction-driven when it matters.
