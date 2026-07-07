@echo off
REM AI-only: re-probes all catalogs from GLBs.
REM Run this yourself only if you added/edited new GLBs.
python tools/probe_faction_catalog.py
echo Catalog refreshed. AI will review/edit purposes/relations.