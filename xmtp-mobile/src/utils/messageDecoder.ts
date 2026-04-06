/**
 * Pure conversion functions for XMTP DecodedMessage -> app types.
 *
 * These functions have zero native/SDK side effects and can be tested
 * without mocking any React Native or XMTP native modules.
 *
 * Decoding logic is delegated to the content type registry (src/content/).
 */
import { decodeMessage } from "../content";
import type { ActionsPayload } from "../content/types";

// ---------------------------------------------------------------------------
// Types -- use plain strings so they are assignable from SDK branded types.
// ---------------------------------------------------------------------------

export interface ReplyRef {
  /** ID of the original message being replied to */
  referenceMessageId: string;
  /** Preview text of the original message (if available) */
  referenceText?: string;
}

/** Aggregated reactions on a message: emoji -> list of sender inboxIds (duplicates allowed for multi-react) */
export type Reactions = Record<string, string[]>;

export interface MessageItem {
  id: string;
  conversationId: string;
  senderInboxId: string;
  /** Decoded text content */
  text: string;
  /** Original content type URI (e.g. "xmtp.org/text:1.0") */
  contentType: string;
  /** Rendering format hint from content handler */
  format?: "markdown";
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
  /** Structured Actions payload (for actions content type) */
  actionsPayload?: ActionsPayload;
  /** Intent reference (for intent content type — which actions set and which action was selected) */
  intentRef?: { actionsId: string; actionId: string };
}

export interface ReactionInfo {
  /** ID of the reaction message itself (for dedup across history + stream) */
  id?: string;
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
function baseMessageFields(
  msg: DecodedMessageLike,
  conversationId: string,
  myInboxId: string | null
): Omit<MessageItem, "text" | "contentType"> {
  return {
    id: msg.id,
    conversationId,
    senderInboxId: msg.senderInboxId,
    sentAt: msg.sentNs ? msg.sentNs / 1_000_000 : Date.now(),
    status: msg.deliveryStatus ?? "published",
    isOwn: msg.senderInboxId === myInboxId,
  };
}

export function decodedToMessageItem(
  msg: DecodedMessageLike,
  conversationId: string,
  myInboxId: string | null
): MessageItem | null {
  const result = decodeMessage(msg, conversationId);
  if (result.kind === "actions") {
    return {
      ...baseMessageFields(msg, conversationId, myInboxId),
      text: result.text,
      contentType: msg.contentTypeId ?? "coinbase.com/actions:1.0",
      actionsPayload: result.payload,
    };
  }
  if (result.kind === "intent") {
    return {
      ...baseMessageFields(msg, conversationId, myInboxId),
      text: result.text,
      contentType: msg.contentTypeId ?? "coinbase.com/intent:1.0",
      intentRef: { actionsId: result.actionsId, actionId: result.actionId },
    };
  }
  if (result.kind !== "message") return null;
  if (!result.text) return null;

  return {
    ...baseMessageFields(msg, conversationId, myInboxId),
    text: result.text,
    contentType: msg.contentTypeId ?? "xmtp.org/text:1.0",
    format: result.format,
    replyRef: result.replyRef,
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
  const result = decodeMessage(msg, conversationId);
  if (result.kind !== "reaction") return null;
  return { ...result.info, id: msg.id };
}

export interface ReadReceiptInfo {
  conversationId: string;
  senderInboxId: string;
}

/**
 * Extract a read receipt from a DecodedMessage, if it is one.
 * Returns null for non-read-receipt messages.
 */
export function decodedToReadReceipt(
  msg: DecodedMessageLike,
  conversationId: string
): ReadReceiptInfo | null {
  const result = decodeMessage(msg, conversationId);
  if (result.kind !== "readReceipt") return null;
  return result.info;
}
