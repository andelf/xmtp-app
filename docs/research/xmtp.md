# XMTP Research Notes

Date: 2026-04-06
Last verified against repo: 2026-04-12
Applies to:
- workspace dependency `xmtp = 0.8.1`
- source scope limited to docs.rs public API pages

## Scope

- Source only: https://docs.rs/xmtp/latest/xmtp/
- Crate version: `xmtp 0.8.1`
- Evidence used: crate root, module pages, and selected struct/enum pages from docs.rs
- This document is a public-API snapshot, not a claim about every internal libxmtp capability

## Current status in this repo

This document is still aligned with the current workspace dependency:

- `Cargo.toml` pins `xmtp = 0.8.1`
- the notes below should be treated as SDK-shape guidance for Rust-side design work
- if the workspace upgrades `xmtp`, this document should be re-verified before being used as an authority

## Team-Lead Summary

`xmtp` is a layered Rust SDK for XMTP. The crate sits on top of `xmtp-sys` FFI, and the public API is intentionally simple: `Client -> Conversation -> Message`. The main design trade-off is that the SDK hides a lot of protocol and storage detail, but it also inherits a fairly stateful client model with sync, local DB, identity, permissions, and streaming lifecycle concerns.

## Architecture

| Layer | Main items | What it does |
| --- | --- | --- |
| Entry point | `Client`, `ClientBuilder` | Builds and owns the primary SDK handle |
| Conversation layer | `Conversation`, `Message`, `GroupMember` | Creates, reads, and mutates DMs/groups |
| Content layer | `content::{Content, encode_*, decode}` | Turns app payloads into XMTP wire content and back |
| Recipient resolution | `resolve::Recipient`, `Resolver` | Normalizes address / inbox / ENS inputs |
| Streaming | `stream::{Subscription, MessageEvent, ConsentUpdate, PreferenceUpdate}` | Real-time event delivery |
| Shared types | `types::*` | Options, enums, identity, permissions, sync, stats |

- The crate root explicitly describes itself as a safe Rust SDK that wraps `xmtp-sys` FFI with idiomatic Rust types.
- The root docs also define the high-level model as `Client -> Conversation -> Message`.
- Feature flags split optional capabilities such as `alloy`, `ledger`, and `ens` from the core crate. `content` is enabled by default.

## Usage

- Create a client with `Client::builder()`, set `Env`, DB path, resolver, and other options, then call `build(&signer)`.
- Use `Recipient::parse(...)` or pass `Recipient::Address`, `Recipient::InboxId`, or `Recipient::Ens` when creating or looking up DMs and groups.
- Send content either as raw encoded bytes with `Conversation::send` / `send_with`, or via helpers like `content::encode_text`, `encode_markdown`, `encode_reply`, `encode_reaction`, and `encode_attachment`.
- Read conversations with `messages()`, `list_messages(...)`, `members()`, `metadata()`, and `permissions()`.
- Use `stream::*` if you need live updates instead of polling.

## Key Data Structures

- `ClientBuilder`: environment, database path, encryption key, API URL, gateway host, app version, resolver, offline mode, and notification mode.
- `Client`: inbox identity, installation identity, sync, DM/group management, account management, signature helpers, and HMAC key lookup.
- `Conversation`: hex conversation ID, type, membership state, DM peer inbox ID, metadata, permissions, consent, disappearing settings, and send/list/membership methods.
- `Message`: enriched message record with `id`, `conversation_id`, sender IDs, timestamps, kind, delivery status, `content_type`, fallback text, raw `content`, expiration, and reaction/reply counts.
- `Recipient`: `Address`, `InboxId`, `Ens`; `parse()` auto-detects by string shape.
- `Content`: `Text`, `Markdown`, `Reaction`, `Reply`, `ReadReceipt`, `Attachment`, `RemoteAttachment`, `Unknown`.
- `Subscription<T>`: stream wrapper with `recv`, `try_recv`, `close`, `is_closed`.
- `MessageEvent`: only `message_id` and `conversation_id`.
- `ConsentUpdate` and `PreferenceUpdate`: stream event payloads for consent and preference changes.
- `Types` worth calling out: `AccountIdentifier`, `ConversationMetadata`, `CreateDmOptions`, `CreateGroupOptions`, `ListConversationsOptions`, `ListMessagesOptions`, `Permissions`, `PermissionPolicySet`, `SyncResult`, `InboxState`, `SendOptions`, `HmacKeyEntry`, `Cursor`, `ApiStats`, `IdentityStats`.

## Performance Characteristics

- The docs do not publish benchmarks or latency numbers.
- `send_optimistic` and `group_optimistic` are explicit latency trade-offs: they return before network publish/sync completes.
- `ClientBuilder::db_path(...)` selects the local DB path; the docs also describe an ephemeral in-memory mode. `release_db` and `reconnect_db` show that DB lifecycle is a first-class cost.
- `stream` is channel-based, so consumers can wait on events instead of polling.
- `Message::decode()` defers typed content decoding until the caller asks for it.
- `content::Content::Unknown` preserves raw bytes and content type instead of discarding unsupported payloads.
- `Client::can_message*` and `list_*` APIs return batch results (`Vec<bool>`, `Vec<Conversation>`, `Vec<Message>`), which is convenient but means callers can pay for full materialization when they ask for full lists.

## Trade-Offs

- Convenience vs explicit control: the SDK gives typed helpers, but the caller still has to manage DB, resolver, signer, and sync behavior.
- Rich feature set vs small surface area: identity changes, permissions, disappearing messages, consent, and streaming are all built in, which is powerful but broad.
- Responsive UX vs consistency: optimistic APIs improve perceived speed, but the caller must accept background publish/sync work.
- Forward compatibility vs strict decoding: unsupported content stays as `Unknown` with raw bytes, which avoids data loss but pushes handling to the app.
- Threading model favors safe ownership over shared mutation: `Client` and `Conversation` are `Send` but not `Sync`, so they can move across threads but should not be shared concurrently by reference.

## Notes On Inference

- The crate does not state performance numbers, so any performance comments above are based on API shape, not benchmarks.
- `MessageEvent` carrying only IDs strongly suggests consumers must fetch or reconstruct full message data separately if they need content. This is an inference from the public fields, not an explicit statement in the docs.
- `Client` and `Conversation` being `Send` but not `Sync` implies the SDK expects ownership transfer rather than shared concurrent access.

## Known limits of this note

- This is intentionally limited to docs.rs, so it does not replace source-level verification when a feature matters operationally.
- It should not be used to infer mobile SDK behavior; React Native support and JS codec support need separate verification.
- It does not attempt to summarize active XIPs; see `xmtp-active-proposals.md` for project-facing ecosystem status.

## Sources

- https://docs.rs/xmtp/latest/xmtp/
- https://docs.rs/xmtp/latest/xmtp/client/index.html
- https://docs.rs/xmtp/latest/xmtp/client/struct.Client.html
- https://docs.rs/xmtp/latest/xmtp/client/struct.ClientBuilder.html
- https://docs.rs/xmtp/latest/xmtp/conversation/index.html
- https://docs.rs/xmtp/latest/xmtp/conversation/struct.Conversation.html
- https://docs.rs/xmtp/latest/xmtp/conversation/struct.Message.html
- https://docs.rs/xmtp/latest/xmtp/content/index.html
- https://docs.rs/xmtp/latest/xmtp/content/enum.Content.html
- https://docs.rs/xmtp/latest/xmtp/resolve/index.html
- https://docs.rs/xmtp/latest/xmtp/resolve/enum.Recipient.html
- https://docs.rs/xmtp/latest/xmtp/stream/index.html
- https://docs.rs/xmtp/latest/xmtp/stream/struct.Subscription.html
- https://docs.rs/xmtp/latest/xmtp/stream/struct.MessageEvent.html
- https://docs.rs/xmtp/latest/xmtp/stream/struct.ConsentUpdate.html
- https://docs.rs/xmtp/latest/xmtp/stream/struct.PreferenceUpdate.html
- https://docs.rs/xmtp/latest/xmtp/types/index.html
