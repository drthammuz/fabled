# industrial faction — GLB catalogue

The **default/middle** transition zone. Its WALLS and CORRIDORS stay the shared
space grammar (gray `SpaceCyber` material) — only the **floor** is folder-owned:
a copy of the Kenney dungeon dirt floor GLBs + the dungeon `colormap.png` (brown
dirt). This decouples industrial's dirt from the shared `dungeon/` kit (and from
priesthood, which has its own recolored copy).

## Look
- **Floor:** brown dirt (dungeon colormap, original brown — NOT recolored).
- **Walls / corridors:** gray space (`SpaceCyber`, `kit=None`, not this folder).
- **Ceilings:** space grammar (`kit=None`).

## Material
Floor pieces use the **NativeGlb** path — the editor keeps the GLB's own external
`Textures/colormap.png` (dirt). These are original Kenney dungeon GLBs (not
Blender re-exports), so NativeGlb is reliable. If you ever Blender-edit one, run
`python tools/repair_priesthood_glb.py <glb>` (or add an explicit material) to
avoid the white-out bug.

## Contents
| Role | Stem (`.glb`) | Notes |
|---|---|---|
| Floor tile | `template-floor` | the dirt tile actually emitted |
| Floor variants | `template-floor-big/detail/detail-a/layer/layer-raised/layer-hole` | available; not all emitted |

Walls/corridors are NOT here on purpose — industrial uses space-kit walls.

## Wiring
- Generator: `FACTION_ZONE_KITS["default"]` → `{"floor": "factions/industrial"}`.
- `kenney_material_slot`: `factions/industrial` is a non-space kit → `NativeGlb`.
