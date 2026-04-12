# Phased Plan for `acp` + `cc-bridge`

## Executive summary

Recommended strategy:

1. **Do not replace `acp`.**
2. **Do not duplicate `acp.rs`.**
3. **First extract a shared bridge core.**
4. **Then add a minimal `cc-bridge`.**
5. **Then harden daemon-side conversation stream stale detection.**

This sequence gives the best balance between near-term experimentation and long-term maintainability.

---

## Design goal

End state should look conceptually like this:

- `xmtp-cli acp ...`
  - shared XMTP bridge core
  - ACP transport adapter
- `xmtp-cli cc-bridge ...`
  - shared XMTP bridge core
  - Claude Code / `cc-sdk` transport adapter

Shared core should own:

- XMTP event consumption
- turn lifecycle
- reply shaping and delivery
- progress rendering
- observability and logs

Adapters should own:

- transport/session connect
- protocol-specific event parsing
- control/interrupt handling
- transport reconnect semantics

---

## Phase 0 — Research freeze / no production behavior changes

### Objective

Capture the current design and confirm the boundaries before code moves.

### Deliverables

- feasibility doc
- change-scope doc
- phased-plan doc

### Rationale

Avoid starting implementation while the command split and abstraction seams are still fuzzy.

---

## Phase 1 — Extract shared bridge-core utilities from `acp.rs`

### Objective

Reduce `acp.rs` from "bridge runtime + protocol + transport + presentation" into mostly an ACP adapter.

### Extraction targets

#### 1. Shared reply logic

Move out:

- markdown splitting
- stream/single reply shaping
- final reply send helpers

Current source:
- `crates/xmtp-cli/src/acp.rs:1890+`

#### 2. Shared feedback logic

Move out:

- `ReactionLevel`
- full-message batching
- `emit_feedback()`
- common formatting helpers
- tool-title ignore filters that are presentation-only

Current source:
- `crates/xmtp-cli/src/acp.rs:43+`
- `crates/xmtp-cli/src/acp.rs:2009+`
- `crates/xmtp-cli/src/acp.rs:2122+`

#### 3. Shared runtime state

Move out:

- `SessionRuntime`
- active turn metadata
- processed message state helpers
- snapshot helpers

Current source:
- `crates/xmtp-cli/src/acp.rs:386+`
- helper functions around `active_turn_snapshot`

#### 4. Shared logging/observability

Move out:

- structured ACP/bridge log writer (possibly renamed to bridge log layer)
- common turn lifecycle events

Current source:
- `acp_log_path`, `append_acp_log`, `log_acp_event` and related helpers

### Success criteria

- `acp` behavior does not change
- file/module boundaries exist for a second adapter
- `acp.rs` becomes smaller and mostly protocol-specific

---

## Phase 2 — Define transport-neutral bridge event model

### Objective

Create the minimal event model that both adapters can feed.

### Suggested event categories

- `InboundUserMessage`
- `AssistantTextDelta`
- `AssistantFinal`
- `ToolStarted`
- `ToolUpdated`
- `ToolFailed`
- `TransportWarning`
- `TransportError`
- `SessionReady`
- `TransportReconnecting`
- `TransportStale`

### Important principle

Do not try to make this model isomorphic with ACP.

It only needs to be rich enough for the bridge core to:

- drive reply delivery
- render progress
- maintain turn state
- log useful debugging information

### Success criteria

- ACP adapter can map current ACP events into this model without losing critical UX behavior
- model is expressive enough for `cc-sdk` message/control stream

---

## Phase 3 — Add `cc-bridge` command with minimal Claude Code path

### Objective

Ship the smallest useful `cc-bridge` that proves the architecture.

### Initial scope

Support only:

- one conversation per process
- receive user text
- send text into `cc-sdk`
- stream assistant output back to XMTP
- finish on final result
- basic warning/error handling

### Do not require in v1

- full ACP-style tool parity
- action menus parity
- perfect session resume parity
- every current progress rendering edge case

### CLI shape (suggested)

A new command in `main.rs`, for example:

- `CcBridge { conversation_id, transport, sdk_url, reactions, reply_mode, ... }`

Possible transport modes:

- `subprocess` via `cc-sdk` subprocess transport
- `websocket` via `cc-sdk` websocket transport

### Why start minimal

This validates:

- shared core is real
- `cc-sdk` integration is practical
- Claude Code path can run independently of ACP

without overcommitting to parity work too early.

---

## Phase 4 — Add `cc-sdk` WebSocket mode as the preferred experiment path

### Objective

Use `cc-sdk`'s stronger transport mechanics where they matter most.

### Why WebSocket matters here

Observed `cc-sdk` capabilities:

- keepalive every 10s by default
- reconnect supervisor
- reconnect backoff
- replay buffer
- `X-Last-Request-Id` dedup header

This gives a better foundation for transport-level stale handling than raw stdio-only process interaction.

### Validation goals

- confirm keepalive traffic is visible and operationally useful
- confirm reconnects happen on forced transport drops
- confirm no duplicate user-visible messages after reconnect

---

## Phase 5 — Tool/progress parity work

### Objective

Bring `cc-bridge` from text-only usefulness to richer operational parity.

### Areas to add incrementally

#### 1. Tool event mapping

Map `cc-sdk` / Claude Code events into shared bridge tool events where possible.

#### 2. Shared progress rendering

Reuse the shared feedback layer to render:

- reactions
- full-message progress
- warning/done states

#### 3. Interrupt/cancel support

Use `cc-sdk` control requests where available.

#### 4. Session semantics refinement

Decide what resume means in `cc-bridge` and how much should be persisted.

---

## Phase 6 — Harden daemon conversation stream staleness

### Objective

Fix the original motivating issue, not just the agent side.

### Why this phase is necessary

Even after `cc-bridge` exists, the current architecture still depends on the XMTP-side per-conversation stream.

Current concern area:

- `crates/xmtp-daemon/src/lib.rs:2756-2869`

Current bridge-side idle timeout only covers the case where no SSE events at all arrive for too long:

- `crates/xmtp-cli/src/acp.rs:1045-1058`

But the observed production symptom looks like:

- connection/process still alive
- no new business messages for one conversation
- no explicit error

### Recommended hardening work

#### 1. Separate transport heartbeat from business-event freshness

Track:

- last SSE frame time
- last conversation message time
- last history catch-up success time

#### 2. Add business-level stale watchdog

If a conversation stream stays "alive" but produces no business events for too long, log and restart it.

#### 3. Add explicit structured events

Examples:

- `conversation_stream_idle`
- `conversation_stream_business_stale`
- `conversation_stream_restarting`
- `conversation_stream_restarted`

#### 4. Add recovery path

Trigger a stream rebuild when stale is detected, even without a transport error.

---

## Recommended implementation order inside the repo

### Order

1. Create docs (this set)
2. Extract bridge-core modules from `acp.rs`
3. Keep `acp` green with no behavior change
4. Add bridge-neutral event model
5. Add `cc-bridge` command stub
6. Wire minimal `cc-sdk` subprocess transport
7. Add optional `cc-sdk` WebSocket mode
8. Add parity features gradually
9. Harden daemon stream stale detection

### Why this order

It minimizes risk by:

- preserving the working ACP path first
- validating the abstraction before adding the second protocol
- avoiding a huge multi-axis cutover

---

## Suggested document / code review checkpoints

### Checkpoint A — after core extraction

Ask:

- can we explain the split between shared bridge logic and ACP logic clearly?
- did we actually remove duplication, or just rename it?

### Checkpoint B — after minimal `cc-bridge`

Ask:

- can one message round-trip cleanly?
- does streamed output behave acceptably?
- are logs still useful?

### Checkpoint C — after WebSocket path

Ask:

- do heartbeat and reconnect work in practice?
- can we intentionally break the transport and recover?
- do we get duplicates on reconnect?

### Checkpoint D — after daemon stale hardening

Ask:

- can we detect the previously observed silent-stale scenario?
- do we emit explicit logs when it happens?
- does the system self-heal or at least fail loudly?

---

## Rollout recommendation

### Recommended production posture

For a while, keep:

- `acp` = stable/default path
- `cc-bridge` = experimental/targeted path

Use `cc-bridge` first where:

- Claude Code is specifically desired
- transport robustness and liveness introspection matter most
- you want to validate WebSocket keepalive/reconnect behavior

Only consider making `cc-bridge` a preferred path after:

- minimal parity is achieved
- reconnect behavior is exercised in real usage
- daemon-side stale stream handling is improved

---

## Final recommendation

Proceed with the architecture initiative.

But frame it as:

- **bridge-core extraction first**
- **parallel `cc-bridge` second**
- **daemon stale-stream hardening in the same broader effort**

That gives the highest chance of turning this investigation into a durable architectural improvement rather than just introducing a second partially overlapping bridge implementation.
