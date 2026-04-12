/**
 * Conversation / chat screen -- message list with input bar.
 *
 * Uses FlashList in inverted mode so the newest messages appear at the bottom.
 * Keyboard avoidance uses react-native-keyboard-controller with
 * behavior="translate-with-padding" (purpose-built for chat screens).
 */
import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { View, StyleSheet, Pressable, Animated, useWindowDimensions } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import {
  KeyboardAwareScrollView,
  KeyboardStickyView,
  useKeyboardState,
} from "react-native-keyboard-controller";
import { ActivityIndicator, Appbar, Text } from "react-native-paper";
import { FlashList, type ListRenderItem } from "@shopify/flash-list";
import { useLocalSearchParams, Stack, useRouter } from "expo-router";
import type { ConversationId } from "@xmtp/react-native-sdk";

import { useConversationStore } from "../../../src/store/conversations";
import { useMessageStore } from "../../../src/store/messages";
import type { MessageItem } from "../../../src/store/messages";
import { sendMessage, sendReply, sendReadReceipt } from "../../../src/xmtp/messages";
import { useSettingsStore } from "../../../src/store/settings";
import { useMessages } from "../../../src/hooks/useMessages";
import { useAppState } from "../../../src/hooks/useAppState";
import { useNetworkState } from "../../../src/hooks/useNetworkState";
import { MessageBubble } from "../../../src/components/MessageBubble";
import { MessageInput } from "../../../src/components/MessageInput";
import { useDraftStore } from "../../../src/store/drafts";
import { shortenAddress } from "../../../src/utils/address";
import { resolveAddresses } from "../../../src/utils/addressLookup";

const EMPTY_MESSAGES: MessageItem[] = [];
export default function ConversationScreen() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const router = useRouter();
  const insets = useSafeAreaInsets();
  const isKeyboardVisible = useKeyboardState((state) => state.isVisible);
  const keyboardHeight = useKeyboardState((state) => state.height);
  const window = useWindowDimensions();

  const conversationId = id ? (id as unknown as ConversationId) : null;

  const readReceiptsEnabled = useSettingsStore((s) => s.readReceipts);

  // Resolve conversation from store (single lookup)
  const conversationsLoading = useConversationStore((s) => s.isLoading);
  const conversation = useConversationStore((s) => (id ? (s.items.get(id) ?? null) : null));
  const conversationTitle = conversation?.title ?? (id ? shortenAddress(id) : "Chat");
  const isGroup = conversation?.kind === "group";
  const [composerHeight, setComposerHeight] = useState(0);
  const keyboardLift = isKeyboardVisible ? Math.max(keyboardHeight - insets.bottom, 0) : 0;

  useEffect(() => {
    console.log("[FoldDebug][Conversation]", {
      screen: "conversation",
      id,
      windowWidth: window.width,
      windowHeight: window.height,
      insetTop: insets.top,
      insetBottom: insets.bottom,
      keyboardVisible: isKeyboardVisible,
      keyboardHeight,
      keyboardLift,
      composerHeight,
    });
  }, [
    composerHeight,
    id,
    insets.bottom,
    insets.top,
    isKeyboardVisible,
    keyboardHeight,
    keyboardLift,
    window.height,
    window.width,
  ]);

  // Guard: redirect to list if conversation was removed (e.g. left group)
  // Skip while store is still loading to avoid false redirect on cold start.
  useEffect(() => {
    if (id && !conversationsLoading && !conversation) {
      router.replace("/(main)/conversations");
    }
  }, [id, conversationsLoading, conversation, router]);

  // Load messages + start real-time stream for this conversation
  const { isLoading: messagesLoading, fetchMore } = useMessages(conversationId, {
    sendReadReceipts: readReceiptsEnabled,
    isDm: !isGroup,
  });

  // Conversation-scoped lifecycle recovery: when returning from background or
  // regaining network, make sure the current chat refetches and resumes.
  useAppState({ currentConversationId: conversationId });
  useNetworkState({ currentConversationId: conversationId });

  // Mark conversation as read and set active
  useEffect(() => {
    if (!id) return;
    const store = useConversationStore.getState();
    store.setActiveConversation(id);
    store.markRead(id);
    // Send read receipt on enter if enabled and DM with unread messages
    const convoItem = store.items.get(id);
    if (
      useSettingsStore.getState().readReceipts &&
      convoItem?.kind === "dm" &&
      (convoItem?.unreadCount ?? 0) > 0
    ) {
      sendReadReceipt(id);
    }
    return () => useConversationStore.getState().setActiveConversation(null);
  }, [id]);

  const listRef = useRef<any>(null);
  const isAtBottomRef = useRef(true);

  // "New messages" floating chip
  const [showNewMsgChip, setShowNewMsgChip] = useState(false);
  const chipOpacity = useRef(new Animated.Value(0)).current;

  const showChip = useCallback(() => {
    setShowNewMsgChip(true);
    Animated.timing(chipOpacity, { toValue: 1, duration: 200, useNativeDriver: true }).start();
  }, [chipOpacity]);

  const hideChip = useCallback(() => {
    Animated.timing(chipOpacity, { toValue: 0, duration: 150, useNativeDriver: true }).start(() =>
      setShowNewMsgChip(false)
    );
  }, [chipOpacity]);

  // Reply state
  const [replyTo, setReplyTo] = useState<MessageItem | null>(null);

  const scrollToBottom = useCallback(() => {
    try {
      listRef.current?.scrollToOffset({ offset: 0, animated: true });
    } catch {}
  }, []);

  // Scroll to bottom when keyboard appears (only if already at bottom)
  useEffect(() => {
    if (isKeyboardVisible && isAtBottomRef.current) {
      scrollToBottom();
    }
  }, [isKeyboardVisible, scrollToBottom]);

  // Track scroll position — inverted list: offset near 0 means at bottom
  const handleScroll = useCallback(
    (e: any) => {
      try {
        const offset = e?.nativeEvent?.contentOffset?.y ?? 0;
        const wasAtBottom = isAtBottomRef.current;
        isAtBottomRef.current = offset < 150;
        if (!wasAtBottom && isAtBottomRef.current && showNewMsgChip) hideChip();
      } catch {}
    },
    [showNewMsgChip, hideChip]
  );

  // Messages from store
  const storeMessages = useMessageStore((s) => s.byConversation[id ?? ""] ?? EMPTY_MESSAGES);
  const historyLoading = useMessageStore((s) => s.isLoading);
  const draftText = useDraftStore((s) => (id ? (s.byConversation[id]?.text ?? "") : ""));

  // All messages (including intents) — used for intentMap lookups
  const allMessages = useMemo(() => {
    if (!storeMessages || storeMessages.length === 0) return [];
    return [...storeMessages].sort((a, b) => b.sentAt - a.sentAt);
  }, [storeMessages]);

  // Visible messages — intent messages are hidden (they show via ActionButtons state)
  const messages = useMemo(() => allMessages.filter((m) => !m.intentRef), [allMessages]);

  // Batch-resolve sender addresses when messages change
  useEffect(() => {
    if (!isGroup || storeMessages.length === 0) return;
    const inboxIds = [...new Set(storeMessages.map((m) => m.senderInboxId))];
    resolveAddresses(inboxIds);
  }, [isGroup, storeMessages]);

  // Auto-scroll to bottom when new messages arrive, or show chip if scrolled up
  const prevMessageCount = useRef(0);
  useEffect(() => {
    const len = messages?.length ?? 0;
    if (len > prevMessageCount.current && len > 0) {
      if (isAtBottomRef.current) {
        setTimeout(scrollToBottom, 50);
      } else {
        showChip();
      }
    }
    prevMessageCount.current = len;
  }, [messages, scrollToBottom, showChip]);

  const [loadingMore, setLoadingMore] = useState(false);

  const handleLoadMore = useCallback(() => {
    if (loadingMore || historyLoading || messagesLoading) return;
    setLoadingMore(true);
    fetchMore().finally(() => setLoadingMore(false));
  }, [loadingMore, historyLoading, messagesLoading, fetchMore]);

  const handleSend = useCallback(
    (text: string) => {
      if (!id) return;
      isAtBottomRef.current = true;
      if (showNewMsgChip) hideChip();
      useDraftStore.getState().clearDraft(id);
      const cid = id as unknown as ConversationId;
      if (replyTo) {
        sendReply(cid, replyTo.id as string, text);
        setReplyTo(null);
      } else {
        sendMessage(cid, text);
      }
      setTimeout(scrollToBottom, 100);
    },
    [id, replyTo, scrollToBottom]
  );

  const handleDraftChange = useCallback(
    (text: string) => {
      if (!id) return;
      if (text.length === 0) {
        useDraftStore.getState().clearDraft(id);
        return;
      }
      useDraftStore.getState().setDraft(id, text);
    },
    [id]
  );

  const handleReply = useCallback((item: MessageItem) => {
    setReplyTo(item);
  }, []);

  const handleCancelReply = useCallback(() => {
    setReplyTo(null);
  }, []);

  const handleRetry = useCallback((item: MessageItem) => {
    const store = useMessageStore.getState();
    store.removeFailed(item.conversationId, item.id as string);
    sendMessage(item.conversationId as unknown as ConversationId, item.text);
  }, []);

  // Build a map of actionsId -> earliest selected actionId from intent messages.
  // allMessages is sorted newest-first, so we overwrite (last write = chronologically earliest).
  const intentMap = useMemo(() => {
    const map = new Map<string, string>();
    for (const msg of allMessages) {
      if (msg.intentRef) {
        map.set(msg.intentRef.actionsId, msg.intentRef.actionId);
      }
    }
    return map;
  }, [allMessages]);

  const intentMapRef = useRef(intentMap);
  useEffect(() => {
    intentMapRef.current = intentMap;
  }, [intentMap]);

  const renderItem: ListRenderItem<MessageItem> = useCallback(
    ({ item, index }) => {
      const prevItem = index + 1 < messages.length ? messages[index + 1] : null;
      const respondedActionId = item.actionsPayload
        ? intentMapRef.current.get(item.actionsPayload.id)
        : undefined;
      return (
        <MessageBubble
          item={item}
          prevItem={prevItem}
          isGroup={isGroup}
          respondedActionId={respondedActionId}
          onReply={handleReply}
          onRetry={handleRetry}
        />
      );
    },
    [isGroup, messages, handleReply, handleRetry]
  );

  const renderScrollComponent = useCallback(
    (props: any) => (
      <KeyboardAwareScrollView
        {...props}
        bottomOffset={0}
        extraKeyboardSpace={0}
        keyboardDismissMode="interactive"
        contentInsetAdjustmentBehavior="never"
        automaticallyAdjustContentInsets={false}
      />
    ),
    []
  );

  const renderFooter = useCallback(() => {
    if (!loadingMore) return null;
    return (
      <View style={styles.loadingMore}>
        <ActivityIndicator size="small" color="#6750A4" />
      </View>
    );
  }, [loadingMore]);

  const renderListHeader = useCallback(
    () => <View style={{ height: keyboardLift }} />,
    [keyboardLift]
  );

  return (
    <>
      <Stack.Screen
        options={{
          headerShown: false,
        }}
      />

      <View style={styles.container}>
        <Appbar.Header style={styles.appbar} elevated>
          <Appbar.BackAction onPress={() => router.back()} color="#E6E1E5" />
          <Appbar.Content title={conversationTitle} titleStyle={styles.appbarTitle} />
          <Appbar.Action
            icon="information-outline"
            iconColor="#E6E1E5"
            onPress={() => {
              if (!id) return;
              const route = isGroup ? "conversation/group-detail" : "conversation/dm-detail";
              router.push({ pathname: `/(main)/${route}`, params: { id } });
            }}
          />
        </Appbar.Header>

        {/* Message list — flex:1 so it shrinks when input bar grows */}
        <View style={{ flex: 1 }}>
          <FlashList
            ref={listRef}
            data={messages}
            renderItem={renderItem}
            renderScrollComponent={renderScrollComponent}
            keyExtractor={(item) => item.id as string}
            inverted
            onScroll={handleScroll}
            scrollEventThrottle={100}
            onEndReached={handleLoadMore}
            onEndReachedThreshold={0.3}
            ListHeaderComponent={renderListHeader}
            ListFooterComponent={renderFooter}
            contentContainerStyle={styles.listContent}
          />
          {/* "New messages" floating chip */}
          {showNewMsgChip && (
            <Animated.View style={[styles.newMsgChip, { opacity: chipOpacity }]}>
              <Pressable
                onPress={() => {
                  scrollToBottom();
                  hideChip();
                }}
                style={styles.newMsgChipInner}
              >
                <Text style={styles.newMsgChipText}>↓ New messages</Text>
              </Pressable>
            </Animated.View>
          )}
        </View>

        <KeyboardStickyView offset={{ opened: insets.bottom }}>
          <View
            onLayout={(e) => {
              const nextHeight = Math.ceil(e.nativeEvent.layout.height);
              if (nextHeight !== composerHeight) {
                setComposerHeight(nextHeight);
              }
            }}
            style={{
              paddingBottom: insets.bottom,
              backgroundColor: "#1a1a2e",
            }}
          >
            <MessageInput
              value={draftText}
              onChangeText={handleDraftChange}
              onSend={handleSend}
              replyTo={replyTo}
              onCancelReply={handleCancelReply}
            />
          </View>
        </KeyboardStickyView>
      </View>
    </>
  );
}

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
    fontWeight: "600",
    fontSize: 18,
  },
  listContent: {
    paddingVertical: 8,
  },
  loadingMore: {
    paddingVertical: 16,
    alignItems: "center",
  },
  newMsgChip: {
    position: "absolute",
    bottom: 8,
    alignSelf: "center",
    zIndex: 10,
  },
  newMsgChipInner: {
    backgroundColor: "#6750A4",
    paddingHorizontal: 16,
    paddingVertical: 8,
    borderRadius: 20,
    elevation: 4,
    shadowColor: "#000",
    shadowOffset: { width: 0, height: 2 },
    shadowOpacity: 0.3,
    shadowRadius: 4,
  },
  newMsgChipText: {
    color: "#FFFFFF",
    fontSize: 13,
    fontWeight: "600",
  },
});
