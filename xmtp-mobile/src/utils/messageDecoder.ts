/**
 * Pure conversion functions for XMTP DecodedMessage → app types.
 *
 * These functions have zero native/SDK side effects and can be tested
 * without mocking any React Native or XMTP native modules.
 */
import { getNativeContent } from "./nativeContent";

// ---------------------------------------------------------------------------
// Types — use plain strings so they are assignable from SDK branded types.
// ---------------------------------------------------------------------------

export interface ReplyRef {
  /** ID of the original message being replied to */
  referenceMessageId: string;
  /** Preview text of the original message (if available) */
  referenceText?: string;
}

/** Aggregated reactions on a message: emoji → set of sender inboxIds */
export type Reactions = Record<string, string[]>;

export interface MessageItem {
  id: string;
  conversationId: string;
  senderInboxId: string;
  /** Decoded text content */
  text: string;
  /** Original content type URI (e.g. "xmtp.org/text:1.0") */
  contentType: string;
  /** Sent timestamp (epoch ms) */
  sentAt: number;
  /** Delivery status */
  status: string;
  /** True if sent by the current user */
  isOwn: boolean;
  /** Reply reference (if this message is a reply) */
  replyRef?: ReplyRef;
  /** Aggregated reactions keyed by emoji */
  reactions?: Reactions;
}

export interface ReactionInfo {
  conversationId: string;
  referenceMessageId: string;
  emoji: string;
  action: "added" | "removed";
  senderInboxId: string;
}

// ---------------------------------------------------------------------------
// Minimal shape we need from DecodedMessage (avoids importing native SDK)
// ---------------------------------------------------------------------------

export interface DecodedMessageLike {
  id: string;
  senderInboxId: string;
  sentNs?: number;
  deliveryStatus?: string;
  contentTypeId?: string;
  nativeContent?: Record<string, any>;
  fallback?: string;
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

/**
 * Convert an XMTP DecodedMessage to our MessageItem.
 * Supports: text, reply. Returns null for non-displayable types (reaction, read receipt, etc.)
 * Returns "Unsupported content type: xxx" for unknown types without extractable text.
 */
export function decodedToMessageItem(
  msg: DecodedMessageLike,
  conversationId: string,
  myInboxId: string | null
): MessageItem | null {
  let text: string = "";
  let replyRef: ReplyRef | undefined;

  const nc = getNativeContent(msg as any);
  if (!nc) return null;

  try {
    if (nc.text != null) {
      // Plain text message
      text = typeof nc.text === "string" ? nc.text : String(nc.text);
    } else if (nc.reply) {
      // Reply: { reply: { reference, content: { text }, contentType } }
      const reply = nc.reply;
      text = reply.content?.text ?? "[reply]";
      replyRef = {
        referenceMessageId: reply.reference ?? "",
        referenceText: undefined, // resolved at render time from store
      };
    } else if (nc.reaction || nc.reactionV2) {
      return null;
    } else if (nc.readReceipt !== undefined) {
      return null;
    } else if (nc.groupUpdated) {
      return null;
    } else if (nc.unknown) {
      // Unknown content type (e.g. markdown) — try to extract text from
      // the encoded payload or fallback text.
      const unk = nc.unknown as { contentTypeId?: string; content?: string };
      if (unk.content) {
        text = unk.content;
      } else if ((msg as any).fallback) {
        text = (msg as any).fallback;
      } else {
        const typeId = unk.contentTypeId ?? msg.contentTypeId ?? "unknown";
        text = `Unsupported content type: ${typeId}`;
      }
    } else {
      // Try to decode from encoded payload (e.g. markdown, custom content types)
      if (nc.encoded) {
        try {
          const encoded = JSON.parse(nc.encoded);
          if (encoded.content) {
            text = globalThis.Buffer.from(encoded.content, "base64").toString("utf-8");
          } else if (encoded.fallback) {
            text = encoded.fallback;
          }
        } catch {
          // encoded parse failed
        }
      }
      if (!text) {
        const typeId = msg.contentTypeId ?? "unknown";
        text = `Unsupported content type: ${typeId}`;
      }
    }
  } catch {
    return null;
  }

  if (!text) return null;

  return {
    id: msg.id,
    conversationId,
    senderInboxId: msg.senderInboxId,
    text,
    contentType: msg.contentTypeId ?? "xmtp.org/text:1.0",
    sentAt: msg.sentNs ? msg.sentNs / 1_000_000 : Date.now(),
    status: msg.deliveryStatus ?? "published",
    isOwn: msg.senderInboxId === myInboxId,
    replyRef,
  };
}

/**
 * Extract a reaction from a DecodedMessage, if it is one.
 * Returns null for non-reaction messages.
 */
export function decodedToReaction(
  msg: DecodedMessageLike,
  conversationId: string
): ReactionInfo | null {
  const nc = getNativeContent(msg as any);
  if (!nc) return null;
  const r = nc.reaction ?? nc.reactionV2;
  if (!r) return null;
  if (r.action !== "added" && r.action !== "removed") return null;
  return {
    conversationId,
    referenceMessageId: r.reference ?? "",
    emoji: r.content ?? "",
    action: r.action,
    senderInboxId: msg.senderInboxId,
  };
}
