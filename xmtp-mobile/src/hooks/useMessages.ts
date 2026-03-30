/**
 * useMessages -- hook that drives per-conversation message lifecycle.
 *
 * On mount:
 *   1. Calls fetchMessages(conversationId) to load recent history (30 msgs).
 *   2. Starts conversation.streamMessages(callback) for real-time messages.
 *
 * On unmount: cancels the message stream.
 *
 * Returns { isLoading, fetchMore } for the UI.
 * Messages themselves should be read from useMessageStore in the component.
 */
import { useEffect, useRef, useCallback } from "react";
import type { ConversationId } from "@xmtp/react-native-sdk";
import { useAuthStore } from "../store/auth";
import { useMessageStore, decodedToMessageItem, decodedToReaction } from "../store/messages";
import { findConversation } from "../xmtp/messages";
import { getNativeContent } from "../utils/nativeContent";

const PAGE_SIZE = 30;

export function useMessages(conversationId: ConversationId | null) {
  const streamStarted = useRef(false);
  const conversationRef = useRef<any>(null);

  useEffect(() => {
    if (!conversationId) return;

    // 1. Fetch initial history (access store directly, not via selector)
    useMessageStore.getState().fetchMessages(conversationId, { limit: PAGE_SIZE });

    // 2. Start message stream with bounded reconnect
    let cancelled = false;
    let retries = 0;
    const MAX_RETRIES = 10;
    const BASE_DELAY = 1000;
    const MAX_DELAY = 30000;

    const scheduleReconnect = () => {
      if (cancelled || retries >= MAX_RETRIES) {
        if (retries >= MAX_RETRIES) {
          console.error("[useMessages] max reconnect attempts reached, giving up");
        }
        return;
      }
      const delay = Math.min(BASE_DELAY * Math.pow(2, retries), MAX_DELAY);
      retries++;
      console.warn(`[useMessages] reconnecting in ${delay}ms (attempt ${retries}/${MAX_RETRIES})...`);
      setTimeout(() => {
        streamStarted.current = false;
        startStream();
      }, delay);
    };

    const startStream = async () => {
      if (streamStarted.current || cancelled) return;

      try {
        const convo = await findConversation(conversationId);
        if (!convo || cancelled) return;

        conversationRef.current = convo;
        streamStarted.current = true;

        const myInboxId = useAuthStore.getState().inboxId;

        await convo.streamMessages(
          async (decodedMsg: any) => {
            if (cancelled) return;
            // Reset retries on successful message — stream is healthy
            retries = 0;
            try {
              const nc = getNativeContent(decodedMsg);
              console.log("[useMessages] stream msg", decodedMsg.id, "contentTypeId=", (decodedMsg as any).contentTypeId, "nc keys=", nc ? Object.keys(nc) : "null");

              const reaction = decodedToReaction(decodedMsg, conversationId);
              if (reaction) {
                useMessageStore.getState().applyReaction(reaction);
                return;
              }
              const item = decodedToMessageItem(decodedMsg, conversationId, myInboxId);
              console.log("[useMessages] decoded item=", item ? `text="${item.text.slice(0, 60)}"` : "null");
              if (item) {
                useMessageStore.getState().append(item);
              }
            } catch (err) {
              console.error("[useMessages] Failed to process streamed message:", err);
            }
          },
          // onClose: stream disconnected — reconnect with backoff
          () => {
            if (!cancelled) {
              streamStarted.current = false;
              scheduleReconnect();
            }
          }
        );
      } catch (err) {
        console.error("[useMessages] streamMessages() failed:", err);
        streamStarted.current = false;
        scheduleReconnect();
      }
    };

    startStream();

    return () => {
      cancelled = true;
      if (streamStarted.current && conversationRef.current) {
        try {
          conversationRef.current.cancelStreamMessages?.();
        } catch {
          // ignore
        }
        streamStarted.current = false;
        conversationRef.current = null;
      }
    };
  }, [conversationId]); // only re-run when conversationId changes

  // Fetch older messages (pagination)
  const fetchMore = useCallback(async () => {
    if (!conversationId) return;
    const store = useMessageStore.getState();
    if (store.isLoading) return;
    const current = store.getMessages(conversationId);
    if (current.length === 0) return;
    const oldest = current[0];
    await store.fetchMessages(conversationId, {
      limit: PAGE_SIZE,
      before: oldest.sentAt,
    });
  }, [conversationId]);

  const isLoading = useMessageStore((s) => s.isLoading);

  return { isLoading, fetchMore };
}
