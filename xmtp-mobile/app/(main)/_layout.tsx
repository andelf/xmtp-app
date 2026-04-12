/**
 * Main stack navigator -- shown after authentication.
 *
 * Activates global foreground-resync and network-recovery hooks so that
 * conversations and messages stay up to date across app lifecycle events.
 */
import { useEffect } from "react";
import { Stack } from "expo-router";
import { useAppState } from "../../src/hooks/useAppState";
import { useNetworkState } from "../../src/hooks/useNetworkState";
import { useConversations } from "../../src/hooks/useConversations";
import { useSettingsStore } from "../../src/store/settings";

export default function MainLayout() {
  // Start conversation list sync + streams on auth
  useConversations();
  // Global lifecycle hooks
  useAppState();
  useNetworkState();
  // Load persisted settings
  useEffect(() => {
    useSettingsStore.getState().load();
  }, []);
  return (
    <Stack
      screenOptions={{
        headerStyle: { backgroundColor: "#1a1a2e" },
        headerTintColor: "#ffffff",
        contentStyle: { backgroundColor: "#1a1a2e" },
        statusBarTranslucent: true,
      }}
    >
      <Stack.Screen name="conversations" options={{ title: "Conversations", headerShown: false }} />
      <Stack.Screen name="conversation/[id]" options={{ title: "Chat" }} />
      <Stack.Screen
        name="new-conversation"
        options={{ title: "New Conversation", headerShown: false }}
      />
      <Stack.Screen name="settings" options={{ title: "Settings", headerShown: false }} />
      <Stack.Screen name="about" options={{ title: "About", headerShown: false }} />
      <Stack.Screen
        name="conversation/dm-detail"
        options={{ title: "DM Details", headerShown: false }}
      />
      <Stack.Screen
        name="conversation/group-detail"
        options={{ title: "Group Details", headerShown: false }}
      />
      <Stack.Screen
        name="conversation/add-member"
        options={{ title: "Add Members", headerShown: false }}
      />
    </Stack>
  );
}
