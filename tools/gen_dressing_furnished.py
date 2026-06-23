#!/usr/bin/env python3
"""Generate validated synth dressing vignettes using placement_catalog.json."""

from __future__ import annotations

import json
import math
import sys
from copy import deepcopy
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
import synth_interior as si  # noqa: E402

IN_DIR = ROOT / "userinput" / "synth_dressing"
OUT_DIR = IN_DIR
HALF_PI = math.pi / 2


def load_template(name: str) -> dict:
    return json.loads((IN_DIR / f"{name}.json").read_text(encoding="utf-8"))


def write_example(name: str, doc: dict, note: str) -> None:
    doc = deepcopy(doc)
    doc["name"] = name
    doc["note"] = note
    if si.validate_props(doc["pieces"], name):
        raise SystemExit(1)
    path = OUT_DIR / f"{name}.json"
    path.write_text(json.dumps(doc, indent=2) + "\n", encoding="utf-8")
    nprops = sum(1 for p in doc["pieces"] if si.stem_info(p["stem"])["class"] != "structure")
    print(f"wrote {path} ({len(doc['pieces'])} pieces, {nprops} props) — {note}")


def bunk_furnished_c() -> None:
    doc = load_template("template_bunk")
    west_col = -4.0
    south_row = -6.0
    bed_corner = si.bed_origin_at_wall("south", west_col, south_row, 0.0)
    bed_west = si.bed_origin_at_wall("west", west_col, -2.0, HALF_PI)
    table = si.flush_back_to_wall("table", "east", 4.0, -HALF_PI, z=0.0)
    furn = [bed_corner, bed_west, table, *si.chairs_at_east_table("chair", table)]
    doc["pieces"] = doc["pieces"] + furn
    doc["spawn_xz"] = [0.0, 0.0]
    write_example(
        "bunk_furnished_c",
        doc,
        "12×16 m bunk — SW corner + west bed, east-wall table",
    )


def lab_furnished_c() -> None:
    doc = load_template("template_lab")
    south_row = -6.0
    desk_l = si.flush_back_to_wall("computer-screen", "south", -4.0, 0.0, z=south_row)
    desk_r = si.flush_back_to_wall("computer-system", "south", 4.0, 0.0, z=south_row)
    furn = [
        desk_l,
        desk_r,
        si.chair_before_desk("chair", desk_l, "computer-screen"),
        si.chair_before_desk("chair-armrest-headrest", desk_r, "computer-system"),
        si.flush_back_to_wall("container-tall", "north", 6.0, math.pi, z=6.0),
    ]
    doc["pieces"] = doc["pieces"] + furn
    doc["spawn_xz"] = [0.0, 0.0]
    write_example(
        "lab_furnished_c",
        doc,
        "16×16 m lab — south-wall desks, tight chairs, NE crate",
    )


def office_furnished_c() -> None:
    doc = load_template("template_office")
    south_row = -4.0
    north_east_col = 4.0
    north_row = 4.0
    desk = si.flush_back_to_wall("computer-system", "south", 0.0, 0.0, z=south_row)
    furn = [
        desk,
        si.chair_before_desk("chair-armrest-headrest", desk),
        si.flush_back_to_wall("container-tall", "east", north_east_col, -HALF_PI, z=north_row),
    ]
    doc["pieces"] = doc["pieces"] + furn
    doc["spawn_xz"] = [0.0, 0.0]
    write_example(
        "office_furnished_c",
        doc,
        "12×12 m office — south console, NE corner locker",
    )


def main() -> None:
    bunk_furnished_c()
    lab_furnished_c()
    office_furnished_c()


if __name__ == "__main__":
    main()
