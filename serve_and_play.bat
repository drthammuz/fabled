@echo off
REM Real game, serve + play in one process (fullscreen). Friends join with join.bat <your-ip>.
REM Build+run from cmd.exe (Git Bash's link.exe breaks the final link).

echo === Building (release) ===
cargo build --release
if errorlevel 1 (
    echo BUILD FAILED.
    pause
    exit /b 1
)

echo === Hosting on UDP port 5000 ===
set RUST_BACKTRACE=1
target\release\fabled.exe --host %* 2>crash_log.txt
if %errorlevel% neq 0 (
    echo Exited with code %errorlevel% - see crash_log.txt / logs\panic.log
    pause
)
