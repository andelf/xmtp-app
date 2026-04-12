# ACP bridge can stop responding after `Method not found`

## Status

Known issue. Not fixed yet.

## Symptom

In an XMTP ACP conversation, a turn fails with an agent-side error such as:

- `ACP prompt failed: ACP prompt: Method not found`

After that, later messages may still be **received** by the bridge and get the initial `👀` reaction, but no final reply arrives.

Observed user-facing pattern:

1. A turn fails with `Method not found`
2. Bridge sends an ACP error message for that turn
3. Later user messages show `👀`
4. But there is no subsequent:
   - `agent responded elapsed_ms=...`
   - `sending reply ...`
   - or a new explicit prompt failure

From the user's perspective, the backend looks like it no longer responds.

## Concrete observed case

Conversation:
- `f91a784bb9961cbe12e35c91689df61a`

Evidence sequence from ACP debug log:

1. Failure turn:
- `2026-04-12T07:46:53.704+08:00 ERROR ACP prompt failed: ACP prompt: Method not found`
- followed by `sent ACP error message`

2. Later stuck turn:
- `2026-04-12T08:26:10.864+08:00 INFO received conversation message ... 出什么错了？刚刚`
- `2026-04-12T08:26:10.865+08:00 INFO prompting agent`
- `2026-04-12T08:26:12.922+08:00 INFO reaction sent ... 👀`
- and then no final reply / no prompt completion log

## Current understanding

The important operational fact is:

- after a `Method not found` prompt failure, later turns may enter a degraded state where the bridge still accepts messages but does not complete turns normally

We do **not** yet know whether the root cause is:

- agent subprocess internal state corruption
- ACP session state corruption
- bridge-side prompt lifecycle bug
- an MCP/tool call path that never resolves after the prior error

## Why this is hard to debug currently

Before the added observability, the logs were missing strong per-turn lifecycle markers for:

- prompt source metadata
- whether a failed turn was specifically `Method not found`
- long-running active turn snapshots
- tool-call titles still associated with a stuck active turn

That made it difficult to distinguish:

- slow processing
- prompt failure with recovery
- hung active turn after previous failure

## Logs to inspect on next reproduction

### Debug log
- `data/logs/acp/<short>.<timestamp>.debug.log`

### Structured log
- `data/logs/acp/<conversation_id>.jsonl`

### Live process state
Use tmux / ps / lsof to capture:
- bridge PID
- agent subprocess PID
- whether both are still alive
- which debug log file is still open

## What to collect next time

When the issue reproduces again, capture all of the following before killing anything:

1. **Pane output**
- `tmux capture-pane -p -t <pane> -S -250`

2. **Bridge / agent processes**
- `ps -Ao pid,ppid,stat,etime,%cpu,%mem,command | grep -E 'xmtp-cli|codex|claude|hermes|acp'`

3. **Open files / sockets**
- `lsof -p <bridge_pid>,<agent_pid>`

4. **Relevant debug log window**
- lines around:
  - last successful reply
  - `Method not found`
  - next stuck `prompting agent`

5. **Structured ACP events**
Search for:
- `agent_prompt`
- `agent_prompt_result`
- `turn_still_active`
- `acp_tool_call`
- `acp_tool_call_update`
- `xmtp_error_sent`

## Instrumentation added

Additional ACP observability has been added in `crates/xmtp-cli/src/acp.rs` to help the next reproduction:

- subprocess spawn event with PID + argv
- richer `agent_prompt` metadata:
  - session id
  - source message id
  - sender inbox id
  - content kind
  - source preview
- structured `agent_prompt_result` event:
  - success/failure
  - elapsed ms
  - whether error contained `Method not found`
- periodic `turn_still_active` event while a turn remains open:
  - active turn age
  - source metadata
  - last reply message id
  - started tool-call count
  - current tool-call titles

## Next likely fix directions

Not implemented yet. Candidates:

1. add prompt timeout around `conn.prompt(...)`
2. mark session/bridge degraded after `Method not found`
3. restart agent subprocess and/or ACP session after prompt-class failures
4. expose explicit user-visible recovery status instead of only `👀`
