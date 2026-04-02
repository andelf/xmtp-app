import { getNativeContent } from "../../utils/nativeContent";
import type { ContentTypeHandler, DecodeResult, DecodedMessageLike } from "../types";

export const textHandler: ContentTypeHandler = {
  typeId: "xmtp.org/text:1.0",

  decode(msg: DecodedMessageLike): DecodeResult {
    const nc = getNativeContent(msg as any);
    if (!nc || nc.text == null) return { kind: "skip" };
    const text = typeof nc.text === "string" ? nc.text : String(nc.text);
    if (!text) return { kind: "skip" };
    return { kind: "message", text };
  },

  preview(msg: DecodedMessageLike): string | null {
    const nc = getNativeContent(msg as any);
    if (!nc || nc.text == null) return null;
    return typeof nc.text === "string" ? nc.text : String(nc.text);
  },
};
