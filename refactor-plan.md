# Refactor Plan

## Goal

Replace the current line-based Unix socket RPC daemon protocol with an `axum`-based local HTTP API plus SSE event streams.

The new design should:

- remove periodic polling for message updates and conversation changes
- let CLI and TUI subscribe to daemon events over SSE
- keep command-style operations as normal HTTP requests
- reduce pre-send sync work so sending a message can start with a single request

## Scope

- daemon transport refactor to `axum`
- SSE event model for status, conversations, and messages
- CLI migration from raw socket RPC to HTTP/SSE
- TUI migration from raw socket RPC to HTTP/SSE
- reduce group-send preflight work

## Non-goals

- full protocol-level rearchitecture of XMTP internals
- remote deployment or public network exposure
- Ratatui redesign unrelated to transport refactor

## Target Architecture

### Transport

- local `axum` server
- listener remains local-only
- preferred first step: local TCP listener on loopback for simpler client support
- optional later step: move HTTP serving back onto Unix socket if desired

### Request Model

- `GET /v1/status`
- `GET /v1/conversations`
- `GET /v1/conversations/:id`
- `GET /v1/conversations/:id/history`
- `POST /v1/direct-message/open`
- `POST /v1/direct-message/send`
- `POST /v1/groups`
- `POST /v1/groups/:id/send`
- `POST /v1/messages/:id/reply`
- `POST /v1/messages/:id/react`

### Event Model

- `GET /v1/events`
- SSE typed events:
  - `status_changed`
  - `conversation_list_changed`
  - `conversation_updated`
  - `history_event`
  - `message_updated`
  - `daemon_error`

### Client Model

- snapshot first
- SSE after snapshot
- no periodic polling for active conversation history
- minimize periodic polling for status and conversation list

## Performance Goals

- sending a message should require one client request to daemon
- group send should not call `sync_welcomes + sync_all + list conversations` on every send
- avoid reopening transport connections for every background refresh

## Implementation Order

1. Add `axum` HTTP server alongside current daemon core logic.
2. Expose read endpoints and command endpoints over HTTP JSON.
3. Introduce daemon event bus and SSE endpoint.
4. Add CLI watch command against SSE and validate live updates.
5. Migrate TUI reads and writes to HTTP.
6. Migrate TUI live updates to SSE.
7. Remove old raw socket RPC path.
8. Optimize send path to avoid repeated pre-send sync/list requests.
9. Review code quality and remove leftover transport duplication.

## Validation

- CLI can fetch status, list conversations, send DM, send group messages over HTTP
- CLI watch receives live history events over SSE
- TUI loads snapshots over HTTP and updates over SSE
- daemon logs clearly show one request per send action
- group send latency is reduced versus current sync-heavy path
- old raw socket request path removed or fully unused

## Review Checklist

- no hidden polling loops remain for active history
- request IDs or event IDs are unique and traceable
- SSE streams are typed and stable
- transport errors surface clearly to CLI and TUI
- send path is no longer doing unnecessary full conversation refresh before every send
