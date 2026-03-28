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
import { getClient } from "../xmtp/client";
import { useAuthStore } from "../store/auth";
import {
  useMessageStore,
  decodedToMessageItem,
} from "../store/messages";

const PAGE_SIZE = 30;

export function useMessages(conversationId: ConversationId | null) {
  const streamStarted = useRef(false);
  const conversationRef = useRef<any>(null);

  useEffect(() => {
    if (!conversationId) return;

    const client = getClient();
    if (!client) return;

    // 1. Fetch initial history (access store directly, not via selector)
    useMessageStore.getState().fetchMessages(conversationId, { limit: PAGE_SIZE });

    // 2. Start message stream
    let cancelled = false;
    const startStream = async () => {
      if (streamStarted.current) return;

      try {
        const groups = await client.conversations.listGroups();
        const dms = await client.conversations.listDms();
        const all = [...groups, ...dms];
        const convo = all.find(
          (c) => (c.id as string) === (conversationId as string)
        );

        if (!convo || cancelled) return;

        conversationRef.current = convo;
        streamStarted.current = true;

        const myInboxId = useAuthStore.getState().inboxId;

        await convo.streamMessages(async (decodedMsg: any) => {
          if (cancelled) return;
          try {
            const item = decodedToMessageItem(
              decodedMsg,
              conversationId,
              myInboxId
            );
            if (item) {
              useMessageStore.getState().append(item);
            }
          } catch (err) {
            console.error("[useMessages] Failed to process streamed message:", err);
          }
        });
      } catch (err) {
        console.error("[useMessages] streamMessages() failed:", err);
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
