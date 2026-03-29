# AGENTS.md

> Context file for AI coding agents working on this project. Read [CLAUDE.md](./CLAUDE.md) first for build commands, architecture, and conventions.

## What This Project Is

A production-grade XMTP messaging client with three interfaces:
- **TUI** (ratatui) — full-featured chat UI with group management, reactions, markdown, read receipts
- **CLI** (clap) — scriptable commands for all XMTP operations
- **Daemon** (axum) — background HTTP+SSE server bridging TUI/CLI to XMTP network

The daemon holds the XMTP client connection and exposes a REST API. TUI and CLI are thin clients that talk to the daemon over HTTP.

## Crate Dependency Graph

```
xmtp-core (shared types)
├── xmtp-config (config r/w)
├── xmtp-store (state persistence)
├── xmtp-logging (event log)
└── xmtp-ipc (HTTP API types)
        ├── xmtp-daemon (server + SDK wrapper)
        │       uses: xmtp-fork/xmtp (local fork of XMTP Rust SDK)
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

Messages are decoded in `xmtp-fork/xmtp/src/content.rs` → `Content` enum:
- Supported: `Text`, `Markdown`, `Reaction`, `Reply`, `ReadReceipt`, `Attachment`, `RemoteAttachment`
- **Not yet supported**: `TransactionReference`, `WalletSendCalls`, `Actions`, `Intent` — these arrive as `Content::Unknown` and display fallback text

To add a new content type:
1. Add variant to `Content` enum in `xmtp-fork/xmtp/src/content.rs`
2. Add decode branch in `decode()` function (JSON via `serde_json::from_slice` for most types)
3. Add handling in daemon's `history_item_from_message()` → set `content_kind` + `content`
4. Add rendering in TUI's `build_message_rows()` or `render_messages()`

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

## Current Backlog

See `.cache/research/REPORT-2026-03-27.md` for the full protocol ecosystem analysis.

**Open items:**
- XMTP content type display: TransactionReference, WalletSendCalls, Actions/Intent
- x402 payment integration (see `.cache/research/x402-xmtp-integration-research.md`)
- Daemon modularization
- Attachment support (Attachment + RemoteAttachment)
- API review (REST endpoint naming/structure cleanup)
- Performance optimization (benchmark, SSE reconnect, connection reuse)
