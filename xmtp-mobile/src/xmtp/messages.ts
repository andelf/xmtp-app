/**
 * Message sending with optimistic UI integration.
 *
 * Flow: addPending -> conversation.send(text) -> confirmSent / markFailed
 */
import type { ConversationId, MessageId } from "@xmtp/react-native-sdk";
import { getClient } from "./client";
import { useAuthStore } from "../store/auth";
import {
  useMessageStore,
  decodedToMessageItem,
  type MessageItem,
} from "../store/messages";
import { useConversationStore } from "../store/conversations";

/**
 * Send a text message to a conversation with optimistic UI.
 *
 * 1. Adds a pending message to the store (status: "sending")
 * 2. Calls conversation.send(text)
 * 3. On success: replaces pending with confirmed message
 * 4. On failure: marks pending as "failed"
 *
 * @returns The confirmed MessageItem on success, or null on failure.
 */
export async function sendMessage(
  conversationId: ConversationId,
  text: string
): Promise<MessageItem | null> {
  const client = getClient();
  if (!client) {
    console.error("[sendMessage] XMTP client not initialised");
    return null;
  }

  const store = useMessageStore.getState();

  // 1. Optimistic insert
  const tempId = store.addPending(conversationId, text);

  try {
    // Find the conversation
    const groups = await client.conversations.listGroups();
    const dms = await client.conversations.listDms();
    const all = [...groups, ...dms];
    const convo = all.find(
      (c) => (c.id as string) === (conversationId as string)
    );

    if (!convo) {
      console.error("[sendMessage] Conversation not found:", conversationId);
      store.markFailed(conversationId, tempId);
      return null;
    }

    // 2. Send via SDK
    const messageId = await convo.send(text);

    // 3. Build the confirmed MessageItem
    const myInboxId = useAuthStore.getState().inboxId ?? "";
    const confirmed: MessageItem = {
      id: (messageId ?? tempId) as unknown as MessageId,
      conversationId,
      senderInboxId: myInboxId,
      text,
      contentType: "xmtp.org/text:1.0",
      sentAt: Date.now(),
      status: "published" as any,
      isOwn: true,
    };

    store.confirmSent(conversationId, tempId, confirmed);

    // Update conversation lastMessage preview
    useConversationStore
      .getState()
      .updateLastMessage(conversationId as string, text, confirmed.sentAt);

    return confirmed;
  } catch (err) {
    // 4. Mark as failed
    console.error("[sendMessage] Failed:", err);
    store.markFailed(conversationId, tempId);
    return null;
  }
}
