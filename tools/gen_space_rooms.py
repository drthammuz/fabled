"""Generate the 'space_rooms' module pool and a connected test map."""
import json
import math
from pathlib import Path

PI   = math.pi
PI2  = PI / 2
PI32 = 3 * PI / 2

POOL     = "space_rooms"
POOL_DIR = Path("userinput/modules") / POOL
MAP_DIR  = Path("userinput/maps")
POOL_DIR.mkdir(parents=True, exist_ok=True)
MAP_DIR.mkdir(exist_ok=True)

CELL      = 4.0
MOD_CELLS = 5
MOD       = MOD_CELLS * CELL  # 20 m


def pc(stem, x, z, yaw=0.0, floor_level=0, scale=1.0):
    return {"stem": stem, "x": float(x), "z": float(z),
            "yaw": float(yaw), "floor_level": floor_level, "scale": float(scale)}

# Template-walls that close each room-large opening
def wall_N(): return pc("template-wall",   0, -10, PI)
def wall_S(): return pc("template-wall",   0,  10, 0.0)
def wall_E(): return pc("template-wall",  10,   0, PI2)
def wall_W(): return pc("template-wall", -10,   0, PI32)

def floor_det(x, z):  return pc("template-floor-detail-a", x, z)
def floor_big(x, z):  return pc("template-floor-big", x, z)
def wall_det(x, z, yaw=0.0): return pc("template-wall-detail-a", x, z, yaw)

def mask_all():
    return {"cells_x": 5, "cells_z": 5, "cells": [True] * 25}

def module(name, pieces):
    return {"version": 1, "name": name, "pool": POOL,
            "floor_mask": mask_all(), "extra_floor_masks": {},
            "pieces": pieces}


# ── 10 modules ────────────────────────────────────────────────────────────────
# Connectivity notation:  room-large openings are at the 20-m boundary centres.
# wall_X() fills that opening.  Extra pieces add visual variety.

# 1) 3-open N+S+E  ─ close W
m01 = module("m01_nse_junction", [
    pc("room-large", 0, 0),
    wall_W(),
    floor_det(-4,  4), floor_det(-4, -4),
])

# 2) 3-open S+E+W  ─ close N  (northernmost row entry module)
m02 = module("m02_sew_junction", [
    pc("room-large", 0, 0),
    wall_N(),
    floor_big(-4, 4),
])

# 3) 1-open N only  ─ dead-end; interior uses room-large-variation + wall details
#    Variety: wall-detail-a panels on the east & west inner walls
m03 = module("m03_n_deadend", [
    pc("room-large-variation", 0, 0),
    wall_S(), wall_E(), wall_W(),
    wall_det( 4, -4, PI2),
    wall_det(-4, -4, PI32),
    floor_det(4, 4), floor_det(-4, 4),
])

# 4) 3-open N+S+W  ─ close E
m04 = module("m04_nsw_junction", [
    pc("room-large", 0, 0),
    wall_E(),
    floor_det(4,  4), floor_det(4, -4),
])

# 5) 4-open N+S+E+W  ─ main hub; no walls, floor-big quad pattern
m05 = module("m05_nsew_hub", [
    pc("room-large", 0, 0),
    floor_big( 4,  4),
    floor_big(-4,  4),
    floor_big( 4, -4),
    floor_big(-4, -4),
])

# 6) 2-open N+W  ─ close S+E  (NW-corner shape)
m06 = module("m06_nw_corner", [
    pc("room-large", 0, 0),
    wall_S(), wall_E(),
    floor_det(-4, -4),
])

# 7) 2-open N+E  ─ close S+W  (NE-corner shape); room-large-variation
m07 = module("m07_ne_corner", [
    pc("room-large-variation", 0, 0),
    wall_S(), wall_W(),
    floor_det(4, -4),
])

# 8) 4-open N+S+E+W  ─ second hub; variation room + gate frames at N/S passages
m08 = module("m08_nsew_variation", [
    pc("room-large-variation", 0, 0),
    pc("gate", 0, -8, 0.0),  # gate frame just inside N boundary
    pc("gate", 0,  8, 0.0),  # gate frame just inside S boundary
])

# 9) 2-open S+E  ─ close N+W  (SE-corner shape)
m09 = module("m09_se_corner", [
    pc("room-large", 0, 0),
    wall_N(), wall_W(),
    floor_det(-4, -4),
])

# 10) 2-open S+W  ─ close N+E  (SW-corner shape)
m10 = module("m10_sw_corner", [
    pc("room-large", 0, 0),
    wall_N(), wall_E(),
    floor_det(4, -4),
])

MODS = [m01, m02, m03, m04, m05, m06, m07, m08, m09, m10]

for m in MODS:
    path = POOL_DIR / (m["name"] + ".json")
    path.write_text(json.dumps(m, indent=2))
    print(f"  wrote {path}")

# ── MAP ───────────────────────────────────────────────────────────────────────
# 5x5 module grid.   world origin: x0 = z0 = -50.
# Slot (col, row) center:  cx = -50 + (col+0.5)*20,  cz = -50 + (row+0.5)*20
# row 0 = northernmost (-Z),  row 3 = southernmost (+Z) used here.
#
# Connected layout (col, row):
#
#          col 0      col 1      col 2
#  row 3:            [m03 N]
#  row 2:  [m07 NE]  [m08 all]  [m06 NW]
#  row 1:  [m01 NSE] [m05 all]  [m04 NSW]
#  row 0:  [m09 SE]  [m02 SEW]  [m10 SW]
#
# Shared-face openings verified:
#   m09.S↔m01.N  m09.E↔m02.W
#   m02.S↔m05.N  m02.E↔m10.W
#   m10.S↔m04.N
#   m01.S↔m07.N  m01.E↔m05.W
#   m05.S↔m08.N  m05.E↔m04.W
#   m04.S↔m06.N
#   m07.E↔m08.W
#   m08.S↔m03.N  m08.E↔m06.W

MAP_MX = 5
MAP_MZ = 5
CELLS  = MAP_MX * MOD_CELLS   # 25
WX0    = -(CELLS * CELL) / 2  # -50
WZ0    = WX0

layout = [
    (0, 0, m09),
    (1, 0, m02),
    (2, 0, m10),
    (0, 1, m01),
    (1, 1, m05),
    (2, 1, m04),
    (0, 2, m07),
    (1, 2, m08),
    (2, 2, m06),
    (1, 3, m03),
]

# Floor mask
floor_cells = [False] * (CELLS * CELLS)
for (col, row, _) in layout:
    for iz_l in range(MOD_CELLS):
        for ix_l in range(MOD_CELLS):
            idx = (row * MOD_CELLS + iz_l) * CELLS + (col * MOD_CELLS + ix_l)
            floor_cells[idx] = True

# Pieces
map_pieces = []
for (col, row, mod) in layout:
    cx = WX0 + (col + 0.5) * MOD
    cz = WZ0 + (row + 0.5) * MOD
    for piece in mod["pieces"]:
        map_pieces.append({
            "stem":        piece["stem"],
            "x":           piece["x"] + cx,
            "z":           piece["z"] + cz,
            "yaw":         piece["yaw"],
            "floor_level": piece["floor_level"],
            "scale":       piece["scale"],
        })

# Spawn at centre of m05 hub  (col=1, row=1)
sp_x = WX0 + 1.5 * MOD   # -20
sp_z = WZ0 + 1.5 * MOD   # -20

map_data = {
    "version":   1,
    "name":      "space_rooms_map",
    "modules_x": MAP_MX,
    "modules_z": MAP_MZ,
    "floors":    {"0": {"cells_x": CELLS, "cells_z": CELLS, "cells": floor_cells}},
    "pieces":    map_pieces,
    "spawn_xz":  [sp_x, sp_z],
}

map_path = MAP_DIR / "space_rooms_map.json"
map_path.write_text(json.dumps(map_data, indent=2))
print(f"  wrote {map_path}")
print(f"  total pieces: {len(map_pieces)},  floor TRUE cells: {sum(floor_cells)}/625")
