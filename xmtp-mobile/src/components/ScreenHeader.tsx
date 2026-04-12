import React, { useCallback } from "react";
import { Appbar } from "react-native-paper";
import { useRouter } from "expo-router";

type ScreenHeaderProps = {
  title: string;
  canGoBack?: boolean;
  onBackPress?: () => void;
};

export function ScreenHeader({ title, canGoBack = true, onBackPress }: ScreenHeaderProps) {
  const router = useRouter();

  const handleBack = useCallback(() => {
    if (onBackPress) {
      onBackPress();
      return;
    }
    if (router.canGoBack()) {
      router.back();
    } else {
      router.replace("/(main)/conversations");
    }
  }, [onBackPress, router]);

  return (
    <Appbar.Header style={{ backgroundColor: "#1a1a2e" }} elevated>
      {canGoBack ? <Appbar.BackAction onPress={handleBack} iconColor="#E6E1E5" /> : null}
      <Appbar.Content
        title={title}
        titleStyle={{ color: "#E6E1E5", fontWeight: "600", fontSize: 18 }}
      />
    </Appbar.Header>
  );
}
