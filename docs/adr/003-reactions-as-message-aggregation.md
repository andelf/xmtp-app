# ADR-003: Aggregate reactions onto parent messages instead of storing separately

## Status
Accepted

## Context
XMTP treats reactions as independent messages with their own IDs and timestamps. They reference a parent message via `reaction.reference`. The UI needs to display reactions as badges below the parent message bubble, not as separate messages in the list.

## Decision
Reactions are aggregated into a `reactions: Record<emoji, senderInboxId[]>` field on the parent `MessageItem`. Two code paths handle this:

1. **History fetch** — After loading messages, iterate reactions and attach them to their referenced messages before storing in state.
2. **Real-time stream** — `applyReaction()` store action finds the referenced message and mutates its reactions map (add/remove sender).

Reaction messages are never stored as standalone `MessageItem`s.

## Alternatives considered
1. **Separate reactions store** — `Record<messageId, Reaction[]>`. Would require cross-store lookups at render time and complex selectors. More normalized but harder to trigger re-renders correctly.
2. **Render reactions as list items** — Show each reaction as its own chat bubble. Clutters the conversation view.
3. **Use SDK's enrichedMessages()** — SDK v5 has `enrichedMessages()` that returns reactions attached to messages. However, this is a different API shape (`DecodedMessageV2`) and would require a separate parsing path.

## Consequences
- Simple render: `MessageBubble` just reads `item.reactions`
- A reaction arriving before its parent message is silently dropped (edge case: message not yet loaded)
- `applyReaction` must produce a new array reference to trigger zustand re-render
- The `added`/`removed` action toggling must be idempotent (check before add, filter on remove)
