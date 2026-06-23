# Synth dressing vignettes

User-authored and generator-produced room layouts for synth interior calibration.

Save from the editor (**Mode: Dressing** → File → Save). Each JSON holds placed
`PieceRecord` entries (`kit: factions/synth`, scale 4) on a 40×40 cell grid.

Launch: **`dressing.bat`** (rebuilds client; window title shows `EDITOR_BUILD_TAG`).

## Generated maps (regenerate + audit)

| File | Generator | Notes |
|------|-----------|-------|
| `interior_showcase.json` | `python tools/gen_dressing_showcase.py` | 10-room wing; perimeter balconies + command mezzanine |
| `interior_rating_20.json` | `python tools/gen_dressing_rating_map.py` | 20 furnished rooms + corridors for role rating |
| `balcony_test.json` | `python tools/gen_balcony_test.py` | Piece orientation reference + L-room concave-corner rails |

After regen, audits run automatically in the generator scripts. Manual check:

```bash
python tools/verify_synth_placement.py userinput/synth_dressing/*.json
python tools/audit_synth_scene.py userinput/synth_dressing/interior_showcase.json
```

Placement rules: `tools/synth_interior.py` (`expected_balcony_floors`, `expected_balcony_rails`, `mezzanine_plan`).
Catalog: `assets/models/factions/synth/placement_catalog.json` (bounds + **orientation** per stem).

## Hand-tuned reference vignettes

- `bunk_furnished_c`, `lab_furnished_c`, `office_furnished_c` — validated singles
- `template_*` — empty shells (`python tools/gen_dressing_templates.py`)
- `bunk.json`, `lab.json`, … — work-in-progress manual layouts

Suggested vignettes to author manually:

- `bunk` — small quarters (bed + storage)
- `lab` — workstation row + wall display
- `mess` — table + chairs
- `storage` — containers in a dead-end
- `corridor_alcove` — wall switch / detail only, no floor clutter
