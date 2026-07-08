@echo off
REM One-shot build (use when code changed; avoids cargo lock while editor is open).
REM Kill any running instance first so the linker can overwrite the exe
REM (otherwise: "error: failed to remove file ... fabled.exe: Access is denied").
taskkill /F /IM fabled.exe >nul 2>&1
cargo build %*
