# Change Scope for Shared Bridge Core + `acp` / `cc-bridge`

## Executive summary

This change is feasible, but it is not a tiny feature addition.

If done well, it is a **medium-to-large refactor** centered in `xmtp-cli`, with some daemon hardening work recommended in parallel.

The change surface is concentrated in three areas:

1. **`xmtp-cli` bridge architecture** — large
2. **CLI command surface** — small to medium
3. **daemon per-conversation stream robustness** — medium, recommended even if `cc-bridge` is added

The current codebase is favorable to this refactor in one sense: most of the complexity is already concentrated in a single file, so the seams are easy to identify. The downside is that extracting those seams cleanly will take deliberate work.

---

## Current architecture hotspots

### Current ACP bridge entrypoint

The current bridge command is declared in:

- `crates/xmtp-cli/src/main.rs:120-159`

It exposes one bridge-oriented subcommand today:

- `Acp { conversation_id, context_prefix, reactions, reply_mode, actions, resume, command }`

### Current ACP bridge implementation

The main implementation lives in a single file:

- `crates/xmtp-cli/src/acp.rs`

Key hotspots:

- entrypoint:
  - `run_acp()` at `acp.rs:135+`
- main loop:
  - `bridge_history_to_acp()` at `acp.rs:738+`
- prompt dispatch:
  - `prompt_agent()` at `acp.rs:1302+`
- message splitting:
  - `split_markdown_reply()` at `acp.rs:1910+`
- progress batching:
  - `FullMessageProgressSender` at `acp.rs:2009+`
- user-facing feedback:
  - `emit_feedback()` at `acp.rs:2122+`
- ACP-side tool event handling:
  - `BridgeClient` at `acp.rs:2294+`
- ACP client implementation:
  - `impl acp::Client for BridgeClient` at `acp.rs:2611+`

This means one file currently mixes:

- bridge runtime
- agent transport lifecycle
- protocol adaptation
- session persistence
- prompt/turn state
- tool progress rendering
- XMTP delivery

That is the main reason this work is larger than "add a second command".

### Current daemon stream runtime

Relevant daemon code:

- history stream constants at `crates/xmtp-daemon/src/lib.rs:47-50`
- per-conversation history loop at `crates/xmtp-daemon/src/lib.rs:2756-2869`
- global app broadcast channel at `crates/xmtp-daemon/src/lib.rs:2000-2003`
- app event broadcast stream at `crates/xmtp-daemon/src/lib.rs:3461-3503`

This matters because the transport split in `xmtp-cli` does **not** remove the daemon-side conversation stream design.

---

## What can be reused as shared bridge core

The following pieces are conceptually protocol-neutral and should move out of ACP-specific code.

### 1. XMTP conversation runtime orchestration

Reusable concept:

- subscribe to one conversation
- consume history/live events
- dedupe / remember processed items
- catch-up after reconnect
- route inbound messages into an agent adapter
- send replies/reactions/progress back to XMTP

Today this is interleaved inside `bridge_history_to_acp()`.

### 2. Turn bookkeeping / observability

Reusable concept:

- active turn state
- source message metadata
- last reply message id
- turn age tracking
- structured jsonl logging
- stuck-turn snapshots

Today visible in:

- `SessionRuntime` / `ActiveTurn` / snapshots around `acp.rs:386+`
- `turn_still_active` logging around the run loop and prompt monitor

### 3. Reply shaping and delivery

Reusable concept:

- single vs stream reply mode
- markdown-aware splitting
- final reply send
- streamed part send
- completion reaction

Today visible in:

- `split_markdown_reply()` and related helpers at `acp.rs:1910+`
- send/reply mode handling in the main bridge loop

### 4. Progress / tool-feedback presentation

Reusable concept:

- reactions vs full-message progress
- batching short progress lines
- done / warning semantics
- filtering noisy tool events

Today visible in:

- `FullMessageProgressSender` at `acp.rs:2009+`
- `emit_feedback()` at `acp.rs:2122+`
- tool reaction filtering and mapping around `acp.rs:2305+` and `acp.rs:2511+`

This logic should become shared presentation logic fed by transport-neutral event types.

---

## What is ACP-specific and should remain adapter-specific

### 1. ACP session lifecycle

Examples:

- `session/new`
- `session/resume`
- `session/load`
- persisted ACP session ids

Current code:

- session persistence around `acp.rs:359-533`
- session negotiation/setup around the top of `run_acp()` / `run_acp_inner()`

### 2. ACP client implementation surface

Current code:

- `impl acp::Client for BridgeClient` at `acp.rs:2611+`

This includes explicit method handling and `method_not_found` responses that do not carry over to `cc-sdk`.

### 3. ACP tool/event mapping

Current code:

- `BridgeClient::handle_tool_call()` / `handle_tool_call_update()` at `acp.rs:2305+`

These are based on ACP event types and should become ACP adapter logic that outputs transport-neutral bridge events.

---

## What is `cc-sdk`-specific and should remain adapter-specific

### 1. WebSocket / subprocess transport selection

`cc-sdk` supports both, but the bridge should treat them as implementation details of a Claude Code adapter.

### 2. `cc-sdk` message/control model

`cc-sdk` uses its own:

- `Transport` trait
- `InputMessage`
- `Message`
- `ControlRequest`

Observed in:

- `cc-sdk/src/interactive.rs:5-6`
- `cc-sdk/src/interactive.rs:58-82`
- `cc-sdk/src/interactive.rs:242-245`

### 3. WebSocket transport behavior

Observed in:

- `cc-sdk/src/transport/websocket.rs`

Especially:

- keepalive task at `557-575`
- supervisor/reconnect loop at `577-720`
- replay/dedup support at `304-320` and `265-267`

These are Claude Code transport concerns, not generic bridge-core concerns.

---

## Recommended module split

A realistic target layout inside `crates/xmtp-cli/src/` would be something like:

- `bridge/mod.rs`
- `bridge/runtime.rs`
- `bridge/events.rs`
- `bridge/session.rs`
- `bridge/feedback.rs`
- `bridge/replies.rs`
- `bridge/logging.rs`
- `bridge/xmtp.rs`
- `bridge/core.rs`
- `transport/acp.rs`
- `transport/cc.rs`
- `acp.rs` (thin command adapter)
- `cc_bridge.rs` (thin command adapter)

### Proposed responsibilities

#### `bridge/events.rs`
Shared bridge-side event model, for example:

- user inbound item
- assistant delta/final
- tool started/updated/failed
- warning/error
- transport stale/reconnecting
- session ready

#### `bridge/runtime.rs`
Owns:

- active turn state
- processed-message tracking
- turn monitor state
- runtime stats/observability snapshots

#### `bridge/feedback.rs`
Owns:

- reaction/full-message modes
- batching
- tool title filtering
- formatting user-visible progress lines

#### `bridge/replies.rs`
Owns:

- markdown splitting
- stream vs single reply shaping
- truncation / formatting rules

#### `bridge/xmtp.rs`
Owns:

- send message
- send reaction
- read/catch-up helpers
- conversation-specific stream consumption wrapper

#### `transport/acp.rs`
Owns:

- ACP session management
- ACP client implementation
- ACP event parsing/mapping into shared bridge events

#### `transport/cc.rs`
Owns:

- `cc-sdk` client lifecycle
- optional WebSocket configuration
- message/control consumption
- mapping `cc-sdk` messages into shared bridge events

---

## CLI surface change scope

### Required changes

`crates/xmtp-cli/src/main.rs`

Add a second bridge command, likely something like:

- `CcBridge { ... }`

Expected scope: **small**.

Likely fields:

- `conversation_id`
- maybe `reactions`
- maybe `reply_mode`
- maybe `enable_actions`
- transport mode / URL
- resume/session options if supported
- Claude Code specific options or endpoint config

### Risk

Low risk if command wiring is isolated.

Most risk is not in CLI parsing. It is in how much bridge logic gets duplicated versus extracted.

---

## Daemon change scope

### Strictly required for `cc-bridge`

None, if `cc-bridge` still consumes the same daemon conversation stream endpoints as `acp`.

### Strongly recommended

Medium-scope daemon hardening work should happen either before or alongside this effort, because the motivating issue is in the daemon/bridge conversation stream path.

Recommended focus:

- add explicit stale-stream observability
- distinguish transport heartbeat from business-event freshness
- possibly add conversation-stream watchdog/restart metrics

Current concern area:

- `stream_history_events()` at `xmtp-daemon/src/lib.rs:2756-2869`

This currently has explicit reconnect on failure/end, but not clear protection against business-level silent staleness.

---

## Test impact

### `xmtp-cli`

Expected impact: **medium to large**.

Need tests for:

- shared reply splitting remains unchanged
- shared feedback formatting remains unchanged
- ACP adapter still emits same behavior
- `cc-bridge` adapter handles basic send/stream/result flow
- reconnect/stale handling for transport layer where mockable

### `xmtp-daemon`

Expected impact: **medium** if stream hardening is touched.

Need tests for:

- conversation stream reconnect behavior
- stale stream watchdog behavior if added
- no regressions to existing history event behavior

### End-to-end

Would benefit from at least one integration smoke path per adapter:

- `acp` still works
- `cc-bridge` can receive one user message and send one final response

---

## Documentation impact

Expected impact: **medium**.

At minimum update:

- command help / README if any bridge usage docs exist
- architecture docs describing dual bridge paths
- pitfalls docs for stale stream behavior
- operational docs for when to use `acp` vs `cc-bridge`

---

## Estimated implementation size

## Option A — naive duplication

Add `cc_bridge.rs` by copying much of `acp.rs`.

### Effort
- lower short-term effort
- high long-term maintenance cost

### Risk
- very high divergence risk
- bug fixes duplicated twice
- more likely to regress tool/progress/reply behavior

Not recommended.

## Option B — staged shared-core refactor

Extract shared bridge runtime first, then implement adapters.

### Effort
- medium-to-large

### Risk
- moderate, but controlled if done in phases

Recommended.

## Rough qualitative sizing

### Phase 1: bridge core extraction
- **large** within `xmtp-cli`

### Phase 2: add `cc-bridge` minimal adapter
- **medium**

### Phase 3: parity on tool/progress/action handling
- **medium to large** depending on `cc-sdk` event richness

### Phase 4: daemon-side stale stream hardening
- **medium**

---

## Main risks

### 1. Over-sharing the wrong abstractions

Danger:
- trying to force ACP and `cc-sdk` into the same protocol model

Correct approach:
- share bridge-core semantics
- keep transport/protocol semantics adapter-specific

### 2. Under-sharing and duplicating too much

Danger:
- `acp.rs` and `cc_bridge.rs` fork into near-copies

Correct approach:
- extract feedback/replies/runtime/XMTP plumbing first

### 3. Solving only the agent side, not the XMTP stale-stream side

Danger:
- `cc-bridge` lands, but pane-specific conversation staleness still exists

Correct approach:
- track daemon/bridge stale-stream hardening as part of the same architecture initiative

---

## Final scope judgment

Adding `cc-bridge` is not a tiny bolt-on; it is a realistic architecture refactor centered on `xmtp-cli`.

That said, the change surface is still manageable because:

- most current complexity is already localized in `acp.rs`
- the daemon interface can remain mostly unchanged at first
- the CLI command addition is straightforward
- `cc-sdk` already provides transport primitives that would otherwise need to be built from scratch

### Final classification

- **Feasibility:** high
- **Value:** high
- **Change size:** medium-to-large
- **Biggest refactor area:** `crates/xmtp-cli/src/acp.rs`
- **Biggest operational risk if ignored:** daemon/bridge conversation stream silent staleness remains even after adding `cc-bridge`
