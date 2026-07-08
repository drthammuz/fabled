@echo off
setlocal
REM Build the game, package ONLY the files a player needs into a zip, and (if
REM the GitHub CLI is set up) upload it to your repo's "playtest" release so
REM friends always download the newest build from one link.
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
echo === Assembling %DEST%\ (only what the game needs) ===
if exist %DEST% rmdir /s /q %DEST%
mkdir %DEST%
copy /y target\release\fabled.exe %DEST%\ >nul
copy /y tools\client_play_template.bat %DEST%\PLAY.bat >nul

REM Runtime asset folders only — skips the big download/staging caches under
REM assets\ (downloads, _dl, quaternius_universal_conversion, staging) that the
REM game never loads.
for %%D in (models textures audio characters fonts shaders) do (
    robocopy assets\%%D %DEST%\assets\%%D /E /NFL /NDL /NJH /NJS /NP >nul
)
REM Drop the one oversized, unused model.
if exist %DEST%\assets\models\misc\cyberpunk_city.glb del /q %DEST%\assets\models\misc\cyberpunk_city.glb
REM Maps + saved layout the client reads at runtime.
robocopy userinput %DEST%\userinput /E /NFL /NDL /NJH /NJS /NP >nul

echo === Zipping to fabled_client.zip ===
if exist fabled_client.zip del /q fabled_client.zip
powershell -NoProfile -Command "Compress-Archive -Path '%DEST%\*' -DestinationPath 'fabled_client.zip' -Force"

echo.
where gh >nul 2>&1
if errorlevel 1 (
    echo -------------------------------------------------------------------
    echo The zip is ready but was NOT uploaded (GitHub CLI 'gh' not found).
    echo   The file is here:  %CD%\fabled_client.zip
    echo   Send that to your friends, OR set up one-command uploads ONCE:
    echo       winget install --id GitHub.cli
    echo       gh auth login
    echo   Then re-run pack_client.bat and it will upload automatically.
    echo -------------------------------------------------------------------
    pause
    exit /b 0
)

echo === Uploading to GitHub (release tag: playtest) ===
gh release view playtest >nul 2>&1
if errorlevel 1 (
    gh release create playtest fabled_client.zip --title "Playtest build" --notes "Latest playtest client. Download fabled_client.zip, unzip it, run PLAY.bat."
) else (
    gh release upload playtest fabled_client.zip --clobber
)
echo.
echo Done. Friends download the newest fabled_client.zip from your repo's
echo Releases page. Direct link:
for /f "delims=" %%U in ('gh release view playtest --json url -q .url 2^>nul') do echo   %%U
pause
