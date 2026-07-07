@echo off
REM Run placement optimization sweeps.
REM Default: 2min for EACH faction (priesthood, necropolis, urban, industrial, synth) = ~10min total.
REM Usage: s.bat          (all factions)
REM        s.bat necropolis (one faction only)

set F=%1
if "%F%"=="" (
  echo Running sweeps for all factions 2min each
  python tools/run_faction_placement_sweeps.py --faction priesthood --minutes 2
  python tools/run_faction_placement_sweeps.py --faction necropolis --minutes 2
  python tools/run_faction_placement_sweeps.py --faction urban --minutes 2
  python tools/run_faction_placement_sweeps.py --faction industrial --minutes 2
  python tools/run_faction_placement_sweeps.py --faction synth --minutes 2
) else (
  python tools/run_faction_placement_sweeps.py --faction %F% --minutes 2
)
echo Sweeps done. Review userinput/sweeps_*.log then re-gen in editor.