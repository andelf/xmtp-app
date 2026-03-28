/**
 * Message store -- manages per-conversation message lists with optimistic send.
 *
 * Messages are stored as `Record<string, MessageItem[]>` keyed by conversationId.
 * Each array is kept sorted by `sentAt` ascending.
 */
import { create } from "zustand";
import type {
  ConversationId,
  MessageId,
  MessageDeliveryStatus,
  DecodedMessage,
} from "@xmtp/react-native-sdk";
import { getClient } from "../xmtp/client";
import { useAuthStore } from "./auth";

// ---------------------------------------------------------------------------
// Shared types (exported for Coder-2)
// ---------------------------------------------------------------------------

export interface MessageItem {
  id: MessageId;
  conversationId: ConversationId;
  senderInboxId: string;
  /** Decoded text content (for text messages) */
  text: string;
  /** Original content type URI (e.g. "xmtp.org/text:1.0") */
  contentType: string;
  /** Sent timestamp (epoch ms) */
  sentAt: number;
  /** Delivery status */
  status: MessageDeliveryStatus | "sending" | "failed";
  /** True if sent by the current user */
  isOwn: boolean;
}

// ---------------------------------------------------------------------------
// Conversion helper
// ---------------------------------------------------------------------------

/**
 * Convert an XMTP DecodedMessage to our MessageItem.
 * Returns null if the message cannot be decoded (e.g. unsupported content type).
 */
export function decodedToMessageItem(
  msg: DecodedMessage,
  conversationId: ConversationId,
  myInboxId: string | null
): MessageItem | null {
  let text: string;
  try {
    const content = msg.content();
    if (typeof content !== "string") return null;
    text = content;
  } catch {
    return null;
  }

  return {
    id: msg.id as MessageId,
    conversationId,
    senderInboxId: msg.senderInboxId,
    text,
    contentType: msg.contentTypeId ?? "xmtp.org/text:1.0",
    sentAt: msg.sentNs ? msg.sentNs / 1_000_000 : Date.now(),
    status: msg.deliveryStatus ?? ("published" as MessageDeliveryStatus),
    isOwn: msg.senderInboxId === myInboxId,
  };
}

// ---------------------------------------------------------------------------
// Store types
// ---------------------------------------------------------------------------

export interface MessageState {
  /** Messages keyed by conversation id (sorted by sentAt ascending) */
  byConversation: Record<string, MessageItem[]>;
  isLoading: boolean;
}

export interface MessageActions {
  /** Load message history for a conversation. */
  fetchMessages: (
    conversationId: ConversationId,
    opts?: { limit?: number; before?: number }
  ) => Promise<void>;
  /** Append a single message (from stream). Deduplicates by id. */
  append: (msg: MessageItem) => void;
  /** Add an optimistic "sending" message. Returns the temporary id. */
  addPending: (conversationId: ConversationId, text: string) => string;
  /** Replace a pending message with the confirmed version. */
  confirmSent: (
    conversationId: ConversationId,
    tempId: string,
    realMessage: MessageItem
  ) => void;
  /** Mark a pending message as failed. */
  markFailed: (conversationId: ConversationId, tempId: string) => void;
  /** Update an existing message (e.g. delivery status change). */
  updateMessage: (
    conversationId: ConversationId,
    messageId: MessageId,
    patch: Partial<MessageItem>
  ) => void;
  /** Get sorted messages for a conversation (sentAt ascending). */
  getMessages: (conversationId: ConversationId) => MessageItem[];
  /** Clear messages for a conversation or all. */
  clear: (conversationId?: ConversationId) => void;
}

export type MessageStore = MessageState & MessageActions;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

let pendingCounter = 0;

function generateTempId(): string {
  pendingCounter += 1;
  return `__pending_${Date.now()}_${pendingCounter}`;
}

/** Insert into sorted array (sentAt ascending), returning new array. */
function insertSorted(arr: MessageItem[], item: MessageItem): MessageItem[] {
  // Fast path: append if newer than last
  if (arr.length === 0 || item.sentAt >= arr[arr.length - 1].sentAt) {
    return [...arr, item];
  }
  const next = [...arr];
  const idx = next.findIndex((m) => m.sentAt > item.sentAt);
  next.splice(idx === -1 ? next.length : idx, 0, item);
  return next;
}

// ---------------------------------------------------------------------------
// Store implementation
// ---------------------------------------------------------------------------

export const useMessageStore = create<MessageStore>((set, get) => ({
  byConversation: {},
  isLoading: false,

  fetchMessages: async (conversationId, opts) => {
    const client = getClient();
    if (!client) return;

    set({ isLoading: true });
    try {
      // Find conversation by id
      const conversation = await client.conversations.findConversationByTopic(
        conversationId as string as any
      ).catch(() => null);

      // Fallback: try by listing all and finding by id
      let convo = conversation;
      if (!convo) {
        const groups = await client.conversations.listGroups();
        const dms = await client.conversations.listDms();
        const all = [...groups, ...dms];
        convo = all.find((c) => (c.id as string) === (conversationId as string)) ?? null;
      }

      if (!convo) {
        console.warn("[MessageStore] Conversation not found:", conversationId);
        set({ isLoading: false });
        return;
      }

      // Sync conversation to get latest messages
      await convo.sync();

      const limit = opts?.limit ?? 30;
      const queryOpts: Record<string, any> = { limit };
      if (opts?.before) {
        // Convert ms to ns for the SDK
        queryOpts.beforeNs = opts.before * 1_000_000;
      }

      const sdkMessages = await convo.messages(queryOpts);
      const myInboxId = useAuthStore.getState().inboxId;

      const items: MessageItem[] = [];
      for (const msg of sdkMessages) {
        const item = decodedToMessageItem(msg, conversationId, myInboxId);
        if (item) items.push(item);
      }

      // Sort ascending by sentAt
      items.sort((a, b) => a.sentAt - b.sentAt);

      set((state) => {
        const key = conversationId as string;
        if (opts?.before) {
          // Prepend older messages (pagination)
          const existing = state.byConversation[key] ?? [];
          const existingIds = new Set(existing.map((m) => m.id));
          const newItems = items.filter((m) => !existingIds.has(m.id));
          return {
            byConversation: {
              ...state.byConversation,
              [key]: [...newItems, ...existing],
            },
            isLoading: false,
          };
        }
        return {
          byConversation: {
            ...state.byConversation,
            [key]: items,
          },
          isLoading: false,
        };
      });
    } catch (err) {
      console.error("[MessageStore] fetchMessages failed:", err);
      set({ isLoading: false });
    }
  },

  append: (msg) => {
    set((state) => {
      const key = msg.conversationId as string;
      const existing = state.byConversation[key] ?? [];
      // Deduplicate by real message id
      if (existing.some((m) => m.id === msg.id)) return state;
      // If this is our own message from stream, replace any pending with
      // matching text — avoids the brief flash of duplicate bubbles.
      if (msg.isOwn) {
        const pendingIdx = existing.findIndex(
          (m) =>
            (m.id as string).startsWith("__pending_") && m.text === msg.text
        );
        if (pendingIdx !== -1) {
          const updated = [...existing];
          updated[pendingIdx] = msg;
          return {
            byConversation: { ...state.byConversation, [key]: updated },
          };
        }
      }
      return {
        byConversation: {
          ...state.byConversation,
          [key]: insertSorted(existing, msg),
        },
      };
    });
  },

  addPending: (conversationId, text) => {
    const tempId = generateTempId();
    const myInboxId = useAuthStore.getState().inboxId ?? "";

    const pending: MessageItem = {
      id: tempId as unknown as MessageId,
      conversationId,
      senderInboxId: myInboxId,
      text,
      contentType: "xmtp.org/text:1.0",
      sentAt: Date.now(),
      status: "sending",
      isOwn: true,
    };

    set((state) => {
      const key = conversationId as string;
      const existing = state.byConversation[key] ?? [];
      return {
        byConversation: {
          ...state.byConversation,
          [key]: [...existing, pending],
        },
      };
    });

    return tempId;
  },

  confirmSent: (conversationId, tempId, realMessage) => {
    set((state) => {
      const key = conversationId as string;
      const existing = state.byConversation[key];
      if (!existing) return state;
      // Remove the pending message and insert the real one
      const filtered = existing.filter(
        (m) => (m.id as string) !== tempId
      );
      // Deduplicate against real message id
      if (filtered.some((m) => m.id === realMessage.id)) {
        return {
          byConversation: { ...state.byConversation, [key]: filtered },
        };
      }
      return {
        byConversation: {
          ...state.byConversation,
          [key]: insertSorted(filtered, realMessage),
        },
      };
    });
  },

  markFailed: (conversationId, tempId) => {
    set((state) => {
      const key = conversationId as string;
      const existing = state.byConversation[key];
      if (!existing) return state;
      return {
        byConversation: {
          ...state.byConversation,
          [key]: existing.map((m) =>
            (m.id as string) === tempId ? { ...m, status: "failed" as const } : m
          ),
        },
      };
    });
  },

  updateMessage: (conversationId, messageId, patch) => {
    set((state) => {
      const key = conversationId as string;
      const existing = state.byConversation[key];
      if (!existing) return state;
      return {
        byConversation: {
          ...state.byConversation,
          [key]: existing.map((m) =>
            m.id === messageId ? { ...m, ...patch } : m
          ),
        },
      };
    });
  },

  getMessages: (conversationId) => {
    const { byConversation } = get();
    return byConversation[conversationId as string] ?? [];
  },

  clear: (conversationId) => {
    if (conversationId) {
      set((state) => {
        const next = { ...state.byConversation };
        delete next[conversationId as string];
        return { byConversation: next };
      });
    } else {
      set({ byConversation: {} });
    }
  },
}));
