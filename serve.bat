@echo off
REM Dedicated headless server (no window) on UDP 5000. Join it from another
REM prompt with join.bat (127.0.0.1) or from friends' PCs with join.bat <ip>.

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

echo === Dedicated server on UDP port 5000 (Ctrl+C to stop) ===
echo Loads the real faction pool map when userinput/maps/pool/index.json
echo exists (bakes GLB colliders itself via raw glTF parsing — no window,
echo no AssetServer needed). Falls back to the legacy sewer game otherwise.
set RUST_BACKTRACE=1
target\release\fabled.exe --server 2>crash_log_server.txt
if %errorlevel% neq 0 (
    echo Exited with code %errorlevel% - see crash_log_server.txt
    pause
)
