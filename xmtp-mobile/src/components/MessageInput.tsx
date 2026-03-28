/**
 * Message input bar -- TextInput with send button.
 *
 * Follows Material Design 3 styling. Send button is disabled when input is
 * empty. Input grows up to 4 lines / 120dp, minimum 56dp.
 */
import React, { memo, useCallback, useState } from "react";
import { View, StyleSheet } from "react-native";
import { TextInput, IconButton } from "react-native-paper";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface MessageInputProps {
  /** Called when the user taps the send button with non-empty text. */
  onSend: (text: string) => void;
  /** Disable the entire input (e.g. while reconnecting). */
  disabled?: boolean;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

function MessageInputInner({ onSend, disabled = false }: MessageInputProps) {
  const [text, setText] = useState("");

  const trimmed = text.trim();
  const canSend = trimmed.length > 0 && !disabled;

  const handleSend = useCallback(() => {
    if (!canSend) return;
    onSend(trimmed);
    setText("");
  }, [canSend, onSend, trimmed]);

  return (
    <View style={styles.container}>
      <TextInput
        mode="outlined"
        placeholder="Message"
        placeholderTextColor="#938F99"
        value={text}
        onChangeText={setText}
        multiline
        numberOfLines={1}
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
    minHeight: 44,
    backgroundColor: "#2a2a3e",
    fontSize: 16,
  },
  inputContent: {
    paddingTop: 10,
    paddingBottom: 10,
  },
  outline: {
    borderRadius: 22,
  },
  sendButton: {
    marginLeft: 4,
    marginBottom: 2,
  },
});
