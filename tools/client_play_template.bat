@echo off
REM This file is copied into the shared client folder as PLAY.bat by pack_client.bat.
title Fabled
echo.
echo   FABLED - join a game
echo.
set /p IP=Type the host's IP address, then press Enter:
echo.
echo Connecting to %IP% ...
fabled.exe --client %IP% 2>crash_log.txt
if %errorlevel% neq 0 (
    echo.
    echo The game closed with an error. See crash_log.txt in this folder.
    pause
)
