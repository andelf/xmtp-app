import React, { useCallback } from "react";
import { Stack, useLocalSearchParams, useRouter } from "expo-router";

import { ConversationPane } from "../../../src/components/ConversationPane";

export default function ConversationScreen() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const router = useRouter();

  const handleMissingConversation = useCallback(() => {
    router.replace("/(main)/conversations");
  }, [router]);

  return (
    <>
      <Stack.Screen
        options={{
          headerShown: false,
        }}
      />
      <ConversationPane
        conversationId={id ?? null}
        showBackButton
        onBackPress={() => router.back()}
        onMissingConversation={handleMissingConversation}
      />
    </>
  );
}
