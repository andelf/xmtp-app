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
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SCREEN_WIDTH = Dimensions.get("window").width;
const MAX_BUBBLE_WIDTH = SCREEN_WIDTH * 0.75;

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

function MessageBubbleInner({ item }: MessageBubbleProps) {
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
});
