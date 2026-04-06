/**
 * Add Member Screen -- input ETH addresses and add them to a group.
 *
 * Navigated from group-detail "Add Member" button.
 */
import React, { useCallback, useState } from "react";
import { View, StyleSheet, ScrollView, Alert } from "react-native";
import { Text, TextInput, Button, Chip, ActivityIndicator } from "react-native-paper";
import { Stack, useLocalSearchParams, useRouter } from "expo-router";

import { addMembers } from "../../../src/xmtp/groups";
import { isValidEthAddress, shortenAddress } from "../../../src/utils/address";

export default function AddMemberScreen() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const router = useRouter();

  const [input, setInput] = useState("");
  const [addresses, setAddresses] = useState<string[]>([]);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleAdd = useCallback(() => {
    const trimmed = input.trim();
    setError(null);

    if (!isValidEthAddress(trimmed)) {
      setError("Invalid ETH address (must be 0x + 40 hex chars)");
      return;
    }
    if (addresses.includes(trimmed.toLowerCase())) {
      setError("Address already added");
      return;
    }

    setAddresses((prev) => [...prev, trimmed.toLowerCase()]);
    setInput("");
  }, [input, addresses]);

  const handleRemoveAddress = useCallback((addr: string) => {
    setAddresses((prev) => prev.filter((a) => a !== addr));
  }, []);

  const handleConfirm = useCallback(async () => {
    if (!id || addresses.length === 0) return;

    setSaving(true);
    setError(null);

    const res = await addMembers(id, addresses);
    setSaving(false);

    if (!res.ok) {
      Alert.alert("Error", res.error);
      return;
    }

    router.back();
  }, [id, addresses, router]);

  return (
    <>
      <Stack.Screen
        options={{
          headerShown: true,
          title: "Add Members",
          headerStyle: { backgroundColor: "#1a1a2e" },
          headerTintColor: "#E6E1E5",
          headerTitleStyle: { fontWeight: "600", fontSize: 18 },
        }}
      />

      <ScrollView
        style={styles.container}
        contentContainerStyle={styles.content}
        keyboardShouldPersistTaps="handled"
      >
        <Text variant="bodyMedium" style={styles.hint}>
          Enter ETH addresses to add to the group.
        </Text>

        {/* Input row */}
        <View style={styles.inputRow}>
          <TextInput
            mode="outlined"
            value={input}
            onChangeText={setInput}
            placeholder="0x..."
            placeholderTextColor="#938F99"
            style={styles.input}
            textColor="#E6E1E5"
            outlineColor="#49454F"
            activeOutlineColor="#6750A4"
            autoCapitalize="none"
            autoCorrect={false}
            returnKeyType="done"
            onSubmitEditing={handleAdd}
          />
          <Button
            mode="contained"
            onPress={handleAdd}
            style={styles.addBtn}
            labelStyle={styles.addBtnLabel}
            disabled={!input.trim()}
          >
            Add
          </Button>
        </View>

        {error && (
          <Text variant="bodySmall" style={styles.errorText}>
            {error}
          </Text>
        )}

        {/* Pending addresses */}
        {addresses.length > 0 && (
          <View style={styles.chipContainer}>
            {addresses.map((addr) => (
              <Chip
                key={addr}
                onClose={() => handleRemoveAddress(addr)}
                closeIconAccessibilityLabel="Remove"
                style={styles.chip}
                textStyle={styles.chipText}
              >
                {shortenAddress(addr)}
              </Chip>
            ))}
          </View>
        )}

        {/* Confirm button */}
        <Button
          mode="contained"
          onPress={handleConfirm}
          disabled={addresses.length === 0 || saving}
          style={styles.confirmBtn}
          icon={saving ? undefined : "account-multiple-plus"}
        >
          {saving ? (
            <ActivityIndicator size={16} color="#E6E1E5" />
          ) : (
            `Add ${addresses.length} Member${addresses.length !== 1 ? "s" : ""}`
          )}
        </Button>
      </ScrollView>
    </>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: "#1a1a2e",
  },
  content: {
    padding: 20,
  },
  hint: {
    color: "#938F99",
    marginBottom: 16,
  },
  inputRow: {
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
    marginBottom: 8,
  },
  input: {
    flex: 1,
    backgroundColor: "#16213e",
    fontSize: 14,
  },
  addBtn: {
    borderRadius: 8,
    height: 48,
    justifyContent: "center",
  },
  addBtnLabel: {
    fontSize: 14,
  },
  errorText: {
    color: "#F2B8B5",
    marginBottom: 8,
  },
  chipContainer: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: 8,
    marginTop: 12,
    marginBottom: 20,
  },
  chip: {
    backgroundColor: "#2B2930",
  },
  chipText: {
    color: "#E6E1E5",
    fontSize: 13,
  },
  confirmBtn: {
    borderRadius: 12,
    marginTop: 12,
  },
});
