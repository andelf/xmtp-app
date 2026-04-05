/**
 * Conversations list page -- displays all DM and Group conversations.
 */
import React, { useCallback, useMemo, useState } from "react";
import { View, StyleSheet, RefreshControl, ActivityIndicator } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { Appbar, Menu, Text, Button, Divider } from "react-native-paper";
import { FlashList, type ListRenderItem } from "@shopify/flash-list";
import { useRouter } from "expo-router";

import { useAuthStore } from "../../src/store/auth";
import { useConversationStore } from "../../src/store/conversations";
import type { ConversationItem } from "../../src/store/conversations";
import { ConversationListItem } from "../../src/components/ConversationListItem";
import { ConversationListSkeleton } from "../../src/components/ConversationListSkeleton";
import { shortenAddress } from "../../src/utils/address";

// ---------------------------------------------------------------------------
// Mock data (will be replaced by store data once Coder-1 wires SDK calls)
// ---------------------------------------------------------------------------

const MOCK_CONVERSATIONS: ConversationItem[] = [
  {
    id: "conv-1" as any,
    topic: "/xmtp/0/dm-abc" as any,
    kind: "dm",
    title: shortenAddress("0xABCD1234567890ABCDEF1234567890ABCDEF1234"),
    lastMessageText: "Hey, are you available for a quick call?",
    lastMessageAt: Date.now() - 30 * 1000, // 30 seconds ago
    createdAt: Date.now() - 7 * 24 * 60 * 60 * 1000,
    unreadCount: 0,
  },
  {
    id: "conv-2" as any,
    topic: "/xmtp/0/group-xyz" as any,
    kind: "group",
    title: "XMTP Builders",
    lastMessageText: "The new SDK version looks great!",
    lastMessageAt: Date.now() - 45 * 60 * 1000, // 45 minutes ago
    createdAt: Date.now() - 14 * 24 * 60 * 60 * 1000,
    unreadCount: 0,
  },
  {
    id: "conv-3" as any,
    topic: "/xmtp/0/dm-def" as any,
    kind: "dm",
    title: shortenAddress("0x9876FEDCBA0987654321FEDCBA0987654321FEDC"),
    lastMessageText: "Thanks for the update",
    lastMessageAt: Date.now() - 5 * 60 * 60 * 1000, // 5 hours ago
    createdAt: Date.now() - 3 * 24 * 60 * 60 * 1000,
    unreadCount: 0,
  },
  {
    id: "conv-4" as any,
    topic: "/xmtp/0/group-aaa" as any,
    kind: "group",
    title: "Agent Protocol Research",
    lastMessageText: "Let me share the latest findings from the A2A spec review...",
    lastMessageAt: Date.now() - 26 * 60 * 60 * 1000, // yesterday
    createdAt: Date.now() - 30 * 24 * 60 * 60 * 1000,
    unreadCount: 0,
  },
  {
    id: "conv-5" as any,
    topic: "/xmtp/0/dm-ghi" as any,
    kind: "dm",
    title: shortenAddress("0x1111222233334444555566667777888899990000"),
    lastMessageText: "Sent you the transaction hash",
    lastMessageAt: Date.now() - 5 * 24 * 60 * 60 * 1000, // 5 days ago
    createdAt: Date.now() - 60 * 24 * 60 * 60 * 1000,
    unreadCount: 0,
  },
];

// ---------------------------------------------------------------------------
// Screen
// ---------------------------------------------------------------------------

export default function ConversationsScreen() {
  const router = useRouter();

  // Subscribe to items Map and derive sorted list via useMemo.
  // This avoids creating a new array inside the zustand selector
  // which would cause FlashList to infinite-loop on re-renders.
  const items = useConversationStore((s) => s.items);
  const conversations = useMemo(() => {
    const arr = Array.from(items.values());
    arr.sort((a, b) => (b.lastMessageAt ?? b.createdAt) - (a.lastMessageAt ?? a.createdAt));
    return arr;
  }, [items]);
  const storeLoading = useConversationStore((s) => s.isLoading);
  const isSyncing = useConversationStore((s) => s.isSyncing);
  const fetchAll = useConversationStore((s) => s.fetchAll);
  const address = useAuthStore((s) => s.address);
  const inboxId = useAuthStore((s) => s.inboxId);
  const storeError = useConversationStore((s) => s.error);

  const insets = useSafeAreaInsets();
  const [refreshing, setRefreshing] = useState(false);
  const [menuVisible, setMenuVisible] = useState(false);

  // Pull-to-refresh handler
  const handleRefresh = useCallback(async () => {
    setRefreshing(true);
    try {
      await fetchAll();
    } finally {
      setRefreshing(false);
    }
  }, [fetchAll]);

  // Navigation handlers
  const handleConversationPress = useCallback(
    (item: ConversationItem) => {
      router.push(`/conversation/${item.id}` as any);
    },
    [router]
  );

  const handleNewConversation = useCallback(() => {
    router.push("/new-conversation" as any);
  }, [router]);

  const openMenu = useCallback(() => setMenuVisible(true), []);
  const closeMenu = useCallback(() => setMenuVisible(false), []);

  // Renderers
  const renderItem: ListRenderItem<ConversationItem> = useCallback(
    ({ item }) => <ConversationListItem item={item} onPress={handleConversationPress} />,
    [handleConversationPress]
  );

  const renderSeparator = useCallback(() => <Divider style={styles.divider} />, []);

  const renderEmpty = useCallback(
    () => (
      <View style={styles.emptyContainer}>
        <Text variant="titleMedium" style={styles.emptyTitle}>
          No conversations yet
        </Text>
        <Text variant="bodySmall" style={styles.emptySubtitle}>
          {address ? `Address: ${address.slice(0, 10)}...` : "Not connected"}
        </Text>
        <Text variant="bodySmall" style={styles.emptySubtitle}>
          {inboxId ? `Inbox: ${inboxId.slice(0, 10)}...` : ""}
        </Text>
        {storeError ? (
          <Text variant="bodySmall" style={{ color: "#F2B8B5", marginTop: 4 }}>
            Error: {storeError}
          </Text>
        ) : null}
        <Text variant="bodyMedium" style={[styles.emptySubtitle, { marginTop: 12 }]}>
          Start a new conversation to get going
        </Text>
        <Button
          mode="contained"
          onPress={handleNewConversation}
          style={styles.emptyButton}
          icon="plus"
        >
          New Conversation
        </Button>
      </View>
    ),
    [handleNewConversation]
  );

  return (
    <View style={styles.container}>
      {/* AppBar */}
      <Appbar.Header style={styles.appbar} elevated>
        <Appbar.Content title="Messages" titleStyle={styles.appbarTitle} />
        <Appbar.Action icon="plus" onPress={handleNewConversation} iconColor="#E6E1E5" />
        <Menu
          visible={menuVisible}
          onDismiss={closeMenu}
          anchorPosition="bottom"
          contentStyle={styles.menuContent}
          anchor={<Appbar.Action icon="dots-vertical" onPress={openMenu} iconColor="#E6E1E5" />}
        >
          <Menu.Item
            leadingIcon="account-group"
            title="New Group"
            titleStyle={styles.menuItemTitle}
            onPress={() => {
              closeMenu();
              router.push("/new-conversation?mode=group" as any);
            }}
          />
          <Menu.Item
            leadingIcon="cog"
            title="Settings"
            titleStyle={styles.menuItemTitle}
            onPress={() => {
              closeMenu();
              router.push("/settings" as any);
            }}
          />
          <Menu.Item
            leadingIcon="information"
            title="About"
            titleStyle={styles.menuItemTitle}
            onPress={() => {
              closeMenu();
              router.push("/about" as any);
            }}
          />
        </Menu>
      </Appbar.Header>

      {/* Syncing indicator */}
      {isSyncing && (
        <View style={styles.syncBar}>
          <ActivityIndicator size="small" color="#CAC4D0" />
          <Text variant="bodySmall" style={styles.syncText}>Syncing...</Text>
        </View>
      )}

      {/* Conversation list */}
      {storeLoading && conversations.length === 0 ? (
        <ConversationListSkeleton />
      ) : (
        <FlashList
          data={conversations}
          renderItem={renderItem}
          keyExtractor={(item) => item.id}
          ItemSeparatorComponent={renderSeparator}
          ListEmptyComponent={renderEmpty}
          contentContainerStyle={{ paddingBottom: insets.bottom }}
          refreshControl={
            <RefreshControl
              refreshing={refreshing}
              onRefresh={handleRefresh}
              tintColor="#CAC4D0"
              colors={["#6750A4"]}
              progressBackgroundColor="#1a1a2e"
            />
          }
        />
      )}
    </View>
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
  divider: {
    backgroundColor: "#49454F",
    marginLeft: 80, // align with text start (past avatar)
  },
  emptyContainer: {
    flex: 1,
    justifyContent: "center",
    alignItems: "center",
    paddingTop: 120,
    paddingHorizontal: 32,
  },
  emptyTitle: {
    color: "#E6E1E5",
    marginBottom: 8,
  },
  emptySubtitle: {
    color: "#938F99",
    textAlign: "center",
    marginBottom: 24,
  },
  emptyButton: {
    borderRadius: 20,
  },
  menuContent: {
    backgroundColor: "#2B2930",
  },
  menuItemTitle: {
    color: "#E6E1E5",
  },
  syncBar: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    paddingVertical: 6,
    backgroundColor: "#2B2930",
  },
  syncText: {
    color: "#CAC4D0",
    marginLeft: 8,
  },
});
