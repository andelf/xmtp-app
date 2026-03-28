/**
 * Message bubble with optional header (sender + time) above the bubble.
 *
 * Header is shown when:
 * - It's the first message, OR
 * - Different sender from previous message, OR
 * - Same sender but >2 min gap from previous message
 *
 * Long-press on the bubble shows a context menu (Copy / Reaction / Reply).
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
  if (!prevItem) return true;
  if (prevItem.senderInboxId !== item.senderInboxId) return true;
  if (Math.abs(item.sentAt - prevItem.sentAt) > GROUP_TIME_THRESHOLD) return true;
  return false;
}

// ---------------------------------------------------------------------------
// Context menu items
// ---------------------------------------------------------------------------

interface MenuItem {
  label: string;
  icon: string;
  onPress: () => void;
  disabled?: boolean;
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
    bubbleRef.current?.measureInWindow((x, y, width, height) => {
      setMenuPosition({
        x: isOwn ? x + width - MENU_WIDTH : x,
        y: y - MENU_HEIGHT - 4,
      });
      setMenuVisible(true);
    });
  }, [isOwn]);

  const closeMenu = useCallback(() => setMenuVisible(false), []);

  const handleCopy = useCallback(() => {
    Clipboard.setString(item.text);
    setMenuVisible(false);
  }, [item.text]);

  const menuItems: MenuItem[] = [
    { label: "Copy", icon: "content-copy", onPress: handleCopy },
    { label: "React", icon: "emoticon-outline", onPress: closeMenu, disabled: true },
    { label: "Reply", icon: "reply", onPress: closeMenu, disabled: true },
  ];

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
          <Text
            variant="bodyMedium"
            style={isOwn ? styles.textOwn : styles.textOther}
          >
            {item.text}
          </Text>
        </View>
      </Pressable>

      {/* Context menu modal */}
      <Modal
        visible={menuVisible}
        transparent
        animationType="fade"
        onRequestClose={closeMenu}
      >
        <TouchableWithoutFeedback onPress={closeMenu}>
          <View style={styles.menuOverlay}>
            <View style={[styles.menuContainer, { left: menuPosition.x, top: menuPosition.y }]}>
              {menuItems.map((mi) => (
                <Pressable
                  key={mi.label}
                  style={({ pressed }) => [
                    styles.menuItem,
                    pressed && !mi.disabled && styles.menuItemPressed,
                    mi.disabled && styles.menuItemDisabled,
                  ]}
                  onPress={mi.disabled ? undefined : mi.onPress}
                >
                  <Icon source={mi.icon} size={16} color={mi.disabled ? "#5E5A5F" : "#E6E1E5"} />
                  <Text style={[styles.menuLabel, mi.disabled && styles.menuLabelDisabled]}>
                    {mi.label}
                  </Text>
                </Pressable>
              ))}
            </View>
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

const MENU_WIDTH = 140;
const MENU_HEIGHT = 3 * 40; // 3 items * ~40px each

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
  // Context menu
  menuOverlay: {
    flex: 1,
  },
  menuContainer: {
    position: "absolute",
    width: MENU_WIDTH,
    backgroundColor: "#2B2930",
    borderRadius: 12,
    paddingVertical: 4,
    elevation: 8,
    shadowColor: "#000",
    shadowOffset: { width: 0, height: 4 },
    shadowOpacity: 0.3,
    shadowRadius: 8,
  },
  menuItem: {
    flexDirection: "row",
    alignItems: "center",
    gap: 10,
    paddingHorizontal: 14,
    paddingVertical: 10,
  },
  menuItemPressed: {
    backgroundColor: "rgba(255,255,255,0.08)",
  },
  menuItemDisabled: {
    opacity: 0.4,
  },
  menuLabel: {
    color: "#E6E1E5",
    fontSize: 14,
  },
  menuLabelDisabled: {
    color: "#5E5A5F",
  },
});
