/**
 * JS Codecs for Coinbase Actions and Intent content types.
 *
 * These must be registered with Client.create({ codecs: [...] })
 * so the SDK's native bridge can send them via convo.send(content, { contentType }).
 */
import { content } from "@xmtp/proto";
import type { JSContentCodec, ContentTypeId, EncodedContent } from "@xmtp/react-native-sdk";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface CoinbaseActionsContent {
  id: string;
  description: string;
  actions: Array<{
    id: string;
    label: string;
    style?: string;
    imageUrl?: string;
  }>;
}

export interface CoinbaseIntentContent {
  id: string;
  actionId: string;
  metadata?: Record<string, unknown>;
}

// ---------------------------------------------------------------------------
// Actions Codec
// ---------------------------------------------------------------------------

export const CoinbaseActionsContentType: ContentTypeId = {
  authorityId: "coinbase.com",
  typeId: "actions",
  versionMajor: 1,
  versionMinor: 0,
};

export class CoinbaseActionsCodec implements JSContentCodec<CoinbaseActionsContent> {
  contentType = CoinbaseActionsContentType;

  encode(actions: CoinbaseActionsContent): EncodedContent {
    const json = JSON.stringify(actions);
    return {
      type: CoinbaseActionsContentType,
      parameters: {},
      content: new Uint8Array(Buffer.from(json, "utf-8")),
      fallback: `${actions.description}\n${actions.actions.map((a, i) => `[${i + 1}] ${a.label}`).join("\n")}`,
    } as EncodedContent;
  }

  decode(encoded: EncodedContent): CoinbaseActionsContent {
    const json = Buffer.from(encoded.content).toString("utf-8");
    return JSON.parse(json);
  }

  fallback(actions: CoinbaseActionsContent): string {
    return `${actions.description}\n${actions.actions.map((a, i) => `[${i + 1}] ${a.label}`).join("\n")}`;
  }

  shouldPush(): boolean {
    return true;
  }
}

// ---------------------------------------------------------------------------
// Intent Codec
// ---------------------------------------------------------------------------

export const CoinbaseIntentContentType: ContentTypeId = {
  authorityId: "coinbase.com",
  typeId: "intent",
  versionMajor: 1,
  versionMinor: 0,
};

export class CoinbaseIntentCodec implements JSContentCodec<CoinbaseIntentContent> {
  contentType = CoinbaseIntentContentType;

  encode(intent: CoinbaseIntentContent): EncodedContent {
    const json = JSON.stringify(intent);
    return {
      type: CoinbaseIntentContentType,
      parameters: {},
      content: new Uint8Array(Buffer.from(json, "utf-8")),
      fallback: `Selected action: ${intent.actionId}`,
    } as EncodedContent;
  }

  decode(encoded: EncodedContent): CoinbaseIntentContent {
    const json = Buffer.from(encoded.content).toString("utf-8");
    return JSON.parse(json);
  }

  fallback(intent: CoinbaseIntentContent): string {
    return `Selected action: ${intent.actionId}`;
  }

  shouldPush(): boolean {
    return true;
  }
}
