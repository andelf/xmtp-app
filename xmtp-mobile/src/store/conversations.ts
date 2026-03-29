/**
 * Conversation store -- manages the list of XMTP conversations (groups + DMs).
 *
 * Data is stored in a Map<string, ConversationItem> for O(1) lookup by id.
 * A secondary topicToId map enables fast message-to-conversation resolution.
 * `sortedList()` returns items sorted by most-recent message first.
 */
import { create } from "zustand";
import {
  ConversationVersion,
  type Conversation,
  type ConversationId,
  type ConversationTopic,
} from "@xmtp/react-native-sdk";
import type { Group } from "@xmtp/react-native-sdk/build/lib/Group";
import type { Dm } from "@xmtp/react-native-sdk/build/lib/Dm";
import { getClient } from "../xmtp/client";
import { extractMarkdownPreview } from "../utils/markdown";
import { getNativeContent, extractNativeText, extractReactionEmoji } from "../utils/nativeContent";

// ---------------------------------------------------------------------------
// Shared types (exported for Coder-2)
// ---------------------------------------------------------------------------

/** Unified conversation list item -- covers both Group and Dm. */
export interface ConversationItem {
  id: ConversationId;
  topic: ConversationTopic;
  /** 'group' | 'dm' */
  kind: "group" | "dm";
  /** Human-readable title: group name or peer address */
  title: string;
  /** Optional group image URL */
  imageUrl?: string;
  /** Peer inbox id (for DMs) */
  peerInboxId?: string;
  /** Last message preview text */
  lastMessageText?: string;
  /** Timestamp of last message (epoch ms) */
  lastMessageAt?: number;
  /** Created at timestamp (epoch ms) */
  createdAt: number;
  /** Number of unread messages */
  unreadCount: number;
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

/**
 * Truncate an address or inbox id for display: "0x1234...abcd"
 */
function truncateAddress(addr: string): string {
  if (addr.length <= 12) return addr;
  return `${addr.slice(0, 6)}...${addr.slice(-4)}`;
}

/**
 * Convert an XMTP SDK Conversation to our ConversationItem.
 * Fetches the latest message (limit: 1) for the preview text.
 */
export async function conversationToItem(
  conversation: Conversation,
  myInboxId?: string
): Promise<ConversationItem> {
  const isGroup = conversation.version === ConversationVersion.GROUP;
  const kind: "group" | "dm" = isGroup ? "group" : "dm";

  let title: string;
  let imageUrl: string | undefined;
  let peerInboxId: string | undefined;

  if (isGroup) {
    const group = conversation as Group;
    title = group.groupName || "Unnamed Group";
    imageUrl = group.groupImageUrl || undefined;
  } else {
    // DM: resolve the peer's inbox id and address
    const dm = conversation as Dm;
    try {
      peerInboxId = await dm.peerInboxId();
      // Try to get peer's address from member list
      const members = await dm.members();
      const peer = members.find((m) => m.inboxId !== myInboxId);
      const peerAddr = peer?.identities?.[0]?.identifier ?? peerInboxId ?? "Unknown";
      title = truncateAddress(peerAddr);
    } catch {
      title = "DM";
    }
  }

  // Sync conversation messages before fetching preview
  try {
    await conversation.sync();
  } catch {}

  // Fetch recent messages for preview — skip reactions, read receipts, group updates
  let lastMessageText: string | undefined;
  let lastMessageAt: number | undefined;
  try {
    const messages = await conversation.messages({ limit: 5 });
    let firstReactionEmoji: string | undefined;
    let firstReactionAt: number | undefined;
    for (const msg of messages) {
      const nc = getNativeContent(msg);
      if (!nc) continue;
      const emoji = extractReactionEmoji(nc);
      if (emoji) {
        if (!firstReactionEmoji) {
          firstReactionEmoji = emoji;
          firstReactionAt = msg.sentNs ? msg.sentNs / 1_000_000 : undefined;
        }
        continue;
      }
      if (nc.readReceipt !== undefined || nc.groupUpdated) continue;

      const raw = extractNativeText(msg);
      if (raw) {
        // Add [md] prefix for markdown content
        const isMarkdown = (msg as any).contentTypeId?.includes("markdown");
        if (isMarkdown) {
          const preview = extractMarkdownPreview(raw);
          lastMessageText = preview ? `[md] ${preview}` : "[md]";
        } else {
          lastMessageText = raw;
        }
      }
      lastMessageAt = msg.sentNs ? msg.sentNs / 1_000_000 : undefined;
      break;
    }
    if (!lastMessageText && firstReactionEmoji) {
      lastMessageText = `[react] ${firstReactionEmoji}`;
      lastMessageAt = firstReactionAt;
    }
  } catch {
    // Non-critical -- leave preview empty
  }

  return {
    id: conversation.id,
    topic: conversation.topic,
    kind,
    title,
    imageUrl,
    peerInboxId,
    lastMessageText,
    lastMessageAt,
    createdAt: conversation.createdAt,
    unreadCount: 0,
  };
}

// ---------------------------------------------------------------------------
// Store types
// ---------------------------------------------------------------------------

export interface ConversationState {
  /** Conversations indexed by id */
  items: Map<string, ConversationItem>;
  /** Reverse lookup: topic string -> conversation id string */
  topicToId: Map<string, string>;
  isLoading: boolean;
  error: string | null;
}

export interface ConversationActions {
  /** Fetch / refresh the full conversation list from XMTP. */
  fetchAll: () => Promise<void>;
  /** Insert or update a single conversation item. */
  upsert: (item: ConversationItem) => void;
  /** Update the last message preview for a conversation. */
  updateLastMessage: (conversationId: string, text: string, timestamp: number) => void;
  /** Mark a conversation as read (reset unread count). */
  markRead: (conversationId: string) => void;
  /** Increment unread count for a conversation. */
  incrementUnread: (conversationId: string) => void;
  /** The conversation id currently being viewed (skip unread increment). */
  activeConversationId: string | null;
  setActiveConversation: (id: string | null) => void;
  /** Clear store on logout. */
  clear: () => void;
  /** Conversations sorted by lastMessageAt descending (derived). */
  sortedList: () => ConversationItem[];
  /** Look up a conversation id by topic. */
  getIdByTopic: (topic: string) => string | undefined;
}

export type ConversationStore = ConversationState & ConversationActions;

// ---------------------------------------------------------------------------
// Store implementation
// ---------------------------------------------------------------------------

export const useConversationStore = create<ConversationStore>((set, get) => ({
  items: new Map(),
  topicToId: new Map(),
  isLoading: false,
  error: null,
  activeConversationId: null,

  fetchAll: async () => {
    const client = getClient();
    if (!client) {
      set({ error: "XMTP client not initialised" });
      return;
    }

    set({ isLoading: true, error: null });
    try {
      await client.conversations.sync();

      const groups = await client.conversations.listGroups();
      const dms = await client.conversations.listDms();
      const all: Conversation[] = [...groups, ...dms];

      const myInboxId = client.inboxId;
      const nextItems = new Map<string, ConversationItem>();
      const nextTopicToId = new Map<string, string>();

      const converted = await Promise.all(all.map((c) => conversationToItem(c, myInboxId)));

      for (const item of converted) {
        const idStr = item.id as string;
        nextItems.set(idStr, item);
        nextTopicToId.set(item.topic as string, idStr);
      }

      set({ items: nextItems, topicToId: nextTopicToId, isLoading: false });
    } catch (err: any) {
      console.error("[ConversationStore] fetchAll failed:", err);
      set({ error: err?.message ?? String(err), isLoading: false });
    }
  },

  upsert: (item) => {
    set((state) => {
      const next = new Map(state.items);
      const nextTopic = new Map(state.topicToId);
      const idStr = item.id as string;
      next.set(idStr, item);
      nextTopic.set(item.topic as string, idStr);
      return { items: next, topicToId: nextTopic };
    });
  },

  updateLastMessage: (conversationId, text, timestamp) => {
    set((state) => {
      const existing = state.items.get(conversationId);
      if (!existing) return state;

      // Only skip if we already have a preview AND the timestamp is not newer
      if (
        existing.lastMessageText &&
        existing.lastMessageAt &&
        existing.lastMessageAt >= timestamp
      ) {
        return state;
      }

      const next = new Map(state.items);
      const isActive = state.activeConversationId === conversationId;
      next.set(conversationId, {
        ...existing,
        lastMessageText: text,
        lastMessageAt: timestamp,
        unreadCount: isActive ? 0 : existing.unreadCount + 1,
      });
      return { items: next };
    });
  },

  markRead: (conversationId) => {
    set((state) => {
      const existing = state.items.get(conversationId);
      if (!existing || existing.unreadCount === 0) return state;
      const next = new Map(state.items);
      next.set(conversationId, { ...existing, unreadCount: 0 });
      return { items: next };
    });
  },

  incrementUnread: (conversationId) => {
    set((state) => {
      const existing = state.items.get(conversationId);
      if (!existing) return state;
      const next = new Map(state.items);
      next.set(conversationId, { ...existing, unreadCount: existing.unreadCount + 1 });
      return { items: next };
    });
  },

  setActiveConversation: (id) => set({ activeConversationId: id }),

  clear: () => set({ items: new Map(), topicToId: new Map(), error: null, activeConversationId: null }),

  sortedList: () => {
    const { items } = get();
    return Array.from(items.values()).sort(
      (a, b) => (b.lastMessageAt ?? b.createdAt) - (a.lastMessageAt ?? a.createdAt)
    );
  },

  getIdByTopic: (topic) => {
    return get().topicToId.get(topic);
  },
}));
