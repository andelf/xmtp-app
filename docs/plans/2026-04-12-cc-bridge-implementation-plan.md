# CC Bridge Implementation Plan

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Add a new `xmtp-cli cc-bridge` command that can bridge an XMTP conversation to Claude Code via `cc-sdk` while preserving the existing `xmtp-cli acp` path.

**Architecture:** Refactor the current monolithic ACP bridge into a shared XMTP bridge core plus protocol-specific adapters. Keep `acp` as the generic ACP path and add `cc-bridge` as a Claude Code-specific path that can use `cc-sdk` subprocess transport first and WebSocket transport second.

**Tech Stack:** Rust, `clap`, `tokio`, existing `xmtp-daemon` HTTP+SSE bridge, existing `xmtp-ipc` types, `agent-client-protocol`, `cc-sdk`, optional `cc-sdk` `websocket` feature.

---

## Preconditions and reference material

Read these before touching code:

- `AGENTS.md`
- `docs/research/cc-bridge/README.md`
- `docs/research/cc-bridge/feasibility.md`
- `docs/research/cc-bridge/change-scope.md`
- `docs/research/cc-bridge/phased-plan.md`
- `docs/pitfalls/acp-post-method-not-found-hang.md`
- `crates/xmtp-cli/src/acp.rs`
- `crates/xmtp-cli/src/main.rs`
- `crates/xmtp-daemon/src/lib.rs`

Important current architecture facts:

- Current bridge runtime is concentrated in `crates/xmtp-cli/src/acp.rs`
- Current `acp` subcommand is declared in `crates/xmtp-cli/src/main.rs:120-159`
- Current per-conversation daemon stream is produced by `stream_history_events()` in `crates/xmtp-daemon/src/lib.rs:2756-2869`
- `cc-sdk` local evidence:
  - WebSocket feature in crate features
  - `WebSocketTransport`
  - keepalive and reconnect in `src/transport/websocket.rs`

---

## Non-goals for v1

Do **not** try to do all of this in the first implementation pass:

- full tool-event parity between ACP and `cc-sdk`
- action-menu parity
- perfect session-resume parity with ACP
- daemon transport replacement
- replacing SSE with WebSocket on the XMTP side
- large daemon modularization

v1 should prove:

- shared bridge core extraction is workable
- `acp` still works unchanged from the user’s perspective
- `cc-bridge` can round-trip one conversation reliably
- `cc-bridge` can stream text and complete a turn cleanly

---

## Target module layout

Create the following new modules under `crates/xmtp-cli/src/`:

- `bridge/mod.rs`
- `bridge/events.rs`
- `bridge/runtime.rs`
- `bridge/replies.rs`
- `bridge/feedback.rs`
- `bridge/logging.rs`
- `bridge/xmtp.rs`
- `bridge/core.rs`
- `transport/mod.rs`
- `transport/acp.rs`
- `transport/cc.rs`
- `cc_bridge.rs`

Keep:

- `acp.rs` as the ACP command entry/adaptor layer
- `main.rs` as CLI wiring only

Do not leave large shared logic in both `acp.rs` and `cc_bridge.rs`.

---

## Shared abstractions to introduce

### Shared bridge event model

Create a transport-neutral event model in `bridge/events.rs`.

Suggested initial shape:

```rust
#[derive(Debug, Clone)]
pub enum BridgeAgentEvent {
    SessionReady {
        session_label: Option<String>,
    },
    AssistantTextDelta {
        text: String,
    },
    AssistantFinal {
        full_text: String,
    },
    ToolStarted {
        kind: Option<String>,
        title: Option<String>,
    },
    ToolUpdated {
        kind: Option<String>,
        title: Option<String>,
        status: Option<String>,
    },
    ToolFailed {
        kind: Option<String>,
        title: Option<String>,
        error: Option<String>,
    },
    Warning {
        message: String,
    },
    Error {
        message: String,
    },
    TransportReconnecting {
        reason: String,
    },
    TransportStale {
        reason: String,
    },
}
```

Keep this model small. It is for bridge rendering and orchestration, not for representing every detail of ACP or `cc-sdk`.

### Shared agent adapter trait

Define a trait in `bridge/core.rs` or `transport/mod.rs`.

Suggested initial shape:

```rust
#[async_trait::async_trait(?Send)]
pub trait AgentBridgeAdapter {
    async fn connect(&mut self) -> anyhow::Result<()>;
    async fn send_user_message(&mut self, text: String) -> anyhow::Result<()>;
    async fn next_event(&mut self) -> anyhow::Result<Option<BridgeAgentEvent>>;
    async fn interrupt(&mut self) -> anyhow::Result<()>;
    async fn shutdown(&mut self) -> anyhow::Result<()>;
}
```

Do not over-design this trait in v1. Only include methods the bridge core actually needs.

---

## Task 1: Snapshot current behavior before refactor

**Objective:** Lock in current ACP behavior before moving code.

**Files:**
- Test: `crates/xmtp-cli/src/acp.rs`

**Step 1: Identify existing bridge helper coverage**

Read tests already in `acp.rs` around:
- markdown splitting
- reaction mapping
- full-message formatting

**Step 2: Add missing focused tests for bridge-core candidates**

Add tests for:
- `split_markdown_reply()` behavior
- `full_message_description()` behavior
- tool-ignore behavior (`todo`/Other)
- transport-agnostic progress batching helpers if already extractable

Keep tests in `acp.rs` first; they can be moved later.

**Step 3: Run narrow tests**

Run:

```bash
cargo test -p xmtp-cli reaction_mapping
cargo test -p xmtp-cli split_markdown_reply
cargo test -p xmtp-cli full_message
```

Expected: pass.

**Step 4: Commit**

```bash
git add crates/xmtp-cli/src/acp.rs
git commit -m "test: lock in bridge helper behavior before refactor"
```

---

## Task 2: Create `bridge/replies.rs`

**Objective:** Move reply-shaping helpers into a shared module.

**Files:**
- Create: `crates/xmtp-cli/src/bridge/replies.rs`
- Modify: `crates/xmtp-cli/src/bridge/mod.rs`
- Modify: `crates/xmtp-cli/src/acp.rs`
- Test: `crates/xmtp-cli/src/bridge/replies.rs` or temporary tests in `acp.rs`

**Step 1: Move reply helpers**

Move only pure helpers first, such as:
- markdown block splitting
- paragraph splitting
- stream vs single shaping helpers that do not depend on ACP types

If a helper still depends on ACP-specific `PromptReply`, split it into:
- a pure helper that works on strings
- an ACP adapter wrapper left in `acp.rs`

**Step 2: Export module**

In `bridge/mod.rs`:

```rust
pub mod replies;
```

**Step 3: Replace imports in `acp.rs`**

Use explicit imports from `crate::bridge::replies`.

**Step 4: Run focused tests**

Run:

```bash
cargo test -p xmtp-cli split_markdown_reply
```

Expected: pass.

**Step 5: Commit**

```bash
git add crates/xmtp-cli/src/bridge/replies.rs crates/xmtp-cli/src/bridge/mod.rs crates/xmtp-cli/src/acp.rs
git commit -m "refactor: extract shared bridge reply helpers"
```

---

## Task 3: Create `bridge/feedback.rs`

**Objective:** Move shared feedback/progress presentation out of ACP-specific code.

**Files:**
- Create: `crates/xmtp-cli/src/bridge/feedback.rs`
- Modify: `crates/xmtp-cli/src/bridge/mod.rs`
- Modify: `crates/xmtp-cli/src/acp.rs`
- Test: `crates/xmtp-cli/src/bridge/feedback.rs`

**Step 1: Move shared enums/helpers**

Move shared presentation pieces only:
- `ReactionLevel`
- `ReactionEmoji`
- formatting helpers
- tool-reaction ignore helpers
- full-message batch helpers if they can stay transport-neutral

Leave transport-triggered event parsing in `acp.rs` for now.

**Step 2: Keep XMTP send side injectable**

Do not hardwire ACP transport assumptions into this module. Feedback should accept:
- target message id
- formatted event title
- output mode
- sending function or sender abstraction

**Step 3: Move and update tests**

Port current reaction-mapping and todo-ignore tests.

**Step 4: Run focused tests**

Run:

```bash
cargo test -p xmtp-cli reaction_mapping
```

Expected: pass.

**Step 5: Commit**

```bash
git add crates/xmtp-cli/src/bridge/feedback.rs crates/xmtp-cli/src/bridge/mod.rs crates/xmtp-cli/src/acp.rs
git commit -m "refactor: extract shared bridge feedback helpers"
```

---

## Task 4: Create `bridge/runtime.rs` and `bridge/logging.rs`

**Objective:** Extract runtime bookkeeping and structured logging.

**Files:**
- Create: `crates/xmtp-cli/src/bridge/runtime.rs`
- Create: `crates/xmtp-cli/src/bridge/logging.rs`
- Modify: `crates/xmtp-cli/src/bridge/mod.rs`
- Modify: `crates/xmtp-cli/src/acp.rs`

**Step 1: Move runtime structs**

Extract:
- `SessionRuntime`
- `ActiveTurn`
- processed-message tracking helpers
- active-turn snapshot helpers

Keep names if they still fit, but prefer protocol-neutral names where possible.

**Step 2: Move log helpers**

Extract:
- log path helpers
- append jsonl helpers
- structured log writer

Rename ACP-specific names only if it does not create noisy churn.

**Step 3: Preserve log schema compatibility for ACP**

Do not break existing ACP log consumers. The logging helper may be shared, but the event names currently emitted by ACP should remain stable.

**Step 4: Run compile + focused tests**

Run:

```bash
cargo test -p xmtp-cli reaction_mapping split_markdown_reply
cargo build --workspace
```

Expected: pass.

**Step 5: Commit**

```bash
git add crates/xmtp-cli/src/bridge/runtime.rs crates/xmtp-cli/src/bridge/logging.rs crates/xmtp-cli/src/bridge/mod.rs crates/xmtp-cli/src/acp.rs
git commit -m "refactor: extract shared bridge runtime and logging"
```

---

## Task 5: Introduce `BridgeAgentEvent` and adapter trait

**Objective:** Create the protocol-neutral contract that both adapters will implement.

**Files:**
- Create: `crates/xmtp-cli/src/bridge/events.rs`
- Create: `crates/xmtp-cli/src/transport/mod.rs`
- Modify: `crates/xmtp-cli/src/bridge/mod.rs`
- Modify: `crates/xmtp-cli/src/acp.rs`

**Step 1: Add event enum**

Add the initial event model described above.

**Step 2: Add adapter trait**

Create a minimal trait for connect / send / next_event / interrupt / shutdown.

**Step 3: Do not convert all ACP logic yet**

First make the trait compile and add TODO comments where `acp.rs` still uses direct ACP plumbing.

**Step 4: Add one small unit test**

Test that shared feedback logic can consume transport-neutral tool events instead of ACP-only types.

**Step 5: Commit**

```bash
git add crates/xmtp-cli/src/bridge/events.rs crates/xmtp-cli/src/transport/mod.rs crates/xmtp-cli/src/bridge/mod.rs crates/xmtp-cli/src/acp.rs
git commit -m "refactor: add transport-neutral bridge event model"
```

---

## Task 6: Wrap current ACP path in `transport/acp.rs`

**Objective:** Make ACP an adapter instead of the whole bridge.

**Files:**
- Create: `crates/xmtp-cli/src/transport/acp.rs`
- Modify: `crates/xmtp-cli/src/acp.rs`
- Modify: `crates/xmtp-cli/src/transport/mod.rs`

**Step 1: Move ACP-specific event mapping**

Move ACP-specific pieces into `transport/acp.rs`:
- session setup/resume/load
- `BridgeClient`
- `impl acp::Client for BridgeClient`
- tool-call/update parsing
- mapping ACP callbacks into `BridgeAgentEvent`

**Step 2: Keep command signature unchanged**

`run_acp()` should remain the external entrypoint used by `main.rs`, but internally it should instantiate the ACP adapter and shared bridge core.

**Step 3: Preserve current user behavior**

Verify:
- same command-line UX
- same reactions/full-message modes
- same reply flow
- same session persistence behavior

**Step 4: Run build + ACP tests**

Run:

```bash
cargo test -p xmtp-cli reaction_mapping split_markdown_reply
cargo build --workspace
```

**Step 5: Manual smoke check**

If safe in local env, run one local ACP bridge against a non-critical conversation and verify:
- it starts
- it receives one message
- it replies

**Step 6: Commit**

```bash
git add crates/xmtp-cli/src/transport/acp.rs crates/xmtp-cli/src/acp.rs crates/xmtp-cli/src/transport/mod.rs
git commit -m "refactor: isolate ACP bridge as a transport adapter"
```

---

## Task 7: Add `cc-sdk` dependency and feature gating

**Objective:** Add the new dependency without disturbing the ACP path.

**Files:**
- Modify: `crates/xmtp-cli/Cargo.toml`

**Step 1: Add dependency**

Add `cc-sdk` with conservative features.

Suggested initial form:

```toml
cc-sdk = { version = "0.8.1", default-features = false }
```

If subprocess mode depends on defaults, verify first and adjust. Do not enable `websocket` until the adapter compiles unless you need it immediately.

**Step 2: Build**

Run:

```bash
cargo build --workspace
```

**Step 3: Commit**

```bash
git add crates/xmtp-cli/Cargo.toml Cargo.lock
git commit -m "build: add cc-sdk dependency for cc-bridge adapter"
```

---

## Task 8: Create `transport/cc.rs` with minimal subprocess adapter

**Objective:** Implement the smallest useful Claude Code adapter using `cc-sdk` subprocess transport first.

**Files:**
- Create: `crates/xmtp-cli/src/transport/cc.rs`
- Modify: `crates/xmtp-cli/src/transport/mod.rs`

**Step 1: Start with subprocess transport**

Use the simplest stable path first:
- construct `cc_sdk::InteractiveClient` or equivalent transport-backed client
- send user text
- consume streamed messages
- stop a turn when `Message::Result` is received

Use evidence from local crate source:
- `cc-sdk/src/interactive.rs:58-82`
- `cc-sdk/src/interactive.rs:153-220`

**Step 2: Map minimal events**

Initially support mapping only:
- assistant text deltas / chunks
- final result
- errors/warnings if available

Do not block on tool-event parity.

**Step 3: Add adapter tests with a fake transport if practical**

If mocking `cc-sdk` directly is hard, isolate your own event mapping logic and test that.

**Step 4: Commit**

```bash
git add crates/xmtp-cli/src/transport/cc.rs crates/xmtp-cli/src/transport/mod.rs
git commit -m "feat: add minimal cc-sdk subprocess bridge adapter"
```

---

## Task 9: Add `cc_bridge.rs` command runtime

**Objective:** Add a new command entrypoint that uses the shared bridge core with the `cc-sdk` adapter.

**Files:**
- Create: `crates/xmtp-cli/src/cc_bridge.rs`
- Modify: `crates/xmtp-cli/src/main.rs`

**Step 1: Add CLI command**

Add a new subcommand in `main.rs`, for example:

```rust
CcBridge {
    #[arg(long)]
    conversation_id: String,
    #[arg(long, default_value_t = acp::ReactionLevel::Off)]
    reactions: acp::ReactionLevel,
    #[arg(long, default_value_t = acp::ReplyMode::Single)]
    reply_mode: acp::ReplyMode,
    #[arg(long, default_value_t = false)]
    context_prefix: bool,
}
```

Do not expose too many knobs in v1.

**Step 2: Implement `run_cc_bridge()`**

Structure it similarly to `run_acp()`, but it should:
- create the cc adapter
- invoke shared bridge runtime
- not depend on ACP session setup

**Step 3: Compile**

Run:

```bash
cargo build --workspace
```

**Step 4: Commit**

```bash
git add crates/xmtp-cli/src/cc_bridge.rs crates/xmtp-cli/src/main.rs
git commit -m "feat: add cc-bridge command"
```

---

## Task 10: Add minimal integration smoke path for `cc-bridge`

**Objective:** Verify the new command can round-trip one message locally.

**Files:**
- Create/Modify tests as needed under `crates/xmtp-cli/tests/`

**Step 1: Add a test harness or documented smoke script**

If a full automated integration test is too heavy immediately, create:
- one small smoke test if possible, or
- one documented manual verification script checked into `docs/` or test comments

**Step 2: Verification**

Minimum success criteria:
- command starts
- message enters adapter
- assistant output streams or finalizes
- one reply is sent back to XMTP

**Step 3: Commit**

```bash
git add crates/xmtp-cli/tests
# or add docs/manual-smoke if that is all that is possible now
git commit -m "test: add cc-bridge smoke verification"
```

---

## Task 11: Enable `cc-sdk` WebSocket transport behind a flag

**Objective:** Add the transport mode that motivated this architecture work.

**Files:**
- Modify: `crates/xmtp-cli/Cargo.toml`
- Modify: `crates/xmtp-cli/src/transport/cc.rs`
- Modify: `crates/xmtp-cli/src/main.rs`

**Step 1: Enable websocket feature**

Update dependency if needed:

```toml
cc-sdk = { version = "0.8.1", default-features = false, features = ["websocket"] }
```

**Step 2: Add transport mode enum**

Add CLI-selectable mode, for example:
- `subprocess`
- `websocket`

**Step 3: Add WebSocket config wiring**

Map CLI args into `cc_sdk::WebSocketConfig`.

Relevant local source:
- `README.md:139-173`
- `src/transport/websocket.rs:49-83`
- `src/transport/websocket.rs:557-720`

**Step 4: Add a reconnect-focused smoke test plan**

At minimum document or test:
- kill WebSocket endpoint / drop network
- verify reconnect attempts occur
- verify no duplicate visible responses after reconnect

**Step 5: Commit**

```bash
git add crates/xmtp-cli/Cargo.toml Cargo.lock crates/xmtp-cli/src/transport/cc.rs crates/xmtp-cli/src/main.rs
git commit -m "feat: add cc-bridge websocket transport mode"
```

---

## Task 12: Add shared tool/progress mapping for `cc-bridge`

**Objective:** Bring `cc-bridge` closer to ACP UX parity without forcing protocol equivalence.

**Files:**
- Modify: `crates/xmtp-cli/src/transport/cc.rs`
- Modify: shared feedback modules

**Step 1: Identify `cc-sdk`-visible event types**

Only map what the crate actually exposes clearly.

**Step 2: Convert those into `BridgeAgentEvent::ToolStarted/Updated/Failed` where possible**

If exact tool semantics are weaker than ACP, degrade gracefully rather than inventing false precision.

**Step 3: Verify full-message progress output**

Use shared `emit_feedback`/batching path.

**Step 4: Commit**

```bash
git add crates/xmtp-cli/src/transport/cc.rs crates/xmtp-cli/src/bridge/feedback.rs
 git commit -m "feat: add shared progress rendering for cc-bridge"
```

---

## Task 13: Harden daemon conversation-stream stale detection

**Objective:** Address the original silent-stale issue on the XMTP side.

**Files:**
- Modify: `crates/xmtp-daemon/src/lib.rs`
- Possibly modify: `crates/xmtp-cli/src/bridge/xmtp.rs` or ACP runtime logging

**Step 1: Add explicit stale metrics/logging**

In or around `stream_history_events()` track:
- last emitted history event time
- last successful catch-up time
- reconnect attempts / restart count

**Step 2: Add explicit daemon log lines for business-level staleness**

Examples:
- `history stream business stale conversation=... idle_secs=...`
- `history stream restarting conversation=... reason=business_stale`

**Step 3: Add restart trigger for conversation stream if stale**

Do not rely solely on transport termination or explicit stream failure.

**Step 4: Add tests if practical**

Even a narrow unit/integration test for restart triggering is valuable.

**Step 5: Commit**

```bash
git add crates/xmtp-daemon/src/lib.rs
git commit -m "fix: detect and recover stale conversation history streams"
```

---

## Task 14: Final verification pass

**Objective:** Prove both bridge paths still work.

**Files:**
- No new files required unless docs are updated

**Step 1: Run required project build**

Run:

```bash
cargo build --workspace
```

**Step 2: Run fast CLI/TUI-adjacent tests**

Run:

```bash
cargo test -p xmtp-cli
cargo test -p xmtp-tui --lib
```

**Step 3: Manual bridge verification**

Verify both commands on safe conversations:
- `xmtp-cli acp ...`
- `xmtp-cli cc-bridge ...`

Check:
- receives message
- replies
- stream mode works if enabled
- reactions/full-message progress still behave acceptably

**Step 4: Update docs if command usage changed**

Update docs under `docs/research/cc-bridge/` or command docs if necessary.

**Step 5: Commit**

```bash
git add docs crates/xmtp-cli crates/xmtp-daemon
git commit -m "docs: finalize cc-bridge rollout documentation"
```

---

## Verification checklist for the whole project

- [ ] `acp` command still works end-to-end
- [ ] `cc-bridge` can handle a single message round-trip
- [ ] `cc-bridge` can stream output to XMTP
- [ ] shared reply splitting logic behaves identically across adapters
- [ ] shared progress formatting is reused
- [ ] WebSocket mode can reconnect after forced disconnect
- [ ] daemon emits explicit logs for conversation-stream stale/restart paths
- [ ] no new clippy/build regressions

---

## Suggested commit sequence

1. `test: lock in bridge helper behavior before refactor`
2. `refactor: extract shared bridge reply helpers`
3. `refactor: extract shared bridge feedback helpers`
4. `refactor: extract shared bridge runtime and logging`
5. `refactor: add transport-neutral bridge event model`
6. `refactor: isolate ACP bridge as a transport adapter`
7. `build: add cc-sdk dependency for cc-bridge adapter`
8. `feat: add minimal cc-sdk subprocess bridge adapter`
9. `feat: add cc-bridge command`
10. `test: add cc-bridge smoke verification`
11. `feat: add cc-bridge websocket transport mode`
12. `feat: add shared progress rendering for cc-bridge`
13. `fix: detect and recover stale conversation history streams`
14. `docs: finalize cc-bridge rollout documentation`

---

## Final recommendation

Implement this in phases, preserving a working `acp` path after every major extraction.

The decisive architectural rule is:

> **One shared XMTP bridge core, two transport adapters.**

Do not allow the codebase to drift into two independent monolithic bridge implementations.
