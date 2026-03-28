# ADR-001: Bypass SDK content() in favor of raw nativeContent

## Status
Accepted

## Context
XMTP React Native SDK v5's `DecodedMessage.content()` requires content type codecs to be registered in `Client.codecRegistry`. Without explicit registration at client creation time, calling `content()` throws for reply, reaction, groupUpdated, markdown, and any custom content types. The SDK ships codec classes (ReplyCodec, ReactionCodec, etc.) but does not auto-register them.

## Decision
Access `msg.nativeContent` directly instead of calling `msg.content()`. Parse the raw `NativeMessageContent` structure which contains typed fields: `text`, `reply`, `reaction`, `reactionV2`, `readReceipt`, `groupUpdated`, `unknown`, `encoded`.

## Alternatives considered
1. **Register all codecs at Client.create()** — Would require importing and maintaining a codec list. The decoded output structure differs from nativeContent (codec unwraps the wrapper key), requiring different parsing logic. Also, new content types would require code changes to register.
2. **Try content() with fallback to nativeContent** — Adds complexity and inconsistent code paths.

## Consequences
- All message parsing uses one consistent code path
- We depend on an undocumented property (`nativeContent`) that could change in SDK updates
- Adding support for new content types only requires adding a branch for the new nativeContent key
- For `encoded` payloads (e.g. markdown), we must manually parse the JSON and base64-decode content bytes
