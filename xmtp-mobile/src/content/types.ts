/**
 * Core types for the content type registry.
 */
import type { ReplyRef, ReactionInfo, ReadReceiptInfo, DecodedMessageLike } from "../utils/messageDecoder";

export type DecodeResult =
  | { kind: "message"; text: string; replyRef?: ReplyRef; format?: "markdown" }
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
