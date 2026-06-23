#!/usr/bin/env python3
"""Verify synth dressing JSON against the generator's own geometry source of truth.

A passing run means the JSON matches `synth_interior.expected_balcony_floors` and
`mezzanine_plan` exactly, beds sit on the deck, and no stairs are stacked.
"""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
import synth_interior as si  # noqa: E402

DECK = si.CATALOG.get("deck_y", 1.2)
TOL = 0.2


def load_doc(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def walkable_top(p: dict) -> float:
    y = float(p.get("y", DECK))
    return y + si.bounds_scaled(p["stem"], p.get("scale", si.SCALE))["y1"]


def check_beds(pieces: list[dict]) -> list[str]:
    errors: list[str] = []
    for p in pieces:
        if p.get("stem") not in ("bed-single", "bed-double"):
            continue
        b = si.bounds_scaled(p["stem"], p.get("scale", si.SCALE))
        bottom = float(p.get("y", DECK)) + b["y0"]
        if abs(bottom - DECK) > TOL:
            errors.append(
                f"bed not on deck: {p['stem']}@({p['x']},{p['z']}) bottom {bottom:.2f} != deck {DECK}"
            )
    return errors


def check_balconies(doc: dict, floor_ix, corridor_ix) -> list[str]:
    errors: list[str] = []
    expected = si.expected_balcony_floors(floor_ix, corridor_ix)
    placed: dict[tuple[float, float], dict] = {}
    for p in doc.get("pieces", []):
        if str(p.get("stem", "")).startswith("balcony-floor"):
            placed[(round(p["x"], 1), round(p["z"], 1))] = p

    for key, (stem, yaw) in expected.items():
        p = placed.get(key)
        if p is None:
            errors.append(f"balcony missing at {key} (expected {stem})")
            continue
        if p["stem"] != stem:
            errors.append(f"balcony stem at {key}: got {p['stem']} want {stem}")
        if abs(float(p.get("yaw", 0.0)) - yaw) > 0.05:
            errors.append(f"balcony yaw at {key}: got {p.get('yaw'):.2f} want {yaw:.2f}")
        top = walkable_top(p)
        if abs(top - DECK) > TOL:
            errors.append(f"balcony height at {key}: top {top:.2f} != {DECK}")
    for key in placed:
        if key not in expected:
            errors.append(f"balcony unexpected at {key} ({placed[key]['stem']})")
    return errors


def _rail_span(p: dict) -> tuple[float, float, float, float, str]:
    """(x0, z0, x1, z1, axis) for a rail piece; axis 'x' spans x, 'z' spans z."""
    half = si.bounds_scaled(p["stem"], p.get("scale", si.SCALE))["x1"]
    yaw = float(p.get("yaw", 0.0))
    x, z = float(p["x"]), float(p["z"])
    if abs(math.cos(yaw)) > 0.5:  # local +x stays along world x
        return x - half, z, x + half, z, "x"
    return x, z - half, x, z + half, "z"


def check_rail_crossings(pieces: list[dict]) -> list[str]:
    """Universal red flag: a horizontal and a perpendicular rail must not cross.

    A butt joint at a corner (rails merely touching) is fine; this only flags rails that
    each overshoot the intersection by > 0.5 m on both sides — the inner-corner bug.
    """
    rails = [p for p in pieces if "balcony_rail" in (p.get("tags") or [])]
    spans = [_rail_span(p) for p in rails]
    hor = [(x0, z0, x1, z1) for x0, z0, x1, z1, a in spans if a == "x"]
    ver = [(x0, z0, x1, z1) for x0, z0, x1, z1, a in spans if a == "z"]
    m = 0.5
    errors: list[str] = []
    for hx0, hz, hx1, _hz2 in hor:
        for vx, vz0, _vx2, vz1 in ver:
            if hx0 + m < vx < hx1 - m and vz0 + m < hz < vz1 - m:
                errors.append(
                    f"RAILS CROSS near ({round(vx, 1)},{round(hz, 1)}): "
                    "perpendicular rails overshoot a corner"
                )
    return errors


def check_balcony_rails(doc: dict, floor_ix, corridor_ix) -> list[str]:
    expected = si.expected_balcony_rails(floor_ix, corridor_ix)
    placed = [
        (round(p["x"], 2), round(p["z"], 2), round(float(p.get("yaw", 0.0)), 2), p["stem"])
        for p in doc.get("pieces", [])
        if "balcony_rail" in (p.get("tags") or [])
    ]
    exp_set = {(round(x, 2), round(z, 2), round(y, 2), s) for x, z, y, s in expected}
    got_set = set(placed)
    errors: list[str] = []
    missing = exp_set - got_set
    extra = got_set - exp_set
    for x, z, y, s in sorted(missing)[:8]:
        errors.append(f"balcony rail missing: {s}@({x},{z}) yaw {y}")
    for x, z, y, s in sorted(extra)[:8]:
        errors.append(f"balcony rail unexpected: {s}@({x},{z}) yaw {y}")
    return errors


def check_stacked_stairs(pieces: list[dict]) -> list[str]:
    """Universal red flag: two stair tiles must never share an (x, z) footprint."""
    errors: list[str] = []
    seen: dict[tuple[float, float], float] = {}
    for s in pieces:
        if s.get("stem") != "stairs":
            continue
        k = (round(s["x"], 1), round(s["z"], 1))
        if k in seen:
            errors.append(
                f"STACKED stairs at {k}: y={seen[k]} and y={s.get('y')} (must advance one cell)"
            )
        seen[k] = float(s.get("y", 0.0))
    return errors


def check_mezzanine(doc: dict, floor_ix, corridor_ix) -> list[str]:
    errors: list[str] = []
    pieces = doc.get("pieces", [])
    mezz_floors = [p for p in pieces if "mezz_floor" in (p.get("tags") or [])]
    stairs = [p for p in pieces if p.get("stem") == "stairs" and "indoor_stairs" in (p.get("tags") or [])]
    if not mezz_floors and not stairs:
        return errors

    name = str(doc.get("name", ""))
    if "showcase" not in name:
        return errors  # only the showcase has a known command room to compare against

    floor_ix2, corridor_ix2, rooms = si.generate_showcase_floor_plan(int(doc.get("seed", 42)))
    roles = si.assign_roles(rooms, corridor_ix2)
    infos = si.build_room_infos(rooms, corridor_ix2, roles)
    command = next((i for i in infos if i.role == "command"), None)
    if command is None:
        return errors
    plan = si.mezzanine_plan(command)
    if plan is None:
        if mezz_floors or stairs:
            errors.append("mezzanine present but plan says it should not fit")
        return errors

    # Stair bases must climb DECK_Y, DECK_Y*2, ... starting from the ground surface.
    want_bases = sorted(b for _, _, b in plan["stairs"])
    got_bases = sorted(round(float(s.get("y", 0.0)), 2) for s in stairs)
    if [round(b, 2) for b in want_bases] != got_bases:
        errors.append(f"stair bases {got_bases} != expected {[round(b,2) for b in want_bases]}")
    if any(float(s.get("y", 0.0)) < DECK - TOL for s in stairs):
        errors.append("a stair flight is below the ground surface (buried in the floor)")

    # Deck must sit at the plan height, flush with the top flight.
    for p in mezz_floors:
        top = walkable_top(p)
        if abs(top - plan["deck_top"]) > TOL:
            errors.append(f"mezz deck top {top:.2f} != plan {plan['deck_top']:.2f}")
            break
    top_stair = max((float(s.get("y", 0.0)) for s in stairs), default=0.0) + DECK
    if stairs and abs(top_stair - plan["deck_top"]) > TOL:
        errors.append(f"top stair reaches {top_stair:.2f} != deck {plan['deck_top']:.2f}")
    return errors


def floor_context(doc: dict) -> tuple[set, set]:
    name = str(doc.get("name", ""))
    seed = int(doc.get("seed", 42))
    if "showcase" in name:
        fix, cor, _ = si.generate_showcase_floor_plan(seed)
        return fix, cor
    if "rating" in name:
        _, cor, _ = si.generate_rating_floor_plan(seed)
        fix = {
            (int(round(p["x"] / si.CELL)), int(round(p["z"] / si.CELL)))
            for p in doc.get("pieces", [])
            if p.get("stem") == "floor" and float(p.get("y", 0.0)) <= 0.01
        }
        return fix, cor
    fix = {
        (int(round(p["x"] / si.CELL)), int(round(p["z"] / si.CELL)))
        for p in doc.get("pieces", [])
        if p.get("stem") == "floor" and float(p.get("y", 0.0)) <= 0.01
    }
    return fix, set()


def verify_doc(doc: dict, label: str) -> list[str]:
    pieces = doc.get("pieces", [])
    errors: list[str] = []
    # Universal red flags (apply to every dressing doc, hand-authored or generated).
    errors.extend(check_beds(pieces))
    errors.extend(check_stacked_stairs(pieces))
    errors.extend(check_rail_crossings(pieces))
    errors.extend(si.validate_props(pieces, label))
    # Generator-rule comparisons only where we can reconstruct the procedural floor plan.
    name = str(doc.get("name", ""))
    if "showcase" in name:
        floor_ix, corridor_ix = floor_context(doc)
        errors.extend(check_balconies(doc, floor_ix, corridor_ix))
        errors.extend(check_balcony_rails(doc, floor_ix, corridor_ix))
        errors.extend(check_mezzanine(doc, floor_ix, corridor_ix))
    return errors


def main() -> None:
    paths = [Path(a) for a in sys.argv[1:]] or [
        ROOT / "userinput" / "synth_dressing" / "interior_showcase.json",
        ROOT / "userinput" / "synth_dressing" / "interior_rating_20.json",
        ROOT / "userinput" / "synth_dressing" / "bunk_furnished_c.json",
    ]
    failed = False
    for path in paths:
        if not path.is_file():
            print(f"skip missing {path}")
            continue
        errors = verify_doc(load_doc(path), path.stem)
        if errors:
            failed = True
            print(f"FAIL {path.name} ({len(errors)} issues):")
            for e in errors[:40]:
                print(f"  - {e}")
            if len(errors) > 40:
                print(f"  ... +{len(errors) - 40} more")
        else:
            print(f"OK   {path.name}")
    raise SystemExit(1 if failed else 0)


if __name__ == "__main__":
    main()
