# XMTP App

A full-featured XMTP messaging client built in Rust, with an interactive TUI, background daemon, AI agent bridge, and a companion React Native mobile app.

## Features

**CLI** — Send messages, manage groups, inspect conversations, and control the daemon from the command line.

**TUI** — Interactive terminal UI powered by [ratatui](https://github.com/ratatui/ratatui) with real-time message streaming, group management, reactions, read receipts, and keyboard-driven navigation.

**Daemon** — Background HTTP+SSE server (axum) that maintains persistent connections to the XMTP network, syncs conversations, and exposes a REST API for the CLI and TUI.

**ACP Bridge** — Connects any [Agent Client Protocol](https://github.com/anthropics/agent-client-protocol) compatible AI agent to an XMTP conversation. Supports streaming replies, reaction indicators, session resume, and interactive Actions menus.

**Mobile App** — React Native companion app (`xmtp-mobile/`) using `@xmtp/react-native-sdk` v5 directly, with optimistic UI, reactions, group management, and Coinbase Actions support.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                      xmtp-cli                           │
│  CLI commands (clap) · daemon mgmt · ACP agent bridge   │
├────────────────────────┬────────────────────────────────┤
│       xmtp-tui         │     HTTP + SSE (REST API)      │
│  ratatui state machine │            │                   │
│  Effect-driven IO      │            ▼                   │
│                        │       xmtp-daemon              │
│                        │  axum server · XMTP SDK wrapper│
│                        │  background monitor             │
├────────────────────────┴────────────────────────────────┤
│  xmtp-core    xmtp-config    xmtp-ipc                  │
│  xmtp-store   xmtp-logging                             │
├─────────────────────────────────────────────────────────┤
│              xmtp (crates.io v0.8.1)                    │
│              XMTP network protocol                      │
└─────────────────────────────────────────────────────────┘
```

| Crate | Role |
|-------|------|
| `xmtp-cli` | CLI entry point, clap commands, daemon process management, ACP bridge |
| `xmtp-tui` | Ratatui TUI: event loop, App state machine, IPC client |
| `xmtp-daemon` | HTTP server (axum) + XMTP SDK wrapper + background monitor |
| `xmtp-ipc` | HTTP Request/Response/Event type definitions |
| `xmtp-core` | Shared types: DaemonState, ConnectionState, StateSnapshot |
| `xmtp-config` | AppConfig JSON read/write, path helpers |
| `xmtp-store` | StateSnapshot disk persistence |
| `xmtp-logging` | Daemon event log file append |

## Quick Start

### Prerequisites

- Rust 1.85+ (edition 2024)
- An XMTP-compatible wallet private key

### Build

```bash
cargo build --workspace
```

### Initialize & Login

```bash
# Create local data directory
xmtp-cli init

# Login to XMTP dev network (generates a new identity)
xmtp-cli login --network dev

# Or import an existing wallet
xmtp-cli login --network dev --private-key 0x...
```

### Start the Daemon

```bash
xmtp-cli daemon start
xmtp-cli doctor          # Verify setup & connectivity
```

### Send Messages

```bash
# Direct message
xmtp-cli dm 0xRecipientAddress "Hello from CLI"

# List conversations
xmtp-cli list

# View conversation history
xmtp-cli history <conversation-id>

# React to a message
xmtp-cli react <message-id> "👍"
```

### Launch the TUI

```bash
xmtp-cli tui
```

### Bridge an AI Agent

Connect any ACP-compatible agent (e.g. Claude Code, custom agents) to an XMTP conversation:

```bash
xmtp-cli acp \
  --conversation-id <id> \
  --reactions basic \
  --reply-mode stream \
  -- claude-code --print
```

Options:
- `--reactions off|basic|verbose` — Reaction emoji indicators for agent activity
- `--reply-mode single|stream` — Deliver one final message or stream progressively
- `--actions` — Enable interactive structured choice menus
- `--resume [index|id]` — Resume a previous ACP session

## CLI Commands

| Command | Description |
|---------|-------------|
| `init` | Initialize the local data directory |
| `login` | Login to an XMTP network |
| `doctor` | Check local setup, daemon reachability, and runtime status |
| `tui` | Launch the interactive TUI |
| `acp` | Bridge a conversation to an ACP agent subprocess |
| `daemon start\|stop\|restart\|status` | Manage the daemon process |
| `logs` | Read daemon logs (events, stdout, stderr) |
| `watch events\|messages` | Watch live daemon events |
| `list` | List DM and group conversations |
| `dm` | Send a direct message |
| `group list\|create\|add-member\|remove-member\|send` | Manage groups |
| `history` | Show conversation history |
| `reply` | Reply to an existing message |
| `react` / `unreact` | Add or remove emoji reactions |
| `leave` | Leave a conversation |
| `send-actions` | Send an Actions message with structured choices |
| `info conversation\|message` | Inspect conversations and messages |

## Content Types

Supports XMTP standard and extended content types:

- **Text** — Plain text messages
- **Reply** — Threaded replies referencing a parent message
- **Reaction** — Emoji reactions with add/remove semantics
- **Read Receipt** — Delivery and read confirmations
- **Group Updated** — Membership and metadata change events
- **Coinbase Actions/Intent** — Interactive structured action menus with button-based selection

## Mobile App

The companion React Native app lives in `xmtp-mobile/`. It connects directly to the XMTP network (no daemon dependency).

**Tech stack:** Expo SDK 55 · React Native 0.83 · @xmtp/react-native-sdk v5 · zustand · react-native-paper (MD3)

**Features:** DM and group messaging, reactions with toggle UX, group management (create, add/remove members, leave), conversation streaming, optimistic UI updates, Coinbase Actions support, network/foreground auto-recovery.

```bash
cd xmtp-mobile
npm install

# Android debug build
cd android && ./gradlew assembleDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

See `CLAUDE.md` for detailed build, debugging, and release instructions.

## Development

```bash
cargo build --workspace          # Full build
cargo test --workspace           # All tests (includes integration tests)
cargo test -p xmtp-tui --lib     # TUI unit tests only (fast, no network)
cargo clippy --workspace         # Lint
```

### Project Stats

- **8 Rust crates**, ~33k lines of Rust
- **Mobile app**, ~5k lines of TypeScript/React Native
- **205 commits** over active development
- **5 ADRs**, **6 pitfall docs**, ongoing solutions library

## License

MIT
