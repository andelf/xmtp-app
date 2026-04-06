/**
 * Renders Coinbase Actions as a vertical list of tappable buttons inside a message bubble.
 *
 * States:
 * 1. No selection: all buttons visible, tappable
 * 2. Sending (optimistic): only selected button visible with loading indicator
 * 3. Confirmed (respondedActionId set): only selected button with ✓ checkmark
 */
import React, { memo, useCallback, useState } from "react";
import { View, StyleSheet, Pressable, ActivityIndicator } from "react-native";
import { Text } from "react-native-paper";

import type { ActionsPayload } from "../content/types";
import { sendIntent } from "../xmtp/messages";

interface ActionButtonsProps {
  conversationId: string;
  payload: ActionsPayload;
  /** If set, the intent has been confirmed via stream — show as completed. */
  respondedActionId?: string;
}

function ActionButtonsInner({ conversationId, payload, respondedActionId }: ActionButtonsProps) {
  const [pendingId, setPendingId] = useState<string | null>(null);
  const confirmed = respondedActionId != null;
  const selectedId = respondedActionId ?? pendingId;
  const busy = pendingId != null && !confirmed;

  const handlePress = useCallback(
    async (actionId: string) => {
      if (busy || confirmed) return;
      setPendingId(actionId);
      const ok = await sendIntent(conversationId, payload.id, actionId);
      if (!ok) setPendingId(null); // reset on failure
    },
    [conversationId, payload.id, busy, confirmed]
  );

  // After selection (pending or confirmed), show only the selected action
  const visibleActions = selectedId != null
    ? payload.actions.filter((a) => a.id === selectedId)
    : payload.actions;

  return (
    <View style={styles.container}>
      <Text variant="bodyMedium" style={styles.description}>
        {payload.description}
      </Text>
      {visibleActions.map((action) => {
        const isSelected = selectedId === action.id;
        const btnStyle = isSelected && confirmed ? styles.btnConfirmed
          : action.style === "danger" ? styles.btnDanger
          : action.style === "primary" ? styles.btnPrimary
          : styles.btnDefault;
        const textStyle = isSelected && confirmed ? styles.textConfirmed
          : action.style === "danger" ? styles.textDanger
          : action.style === "primary" ? styles.textPrimary
          : styles.textDefault;

        return (
          <Pressable
            key={action.id}
            onPress={() => handlePress(action.id)}
            disabled={busy || confirmed}
            style={[styles.btn, btnStyle]}
          >
            <View style={styles.btnContent}>
              {busy && isSelected && (
                <ActivityIndicator size="small" color="#938F99" style={styles.spinner} />
              )}
              <Text style={[styles.btnText, textStyle]}>
                {confirmed && isSelected ? `✓ ${action.label}` : action.label}
              </Text>
            </View>
          </Pressable>
        );
      })}
    </View>
  );
}

export const ActionButtons = memo(ActionButtonsInner);

const styles = StyleSheet.create({
  container: {
    gap: 6,
  },
  description: {
    color: "#E6E1E5",
    marginBottom: 4,
  },
  btn: {
    paddingVertical: 8,
    paddingHorizontal: 14,
    borderRadius: 8,
    borderWidth: 1,
    alignItems: "center",
  },
  btnContent: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    gap: 6,
  },
  btnPrimary: {
    borderColor: "#64B5F6",
    backgroundColor: "rgba(100, 181, 246, 0.15)",
  },
  btnDanger: {
    borderColor: "#EF5350",
    backgroundColor: "rgba(239, 83, 80, 0.12)",
  },
  btnDefault: {
    borderColor: "#938F99",
    backgroundColor: "rgba(147, 143, 153, 0.1)",
  },
  btnConfirmed: {
    borderColor: "#4CAF50",
    backgroundColor: "rgba(76, 175, 80, 0.25)",
  },
  spinner: {
    marginRight: 2,
  },
  btnText: {
    fontSize: 14,
    fontWeight: "500",
  },
  textPrimary: {
    color: "#64B5F6",
  },
  textDanger: {
    color: "#EF5350",
  },
  textDefault: {
    color: "#E6E1E5",
  },
  textConfirmed: {
    color: "#FFFFFF",
  },
});
