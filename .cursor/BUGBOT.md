# Bugbot review context (Fabled)

## Hub / extraction — read first

Before reviewing changes to Kenney hub geometry, editor G playtest, pit cutouts, or hatch pieces, read:

**[docs/hub-extraction-agent-failures.md](../docs/hub-extraction-agent-failures.md)**

Key paths:

- `crates/shared/src/kenney_pit.rs`, `kenney_hub.rs`
- `crates/client/src/editor_playtest.rs`, `test_showcase.rs`, `kenney_editor.rs`
- `crates/server/src/level.rs`, `character.rs`

## What to flag

1. **Visual vs physics split** — client `EditorPlaced` meshes vs server `play_layout(true)` colliders must stay aligned.
2. **`template-floor-hole.glb`** — do not despawn at floor 0 extraction without replacing the visible frame; hub −1 hatches cause diagonal wedge artifacts.
3. **Stairs opening `(6, 20)` on hub floor −1** — mesh cutouts must match floor mask / collider skips; playtest walk-back off stairs is **not** a valid fall test (step-up holds Y).
4. **Probe false PASS** — `tools/probe_hub_tile_audit.py` passing does not prove G playtest matches.

Do not treat hub/extraction as fixed unless probes **and** stated user acceptance agree.
