/**
 * useConversations -- manages conversation list lifecycle.
 *
 * On mount: fetches all conversations, starts streams for new conversations
 * and all messages (to update lastMessage previews).
 *
 * Streams use onClose callback to detect disconnection and auto-reconnect.
 *
 * On unmount: cancels all streams.
 */
import { useEffect, useRef } from "react";
import { getClient } from "../xmtp/client";
import { extractMarkdownPreview } from "../utils/markdown";
import { extractNativeText, getNativeContent } from "../utils/nativeContent";
import { useConversationStore, conversationToItem } from "../store/conversations";
import { log } from "../utils/logger";
import { MAX_RECONNECT, backoffDelay } from "../utils/reconnect";

export function useConversations() {
  const unmountedRef = useRef(false);
  const streamsStarted = useRef(false);

  useEffect(() => {
    const client = getClient();
    if (!client) return;

    unmountedRef.current = false;
    const store = useConversationStore.getState;

    // Initial fetch
    log("Convos", "Starting initial fetchAll + streams");
    store().fetchAll();

    // --- Conversation stream with onClose reconnect ---
    let convoRetries = 0;
    const startConvoStream = () => {
      if (unmountedRef.current) return;

      client.conversations
        .stream(
          async (conversation) => {
            try {
              convoRetries = 0;
              const item = await conversationToItem(conversation, client.inboxId);
              store().upsert(item);
            } catch (err) {
              console.error("[useConversations] convert streamed conv failed:", err);
            }
          },
          "all",
          // onClose: stream disconnected — reconnect with backoff
          () => {
            if (unmountedRef.current) return;
            if (convoRetries < MAX_RECONNECT) {
              const delay = backoffDelay(convoRetries);
              convoRetries++;
              console.warn(
                `[useConversations] convo stream closed, reconnecting in ${delay}ms (${convoRetries}/${MAX_RECONNECT})...`
              );
              store()
                .fetchAll()
                .catch(() => {});
              setTimeout(startConvoStream, delay);
            }
          }
        )
        .catch((err) => {
          if (unmountedRef.current) return;
          console.error("[useConversations] convo stream error:", err);
          if (convoRetries < MAX_RECONNECT) {
            const delay = backoffDelay(convoRetries);
            convoRetries++;
            setTimeout(startConvoStream, delay);
          }
        });
    };

    // --- All messages stream with onClose reconnect ---
    let msgRetries = 0;
    const startMsgStream = () => {
      if (unmountedRef.current) return;

      client.conversations
        .streamAllMessages(
          async (message) => {
            try {
              msgRetries = 0;
              const topicStr = message.topic as string;
              log("MsgStream", `received msg id=${message.id} topic=${topicStr}`);

              let conversationId = store().getIdByTopic(topicStr);
              log(
                "MsgStream",
                `topicLookup: convId=${conversationId ?? "NULL"} topicToIdSize=${store().topicToId.size}`
              );

              // Unknown topic — new conversation, refresh list
              if (!conversationId) {
                log("MsgStream", "unknown topic, doing fetchAll...");
                await store().fetchAll();
                conversationId = store().getIdByTopic(topicStr);
                log("MsgStream", `after fetchAll: convId=${conversationId ?? "STILL NULL"}`);
                if (!conversationId) return;
              }

              // Protocol-level signals — skip before preview extraction
              const nc = getNativeContent(message as any);
              if (nc?.leaveRequest !== undefined) return;
              if (nc?.groupUpdated) {
                // Detect self-removal from group
                const removed = nc.groupUpdated.membersRemoved as { inboxId: string }[] | undefined;
                if (removed?.some((m) => m.inboxId === client.inboxId)) {
                  log(
                    "MsgStream",
                    `self removed from group ${conversationId}, removing from store`
                  );
                  store().remove(conversationId);
                  return;
                }
              }

              const raw = extractNativeText(message);
              if (!raw) return; // skip reactions, read receipts
              const isMarkdown = (message as any).contentTypeId?.includes("markdown");
              let text: string | undefined;
              if (isMarkdown) {
                const preview = extractMarkdownPreview(raw);
                text = preview ? `[md] ${preview}` : "[md]";
              } else {
                text = raw;
              }

              const timestamp = message.sentNs ? message.sentNs / 1_000_000 : Date.now();

              if (text) {
                log(
                  "MsgStream",
                  `updateLastMessage convId=${conversationId} text="${text.slice(0, 30)}" ts=${timestamp}`
                );
                store().updateLastMessage(conversationId, text, timestamp);
                log(
                  "MsgStream",
                  `after update, item.lastMessageText="${store().items.get(conversationId)?.lastMessageText}"`
                );
              }
            } catch (err) {
              console.error("[useConversations] process streamed msg failed:", err);
            }
          },
          "all", // type: groups + dms
          undefined, // consentStates: all (no filter)
          // onClose: stream disconnected — reconnect with backoff
          () => {
            log("MsgStream", "*** onClose triggered — stream disconnected ***");
            if (unmountedRef.current) return;
            if (msgRetries < MAX_RECONNECT) {
              const delay = backoffDelay(msgRetries);
              msgRetries++;
              log("MsgStream", `reconnecting in ${delay}ms (${msgRetries}/${MAX_RECONNECT})...`);
              store()
                .fetchAll()
                .catch(() => {});
              setTimeout(startMsgStream, delay);
            }
          }
        )
        .catch((err) => {
          if (unmountedRef.current) return;
          console.error("[useConversations] msg stream error:", err);
          if (msgRetries < MAX_RECONNECT) {
            const delay = backoffDelay(msgRetries);
            msgRetries++;
            setTimeout(startMsgStream, delay);
          }
        });
    };

    if (!streamsStarted.current) {
      streamsStarted.current = true;
      log("Convos", "Starting convo stream...");
      startConvoStream();
      log("Convos", "Starting msg stream...");
      startMsgStream();
      log("Convos", "Both streams started");
    }

    return () => {
      unmountedRef.current = true;
      if (streamsStarted.current) {
        try {
          client.conversations.cancelStream();
        } catch {}
        try {
          client.conversations.cancelStreamAllMessages();
        } catch {}
        streamsStarted.current = false;
      }
    };
  }, []);
}
