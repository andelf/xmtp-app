/**
 * Group Detail Screen -- displays group name, members, conversation metadata.
 *
 * Accessible from the Chat Screen header info button for group conversations.
 */
import React, { useCallback, useEffect, useState } from "react";
import { View, StyleSheet, ScrollView, Clipboard } from "react-native";
import { Text, Divider, ActivityIndicator } from "react-native-paper";
import { Stack, useLocalSearchParams } from "expo-router";

import { useConversationStore } from "../../../src/store/conversations";
import { findConversation } from "../../../src/xmtp/messages";
import { InfoRow } from "../../../src/components/InfoRow";
import { formatDateTime } from "../../../src/utils/time";

interface MemberInfo {
  inboxId: string;
  address: string;
}

export default function GroupDetailScreen() {
  const { id } = useLocalSearchParams<{ id: string }>();

  const conversation = useConversationStore((s) => (id ? s.items.get(id) : undefined));

  const [members, setMembers] = useState<MemberInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!id) return;

    let cancelled = false;

    (async () => {
      try {
        const convo = await findConversation(id);
        if (!convo || cancelled) return;

        const rawMembers = await convo.members();
        const parsed: MemberInfo[] = rawMembers.map(
          (m: { inboxId: string; identities?: { identifier?: string }[] }) => ({
            inboxId: m.inboxId,
            address: m.identities?.[0]?.identifier ?? m.inboxId,
          })
        );

        if (!cancelled) setMembers(parsed);
      } catch (err: any) {
        if (!cancelled) setError(err?.message ?? "Failed to load members");
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [id]);

  const handleCopyAddress = useCallback((addr: string) => {
    Clipboard.setString(addr);
  }, []);

  return (
    <>
      <Stack.Screen
        options={{
          headerShown: true,
          title: "Group Details",
          headerStyle: { backgroundColor: "#1a1a2e" },
          headerTintColor: "#E6E1E5",
          headerTitleStyle: { fontWeight: "600", fontSize: 18 },
        }}
      />

      <ScrollView style={styles.container} contentContainerStyle={styles.content}>
        {/* Group info */}
        <Text variant="titleMedium" style={styles.sectionTitle}>
          Group
        </Text>
        <InfoRow label="Group Name" value={conversation?.title ?? null} />

        <Divider style={styles.divider} />

        {/* Members */}
        <Text variant="titleMedium" style={styles.sectionTitle}>
          Members{!loading && ` (${members.length})`}
        </Text>

        {loading ? (
          <View style={styles.loadingContainer}>
            <ActivityIndicator size="small" color="#6750A4" />
            <Text variant="bodySmall" style={styles.loadingText}>
              Loading members...
            </Text>
          </View>
        ) : error ? (
          <Text variant="bodySmall" style={styles.errorText}>
            {error}
          </Text>
        ) : (
          members.map((member) => (
            <View key={member.inboxId} style={styles.memberRow}>
              <Text
                variant="bodyMedium"
                style={styles.memberAddress}
                selectable
                onPress={() => handleCopyAddress(member.address)}
                numberOfLines={2}
              >
                {member.address}
              </Text>
            </View>
          ))
        )}

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
  memberRow: {
    marginBottom: 12,
    paddingVertical: 8,
    paddingHorizontal: 12,
    backgroundColor: "#16213e",
    borderRadius: 8,
  },
  memberAddress: {
    color: "#E6E1E5",
  },
});
