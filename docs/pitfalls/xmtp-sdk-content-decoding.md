# XMTP React Native SDK: content() throws for unregistered codecs

## Problem

Calling `msg.content()` on a `DecodedMessage` throws `Error: no content type found` for reply, reaction, groupUpdated, and any custom content type (e.g. markdown).

## Root cause

`DecodedMessage.content()` looks up the content type in `Client.codecRegistry`. The registry is populated by codecs passed to `Client.create({ codecs: [...] })`. Without explicit registration, only text and a few built-in types work. Reply, reaction, and others are NOT auto-registered despite having built-in codec classes in the SDK.

## Failed approach

Wrapping `msg.content()` in try/catch and returning null — silently drops all non-text messages.

## Fix

Bypass `msg.content()` entirely. Access `(msg as any).nativeContent` directly, which is the raw `NativeMessageContent` object from the native bridge:

```typescript
const nc = (msg as any).nativeContent;
// nc.text, nc.reply, nc.reaction, nc.reactionV2, nc.groupUpdated, nc.encoded
```

For encoded payloads (e.g. markdown with `contentTypeId = "xmtp.org/markdown:1.0"`), the content is in `nc.encoded` as a JSON string with base64-encoded content bytes.

## Additional pitfall: base64 decoding

`atob()` in Hermes decodes base64 to latin1, not UTF-8. Chinese/emoji characters corrupt. Use `globalThis.Buffer.from(str, "base64").toString("utf-8")` instead (Buffer polyfill is already present for XMTP SDK signature handling).

## Lesson

- Don't assume SDK convenience methods work for all content types
- Check the actual native bridge data structure when SDK methods fail
- Always test with non-ASCII content when doing encoding/decoding
