/**
 * Conversation / chat screen -- message list with input bar.
 *
 * Uses FlashList in inverted mode so the newest messages appear at the bottom.
 * Keyboard avoidance uses react-native-keyboard-controller with
 * behavior="translate-with-padding" (purpose-built for chat screens).
 */
import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { View, StyleSheet, Keyboard } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { KeyboardAvoidingView } from "react-native-keyboard-controller";
import { ActivityIndicator } from "react-native-paper";
import { FlashList, type ListRenderItem } from "@shopify/flash-list";
import { useLocalSearchParams, Stack } from "expo-router";
import { useHeaderHeight } from "@react-navigation/elements";
import type { ConversationId } from "@xmtp/react-native-sdk";

import { useConversationStore } from "../../../src/store/conversations";
import { useMessageStore } from "../../../src/store/messages";
import type { MessageItem } from "../../../src/store/messages";
import { sendMessage, sendReply } from "../../../src/xmtp/messages";
import { useMessages } from "../../../src/hooks/useMessages";
import { MessageBubble } from "../../../src/components/MessageBubble";
import { MessageInput } from "../../../src/components/MessageInput";
import { shortenAddress } from "../../../src/utils/address";
import { resolveAddresses } from "../../../src/utils/addressLookup";

const EMPTY_MESSAGES: MessageItem[] = [];

export default function ConversationScreen() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const headerHeight = useHeaderHeight();
  const insets = useSafeAreaInsets();

  const conversationId = id ? (id as unknown as ConversationId) : null;

  // Load messages + start real-time stream for this conversation
  const { isLoading: messagesLoading, fetchMore } = useMessages(conversationId);

  // Resolve conversation title and kind from store
  const conversationTitle = useConversationStore((s) => {
    if (!id) return "Chat";
    const item = s.items.get(id);
    return item?.title ?? shortenAddress(id);
  });
  const isGroup = useConversationStore((s) => {
    if (!id) return false;
    return s.items.get(id)?.kind === "group";
  });

  const listRef = useRef<any>(null);
  const [keyboardVisible, setKeyboardVisible] = useState(false);
  const isAtBottomRef = useRef(true);

  // Reply state
  const [replyTo, setReplyTo] = useState<MessageItem | null>(null);

  const scrollToBottom = useCallback(() => {
    try {
      listRef.current?.scrollToOffset({ offset: 0, animated: true });
    } catch {}
  }, []);

  // Scroll to bottom when keyboard appears (only if already at bottom)
  useEffect(() => {
    const showSub = Keyboard.addListener("keyboardDidShow", () => {
      setKeyboardVisible(true);
      if (isAtBottomRef.current) scrollToBottom();
    });
    const hideSub = Keyboard.addListener("keyboardDidHide", () => {
      setKeyboardVisible(false);
    });
    return () => {
      showSub.remove();
      hideSub.remove();
    };
  }, []);

  // Track scroll position — inverted list: offset near 0 means at bottom
  const handleScroll = useCallback((e: any) => {
    try {
      const offset = e?.nativeEvent?.contentOffset?.y ?? 0;
      isAtBottomRef.current = offset < 150;
    } catch {}
  }, []);

  // Messages from store
  const storeMessages = useMessageStore((s) => s.byConversation[id ?? ""] ?? EMPTY_MESSAGES);
  const storeLoading = useMessageStore((s) => s.isLoading);

  const messages = useMemo(() => {
    if (!storeMessages || storeMessages.length === 0) return [];
    return [...storeMessages].sort((a, b) => b.sentAt - a.sentAt);
  }, [storeMessages]);

  // Batch-resolve sender addresses when messages change
  useEffect(() => {
    if (!isGroup || storeMessages.length === 0) return;
    const inboxIds = [...new Set(storeMessages.map((m) => m.senderInboxId))];
    resolveAddresses(inboxIds);
  }, [isGroup, storeMessages]);

  // Auto-scroll to bottom when new messages arrive (only if at bottom)
  const prevMessageCount = useRef(0);
  useEffect(() => {
    const len = messages?.length ?? 0;
    if (len > prevMessageCount.current && len > 0 && isAtBottomRef.current) {
      setTimeout(scrollToBottom, 50);
    }
    prevMessageCount.current = len;
  }, [messages, scrollToBottom]);

  const [loadingMore, setLoadingMore] = useState(false);

  const handleLoadMore = useCallback(() => {
    if (loadingMore || storeLoading || messagesLoading) return;
    setLoadingMore(true);
    fetchMore().finally(() => setLoadingMore(false));
  }, [loadingMore, storeLoading, messagesLoading, fetchMore]);

  const handleSend = useCallback(
    (text: string) => {
      if (!id) return;
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

  const handleReply = useCallback((item: MessageItem) => {
    setReplyTo(item);
  }, []);

  const handleCancelReply = useCallback(() => {
    setReplyTo(null);
  }, []);

  const renderItem: ListRenderItem<MessageItem> = useCallback(
    ({ item, index }) => {
      const prevItem = index + 1 < messages.length ? messages[index + 1] : null;
      return (
        <MessageBubble item={item} prevItem={prevItem} isGroup={isGroup} onReply={handleReply} />
      );
    },
    [isGroup, messages, handleReply]
  );

  const renderFooter = useCallback(() => {
    if (!loadingMore) return null;
    return (
      <View style={styles.loadingMore}>
        <ActivityIndicator size="small" color="#6750A4" />
      </View>
    );
  }, [loadingMore]);

  return (
    <>
      <Stack.Screen
        options={{
          headerShown: true,
          title: conversationTitle,
          headerStyle: { backgroundColor: "#1a1a2e" },
          headerTintColor: "#E6E1E5",
          headerTitleStyle: { fontWeight: "600", fontSize: 18 },
        }}
      />

      <KeyboardAvoidingView
        behavior="translate-with-padding"
        keyboardVerticalOffset={headerHeight}
        style={styles.container}
      >
        {/* Message list */}
        <FlashList
          ref={listRef}
          data={messages}
          renderItem={renderItem}
          keyExtractor={(item) => item.id as string}
          inverted
          onScroll={handleScroll}
          scrollEventThrottle={100}
          onEndReached={handleLoadMore}
          onEndReachedThreshold={0.3}
          ListFooterComponent={renderFooter}
          contentContainerStyle={styles.listContent}
        />

        {/* Input bar — only pad for nav bar when keyboard is closed */}
        <View
          style={{ paddingBottom: keyboardVisible ? 0 : insets.bottom, backgroundColor: "#1a1a2e" }}
        >
          <MessageInput onSend={handleSend} replyTo={replyTo} onCancelReply={handleCancelReply} />
        </View>
      </KeyboardAvoidingView>
    </>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: "#1a1a2e",
  },
  listContent: {
    paddingVertical: 8,
  },
  loadingMore: {
    paddingVertical: 16,
    alignItems: "center",
  },
});
