/**
 * Settings page -- user preferences.
 *
 * Currently supports:
 *   - Custom quick-reaction list (emoji or short text, max 4 chars each)
 *   - Logout
 */
import React, { useCallback, useEffect, useState } from "react";
import { View, StyleSheet, ScrollView, Pressable, Alert } from "react-native";
import { Text, TextInput, Button, IconButton, Divider } from "react-native-paper";
import { Stack, useRouter } from "expo-router";
import { useSettingsStore, DEFAULT_REACTIONS } from "../../src/store/settings";
import { useAuthStore } from "../../src/store/auth";
import { ScreenHeader } from "../../src/components/ScreenHeader";

const MAX_REACTION_CHARS = 4;
const MAX_REACTION_SLOTS = 6;

export default function SettingsScreen() {
  const router = useRouter();
  const logout = useAuthStore((s) => s.logout);
  const quickReactions = useSettingsStore((s) => s.quickReactions);
  const setQuickReactions = useSettingsStore((s) => s.setQuickReactions);

  // Local editable copy
  const [reactions, setReactions] = useState<string[]>([]);
  const [editingIndex, setEditingIndex] = useState<number | null>(null);
  const [editValue, setEditValue] = useState("");

  useEffect(() => {
    setReactions([...quickReactions]);
  }, [quickReactions]);

  const handleStartEdit = useCallback(
    (index: number) => {
      setEditingIndex(index);
      setEditValue(reactions[index]);
    },
    [reactions]
  );

  const handleFinishEdit = useCallback(() => {
    if (editingIndex === null) return;
    const trimmed = editValue.trim();
    if (trimmed.length === 0 || trimmed.length > MAX_REACTION_CHARS) {
      // Revert if invalid
      setEditingIndex(null);
      setEditValue("");
      return;
    }
    const updated = [...reactions];
    updated[editingIndex] = trimmed;
    setReactions(updated);
    setQuickReactions(updated);
    setEditingIndex(null);
    setEditValue("");
  }, [editingIndex, editValue, reactions, setQuickReactions]);

  const handleRemove = useCallback(
    (index: number) => {
      if (reactions.length <= 1) return; // keep at least one
      const updated = reactions.filter((_, i) => i !== index);
      setReactions(updated);
      setQuickReactions(updated);
    },
    [reactions, setQuickReactions]
  );

  const handleAdd = useCallback(() => {
    if (reactions.length >= MAX_REACTION_SLOTS) return;
    const updated = [...reactions, "👍"];
    setReactions(updated);
    setQuickReactions(updated);
    // Auto-edit the new item
    setEditingIndex(updated.length - 1);
    setEditValue("👍");
  }, [reactions, setQuickReactions]);

  const handleReset = useCallback(() => {
    setReactions([...DEFAULT_REACTIONS]);
    setQuickReactions([...DEFAULT_REACTIONS]);
    setEditingIndex(null);
  }, [setQuickReactions]);

  const handleLogout = useCallback(async () => {
    Alert.alert("Logout", "Are you sure you want to logout?", [
      { text: "Cancel", style: "cancel" },
      {
        text: "Logout",
        style: "destructive",
        onPress: async () => {
          await logout();
          router.replace("/login");
        },
      },
    ]);
  }, [logout, router]);

  return (
    <>
      <Stack.Screen
        options={{
          headerShown: false,
        }}
      />

      <View style={styles.container}>
        <ScreenHeader title="Settings" />
        <ScrollView style={styles.scrollView} contentContainerStyle={styles.content}>
          {/* Quick Reactions */}
          <Text variant="titleMedium" style={styles.sectionTitle}>
            Quick Reactions
          </Text>
          <Text variant="bodySmall" style={styles.sectionHint}>
            Customize the reactions shown on long-press. Supports emoji or short text (max{" "}
            {MAX_REACTION_CHARS} chars).
          </Text>

          <View style={styles.reactionsGrid}>
            {reactions.map((r, i) => (
              <View key={i} style={styles.reactionItem}>
                {editingIndex === i ? (
                  <TextInput
                    value={editValue}
                    onChangeText={(t) => {
                      if (t.length <= MAX_REACTION_CHARS) setEditValue(t);
                    }}
                    onBlur={handleFinishEdit}
                    onSubmitEditing={handleFinishEdit}
                    autoFocus
                    style={styles.reactionInput}
                    contentStyle={styles.reactionInputContent}
                    mode="outlined"
                    outlineColor="#49454F"
                    activeOutlineColor="#D0BCFF"
                    textColor="#E6E1E5"
                    maxLength={MAX_REACTION_CHARS}
                    dense
                  />
                ) : (
                  <Pressable style={styles.reactionBubble} onPress={() => handleStartEdit(i)}>
                    <Text style={styles.reactionText}>{r}</Text>
                  </Pressable>
                )}
                <IconButton
                  icon="close-circle"
                  size={16}
                  iconColor="#938F99"
                  style={styles.removeBtn}
                  onPress={() => handleRemove(i)}
                  disabled={reactions.length <= 1}
                />
              </View>
            ))}

            {reactions.length < MAX_REACTION_SLOTS && (
              <Pressable style={styles.addBubble} onPress={handleAdd}>
                <Text style={styles.addText}>+</Text>
              </Pressable>
            )}
          </View>

          <Button
            mode="text"
            onPress={handleReset}
            textColor="#938F99"
            compact
            style={styles.resetBtn}
          >
            Reset to defaults
          </Button>

          <Divider style={styles.divider} />

          {/* Logout */}
          <Button
            mode="outlined"
            onPress={handleLogout}
            textColor="#F2B8B5"
            style={styles.logoutBtn}
            icon="logout"
          >
            Logout
          </Button>
        </ScrollView>
      </View>
    </>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: "#1a1a2e",
  },
  scrollView: {
    flex: 1,
    backgroundColor: "#1a1a2e",
  },
  content: {
    padding: 20,
  },
  sectionTitle: {
    color: "#E6E1E5",
    fontWeight: "600",
    marginBottom: 4,
  },
  sectionHint: {
    color: "#938F99",
    marginBottom: 16,
  },
  reactionsGrid: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: 12,
    alignItems: "center",
  },
  reactionItem: {
    alignItems: "center",
  },
  reactionBubble: {
    width: 48,
    height: 48,
    borderRadius: 12,
    backgroundColor: "#2B2930",
    alignItems: "center",
    justifyContent: "center",
  },
  reactionText: {
    fontSize: 22,
    color: "#E6E1E5",
  },
  reactionInput: {
    width: 56,
    height: 48,
    backgroundColor: "#2B2930",
    textAlign: "center",
  },
  reactionInputContent: {
    textAlign: "center",
    fontSize: 18,
  },
  removeBtn: {
    margin: 0,
    marginTop: -4,
  },
  addBubble: {
    width: 48,
    height: 48,
    borderRadius: 12,
    borderWidth: 1,
    borderColor: "#49454F",
    borderStyle: "dashed",
    alignItems: "center",
    justifyContent: "center",
  },
  addText: {
    fontSize: 22,
    color: "#938F99",
  },
  resetBtn: {
    alignSelf: "flex-start",
    marginTop: 8,
  },
  divider: {
    backgroundColor: "#49454F",
    marginVertical: 24,
  },
  logoutBtn: {
    borderColor: "#F2B8B5",
  },
});
