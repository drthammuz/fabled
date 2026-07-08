@echo off
REM Join a server:  join.bat <ip>     (default 127.0.0.1 for a local server)
REM Fullscreen real-game client. Build+run from cmd.exe, not Git Bash.

set IP=%1
if "%IP%"=="" set IP=127.0.0.1

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

echo === Joining %IP% ===
set RUST_BACKTRACE=1
target\release\fabled.exe --client %IP% 2>crash_log.txt
if %errorlevel% neq 0 (
    echo Exited with code %errorlevel% - see crash_log.txt / logs\panic.log
    pause
)
