@echo off

REM Play the Kenney map pool (userinput/maps/pool/) — syncs start map then builds.

taskkill /IM fabled.exe /F >nul 2>&1

timeout /t 1 /nobreak >nul

echo Syncing playtest layout from pool...
python -c "import json,sys; sys.path.insert(0,'tools'); import gen_maps as gm; gm.export_kenney_layout(json.load(open('userinput/maps/pool/map_001.json')))"

echo Building...

cargo build %*

if errorlevel 1 (

    echo Build failed.

    exit /b 1

)

target\debug\fabled.exe --host --test --kenney %*

