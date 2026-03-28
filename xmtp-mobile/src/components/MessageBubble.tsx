/**
 * Message bubble with optional header (sender + time) above the bubble.
 *
 * Long-press shows a horizontal context menu:
 *   Row 1: Quick-react emoji bar (tap to send reaction — placeholder)
 *   Row 2: Copy | Reply action buttons
 */
import React, { memo, useState, useCallback, useRef } from "react";
import {
  View,
  StyleSheet,
  Dimensions,
  Pressable,
  Clipboard,
  Modal,
  TouchableWithoutFeedback,
} from "react-native";
import { Text, Icon } from "react-native-paper";

import type { MessageItem } from "../store/messages";
import { useMessageStore } from "../store/messages";
import { formatMessageTime } from "../utils/time";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface MessageBubbleProps {
  item: MessageItem;
  prevItem?: MessageItem | null;
  isGroup?: boolean;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SCREEN_WIDTH = Dimensions.get("window").width;
const MAX_BUBBLE_WIDTH = SCREEN_WIDTH * 0.75;
const GROUP_TIME_THRESHOLD = 2 * 60 * 1000;

const SENDER_COLORS = [
  "#BB86FC", "#03DAC6", "#CF6679", "#FFAB40",
  "#69F0AE", "#40C4FF", "#FF8A65", "#B388FF",
];

const QUICK_EMOJIS = ["👍", "❤️", "😂", "🔥", "👀", "🙏"];
const MENU_WIDTH = 240;

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

function shouldShowHeader(
  item: MessageItem,
  prevItem: MessageItem | null | undefined,
): boolean {
  if (!prevItem) return true;
  if (prevItem.senderInboxId !== item.senderInboxId) return true;
  if (Math.abs(item.sentAt - prevItem.sentAt) > GROUP_TIME_THRESHOLD) return true;
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

  // Context menu state
  const [menuVisible, setMenuVisible] = useState(false);
  const [menuPosition, setMenuPosition] = useState({ x: 0, y: 0 });
  const bubbleRef = useRef<View>(null);

  const handleLongPress = useCallback(() => {
    bubbleRef.current?.measureInWindow((x, y, _width, _height) => {
      // Center menu horizontally over bubble, clamp to screen edges
      let left = isOwn ? x + _width - MENU_WIDTH : x;
      left = Math.max(8, Math.min(left, SCREEN_WIDTH - MENU_WIDTH - 8));
      setMenuPosition({ x: left, y: y - 90 });
      setMenuVisible(true);
    });
  }, [isOwn]);

  const closeMenu = useCallback(() => setMenuVisible(false), []);

  const handleCopy = useCallback(() => {
    Clipboard.setString(item.text);
    setMenuVisible(false);
  }, [item.text]);

  const handleReaction = useCallback((_emoji: string) => {
    // TODO: send reaction via XMTP
    setMenuVisible(false);
  }, []);

  const handleReply = useCallback(() => {
    // TODO: set reply context in input bar
    setMenuVisible(false);
  }, []);

  // Resolve reply reference text from store
  let replyText: string | undefined;
  if (item.replyRef) {
    const msgs = useMessageStore.getState().getMessages(item.conversationId);
    const refId = item.replyRef.referenceMessageId;
    replyText = msgs.find((m) => (m.id as string) === refId)?.text;
  }

  return (
    <View style={[styles.row, isOwn ? styles.rowOwn : styles.rowOther]}>
      {/* Header: sender + time */}
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
          {isSending && <Icon source="clock-outline" size={11} color="#938F99" />}
          {isFailed && <Icon source="alert-circle-outline" size={11} color="#F2B8B5" />}
        </View>
      )}

      {/* Reply quote — above the bubble */}
      {item.replyRef && (
        <View style={[styles.replyBar, isOwn ? styles.replyBarOwn : styles.replyBarOther]}>
          <Icon source="reply" size={12} color="#938F99" />
          <Text variant="labelSmall" numberOfLines={1} style={styles.replyText}>
            {replyText ?? item.replyRef.referenceText ?? "..."}
          </Text>
        </View>
      )}

      {/* Bubble — long-pressable */}
      <Pressable onLongPress={handleLongPress} delayLongPress={300}>
        <View
          ref={bubbleRef}
          style={[
            styles.bubble,
            isOwn ? styles.bubbleOwn : styles.bubbleOther,
            !showHeader && !item.replyRef && styles.bubbleGrouped,
          ]}
        >
          <Text variant="bodyMedium" style={isOwn ? styles.textOwn : styles.textOther}>
            {item.text}
          </Text>
        </View>
      </Pressable>

      {/* Reaction badges */}
      {item.reactions && Object.keys(item.reactions).length > 0 && (
        <View style={[styles.reactionsRow, isOwn ? styles.reactionsOwn : styles.reactionsOther]}>
          {Object.entries(item.reactions).map(([emoji, senders]) => (
            <View key={emoji} style={styles.reactionBadge}>
              <Text style={styles.reactionEmoji}>{emoji}</Text>
              {senders.length > 1 && (
                <Text style={styles.reactionCount}>{senders.length}</Text>
              )}
            </View>
          ))}
        </View>
      )}

      {/* Context menu */}
      <Modal visible={menuVisible} transparent animationType="fade" onRequestClose={closeMenu}>
        <TouchableWithoutFeedback onPress={closeMenu}>
          <View style={styles.menuOverlay}>
            <TouchableWithoutFeedback>
              <View style={[styles.menuContainer, { left: menuPosition.x, top: menuPosition.y }]}>
                {/* Emoji quick-react row */}
                <View style={styles.emojiRow}>
                  {QUICK_EMOJIS.map((emoji) => (
                    <Pressable
                      key={emoji}
                      style={({ pressed }) => [styles.emojiBtn, pressed && styles.emojiBtnPressed]}
                      onPress={() => handleReaction(emoji)}
                    >
                      <Text style={styles.emojiText}>{emoji}</Text>
                    </Pressable>
                  ))}
                </View>

                {/* Divider */}
                <View style={styles.menuDivider} />

                {/* Action row */}
                <View style={styles.actionRow}>
                  <Pressable
                    style={({ pressed }) => [styles.actionBtn, pressed && styles.actionBtnPressed]}
                    onPress={handleCopy}
                  >
                    <Icon source="content-copy" size={16} color="#E6E1E5" />
                    <Text style={styles.actionLabel}>Copy</Text>
                  </Pressable>

                  <View style={styles.actionDivider} />

                  <Pressable
                    style={({ pressed }) => [styles.actionBtn, pressed && styles.actionBtnPressed]}
                    onPress={handleReply}
                  >
                    <Icon source="reply" size={16} color="#E6E1E5" />
                    <Text style={styles.actionLabel}>Reply</Text>
                  </Pressable>
                </View>
              </View>
            </TouchableWithoutFeedback>
          </View>
        </TouchableWithoutFeedback>
      </Modal>
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
  // Reaction badges
  reactionsRow: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: 4,
    marginTop: 2,
    paddingHorizontal: 4,
  },
  reactionsOwn: {
    justifyContent: "flex-end",
  },
  reactionsOther: {
    justifyContent: "flex-start",
  },
  reactionBadge: {
    flexDirection: "row",
    alignItems: "center",
    backgroundColor: "rgba(255,255,255,0.1)",
    borderRadius: 10,
    paddingHorizontal: 6,
    paddingVertical: 2,
    gap: 2,
  },
  reactionEmoji: {
    fontSize: 14,
  },
  reactionCount: {
    fontSize: 11,
    color: "#CAC4D0",
  },
  // Context menu
  menuOverlay: {
    flex: 1,
  },
  menuContainer: {
    position: "absolute",
    width: MENU_WIDTH,
    backgroundColor: "#2B2930",
    borderRadius: 14,
    paddingVertical: 6,
    elevation: 8,
    shadowColor: "#000",
    shadowOffset: { width: 0, height: 4 },
    shadowOpacity: 0.3,
    shadowRadius: 8,
  },
  emojiRow: {
    flexDirection: "row",
    justifyContent: "space-evenly",
    paddingHorizontal: 6,
    paddingVertical: 4,
  },
  emojiBtn: {
    width: 34,
    height: 34,
    borderRadius: 17,
    alignItems: "center",
    justifyContent: "center",
  },
  emojiBtnPressed: {
    backgroundColor: "rgba(255,255,255,0.12)",
  },
  emojiText: {
    fontSize: 20,
  },
  menuDivider: {
    height: StyleSheet.hairlineWidth,
    backgroundColor: "rgba(255,255,255,0.12)",
    marginHorizontal: 10,
    marginVertical: 4,
  },
  actionRow: {
    flexDirection: "row",
    alignItems: "center",
  },
  actionBtn: {
    flex: 1,
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    gap: 6,
    paddingVertical: 8,
  },
  actionBtnPressed: {
    backgroundColor: "rgba(255,255,255,0.08)",
  },
  actionDivider: {
    width: StyleSheet.hairlineWidth,
    height: 20,
    backgroundColor: "rgba(255,255,255,0.12)",
  },
  actionLabel: {
    color: "#E6E1E5",
    fontSize: 13,
  },
});
