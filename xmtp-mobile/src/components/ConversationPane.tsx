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
import type { ConversationId } from "@xmtp/react-native-sdk";
import { useRouter } from "expo-router";

import { useConversationStore } from "../store/conversations";
import { useMessageStore } from "../store/messages";
import type { MessageItem } from "../store/messages";
import { sendMessage, sendReply, sendReadReceipt } from "../xmtp/messages";
import { useSettingsStore } from "../store/settings";
import { useMessages } from "../hooks/useMessages";
import { useAppState } from "../hooks/useAppState";
import { useNetworkState } from "../hooks/useNetworkState";
import { MessageBubble } from "./MessageBubble";
import { MessageInput } from "./MessageInput";
import { useDraftStore } from "../store/drafts";
import { shortenAddress } from "../utils/address";
import { resolveAddresses } from "../utils/addressLookup";

const EMPTY_MESSAGES: MessageItem[] = [];

export interface ConversationPaneProps {
  conversationId: string | null;
  showBackButton?: boolean;
  onBackPress?: () => void;
  onMissingConversation?: () => void;
}

export function ConversationPane({
  conversationId,
  showBackButton = false,
  onBackPress,
  onMissingConversation,
}: ConversationPaneProps) {
  const router = useRouter();
  const insets = useSafeAreaInsets();
  const isKeyboardVisible = useKeyboardState((state) => state.isVisible);
  const keyboardHeight = useKeyboardState((state) => state.height);
  const window = useWindowDimensions();

  const xmtpConversationId = conversationId ? (conversationId as unknown as ConversationId) : null;
  const readReceiptsEnabled = useSettingsStore((s) => s.readReceipts);
  const conversationsLoading = useConversationStore((s) => s.isLoading);
  const conversation = useConversationStore((s) =>
    conversationId ? (s.items.get(conversationId) ?? null) : null
  );
  const conversationTitle =
    conversation?.title ?? (conversationId ? shortenAddress(conversationId) : "Chat");
  const isGroup = conversation?.kind === "group";
  const [composerHeight, setComposerHeight] = useState(0);
  const keyboardLift = isKeyboardVisible ? Math.max(keyboardHeight - insets.bottom, 0) : 0;

  useEffect(() => {
    console.log("[FoldDebug][Conversation]", {
      screen: "conversation",
      id: conversationId,
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
    conversationId,
    insets.bottom,
    insets.top,
    isKeyboardVisible,
    keyboardHeight,
    keyboardLift,
    window.height,
    window.width,
  ]);

  useEffect(() => {
    if (conversationId && !conversationsLoading && !conversation) {
      onMissingConversation?.();
    }
  }, [conversationId, conversationsLoading, conversation, onMissingConversation]);

  const { isLoading: messagesLoading, fetchMore } = useMessages(xmtpConversationId, {
    sendReadReceipts: readReceiptsEnabled,
    isDm: !isGroup,
  });

  useAppState({ currentConversationId: xmtpConversationId });
  useNetworkState({ currentConversationId: xmtpConversationId });

  useEffect(() => {
    if (!conversationId) return;
    const store = useConversationStore.getState();
    store.setActiveConversation(conversationId);
    store.markRead(conversationId);
    const convoItem = store.items.get(conversationId);
    if (
      useSettingsStore.getState().readReceipts &&
      convoItem?.kind === "dm" &&
      (convoItem?.unreadCount ?? 0) > 0
    ) {
      sendReadReceipt(conversationId);
    }
    return () => useConversationStore.getState().setActiveConversation(null);
  }, [conversationId]);

  const listRef = useRef<any>(null);
  const isAtBottomRef = useRef(true);
  const [showNewMsgChip, setShowNewMsgChip] = useState(false);
  const chipOpacity = useRef(new Animated.Value(0)).current;
  const [replyTo, setReplyTo] = useState<MessageItem | null>(null);
  const [loadingMore, setLoadingMore] = useState(false);

  const showChip = useCallback(() => {
    setShowNewMsgChip(true);
    Animated.timing(chipOpacity, { toValue: 1, duration: 200, useNativeDriver: true }).start();
  }, [chipOpacity]);

  const hideChip = useCallback(() => {
    Animated.timing(chipOpacity, { toValue: 0, duration: 150, useNativeDriver: true }).start(() =>
      setShowNewMsgChip(false)
    );
  }, [chipOpacity]);

  const scrollToBottom = useCallback(() => {
    try {
      listRef.current?.scrollToOffset({ offset: 0, animated: true });
    } catch {}
  }, []);

  useEffect(() => {
    if (isKeyboardVisible && isAtBottomRef.current) {
      scrollToBottom();
    }
  }, [isKeyboardVisible, scrollToBottom]);

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

  const storeMessages = useMessageStore(
    (s) => s.byConversation[conversationId ?? ""] ?? EMPTY_MESSAGES
  );
  const historyLoading = useMessageStore((s) => s.isLoading);
  const draftText = useDraftStore((s) =>
    conversationId ? (s.byConversation[conversationId]?.text ?? "") : ""
  );

  const allMessages = useMemo(() => {
    if (!storeMessages || storeMessages.length === 0) return [];
    return [...storeMessages].sort((a, b) => b.sentAt - a.sentAt);
  }, [storeMessages]);

  const messages = useMemo(() => allMessages.filter((m) => !m.intentRef), [allMessages]);

  useEffect(() => {
    if (!isGroup || storeMessages.length === 0) return;
    const inboxIds = [...new Set(storeMessages.map((m) => m.senderInboxId))];
    resolveAddresses(inboxIds);
  }, [isGroup, storeMessages]);

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

  const handleLoadMore = useCallback(() => {
    if (loadingMore || historyLoading || messagesLoading) return;
    setLoadingMore(true);
    fetchMore().finally(() => setLoadingMore(false));
  }, [loadingMore, historyLoading, messagesLoading, fetchMore]);

  const handleSend = useCallback(
    (text: string) => {
      if (!conversationId) return;
      isAtBottomRef.current = true;
      if (showNewMsgChip) hideChip();
      useDraftStore.getState().clearDraft(conversationId);
      const cid = conversationId as unknown as ConversationId;
      if (replyTo) {
        sendReply(cid, replyTo.id as string, text);
        setReplyTo(null);
      } else {
        sendMessage(cid, text);
      }
      setTimeout(scrollToBottom, 100);
    },
    [conversationId, replyTo, scrollToBottom, showNewMsgChip, hideChip]
  );

  const handleDraftChange = useCallback(
    (text: string) => {
      if (!conversationId) return;
      if (text.length === 0) {
        useDraftStore.getState().clearDraft(conversationId);
        return;
      }
      useDraftStore.getState().setDraft(conversationId, text);
    },
    [conversationId]
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

  if (!conversationId) {
    return (
      <View style={styles.emptyState}>
        <Text variant="titleMedium" style={styles.emptyTitle}>
          Select a conversation
        </Text>
        <Text variant="bodyMedium" style={styles.emptySubtitle}>
          Pick a chat from the list to open it here.
        </Text>
      </View>
    );
  }

  return (
    <View style={styles.container}>
      <Appbar.Header style={styles.appbar} elevated>
        {showBackButton ? (
          <Appbar.BackAction
            onPress={onBackPress ?? (() => router.back())}
            color="#E6E1E5"
          />
        ) : null}
        <Appbar.Content title={conversationTitle} titleStyle={styles.appbarTitle} />
        <Appbar.Action
          icon="information-outline"
          iconColor="#E6E1E5"
          onPress={() => {
            if (!conversationId) return;
            const route = isGroup ? "conversation/group-detail" : "conversation/dm-detail";
            router.push({ pathname: `/(main)/${route}`, params: { id: conversationId } });
          }}
        />
      </Appbar.Header>

      <View style={styles.listPane}>
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
          style={[styles.composerShell, { paddingBottom: insets.bottom }]}
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
  listPane: {
    flex: 1,
  },
  listContent: {
    paddingVertical: 8,
  },
  composerShell: {
    backgroundColor: "#1a1a2e",
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
  emptyState: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    paddingHorizontal: 32,
    backgroundColor: "#1a1a2e",
  },
  emptyTitle: {
    color: "#E6E1E5",
    marginBottom: 8,
  },
  emptySubtitle: {
    color: "#938F99",
    textAlign: "center",
  },
});
