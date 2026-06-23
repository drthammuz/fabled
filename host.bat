@echo off
REM Kill a leftover game process so cargo can replace fabled.exe (Windows file lock).
taskkill /IM fabled.exe /F >nul 2>&1
timeout /t 1 /nobreak >nul
REM For full Kenney map editor: editor.bat  (or --host --editor)
REM For synth vignette sandbox only: dressing.bat  (or --host --dressing)
REM Test saved Kenney layout: testkenney.bat  (or --host --test --kenney)
REM --test = developer test map (bypasses class-select + procgen + hub).
REM   --test           = rusty procgen walls from userinput/wall_map.json
REM   --test --kenney  = same walls + Kenney GLB stairs/meshes
REM   --test --rusty   = explicit rusty (same as plain --test)
REM Remove --test to play the real game.
cargo run -- --host --test %*
