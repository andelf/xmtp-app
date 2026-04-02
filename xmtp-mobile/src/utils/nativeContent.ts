/**
 * Shared helpers for reading XMTP DecodedMessage nativeContent.
 *
 * msg.content() throws for unregistered codecs, so we bypass it and
 * read the raw nativeContent directly from the native bridge.
 */
import type { DecodedMessage } from "@xmtp/react-native-sdk";
import { previewMessage } from "../content";

/** Access the undocumented nativeContent field, centralising the cast. */
export function getNativeContent(msg: DecodedMessage): Record<string, any> | undefined {
  return (msg as any).nativeContent as Record<string, any> | undefined;
}

/**
 * Extract plain text from a DecodedMessage for preview purposes.
 * Returns null for non-content messages (reaction, readReceipt, groupUpdated).
 *
 * Delegates to the content type registry's previewMessage.
 */
export function extractNativeText(msg: DecodedMessage): string | null {
  return previewMessage(msg as any);
}

/** Check if a nativeContent is a reaction (not a content message). */
export function isReactionContent(nc: Record<string, any>): boolean {
  return !!(nc.reaction || nc.reactionV2);
}

/** Extract reaction emoji from nativeContent, or undefined. */
export function extractReactionEmoji(nc: Record<string, any>): string | undefined {
  const r = nc.reaction ?? nc.reactionV2;
  return r?.content;
}
