/**
 * Shared helpers for reading XMTP DecodedMessage nativeContent.
 *
 * msg.content() throws for unregistered codecs, so we bypass it and
 * read the raw nativeContent directly from the native bridge.
 */
import type { DecodedMessage } from "@xmtp/react-native-sdk";

/** Access the undocumented nativeContent field, centralising the cast. */
export function getNativeContent(msg: DecodedMessage): Record<string, any> | undefined {
  return (msg as any).nativeContent as Record<string, any> | undefined;
}

/**
 * Extract plain text from a DecodedMessage for preview purposes.
 * Returns null for non-content messages (reaction, readReceipt, groupUpdated).
 */
export function extractNativeText(msg: DecodedMessage): string | null {
  const nc = getNativeContent(msg);
  if (!nc) return null;

  try {
    if (nc.text != null) {
      return typeof nc.text === "string" ? nc.text : String(nc.text);
    }
    if (nc.reply) {
      return nc.reply.content?.text ?? "[reply]";
    }
    if (nc.reaction || nc.reactionV2 || nc.readReceipt !== undefined || nc.groupUpdated) {
      return null;
    }
    if (nc.unknown) {
      const unk = nc.unknown as { contentTypeId?: string; content?: string };
      return unk.content ?? (msg as any).fallback ?? `Unsupported content type: ${unk.contentTypeId ?? msg.contentTypeId ?? "unknown"}`;
    }
    if (nc.encoded) {
      try {
        const encoded = JSON.parse(nc.encoded);
        if (encoded.content) {
          return globalThis.Buffer.from(encoded.content, "base64").toString("utf-8");
        }
        if (encoded.fallback) {
          return encoded.fallback;
        }
      } catch {}
      return `Unsupported content type: ${msg.contentTypeId ?? "unknown"}`;
    }
  } catch {}

  return `Unsupported content type: ${msg.contentTypeId ?? "unknown"}`;
}

/** Check if a nativeContent is a reaction (not a content message). */
export function isReactionContent(nc: Record<string, any>): boolean {
  return !!(nc.reaction || nc.reactionV2);
}

/** Extract reaction emoji from nativeContent, or undefined. */
export function extractReactionEmoji(nc: Record<string, any>): string | undefined {
  const r = nc.reaction ?? nc.reactionV2;
  return r?.content;
}
