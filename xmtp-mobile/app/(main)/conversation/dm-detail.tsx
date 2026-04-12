/**
 * DM Detail Screen -- displays peer address, inbox ID, conversation metadata.
 *
 * Accessible from the Chat Screen header info button for DM conversations.
 */
import React, { useEffect, useState } from "react";
import { View, StyleSheet, ScrollView } from "react-native";
import { Text, Divider, ActivityIndicator } from "react-native-paper";
import { Stack, useLocalSearchParams } from "expo-router";

import { useConversationStore } from "../../../src/store/conversations";
import { useAuthStore } from "../../../src/store/auth";
import { findConversation } from "../../../src/xmtp/messages";
import { InfoRow } from "../../../src/components/InfoRow";
import { ScreenHeader } from "../../../src/components/ScreenHeader";
import { formatDateTime } from "../../../src/utils/time";

export default function DmDetailScreen() {
  const { id } = useLocalSearchParams<{ id: string }>();

  const conversation = useConversationStore((s) => (id ? s.items.get(id) : undefined));
  const myInboxId = useAuthStore((s) => s.inboxId);

  const [peerAddress, setPeerAddress] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!id) return;

    let cancelled = false;

    (async () => {
      try {
        const convo = await findConversation(id);
        if (!convo || cancelled) return;

        const members = await convo.members();
        const peer = members.find((m: { inboxId: string }) => m.inboxId !== myInboxId);
        const addr = peer?.identities?.[0]?.identifier ?? "Unknown";

        if (!cancelled) setPeerAddress(addr);
      } catch (err: any) {
        if (!cancelled) setError(err?.message ?? "Failed to load member info");
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [id, myInboxId]);

  return (
    <>
      <Stack.Screen
        options={{
          headerShown: false,
        }}
      />

      <View style={styles.container}>
        <ScreenHeader title="DM Details" />
        <ScrollView style={styles.scrollView} contentContainerStyle={styles.content}>
        {/* Peer info */}
        <Text variant="titleMedium" style={styles.sectionTitle}>
          Peer
        </Text>

        {loading ? (
          <View style={styles.loadingContainer}>
            <ActivityIndicator size="small" color="#6750A4" />
            <Text variant="bodySmall" style={styles.loadingText}>
              Loading member info...
            </Text>
          </View>
        ) : error ? (
          <Text variant="bodySmall" style={styles.errorText}>
            {error}
          </Text>
        ) : (
          <InfoRow label="Peer Address" value={peerAddress} numberOfLines={3} />
        )}

        <InfoRow
          label="Peer Inbox ID"
          value={conversation?.peerInboxId ?? null}
          numberOfLines={3}
        />

        <Divider style={styles.divider} />

        {/* Conversation metadata */}
        <Text variant="titleMedium" style={styles.sectionTitle}>
          Conversation
        </Text>
        <InfoRow label="Conversation ID" value={id ?? null} numberOfLines={3} />
        <InfoRow label="Topic" value={(conversation?.topic as string) ?? null} numberOfLines={3} />
        <InfoRow
          label="Created At"
          value={conversation ? formatDateTime(conversation.createdAt) : null}
        />
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
    marginBottom: 12,
  },
  divider: {
    backgroundColor: "#49454F",
    marginVertical: 20,
  },
  loadingContainer: {
    flexDirection: "row",
    alignItems: "center",
    marginBottom: 16,
    gap: 8,
  },
  loadingText: {
    color: "#938F99",
  },
  errorText: {
    color: "#F2B8B5",
    marginBottom: 16,
  },
});
