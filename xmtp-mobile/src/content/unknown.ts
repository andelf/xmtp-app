/**
 * Fallback decoder for unregistered content types.
 * Handles nc.unknown, nc.encoded, and msg.fallback.
 */
import { getNativeContent } from "../utils/nativeContent";
import type { DecodeResult, DecodedMessageLike } from "./types";

/**
 * Attempt to decode an unknown/unregistered content type.
 * Returns a message DecodeResult with best-effort text extraction,
 * or skip if no content can be extracted at all.
 */
export function decodeUnknown(msg: DecodedMessageLike): DecodeResult {
  const nc = getNativeContent(msg as any);
  if (!nc) return { kind: "skip" };

  // Try nc.unknown.content first
  if (nc.unknown) {
    const unk = nc.unknown as { contentTypeId?: string; content?: string };
    if (unk.content) {
      return { kind: "message", text: unk.content };
    }
    if ((msg as any).fallback) {
      return { kind: "message", text: (msg as any).fallback };
    }
    const typeId = unk.contentTypeId ?? msg.contentTypeId ?? "unknown";
    const fallback = (msg as any).fallback;
    const text = fallback
      ? `Unsupported content type: ${typeId}\n${fallback}`
      : `Unsupported content type: ${typeId}`;
    return { kind: "message", text };
  }

  // Try nc.encoded (e.g. markdown, custom content types)
  if (nc.encoded) {
    try {
      const encoded = JSON.parse(nc.encoded);
      if (encoded.content) {
        const text = globalThis.Buffer.from(encoded.content, "base64").toString("utf-8");
        return { kind: "message", text };
      }
      if (encoded.fallback) {
        return { kind: "message", text: encoded.fallback };
      }
    } catch {
      // encoded parse failed — fall through
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

  if (nc.unknown) {
    const unk = nc.unknown as { contentTypeId?: string; content?: string };
    return unk.content ?? (msg as any).fallback ?? `Unsupported content type: ${unk.contentTypeId ?? msg.contentTypeId ?? "unknown"}`;
  }

  if (nc.encoded) {
    try {
      const encoded = JSON.parse(nc.encoded);
      if (encoded.content) {
        return globalThis.Buffer.from(encoded.content, "base64").toString("utf-8");
      }
      if (encoded.fallback) {
        return encoded.fallback;
      }
    } catch {
      // fall through
    }
  }

  return `Unsupported content type: ${msg.contentTypeId ?? "unknown"}`;
}
