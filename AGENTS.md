# AGENTS.md

> Context file for AI coding agents working on **xmtp-app**. Read [CLAUDE.md](./CLAUDE.md) first for build commands, architecture, and conventions.

## What This Project Is

A production-grade XMTP messaging client with four interfaces:
- **TUI** (ratatui) — full-featured chat UI with group management, reactions, markdown, read receipts
- **CLI** (clap) — scriptable commands for all XMTP operations
- **Daemon** (axum) — background HTTP+SSE server bridging TUI/CLI to XMTP network
- **Mobile** (React Native) — companion app using `@xmtp/react-native-sdk` v5 directly (no daemon)

The daemon holds the XMTP client connection and exposes a REST API. TUI and CLI are thin clients that talk to the daemon over HTTP. The mobile app connects to XMTP independently.

## High-Value Pitfalls

- Mobile markdown / vendored renderer pitfalls: read [docs/pitfalls/react-native-markdown-code-font-and-vendoring.md](./docs/pitfalls/react-native-markdown-code-font-and-vendoring.md) before changing `MessageBubble.tsx`, Metro config, CI for mobile, or `vendor/react-native-enriched-markdown`. Critical rules: Android inline code and fenced code block do not share the same native renderer; long inline code and links can also break bubble height if the native renderer loses soft wrap opportunities.

## Crate Dependency Graph

```
xmtp-core (shared types)
├── xmtp-config (config r/w)
├── xmtp-store (state persistence)
├── xmtp-logging (event log)
└── xmtp-ipc (HTTP API types)
        ├── xmtp-daemon (server + SDK wrapper)
        │       uses: xmtp v0.8.1 (crates.io)
        └── xmtp-tui (TUI client)
                └── xmtp-cli (CLI entry point, depends on all above)
```

## Before You Code

1. Run `cargo build --workspace` to verify the build works
2. Run `cargo test -p xmtp-tui --lib` for fast unit tests (no network needed)
3. Read the specific file you're modifying — don't guess at patterns

## TUI Architecture (most common edit target)

```
lib.rs          Event loop: terminal events + IPC events → App → Effects → Runtime
app.rs          Pure state machine. handle_event() returns Vec<Effect>, no IO.
                ~30 fields on App struct. 14 Modal variants.
event.rs        AppEvent (inputs), Effect (outputs), ActionOutcome enums
ipc.rs          Runtime: spawns tokio tasks for each Effect, sends results back as AppEvent
ui.rs           Stateless render functions. Each modal has its own render_* function.
                Messages use per-line ListItems (height=1 each) with manual highlight.
markdown.rs     Markdown → Vec<Line> renderer (custom, not pulldown-cmark widgets)
format.rs       Time/date formatting helpers
```

**Key invariant**: `App::handle_event()` is pure — it never does IO. All side effects go through `Effect` variants processed by `Runtime`.

**Message list rendering**: Each message is split into multiple single-line `ListItem`s (header, content lines, collapse hint, reactions). Selected message highlighting is applied manually via `Style::bg(Color::DarkGray)` on each row — the `List` widget's `highlight_style` is NOT used.

## Daemon Architecture

All in `crates/xmtp-daemon/src/lib.rs` (~3100 lines, monolithic — planned for modular split):

- **SDK wrapper**: `*_with_client()` functions (20+) that open XMTP client and perform operations
- **HTTP handlers**: 25 axum handlers for REST endpoints
- **DaemonApp state**: `Arc<Mutex<DaemonApp>>` holding cached state + broadcast channel
- **SSE system**: Two streams — `/v1/events` (app-level) via broadcast channel, `/v1/conversations/:id/events` (history) via dedicated `std::thread`
- **Monitor task**: tokio::spawn, polls every 2s for conversation/status changes
- **SQLite direct queries**: `fetch_reactions_from_db()`, `fetch_latest_read_receipts_from_db()` — queries libxmtp's internal `group_messages` table (fragile, hardcoded content_type IDs)

## Content Types

Supported across Rust (daemon/TUI/CLI) and Mobile:
- **Fully supported**: Text, Markdown, Reaction, Reply, ReadReceipt, Actions, Intent
- **Not yet supported**: TransactionReference, WalletSendCalls, Attachment, RemoteAttachment — arrive as `Content::Unknown`

To add a new content type:
1. Add decode/encode in daemon's content handling
2. Add `content_kind` mapping in `history_item_from_message()`
3. Add rendering in TUI's `build_message_rows()`
4. Add handler in `xmtp-mobile/src/content/handlers/` and register in registry

## Testing

| Test suite | Command | Network? | What it covers |
|------------|---------|----------|----------------|
| TUI unit | `cargo test -p xmtp-tui --lib` | No | App state transitions, UI helpers, markdown |
| CLI unit | `cargo test -p xmtp-cli --lib` | No | Command parsing, rendering |
| IPC roundtrip | `cargo test -p xmtp-ipc` | No | JSON serialization of all API types |
| Daemon integration | `cargo test -p xmtp-daemon --test http_transport` | Yes (dev) | Full daemon lifecycle, HTTP endpoints, SSE |
| Config/store | `cargo test -p xmtp-config -p xmtp-store` | No | Config/state file roundtrip |

Integration tests use `DaemonProcess::start()` helper that spawns a real daemon on a random port. Rate-limited XMTP operations are guarded with `is_rate_limited()` skip logic.

## Git Conventions

- Stage specific files, **never** `git add -A` (data/ directory contains local XMTP database)
- **Never auto-push** — always wait for explicit user confirmation before `git push`
- Commit message: imperative mood, 1-2 sentences
- Prefer creating a new commit by default; only use `git commit --amend` when the user explicitly asks for amend
- AI-assisted commits end with: `Co-Authored-By: Claude <noreply@anthropic.com>`
- Run `cargo build --workspace` before committing

## Known Architectural Debt

1. **daemon/lib.rs is monolithic** (~3100 lines) — should split into client.rs, handlers.rs, state.rs, events.rs, db.rs, content.rs
2. **TUI depends on daemon crate** only for `addr_path()` — should move to xmtp-config
3. **HistoryEntry ≈ HistoryItem** duplicate types — should merge
4. **SQLite direct queries** bypass SDK API — fragile, will break on libxmtp schema changes
5. **History SSE uses std::thread** — switching conversations may accumulate short-lived zombie threads

## CI

GitHub Actions (`.github/workflows/`):
- **rust.yml** — fmt + clippy + unit tests, triggered by `crates/**` changes
- **mobile.yml** — tsc + eslint + jest + Android build, triggered by `xmtp-mobile/**` changes

Relevant mobile pitfall docs:
- `docs/pitfalls/react-native-debug-workflow.md`
- `docs/pitfalls/react-native-keyboard-avoidance.md`
- `docs/pitfalls/react-native-markdown-code-font-and-vendoring.md`

## Current Backlog

**Open items:**
- Content types: TransactionReference, WalletSendCalls, Attachment, RemoteAttachment
- Push notifications (FCM) for mobile
- WalletConnect integration (replace private key login)
- iOS support (Expo prebuild)
- Daemon modularization
- Attachment support
- API review (REST endpoint naming/structure cleanup)
- Performance optimization (benchmark, SSE reconnect, connection reuse)
