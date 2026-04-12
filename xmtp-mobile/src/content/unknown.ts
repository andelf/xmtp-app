/**
 * Fallback decoder for unregistered content types.
 * Handles nc.unknown, nc.encoded, and msg.fallback.
 */
import { getNativeContent } from "../utils/nativeContent";
import type { DecodeResult, DecodedMessageLike } from "./types";
import { extractRawContent } from "./decode-utils";

/**
 * Attempt to decode an unknown/unregistered content type.
 * Returns a message DecodeResult with best-effort text extraction,
 * or skip if no content can be extracted at all.
 */
export function decodeUnknown(msg: DecodedMessageLike): DecodeResult {
  const nc = getNativeContent(msg as any);
  if (!nc) return { kind: "skip" };

  // Try shared extraction (nc.unknown.content, nc.encoded base64)
  const raw = extractRawContent(msg);
  if (raw) return { kind: "message", text: raw };

  // Try nc.unknown with fallback text
  if (nc.unknown) {
    const unk = nc.unknown as { contentTypeId?: string };
    if ((msg as any).fallback) {
      return { kind: "message", text: (msg as any).fallback };
    }
    const typeId = unk.contentTypeId ?? msg.contentTypeId ?? "unknown";
    return { kind: "message", text: `Unsupported content type: ${typeId}` };
  }

  // Try nc.encoded fallback field
  if (nc.encoded) {
    try {
      const encoded = JSON.parse(nc.encoded);
      if (encoded.fallback) {
        return { kind: "message", text: encoded.fallback };
      }
    } catch {
      // fall through
    }
  }

  // Final fallback
  const typeId = msg.contentTypeId ?? "unknown";
  const fallback = (msg as any).fallback;
  const text = fallback
    ? `Unsupported content type: ${typeId}\n${fallback}`
    : `Unsupported content type: ${typeId}`;
  return { kind: "message", text };
}

/**
 * Preview text for unknown content types.
 */
export function previewUnknown(msg: DecodedMessageLike): string | null {
  const nc = getNativeContent(msg as any);
  if (!nc) return null;

  // Try shared extraction first
  const raw = extractRawContent(msg);
  if (raw) return raw;

  // Fallback text from message or encoded payload
  if ((msg as any).fallback) return (msg as any).fallback;
  if (nc.encoded) {
    try {
      const encoded = JSON.parse(nc.encoded);
      if (encoded.fallback) return encoded.fallback;
    } catch {
      // fall through
    }
  }

  return `Unsupported content type: ${msg.contentTypeId ?? "unknown"}`;
}
