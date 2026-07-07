@echo off
REM Main user cycle:
REM 1. AI should have run p + script edits.
REM 2. You run this for sweeps (all factions, 2min each).
REM 3. Then use editor's Proc tab to generate multiple seeds.
REM 4. Human review in editor + G playtest.
echo Running full sweep cycle all factions 2min each
call s.bat
echo.
echo Sweeps complete.
echo Next: open editor and use the Proc tab to generate + test multiple seeds across factions.
echo After edits to placement logic, re-run this to sweep again.
echo.