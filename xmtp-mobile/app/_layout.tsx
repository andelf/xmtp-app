/**
 * Root layout -- wraps the entire app with providers (Paper, SafeArea)
 * and handles initial auth restoration.
 */
import { Buffer } from "buffer";

import { useEffect } from "react";
import { Slot } from "expo-router";
import { PaperProvider, MD3DarkTheme } from "react-native-paper";
import { SafeAreaProvider } from "react-native-safe-area-context";
import { StatusBar } from "expo-status-bar";
import { KeyboardProvider } from "react-native-keyboard-controller";
import { useAuthStore } from "../src/store/auth";
import { log } from "../src/utils/logger";
// Polyfill Buffer for Hermes — required by @xmtp/react-native-sdk internals
if (typeof globalThis.Buffer === "undefined") {
  globalThis.Buffer = Buffer as any;
}

const theme = {
  ...MD3DarkTheme,
  colors: {
    ...MD3DarkTheme.colors,
    primary: "#6C5CE7",
    secondary: "#A29BFE",
  },
};

export default function RootLayout() {
  const restore = useAuthStore((s) => s.restore);

  useEffect(() => {
    log("App", "=== App started ===");
    restore();
  }, [restore]);

  return (
    <SafeAreaProvider>
      <KeyboardProvider>
        <PaperProvider theme={theme}>
          <StatusBar style="light" />
          <Slot />
        </PaperProvider>
      </KeyboardProvider>
    </SafeAreaProvider>
  );
}
