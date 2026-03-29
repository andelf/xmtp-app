/**
 * Single conversation row for the conversations list.
 *
 * Renders avatar, title, last message preview, and relative timestamp
 * following Material Design 3 list item conventions.
 */
import React, { memo } from "react";
import { View, StyleSheet } from "react-native";
import { Avatar, Text, TouchableRipple } from "react-native-paper";

import type { ConversationItem } from "../store/conversations";
import { formatRelativeTime } from "../utils/time";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface ConversationListItemProps {
  /** The conversation data to render. */
  item: ConversationItem;
  /** Called when the user taps the row. */
  onPress: (item: ConversationItem) => void;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Extract initials for the avatar. For ETH addresses (0x...), use chars after the prefix. */
function getInitials(title: string): string {
  // ETH address: skip "0x" prefix, take next 2 hex chars
  if (title.startsWith("0x") && title.length > 4) {
    return title.slice(2, 4).toUpperCase();
  }
  const parts = title.trim().split(/\s+/);
  if (parts.length >= 2) {
    return (parts[0][0] + parts[1][0]).toUpperCase();
  }
  return title.slice(0, 2).toUpperCase();
}

/** Deterministic color from string hash -- produces a muted Material palette colour. */
const AVATAR_COLORS = [
  "#6750A4", // primary
  "#625B71", // secondary
  "#7D5260", // tertiary
  "#BA1A1A", // error
  "#006C4C", // green
  "#1B6D91", // blue
  "#8B5000", // amber
  "#4A6267", // teal
];

function colorForTitle(title: string): string {
  let hash = 0;
  for (let i = 0; i < title.length; i++) {
    hash = (hash * 31 + title.charCodeAt(i)) | 0;
  }
  return AVATAR_COLORS[Math.abs(hash) % AVATAR_COLORS.length];
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

function ConversationListItemInner({ item, onPress }: ConversationListItemProps) {
  const timestamp = item.lastMessageAt ?? item.createdAt;
  const timeLabel = formatRelativeTime(timestamp);
  const initials = getInitials(item.title);
  const bgColor = colorForTitle(item.title);

  return (
    <TouchableRipple onPress={() => onPress(item)} rippleColor="rgba(103,80,164,0.12)">
      <View style={styles.row}>
        {/* Avatar */}
        <Avatar.Text
          size={48}
          label={initials}
          style={[styles.avatar, { backgroundColor: bgColor }]}
          labelStyle={styles.avatarLabel}
        />

        {/* Middle: title + preview */}
        <View style={styles.body}>
          <Text variant="bodyLarge" numberOfLines={1} style={styles.title}>
            {item.title}
          </Text>
          <Text variant="bodyMedium" numberOfLines={1} style={styles.preview}>
            {item.lastMessageText ?? "No messages yet"}
          </Text>
        </View>

        {/* Right: timestamp + unread badge */}
        <View style={styles.rightCol}>
          <Text variant="labelSmall" style={styles.time}>
            {timeLabel}
          </Text>
          {item.unreadCount > 0 && (
            <View style={styles.badge}>
              <Text style={styles.badgeText}>
                {item.unreadCount > 99 ? "99+" : item.unreadCount}
              </Text>
            </View>
          )}
        </View>
      </View>
    </TouchableRipple>
  );
}

export const ConversationListItem = memo(ConversationListItemInner);

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

const styles = StyleSheet.create({
  row: {
    flexDirection: "row",
    alignItems: "center",
    paddingHorizontal: 16,
    paddingVertical: 12,
  },
  avatar: {
    marginRight: 16,
  },
  avatarLabel: {
    fontSize: 18,
    fontWeight: "600",
    color: "#ffffff",
  },
  body: {
    flex: 1,
    justifyContent: "center",
    marginRight: 12,
  },
  title: {
    color: "#E6E1E5",
    fontWeight: "500",
  },
  preview: {
    color: "#CAC4D0",
    marginTop: 2,
  },
  rightCol: {
    alignItems: "flex-end",
    gap: 4,
    marginTop: 2,
  },
  time: {
    color: "#938F99",
  },
  badge: {
    backgroundColor: "#6750A4",
    borderRadius: 10,
    minWidth: 20,
    height: 20,
    alignItems: "center",
    justifyContent: "center",
    paddingHorizontal: 5,
  },
  badgeText: {
    color: "#FFFFFF",
    fontSize: 11,
    fontWeight: "700",
  },
});
