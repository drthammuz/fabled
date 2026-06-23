#!/usr/bin/env python3
"""Red-flag audit for synth dressing JSON — run before asking a human to look.

Catches the editor-visible failures (sunk beds, stacked stairs, balconies that don't
touch the building, mezz deck/stair height mismatches). Exit 0 = safe to load.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
import synth_interior as si  # noqa: E402
import verify_synth_placement as vsp  # noqa: E402

DECK = si.CATALOG.get("deck_y", 1.2)


def audit_doc(doc: dict, label: str) -> list[str]:
    # The verifier already compares against the geometry source of truth; surface its
    # findings as red flags. Anything it reports would be visibly wrong in the editor.
    flags = list(vsp.verify_doc(doc, label))
    return [f if f.startswith(("RED FLAG", "STACKED")) else f"RED FLAG {f}" for f in flags]


def main() -> None:
    paths = [Path(a) for a in sys.argv[1:]] or [
        ROOT / "userinput" / "synth_dressing" / "interior_showcase.json",
    ]
    failed = False
    for path in paths:
        if not path.is_file():
            print(f"skip missing {path}")
            continue
        doc = json.loads(path.read_text(encoding="utf-8"))
        flags = audit_doc(doc, path.stem)
        if flags:
            failed = True
            print(f"AUDIT FAIL {path.name} ({len(flags)} red flags):")
            for f in flags[:50]:
                print(f"  - {f}")
            if len(flags) > 50:
                print(f"  ... +{len(flags) - 50} more")
        else:
            print(f"AUDIT OK   {path.name}")
    raise SystemExit(1 if failed else 0)


if __name__ == "__main__":
    main()
