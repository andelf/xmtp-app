# CLAUDE.md

## Project Overview

**xmtp-app** — Rust monorepo implementing XMTP messaging protocol client: CLI + TUI + background daemon + React Native mobile app. Depends on `xmtp` crate v0.8.1 from crates.io.

## Build & Test

```bash
cargo build --workspace          # Full build
cargo test --workspace           # All tests (includes integration tests requiring XMTP dev network)
cargo test -p xmtp-tui --lib     # TUI unit tests only (fast, no network)
cargo test -p xmtp-daemon --test http_transport  # Integration tests (starts real daemon, needs network)
cargo clippy --workspace         # Lint
```

## Architecture

```
xmtp-core          Shared types: DaemonState, ConnectionState, StateSnapshot
xmtp-config        AppConfig JSON read/write, path helpers
xmtp-store         StateSnapshot disk persistence (state.json)
xmtp-logging       Daemon event log file append
xmtp-ipc           All HTTP Request/Response/Event type definitions
xmtp-daemon        HTTP server (axum) + XMTP SDK wrapper + background monitor
xmtp-tui           Ratatui TUI: event loop, App state machine, IPC client (HTTP+SSE)
xmtp-cli           CLI entry point (clap), daemon process management, ACP bridge
```

Dependency direction: `core -> config/store/logging/ipc -> daemon -> tui -> cli`. No cycles.

**Key dependency:** `xmtp-daemon` depends on `xmtp = "0.8.1"` from crates.io.

## Key Patterns

- **TUI is a pure state machine**: `App::handle_event(AppEvent) -> Vec<Effect>` does zero IO. `Runtime::apply_effects()` executes side effects via `tokio::spawn`.
- **Daemon communicates via HTTP+SSE**: TUI/CLI never call daemon functions directly at runtime. They use `GET/POST /v1/...` endpoints and SSE streams.
- **CLI has dual mode**: Some commands call daemon functions directly (no daemon process needed); others go through HTTP (need running daemon).
- **Content types**: Text, Markdown, Reaction, Reply, ReadReceipt, Actions, Intent are fully supported. Unknown types arrive as `Content::Unknown`.

## CI

GitHub Actions (`.github/workflows/`):
- **rust.yml** — `cargo fmt --check` + `cargo clippy` + `cargo test --workspace --lib` on `crates/**` changes
- **mobile.yml** — `tsc --noEmit` + `eslint` + `jest` + `./gradlew assembleDebug` on `xmtp-mobile/**` changes

## Conventions

- Commit messages: imperative mood, concise. Co-author line for AI-assisted commits.
- No `git add -A` — always stage specific files to avoid committing data/ directory.
- **Never auto-push** — always wait for explicit user confirmation before `git push`.
- UI text in English only (no Chinese in source strings).
- Tests: unit tests in `#[cfg(test)] mod tests` within source files; integration tests in `crates/*/tests/`.
- Error handling: `anyhow::Result` internally, `(StatusCode, Json<ApiErrorBody>)` at HTTP boundary.

## Important Files

| File | Lines | Role |
|------|-------|------|
| `crates/xmtp-daemon/src/lib.rs` | ~3100 | All daemon logic (monolithic, planned for split) |
| `crates/xmtp-tui/src/app.rs` | ~3300 | TUI state machine, all event handling |
| `crates/xmtp-tui/src/ui.rs` | ~1300 | All ratatui rendering functions |
| `crates/xmtp-tui/src/ipc.rs` | ~1000 | HTTP client + SSE consumer |
| `crates/xmtp-cli/src/main.rs` | ~2600 | CLI entry, clap commands, daemon management |
| `crates/xmtp-ipc/src/lib.rs` | ~250 | All IPC type definitions |

## Data Flow

**Send**: TUI input -> `Effect::SendMessage` -> HTTP POST `/v1/groups/:id/send` -> daemon handler -> `conversation.send_text()` -> XMTP network

**Receive**: XMTP network -> daemon SSE `/v1/conversations/:id/events` -> `AppEvent::HistoryEvent` -> `merge_history_item()` -> re-render

## Mobile App (xmtp-mobile/)

React Native app using `@xmtp/react-native-sdk` v5 directly (no daemon dependency).

**Tech stack**: Expo SDK 55, expo-router, zustand, react-native-paper (MD3), react-native-keyboard-controller, @shopify/flash-list, ethers v6.

### Build & Install

```bash
cd xmtp-mobile

# Dev build (with Metro, console.log visible):
cd android && ./gradlew assembleDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb reverse tcp:8081 tcp:8081       # USB port forward for Metro
npx expo start --dev-client         # Start Metro bundler

# Release build (embedded bundle, no Metro needed):
npx expo export --platform android
mkdir -p android/app/src/main/assets
cp dist/_expo/static/js/android/*.hbc android/app/src/main/assets/index.android.bundle
cd android && ./gradlew assembleRelease
adb install -r app/build/outputs/apk/release/app-release.apk
```

### Debugging

**Method 1: Metro + DevTools (preferred)**
- Build debug APK (no embedded bundle) → app loads JS from Metro
- `adb reverse tcp:8081 tcp:8081` — USB forward Metro port
- `npx expo start --dev-client` — start Metro
- `adb logcat -s "ReactNativeJS:*"` — see all console.log output
- React DevTools available via Metro terminal

**Method 2: Dev bundle in release APK (when DevTools blocked)**
- `npx expo export --platform android --dev` — generates `.js` (not `.hbc`)
- Copy to `android/app/src/main/assets/index.android.bundle`
- Build release APK — console.log preserved in dev bundle
- `adb logcat -s "ReactNativeJS:*"` — logs visible

**Important**: Production export (`npx expo export` without `--dev`) generates `.hbc` (Hermes bytecode) which **strips all console.log**. Always use `--dev` when debugging.

**`debuggable true` in build.gradle**: Currently set for release builds to allow `adb shell run-as`. **Remove before production release.**

### Mobile Architecture

```
app/                    expo-router pages
├── _layout.tsx         Root: SafeAreaProvider + KeyboardProvider + PaperProvider
├── login.tsx           Private key input + server env selector (dev/prod/local)
└── (main)/
    ├── _layout.tsx     Stack nav + useConversations (global stream)
    ├── conversations.tsx   Conversation list
    ├── conversation/[id].tsx   Chat screen
    └── new-conversation.tsx    New DM
src/
├── xmtp/client.ts      XMTP Client.create() singleton
├── xmtp/messages.ts    sendMessage with optimistic UI
├── store/auth.ts       Zustand: login/restore/logout + SecureStore
├── store/conversations.ts  Zustand: conversation list + topicToId map
├── store/messages.ts   Zustand: per-conversation messages + dedup
├── hooks/useConversations.ts  Stream lifecycle (conversations + allMessages)
├── hooks/useMessages.ts       Per-conversation message stream
├── hooks/useAppState.ts       Foreground recovery
├── hooks/useNetworkState.ts   Network recovery
├── components/         MessageBubble, MessageInput, ConversationListItem
└── utils/              time formatting, address shortening, logger
```

### Key Patterns (Mobile)

- **Zustand getState() in hooks**: All hooks use `useXxxStore.getState()` to access store actions, never selectors in useEffect deps. Prevents infinite re-render loops.
- **Stream onClose reconnect**: All XMTP streams (`stream()`, `streamAllMessages()`, `streamMessages()`) use the `onClose` callback for auto-reconnect.
- **Never call cancelStreamAllMessages() outside useConversations**: It's a global cancel that kills ALL allMessages subscriptions on the client, not just the one you registered.
- **Keyboard avoidance**: `react-native-keyboard-controller` with `behavior="translate-with-padding"` — the only approach that works with Expo edge-to-edge on Android.
- **Buffer polyfill**: `globalThis.Buffer = Buffer` required in root layout for XMTP SDK signature handling.

### Known Pitfalls

See `docs/pitfalls/` for detailed writeups:
- `react-native-debug-workflow.md` — build variants, Metro, logging, testing, and common gotchas
- `react-native-keyboard-avoidance.md` — 6 failed approaches before finding the right one
- `xmtp-sdk-stream-cancelled-by-verification.md` — global stream cancel scope + debug method

### Backlog

See `docs/mobile-app-backlog.md` for full roadmap (P1/P2/P3).

## Research & Planning

- `.cache/research/` — cloned repos (libxmtp, XIPs, agentkit, A2A) and research reports
- `.cache/research/REPORT-2026-03-27.md` — protocol ecosystem overview
- `.cache/research/x402-xmtp-integration-research.md` — x402 integration plan
