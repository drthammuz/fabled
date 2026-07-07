@echo off
REM Dedicated headless server (no window) on UDP 5000. Join it from another
REM prompt with join.bat (127.0.0.1) or from friends' PCs with join.bat <ip>.

echo === Building (release) ===
cargo build --release
if errorlevel 1 (
    echo BUILD FAILED.
    pause
    exit /b 1
)

echo === Dedicated server on UDP port 5000 (Ctrl+C to stop) ===
set RUST_BACKTRACE=1
target\release\fabled.exe --server 2>crash_log_server.txt
if %errorlevel% neq 0 (
    echo Exited with code %errorlevel% - see crash_log_server.txt
    pause
)
