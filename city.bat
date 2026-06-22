@echo off
REM Fly around cyberpunk_city.glb with daylight (no editor / no server).
taskkill /IM fabled.exe /F >nul 2>&1
timeout /t 1 /nobreak >nul
echo Building...
cargo build
if errorlevel 1 exit /b 1
target\debug\fabled.exe --city %*
