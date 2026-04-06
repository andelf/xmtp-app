/**
 * Core types for the content type registry.
 */
import type { ReplyRef, ReactionInfo, ReadReceiptInfo, DecodedMessageLike } from "../utils/messageDecoder";

export interface ActionItem {
  id: string;
  label: string;
  style?: string;
  imageUrl?: string;
}

export interface ActionsPayload {
  id: string;
  description: string;
  actions: ActionItem[];
}

export type DecodeResult =
  | { kind: "message"; text: string; replyRef?: ReplyRef; format?: "markdown" }
  | { kind: "actions"; text: string; payload: ActionsPayload }
  | { kind: "intent"; text: string; actionsId: string; actionId: string }
  | { kind: "reaction"; info: ReactionInfo }
  | { kind: "readReceipt"; info: ReadReceiptInfo }
  | { kind: "skip" };

export interface ContentTypeHandler {
  typeId: string;
  decode(msg: DecodedMessageLike, conversationId: string): DecodeResult;
  preview(msg: DecodedMessageLike): string | null;
}

// Re-export for convenience
export type { ReplyRef, ReactionInfo, ReadReceiptInfo, DecodedMessageLike };
