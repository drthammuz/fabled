# Handover — Pass F round 1: usable in-world terminals (+ mini-market swap) — 2026-07-07

Session scope (user: "npc interactions good enough for now. i put the market zip
in assets/downloads. next is F"): Pass D closed as approved, mini-market stall
dressing swapped in, and all four Pass F spec items landed. Full `cargo build`
green, `cargo test -p client terminal_shell` 12/12, `tools/test_agent_spawns.py`
250/250 (50 seeds × 5 faction mixes). Awaiting human test — checklist in
docs/TODO.md, Pass F section.

## 1. Mini-market stall swap (P5, was blocked on the zip)

- Unzipped to `assets/models/mini_market/` (20 GLBs + `Textures/colormap.png`
  — the relative `Textures/` subfolder is load-bearing, same as every Kenney kit).
- `tools/gen_freeform.py :: _dress_hub_and_hidden` (~line 2011): the `prop()`
  helper now takes `kit=`/`scale=`. Hub stall per placed cell = retro-urban
  `detail-awning-wide` (kept — the spawn-gate keys on this stem + `hub_market`
  tag) + mini-market `cash-register` as the counter (scale **2.0**) +
  `shelf-end`, `display-fruit`, `display-bread` (scale **2.5**). Hidden shack:
  awning-small + `display-fruit` + `shopping-cart`.
- Scale rationale: mini-market is a 1-unit kit but its props are chunky
  (cash-register 0.85 units square → ×2.5 would be a 2.1 m register; ×2.0 counter
  ≈ 1.7 m, human-ish). Blanket kit ×4 would be absurd. **Needs user eyeball.**

## 2. Screen-quad probe → `assets/models/screen_catalog.json`

`tools/probe_screen_quads.py` — pure-Python GLB parser (JSON chunk + BIN,
node-tree transforms, accessor decode), no Blender. Finds the flat SCREEN
rectangle per prop: triangles grouped by quantized plane → planar regions scored
by area, fill ratio, verticality, and darkness (samples the kit colormap atlas
through the UVs with PIL; materials named dark/black/screen count as dark).

Generic scoring alone picks monitor BACKS and casings, so per-stem `KEEP_RULES`
curate: `"best1"` (default), `"sym4"` (table-display/-small: 4 symmetric faces),
`"outer_x2"` (computer-system: two outward monitors), `None` (exclude —
table-display-planet has no flat screen), `"mat:dark"` (space_kit desks),
`"mat:metalDark"` (furniture/computerScreen — that stem's SCREEN is metalDark
and its casing is plain metal, the inverse of everything else).

Output: 26 stems (8 synth, 8 factory, 6 space_station, 1 furniture, 3
space_kit). Keys are `{kit}/{stem}` matching runtime
`KenneyModule.kit.unwrap_or("space")` + name. Quads are GLB-local
`{center, normal, up, size_wh}` — runtime multiplies by module scale.
Re-run the tool after adding kits; it prints per-stem winners for review.

## 3. Runtime: `crates/client/src/terminal.rs` (TerminalPlugin)

- **attach_screens** on `Added<KenneyModule>` → one system covers editor,
  playtest and real runs (standing rule: playtest == run). Child entity =
  shared 1×1 `Rectangle` mesh, Transform at `center + normal*0.003`, rotation
  `Quat::from_mat3(right/up/normal)`, scale `(w, h, 1)`, `NotShadowCaster`,
  idle emissive material (pre-tinted green texture, emissive ×2.2).
- **Prompt/open** (`prompt_and_open`, `.before(netplay::send_input)`): within
  2.8 m + facing (dot 0.88) + not captured + alive → "[E] terminal" (bottom
  150 px; dialogue's sits at 120 px). **NPC priority**: if an NPC is within
  3.2 m/dot 0.92 the terminal ignores E (server opens dialogue on that press).
  On E: eat the key, seed a `Shell` from world position, swap the quad to the
  live material, wake the offscreen camera, set `netplay::InputCapture`.
- **Live screen**: offscreen `Camera2d` (order −1, `is_active=false` when
  closed, `RenderLayers` layer 3) + `Text2d` (Bevy's default font is a Fira
  Mono subset — already monospace) → 512×320 `Image` (RENDER_ATTACHMENT) used
  as `emissive_texture`. `update_screen_text` copies `shell.screen_text()` in.
- **focus_camera** in `PostUpdate.before(TransformSystems::Propagate)` — the
  Update-schedule camera drivers in fly_camera.rs are private, so the terminal
  takes the last word instead of fighting them. Smoothstep blend (Local<f32>,
  ±time*3), fit distance from the camera `Projection` fov vs quad w/h.
- **terminal_keys**: `MessageReader<KeyboardInput>` Pressed events →
  `Key::Character/Space/Backspace/Enter`; Esc closes (eaten via
  `clear_just_pressed` so fly_camera doesn't ungrab the cursor). `auto_close`
  on death or range > 3.5 m. Close = idle material back, camera asleep,
  InputCapture cleared.
- **Bevy 0.18 gotchas hit**: `Camera` has NO `target` field — `RenderTarget`
  is a separate component spawned next to it (`bevy_camera` 0.18);
  `NotShadowCaster` moved to `bevy::light` (out of `bevy::pbr`).

## 4. Shell: `crates/client/src/terminal_shell.rs`

Pure logic, zero Bevy imports, 12 unit tests. `Shell::new(kit, seed, cols=54,
rows=17)`. Hostname per faction: synth → `synth-node-NN`, factory →
`plant-ctl-NN`, else `term-NN` (NN = seed%90+10, seeded from terminal world
pos → same terminal always the same box). In-memory fs: 10 dirs, 9 flavor
files (`/etc/motd`, `/etc/passwd`, `/home/operator/notes.txt` + `todo.txt`,
`/var/log/incident.log` + `door.log`, `/sys/doors/manifest`,
`/sys/reactor/status`, `/bin/sh`). Commands: help ls cd cat pwd echo clear
whoami hostname uname exit. ASCII-printable input only, width-capped; output
hard-wraps at cols; scrollback capped 200; `screen_text()` = last rows−1 lines
+ `user@host:dir$ input█`.

## 5. Where round 2 goes

Gameplay hooks: wire `/sys/doors/manifest` + an `unlock <door>` command to the
server door seals (needs a PlayerInput field, validated server-side like
trade_buy); per-terminal persistent fs; more commands (grep/head/ps); CRT
scanline shader on the live material. All listed under Pass F in TODO.md.

## 6. Verification status

| Gate | Result |
|---|---|
| `cargo build` (full workspace, PowerShell) | exit 0 |
| `cargo test -p client terminal_shell` | 12/12 |
| `tools/test_agent_spawns.py` (50 seeds × 5 mixes) | 250/250 |
| Human test | PENDING — checklist in TODO.md Pass F |

## 7. Round 2 (2026-07-07, human test feedback)

User (editor → synth as last faction → G, seed 1 outlaw/industrial/synth):
**no mini-market GLBs in the hub, and no screen interactive anywhere** —
e.g. the mezz `computer-screen` at (-40, 3, 30). Two independent root causes,
both data-side (NO Rust changes; editor.bat's build-on-launch picks them up):

1. **Hub stalls 4 m above the hub floor.** `PieceRecord.y` is an ABSOLUTE
   world-height override, but `gen_freeform._dress_hub_and_hidden::prop()`
   wrote `y: 0.05` for stalls on hub floor -1 (surface at -4.0, same constant
   as the keeper spawns). They spawned inside the ground-level geometry — hub
   showed keeper NPCs but no stalls. Fix: `y = floor * 4.0 + 0.05`. All 44
   saved maps (kenney_layout.json, _editor_preview.json, 42 stream children)
   patched in place: 660 pieces, y 0.05 → -3.95.

2. **Screen quads glued to the monitors' BACK panels.** The probe scores
   "dark = glass", which is right for the factory kit (navy glass) but
   INVERTED on the space-station colormap: its screen swatch is bright
   ORANGE rgb≈(255,180,72); the grey-blue back casing (lum 115) beat the
   orange front (lum 188). Every synth/space_station pick landed on the back
   face → the emissive quad rendered INTO the prop/wall (single-sided, so no
   green glow anywhere) and `prompt_and_open`'s front-side check
   (`normal.dot(player - pos) > 0`) rejected the player from the playable
   side. Also wrong: `outer_x2` on computer-system kept two blank ±x cabinet
   sides (its real screen is a small 0.3×0.24 angled console face, n=(0,.71,.71));
   furniture/computerScreen's screen is its light `metal` face, not
   `metalDark`. **Every pick verified by Blender headless renders from
   ±quad-normal** (contact sheets; script pattern in scratchpad, re-derivable).
   New probe rule `"orange"` (screen-swatch match) for all synth/space_station
   stems; factory/space_kit confirmed correct as-is. Catalog regenerated
   (26 stems, schema unchanged). screen_catalog.json is include_str!'d →
   needs the rebuild that editor.bat does anyway.

Verification round 2: `tools/test_agent_spawns.py` ALL PASS after the gen
change; regenerated seed-1 preview shows hub_market y = -3.95; catalog
front renders all show the actual screen face. Human re-test: same checklist
as §6, plus "stalls visible in hub".
