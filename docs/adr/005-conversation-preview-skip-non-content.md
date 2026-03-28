# ADR-005: Skip non-content messages when resolving conversation preview text

## Status
Accepted

## Context
The conversation list shows a preview of the last message. XMTP conversations contain interleaved content messages (text, reply, markdown) and non-content messages (reactions, read receipts, group updates). If the most recent message is a reaction, the preview would show "No messages yet" or meaningless text.

## Decision
Fetch up to 5 recent messages and iterate to find the first content message (text, reply, or encoded/markdown). Skip reactions, read receipts, and group updates. If only reactions exist in the window, fall back to showing `[react] {emoji}`.

For the real-time stream (`streamAllMessages`), reactions are also skipped for preview updates — a reaction should not overwrite an existing text preview.

## Alternatives considered
1. **Always fetch limit=1** — Simple but broken when last message is a reaction.
2. **Fetch limit=20 to guarantee finding content** — Wasteful for the common case.
3. **Maintain a separate "last content message" field** — Would require schema changes and migration.

## Consequences
- Preview is always a meaningful content message (text, reply, markdown)
- The 5-message window is a pragmatic trade-off: handles common cases (a few reactions after a text) without over-fetching
- Edge case: if >5 consecutive reactions/receipts exist with no content, preview falls back to reaction emoji or "No messages yet"
