# Priesthood faction — GLB catalogue

Self-contained copy of the Kenney dungeon kit. Every GLB references
`Textures/colormap.png` **in this folder**, which is a recolored copy of the
dungeon atlas: the brown dirt/rubble region has been neutralized to cool stone
gray (`tools/recolor_priesthood.py`). Editing this colormap only affects
priesthood pieces — zone-2 (industrial) dirt floors still read the original
`models/dungeon/Textures/colormap.png`.

## Look
- **Walls:** blue-gray stone (dungeon stone palette, untouched).
- **Floors:** stone gray (was brown dirt — recolored).
- **Wall-base rubble / corner-corridor floor:** stone gray (was brown — recolored).
- **Ceilings:** NOT from this kit. Ceilings always use the space grammar
  (`kit=None`) so they bake with the space ceiling material.

## Which GLB per situation
| Role | Stem (`.glb`) | Notes |
|---|---|---|
| Floor tile | `template-floor` | flat 1×1 cell |
| Floor (detail) | `template-floor-detail`, `template-floor-detail-a` | optional dressing |
| Wall | `template-wall` | single straight wall, one side of a cell |
| Wall corner | `template-wall-corner` | |
| Wall (half / top) | `template-wall-half`, `template-wall-top` | |
| Corridor straight | built from `template-floor` + `template-wall` tiles | retintable per surface |
| Corridor L-bend | `corridor-corner` | **floor mesh removed** (walls only); texture EMBEDDED (gray). Generator lays a `template-floor` tile under it so the floor matches surrounding floors. |
| Corridor end | `corridor-end` | |
| Corridor T-junction | `corridor-junction` | |
| Corridor cross | `corridor-intersection` | |
| Door | `gate-door`, `gate-door-window` | |
| Bars / grate | `gate-metal-bars` | |
| Stairs | `stairs`, `stairs-wide` | |
| Room shells | `room-small`, `room-large`, `room-wide` (+ `-variation`) | |

## Wiring
- Generator stamps `kit="factions/priesthood"` on zone-3 (next) walls + corridors
  in `tools/gen_freeform.py::_piece`.
- `glb_asset_path_in_kit(stem, "factions/priesthood")` → `models/factions/priesthood/{stem}.glb`.
- `kenney_material_slot` returns `NativeGlb` for this kit (non-space) → keeps the
  embedded recolored colormap rather than applying a cyber tint.

## Regenerating the recolor
`python tools/recolor_priesthood.py` (idempotent on the dungeon source; it writes
into this folder's `Textures/colormap.png`). Re-run after re-copying from dungeon.
