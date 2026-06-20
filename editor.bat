@echo off
REM Kenney module editor. ALWAYS rebuilds so you are not running a stale fabled.exe.

taskkill /IM fabled.exe /F >nul 2>&1
timeout /t 1 /nobreak >nul

echo.
echo === Building editor (required after code changes) ===
cargo build
if errorlevel 1 (
    echo.
    echo BUILD FAILED. Fix errors above, then run editor.bat again.
    pause
    exit /b 1
)

echo.
echo === Starting fabled editor ===
echo Look for window title: "fabled editor [build ...]"
echo.

set RUST_BACKTRACE=1
target\debug\fabled.exe --host --editor %* 2>crash_log.txt
set EXIT=%errorlevel%

if %EXIT% neq 0 (
    echo.
    echo Editor exited with code %EXIT%.
    pause
)

exit /b %EXIT%
