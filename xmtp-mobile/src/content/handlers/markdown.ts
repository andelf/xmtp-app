import { extractMarkdownPreview } from "../../utils/markdown";
import type { ContentTypeHandler, DecodeResult, DecodedMessageLike } from "../types";
import { extractRawContent } from "../decode-utils";

export const markdownHandler: ContentTypeHandler = {
  typeId: "xmtp.org/markdown:1.0",

  decode(msg: DecodedMessageLike): DecodeResult {
    const text = extractRawContent(msg);
    if (!text) return { kind: "skip" };
    return { kind: "message", text, format: "markdown" };
  },

  preview(msg: DecodedMessageLike): string | null {
    const text = extractRawContent(msg);
    if (!text) return null;
    return extractMarkdownPreview(text) ?? text;
  },
};
