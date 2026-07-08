@echo off
REM Builds the game and assembles a share-ready CLIENT folder in client_dist\.
REM A friend can run that folder WITHOUT installing Rust or cloning the repo:
REM they just unzip it and double-click PLAY.bat.
REM
REM Run this from cmd.exe in the repo root (Git Bash's link.exe breaks the build).

taskkill /F /IM fabled.exe >nul 2>&1

echo === Building the game (release) ===
cargo build --release
if errorlevel 1 (
    echo BUILD FAILED.
    pause
    exit /b 1
)

set DEST=client_dist
echo === Assembling %DEST%\ ===
if exist %DEST% rmdir /s /q %DEST%
mkdir %DEST%
copy /y target\release\fabled.exe %DEST%\ >nul
xcopy /e /i /y /q assets %DEST%\assets >nul
xcopy /e /i /y /q userinput %DEST%\userinput >nul
copy /y tools\client_play_template.bat %DEST%\PLAY.bat >nul

echo.
echo Done. Now:
echo   1. Right-click the '%DEST%' folder -^> Send to -^> Compressed (zipped) folder.
echo   2. Send that .zip to your friend (Discord, WeTransfer, etc.).
echo   3. They unzip it anywhere and double-click PLAY.bat.
echo.
pause
