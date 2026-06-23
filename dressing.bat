@echo off
REM Synth vignette sandbox — minimal UI for authoring userinput/synth_dressing/*.json
REM (Not the full Kenney map editor — use editor.bat for that.)

taskkill /IM fabled.exe /F >nul 2>&1
timeout /t 1 /nobreak >nul

echo.
echo === Building dressing shell ===
cargo build
if errorlevel 1 (
    echo.
    echo BUILD FAILED. Fix errors above, then run dressing.bat again.
    pause
    exit /b 1
)

echo.
echo === Starting fabled dressing shell ===
echo Window title: "fabled dressing [build ...]"
echo File - New vignette / Save / Load  ^|  Actions - Add/Remove floor  ^|  Place props from sidebar
echo.

set RUST_BACKTRACE=1
target\debug\fabled.exe --host --dressing %* 2>crash_log.txt
set EXIT=%errorlevel%

if %EXIT% neq 0 (
    echo.
    echo Dressing shell exited with code %EXIT%.
    pause
)

exit /b %EXIT%
