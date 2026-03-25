# XMTP CLI Design

## Decision

This project should start with a feature-complete CLI first.

Ratatui should not be part of the first implementation phase. The CLI does not need to be simple. The priority is to make the XMTP integration, state model, history handling, status reporting, and operational behavior stable first. Once the command model and daemon behavior are proven in real use, a Ratatui frontend can be added on top.

The current product is explicitly a client-side CLI. At this stage we are not designing for server-side deployment, multi-tenant hosting, or a remotely hosted XMTP service.

## Why CLI First

- XMTP is stateful and operationally heavy compared with a thin chat API.
- The hard parts are daemon behavior, sync, local state, message lifecycle, logging, and recovery.
- A CLI makes it easier to expose all internal state and debug surfaces early.
- A TUI built too early would hide problems instead of forcing the data model and command flows to become clear.
- Ratatui can be added later as a presentation layer once the daemon and command semantics are stable.

## Target Architecture

The system should be split into two executables:

- `xmtp-daemon`
- `xmtp-cli`

### `xmtp-daemon`

The daemon is the long-lived local XMTP runtime for the client. It owns:

- XMTP client initialization
- inbox and installation binding
- local database access
- sync cursors
- welcome sync
- message send and receive
- DM and Group operations
- background workers
- retries and reconnects
- event recording
- structured logs

The daemon should be the only local process that talks directly to XMTP and the local XMTP database.

This daemon is part of the client application architecture. It should not be treated as a general server component.

### `xmtp-cli`

The CLI is a thin command client over the daemon. It owns:

- command parsing
- human-readable output
- JSON output for scripting
- querying daemon state
- triggering actions such as send, join, reply, react
- inspecting message history, status, and logs

The CLI should not create its own XMTP runtime or open the XMTP database directly.

## Why A Daemon Is Required

The daemon is not optional if we want stable behavior.

Reasons:

- XMTP relies on local state, sync progress, and background processing.
- multiple short-lived CLI processes would compete for DB access and cursor ownership
- message streaming and retries need a long-lived owner
- history state and detailed event traces are much easier to maintain in one place
- delivery and sync status should survive across individual commands

## Client-Only Scope

The current scope is intentionally client-only.

What this means:

- the daemon is a local companion process for the CLI
- one local user profile owns one local XMTP runtime
- we optimize for local state clarity, debugging, and durability
- we do not currently design for remote API serving
- we do not currently design for multi-user hosting
- we do not currently design for stateless server execution

This should affect all later design choices. If a design mainly makes sense for a remote service but adds complexity to the local client, it should be rejected for now.

## XMTP Capability Boundary

For this project, XMTP should be treated primarily as a client runtime.

Use official XMTP capabilities in these ways:

- use the XMTP client, local database, sync, stream, DM, Group, reply, react, and history-related features as local client runtime features
- let the local daemon own inbox, installation, sync cursors, and long-lived connection state
- treat protocol validation, local persistence, and background sync as client responsibilities exposed through the daemon

Do not optimize the first version around server-oriented assumptions such as:

- remote hosted runtime ownership
- shared multi-user state
- stateless request and response usage
- centralized service-first abstractions

## Primary Product Goal

Build a terminal-first XMTP client that supports:

- DM messaging
- Group messaging
- message reply
- message reactions
- conversation history
- message lifecycle state
- detailed historical event records
- sync and daemon status
- structured logs
- operational debugging

## Functional Scope

### Core Messaging

- send DM messages
- create groups
- send group messages
- join or attach to conversations through XMTP-supported flows
- leave conversations where supported
- reply to a message
- react to a message
- remove a reaction if supported cleanly by the SDK

### Conversation Inspection

- list conversations
- filter by DM or Group
- show unread or recently active conversations
- inspect one conversation in detail
- inspect conversation members
- inspect conversation permissions and metadata

### Message Inspection

- list conversation history
- fetch message details by message ID
- show replies and reactions for a message
- show delivery and processing state
- show raw metadata and content type where useful

### Historical State

The project should explicitly support three kinds of history:

1. Message history
2. Lifecycle state history
3. Detailed event history

#### Message history

This is the normal chat transcript:

- sender
- content
- timestamp
- reply linkage
- reactions
- delivery status

#### Lifecycle state history

This tracks how a local message moves through the system. Example states:

- created locally
- queued
- publishing
- published
- committed
- processed
- synced
- failed
- retried

#### Detailed event history

This is the debugging timeline:

- local persistence time
- daemon event time
- stream receive time
- cursor changes
- validation results
- permission failures
- retry attempts
- error codes
- protocol metadata

### Status And Health

- daemon status
- connection status
- sync status
- last successful sync
- pending action count
- recent error summary
- identity summary
- inbox and installation summary

### Logging

Logging must be a first-class feature.

Required log classes:

- daemon lifecycle logs
- sync logs
- message logs
- protocol and validation logs
- error logs

Required capabilities:

- human-readable output
- JSON output
- filter by level
- filter by conversation
- filter by message ID
- follow mode
- time range filtering

## CLI Command Model

The exact syntax can evolve, but the CLI should support a command set in this shape:

### Daemon

- `xmtp-cli daemon start`
- `xmtp-cli daemon stop`
- `xmtp-cli daemon restart`
- `xmtp-cli daemon status`

### Initialization And Identity

- `xmtp-cli init`
- `xmtp-cli login`
- `xmtp-cli logout`
- `xmtp-cli info self`
- `xmtp-cli info inbox <inbox_id>`

### Status

- `xmtp-cli status`
- `xmtp-cli doctor`

### Conversations

- `xmtp-cli list-conversations`
- `xmtp-cli list-conversations --dm`
- `xmtp-cli list-conversations --group`
- `xmtp-cli list-conversations --unread`
- `xmtp-cli info conversation <conversation_id>`
- `xmtp-cli join <target>`

### DM And Group Messaging

- `xmtp-cli dm <recipient> <message>`
- `xmtp-cli group create --name <name> --member <id>...`
- `xmtp-cli group send <conversation_id> <message>`
- `xmtp-cli leave <conversation_id>`

### Replies And Reactions

- `xmtp-cli reply <message_id> <message>`
- `xmtp-cli react <message_id> <emoji>`
- `xmtp-cli unreact <message_id> <emoji>`

### History And Detail

- `xmtp-cli history <conversation_id>`
- `xmtp-cli history <conversation_id> --limit <n>`
- `xmtp-cli history <conversation_id> --around <message_id>`
- `xmtp-cli info message <message_id>`
- `xmtp-cli trace message <message_id>`
- `xmtp-cli trace conversation <conversation_id>`

### Group Management

- `xmtp-cli group members <conversation_id>`
- `xmtp-cli group add <conversation_id> <member>...`
- `xmtp-cli group remove <conversation_id> <member>...`
- `xmtp-cli group permissions <conversation_id>`
- `xmtp-cli group rename <conversation_id> <name>`

### Logs

- `xmtp-cli logs`
- `xmtp-cli logs daemon`
- `xmtp-cli logs sync`
- `xmtp-cli logs conversation <conversation_id>`
- `xmtp-cli logs message <message_id>`
- `xmtp-cli logs --follow`
- `xmtp-cli logs --json`

## Output Design

Every command should support:

- readable terminal output by default
- `--json` for machine consumption

Human output should be optimized for operators and developers, not for a minimal end-user shell.

The CLI is allowed to be detailed and explicit.

## Local Data Model

The project should persist more than the underlying XMTP state.

Recommended layers:

- XMTP SDK database
- daemon-owned operational state store
- append-only event log

The daemon-owned state should track:

- conversation summaries
- unread counts
- local message lifecycle state
- retry information
- last sync metadata
- recent errors
- traceable event history

## Rust Workspace Layout

The workspace should stay small but explicitly layered.

Recommended crates:

- `crates/xmtp-cli`
- `crates/xmtp-daemon`
- `crates/xmtp-core`
- `crates/xmtp-ipc`
- `crates/xmtp-store`
- `crates/xmtp-logging`
- `crates/xmtp-config`

### `xmtp-cli`

Responsibilities:

- command parsing
- user-facing output
- `--json` output
- daemon request dispatch

This crate should not own XMTP runtime initialization.

### `xmtp-daemon`

Responsibilities:

- daemon main loop
- XMTP runtime ownership
- sync and stream handling
- send, reply, react operations
- local state updates
- event production
- IPC server

This crate is the only place that should directly drive the XMTP client.

### `xmtp-core`

Responsibilities:

- shared domain types
- conversation and message models
- lifecycle state enums
- trace record models
- status summary models

This crate should avoid transport and file-format assumptions.

### `xmtp-ipc`

Responsibilities:

- request and response types
- daemon event subscription types
- IPC error model
- protocol versioning for local client and daemon compatibility

This crate exists so the CLI and daemon speak one stable local protocol.

### `xmtp-store`

Responsibilities:

- local storage traits
- `json` and `jsonl` backend implementation
- snapshot load and save
- append-only event write path
- simple index maintenance

This is a project-local abstraction, not an official XMTP crate.

### `xmtp-logging`

Responsibilities:

- structured log model
- log formatting
- log filters
- correlation helpers by conversation ID and message ID

### `xmtp-config`

Responsibilities:

- config file loading
- default path resolution
- runtime option parsing
- profile and environment configuration

## Daemon IPC Design

The daemon should expose a local IPC interface, not a network service.

Recommended transport:

- Unix domain socket on Unix-like systems

The first version should optimize for local reliability and inspectability, not transport abstraction purity.

### IPC Principles

- local only
- request and response oriented for normal commands
- event subscription support for logs and state changes
- explicit version field in the protocol
- stable object identifiers such as conversation IDs and message IDs

### IPC Request Categories

- daemon lifecycle
- identity and status
- conversation listing and inspection
- DM and Group send actions
- reply and reaction actions
- history queries
- trace queries
- log queries
- sync and repair actions

### IPC Request Examples

- `GetStatus`
- `ListConversations`
- `GetConversation`
- `GetMessage`
- `SendDm`
- `SendGroupMessage`
- `ReplyToMessage`
- `ReactToMessage`
- `ListHistory`
- `TraceMessage`
- `TraceConversation`
- `TailLogs`

### IPC Response Shape

Each response should contain:

- protocol version
- request ID
- success or error result
- payload

Errors should be structured and stable. At minimum:

- error code
- human-readable message
- optional retry hint
- optional object reference such as message ID or conversation ID

### IPC Event Stream

The daemon should support local subscriptions for:

- daemon state changes
- sync progress updates
- message lifecycle updates
- new message notifications
- error events
- structured logs

This is useful immediately for `logs --follow` and later for Ratatui.

## `xmtp-ipc` Rust Type Draft

The IPC contract should be explicit, versioned, and serializable with `serde`.

Suggested top-level model:

```rust
pub type RequestId = String;
pub type ProtocolVersion = u32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcEnvelope<T> {
    pub version: ProtocolVersion,
    pub request_id: RequestId,
    pub payload: T,
}
```

### Request Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonRequest {
    GetStatus,
    Init(InitRequest),
    Login(LoginRequest),
    Logout,
    GetSelfInfo,
    GetInboxInfo { inbox_id: String },
    ListConversations(ListConversationsRequest),
    GetConversation { conversation_id: String },
    Join(JoinRequest),
    ListHistory(ListHistoryRequest),
    GetMessage { message_id: String },
    SendDm(SendDmRequest),
    SendGroupMessage(SendGroupMessageRequest),
    ReplyToMessage(ReplyToMessageRequest),
    ReactToMessage(ReactToMessageRequest),
    RemoveReaction(RemoveReactionRequest),
    TraceMessage { message_id: String },
    TraceConversation { conversation_id: String },
    QueryLogs(QueryLogsRequest),
    TailLogs(TailLogsRequest),
}
```

### Response Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonResponse {
    pub ok: bool,
    pub result: Option<DaemonResponseData>,
    pub error: Option<IpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonResponseData {
    Status(StatusSnapshot),
    InitResult(InitResult),
    LoginResult(LoginResult),
    SelfInfo(SelfInfo),
    InboxInfo(InboxInfo),
    ConversationList(ConversationListResult),
    Conversation(ConversationSnapshot),
    History(HistoryResult),
    Message(MessageDetail),
    ActionAccepted(ActionAccepted),
    Trace(TraceResult),
    LogBatch(LogBatch),
    Ack,
}
```

### Event Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonEvent {
    DaemonStateChanged(DaemonStateChangedEvent),
    SyncProgress(SyncProgressEvent),
    NewMessage(MessageEvent),
    MessageLifecycle(MessageLifecycleEvent),
    ConversationUpdated(ConversationUpdatedEvent),
    ErrorRecorded(ErrorEvent),
    LogRecord(LogRecord),
}
```

### Error Model

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    pub conversation_id: Option<String>,
    pub message_id: Option<String>,
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    DaemonNotRunning,
    InvalidRequest,
    InvalidState,
    NotFound,
    Conflict,
    StorageError,
    XmtpError,
    SyncError,
    PermissionDenied,
    Unsupported,
    Internal,
}
```

### Request Payload Drafts

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitRequest {
    pub profile: Option<String>,
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub env: String,
    pub db_path: Option<String>,
    pub signer_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListConversationsRequest {
    pub kind: Option<ConversationKind>,
    pub unread_only: bool,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListHistoryRequest {
    pub conversation_id: String,
    pub limit: Option<u32>,
    pub around_message_id: Option<String>,
    pub since_unix_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinRequest {
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendDmRequest {
    pub recipient: RecipientRef,
    pub message: OutboundContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendGroupMessageRequest {
    pub conversation_id: String,
    pub message: OutboundContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyToMessageRequest {
    pub message_id: String,
    pub message: OutboundContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactToMessageRequest {
    pub message_id: String,
    pub reaction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveReactionRequest {
    pub message_id: String,
    pub reaction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryLogsRequest {
    pub level: Option<LogLevel>,
    pub conversation_id: Option<String>,
    pub message_id: Option<String>,
    pub since_unix_ms: Option<i64>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailLogsRequest {
    pub level: Option<LogLevel>,
    pub conversation_id: Option<String>,
    pub message_id: Option<String>,
}
```

### Shared Domain Drafts

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationKind {
    Dm,
    Group,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecipientRef {
    Address { value: String },
    InboxId { value: String },
    Ens { value: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutboundContent {
    Text { text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}
```

### Core Response Payload Drafts

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionAccepted {
    pub action_id: String,
    pub status: MessageLifecycleState,
    pub conversation_id: Option<String>,
    pub message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusSnapshot {
    pub daemon_state: DaemonState,
    pub started_at_unix_ms: Option<i64>,
    pub current_profile: Option<String>,
    pub inbox_id: Option<String>,
    pub installation_id: Option<String>,
    pub connection_state: ConnectionState,
    pub sync_state: SyncState,
    pub pending_actions: u32,
    pub recent_error: Option<ErrorSummary>,
}
```

### Status Enum Drafts

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonState {
    Starting,
    Running,
    Stopping,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Degraded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    pub phase: SyncPhase,
    pub last_cursor: Option<String>,
    pub last_successful_sync_unix_ms: Option<i64>,
    pub pending_actions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncPhase {
    Idle,
    Syncing,
    Recovering,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorSummary {
    pub code: String,
    pub message: String,
    pub at_unix_ms: i64,
}
```

### Lifecycle Enum Draft

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageLifecycleState {
    CreatedLocal,
    Queued,
    Publishing,
    Published,
    Committed,
    Processed,
    Synced,
    Failed,
    Retrying,
}
```

This draft is intentionally small. It is enough to start TDD and should be extended only when a failing test requires more shape.

## `json` And `jsonl` Storage Plan

For Phase 1, `json` and `jsonl` are a reasonable choice if they are treated as an implementation detail behind `xmtp-store`.

This is acceptable now because:

- local debugging matters more than advanced query performance
- append-only history is a first-class requirement
- direct file inspection is valuable during early stabilization

This is not intended to be the final long-term database design.

## Storage Record Schema Draft

All persisted records should use explicit schema versions.

Recommended common fields:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordMeta {
    pub schema_version: u32,
    pub written_at_unix_ms: i64,
    pub local_seq: Option<u64>,
}
```

### `config.json` Schema Draft

```json
{
  "schema_version": 1,
  "profile": "default",
  "xmtp_env": "dev",
  "data_dir": "/path/to/data",
  "ipc_socket_path": "/path/to/data/daemon/socket",
  "log_level": "info",
  "feature_flags": {}
}
```

### `state.json` Schema Draft

```json
{
  "schema_version": 1,
  "daemon_state": "running",
  "started_at_unix_ms": 1742880000000,
  "last_successful_sync_unix_ms": 1742880030000,
  "current_profile": "default",
  "inbox_id": "inbox-123",
  "installation_id": "inst-123",
  "connection_state": "connected",
  "sync_state": {
    "phase": "idle",
    "last_cursor": "cursor-abc",
    "pending_actions": 0
  },
  "recent_error": null
}
```

### Conversation Snapshot Schema

`conversations/<conversation_id>.json`

```json
{
  "schema_version": 1,
  "conversation_id": "conv-123",
  "kind": "group",
  "title": "Core Team",
  "derived_label": "Core Team",
  "member_count": 4,
  "members": [
    { "inbox_id": "inbox-a", "label": "alice" }
  ],
  "permissions_summary": {
    "preset": "custom"
  },
  "consent_state": "allowed",
  "unread_count": 2,
  "last_message": {
    "message_id": "msg-9",
    "preview": "hello",
    "sent_at_unix_ms": 1742880040000
  },
  "last_activity_unix_ms": 1742880040000
}
```

### Message Record Schema

`messages/<conversation_id>.jsonl`

Each line should be one `MessageRecordEnvelope`.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRecordEnvelope {
    #[serde(flatten)]
    pub meta: RecordMeta,
    pub conversation_id: String,
    pub message_id: String,
    pub record: MessageRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageRecord {
    MessageReceived {
        sender_inbox_id: String,
        sent_at_unix_ms: i64,
        content: ContentSnapshot,
    },
    MessageSentLocal {
        sender_inbox_id: Option<String>,
        created_at_unix_ms: i64,
        content: ContentSnapshot,
    },
    MessageStatusChanged {
        from: Option<MessageLifecycleState>,
        to: MessageLifecycleState,
        reason: Option<String>,
    },
    MessageReactionAdded {
        actor_inbox_id: String,
        reaction: String,
        target_message_id: String,
    },
    MessageReactionRemoved {
        actor_inbox_id: String,
        reaction: String,
        target_message_id: String,
    },
    MessageReplyLinked {
        reply_to_message_id: String,
    },
}
```

### Event Record Schema

`events/<date>.jsonl`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecordEnvelope {
    #[serde(flatten)]
    pub meta: RecordMeta,
    pub event: EventRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventRecord {
    DaemonStarted {
        pid: u32,
        profile: String,
    },
    DaemonStopped {
        reason: Option<String>,
    },
    SyncStarted {
        scope: String,
    },
    SyncCompleted {
        scope: String,
        duration_ms: i64,
    },
    SyncFailed {
        scope: String,
        error_code: String,
        message: String,
    },
    ConversationUpdated {
        conversation_id: String,
        kind: String,
    },
    GroupMembershipChanged {
        conversation_id: String,
        summary: String,
    },
    ErrorRecorded {
        error_code: String,
        message: String,
        conversation_id: Option<String>,
        message_id: Option<String>,
    },
}
```

### Log Record Schema

`logs/*.jsonl`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRecord {
    #[serde(flatten)]
    pub meta: RecordMeta,
    pub level: LogLevel,
    pub subsystem: String,
    pub message: String,
    pub conversation_id: Option<String>,
    pub message_id: Option<String>,
    pub error_code: Option<String>,
    pub fields: serde_json::Map<String, serde_json::Value>,
}
```

### Trace Record Schema

`traces/messages/<message_id>.jsonl`
`traces/conversations/<conversation_id>.jsonl`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceRecordEnvelope {
    #[serde(flatten)]
    pub meta: RecordMeta,
    pub trace: TraceRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TraceRecord {
    MessageLifecycle {
        conversation_id: String,
        message_id: String,
        state: MessageLifecycleState,
        detail: Option<String>,
    },
    PublishAttempt {
        conversation_id: String,
        message_id: String,
        attempt: u32,
    },
    PublishResult {
        conversation_id: String,
        message_id: String,
        ok: bool,
        detail: Option<String>,
    },
    SyncObserved {
        conversation_id: String,
        message_id: Option<String>,
        cursor: Option<String>,
    },
    ConversationMetadataChanged {
        conversation_id: String,
        summary: String,
    },
    ConversationMembershipChanged {
        conversation_id: String,
        summary: String,
    },
}
```

### Content Snapshot Draft

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContentSnapshot {
    Text {
        text: String,
    },
    Reaction {
        reaction: String,
    },
    Unknown {
        content_type: String,
        raw_preview: Option<String>,
    },
}
```

### Corruption Handling Policy

For Phase 1:

- `json` snapshots should fail fast on invalid content
- `jsonl` readers should skip malformed lines, count them, and surface the count in logs or debug output
- append-only writers should always terminate records with a newline

This policy is chosen to maximize recoverability for timelines and logs while keeping snapshots strict.

### Storage Rules

- use `json` for snapshots and configuration
- use `jsonl` for append-only logs, events, and timelines
- avoid making the daemon depend on physical file layout directly
- make every record self-describing enough for manual inspection
- include timestamps in all persisted event records

### Recommended File Layout

```text
data/
  config.json
  state.json
  daemon/
    pid.json
    socket
  conversations/
    <conversation_id>.json
  messages/
    <conversation_id>.jsonl
  events/
    2026-03-25.jsonl
  logs/
    daemon.jsonl
    sync.jsonl
    message.jsonl
    error.jsonl
  traces/
    messages/
      <message_id>.jsonl
    conversations/
      <conversation_id>.jsonl
```

### `config.json`

Should contain:

- profile name
- XMTP environment
- local data directory
- log level
- IPC socket path
- optional feature flags

### `state.json`

Should contain the latest daemon snapshot:

- daemon start time
- last successful sync time
- current inbox ID
- current installation ID
- connection state
- sync state
- pending action counts
- recent error summary

### `conversations/<conversation_id>.json`

Should contain a conversation snapshot:

- conversation ID
- type: DM or Group
- title or derived label
- member summary
- permission summary
- consent summary
- unread count
- last message summary
- last activity time

### `messages/<conversation_id>.jsonl`

Should contain append-only message records and message-related lifecycle events.

Recommended record kinds:

- `message_received`
- `message_sent_local`
- `message_status_changed`
- `message_reaction_added`
- `message_reaction_removed`
- `message_reply_linked`

### `events/<date>.jsonl`

Should contain a normalized daemon event timeline across all subsystems.

Recommended record kinds:

- `daemon_started`
- `daemon_stopped`
- `sync_started`
- `sync_completed`
- `sync_failed`
- `conversation_updated`
- `group_membership_changed`
- `error_recorded`

### `logs/*.jsonl`

Should contain structured log records with:

- timestamp
- level
- subsystem
- message
- optional conversation ID
- optional message ID
- optional error code

### `traces/messages/<message_id>.jsonl`

Should contain a detailed per-message timeline:

- local creation
- queue entry
- publish attempt
- publish success or failure
- commit processed
- sync observation
- retry events
- terminal state

### `traces/conversations/<conversation_id>.jsonl`

Should contain conversation-level events:

- metadata changes
- membership changes
- permission changes
- high-level sync events

## Storage Record Guidance

Every persisted event-like record should contain at least:

- record type
- timestamp
- local sequence or monotonic counter if available
- conversation ID when relevant
- message ID when relevant
- payload object

When a state transition occurs, prefer writing an explicit new event rather than mutating history.

## Phase 1 Command Priority

Phase 1 should not try to ship the full command tree at once.

Recommended order:

### Priority 0: Runtime Bootstrap

- `xmtp-cli init`
- `xmtp-cli daemon start`
- `xmtp-cli daemon status`
- `xmtp-cli status`

Goal:

- prove config, local paths, socket boot, and basic status reporting

### Priority 1: Identity And Conversation Read Path

- `xmtp-cli login`
- `xmtp-cli info self`
- `xmtp-cli list-conversations`
- `xmtp-cli info conversation <conversation_id>`
- `xmtp-cli history <conversation_id>`

Goal:

- prove that the daemon can own XMTP state and expose readable local history

### Priority 2: Message Write Path

- `xmtp-cli dm <recipient> <message>`
- `xmtp-cli group send <conversation_id> <message>`
- `xmtp-cli reply <message_id> <message>`
- `xmtp-cli react <message_id> <emoji>`

Goal:

- prove that send actions and local lifecycle state tracking are correct

### Priority 3: Debuggability

- `xmtp-cli info message <message_id>`
- `xmtp-cli trace message <message_id>`
- `xmtp-cli logs`
- `xmtp-cli logs --follow`

Goal:

- make failures and timing behavior visible before broadening feature scope

### Priority 4: Group Management

- `xmtp-cli group create`
- `xmtp-cli group members`
- `xmtp-cli group add`
- `xmtp-cli group remove`
- `xmtp-cli group permissions`

Goal:

- expand from core messaging into richer group operations once the base runtime is stable

## Phase 1 Exit Criteria

Before moving to Ratatui or a larger command surface, Phase 1 should satisfy all of the following:

- daemon restart does not corrupt local client state
- conversation history remains readable across restarts
- sent messages have traceable lifecycle records
- sync status is visible and believable
- logs and trace records are sufficient to debug common failures
- DM and Group send flows are stable enough for daily use

## TDD Development Policy

This project should be developed using strict TDD.

Core rule:

- no production code without a failing test first

For every behavior change:

1. write one minimal test for the behavior
2. run it and confirm it fails for the expected reason
3. write the smallest implementation that makes it pass
4. run the focused test again
5. run the relevant broader test set
6. refactor only after green

This applies to:

- new CLI commands
- daemon behavior
- storage behavior
- lifecycle state transitions
- log and trace generation
- bug fixes

The goal is not just coverage. The goal is to prove each behavior by first observing a real failing test.

## Testing Strategy

The test strategy should mirror the crate boundaries.

### Test Layers

- unit tests
- contract tests
- integration tests
- end-to-end CLI tests
- failure and recovery tests

### Unit Tests

Use unit tests for:

- domain models in `xmtp-core`
- storage record encoding and decoding in `xmtp-store`
- log formatting and filtering in `xmtp-logging`
- config parsing in `xmtp-config`
- IPC type validation and serialization in `xmtp-ipc`

Unit tests should avoid full daemon startup whenever possible.

### Contract Tests

Use contract tests to lock down boundaries between crates.

Priority contract surfaces:

- CLI request to IPC request mapping
- daemon response to CLI output mapping
- store trait behavior across implementations
- event record schema stability

These tests are important because the project is intentionally layered.

### Integration Tests

Use integration tests for:

- daemon startup with local config and local data directory
- IPC round-trips over the local socket
- snapshot persistence and reload
- append-only event log behavior
- daemon status reporting
- conversation and message history queries against persisted local records

Integration tests should use isolated temporary directories.

### End-To-End CLI Tests

Use end-to-end tests to validate the actual user workflow.

Phase 1 priority flows:

- init then daemon start then status
- login then info self
- list conversations
- history lookup
- dm send
- group send
- reply
- react
- logs follow
- trace message

These tests should execute the real binaries or binary entrypoints, not only internal functions.

### Failure And Recovery Tests

This project needs explicit recovery testing because local durability is a core feature.

Required scenarios:

- daemon restart with existing state
- truncated or partially written `jsonl` record handling
- missing optional snapshot files
- socket already exists on startup
- message lifecycle contains a failed send followed by retry
- log and trace files remain readable after restart

## Test Ownership By Crate

### `xmtp-core`

Test:

- lifecycle state transitions
- summary object derivation
- message and conversation identity rules
- trace record normalization

### `xmtp-ipc`

Test:

- request and response serialization
- protocol version handling
- structured error encoding
- event stream message shapes

### `xmtp-store`

Test:

- snapshot write then reload
- append-only `jsonl` writes
- ordering guarantees for records written in sequence
- corrupted line handling policy
- conversation and trace file path resolution

### `xmtp-logging`

Test:

- structured log formatting
- filtering by level
- filtering by conversation ID
- filtering by message ID
- follow-mode reader behavior on appended lines

### `xmtp-daemon`

Test:

- daemon boot with empty data directory
- daemon boot with existing state
- IPC request handling
- state snapshot updates after actions
- event and trace emission
- restart safety

### `xmtp-cli`

Test:

- argument parsing
- subcommand mapping
- `--json` output shape
- readable output shape for key commands
- non-zero exit behavior on daemon or command errors

## Phase 1 TDD Slices

Development should proceed in narrow vertical slices.

Recommended first slices:

### Slice 1: Init And Status

Behavior target:

- `xmtp-cli init` creates expected local files
- `xmtp-cli daemon start` creates runnable local daemon state
- `xmtp-cli status` returns a meaningful snapshot

Tests first:

- init creates `config.json` and `state.json`
- daemon status fails cleanly before daemon start
- daemon start followed by status returns running state

### Slice 2: IPC And Basic Daemon Query

Behavior target:

- CLI can talk to daemon and fetch stable status

Tests first:

- `GetStatus` request serializes and deserializes
- daemon replies to `GetStatus`
- CLI renders returned status in text and json

### Slice 3: Conversation Read Path

Behavior target:

- list and inspect persisted conversations

Tests first:

- daemon can read conversation snapshot from local store
- list-conversations returns ordered results
- info conversation returns expected fields

### Slice 4: Message History

Behavior target:

- message history is readable from persisted records

Tests first:

- message record append then history query returns expected transcript
- history around a message ID returns the correct window
- malformed message line does not destroy the whole history query

### Slice 5: Trace And Logs

Behavior target:

- trace and logs expose the lifecycle and debug timeline

Tests first:

- appending message lifecycle events produces trace output
- logs filter by conversation ID
- logs follow emits newly appended records

### Slice 6: Real XMTP Send Path

Behavior target:

- DM send, Group send, reply, and react produce local state changes and visible traces

Tests first:

- successful send creates lifecycle records
- failed send creates error and retry records
- reply action links to source message
- react action records the reaction event

## Test Environment Plan

Tests should use two environments:

- local deterministic tests without XMTP network dependency
- selective integration tests that use real XMTP behavior when practical

Most tests in Phase 1 should be local and deterministic.

Reason:

- TDD works best when tests are fast and failures are unambiguous
- daemon, storage, IPC, history, and logging behavior do not require live network access to validate first

Real XMTP-backed tests should be a smaller layer used to validate wiring and integration assumptions.

## Test Fixtures

Recommended reusable fixtures:

- temporary data directory
- prebuilt `config.json`
- prebuilt `state.json`
- sample conversation snapshot
- sample message history `jsonl`
- sample trace `jsonl`
- sample structured log `jsonl`
- daemon started on a temporary local socket

Fixtures should prefer real serialized records over mocks where practical.

## Command Verification Matrix

Each user-facing command in Phase 1 should have at least:

- one parsing test
- one success-path integration test
- one failure-path integration test
- one output-format test for readable mode or `--json`

Priority commands:

- `init`
- `daemon start`
- `daemon status`
- `status`
- `login`
- `info self`
- `list-conversations`
- `history`
- `dm`
- `group send`
- `reply`
- `react`
- `logs`
- `trace message`

## Storage Verification Matrix

Each persisted file type should have tests for:

- initial creation
- append behavior
- reload behavior
- restart behavior
- malformed content handling

Priority file types:

- `config.json`
- `state.json`
- `conversations/<conversation_id>.json`
- `messages/<conversation_id>.jsonl`
- `events/<date>.jsonl`
- `logs/*.jsonl`
- `traces/messages/<message_id>.jsonl`

## Suggested Test Commands

The exact commands depend on the final Rust workspace setup, but the workflow should support:

- focused crate tests
- focused single-test execution
- workspace-wide regression runs

Example shape:

```bash
cargo test -p xmtp-store message_history_roundtrip
cargo test -p xmtp-daemon daemon_status_before_start
cargo test -p xmtp-cli cli_status_json_output
cargo test --workspace
```

## Test Completion Criteria

Work on a feature slice is complete only when:

- the new behavior has a test that failed before implementation
- the focused test now passes
- the relevant crate tests pass
- the workspace regression set still passes
- the behavior is observable through logs, traces, or status output when applicable

## Non-Goals For Phase 1

- Ratatui UI
- visual chat layout work
- optimizing for simplicity over observability
- hiding protocol details from the operator

Phase 1 should prefer observability, control, and debuggability.

## Phase Plan

### Phase 1: Stable CLI And Daemon

Deliver:

- daemon runtime
- init and login flows
- status
- list conversations
- DM send
- Group send
- history
- reply
- react
- info and trace commands
- logs

Success criteria:

- commands are stable
- local state is durable
- history and detailed traces are reliable
- failures are visible and debuggable

### Phase 2: Broader Group And Admin Features

Deliver:

- group membership management
- group permissions inspection and updates
- better consent handling
- richer repair and resync flows

### Phase 3: Ratatui Frontend

Only start after Phase 1 and key parts of Phase 2 feel stable.

The Ratatui app should be treated as a frontend over the daemon, not as a replacement for the CLI.

#### Ratatui TUI Design

The TUI should be a thin interactive shell over the existing daemon and command model.

It should optimize for:

- fast keyboard-driven chat operations
- stable layout for frequent terminal use
- readable but structured output
- reuse of daemon-owned history, status, and send flows

The TUI should not introduce a separate XMTP runtime.

#### Main Screen Layout

The main screen should use a three-zone layout plus a status bar:

- left pane: conversation list
- center pane: current conversation message list
- bottom pane: message input box
- bottom status bar: identity and connection summary

Suggested proportions:

- conversation list: 28 to 34 columns
- message pane: remaining width
- input box: 3 to 5 rows
- status bar: 1 row

#### Conversation List Pane

The conversation list is the primary navigation surface.

Each row should show:

- conversation name if present
- conversation kind: `dm` or `group`
- short conversation ID
- unread marker if available
- active marker for the currently opened conversation

Display rules:

- the currently selected row should be highlighted
- the currently opened conversation should also be visually marked even if the list focus moves away
- group and DM should use consistent short labels for scan speed

Example row shape:

`> Andelf       group   a68d....2281`

#### Message Pane

The message pane should show the current conversation transcript.

Each message row should include:

- send time
- sender short ID
- message short ID
- content summary

Optional inline suffixes:

- reply count
- reaction count
- delivery or error marker if needed

Message rows should use different colors for:

- self-authored messages
- messages from others
- system or unsupported messages

The visual distinction should be simple and stable:

- self messages: one accent color
- other messages: neutral or contrasting color
- unsupported or system messages: dim or warning color

The currently selected message row should be highlighted independently of message ownership color.

#### Input Pane

The input pane is always visible at the bottom.

It should support:

- normal message input
- multiline input if needed later
- placeholder text when empty
- disabled or read-only state if no conversation is selected

The input pane should not hide message history.

When the input pane has focus, text entry must have priority over global shortcuts.

This means:

- plain character keys should go into the input buffer
- global one-key shortcuts should not fire while typing
- only navigation and explicit control keys should remain active during input focus

In practice, the TUI should treat shortcuts such as `c` and `g` as active only when focus is not in the input pane, or when they are triggered through a dedicated command mode later.

#### Status Bar

The status bar should always be visible and concise.

It should show:

- current user inbox ID in shortened form
- connection state
- daemon state
- current conversation short ID or name
- optional sync status

Example:

`me d15d....76ca | online | daemon running | group Andelf`

#### ID Display Rules

All IDs shown in the TUI should use the same shortened display rules.

Ethereum-style account IDs:

- `0xAAAA....BBBB`

Conversation, inbox, installation, and message IDs:

- `AAAAAA....BBBB`

This rule should apply consistently in:

- conversation list
- message list
- status bar
- action menus
- popups

Full IDs should only appear in explicit detail dialogs or copy actions.

#### Focus Model

The TUI should have three focusable areas:

- conversation list
- message list
- input box

`Tab` should move focus forward in this order:

- conversation list -> message list -> input box -> conversation list

`Shift+Tab` should move focus backward.

Only one pane should have active focus at a time.

Focused pane styling should be stronger than simple selection styling.

#### Keyboard Model

Required keys:

- `Tab`: next focus area
- `Shift+Tab`: previous focus area
- `Up` and `Down`: move selection in the focused list
- `Enter` in input box: send message
- `Alt+Enter` in input box: insert newline
- `Enter` in message list: open message action menu
- `Esc`: close popup or clear transient mode
- `q`: quit TUI

Suggested global shortcuts:

- `Ctrl+N`: create new direct-message target
- `g`: create new group
- `/`: search or jump to conversation later
- `r`: quick reply to selected message when message list is focused
- `e`: quick react to selected message when message list is focused

Shortcut safety rules:

- if the input box has focus, character keys should be treated as message text, not as global commands
- destructive or workflow-changing actions should prefer popup or modal entry over immediate execution
- popup-specific shortcuts should only be active while the popup is open

#### Message Action Menu

When focus is on the message list and the user presses `Enter`, a small popup menu should appear for the selected message.

Initial actions:

- `reply`
- `reaction`

Future actions can be added later:

- `copy id`
- `copy sender`
- `info`

The popup should be simple and keyboard-driven:

- `Up` and `Down` to choose
- `Enter` to confirm
- `Esc` to cancel

Selecting `reply` should move focus to the input box and set a reply context.

Selecting `reaction` should open a lightweight reaction picker or free-text emoji input.

#### Create Chat And Group Flows

The TUI should support direct keyboard creation of new chats.

`Ctrl+N` should open a small dialog instead of triggering immediate creation:

- target inbox ID or address input
- confirm action creates or opens the DM

`g` should open a group creation dialog:

- group name input
- member input list
- confirm action creates the group and switches into it

These dialogs should call the same daemon flows as the CLI commands.

These should be modal flows, not inline edits in the main input box.

Reason:

- they avoid conflict with message typing
- they make validation and confirmation clearer
- they scale better once more fields are needed

#### Conversation Selection Behavior

When the selected conversation changes:

- the message pane should refresh with recent history
- the input box should update its target context
- the status bar should reflect the selected conversation

The first TUI version should use immediate selection-driven switching:

- moving the cursor in the conversation list should immediately load that conversation
- `Enter` is not required to open a conversation from the list

The TUI should keep a distinction between:

- selected conversation in the list
- opened conversation in the message pane

In most cases they will match, but the state model should stay explicit.

#### Unsupported Message Handling

The TUI should not dump raw bytes into the message pane.

For unsupported content, show a stable machine-friendly summary such as:

- `type=unknown content_type=xmtp.org/group_updated:1.0`

If fallback text exists and is trustworthy, the TUI should prefer fallback text first.

If no useful fallback text exists, it should fall back to:

- `type=unknown content_type=...`

#### Data Flow

The TUI should consume daemon-owned data through local IPC and subscriptions.

Read paths:

- conversation list snapshot
- conversation history snapshot
- daemon status snapshot
- daemon-driven live updates for the active conversation

Write paths:

- send direct-message
- send group message
- reply
- react
- create group

The TUI should never talk to XMTP directly.

#### Ratatui State Model

The frontend should keep a small UI state only.

Suggested state objects:

- app focus state
- conversation list items and selected index
- current conversation ID
- message list items and selected index
- input buffer
- popup state
- reply context
- status snapshot

The TUI should not become the source of truth for protocol state.

#### Implementation Plan For First TUI Slice

The first Ratatui implementation should be broken into small slices:

1. shell and layout

- app frame
- pane borders
- focus styling
- status bar
- static keyboard loop

2. conversation browsing

- load conversation list from daemon
- move selection
- open selected conversation
- render short IDs consistently

3. history and watch

- load conversation history
- subscribe to new message events as part of the normal TUI session
- append new messages into the active pane
- keep non-active panes stable

The TUI should not expose a separate `watch` mode toggle.

Reason:

- the TUI itself is already a long-lived interactive session
- the normal active conversation should update live by default

4. input and send

- input buffer
- send current text into active conversation
- clear input on success
- keep error feedback visible in status area

5. message action popup

- open from selected message
- support `reply`
- support `reaction`
- return focus cleanly after action

6. modal creation flows

- create direct-message dialog
- create group dialog
- validate inputs before submit
- switch into the created or opened conversation

#### TUI Decisions Confirmed

The following product decisions are now fixed for the first TUI version.

1. input behavior

- `Enter` sends the current message
- `Alt+Enter` inserts a newline

2. shortcut scope

- `c` and `g` remain shortcut entry points
- `Ctrl+N` and `g` remain shortcut entry points
- they should only be active when focus is outside the input box
- creation flows should open modal dialogs

3. conversation opening behavior

- moving selection in the conversation list immediately switches the active conversation

4. message action entry

- `Enter` on the selected message opens the action menu

5. reaction input style

- the first version should use a fixed small reaction picker

6. conversation list density

- the first version should keep one conversation per line

7. unsupported message display

- if trustworthy fallback text exists, show fallback first
- otherwise show `type=unknown content_type=...`

8. watch behavior

- the TUI should not expose a separate watch mode
- live updates are part of the normal session behavior

9. status bar detail

- the first version should show only:
  `me / online / daemon / current conversation`

10. create-dialog fields

- direct-message creation should support inbox ID and address input
- group creation should support only `name + members` in the first version

#### TUI Open Questions

The main interaction decisions are now fixed.

The remaining open questions are implementation-oriented and can be resolved during build work unless product requirements change:

- exact reaction picker contents
- exact colors for self, others, and unsupported messages
- exact modal sizing rules for narrow terminals
- whether status bar should show short conversation name first or short ID first

#### TUI Subscription Gaps To Close

The current daemon is good enough for command-style request and response flows, but it is not yet exposing a production-grade subscription surface for Ratatui.

Current state:

- normal request and response IPC exists
- `WatchHistory` exists as a narrow history stream concept
- logs can be tailed indirectly

Current gaps:

- no unified event subscription channel
- no stable event type model for the TUI
- no separation between snapshot fetch and live patch events
- no explicit subscription lifecycle for frontend sessions
- no backpressure or resync story if the TUI misses events

For a production-grade TUI, these gaps should be addressed explicitly rather than extending `WatchHistory` ad hoc.

#### Required Daemon Subscription Interfaces

The first TUI version does not need every possible subscription, but it does need a minimal stable set.

Required subscriptions:

1. app status subscription

Purpose:

- keep the status bar current
- reflect daemon or connection state changes

Suggested event payload:

- daemon state changed
- connection state changed
- inbox identity available
- current sync summary changed

2. conversation list subscription

Purpose:

- update the left pane when new DM or group appears
- refresh name, unread, or ordering hints

Suggested event payload:

- conversation created
- conversation updated
- conversation removed or hidden
- unread counters changed

3. active conversation message subscription

Purpose:

- append incoming and outgoing messages in the current conversation pane
- reflect reply and reaction count changes

Suggested event payload:

- message appended
- message updated
- message lifecycle changed

4. active conversation metadata subscription

Purpose:

- keep title, members, and lightweight context correct when user stays inside a group

Suggested event payload:

- conversation renamed
- member count changed
- membership state changed

5. error and operational event subscription

Purpose:

- surface transient failures in the status area without forcing a log view

Suggested event payload:

- send failure
- sync failure
- reconnecting
- resync required

#### Subscription API Shape

The TUI should not subscribe to several unrelated raw streams and try to merge them itself.

Instead, the daemon should expose one frontend-oriented subscription API with typed events.

Suggested additions to `xmtp-ipc`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonRequest {
    // existing requests...
    OpenSubscription { topics: Vec<SubscriptionTopic> },
    CloseSubscription { subscription_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionTopic {
    AppStatus,
    ConversationList,
    ActiveConversation,
    ActiveConversationMetadata,
    Errors,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionOpened {
    pub subscription_id: String,
}
```

Suggested event envelope:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonEventEnvelope {
    pub version: ProtocolVersion,
    pub subscription_id: String,
    pub sequence: u64,
    pub emitted_at_unix_ms: i64,
    pub payload: DaemonEvent,
}
```

The sequence field matters.

It gives the frontend a clear way to detect:

- missed events
- out-of-order events
- reconnection gaps

#### Snapshot Plus Stream Model

The TUI should not rely on events alone.

For each screen surface, the flow should be:

1. fetch snapshot
2. render snapshot
3. open subscription
4. apply incremental events
5. if sequence gap or subscription reset occurs, refetch snapshot

This model is simpler and safer than trying to reconstruct full screen state from pure event replay.

Use this pattern for:

- app status
- conversation list
- active conversation messages

#### Event Types Needed For The First TUI

The TUI does not need raw protocol events. It needs stable UI-oriented events.

Suggested first event set:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonEvent {
    StatusUpdated(StatusUpdatedEvent),
    ConversationListPatched(ConversationListPatchedEvent),
    ConversationMessagesPatched(ConversationMessagesPatchedEvent),
    ConversationMetadataPatched(ConversationMetadataPatchedEvent),
    ErrorRaised(ErrorRaisedEvent),
}
```

Important rule:

- event payloads should already be normalized for frontend use
- the TUI should not need to understand XMTP-native event internals

#### Recommended TUI Component Breakdown

The Ratatui implementation should be split into a small number of production-friendly components.

Suggested modules:

- `app`
  - top-level app state
  - startup and shutdown
  - mode and focus ownership

- `event_loop`
  - merges terminal events, daemon events, and async task completions
  - owns the main dispatch loop

- `ipc_client`
  - request and response client
  - subscription client
  - reconnect and resubscribe behavior

- `screens/chat`
  - main chat layout coordinator

- `widgets/conversation_list`
  - left pane rendering and selection behavior

- `widgets/message_list`
  - message pane rendering
  - selection and scroll state

- `widgets/input_box`
  - message compose area
  - reply banner rendering

- `widgets/status_bar`
  - bottom bar rendering

- `widgets/popup_menu`
  - message action popup

- `dialogs/create_dm`
  - modal for direct-message creation

- `dialogs/create_group`
  - modal for group creation

- `dialogs/reaction_picker`
  - fixed reaction selection modal

- `theme`
  - colors and text styles

- `format`
  - ID shortening
  - timestamp formatting
  - unsupported content formatting

This is enough structure to stay clean without over-splitting the workspace.

#### Recommended Ratatui Crate Structure

The current workspace does not need many new crates for the TUI.

A pragmatic structure is:

- keep daemon and CLI crates as they are
- add one new crate: `crates/xmtp-tui`
- reuse `xmtp-ipc`, `xmtp-core`, `xmtp-config`, and formatting helpers where sensible

Recommended internal layout for `crates/xmtp-tui`:

```text
crates/xmtp-tui/src/
  main.rs
  app.rs
  action.rs
  event.rs
  runtime.rs
  ipc/
    client.rs
    subscription.rs
  screens/
    chat.rs
  widgets/
    conversation_list.rs
    message_list.rs
    input_box.rs
    status_bar.rs
    popup_menu.rs
  dialogs/
    create_dm.rs
    create_group.rs
    reaction_picker.rs
  theme.rs
  format.rs
```

This matches the current project size and avoids unnecessary crate explosion.

#### Event Loop Architecture

The TUI event loop must be designed carefully.

This is the part most likely to become brittle if it is not explicit.

The first production-grade version should use a single app reducer model:

- all external inputs become internal `AppEvent`
- all state changes happen through one update path
- rendering happens after state updates, not from background tasks directly

Suggested event sources:

- terminal input events
- periodic tick events
- daemon subscription events
- async command completion events

Suggested internal shape:

```rust
pub enum AppEvent {
    Terminal(TerminalEvent),
    Tick,
    Daemon(DaemonEventEnvelope),
    CommandResult(CommandResultEvent),
}
```

Suggested processing model:

1. read next `AppEvent`
2. reduce app state
3. emit zero or more async effects
4. render next frame

Important rule:

- background tasks must not mutate UI state directly
- they should send `AppEvent::CommandResult` or `AppEvent::Daemon(...)` back into the main loop

#### Event Loop Effects

The reducer should return effect descriptions rather than performing all side effects inline.

Examples:

- load conversation list
- load conversation history
- send message
- open DM creation dialog
- submit group creation
- subscribe to active conversation
- resubscribe after reconnect

This gives cleaner control over:

- retries
- cancellation
- in-flight request deduplication
- testability

Suggested effect shape:

```rust
pub enum Effect {
    LoadStatus,
    LoadConversationList,
    LoadConversationHistory { conversation_id: String },
    SendMessage { conversation_id: String, text: String },
    Reply { message_id: String, text: String },
    React { message_id: String, emoji: String },
    CreateDm { target: String },
    CreateGroup { name: Option<String>, members: Vec<String> },
    OpenSubscriptions,
    SwitchActiveConversation { conversation_id: String },
}
```

#### Focus And Event Routing Rules

The reducer needs explicit routing rules so key handling stays predictable.

Recommended routing order:

1. if a modal dialog is open, it receives the key first
2. else if a popup menu is open, it receives the key first
3. else route by focused pane
4. then process allowed global shortcuts

This prevents accidental conflicts between:

- typing text
- message actions
- creation dialogs
- navigation

#### Required Daemon Work Before TUI Implementation

Before serious TUI work starts, the daemon should gain these capabilities:

1. a real subscription channel with typed event envelopes
2. subscription open and close lifecycle
3. sequence-numbered event delivery
4. snapshot fetches that correspond cleanly to stream topics
5. normalized conversation patch events
6. normalized message patch events
7. safe reconnect and resubscribe behavior

Without these, the TUI would end up reimplementing daemon logic in the frontend, which is the wrong architecture.

#### Mapping To Current Codebase

Given the current repository state, the following practical mapping should be used.

Current strengths:

- `xmtp-daemon` already owns the XMTP runtime
- `xmtp-ipc` already has stable request and response types
- direct history loading and narrow history watch already exist
- the daemon already has one async local socket loop

Current limitations:

- `WatchHistory` is too narrow to serve as the main TUI subscription model
- there is no generic subscription open or close handshake
- there is no event envelope with sequence numbers
- there is no frontend-oriented conversation patch stream

This means the next daemon step should not be:

- adding more one-off watch commands

It should be:

- introducing one reusable subscription transport shape
- then migrating history watch onto that shape
- then adding status and conversation list topics

Recommended implementation order inside the current codebase:

1. extend `xmtp-ipc` with subscription request and event envelope types
2. add subscription session management to `xmtp-daemon`
3. convert current history watch into `ActiveConversation` events
4. add `AppStatus` events
5. add `ConversationList` patch events
6. only then start `xmtp-tui`

#### TUI Build Order Recommendation

Recommended order:

1. add IPC subscription primitives
2. add normalized daemon event types
3. build `xmtp-tui` shell with static layout
4. wire snapshot loading
5. wire live status and conversation updates
6. wire active conversation message updates
7. add input and send
8. add message action popup
9. add create DM and create group dialogs

This order keeps the architecture clean and avoids writing UI code against unstable daemon behavior.

#### TUI Non-Goals For First Ratatui Version

The first TUI version does not need:

- inline rich media rendering
- mouse-first interactions
- editable message history
- raw trace visualization inside the main screen
- deep admin and permissions UI

The first version should focus on:

- stable navigation
- fast sending
- clear status
- readable message history
- reply and reaction flows

## Implementation Guidance

- prefer exposing more state rather than less
- design commands around durable objects such as conversation IDs and message IDs
- keep the daemon as the single runtime owner of XMTP state
- make logs and traces part of the core architecture, not an afterthought
- avoid embedding Ratatui assumptions into the daemon API

## Current Decision Summary

The approved direction is:

- build a robust CLI first
- treat the product as a client-side CLI, not a server product
- keep the daemon as the XMTP runtime owner
- support DM, Group, status, history, detailed records, and logs early
- defer Ratatui until the command surface and runtime behavior are stable
