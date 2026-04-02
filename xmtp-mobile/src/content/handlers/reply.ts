import { getNativeContent } from "../../utils/nativeContent";
import type { ContentTypeHandler, DecodeResult, DecodedMessageLike } from "../types";

export const replyHandler: ContentTypeHandler = {
  typeId: "xmtp.org/reply:1.0",

  decode(msg: DecodedMessageLike): DecodeResult {
    const nc = getNativeContent(msg as any);
    if (!nc?.reply) return { kind: "skip" };
    const reply = nc.reply;
    const text: string = reply.content?.text ?? "[reply]";
    return {
      kind: "message",
      text,
      replyRef: {
        referenceMessageId: reply.reference ?? "",
        referenceText: undefined, // resolved at render time from store
      },
    };
  },

  preview(msg: DecodedMessageLike): string | null {
    const nc = getNativeContent(msg as any);
    if (!nc?.reply) return null;
    return nc.reply.content?.text ?? "[reply]";
  },
};
