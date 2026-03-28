/**
 * useNetworkState -- monitors network connectivity changes.
 *
 * Polls every 10 seconds. When network recovers (offline -> online),
 * syncs conversations and optionally fetches messages for the current chat.
 */
import { useEffect, useRef } from "react";
import { AppState } from "react-native";
import type { ConversationId } from "@xmtp/react-native-sdk";
import { useConversationStore } from "../store/conversations";
import { useMessageStore } from "../store/messages";
import { getClient } from "../xmtp/client";

interface UseNetworkStateOptions {
  currentConversationId?: ConversationId | null;
}

const POLL_INTERVAL_MS = 10_000;

export function useNetworkState(options?: UseNetworkStateOptions) {
  const wasOnlineRef = useRef<boolean>(true);
  const optionsRef = useRef(options);
  optionsRef.current = options;

  useEffect(() => {
    const checkConnectivity = async () => {
      if (AppState.currentState !== "active") return;

      try {
        const controller = new AbortController();
        const timeoutId = setTimeout(() => controller.abort(), 5000);
        await fetch("https://clients.google.com/generate_204", {
          method: "HEAD",
          signal: controller.signal,
        });
        clearTimeout(timeoutId);

        if (!wasOnlineRef.current) {
          wasOnlineRef.current = true;
          const client = getClient();
          if (!client) return;

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
      } catch {
        if (wasOnlineRef.current) {
          console.log("[useNetworkState] Network appears offline");
        }
        wasOnlineRef.current = false;
      }
    };

    const timer = setInterval(checkConnectivity, POLL_INTERVAL_MS);
    checkConnectivity();

    return () => clearInterval(timer);
  }, []); // no deps — stable forever
}
