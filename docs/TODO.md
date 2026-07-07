# Fabled — persistent backlog

Prioritized work list maintained by Claude across sessions (user gets ~5 prompts/week —
this is the memory of what's deferred and why). Update it every round: remove done items,
add newly discovered ones, keep priorities honest.

**Delegation key:** `[S]` = a Sonnet/Opus-class model can do this well with the pointers
given (mechanical, well-scoped, verifiable by a script/test); `[F]` = needs Fable-level
debugging/design judgment. Anything with a listed verification gate is safer to delegate.

## Roadmap (reordered 2026-07-07 per plan4)

Current order: **F round 2 (after human test)**, then the POSTPONED section at the
bottom of this file (Pass C leftovers incl. the Tab map, then Pass E) — user
decision 2026-07-07. Passes A (combat core), B (third person) and **D (NPC
dialogue/trade — user verdict 2026-07-07: "good enough for now")** are **DONE**.
Pass F round 1 landed 2026-07-07 (below), awaiting human test. Full descriptions
of what landed live in git history and docs/handover-*.md.

### DONE — Pass A + Pass B (user-approved 2026-07-07), plus the aim fix

The one item out of the final A/B round — fire origin / crosshair convergence —
landed 2026-07-07 and is the ONLY untested piece left here:
- Problem: first person fired exactly from the eye (tracer leaves the camera lens,
  looks unnatural); third person fired down the EYE ray while the camera sits
  0.55 m right of it, so close/mid shots landed LEFT of the crosshair.
- Fix (the standard TPS/FPS approach): the client casts the CAMERA ray through
  screen center every frame (`fly_camera::CrosshairAim`, wall-avoided camera,
  own capsule + sensors excluded) and ships the hit point in `PlayerInput.aim`;
  the server fires from a right-hand MUZZLE (eye + 0.22 right, 0.25 down,
  0.45 fwd in look-space, `combat::PLAYER_MUZZLE_OFFSET`) toward that point
  (`aim_direction`, falls back to the raw yaw/pitch ray on degenerate/hostile
  input; unit-tested). Bat swings use the same direction, so third-person melee
  also hits what's under the crosshair.
- **HUMAN CHECK (quick):** third person, pistol, enemy at 3–8 m: shots land ON
  the crosshair (used to miss left). First person: tracer starts low-right like
  a handheld gun, not from the eye. Point-blank wall: no self-hit weirdness.

### DONE — Pass D — friendly NPCs + hub market (user-approved 2026-07-07)

**Landed and approved:**
- **E on an NPC opens a dialogue window** (client/src/dialogue.rs, styled on
  ui_theme): NPC name + flavor greeting, options **[1] Trade / [2] Ask around
  (rumor line) / [Esc] Leave**. A cyan "[E] talk" prompt shows when facing an
  NPC within ~3 m. NPCs have stable identities (name/greeting/rumor rolled at
  spawn, server/src/npc.rs).
- **Trade window**: BUY column (bat 10c, flashlight 12c, sector map 20c, medbag
  30c, scrap pistol 45c) and SELL column (your inventory at half value, shared
  `items::sell_price` so UI and server always agree). Arrows navigate, ←/→
  switch columns, Enter confirms, Esc backs out. Wallet = party credits on
  RunState (same as hub shop); footer shows it live.
- **Server-authoritative, stateless**: buy/sell travel as
  `PlayerInput.trade_buy/trade_sell` and are validated against live NPC
  proximity (4.5 m) — no dialogue session state to desync. Map purchase
  respects the one-map-holder rule.
- **Input capture** (`netplay::InputCapture`): while a dialogue is open, look
  is frozen, movement/attack/hotbar/Tab are suppressed, crosshair hidden, and
  Esc closes the window instead of releasing the cursor. Pass F terminals
  should reuse this resource.
- **Dev wallet**: editor/test modes start with 60 credits so trading is
  testable in playtest immediately.
- **NPC counts are RULES now** (slider removed from the editor sidebar +
  GenKnobs): gen_freeform places 1–2 keepers per hidden room, and in the hub
  THREE market stalls (spaced ≥3 cells, wall-adjacent) each with a keeper NPC
  + 1–2 wanderers. Gate: test_agent_spawns.py validates all of it (50/50 seeds
  green).
- **Mini-market dressing swap landed 2026-07-07** (zip provided by user):
  kit unzipped to `assets/models/mini_market/`, hub stalls now awning +
  cash-register counter (×2.0) + shelf-end + fruit/bread displays (×2.5),
  hidden shacks get fruit display + shopping cart. Gate green: 250/250
  test_agent_spawns runs. **HUMAN EYEBALL still wanted:** stall scale/yaws
  in a playtest hub (props sized for humans, facing the room?).
  2026-07-07 later: first eyeball found NO stalls at all — they spawned 4 m
  above the hub floor (absolute-y bug, fixed; see Pass F round 2 below).

**Later rounds:** per-faction stock, scrap as second currency, NPCs face you
while talking, dialogue trees with real hooks (quests/door codes).

### Pass F — usable in-world terminals — round 2 landed 2026-07-07, awaiting human re-test

**Round 2 (human test feedback 2026-07-07: "no mini-market GLBs in the hub,
no screen is interactive"):** two root causes found and fixed, NO Rust changes —
next editor.bat launch rebuilds and picks both up.
- **Hub stalls spawned 4 m above the hub** (inside the ground-level floor, so
  the hub showed keepers but no stalls): `PieceRecord.y` is an ABSOLUTE world
  height, but `gen_freeform._dress_hub_and_hidden` wrote a flat `y: 0.05` for
  props on hub floor -1 (surface at -4.0). Fixed to `floor * 4.0 + 0.05`;
  patched the 15 stall pieces in all 44 saved maps (kenney_layout.json,
  _editor_preview.json, stream children) in place — no regeneration needed.
- **Screen quads were glued to the monitors' BACK panels** on every
  synth/space_station stem (+ the furniture monitor): the probe's "dark =
  glass" score inverted on the space-station colormap, whose screen swatch is
  BRIGHT orange (rgb≈255,180,72) — the grey-blue back casing won. So the glow
  faced the wall and the terminal's front-side check rejected the player from
  the playable side (e.g. the mezz computer-screen at seed 1 (-40, 3, 30)).
  Verified every pick by Blender-rendering each GLB from ±quad-normal; new
  probe rule "orange" keys on the screen swatch, computer-system now uses its
  small angled console screen (outer_x2 ±x picks were blank cabinet sides),
  furniture/computerScreen keeps its light 'metal' face ('metalDark' was the
  back). factory + space_kit picks were verified correct as-is.
  screen_catalog.json regenerated (26 stems) — it is include_str!'d, hence the
  rebuild-on-next-launch note above.

### Pass F round 1 — landed 2026-07-07 (human test round 1 failed, see round 2)

**Landed (all four spec items):**
- **Screen probe** (`tools/probe_screen_quads.py`, pure-Python GLB parser — no
  Blender needed): finds the flat dark screen quad per prop via planar-region
  analysis + UV→colormap darkness sampling + per-stem KEEP_RULES curation →
  `assets/models/screen_catalog.json`, **26 stems** across synth / factory /
  space_station / furniture / space_kit (quads in GLB-local units:
  center/normal/up/size).
- **Emissive screens on every placed prop** (client/src/terminal.rs,
  `attach_screens` on `Added<KenneyModule>` so editor, playtest and real runs
  all get them): a quad child at the probed rect, matrix-green idle glow;
  cyan "[E] terminal" prompt when within 2.8 m and looking at the screen.
- **Focus mode + full keyboard capture**: E swaps to a live render-to-texture
  material (offscreen Camera2d + Text2d → 512×320 emissive texture), blends
  the camera to face the screen (PostUpdate, before transform propagation),
  and sets `netplay::InputCapture` — ALL keys go to the terminal until Esc
  (Esc eaten so the cursor stays locked). NPC dialogue wins if both are in
  range. Auto-closes on death/walking away (3.5 m).
- **Fake unix shell v1** (client/src/terminal_shell.rs, pure logic, **12 unit
  tests green**): `user@{host}` prompt (hostname per faction: synth-node-NN /
  plant-ctl-NN / term-NN, seeded per terminal position), in-memory fs with
  9 lore files (/etc/motd, /etc/passwd, operator notes/todo, incident+door
  logs, /sys/doors/manifest, /sys/reactor/status), commands: help ls cd cat
  pwd echo clear whoami hostname uname exit. Hard-wraps at 54 cols, 17 rows.

**HUMAN TEST (editor playtest, synth zone or hub):**
  1. Computer/monitor props glow faint green on their actual screen faces
     (not on casings/backs); walk close + look at one → "[E] terminal".
  2. E: camera glides to face the screen; a live green shell appears ON the
     monitor; WASD/mouse no longer move you.
  3. Type `ls`, `cat /etc/motd`, `cd /home/operator`, `cat notes.txt`,
     `help`, `clear` — output renders on the in-world screen as you type.
  4. Esc: camera glides back, control returns, cursor still locked; screen
     returns to idle glow. Walk out of range mid-session: same.
  5. Stand between an NPC and a terminal: E opens the NPC dialogue (NPC wins).

**Later rounds:** gameplay hooks (unlock doors via /sys/doors, hacking
minigame, lore quests), per-terminal persistent fs, more commands (grep,
head, ps), CRT scanline/flicker shader on the live material.

## P1 — gameplay-visible bugs

- **Editor crashes (exit 101) when leaving playtest back to editor** (user, 2026-07-06).
  Cause unknown — a panic hook now appends message+location to `logs/panic.log`
  (main.rs `install_panic_log`; the release window has no console under
  `windows_subsystem="windows"`). Get that line from the user, then fix. `[F]`
- **Enemies can push the player into (necro) walls.** `move_and_slide` has NO penetration
  recovery (known); an enemy kinematic box shoving the capsule embeds it in the inset
  brick wall. Fix = depenetration pass in the player mover (character.rs has
  `MoveAndSlide::depenetrate` used by `position_is_clear` — apply it to the player each
  tick when overlapped), or stop enemies from physically displacing players (contact
  damage doesn't need pushing: make enemy colliders sensors w.r.t. players). `[F]`
  (physics feel — user warns against hotfix-on-hotfix here)
- **Verify enemy spawn heights in the editor flow end-to-end.** Baked y + nav-authoritative
  spawn + nav floor clamp all landed 2026-07-03b, but the user's last session still showed
  embedded spawns (likely a stale layout). If it recurs: log `enemy nav:` line + spawn y
  values from the console. `[S]` (diagnosis from logs)

## P2 — streaming follow-ups (v1 + exit shafts landed 2026-07-03, round 4)

- **LIVE-VALIDATE streaming next session** (round-3 failure was solid floor-−2 exit rooms —
  shafts are now carved and sealed until the child mounts). Playtest HUD shows
  "next maps: N/2 ready"; console logs `proc stream: generating child… / child ready /
  COMMIT / active map swapped`. Report those lines if anything misbehaves. `[S]` (triage)
- **Exit shafts are permanently open during dev** (2026-07-04: seal removed — it was one
  of two things plugging the drop; the other was the child's colliding roof, now skipped
  over its spawn cell). Later: a visible hatch that opens when the child is ready, so a
  fast player can't outrun generation into the void. `[S]`
- **Stagger the −2 shaft one tile from the hub hole** (user: fall one floor at a time,
  not hub→child in one continuous drop). Needs HubExit.shaft = landing cell adjacent to
  trap; move the −2 hole frame/mask/hub_exits x/z to it (gen_freeform build_hub + to_doc). `[S]`
- **Candidate visual fidelity**: mounted children render plain GLB materials (no zone
  tints / synth marble) until committed. Apply the faction material pass to
  `ProcChildVisual` scenes too (client, test_showcase material slot code). `[S]`
- **Multiplayer streaming**: commit detection uses OwnPlayer (host). Needs replicated
  stream state + party-commit rules (see run.rs hub_commit for the pool-based pattern). `[F]`
- **Old pool streaming path**: map_stream.rs pool machinery still exists in parallel;
  unify or retire once proc streaming is proven. `[F]`

## P3 — industrial round 2 (user re-confirmed the full vision 2026-07-03)

Landed so far: flush machine BANKS (probed widths — machines are ~2.4 m modules, NOT
full tiles), machine-capped conveyor lines, glass/metal pipe families with inline valves,
skip/rocks props. Still expected by the user:
- **Animated pistons + ambient machine sounds** (piston thud, distinct hums per machine
  type). Needs Rust: play embedded GLB animations on placed kenney pieces
  (AnimationPlayer — prop_render.rs agent observer is the pattern) + piece-anchored
  proximity audio (audio.rs electricbuzz pattern). Then emit `piston-*` near banks. `[S]`
- **Moving conveyor belts** you can ride and drop items onto: animated belt material
  (UV scroll) + a surface-velocity volume above each belt segment pushing
  players/props along the belt yaw. `[F]` (physics feel)
- **Bigger industrial rooms with catwalk LEVELS**: per-zone room-size knob in gen_maps;
  catwalk planner (catwalk-straight/-corner/-stairs at 2.4 m) with full-height double
  walls per tier (user rule: tiers never overlap walls). `[F]`
- **Machine-to-machine pipe networks**: kit has `machine-connection-pipe/-hole`; route
  short pipe runs between bank ends and hoppers. `[S]` (data-driven + bbox-continuity
  check script)

## P4 — AI round 2 (mostly ABSORBED into Pass A, 2026-07-06)

- ~~LOS gate on aggro~~ / ~~patrol routes~~ / ~~debug markers~~ — done in Pass A.
- **Combat anims**: models have Punch/RecieveHit/Death clips — wire to attack/damage/kill
  events (prop_render.rs AgentAnim has the node-graph pattern). Death currently =
  instant despawn. `[S]`
- **Debug vision cone mesh** (translucent arc child) — markers cover the need for now;
  add only if mode markers prove insufficient. `[S]`

## P5 — content / polish

- ~~Mini-market pack for the hub~~ — **DONE 2026-07-07** (see Pass D section: kit at
  `assets/models/mini_market/`, stalls swapped in `gen_freeform._dress_hub_and_hidden`).
- **Hub/hidden dressing v2**: verify mini-market stall yaws/scales in-game (cash-register
  ×2.0, props ×2.5 — chosen from probed bounds, needs user eyeball); grow into a fuller
  r.e.p.o.-style re-stock counter if the user wants more. `[S]`
- **Priesthood/urban visual pass feedback** — awaiting user eyeball of the 4.8 prop scale.
- **Phase-3 tuning UI** must be a real responsive UI, not the ascii-feeling randomize
  panel (standing user feedback). `[F]`

## POSTPONED to the end of the list (user decision 2026-07-07, plan4)

Do these only after D and F (and the P-sections above as they become urgent).

### Pass C leftovers — gameplay UI polish

The ui_theme.rs framework + HUD/hotbar/vignette/Tab-inventory that landed 2026-07-06
stay and are what Pass D builds on. Deferred:
- **Tab MAP panel** — postponed entirely (user: no map needed right now). HARD
  REQUIREMENT when it returns: it must work in **editor playtest exactly like in a
  real run**. The playtest-from-editor IS the development environment; any
  "works in real runs only" gap is a defect, not an acceptable limitation.
  (Currently the map panel is empty in playtest — no RunState/grid there.)
- Real item icon art (colored 3-letter text tags are the stand-in).
- Hit MARKER on dealing damage (needs a replicated hit event).
- Death banner centering is a fixed-px offset (fine above ~340 px width).
- Tab map player-trail / seen-rooms fog; controller-size scaling.

### Pass E — atmosphere graphics `[S]`-heavy

**User clarification 2026-07-07: REAL smoke and REAL effects — properly rendered
volumetric/textured smoke and particle effects, NOT simple geometric particle
placeholders.** Machine smoke + air dust (Hanabi soft-particle pattern with textured
sprites + alpha blend — modifier-order gotcha is in memory), distance fog, dim
ambient with player-centred falloff + real placed light sources per faction.
FogVolume/VolumetricFog gotchas are in memory/CLAUDE.md. Human test: mood/perf only.

## Process notes (for whoever picks work up)

- Verify procgen at **cells=25, fractions 0.14/0.55/0.32** (editor defaults).
- Gates: `test_zone_seams.py 15`, `test_nav_grid.py N`, `test_transition_stairs.py`,
  `test_synth_transition.py`, `test_synth_interior_rules.py`, `verify_synth_placement.py`.
  Long-running mass simulation is FREE (user: scripts may run for hours) — prefer
  script-verified changes over manual playtesting.
- Full `cargo build` needs PowerShell/cmd (Git Bash link.exe conflict); fails if the game
  is running (exe locked).
- NEVER `git checkout`/`git restore` — repo is mostly uncommitted work.
