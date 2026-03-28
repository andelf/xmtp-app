/**
 * New conversation page -- enter an ETH address to create a DM.
 */
import React, { useCallback, useState } from "react";
import {
  View,
  StyleSheet,
  KeyboardAvoidingView,
  Platform,
} from "react-native";
import {
  Appbar,
  TextInput,
  Button,
  Text,
  HelperText,
} from "react-native-paper";
import { useRouter } from "expo-router";

import { getClient } from "../../src/xmtp/client";
import {
  useConversationStore,
  conversationToItem,
} from "../../src/store/conversations";
import { PublicIdentity } from "@xmtp/react-native-sdk";

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

const ETH_ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/;

function validateAddress(addr: string): string | null {
  if (!addr) return "Address is required";
  if (!addr.startsWith("0x")) return "Address must start with 0x";
  if (addr.length !== 42) return "Address must be 42 characters";
  if (!ETH_ADDRESS_RE.test(addr)) return "Invalid Ethereum address";
  return null;
}

// ---------------------------------------------------------------------------
// Screen
// ---------------------------------------------------------------------------

export default function NewConversationScreen() {
  const router = useRouter();
  const upsert = useConversationStore((s) => s.upsert);

  const [address, setAddress] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Only show validation error after the user has interacted
  const [touched, setTouched] = useState(false);
  const validationError = touched ? validateAddress(address.trim()) : null;

  const handleCreate = useCallback(async () => {
    const trimmed = address.trim();
    const valErr = validateAddress(trimmed);
    setTouched(true);
    if (valErr) {
      setError(null); // clear API error; validation error shown via helper
      return;
    }

    const client = getClient();
    if (!client) {
      setError("XMTP client not initialised. Please log in again.");
      return;
    }

    setLoading(true);
    setError(null);

    try {
      // Check if the address is reachable on XMTP
      const peerIdentity = new PublicIdentity(trimmed, "ETHEREUM");
      const canMessageResult = await client.canMessage([peerIdentity]);
      const canReach = Object.values(canMessageResult)[0] ?? false;
      if (!canReach) {
        setError(
          "This address is not registered on the XMTP network. The recipient must activate XMTP first.",
        );
        setLoading(false);
        return;
      }

      // Create the DM conversation
      const dm = await client.conversations.findOrCreateDmWithIdentity(peerIdentity);

      // Convert to store item and upsert
      const item = await conversationToItem(dm, client.inboxId);
      upsert(item);

      // Navigate to the new conversation
      router.replace(`/conversation/${dm.id}` as any);
    } catch (err: any) {
      console.error("[NewConversation] create failed:", err);
      setError(err?.message ?? "Failed to create conversation");
    } finally {
      setLoading(false);
    }
  }, [address, upsert, router]);

  const handleBack = useCallback(() => {
    router.back();
  }, [router]);

  return (
    <KeyboardAvoidingView
      style={styles.container}
      behavior={Platform.OS === "ios" ? "padding" : undefined}
    >
      {/* AppBar */}
      <Appbar.Header style={styles.appbar} elevated>
        <Appbar.BackAction onPress={handleBack} iconColor="#E6E1E5" />
        <Appbar.Content
          title="New Conversation"
          titleStyle={styles.appbarTitle}
        />
      </Appbar.Header>

      <View style={styles.content}>
        <Text variant="bodyMedium" style={styles.description}>
          Enter an Ethereum address to start a direct message conversation.
        </Text>

        {/* Address input */}
        <TextInput
          label="ETH Address"
          placeholder="0x..."
          value={address}
          onChangeText={(text) => {
            setAddress(text);
            if (error) setError(null);
          }}
          onBlur={() => setTouched(true)}
          mode="outlined"
          autoCapitalize="none"
          autoCorrect={false}
          style={styles.input}
          contentStyle={styles.inputContent}
          outlineColor="#49454F"
          activeOutlineColor="#D0BCFF"
          textColor="#E6E1E5"
          placeholderTextColor="#938F99"
          disabled={loading}
          error={!!(validationError || error)}
        />

        {/* Validation helper text */}
        {validationError && (
          <HelperText type="error" visible style={styles.helperText}>
            {validationError}
          </HelperText>
        )}

        {/* API error */}
        {error && !validationError && (
          <HelperText type="error" visible style={styles.helperText}>
            {error}
          </HelperText>
        )}

        {/* Submit button */}
        <Button
          mode="contained"
          onPress={handleCreate}
          loading={loading}
          disabled={loading}
          style={styles.button}
          contentStyle={styles.buttonContent}
          labelStyle={styles.buttonLabel}
          icon={loading ? undefined : "message-plus"}
        >
          {loading ? "Creating..." : "Start Conversation"}
        </Button>
      </View>
    </KeyboardAvoidingView>
  );
}

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: "#1a1a2e",
  },
  appbar: {
    backgroundColor: "#1a1a2e",
  },
  appbarTitle: {
    color: "#E6E1E5",
    fontWeight: "700",
  },
  content: {
    flex: 1,
    paddingHorizontal: 24,
    paddingTop: 32,
  },
  description: {
    color: "#938F99",
    marginBottom: 24,
  },
  input: {
    backgroundColor: "#1a1a2e",
  },
  inputContent: {
    fontFamily: Platform.OS === "ios" ? "Menlo" : "monospace",
    fontSize: 14,
  },
  helperText: {
    paddingHorizontal: 0,
  },
  button: {
    marginTop: 24,
    borderRadius: 20,
  },
  buttonContent: {
    paddingVertical: 6,
  },
  buttonLabel: {
    fontSize: 16,
    fontWeight: "600",
  },
});
