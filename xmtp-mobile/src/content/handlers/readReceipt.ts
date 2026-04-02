import { getNativeContent } from "../../utils/nativeContent";
import type { ContentTypeHandler, DecodeResult, DecodedMessageLike } from "../types";

export const readReceiptHandler: ContentTypeHandler = {
  typeId: "xmtp.org/readReceipt:1.0",

  decode(msg: DecodedMessageLike, conversationId: string): DecodeResult {
    const nc = getNativeContent(msg as any);
    if (!nc || nc.readReceipt === undefined) return { kind: "skip" };
    return {
      kind: "readReceipt",
      info: {
        conversationId,
        senderInboxId: msg.senderInboxId,
      },
    };
  },

  preview(): string | null {
    return null;
  },
};
