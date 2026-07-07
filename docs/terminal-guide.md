# In-world computer terminals — user guide (Pass F)

Walk up to any screen-bearing prop (monitors, computers, consoles — anything in
`assets/models/screen_catalog.json`) and an **[E] terminal** prompt appears.
Press **E**: the camera focuses on the screen and your keyboard types into a
fake unix shell rendered live on the in-world monitor.

**While connected, the keyboard belongs to the computer.** Every key is shell
input; nothing (movement, hotbar, G, B, Tab…) does its normal thing. The one
exception is **Esc**, which disconnects and gives you your controls back.
Dying, or being shoved more than ~3.5 m away, also disconnects.

## Commands

| Command | Does |
|---|---|
| `help` | list commands |
| `ls [dir]` | list directory |
| `cd <dir>` | change directory (`cd` alone → root, `..` works) |
| `cat <file>` | print a file |
| `pwd` | print working directory |
| `echo <text>` | print text |
| `clear` | wipe the screen |
| `whoami` / `hostname` / `uname` | session flavor |
| `exit` / `logout` | tells you to press Esc |

Anything else: `sh: <cmd>: command not found`.

## Filesystem

```
/
├── bin/            sh ("[binary]")
├── etc/            motd, passwd
├── home/operator/  notes.txt, todo.txt
├── var/log/        incident.log, door.log
└── sys/
    ├── doors/      manifest
    └── reactor/    status
```

## Is there anything to find?

Right now it is **all flavor/lore — no gameplay hooks yet**. The files sketch
the setting: an operator's notes ("keep hearing something in the vents near
the hub"), an incident log with two unexplained heat signatures, a door
manifest showing door-03 with OVERRIDE ENABLED, and a reactor status that
tells you not to trust the gauge on the floor above. The tree was deliberately
shaped so later rounds can hang real hooks off it (door codes in `/sys/doors`,
hacking via `/bin`), but none of that is wired yet.

Per-faction identity: synth boxes greet as `synth-node-NN` ("SYNTH COLLECTIVE
// uplink stable"), factory PLCs as `plant-ctl-NN`, everything else `term-NN`
running "FACILITY OS 0.9". The hostname number is seeded from the prop's
position, so the same terminal always has the same name.

## Multiplayer status (honest answer)

The terminal is currently **client-side flavor only** (`crates/client/src/
terminal.rs` — "no server round-trips"):

- **Other players do NOT see your session.** The live screen texture exists
  only on your client; on their machines the monitor keeps showing the idle
  green-glow texture (that's also the "green dash" you see from afar — a
  procedural idle image with a cursor block, drawn on every screen).
- **No locking.** Two players can "use" the same terminal simultaneously;
  each gets their own private session.
- **No persistence.** Each E press builds a fresh shell (`Shell::new` per
  open). Leave and return — scrollback and cwd are gone, for you too.
- The "screen turns black then fills with text" you noticed is the swap from
  the idle texture to your private render-target texture.

Making it a real shared computer means moving the `Shell` state server-side
(one shell per screen entity), replicating the screen text (~1 KB per redraw,
Channel::Ordered), rendering incoming text on every client's quad, and adding
an in-use lock or shared typing. It's a modest, well-bounded feature —
roughly: server `terminal.rs` with `HashMap<ScreenId, Shell>`, two protocol
messages (`TerminalKey` up, `TerminalScreen` down) — but it's real protocol
work, not a flag flip. Worth doing once terminals carry gameplay (door codes),
so a team can watch one player dig up the override code together.
