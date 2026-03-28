/**
 * Message bubble with optional header (sender + time) above the bubble.
 *
 * Header is shown when:
 * - It's the first message, OR
 * - Different sender from previous message, OR
 * - Same sender but >2 min gap from previous message
 *
 * In group chats, the header shows sender label (colored) + time.
 * In DMs or for own messages, the header shows only time.
 * The bubble itself contains only the message text + status icon.
 */
import React, { memo } from "react";
import { View, StyleSheet, Dimensions } from "react-native";
import { Text, Icon } from "react-native-paper";

import type { MessageItem } from "../store/messages";
import { useMessageStore } from "../store/messages";
import { formatMessageTime } from "../utils/time";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface MessageBubbleProps {
  item: MessageItem;
  /** Previous message in the list (for grouping consecutive messages). */
  prevItem?: MessageItem | null;
  /** Whether this is a group conversation. */
  isGroup?: boolean;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SCREEN_WIDTH = Dimensions.get("window").width;
const MAX_BUBBLE_WIDTH = SCREEN_WIDTH * 0.75;
const GROUP_TIME_THRESHOLD = 2 * 60 * 1000; // 2 minutes in ms

const SENDER_COLORS = [
  "#BB86FC", "#03DAC6", "#CF6679", "#FFAB40",
  "#69F0AE", "#40C4FF", "#FF8A65", "#B388FF",
];

function senderColor(inboxId: string): string {
  let hash = 0;
  for (let i = 0; i < inboxId.length; i++) {
    hash = (hash * 31 + inboxId.charCodeAt(i)) | 0;
  }
  return SENDER_COLORS[Math.abs(hash) % SENDER_COLORS.length];
}

function senderLabel(inboxId: string): string {
  if (inboxId.startsWith("0x") && inboxId.length > 10) {
    return inboxId.slice(0, 6) + "..." + inboxId.slice(-4);
  }
  return inboxId.slice(0, 8) + "...";
}

/** Should we show the header (sender + time) above this bubble? */
function shouldShowHeader(
  item: MessageItem,
  prevItem: MessageItem | null | undefined,
): boolean {
  if (!prevItem) return true; // first message
  if (prevItem.senderInboxId !== item.senderInboxId) return true; // different sender
  if (Math.abs(item.sentAt - prevItem.sentAt) > GROUP_TIME_THRESHOLD) return true; // >2min gap
  return false;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

function MessageBubbleInner({ item, prevItem, isGroup = false }: MessageBubbleProps) {
  const isOwn = item.isOwn;
  const showHeader = shouldShowHeader(item, prevItem);
  const timeLabel = formatMessageTime(item.sentAt);
  const isSending = item.status === "sending";
  const isFailed = item.status === "failed";

  // Resolve reply reference text from store
  let replyText: string | undefined;
  if (item.replyRef) {
    const msgs = useMessageStore.getState().getMessages(item.conversationId);
    const refId = item.replyRef.referenceMessageId;
    const found = msgs.find((m) => (m.id as string) === refId);
    replyText = found?.text;

    // Debug: log ID comparison
    if (!found) {
      const sampleIds = msgs.slice(0, 5).map((m) => m.id as string);
      console.log("[ReplyLookup] MISS refId=", refId, "sampleMsgIds=", sampleIds);
    }
  }

  return (
    <View style={[styles.row, isOwn ? styles.rowOwn : styles.rowOther]}>
      {/* Header: sender + time, outside the bubble */}
      {showHeader && (
        <View style={[styles.header, isOwn ? styles.headerOwn : styles.headerOther]}>
          {isGroup && !isOwn && (
            <Text
              variant="labelSmall"
              style={[styles.senderText, { color: senderColor(item.senderInboxId) }]}
              numberOfLines={1}
            >
              {senderLabel(item.senderInboxId)}
            </Text>
          )}
          <Text variant="labelSmall" style={styles.headerTime}>
            {timeLabel}
          </Text>
          {isSending && (
            <Icon source="clock-outline" size={11} color="#938F99" />
          )}
          {isFailed && (
            <Icon source="alert-circle-outline" size={11} color="#F2B8B5" />
          )}
        </View>
      )}

      {/* Reply quote — above the bubble */}
      {item.replyRef && (
        <View style={[styles.replyBar, isOwn ? styles.replyBarOwn : styles.replyBarOther]}>
          <Icon source="reply" size={12} color="#938F99" />
          <Text
            variant="labelSmall"
            numberOfLines={1}
            style={styles.replyText}
          >
            {replyText ?? item.replyRef.referenceText ?? "..."}
          </Text>
        </View>
      )}

      {/* Bubble */}
      <View
        style={[
          styles.bubble,
          isOwn ? styles.bubbleOwn : styles.bubbleOther,
          !showHeader && !item.replyRef && styles.bubbleGrouped,
        ]}
      >
        <Text
          variant="bodyMedium"
          style={isOwn ? styles.textOwn : styles.textOther}
        >
          {item.text}
        </Text>
      </View>
    </View>
  );
}

export const MessageBubble = memo(MessageBubbleInner);

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

const styles = StyleSheet.create({
  row: {
    paddingHorizontal: 12,
    marginTop: 1,
  },
  rowOwn: {
    alignItems: "flex-end",
  },
  rowOther: {
    alignItems: "flex-start",
  },
  header: {
    flexDirection: "row",
    alignItems: "center",
    gap: 6,
    marginTop: 8,
    marginBottom: 2,
    paddingHorizontal: 4,
  },
  headerOwn: {
    justifyContent: "flex-end",
  },
  headerOther: {
    justifyContent: "flex-start",
  },
  senderText: {
    fontSize: 12,
    fontWeight: "600",
  },
  headerTime: {
    fontSize: 11,
    color: "#938F99",
  },
  bubble: {
    maxWidth: MAX_BUBBLE_WIDTH,
    borderRadius: 12,
    paddingHorizontal: 12,
    paddingVertical: 8,
  },
  bubbleGrouped: {
    marginTop: 1,
  },
  bubbleOwn: {
    backgroundColor: "#6750A4",
    borderBottomRightRadius: 4,
  },
  bubbleOther: {
    backgroundColor: "#49454F",
    borderBottomLeftRadius: 4,
  },
  textOwn: {
    color: "#FFFFFF",
  },
  textOther: {
    color: "#E6E1E5",
  },
  replyBar: {
    flexDirection: "row",
    alignItems: "center",
    gap: 4,
    maxWidth: MAX_BUBBLE_WIDTH,
    marginBottom: 2,
    paddingHorizontal: 4,
  },
  replyBarOwn: {
    justifyContent: "flex-end",
  },
  replyBarOther: {
    justifyContent: "flex-start",
  },
  replyText: {
    color: "#938F99",
    fontSize: 11,
    fontStyle: "italic",
    flexShrink: 1,
  },
});
