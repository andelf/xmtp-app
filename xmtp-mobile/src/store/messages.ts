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
import { getNativeContent } from "../utils/nativeContent";

// ---------------------------------------------------------------------------
// Shared types (exported for Coder-2)
// ---------------------------------------------------------------------------

export interface ReplyRef {
  /** ID of the original message being replied to */
  referenceMessageId: string;
  /** Preview text of the original message (if available) */
  referenceText?: string;
}

/** Aggregated reactions on a message: emoji → set of sender inboxIds */
export type Reactions = Record<string, string[]>;

export interface MessageItem {
  id: MessageId;
  conversationId: ConversationId;
  senderInboxId: string;
  /** Decoded text content */
  text: string;
  /** Original content type URI (e.g. "xmtp.org/text:1.0") */
  contentType: string;
  /** Sent timestamp (epoch ms) */
  sentAt: number;
  /** Delivery status */
  status: MessageDeliveryStatus | "sending" | "failed";
  /** True if sent by the current user */
  isOwn: boolean;
  /** Reply reference (if this message is a reply) */
  replyRef?: ReplyRef;
  /** Aggregated reactions keyed by emoji */
  reactions?: Reactions;
}

// ---------------------------------------------------------------------------
// Conversion helper
// ---------------------------------------------------------------------------

/**
 * Convert an XMTP DecodedMessage to our MessageItem.
 * Supports: text, reply. Returns null for unsupported types (reaction, read receipt, etc.)
 */
export function decodedToMessageItem(
  msg: DecodedMessage,
  conversationId: ConversationId,
  myInboxId: string | null
): MessageItem | null {
  let text: string = "";
  let replyRef: ReplyRef | undefined;

  const nc = getNativeContent(msg);
  if (!nc) return null;

  try {
    if (nc.text != null) {
      // Plain text message
      text = typeof nc.text === "string" ? nc.text : String(nc.text);
    } else if (nc.reply) {
      // Reply: { reply: { reference, content: { text }, contentType } }
      const reply = nc.reply;
      text = reply.content?.text ?? "[reply]";
      replyRef = {
        referenceMessageId: reply.reference ?? "",
        referenceText: undefined, // resolved at render time from store
      };
    } else if (nc.reaction || nc.reactionV2) {
      return null;
    } else if (nc.readReceipt !== undefined) {
      return null;
    } else if (nc.groupUpdated) {
      return null;
    } else if (nc.unknown) {
      // Unknown content type (e.g. markdown) — try to extract text from
      // the encoded payload or fallback text.
      const unk = nc.unknown as { contentTypeId?: string; content?: string };
      if (unk.content) {
        text = unk.content;
      } else if ((msg as any).fallback) {
        text = (msg as any).fallback;
      } else {
        return null;
      }
    } else {
      // Try to decode from encoded payload (e.g. markdown, custom content types)
      if (nc.encoded) {
        try {
          const encoded = JSON.parse(nc.encoded);
          if (encoded.content) {
            text = globalThis.Buffer.from(encoded.content, "base64").toString("utf-8");
          } else if (encoded.fallback) {
            text = encoded.fallback;
          }
        } catch (e) {
          // encoded parse failed — skip message
        }
      }
      if (!text) return null;
    }
  } catch {
    return null;
  }

  if (!text) return null;

  return {
    id: msg.id as MessageId,
    conversationId,
    senderInboxId: msg.senderInboxId,
    text,
    contentType: msg.contentTypeId ?? "xmtp.org/text:1.0",
    sentAt: msg.sentNs ? msg.sentNs / 1_000_000 : Date.now(),
    status: msg.deliveryStatus ?? ("published" as MessageDeliveryStatus),
    isOwn: msg.senderInboxId === myInboxId,
    replyRef,
  };
}

/**
 * Extract a reaction from a DecodedMessage, if it is one.
 * Returns null for non-reaction messages.
 */
export interface ReactionInfo {
  conversationId: ConversationId;
  referenceMessageId: string;
  emoji: string;
  action: "added" | "removed";
  senderInboxId: string;
}

export function decodedToReaction(
  msg: DecodedMessage,
  conversationId: ConversationId
): ReactionInfo | null {
  const nc = getNativeContent(msg);
  if (!nc) return null;
  const r = nc.reaction ?? nc.reactionV2;
  if (!r) return null;
  if (r.action !== "added" && r.action !== "removed") return null;
  return {
    conversationId,
    referenceMessageId: r.reference ?? "",
    emoji: r.content ?? "",
    action: r.action,
    senderInboxId: msg.senderInboxId,
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
  /** Apply a reaction (add/remove) to a message. */
  applyReaction: (reaction: ReactionInfo) => void;
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
      const reactions: ReactionInfo[] = [];
      for (const msg of sdkMessages) {
        const reaction = decodedToReaction(msg, conversationId);
        if (reaction) {
          reactions.push(reaction);
          continue;
        }
        const item = decodedToMessageItem(msg, conversationId, myInboxId);
        if (item) items.push(item);
      }

      // Sort ascending by sentAt
      items.sort((a, b) => a.sentAt - b.sentAt);

      // Resolve reply reference text while all messages are in memory
      for (const item of items) {
        if (item.replyRef && !item.replyRef.referenceText) {
          const ref = items.find((m) => (m.id as string) === item.replyRef!.referenceMessageId);
          if (ref) item.replyRef.referenceText = ref.text;
        }
      }

      // Apply reactions to their referenced messages
      for (const r of reactions) {
        const target = items.find((m) => (m.id as string) === r.referenceMessageId);
        if (!target) continue;
        const prev = target.reactions ?? {};
        const senders = prev[r.emoji] ?? [];
        if (r.action === "added" && !senders.includes(r.senderInboxId)) {
          target.reactions = { ...prev, [r.emoji]: [...senders, r.senderInboxId] };
        } else if (r.action === "removed") {
          const next = senders.filter((s) => s !== r.senderInboxId);
          target.reactions = { ...prev };
          if (next.length > 0) target.reactions[r.emoji] = next;
          else delete target.reactions[r.emoji];
        }
      }

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

  applyReaction: (reaction) => {
    set((state) => {
      const key = reaction.conversationId as string;
      const existing = state.byConversation[key];
      if (!existing) return state;
      const idx = existing.findIndex(
        (m) => (m.id as string) === reaction.referenceMessageId
      );
      if (idx === -1) return state;
      const msg = existing[idx];
      const prev = msg.reactions ?? {};
      const senders = prev[reaction.emoji] ?? [];
      let next: string[];
      if (reaction.action === "added") {
        if (senders.includes(reaction.senderInboxId)) return state;
        next = [...senders, reaction.senderInboxId];
      } else {
        next = senders.filter((s) => s !== reaction.senderInboxId);
      }
      const reactions = { ...prev };
      if (next.length > 0) {
        reactions[reaction.emoji] = next;
      } else {
        delete reactions[reaction.emoji];
      }
      const updated = [...existing];
      updated[idx] = { ...msg, reactions };
      return {
        byConversation: { ...state.byConversation, [key]: updated },
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
