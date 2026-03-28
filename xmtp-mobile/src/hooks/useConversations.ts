/**
 * useConversations -- manages conversation list lifecycle.
 *
 * On mount: fetches all conversations, starts streams for new conversations
 * and all messages (to update lastMessage previews).
 *
 * On unmount: cancels all streams.
 *
 * All store access uses getState() to avoid selector-induced re-renders.
 */
import { useEffect, useRef } from "react";
import { getClient } from "../xmtp/client";
import {
  useConversationStore,
  conversationToItem,
} from "../store/conversations";

const STREAM_RECONNECT_DELAY = 3000;
const MAX_RECONNECT_ATTEMPTS = 10;

export function useConversations() {
  const unmountedRef = useRef(false);
  const streamsStarted = useRef(false);

  useEffect(() => {
    const client = getClient();
    if (!client) return;

    unmountedRef.current = false;
    const store = useConversationStore.getState;

    // 1. Initial fetch
    store().fetchAll();

    // 2. Start conversation stream with auto-reconnect
    let convoRetries = 0;
    const startConvoStream = async () => {
      if (unmountedRef.current) return;
      try {
        await client.conversations.stream(async (conversation) => {
          try {
            const item = await conversationToItem(conversation, client.inboxId);
            store().upsert(item);
          } catch (err) {
            console.error("[useConversations] convert streamed conv failed:", err);
          }
        });
        convoRetries = 0;
      } catch (err) {
        if (!unmountedRef.current && convoRetries < MAX_RECONNECT_ATTEMPTS) {
          convoRetries++;
          store().fetchAll().catch(() => {});
          setTimeout(startConvoStream, STREAM_RECONNECT_DELAY);
        }
      }
    };

    // 3. Start allMessages stream with auto-reconnect
    let msgRetries = 0;
    const startMsgStream = async () => {
      if (unmountedRef.current) return;
      try {
        await client.conversations.streamAllMessages(async (message) => {
          try {
            const topicStr = message.topic as string;
            let conversationId = store().getIdByTopic(topicStr);

            // Unknown topic — new conversation we haven't synced yet.
            // Refresh the full list so it appears in the UI.
            if (!conversationId) {
              await store().fetchAll();
              // Try again after refresh
              conversationId = store().getIdByTopic(topicStr);
              if (!conversationId) return; // still unknown, give up
            }

            let text: string | undefined;
            try {
              const content = message.content();
              text = typeof content === "string" ? content : undefined;
            } catch {
              return;
            }

            const timestamp = message.sentNs
              ? message.sentNs / 1_000_000
              : Date.now();

            if (text) {
              store().updateLastMessage(conversationId, text, timestamp);
            }
          } catch (err) {
            console.error("[useConversations] process streamed msg failed:", err);
          }
        });
        msgRetries = 0;
      } catch (err) {
        if (!unmountedRef.current && msgRetries < MAX_RECONNECT_ATTEMPTS) {
          msgRetries++;
          setTimeout(startMsgStream, STREAM_RECONNECT_DELAY);
        }
      }
    };

    if (!streamsStarted.current) {
      streamsStarted.current = true;
      startConvoStream();
      startMsgStream();
    }

    return () => {
      unmountedRef.current = true;
      if (streamsStarted.current) {
        try { client.conversations.cancelStream(); } catch {}
        try { client.conversations.cancelStreamAllMessages(); } catch {}
        streamsStarted.current = false;
      }
    };
  }, []); // empty deps — runs once on mount
}
