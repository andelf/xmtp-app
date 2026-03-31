/**
 * Message sending with optimistic UI integration.
 *
 * Flow: addPending -> conversation.send(text) -> confirmSent / markFailed
 */
import { getClient } from "./client";
import { useAuthStore } from "../store/auth";
import { useMessageStore, type MessageItem } from "../store/messages";
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
  conversationId: string,
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
    const convo = await findConversation(conversationId);
    if (!convo) {
      store.markFailed(conversationId, tempId);
      return null;
    }

    // 2. Send via SDK
    const messageId = await convo.send(text);

    // 3. Build the confirmed MessageItem
    const myInboxId = useAuthStore.getState().inboxId ?? "";
    const confirmed: MessageItem = {
      id: messageId ?? tempId,
      conversationId,
      senderInboxId: myInboxId,
      text,
      contentType: "xmtp.org/text:1.0",
      sentAt: Date.now(),
      status: "published",
      isOwn: true,
    };

    store.confirmSent(conversationId, tempId, confirmed);

    // Update conversation lastMessage preview
    useConversationStore
      .getState()
      .updateLastMessage(conversationId, text, confirmed.sentAt);

    return confirmed;
  } catch (err) {
    // 4. Mark as failed
    console.error("[sendMessage] Failed:", err);
    store.markFailed(conversationId, tempId);
    return null;
  }
}

/** Find a conversation object by id, with module-level cache. */
const conversationCache = new Map<string, any>();

/** Clear cached conversation objects (call on logout). */
export function clearConversationCache() {
  conversationCache.clear();
}

export async function findConversation(conversationId: string) {
  const key = conversationId;
  if (conversationCache.has(key)) return conversationCache.get(key)!;
  const client = getClient();
  if (!client) return null;
  const groups = await client.conversations.listGroups();
  const dms = await client.conversations.listDms();
  const convo = [...groups, ...dms].find((c) => (c.id as string) === key) ?? null;
  if (convo) conversationCache.set(key, convo);
  return convo;
}

/**
 * Send a reaction to a message.
 * Passes NativeMessageContent directly to convo.send() to avoid codec registration.
 */
export async function sendReaction(
  conversationId: string,
  referenceMessageId: string,
  emoji: string,
  action: "added" | "removed" = "added"
): Promise<boolean> {
  try {
    const convo = await findConversation(conversationId);
    if (!convo) return false;

    // Send as NativeMessageContent — the native bridge handles it directly
    await convo.send({
      reaction: {
        reference: referenceMessageId,
        action,
        schema: "unicode",
        content: emoji,
      },
    } as any);

    // Optimistically apply locally
    const myInboxId = useAuthStore.getState().inboxId ?? "";
    useMessageStore.getState().applyReaction({
      conversationId,
      referenceMessageId,
      emoji,
      action,
      senderInboxId: myInboxId,
    });

    return true;
  } catch (err) {
    console.error("[sendReaction] Failed:", err);
    return false;
  }
}

/**
 * Send a reply to a message.
 * Uses NativeMessageContent { reply: { reference, content: { text }, contentType } }.
 */
export async function sendReply(
  conversationId: string,
  referenceMessageId: string,
  text: string
): Promise<MessageItem | null> {
  const client = getClient();
  if (!client) return null;

  const store = useMessageStore.getState();
  const tempId = store.addPending(conversationId, text);

  try {
    const convo = await findConversation(conversationId);
    if (!convo) {
      store.markFailed(conversationId, tempId);
      return null;
    }

    const messageId = await convo.send({
      reply: {
        reference: referenceMessageId,
        content: { text },
        contentType: "xmtp.org/text:1.0",
      },
    } as any);

    const myInboxId = useAuthStore.getState().inboxId ?? "";
    const confirmed: MessageItem = {
      id: messageId ?? tempId,
      conversationId,
      senderInboxId: myInboxId,
      text,
      contentType: "xmtp.org/reply:1.0",
      sentAt: Date.now(),
      status: "published",
      isOwn: true,
      replyRef: {
        referenceMessageId,
        referenceText: undefined,
      },
    };

    store.confirmSent(conversationId, tempId, confirmed);
    useConversationStore
      .getState()
      .updateLastMessage(conversationId, text, confirmed.sentAt);

    return confirmed;
  } catch (err) {
    console.error("[sendReply] Failed:", err);
    store.markFailed(conversationId, tempId);
    return null;
  }
}

/**
 * Send a read receipt for a conversation.
 * Uses NativeMessageContent { readReceipt: {} } — the native bridge handles it.
 */
async function sendReadReceiptRaw(conversationId: string): Promise<boolean> {
  try {
    const convo = await findConversation(conversationId);
    if (!convo) return false;
    await convo.send({ readReceipt: {} } as any);
    return true;
  } catch (err) {
    console.error("[sendReadReceipt] Failed:", err);
    return false;
  }
}

/**
 * Throttled read receipt sender — max 1 per 3s per conversation.
 * Prevents message storms when stream replays multiple messages on connect/reconnect.
 */
const READ_RECEIPT_THROTTLE_MS = 3000;
const lastReadReceiptSent = new Map<string, number>();

export function sendReadReceipt(conversationId: string): Promise<boolean> {
  const now = Date.now();
  const last = lastReadReceiptSent.get(conversationId) ?? 0;
  if (now - last < READ_RECEIPT_THROTTLE_MS) {
    return Promise.resolve(false);
  }
  lastReadReceiptSent.set(conversationId, now);
  return sendReadReceiptRaw(conversationId);
}
