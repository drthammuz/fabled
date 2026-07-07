# Online playtest guide — hosting from your home PC (2026-07-07)

The game is one binary. Networking: server listens on **UDP port 5000**
(`shared/config.rs::DEFAULT_PORT`), up to **8 players** (`MAX_CLIENTS`),
renet/netcode over UDP, "unsecure" auth (fine for friends). Everyone must run
**the same git commit** — the wire protocol has no version negotiation beyond
`PROTOCOL_ID`; mismatched builds mean weird bugs, not clean errors.

Real game sessions (`--host` and `--client`) are now **fullscreen-only**
(borderless). Windowed remains for the editor/dev modes only.

---

## Part 1 — You (the host)

### One-time setup

1. **Build in release** (debug works but release keeps the tick rate and your
   GPU happy once 3+ players connect). In **cmd.exe** (not Git Bash — its
   `link.exe` shadows MSVC's):

   ```
   cd C:\Users\Benji\fabled
   cargo build --release
   ```

2. **Windows Firewall**: first launch, Windows shows the "allow access"
   dialog — tick *both* private and public and allow. If you missed it:
   Windows Security → Firewall → Allow an app → add
   `target\release\fabled.exe`, or an inbound rule for **UDP 5000**.

3. **Router port forward**: forward external **UDP 5000** → this PC's LAN IP
   (find it with `ipconfig` → IPv4 Address, e.g. `192.168.1.23`). Give the PC
   a static/reserved LAN IP in the router if possible, so the forward
   doesn't rot.

4. **Your public IP**: google "what is my ip". That's what friends type.
   ⚠ If your ISP gives you CGNAT (public IP in `100.64.x.x`, or router WAN IP
   differs from what google shows), port forwarding won't work — use the
   Tailscale fallback in Part 3.

### Every session

Serve and play in one process (recommended — this is the proven path):

```
serve_and_play.bat        (= target\release\fabled.exe --host)
```

Or the two-prompt setup you described (dedicated server + join yourself):

```
prompt 1:  target\release\fabled.exe --server
prompt 2:  join.bat            (= --client 127.0.0.1)
```

Both listen on UDP 5000. Note the headless `--server` path has had less
soak-testing than `--host`; if anything's odd tonight, fall back to `--host`.

---

## Part 2 — Your friends

They build from source (no binary distribution set up yet).

1. **Install prerequisites** (one-time, ~20 min):
   - **Rust**: https://rustup.rs → default MSVC toolchain.
   - When rustup asks, let it install **Visual Studio Build Tools (C++)** —
     needed for the linker. (Linux friends: `clang`/`lld` + ALSA/udev dev
     packages per bevy docs.)
   - **Git**.

2. **Clone your repo** (they need your GitHub access if it's private):

   ```
   git clone https://github.com/drthammuz/fabled.git fabled
   cd fabled
   git checkout freeform/ceiling-roofs
   ```

   (The playtest build lives on the `freeform/ceiling-roofs` branch, not
   `master` — don't skip the checkout.)

3. **Build + join** (first build is 10–20 min; later ones seconds). In
   **cmd.exe**:

   ```
   join.bat <your-public-ip>
   ```

   (equivalent to `cargo run --release -- --client <ip>`; `join.bat` builds
   first if needed.)

4. In game: class select screen → pick 1–4 → you're in the hub together.

**Before the session**: everyone `git pull` and confirm the same
`git rev-parse --short HEAD`.

---

## Part 3 — Most probable errors

| Symptom | Cause / fix |
|---|---|
| Client hangs at "connecting to …" forever | UDP 5000 not reaching the server: port forward wrong, firewall blocked it, or wrong IP. Test locally first: friend runs `--client <your-LAN-ip>` from the same house/VPN, or you run `join.bat` on the host machine (127.0.0.1) — if local works, it's router/firewall. |
| Local connect works, internet doesn't, forwarding looks right | CGNAT. Fastest workaround: everyone installs **Tailscale** (free), you share your tailnet, friends use your Tailscale `100.x.y.z` IP instead. No router config at all — honestly a good Plan A if your router is annoying. |
| `error: linker link.exe failed` on friend's build | They built from Git Bash — its `link.exe` shadows MSVC's. Build from cmd.exe/PowerShell, or install VS Build Tools if missing entirely. |
| Weird desync / entities missing / instant disconnect | Commit mismatch. Everyone `git pull`, rebuild, retry. |
| Friend sees black/empty world after connect | Assets missing → they cloned without LFS?? (repo doesn't use LFS — more likely they run the exe from outside the repo; run via `join.bat`/`cargo run` from the repo root so `assets/` resolves). |
| Host PC hitches when friends join | You're on the debug build — use release. |

No in-game voice/text chat — use Discord.
