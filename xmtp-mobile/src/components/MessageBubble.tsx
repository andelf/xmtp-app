/**
 * Single message bubble -- renders text content with timestamp and status.
 *
 * Own messages are right-aligned with primary colour background;
 * others' messages are left-aligned with surface variant background.
 */
import React, { memo } from "react";
import { View, StyleSheet, Dimensions } from "react-native";
import { Text, Icon } from "react-native-paper";

import type { MessageItem } from "../store/messages";
import { formatMessageTime } from "../utils/time";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface MessageBubbleProps {
  /** The message data to render. */
  item: MessageItem;
  /** Whether this is a group conversation (shows sender name for others' messages). */
  isGroup?: boolean;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SCREEN_WIDTH = Dimensions.get("window").width;
const MAX_BUBBLE_WIDTH = SCREEN_WIDTH * 0.75;

/** Deterministic color for sender label based on inboxId hash. */
const SENDER_COLORS = [
  "#BB86FC", // purple
  "#03DAC6", // teal
  "#CF6679", // pink
  "#FFAB40", // amber
  "#69F0AE", // green
  "#40C4FF", // blue
  "#FF8A65", // orange
  "#B388FF", // light purple
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

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

function MessageBubbleInner({ item, isGroup = false }: MessageBubbleProps) {
  const isOwn = item.isOwn;
  const timeLabel = formatMessageTime(item.sentAt);

  const isSending = item.status === "sending";
  const isFailed = item.status === "failed";

  return (
    <View
      style={[
        styles.row,
        isOwn ? styles.rowOwn : styles.rowOther,
      ]}
    >
      <View
        style={[
          styles.bubble,
          isOwn ? styles.bubbleOwn : styles.bubbleOther,
        ]}
      >
        {/* Show sender label in group chats for others' messages */}
        {isGroup && !isOwn && (
          <Text
            variant="labelSmall"
            style={[styles.senderLabel, { color: senderColor(item.senderInboxId) }]}
            numberOfLines={1}
          >
            {senderLabel(item.senderInboxId)}
          </Text>
        )}

        <Text
          variant="bodyMedium"
          style={isOwn ? styles.textOwn : styles.textOther}
        >
          {item.text}
        </Text>

        <View style={styles.meta}>
          <Text
            variant="labelSmall"
            style={[
              styles.time,
              isOwn ? styles.timeOwn : styles.timeOther,
            ]}
          >
            {timeLabel}
          </Text>

          {isSending && (
            <View style={styles.statusIcon}>
              <Icon
                source="clock-outline"
                size={12}
                color={isOwn ? "rgba(255,255,255,0.6)" : "#938F99"}
              />
            </View>
          )}

          {isFailed && (
            <View style={styles.statusIcon}>
              <Icon
                source="alert-circle-outline"
                size={12}
                color="#F2B8B5"
              />
            </View>
          )}
        </View>
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
    marginVertical: 2,
  },
  rowOwn: {
    alignItems: "flex-end",
  },
  rowOther: {
    alignItems: "flex-start",
  },
  bubble: {
    maxWidth: MAX_BUBBLE_WIDTH,
    borderRadius: 12,
    paddingHorizontal: 12,
    paddingVertical: 8,
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
  meta: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "flex-end",
    marginTop: 4,
  },
  time: {
    fontSize: 12,
  },
  timeOwn: {
    color: "rgba(255,255,255,0.6)",
  },
  timeOther: {
    color: "#938F99",
  },
  statusIcon: {
    marginLeft: 4,
  },
  senderLabel: {
    fontSize: 12,
    fontWeight: "600",
    marginBottom: 2,
  },
});
