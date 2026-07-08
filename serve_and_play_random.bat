@echo off
REM Real game, serve + play in one process (fullscreen), starting on a RANDOM
REM pool map. Friends join with join.bat <your-ip>. Build+run from cmd.exe
REM (Git Bash's link.exe breaks the final link).

REM Kill any still-running instance so the linker can overwrite fabled.exe
REM (otherwise: "error: failed to remove file ... fabled.exe: Access is denied").
taskkill /F /IM fabled.exe >nul 2>&1

echo === Building (release) ===
cargo build --release
if errorlevel 1 (
    echo BUILD FAILED.
    pause
    exit /b 1
)

echo === Hosting on UDP port 5000 (random start map) ===
set RUST_BACKTRACE=1
target\release\fabled.exe --host --start-map random %* 2>crash_log.txt
if %errorlevel% neq 0 (
    echo Exited with code %errorlevel% - see crash_log.txt / logs\panic.log
    pause
)
