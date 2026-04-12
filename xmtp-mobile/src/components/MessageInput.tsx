/**
 * Message input bar -- TextInput with send button.
 * Optionally shows a reply preview above the input when replying to a message.
 */
import React, { memo, useCallback, useEffect, useRef } from "react";
import { View, StyleSheet, Pressable } from "react-native";
import { TextInput, IconButton, Text, Icon } from "react-native-paper";

import type { MessageItem } from "../store/messages";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface MessageInputProps {
  /** Current draft text for the active conversation. */
  value: string;
  /** Update draft text for the active conversation. */
  onChangeText: (text: string) => void;
  /** Called when the user taps the send button with non-empty text. */
  onSend: (text: string) => void;
  /** Disable the entire input (e.g. while reconnecting). */
  disabled?: boolean;
  /** Message being replied to (shows preview bar above input). */
  replyTo?: MessageItem | null;
  /** Called when user dismisses the reply preview. */
  onCancelReply?: () => void;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

function MessageInputInner({
  value,
  onChangeText,
  onSend,
  disabled = false,
  replyTo,
  onCancelReply,
}: MessageInputProps) {
  const inputRef = useRef<any>(null);

  const trimmed = value.trim();
  const canSend = trimmed.length > 0 && !disabled;

  // Focus input when reply context is set
  useEffect(() => {
    if (replyTo) {
      inputRef.current?.focus();
    }
  }, [replyTo]);

  const handleSend = useCallback(() => {
    if (!canSend) return;
    onSend(trimmed);
  }, [canSend, onSend, trimmed]);

  return (
    <View>
      {/* Reply preview bar */}
      {replyTo && (
        <View style={styles.replyPreview}>
          <Icon source="reply" size={14} color="#BB86FC" />
          <Text style={styles.replyPreviewText} numberOfLines={1}>
            {replyTo.text}
          </Text>
          <Pressable onPress={onCancelReply} hitSlop={8}>
            <Icon source="close" size={16} color="#938F99" />
          </Pressable>
        </View>
      )}

      <View style={styles.container}>
        <TextInput
          ref={inputRef}
          mode="outlined"
          placeholder="Message"
          placeholderTextColor="#938F99"
          value={value}
          onChangeText={onChangeText}
          multiline
          dense
          style={styles.input}
          contentStyle={styles.inputContent}
          outlineStyle={styles.outline}
          outlineColor="#49454F"
          activeOutlineColor="#6750A4"
          textColor="#E6E1E5"
          disabled={disabled}
          returnKeyType="default"
        />
        <IconButton
          icon="send"
          mode="contained"
          containerColor={canSend ? "#6750A4" : "#49454F"}
          iconColor={canSend ? "#FFFFFF" : "#938F99"}
          size={22}
          onPress={handleSend}
          disabled={!canSend}
          style={styles.sendButton}
        />
      </View>
    </View>
  );
}

export const MessageInput = memo(MessageInputInner);

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

const styles = StyleSheet.create({
  container: {
    flexDirection: "row",
    alignItems: "flex-end",
    paddingHorizontal: 8,
    paddingVertical: 6,
    backgroundColor: "#1a1a2e",
    borderTopWidth: StyleSheet.hairlineWidth,
    borderTopColor: "#49454F",
  },
  input: {
    flex: 1,
    maxHeight: 120,
    backgroundColor: "#2a2a3e",
    fontSize: 16,
  },
  inputContent: {
    paddingTop: 12,
    paddingBottom: 12,
  },
  outline: {
    borderRadius: 16,
  },
  sendButton: {
    marginLeft: 4,
    marginBottom: 4,
  },
  replyPreview: {
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
    paddingHorizontal: 14,
    paddingVertical: 8,
    backgroundColor: "#2B2930",
    borderTopWidth: StyleSheet.hairlineWidth,
    borderTopColor: "#49454F",
  },
  replyPreviewText: {
    flex: 1,
    color: "#CAC4D0",
    fontSize: 13,
    fontStyle: "italic",
  },
});
