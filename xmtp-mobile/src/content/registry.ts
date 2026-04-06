/**
 * Content type registry: register handlers, decode and preview messages.
 *
 * The registry first tries to match by contentTypeId, then falls back to
 * field detection on nativeContent for messages without a proper contentTypeId.
 */
import { getNativeContent } from "../utils/nativeContent";
import type { ContentTypeHandler, DecodeResult, DecodedMessageLike } from "./types";
import { decodeUnknown, previewUnknown } from "./unknown";

const handlers = new Map<string, ContentTypeHandler>();

/** Register a content type handler. */
export function registerContentType(handler: ContentTypeHandler): void {
  handlers.set(handler.typeId, handler);
}

/**
 * Decode a message using the registry.
 * 1. Try typeId-based lookup.
 * 2. Fall back to field detection on nativeContent.
 * 3. Fall back to unknown handler.
 */
export function decodeMessage(msg: DecodedMessageLike, conversationId: string): DecodeResult {
  try {
    // 1. Try registered handler by contentTypeId
    if (msg.contentTypeId) {
      const handler = handlers.get(msg.contentTypeId);
      if (handler) {
        const result = handler.decode(msg, conversationId);
        // If the handler matched but returned skip, still try field detection.
        // This handles cases where contentTypeId doesn't match actual nativeContent
        // (e.g. a reaction message with contentTypeId "xmtp.org/text:1.0").
        if (result.kind !== "skip") return result;
      }
    }

    // 2. Field detection fallback — scan nativeContent fields to find the right handler
    const nc = getNativeContent(msg as any);
    if (nc) {
      if (nc.text != null) {
        const h = handlers.get("xmtp.org/text:1.0");
        if (h) return h.decode(msg, conversationId);
      }
      if (nc.reply) {
        const h = handlers.get("xmtp.org/reply:1.0");
        if (h) return h.decode(msg, conversationId);
      }
      if (nc.reaction || nc.reactionV2) {
        const h = handlers.get("xmtp.org/reaction:1.0");
        if (h) return h.decode(msg, conversationId);
      }
      if (nc.readReceipt !== undefined) {
        const h = handlers.get("xmtp.org/readReceipt:1.0");
        if (h) return h.decode(msg, conversationId);
      }
      if (nc.groupUpdated) {
        const h = handlers.get("xmtp.org/group_updated:1.0");
        if (h) return h.decode(msg, conversationId);
      }
      // Protocol-level signals — never display
      if (nc.leaveRequest !== undefined) return { kind: "skip" };

      // Unknown content types — check if we have a handler by the inner contentTypeId
      if (nc.unknown) {
        const unk = nc.unknown as { contentTypeId?: string };
        if (unk.contentTypeId) {
          const h = handlers.get(unk.contentTypeId);
          if (h) return h.decode(msg, conversationId);
        }
      }
      // Also check encoded payload's type field
      if (nc.encoded) {
        try {
          const encoded = JSON.parse(nc.encoded);
          const t = encoded.type;
          if (t?.authorityId && t?.typeId) {
            const typeStr = `${t.authorityId}/${t.typeId}:${t.versionMajor ?? 1}.${t.versionMinor ?? 0}`;
            const h = handlers.get(typeStr);
            if (h) return h.decode(msg, conversationId);
          }
        } catch {
          // fall through
        }
      }
    }

    // 3. Unknown fallback
    return decodeUnknown(msg);
  } catch {
    return { kind: "skip" };
  }
}

/**
 * Get a preview string for a message.
 * Returns null for non-displayable content (reactions, read receipts, group updates).
 */
export function previewMessage(msg: DecodedMessageLike): string | null {
  try {
    // 1. Try registered handler by contentTypeId
    if (msg.contentTypeId) {
      const handler = handlers.get(msg.contentTypeId);
      if (handler) {
        const result = handler.preview(msg);
        if (result !== null) return result;
        // Fall through to field detection if handler returned null
      }
    }

    // 2. Field detection fallback
    const nc = getNativeContent(msg as any);
    if (nc) {
      if (nc.text != null) {
        const h = handlers.get("xmtp.org/text:1.0");
        if (h) return h.preview(msg);
      }
      if (nc.reply) {
        const h = handlers.get("xmtp.org/reply:1.0");
        if (h) return h.preview(msg);
      }
      if (nc.reaction || nc.reactionV2) return null;
      if (nc.readReceipt !== undefined) return null;
      if (nc.groupUpdated) return null;
      if (nc.leaveRequest !== undefined) return null;
    }

    // 3. Unknown fallback
    return previewUnknown(msg);
  } catch {
    return null;
  }
}
