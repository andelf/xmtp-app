# ADR-002: Send reactions and replies via raw NativeMessageContent

## Status
Accepted

## Context
Sending non-text messages (reactions, replies) through the SDK normally requires registering codecs and using `convo.send(content, { contentType })`. However, the SDK's `send()` method has a fast path: when no `contentType` option is provided and content is not a string, it passes the object directly to `XMTP.sendMessage()` on the native bridge as `NativeMessageContent`.

## Decision
Send reactions and replies by constructing the `NativeMessageContent` object directly:

```typescript
// Reaction
convo.send({ reaction: { reference, action, schema: "unicode", content: emoji } })

// Reply
convo.send({ reply: { reference, content: { text }, contentType: "xmtp.org/text:1.0" } })
```

## Alternatives considered
1. **Register codecs and use contentType option** — Requires managing codec registry, and the `_sendWithJSCodec` path serializes through a different code path that may behave differently.
2. **Use SDK helper methods if they exist** — No dedicated `sendReaction()` or `sendReply()` methods exist in SDK v5.

## Consequences
- No codec registration needed — simpler setup
- Symmetric with our read path (both use nativeContent)
- Depends on the native bridge accepting raw NativeMessageContent shapes
- If the SDK changes how `send()` routes non-string content, this could break
