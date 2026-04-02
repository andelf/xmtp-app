/**
 * Shared helpers for content type decoding.
 */
import { getNativeContent } from "../utils/nativeContent";
import type { DecodedMessageLike } from "./types";

const MAX_CONTENT_LENGTH = 2000;

/** Truncate text to MAX_CONTENT_LENGTH, appending "…" if trimmed. */
export function truncate(text: string): string {
  if (text.length <= MAX_CONTENT_LENGTH) return text;
  return text.slice(0, MAX_CONTENT_LENGTH) + "…";
}

/**
 * Extract raw text content from a message's nativeContent.
 * Tries nc.unknown.content first, then nc.encoded (base64).
 * Returns null if no text can be extracted.
 */
export function extractRawContent(msg: DecodedMessageLike): string | null {
  const nc = getNativeContent(msg as any);
  if (!nc) return null;

  if (nc.unknown) {
    const unk = nc.unknown as { content?: string };
    if (unk.content) return unk.content;
  }

  if (nc.encoded) {
    try {
      const encoded = JSON.parse(nc.encoded);
      if (encoded.content) {
        return globalThis.Buffer.from(encoded.content, "base64").toString("utf-8");
      }
    } catch {
      // fall through
    }
  }

  return null;
}
