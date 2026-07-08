# How to play online together

Two roles: **the host** (you — one person runs the game world) and **the
friends** (everyone else, who join it). Follow the part that is you.

The game talks over the internet on **UDP port 5000**. Up to **8 players**.

---

## PART 1 — YOU, THE HOST (do this on your PC)

**A. First time only**

1. Open **cmd.exe** (the black Command Prompt window — *not* Git Bash), and go
   to the game folder:
   ```
   cd C:\Users\Benji\fabled
   ```
2. Let your friends reach you. Pick ONE:
   - **Easy way (recommended): Tailscale.** Everyone (you + friends) installs
     **Tailscale** (free, https://tailscale.com), makes an account, and you
     invite them to your network. No router settings at all. Your address is
     the `100.x.y.z` number Tailscale shows you.
   - **Hard way: port forwarding.** In your router, forward **UDP port 5000** to
     your PC. Your address is your public IP (google "what is my ip"). This
     often doesn't work on home internet (CGNAT) — if in doubt, use Tailscale.
3. The first time the game opens, Windows asks to "allow access" through the
   firewall. Tick **both** boxes and allow.

**B. Every time you want to play**

1. In cmd.exe, in the game folder, double-click or run:
   ```
   serve_and_play_random.bat
   ```
   This builds the game, hosts it, AND drops you into a random level. (Use
   `serve_and_play.bat` if you want the fixed starting map instead.)
2. Tell your friends your address (your Tailscale `100.x.y.z`, or your public
   IP if you used port forwarding).
3. Pick your class and play. Friends can join at any time.

**C. Give your friends the game (so they DON'T need to install anything)**

Run this once in cmd.exe:
```
pack_client.bat
```
It makes a folder called **`client_dist`**. Right-click it → *Send to* →
*Compressed (zipped) folder*, and send that `.zip` to your friends (Discord,
WeTransfer, a USB stick — whatever). That's the whole "client". They do **not**
need the code, GitHub, or Rust.

> Re-run `pack_client.bat` and re-send the zip whenever you change the game, so
> everyone is on the same version. Mismatched versions cause weird glitches.

---

## PART 2 — YOUR FRIENDS (the easy way — no installing)

You'll get a **zip file** from the host.

1. **Unzip it** anywhere (Desktop is fine). You get a folder with `fabled.exe`,
   an `assets` folder, and **`PLAY.bat`**.
2. **Double-click `PLAY.bat`.**
3. It asks for the host's address — type the number the host gave you (their
   Tailscale `100.x.y.z`, or their public IP) and press **Enter**.
4. Pick your class. You're in!

If you used Tailscale, install it first (https://tailscale.com) and accept the
host's invite, so their `100.x.y.z` address works.

That's everything. No code, no GitHub, no Rust. Just unzip and play.

---

## PART 2 (alternative) — friends who'd rather build from source

Only needed if the host would rather share the code than a zip.

1. Install **Rust** (https://rustup.rs, default MSVC toolchain — let it install
   the Visual Studio C++ Build Tools when asked) and **Git**.
2. Clone the repo and switch to the play branch:
   ```
   git clone https://github.com/drthammuz/fabled.git fabled
   cd fabled
   git checkout freeform/ceiling-roofs
   ```
3. In **cmd.exe** (not Git Bash), join:
   ```
   join.bat <host-address>
   ```
   First build takes 10–20 min; after that it's seconds.

---

## If something goes wrong

| What you see | What it means / fix |
|---|---|
| Stuck on "connecting…" forever | The host's address is wrong, or UDP 5000 isn't getting through. Try Tailscale (Part 1-A). Test on the same house first with the host's LAN IP. |
| Works at home, not over internet | Your internet uses CGNAT — port forwarding can't work. Use **Tailscale**. |
| Friend sees a black/empty world | They ran `fabled.exe` on its own. They must run **PLAY.bat** from inside the unzipped folder (so `assets` and `userinput` sit next to the exe). |
| Weird desync / players missing | Someone's on a different version. Host re-runs `pack_client.bat`, re-sends the zip, everyone uses the new one. |
| `link.exe failed` while building from source | They built from Git Bash. Use cmd.exe instead. |

No in-game voice or text chat — use Discord.

---

### Quick reference (technical)

- One binary, three modes: `--host` (host + play), `--server` (dedicated, no
  window), `--client <ip>` (join). Port `5000` = `shared/config.rs::DEFAULT_PORT`.
- Bats: `serve_and_play_random.bat` (host, random map), `serve_and_play.bat`
  (host, fixed map), `join.bat <ip>` (build + join from source),
  `pack_client.bat` (build the shareable client zip).
- The client reads `assets/` and `userinput/` from its working directory — that
  is why the shared folder must contain both next to `fabled.exe`.
- Real game (`--host`/`--client`) is fullscreen. Everyone must run the same
  build (no protocol version negotiation).
