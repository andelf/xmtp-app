/**
 * useAppState -- monitors app foreground/background transitions.
 *
 * When the app returns to the foreground after being backgrounded:
 *   1. Syncs all known conversations via conversationStore.fetchAll().
 *   2. If a `currentConversationId` is provided, fetches missing messages.
 */
import { useEffect, useRef } from "react";
import { AppState, type AppStateStatus } from "react-native";
import type { ConversationId } from "@xmtp/react-native-sdk";
import { useConversationStore } from "../store/conversations";
import { useMessageStore } from "../store/messages";
import { getClient } from "../xmtp/client";

interface UseAppStateOptions {
  currentConversationId?: ConversationId | null;
}

export function useAppState(options?: UseAppStateOptions) {
  const lastActiveRef = useRef<number>(Date.now());
  const appStateRef = useRef<AppStateStatus>(AppState.currentState);
  const optionsRef = useRef(options);
  optionsRef.current = options;

  useEffect(() => {
    const subscription = AppState.addEventListener("change", (nextState: AppStateStatus) => {
      const prev = appStateRef.current;

      if (prev === "active" && nextState.match(/inactive|background/)) {
        lastActiveRef.current = Date.now();
      }

      if (prev.match(/inactive|background/) && nextState === "active") {
        const client = getClient();
        if (!client) return;

        // Access stores directly (not via selector) to avoid re-render deps
        useConversationStore
          .getState()
          .fetchAll()
          .catch(() => {});

        const conversationId = optionsRef.current?.currentConversationId;
        if (conversationId) {
          useMessageStore
            .getState()
            .fetchMessages(conversationId, { limit: 30 })
            .catch(() => {});
        }
      }

      appStateRef.current = nextState;
    });

    return () => subscription.remove();
  }, []); // no deps — stable forever
}
