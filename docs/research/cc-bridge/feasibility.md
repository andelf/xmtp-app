# Feasibility of a Parallel `xmtp-cli cc-bridge`

## Executive summary

Yes — a parallel `xmtp-cli cc-bridge` command built on `cc-sdk` appears feasible.

The strongest reason is not merely that `cc-sdk` can talk WebSocket. It is that `cc-sdk` already exposes a transport abstraction with:

- subprocess and WebSocket options
- message streaming
- control messaging
- keepalive / heartbeat behavior
- automatic reconnect with backoff
- replay/dedup primitives

That makes it a realistic candidate for a second bridge path that can coexist with the current ACP-based bridge.

## Why this research was initiated

The current `xmtp-cli acp` flow has shown a failure mode where a single per-conversation history/SSE path can silently stop surfacing new conversation messages while:

- the daemon process remains alive
- the bridge process remains alive
- other conversations continue to work
- no explicit error is emitted for the affected conversation

Relevant current code/logging points:

- bridge SSE idle handling exists in `crates/xmtp-cli/src/acp.rs:23-26` and `crates/xmtp-cli/src/acp.rs:1045-1058`
- per-conversation history streams are produced by `stream_history_events()` in `crates/xmtp-daemon/src/lib.rs:2756-2869`
- global daemon events use `broadcast::Sender<DaemonEventEnvelope>` in `crates/xmtp-daemon/src/lib.rs:2000-2003`

The question is whether a `cc-sdk`-backed bridge, especially using WebSocket transport, gives better transport-level liveness and recovery.

---

## What `cc-sdk` provides

### Crate identity

Local crate metadata shows:

- crate: `cc-sdk = 0.8.1`
- description: `Rust SDK for Claude Code CLI with full interactive capabilities`
- repository: `https://github.com/ZhangHanDong/claude-code-api-rs`
- docs: `https://docs.rs/cc-sdk`

Observed via `cargo info cc-sdk`.

### Transport support

`cc-sdk` includes an optional `websocket` feature.

Evidence:

- `~/.cargo/registry/.../cc-sdk-0.8.1/Cargo.toml:50-56`
  - feature `websocket = ["tokio-tungstenite", "url", "http"]`
- `README.md:139-173`
  - documents `WebSocketTransport`
  - shows `ws://` example usage
- `src/lib.rs:131-133`
  - re-exports `WebSocketTransport` and `WebSocketConfig` behind the `websocket` feature

This means `cc-sdk` is not subprocess-only. It can run against an externally managed Claude Code endpoint over WebSocket.

### Heartbeat / keepalive

`cc-sdk`'s WebSocket transport explicitly includes keepalive behavior.

Evidence:

- `CHANGELOG.md:177-188`
  - `keepalive (ping/keep_alive)`
  - `ping_interval_secs (default: 10)`
- `src/transport/websocket.rs:58-59`
  - `ping_interval_secs` config field
- `src/transport/websocket.rs:77-83`
  - default `ping_interval_secs: 10`
- `src/transport/websocket.rs:557-575`
  - spawns a keepalive task sending `{"type":"keep_alive"}` on a fixed interval
- `src/transport/websocket.rs:502-504`
  - incoming `keep_alive` messages are recognized
- `src/transport/websocket.rs:397-398`
  - WebSocket `Ping`/`Pong` frames are also handled at the tungstenite layer

### Reconnect behavior

`cc-sdk`'s WebSocket transport has much stronger built-in reconnect behavior than the current per-conversation SSE path.

Evidence:

- `src/transport/websocket.rs:4-12`
  - reconnection features are documented in module comments
- `src/transport/websocket.rs:39-43`
  - reconnect time budget and sleep/wake detection constants
- `src/transport/websocket.rs:52-57`
  - backoff and reconnect budget config
- `src/transport/websocket.rs:66-69`
  - `auto_reconnect` and `max_reconnect_attempts`
- `src/transport/websocket.rs:577-720`
  - supervisor task waits for disconnect reason and performs reconnection with backoff
- `src/transport/websocket.rs:608-614`
  - permanent close codes skip reconnect
- `src/transport/websocket.rs:644-651`
  - reconnect budget exhaustion
- `src/transport/websocket.rs:654-668`
  - exponential backoff with jitter
- `src/transport/websocket.rs:631-640`
  - sleep/wake detection resets reconnection budget

### Replay / dedup support

`cc-sdk` includes replay/dedup mechanisms that are directly relevant to reconnect safety.

Evidence:

- `src/transport/websocket.rs:92-125`
  - circular replay buffer for outbound messages
- `src/transport/websocket.rs:265-267`
  - `X-Last-Request-Id` header added on reconnect
- `src/transport/websocket.rs:304-320`
  - replay buffered messages after reconnect
- `CHANGELOG.md:181-183`
  - replay + dedup explicitly called out

This is important because a robust reconnect story is not just reconnecting the socket. It also needs to handle duplicate or lost in-flight writes.

### Message / control model

`cc-sdk` is not ACP. It has its own transport trait and message model.

Evidence:

- `src/interactive.rs:5-6`
  - uses `transport::{InputMessage, SubprocessTransport, Transport}` and `types::{ClaudeCodeOptions, ControlRequest, Message}`
- `src/interactive.rs:58-82`
  - sends a user input message and waits until a `Message::Result` arrives
- `src/interactive.rs:153-220`
  - exposes a stream of messages
- `src/interactive.rs:242-245`
  - control request includes interrupt support
- `src/interactive.rs:333-357`
  - exposes MCP reconnect control path
- `src/message_parser.rs:280+`
  - parses stream events into typed messages

This is sufficient for a bridge path, but it is not a drop-in replacement for `agent-client-protocol`.

### Startup/configuration parameter support

`cc-sdk` does support startup/configuration parameters, but primarily as **structured SDK options**, not as an unrestricted raw argv passthrough layer.

Evidence:

- `README.md:75-81`
  - `ClaudeCodeOptions::builder()` supports `.model(...)`, `.max_turns(...)`, `.max_output_tokens(...)`, `.allowed_tools(...)`, `.permission_mode(...)`
- `README.md:410-419`
  - builder supports `.system_prompt(...)`, `.model(...)`, `.permission_mode(...)`, `.max_turns(...)`, `.max_thinking_tokens(...)`, `.allowed_tools(...)`, `.cwd(...)`, `.settings(...)`
- `README.md:431-433`
  - supports `Query::set_permission_mode(...)`, `Query::set_model(...)`, and `include_partial_messages(true)`
- `README.md:472-497`
  - documents `allowed_tools`, `disallowed_tools`, runtime approval hooks, and permission settings
- `src/transport/subprocess.rs:68-72`
  - `SubprocessTransport` stores `ClaudeCodeOptions`
- `src/transport/subprocess.rs:301-314`
  - `build_command()` converts options into concrete Claude Code CLI arguments, including `--include-partial-messages`
- `src/transport/subprocess.rs:138-225`
  - `settings` and `sandbox` are merged and passed through as startup configuration
- `src/transport/subprocess.rs:262-264`
  - `with_cli_path(...)` allows selecting a specific Claude CLI executable path

Operationally, this means `cc-bridge` should assume it can configure most bridge-relevant startup behavior through typed fields such as:

- model
- permission mode
- system prompt
- working directory
- allowed/disallowed tools
- settings file / settings JSON
- sandbox config
- partial-message streaming
- auto-download and CLI path selection

However, this is **not yet evidence of a generic `extra_args: Vec<String>` or arbitrary raw-flag passthrough API**. If `cc-bridge` needs fully open-ended CLI flag forwarding in the future, we should assume one of these will be necessary:

1. patch `cc-sdk`
2. add a thin custom subprocess launcher below the adapter
3. keep the initial `cc-bridge` scope limited to the structured options already supported by `ClaudeCodeOptions`

For the first implementation, the existing structured option surface appears sufficient.

---

## Feasibility judgment

## 1. Is a new `cc-bridge` command feasible?

Yes.

A dedicated `cc-bridge` command can plausibly use:

- `cc-sdk` subprocess transport first for easier bring-up, or
- `cc-sdk` WebSocket transport if an external Claude Code endpoint is available

The command would not need to speak ACP internally. It only needs to map:

- XMTP inbound message
- -> `cc-sdk` input/control operations
- -> streamed output/tool-ish events
- -> XMTP replies, reactions, and progress

## 2. Can it coexist with the current `acp` command?

Yes, and that is the recommended approach.

`acp` and `cc-bridge` should be treated as two agent transport adapters sharing a common XMTP-facing core.

## 3. Does `cc-sdk` solve the current silent-stale problem by itself?

Not automatically.

It improves the **agent transport side** because it has explicit keepalive and reconnect behavior.

But the currently observed stale path is on the **XMTP conversation stream side**:

- daemon per-conversation history stream
- bridge consumption of that conversation stream

So `cc-sdk` helps with half of the system:

- Claude Code transport liveness, reconnect, replay

It does **not** remove the need to harden the XMTP-side per-conversation runtime.

---

## What is attractive about this path

### Better transport control than ACP subprocess stdio alone

Compared to the current ACP path, `cc-sdk` WebSocket offers:

- explicit keepalive interval
- explicit reconnect policy
- explicit disconnect reasons
- replay buffer
- dedup header
- transport abstraction already separated in the library

### Cleaner path for Claude Code specific features

If the target is specifically Claude Code, `cc-sdk` is likely a better native fit than forcing everything through ACP client glue.

### Good opportunity for a bridge-core refactor

The current bridge implementation in `crates/xmtp-cli/src/acp.rs` mixes several responsibilities:

- XMTP conversation subscription
- session persistence
- prompt orchestration
- tool-call observation
- progress feedback
- reply splitting
- ACP client implementation details

Adding `cc-bridge` creates a strong reason to split these apart cleanly.

---

## Constraints and risks

### 1. `cc-sdk` is Claude Code specific, not protocol-generic

This is the biggest conceptual difference versus ACP.

Implication:

- `cc-bridge` is a Claude Code adapter
- `acp` remains the generic agent protocol path

That is fine, but it should be explicit in the architecture.

### 2. Tool-call surface is not guaranteed to match ACP 1:1

The current ACP path benefits from explicit `ToolCall` and `ToolCallUpdate` events.

Relevant current ACP-specific logic lives in:

- `crates/xmtp-cli/src/acp.rs:2294-2479`
  - `BridgeClient`
  - tool call tracking
  - reaction mapping
  - full-message progress

`cc-sdk` may expose a different event granularity or message taxonomy. Tool progress rendering can likely be shared at the last stage, but event extraction/mapping will be adapter-specific.

### 3. Session semantics are different

Current ACP path uses session concepts in `run_acp()` and session persistence in:

- `crates/xmtp-cli/src/acp.rs:359-533`

`cc-sdk` has its own connection/session behavior. Session resume logic may need to be adapter-specific rather than shared.

### 4. XMTP-side stale stream remains unsolved unless explicitly refactored

Current daemon per-conversation stream logic:

- `crates/xmtp-daemon/src/lib.rs:2756-2869`

Current bridge-side SSE idle handling:

- `crates/xmtp-cli/src/acp.rs:1045-1058`

A `cc-bridge` path still depends on XMTP-side conversation delivery unless the bridge architecture is also changed there.

---

## Recommended feasibility decision

### Recommended verdict

Proceed.

But proceed with this framing:

- **Do not replace `acp`.**
- **Add `cc-bridge` as a parallel bridge path.**
- **Refactor shared bridge logic first or during the implementation.**
- **Treat `cc-sdk` transport benefits as additive, not a silver bullet for all stale-stream issues.**

### Recommended initial scope

Build `cc-bridge` as:

- Claude Code specific
- one conversation per process, matching current bridge model
- minimal text streaming first
- basic progress feedback second
- richer tool-event mapping third

### Things that are likely reusable immediately

- XMTP send/reply/reaction helpers
- markdown reply chunking
- progress batching
- structured logging format
- active turn bookkeeping concepts

### Things that should remain adapter-specific

- session init/resume protocol
- transport connect/reconnect semantics
- incoming agent event parsing
- control/interrupt semantics
- tool event mapping source

---

## Final call

A parallel `cc-bridge` is technically viable and strategically worthwhile.

Its biggest value is twofold:

1. it provides a Claude Code-native path with better transport controls than plain ACP stdio bridging alone
2. it creates the right pressure to refactor the current monolithic `acp.rs` bridge into a shared core plus protocol adapters

That said, adopting `cc-sdk` does not eliminate the need to fix the current XMTP-side silent-stale stream problem. It only gives a stronger foundation on the agent side.
