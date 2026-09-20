# ovpn3-tui

[![CI](https://github.com/bardiz12/ovpn3-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/bardiz12/ovpn3-tui/actions/workflows/ci.yml)
[![Release](https://github.com/bardiz12/ovpn3-tui/actions/workflows/release.yml/badge.svg)](https://github.com/bardiz12/ovpn3-tui/actions/workflows/release.yml)
[![Rust](https://img.shields.io/badge/rust-2024%20edition-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A modern, fast, and responsive Terminal User Interface (TUI) frontend for **OpenVPN 3 Linux**, written in **Rust** using **Ratatui**.

---

## Features

- **Profile Management:** Scans and manages OpenVPN configuration profiles (`.ovpn`, `.conf`) from `~/.config/ovpn3-tui/configs/`.
- **Encrypted Credential Storage:** Safely stores usernames and passwords encrypted with **AES-256-GCM** in SQLite3 (`~/.config/ovpn3-tui/ovpn3_tui.db`).
- **Secure Key Management:** Automatically generates a 256-bit symmetric encryption key stored at `~/.config/ovpn3-tui/.key` with strict Unix permissions (`0600`).
- **OpenVPN 3 CLI Integration:** Full lifecycle management (Connect, Disconnect, Status sync) communicating directly with `openvpn3` CLI.
- **2FA / Authenticator Code Support:**
  - Automatically captures dynamic challenges from OpenVPN 3 (`openvpn3 session-auth`).
  - Interactive pop-up prompt with real-time verification status (`[VERIFYING...]`), preventing dialog re-opening loops.
  - Supports WebAuth / SSO URLs with direct browser launch (`[o]`).
  - Optional inline OTP / Password concatenation (`[F3]`).
- **Real-Time Network Monitoring:**
  - Queries transmission statistics directly from `openvpn3 session-stats --json`.
  - Automatic fallback to Linux kernel virtual interfaces (`/sys/class/net/<device>/statistics/`).
  - Dynamic **Sparkline** throughput graphs for Download and Upload transfer rates.
- **Responsive Layout:**
  - Adaptive modal sizing that scales cleanly and prevents text clipping on small terminals (e.g. 80x24).

---

## Interface Preview

```text
╭──────────────────────────────────────────────────────────────────────────────╮
│ OVPN3-TUI — OpenVPN 3 Client Manager  ● CONNECTED                            │
╰──────────────────────────────────────────────────────────────────────────────╯
╭ Config Profiles (3) ─────────────╮╭ Active Session Details ──────────────────╮
│▶ [● Connected]  [K] office.ovpn  ││Status     : Connection, Client connected │
│  [○ Idle]       [K] home-lab.ovpn││Profile    : office.ovpn                  │
│  [○ Idle]       [ ] vps-sg.ovpn  ││Endpoint   : udp:203.0.113.10:1194        │
│                                  ││Interface  : tun0                         │
│                                  ││Session ID : ...0b5a34f5s5fa8s4801sbfd3sbe│
│                                  ││PID / Owner: 1507962 / dizba              │
│                                  ││Total Xfer : ↓ 14.82 MB  |  ↑ 2.45 MB     │
│                                  │╰──────────────────────────────────────────╯
│                                  │╭ Network Throughput (Sparkline) ──────────╮
│                                  ││ ↓ Download Rate: 1.24 MB/s               │
│                                  ││  ▂▃▅▇█▆▅▃▂                               │
│                                  ││ ↑ Upload Rate  : 140.50 KB/s             │
│                                  ││   ▂▄▃▅▄▃▂                                │
╰──────────────────────────────────╯╰──────────────────────────────────────────╯
 [↑/↓/j/k] Select  [c] Connect  [d] Disconnect  [a] 2FA Code  [e] Edit Creds  [x] Del Creds  [r] Refresh  [q] Quit
```

---

## Requirements

1. **OpenVPN 3 Linux** (`openvpn3`) installed and running on your system.
2. **Rust Toolchain** (1.70+ recommended).

---

## Installation & Build

### From Source

```bash
# Clone repository
git clone https://github.com/bardiz12/ovpn3-tui.git
cd ovpn3-tui

# Build optimized binary
cargo build --release

# Run
./target/release/ovpn3-tui
```

---

## Configuration & Files

On first run, `ovpn3-tui` automatically creates the required directory tree:

| Path | Description |
|---|---|
| `~/.config/ovpn3-tui/configs/` | Directory where your `.ovpn` or `.conf` files should be placed. |
| `~/.config/ovpn3-tui/ovpn3_tui.db` | Embedded SQLite3 database storing encrypted credentials. |
| `~/.config/ovpn3-tui/.key` | 32-byte AES-256-GCM symmetric key with permissions `0600`. |

To add profiles, simply copy your VPN configuration files:
```bash
cp /path/to/company.ovpn ~/.config/ovpn3-tui/configs/
```

---

## Keyboard Shortcuts

| Key | Action |
|---|---|
| `↑` / `k` or `↓` / `j` | Navigate through configuration profiles |
| `c` / `Enter` | Connect to selected profile (opens credentials dialog if not saved) |
| `d` | Disconnect active VPN session |
| `a` | Open 2FA / Authenticator Code challenge prompt |
| `e` | Edit & store encrypted Username, Password, and optional OTP |
| `x` | Delete saved credentials for the selected profile |
| `r` | Refresh profiles, active sessions, and authentication queue |
| `q` / `Esc` / `Ctrl+C` | Quit application |

### Inside Modal Dialogs

| Key | Action |
|---|---|
| `Tab` / `BackTab` | Cycle between input fields (Username → Password → OTP) |
| `Enter` | Save credentials / Submit Authenticator code |
| `F2` | Toggle password visibility (`*` mask) |
| `F3` | Toggle OTP mode (`Append to Password` vs `Challenge Response`) |
| `o` | Open WebAuth / SSO URL in default browser (if challenge contains URL) |
| `Esc` | Dismiss / cancel modal |

---

## Development & Testing

```bash
# Run tests
cargo test

# Check formatting and linter
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

---

## License

This project is licensed under the MIT License.
