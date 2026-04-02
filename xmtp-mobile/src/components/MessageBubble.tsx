/**
 * Message bubble with optional header (sender + time) above the bubble.
 *
 * Long-press shows a horizontal context menu:
 *   Row 1: Quick-react emoji bar (tap to send reaction — placeholder)
 *   Row 2: Copy | Reply action buttons
 */
import React, { memo, useState, useCallback, useEffect } from "react";
import {
  View,
  StyleSheet,
  Dimensions,
  Pressable,
  Clipboard,
  Modal,
  TouchableWithoutFeedback,
  Linking,
} from "react-native";
import { Text, Icon } from "react-native-paper";
import { EnrichedMarkdownText, type MarkdownStyle } from "react-native-enriched-markdown";

import type { MessageItem } from "../store/messages";
import { sendReaction } from "../xmtp/messages";
import { useSettingsStore } from "../store/settings";
import { formatMessageTime } from "../utils/time";
import { getCachedAddress, resolveAddress } from "../utils/addressLookup";

// Dark theme markdown styles
const MD_TABLE_COMMON = {
  fontSize: 12,
  cellPaddingHorizontal: 4,
  cellPaddingVertical: 3,
  borderWidth: 1,
  borderRadius: 4,
};

const MD_STYLE_OWN: MarkdownStyle = {
  paragraph: { color: "#FFFFFF", fontSize: 14, marginBottom: 0, marginTop: 0 },
  h1: { color: "#FFFFFF", fontSize: 20, fontWeight: "700", marginBottom: 4, marginTop: 4 },
  h2: { color: "#FFFFFF", fontSize: 18, fontWeight: "700", marginBottom: 4, marginTop: 4 },
  h3: { color: "#FFFFFF", fontSize: 16, fontWeight: "600", marginBottom: 2, marginTop: 2 },
  h4: { color: "#FFFFFF", fontSize: 15, fontWeight: "600", marginBottom: 2, marginTop: 2 },
  h5: { color: "#FFFFFF", fontSize: 14, fontWeight: "600" },
  h6: { color: "#FFFFFF", fontSize: 14, fontWeight: "600" },
  strong: { color: "#FFFFFF" },
  em: { color: "#FFFFFF" },
  link: { color: "#D0BCFF", underline: true },
  code: {
    color: "#D0BCFF",
    backgroundColor: "rgba(0,0,0,0.2)",
    fontSize: 13,
    borderColor: "transparent",
  },
  codeBlock: {
    color: "#E6E1E5",
    backgroundColor: "rgba(0,0,0,0.25)",
    borderRadius: 6,
    padding: 8,
    fontSize: 13,
  },
  blockquote: {
    borderColor: "#D0BCFF",
    backgroundColor: "rgba(0,0,0,0.15)",
    color: "#E6E1E5",
    fontSize: 14,
  },
  list: { color: "#FFFFFF", bulletColor: "#D0BCFF", fontSize: 14 },
  table: {
    ...MD_TABLE_COMMON,
    color: "#FFFFFF",
    headerBackgroundColor: "rgba(0,0,0,0.2)",
    headerTextColor: "#FFFFFF",
    borderColor: "rgba(255,255,255,0.2)",
    rowEvenBackgroundColor: "rgba(0,0,0,0.1)",
    rowOddBackgroundColor: "transparent",
  },
};

const MD_STYLE_OTHER: MarkdownStyle = {
  paragraph: { color: "#E6E1E5", fontSize: 14, marginBottom: 0, marginTop: 0 },
  h1: { color: "#E6E1E5", fontSize: 20, fontWeight: "700", marginBottom: 4, marginTop: 4 },
  h2: { color: "#E6E1E5", fontSize: 18, fontWeight: "700", marginBottom: 4, marginTop: 4 },
  h3: { color: "#E6E1E5", fontSize: 16, fontWeight: "600", marginBottom: 2, marginTop: 2 },
  h4: { color: "#E6E1E5", fontSize: 15, fontWeight: "600", marginBottom: 2, marginTop: 2 },
  h5: { color: "#E6E1E5", fontSize: 14, fontWeight: "600" },
  h6: { color: "#E6E1E5", fontSize: 14, fontWeight: "600" },
  strong: { color: "#E6E1E5" },
  em: { color: "#E6E1E5" },
  link: { color: "#D0BCFF", underline: true },
  code: {
    color: "#D0BCFF",
    backgroundColor: "rgba(255,255,255,0.08)",
    fontSize: 13,
    borderColor: "transparent",
  },
  codeBlock: {
    color: "#E6E1E5",
    backgroundColor: "rgba(255,255,255,0.06)",
    borderRadius: 6,
    padding: 8,
    fontSize: 13,
  },
  blockquote: {
    borderColor: "#BB86FC",
    backgroundColor: "rgba(255,255,255,0.05)",
    color: "#CAC4D0",
    fontSize: 14,
  },
  list: { color: "#E6E1E5", bulletColor: "#BB86FC", fontSize: 14 },
  table: {
    ...MD_TABLE_COMMON,
    color: "#E6E1E5",
    headerBackgroundColor: "rgba(255,255,255,0.1)",
    headerTextColor: "#E6E1E5",
    borderColor: "rgba(255,255,255,0.15)",
    rowEvenBackgroundColor: "rgba(255,255,255,0.04)",
    rowOddBackgroundColor: "transparent",
  },
};

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface MessageBubbleProps {
  item: MessageItem;
  prevItem?: MessageItem | null;
  isGroup?: boolean;
  /** Called when user taps Reply in context menu */
  onReply?: (item: MessageItem) => void;
  /** Called when user taps Retry on a failed message */
  onRetry?: (item: MessageItem) => void;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SCREEN_WIDTH = Dimensions.get("window").width;
const MAX_BUBBLE_WIDTH = SCREEN_WIDTH * 0.75;
const GROUP_TIME_THRESHOLD = 2 * 60 * 1000;

const SENDER_COLORS = [
  "#BB86FC",
  "#03DAC6",
  "#CF6679",
  "#FFAB40",
  "#69F0AE",
  "#40C4FF",
  "#FF8A65",
  "#B388FF",
];

const MENU_WIDTH = 240;

function senderColor(inboxId: string): string {
  let hash = 0;
  for (let i = 0; i < inboxId.length; i++) {
    hash = (hash * 31 + inboxId.charCodeAt(i)) | 0;
  }
  return SENDER_COLORS[Math.abs(hash) % SENDER_COLORS.length];
}

function senderLabel(address: string): string {
  if (address.startsWith("0x") && address.length > 10) {
    return address.slice(0, 6) + "..." + address.slice(-4);
  }
  return address.slice(0, 8) + "...";
}

/** Resolve inboxId to address, using cache-first with async fallback. */
function useSenderAddress(inboxId: string): string {
  const [address, setAddress] = useState(() => getCachedAddress(inboxId) ?? inboxId);

  useEffect(() => {
    if (getCachedAddress(inboxId)) {
      setAddress(getCachedAddress(inboxId)!);
      return;
    }
    let cancelled = false;
    resolveAddress(inboxId).then((resolved) => {
      if (!cancelled) setAddress(resolved);
    });
    return () => {
      cancelled = true;
    };
  }, [inboxId]);

  return address;
}

function shouldShowHeader(item: MessageItem, prevItem: MessageItem | null | undefined): boolean {
  if (!prevItem) return true;
  if (prevItem.senderInboxId !== item.senderInboxId) return true;
  if (Math.abs(item.sentAt - prevItem.sentAt) > GROUP_TIME_THRESHOLD) return true;
  return false;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

function MessageBubbleInner({ item, prevItem, isGroup = false, onReply, onRetry }: MessageBubbleProps) {
  const quickReactions = useSettingsStore((s) => s.quickReactions);
  const isOwn = item.isOwn;
  const showHeader = shouldShowHeader(item, prevItem);
  const timeLabel = formatMessageTime(item.sentAt);
  const isSending = item.status === "sending";
  const isFailed = item.status === "failed";

  const isMarkdown = item.contentType?.includes("markdown") === true;

  // Resolve sender inboxId to Ethereum address for display
  const senderAddress = useSenderAddress(item.senderInboxId);

  // Context menu state
  const [menuVisible, setMenuVisible] = useState(false);
  const [menuPosition, setMenuPosition] = useState({ x: 0, y: 0 });

  const handleLongPress = useCallback(
    (e: any) => {
      const pageX = e?.nativeEvent?.pageX ?? 0;
      const pageY = e?.nativeEvent?.pageY ?? 0;
      // Position menu above the touch point, clamped to screen edges
      let left = isOwn ? pageX - MENU_WIDTH + 20 : pageX - 20;
      left = Math.max(8, Math.min(left, SCREEN_WIDTH - MENU_WIDTH - 8));
      const top = Math.max(8, pageY - 120);
      setMenuPosition({ x: left, y: top });
      setMenuVisible(true);
    },
    [isOwn]
  );

  const closeMenu = useCallback(() => setMenuVisible(false), []);

  const handleCopy = useCallback(() => {
    Clipboard.setString(item.text);
    setMenuVisible(false);
  }, [item.text]);

  const handleReaction = useCallback(
    (emoji: string) => {
      setMenuVisible(false);
      sendReaction(item.conversationId, item.id as string, emoji);
    },
    [item.conversationId, item.id]
  );

  const handleReply = useCallback(() => {
    setMenuVisible(false);
    onReply?.(item);
  }, [onReply, item]);

  return (
    <View style={[styles.row, isOwn ? styles.rowOwn : styles.rowOther]}>
      {/* Header: sender + time */}
      {showHeader && (
        <View style={[styles.header, isOwn ? styles.headerOwn : styles.headerOther]}>
          {isGroup && !isOwn && (
            <Text
              variant="labelSmall"
              style={[styles.senderText, { color: senderColor(senderAddress) }]}
              numberOfLines={1}
            >
              {senderLabel(senderAddress)}
            </Text>
          )}
          <Text variant="labelSmall" style={styles.headerTime}>
            {timeLabel}
          </Text>
          {isSending && <Icon source="clock-outline" size={11} color="#938F99" />}
          {isFailed && (
            <Pressable onPress={() => onRetry?.(item)} style={styles.retryBtn}>
              <Icon source="alert-circle-outline" size={11} color="#F2B8B5" />
              <Text style={styles.retryText}>Retry</Text>
            </Pressable>
          )}
          {isOwn && !isSending && !isFailed && item.status === "published" && (
            <Icon source="circle-outline" size={10} color="#938F99" />
          )}
          {isOwn && !isSending && !isFailed && item.status === "read" && (
            <Icon source="circle-slice-8" size={10} color="#4CAF50" />
          )}
        </View>
      )}

      {/* Reply quote — above the bubble */}
      {item.replyRef && (
        <View style={[styles.replyBar, isOwn ? styles.replyBarOwn : styles.replyBarOther]}>
          <Icon source="reply" size={12} color="#938F99" />
          <Text variant="labelSmall" numberOfLines={1} style={styles.replyText}>
            {item.replyRef.referenceText ?? "..."}
          </Text>
        </View>
      )}

      {/* Bubble — long-pressable */}
      <Pressable onLongPress={handleLongPress} delayLongPress={300}>
        <View
          style={[
            styles.bubble,
            isOwn ? styles.bubbleOwn : styles.bubbleOther,
            !showHeader && !item.replyRef && styles.bubbleGrouped,
            isMarkdown && styles.bubbleMarkdown,
          ]}
        >
          {isMarkdown ? (
            <EnrichedMarkdownText
              markdown={item.text}
              markdownStyle={isOwn ? MD_STYLE_OWN : MD_STYLE_OTHER}
              onLinkPress={(e) => Linking.openURL(e.url)}
              allowTrailingMargin={false}
              flavor="github"
            />
          ) : (
            <Text variant="bodyMedium" style={isOwn ? styles.textOwn : styles.textOther}>
              {item.text}
            </Text>
          )}
        </View>
      </Pressable>

      {/* Reaction badges */}
      {item.reactions && Object.keys(item.reactions).length > 0 && (
        <View style={[styles.reactionsRow, isOwn ? styles.reactionsOwn : styles.reactionsOther]}>
          {Object.entries(item.reactions).map(([emoji, senders]) => (
            <View key={emoji} style={styles.reactionBadge}>
              <Text style={styles.reactionEmoji}>{emoji}</Text>
              {senders.length > 1 && <Text style={styles.reactionCount}>{senders.length}</Text>}
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
                  {quickReactions.map((emoji) => (
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
  retryBtn: {
    flexDirection: "row",
    alignItems: "center",
    gap: 3,
  },
  retryText: {
    fontSize: 11,
    color: "#F2B8B5",
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
  bubbleMarkdown: {
    maxWidth: SCREEN_WIDTH - 32,
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
    minWidth: 34,
    height: 34,
    borderRadius: 17,
    alignItems: "center",
    justifyContent: "center",
    paddingHorizontal: 4,
  },
  emojiBtnPressed: {
    backgroundColor: "rgba(255,255,255,0.12)",
  },
  emojiText: {
    fontSize: 18,
    color: "#E6E1E5",
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
