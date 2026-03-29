# ACP Reaction Status Plan

## Goal

When `xmtp-cli acp` processes an incoming XMTP message, add reactions to the source message to expose agent progress and failures:

- Enter tool call: react `🛠️`
- Enter subagent: react `🤖`
- Error during handling: react `⚠️`

Confirmed product rule:

- Reactions are additive
- Reactions do not need to represent a single latest state
- The same emoji may be added multiple times by the same actor to the same message
- We do not need to remove or replace earlier reactions for this feature

## Why This Fits The Current Architecture

- XMTP reactions are already supported by the daemon and CLI bridge
- `claude-agent-acp` already emits ACP session updates that expose tool activity
- Our bridge currently only consumes `AgentMessageChunk`, but the protocol stream can provide enough signal to drive reactions
- Additive reactions reduce state-management complexity: we do not need a per-message "current status" FSM

## Relevant Signals Available Today

From `claude-agent-acp`, the bridge can infer:

- `tool_call`: tool execution started
- `tool_call_update`: tool progressed / completed / failed
- Tool name in `_meta.claudeCode.toolName` or tool-call payload
- Subagent-like operations via Claude Code tool names such as `Agent` or `Task`
- Prompt-level failure when `conn.prompt(...)` returns an error
- Empty final reply in our bridge, which we already treat as a user-visible failure path

## Desired Mapping

### 1. Tool Start -> `🛠️`

Trigger when receiving ACP updates that indicate a tool has started:

- `tool_call`
- optionally first `tool_call_update` if some tools skip an initial `tool_call`

Rules:

- Ignore subagent tools here if they are covered by the `🤖` rule below
- React to the original XMTP message that triggered the current agent turn
- Do not deduplicate across repeated tool calls

### 2. Subagent Start -> `🤖`

Trigger when receiving a tool event whose tool name indicates delegation / nested agent work.

Initial mapping:

- `Agent`
- `Task`

Rules:

- Treat these as a separate status from normal tool execution
- Allow `🤖` to stack on top of prior `🛠️`
- Do not block later `⚠️`

### 3. Failure -> `⚠️`

Trigger when any of these happen while handling the current XMTP message:

- `tool_call_update` reports failed status
- `conn.prompt(...)` returns error
- bridge produces "no reply" fallback
- any bridge-level agent error we already convert into an XMTP error message

Rules:

- `⚠️` is additive; do not remove prior reactions
- If multiple failures happen in one turn, repeating `⚠️` is acceptable

## Implementation Plan

### Phase 1: Capture ACP Session Updates In The Bridge

Extend `BridgeClient::session_notification()` in `crates/xmtp-cli/src/acp.rs` to process more than `AgentMessageChunk`.

Add handling for:

- `tool_call`
- `tool_call_update`

Store lightweight per-session progress events so the message-processing loop can act on them.

Recommended shape:

- Keep current text-chunk buffer
- Add a second per-session event buffer for progress/status events

Example internal event enum:

```rust
enum AcpProgressEvent {
    ToolStarted { tool_name: Option<String> },
    ToolFailed { tool_name: Option<String> },
}
```

### Phase 2: Track The Current XMTP Source Message

In `bridge_history_to_acp()`, when a user message arrives, treat its `message_id` as the active source message for this turn.

For each incoming XMTP message being processed:

- clear any stale per-session ACP progress events
- prompt the agent
- consume progress events produced during that prompt
- emit reactions against the source `message_id`

This keeps reactions tied to the correct XMTP user message.

### Phase 3: Emit Reactions From Progress Events

Add a helper that maps buffered ACP progress events into daemon reaction calls:

- normal tool start -> `🛠️`
- subagent start (`Agent` / `Task`) -> `🤖`
- failed tool / bridge failure -> `⚠️`

Recommended behavior:

- send reactions in observed order
- best-effort only: reaction failure should not abort the bridge
- log every reaction attempt into ACP log stream for debugging

### Phase 4: Hook Bridge-Level Failures

For failures outside ACP tool updates, reuse the same reaction helper:

- prompt error
- empty reply fallback
- any future explicit bridge error path

This ensures `⚠️` is sent even when the failure is not surfaced as a `tool_call_update`.

## Data-Flow Sketch

1. XMTP message arrives
2. bridge records `source_message_id`
3. bridge sends prompt to ACP agent
4. during prompt, ACP emits session updates
5. bridge buffers tool-start / tool-failed signals
6. after prompt completes or fails:
   - apply reactions to `source_message_id`
   - send normal reply or error reply to XMTP

## Suggested Code Changes

Primary file:

- `crates/xmtp-cli/src/acp.rs`

Likely additions:

- new progress-event buffer on `BridgeClient`
- helper to extract tool name and status from ACP updates
- helper to send progress reactions
- integration in `prompt_agent()` or directly around its call site

No daemon API change should be required if existing message reaction endpoint remains sufficient.

## Edge Cases

### Multiple tool calls in one turn

Expected outcome:

- multiple `🛠️` reactions are acceptable

### Tool call then subagent

Expected outcome:

- `🛠️` then `🤖`, both can coexist

### Tool fails after prior success

Expected outcome:

- earlier `🛠️` remains
- add `⚠️`

### Agent emits no usable text

Expected outcome:

- add `⚠️`
- send fallback error message to XMTP

### Reaction send itself fails

Expected outcome:

- log warning
- continue bridge flow

## Verification Plan

### Unit Tests

Add tests for:

- mapping ACP tool names to `🛠️` vs `🤖`
- failed tool update -> `⚠️`
- multiple tool events preserve additive behavior

### Bridge-Level Tests

Add tests for:

- prompt error triggers `⚠️`
- empty reply fallback triggers `⚠️`
- reaction failure does not abort prompt handling

### Manual Validation

Use a known ACP agent that:

- performs a normal tool call
- delegates to subagent / task
- triggers a controlled error

Confirm the original XMTP user message receives:

- `🛠️` on tool start
- `🤖` on subagent start
- `⚠️` on failure

## Non-Goals

- Removing or collapsing earlier reactions
- Maintaining a single authoritative "current state"
- Building a generalized ACP event viewer in this change

## Recommended Implementation Order

1. Parse and buffer `tool_call` / `tool_call_update`
2. Add reaction helper and best-effort send path
3. Wire `🛠️` and `🤖`
4. Wire `⚠️` for tool failures and bridge failures
5. Add tests
