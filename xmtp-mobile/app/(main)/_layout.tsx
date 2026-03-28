/**
 * Main stack navigator -- shown after authentication.
 *
 * Activates global foreground-resync and network-recovery hooks so that
 * conversations and messages stay up to date across app lifecycle events.
 */
import { Stack } from "expo-router";
import { useAppState } from "../../src/hooks/useAppState";
import { useNetworkState } from "../../src/hooks/useNetworkState";
import { useConversations } from "../../src/hooks/useConversations";

export default function MainLayout() {
  // Start conversation list sync + streams on auth
  useConversations();
  // Global lifecycle hooks
  useAppState();
  useNetworkState();
  return (
    <Stack
      screenOptions={{
        headerStyle: { backgroundColor: "#1a1a2e" },
        headerTintColor: "#ffffff",
        contentStyle: { backgroundColor: "#1a1a2e" },
      }}
    >
      <Stack.Screen
        name="conversations"
        options={{ title: "Conversations", headerShown: false }}
      />
      <Stack.Screen
        name="conversation/[id]"
        options={{ title: "Chat" }}
      />
      <Stack.Screen
        name="new-conversation"
        options={{ title: "New Conversation", headerShown: false }}
      />
    </Stack>
  );
}
