/**
 * Handler for Coinbase Actions (coinbase.com/actions:1.0)
 * and Intent (coinbase.com/intent:1.0) content types.
 *
 * Actions are decoded into structured ActionItem[] for interactive rendering.
 * Intent is decoded as a plain text summary ("Selected action: ...").
 */
import { extractRawContent } from "../decode-utils";
import type {
  ContentTypeHandler,
  DecodeResult,
  DecodedMessageLike,
  ActionsPayload,
} from "../types";

/** Parse raw JSON content into an ActionsPayload, or null if invalid. */
function parseActions(raw: string): ActionsPayload | null {
  try {
    const parsed = JSON.parse(raw);
    if (!parsed.id || !parsed.description || !Array.isArray(parsed.actions)) return null;
    if (parsed.actions.length === 0) return null;
    for (const a of parsed.actions) {
      if (!a.id || !a.label) return null;
    }
    return {
      id: parsed.id,
      description: parsed.description,
      actions: parsed.actions.map((a: any) => ({
        id: a.id,
        label: a.label,
        style: a.style,
        imageUrl: a.imageUrl ?? a.image_url,
      })),
    };
  } catch {
    return null;
  }
}

function formatActionsSummary(payload: ActionsPayload): string {
  const lines = payload.actions.map((a, i) => `[${i + 1}] ${a.label}`);
  return `${payload.description}\n${lines.join("\n")}`;
}

export const actionsHandler: ContentTypeHandler = {
  typeId: "coinbase.com/actions:1.0",

  decode(msg: DecodedMessageLike): DecodeResult {
    const raw = extractRawContent(msg);
    if (!raw) {
      const fallback = (msg as any).fallback;
      if (fallback) return { kind: "message", text: fallback };
      return { kind: "message", text: "Unsupported content type: coinbase.com/actions:1.0" };
    }

    const payload = parseActions(raw);
    if (!payload) {
      return { kind: "message", text: raw };
    }

    return {
      kind: "actions",
      text: formatActionsSummary(payload),
      payload,
    };
  },

  preview(msg: DecodedMessageLike): string | null {
    const raw = extractRawContent(msg);
    if (!raw) return (msg as any).fallback ?? null;
    const payload = parseActions(raw);
    if (!payload) return raw;
    return `[Actions] ${payload.description}`;
  },
};

export const intentHandler: ContentTypeHandler = {
  typeId: "coinbase.com/intent:1.0",

  decode(msg: DecodedMessageLike): DecodeResult {
    const raw = extractRawContent(msg);
    if (!raw) {
      const fallback = (msg as any).fallback;
      if (fallback) return { kind: "message", text: fallback };
      return { kind: "message", text: "Unsupported content type: coinbase.com/intent:1.0" };
    }

    try {
      const parsed = JSON.parse(raw);
      const actionId = parsed.actionId ?? parsed.action_id ?? "unknown";
      const actionsId = parsed.id ?? "unknown";
      return { kind: "intent", text: `Selected action: ${actionId}`, actionsId, actionId };
    } catch {
      return { kind: "message", text: raw };
    }
  },

  preview(): string | null {
    return null;
  },
};
