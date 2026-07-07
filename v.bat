@echo off
REM AI verification tools (showcase + placement checks).
REM You can run it, but main human review is in editor + G playtest.
set F=%1
if "%F%"=="" set F=necropolis
python tools/gen_dressing_showcase.py
python tools/verify_synth_placement.py userinput/synth_dressing/*.json
echo AI verify done. Use editor/G for your eyes.