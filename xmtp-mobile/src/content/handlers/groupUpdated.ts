import { getNativeContent } from "../../utils/nativeContent";
import type { ContentTypeHandler, DecodeResult, DecodedMessageLike } from "../types";

export const groupUpdatedHandler: ContentTypeHandler = {
  typeId: "xmtp.org/group_updated:1.0",

  decode(msg: DecodedMessageLike): DecodeResult {
    const nc = getNativeContent(msg as any);
    if (!nc?.groupUpdated) return { kind: "skip" };
    return { kind: "skip" };
  },

  preview(): string | null {
    return null;
  },
};
