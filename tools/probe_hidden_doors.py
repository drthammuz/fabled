#!/usr/bin/env python3
"""Audit hidden-room gate-door placement vs emitted corridor openings.

Compares each generated gate-door against:
  - wall_plane: midpoint between parent room cell and chosen corridor cell
  - corridor_center: centre of the chosen corridor cell (same anchor as corridor GLBs)
  - emitted: actual corridor / template-floor piece at the opening cell(s)

Real failures: wrong entrance cell pair, whole-tile (4 m) offset vs emitted piece,
or door not on any valid wall plane for the link.

The systematic 2 m gap between wall-plane door and corridor GLB centre is expected
(Kenney gate-door sits on the wall; corridor GLBs anchor at cell centre).

Usage:
    python tools/probe_hidden_doors.py
    python tools/probe_hidden_doors.py --seeds 500 --cells 16 --rooms 2 --loops 0
    python tools/probe_hidden_doors.py --verbose --seed 0 --cells 16 --rooms 2
"""
from __future__ import annotations

import argparse
import sys
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Optional, Tuple

sys.path.insert(0, str(Path(__file__).resolve().parent))
import gen_freeform as gf  # noqa: E402

Cell = Tuple[int, int]
EPS = 0.06
HALF = gf.CELL * 0.5


@dataclass
class DoorAudit:
    seed: int
    door_idx: int
    door: Tuple[float, float, float]
    room_cell: Cell
    corridor_cell: Cell
    adjacent_link: Tuple[Cell, ...]
    wall_plane: Tuple[float, float]
    corridor_center: Tuple[float, float]
    emitted: Optional[Tuple[str, float, float]]
    err_wall: Tuple[float, float]
    err_center: Tuple[float, float]
    err_emitted: Optional[Tuple[float, float]]
    matching_pair: Optional[Tuple[Cell, Cell]]
    best_adjacent: Optional[Cell]
    err_best_adjacent: Optional[Tuple[float, float]]
    failure: Optional[str]


def _near(wx: float, wz: float, x: float, z: float) -> bool:
    return abs(wx - x) < EPS and abs(wz - z) < EPS


def _err(a: Tuple[float, float], b: Tuple[float, float]) -> Tuple[float, float]:
    return (a[0] - b[0], a[1] - b[1])


def _mag(e: Tuple[float, float]) -> float:
    return (e[0] ** 2 + e[1] ** 2) ** 0.5


def wall_plane(gx: int, gz: int, room_cell: Cell, corridor_cell: Cell) -> Tuple[float, float]:
    ax, az = room_cell
    dx, dz = corridor_cell[0] - ax, corridor_cell[1] - az
    return (
        gf.world_x(gx, ax) + dx * HALF,
        gf.world_z(gz, az) + dz * HALF,
    )


def corridor_center(gx: int, gz: int, cell: Cell) -> Tuple[float, float]:
    return gf.world_x(gx, cell[0]), gf.world_z(gz, cell[1])


def emitted_at_cell(pieces: List[dict], gx: int, gz: int, cell: Cell) -> Optional[Tuple[str, float, float]]:
    wx, wz = corridor_center(gx, gz, cell)
    hits = [
        p for p in pieces
        if (p["stem"].startswith("corridor") or p["stem"] == "template-floor")
        and _near(float(p["x"]), float(p["z"]), wx, wz)
    ]
    if not hits:
        return None
    p = hits[0]
    return p["stem"], float(p["x"]), float(p["z"])


def entrance_pairs(fm: gf.FreeformMap, meta: gf.SecretDoorMeta) -> List[Tuple[Cell, Cell]]:
    """All (parent room cell, link corridor cell) pairs for this hidden link."""
    parent_cells = set(fm.rooms[meta.parent_room].cells())
    out: List[Tuple[Cell, Cell]] = []
    for rc in parent_cells:
        for dx, dz in gf.DELTA.values():
            cc = (rc[0] + dx, rc[1] + dz)
            if cc in meta.link_cells:
                out.append((rc, cc))
    return out


def find_matching_pair(
    wx: float, wz: float, gx: int, gz: int, pairs: List[Tuple[Cell, Cell]],
) -> Optional[Tuple[Cell, Cell]]:
    for rc, cc in pairs:
        if _mag(_err((wx, wz), wall_plane(gx, gz, rc, cc))) < EPS:
            return (rc, cc)
    return None


def classify(
    err_wall: Tuple[float, float],
    err_emitted: Optional[Tuple[float, float]],
    err_best: Optional[Tuple[float, float]],
    adjacent: Tuple[Cell, ...],
    corridor_cell: Cell,
    best_adjacent: Optional[Cell],
    meta_pair: Tuple[Cell, Cell],
    matching_pair: Optional[Tuple[Cell, Cell]],
) -> Optional[str]:
    """Return failure tag; None = door on wall plane for stored meta cells."""
    if not adjacent:
        return "no_link_adjacent"

    if matching_pair and matching_pair != meta_pair:
        return "door_matches_different_pair"

    if _mag(err_wall) >= EPS:
        if matching_pair is None:
            return "off_wall_plane"
        return "meta_pair_mismatch"

    if len(adjacent) > 1 and best_adjacent and best_adjacent != corridor_cell:
        if err_best is not None and _mag(err_best) < EPS:
            return "wrong_adjacent_cell"

    if err_emitted is not None and _mag(err_emitted) > EPS:
        ex, ez = err_emitted
        if abs(ex) in (gf.CELL, gf.CELL * 2) or abs(ez) in (gf.CELL, gf.CELL * 2):
            return "whole_tile_vs_emitted"

    return None


def audit_door(
    seed: int,
    door_idx: int,
    fm: gf.FreeformMap,
    doc: dict,
    meta: gf.SecretDoorMeta,
    door: Tuple[float, float, float],
) -> DoorAudit:
    gx, gz = fm.gx, fm.gz
    pieces = doc["pieces"]
    wx, wz, _yaw = door
    rc, cc = meta.room_cell, meta.corridor_cell
    meta_pair = (rc, cc)
    wp = wall_plane(gx, gz, rc, cc)
    ccen = corridor_center(gx, gz, cc)
    em = emitted_at_cell(pieces, gx, gz, cc)
    err_w = _err((wx, wz), wp)
    err_c = _err((wx, wz), ccen)
    err_e = _err((wx, wz), (em[1], em[2])) if em else None

    pairs = entrance_pairs(fm, meta)
    match = find_matching_pair(wx, wz, gx, gz, pairs)

    best_adj: Optional[Cell] = None
    err_best: Optional[Tuple[float, float]] = None
    best_mag = float("inf")
    for ac in meta.adjacent_link or (cc,):
        em_a = emitted_at_cell(pieces, gx, gz, ac)
        if em_a is None:
            continue
        e = _err((wx, wz), (em_a[1], em_a[2]))
        m = _mag(e)
        if m < best_mag:
            best_mag, best_adj, err_best = m, ac, e

    failure = classify(
        err_w, err_e, err_best, meta.adjacent_link, cc, best_adj, meta_pair, match,
    )

    emitted_pairs = [(pr, pc) for pr, pc in pairs if emitted_at_cell(pieces, gx, gz, pc)]
    if failure is None and emitted_pairs and cc not in {p[1] for p in emitted_pairs}:
        failure = "emitted_at_different_link_cell"

    return DoorAudit(
        seed=seed,
        door_idx=door_idx,
        door=door,
        room_cell=rc,
        corridor_cell=cc,
        adjacent_link=meta.adjacent_link,
        wall_plane=wp,
        corridor_center=ccen,
        emitted=em,
        err_wall=err_w,
        err_center=err_c,
        err_emitted=err_e,
        matching_pair=match,
        best_adjacent=best_adj,
        err_best_adjacent=err_best,
        failure=failure,
    )


def run_probe(
    seeds: int,
    *,
    cells: int,
    rooms: int,
    loops: int,
    hidden: float,
    room_min: int,
    room_max: int,
    verbose: bool,
    seed_start: int = 0,
) -> int:
    failures: List[DoorAudit] = []
    counts: Counter = Counter()
    half_tile_ok = 0
    total_doors = 0

    for s in range(seed_start, seed_start + seeds):
        fm = gf.generate_map(
            seed=s,
            cells=cells,
            max_rooms=rooms,
            loops=loops,
            hidden_area_prevalence=hidden,
            room_min=room_min,
            room_max=room_max,
        )
        if not fm or not fm.secret_doors:
            continue
        doc = gf.to_doc(fm, f"probe_{s}")
        for i, (door, meta) in enumerate(zip(fm.secret_doors, fm.secret_door_meta)):
            total_doors += 1
            audit = audit_door(s, i, fm, doc, meta, door)
            if audit.failure is None and audit.err_emitted and _mag(audit.err_emitted) >= HALF - EPS:
                half_tile_ok += 1
            if audit.failure:
                failures.append(audit)
                counts[audit.failure] += 1

    print(f"maps scanned: {seeds}  doors: {total_doors}  failures: {len(failures)}")
    if total_doors:
        ok = total_doors - len(failures)
        print(f"pass rate: {ok / total_doors * 100:.1f}%")
        print(f"  expected 2 m wall-vs-corridor-centre offset: {half_tile_ok}/{ok} passing doors")
    if counts:
        print("\nFailure breakdown:")
        for tag, n in counts.most_common():
            print(f"  {tag}: {n}")

    if failures:
        buckets: Dict[str, Counter] = defaultdict(Counter)
        for a in failures:
            e = a.err_emitted or a.err_wall
            key = f"({e[0]:+.0f},{e[1]:+.0f})"
            buckets[a.failure or "?"][key] += 1
        print("\nError vectors (door minus reference):")
        for tag, ctr in sorted(buckets.items()):
            top = ", ".join(f"{k}×{v}" for k, v in ctr.most_common(4))
            print(f"  {tag}: {top}")

    if verbose:
        sample = failures[:12] if failures else []
        if not sample and total_doors:
            # Show passing doors for single-seed verbose runs
            fm = gf.generate_map(
                seed=seed_start, cells=cells, max_rooms=rooms, loops=loops,
                hidden_area_prevalence=hidden, room_min=room_min, room_max=room_max,
            )
            if fm and fm.secret_doors:
                doc = gf.to_doc(fm, f"probe_{seed_start}")
                sample = [
                    audit_door(seed_start, i, fm, doc, meta, door)
                    for i, (door, meta) in enumerate(zip(fm.secret_doors, fm.secret_door_meta))
                ]
        if sample:
            print("\n--- sample audits ---")
            for a in sample:
                tag = a.failure or "ok"
                print(
                    f"seed={a.seed} door#{a.door_idx} {tag}\n"
                    f"  door=({a.door[0]:.1f},{a.door[1]:.1f}) wall=({a.wall_plane[0]:.1f},{a.wall_plane[1]:.1f}) "
                    f"center=({a.corridor_center[0]:.1f},{a.corridor_center[1]:.1f}) "
                    f"emitted={a.emitted}\n"
                    f"  meta=({a.room_cell},{a.corridor_cell}) match={a.matching_pair} "
                    f"adjacent={a.adjacent_link} best_adj={a.best_adjacent}\n"
                    f"  err_wall={a.err_wall} err_center={a.err_center} err_emitted={a.err_emitted}"
                )

    return 1 if failures else 0


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--seeds", type=int, default=300)
    ap.add_argument("--seed", type=int, default=None, help="Single seed (sets --seeds 1)")
    ap.add_argument("--cells", type=int, default=25)
    ap.add_argument("--rooms", type=int, default=11)
    ap.add_argument("--loops", type=int, default=3)
    ap.add_argument("--hidden", type=float, default=0.6)
    ap.add_argument("--room-min", type=int, default=3)
    ap.add_argument("--room-max", type=int, default=5)
    ap.add_argument("--verbose", action="store_true")
    args = ap.parse_args()

    seed_start = args.seed if args.seed is not None else 0
    n = 1 if args.seed is not None else args.seeds

    raise SystemExit(
        run_probe(
            n,
            cells=args.cells,
            rooms=args.rooms,
            loops=args.loops,
            hidden=args.hidden,
            room_min=args.room_min,
            room_max=args.room_max,
            verbose=args.verbose or args.seed is not None,
            seed_start=seed_start,
        )
    )


if __name__ == "__main__":
    main()
