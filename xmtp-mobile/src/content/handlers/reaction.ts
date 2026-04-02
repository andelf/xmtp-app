import { getNativeContent } from "../../utils/nativeContent";
import type { ContentTypeHandler, DecodeResult, DecodedMessageLike } from "../types";

export const reactionHandler: ContentTypeHandler = {
  typeId: "xmtp.org/reaction:1.0",

  decode(msg: DecodedMessageLike, conversationId: string): DecodeResult {
    const nc = getNativeContent(msg as any);
    if (!nc) return { kind: "skip" };
    const r = nc.reaction ?? nc.reactionV2;
    if (!r) return { kind: "skip" };
    if (r.action !== "added" && r.action !== "removed") return { kind: "skip" };
    return {
      kind: "reaction",
      info: {
        conversationId,
        referenceMessageId: r.reference ?? "",
        emoji: r.content ?? "",
        action: r.action,
        senderInboxId: msg.senderInboxId,
      },
    };
  },

  preview(): string | null {
    return null;
  },
};
