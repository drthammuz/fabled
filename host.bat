@echo off
REM Kill a leftover game process so cargo can replace fabled.exe (Windows file lock).
taskkill /IM fabled.exe /F >nul 2>&1
timeout /t 1 /nobreak >nul
cargo run -- --host %*
