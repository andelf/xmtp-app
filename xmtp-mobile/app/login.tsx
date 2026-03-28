/**
 * Login page -- private key input to create XMTP client.
 */
import { useState } from "react";
import { View, StyleSheet, KeyboardAvoidingView, Platform, ScrollView } from "react-native";
import { TextInput, Button, Text, HelperText, Surface, SegmentedButtons } from "react-native-paper";
import { useRouter } from "expo-router";
import { useAuthStore } from "../src/store/auth";

type XmtpEnv = "dev" | "production" | "local";

const ENV_BUTTONS = [
  { value: "dev", label: "Dev" },
  { value: "production", label: "Production" },
  { value: "local", label: "Local" },
];

const SUBTITLES: Record<XmtpEnv, string> = {
  dev: "Connect to the XMTP dev network",
  production: "Connect to the XMTP production network",
  local: "Connect to a self-hosted XMTP node",
};

export default function LoginScreen() {
  const [privateKey, setPrivateKey] = useState("");
  const [env, setEnv] = useState<XmtpEnv>("dev");
  const [customLocalHost, setCustomLocalHost] = useState("");
  const init = useAuthStore((s) => s.init);
  const isLoading = useAuthStore((s) => s.isLoading);
  const error = useAuthStore((s) => s.error);
  const isReady = useAuthStore((s) => s.isReady);
  const router = useRouter();

  const isConnectDisabled =
    isLoading || !privateKey.trim() || (env === "local" && !customLocalHost.trim());

  const handleConnect = async () => {
    const trimmed = privateKey.trim();
    if (!trimmed) return;
    await init(trimmed, env, env === "local" ? customLocalHost.trim() : undefined);
  };

  // Navigate after successful init
  if (isReady) {
    // Use setTimeout to avoid state update during render
    setTimeout(() => router.replace("/(main)/conversations"), 0);
  }

  return (
    <KeyboardAvoidingView
      style={styles.container}
      behavior={Platform.OS === "ios" ? "padding" : "height"}
    >
      <ScrollView contentContainerStyle={styles.scrollContent} keyboardShouldPersistTaps="handled">
        <Surface style={styles.card} elevation={2}>
          <Text variant="headlineMedium" style={styles.title}>
            XMTP Mobile
          </Text>
          <Text variant="bodyMedium" style={styles.subtitle}>
            {SUBTITLES[env]}
          </Text>

          <Text variant="labelMedium" style={styles.networkLabel}>
            Network
          </Text>
          <SegmentedButtons
            value={env}
            onValueChange={(value) => setEnv(value as XmtpEnv)}
            buttons={ENV_BUTTONS}
            style={styles.segmentedButtons}
            density="medium"
          />

          {env === "local" && (
            <TextInput
              label="Local Host URL"
              value={customLocalHost}
              onChangeText={setCustomLocalHost}
              mode="outlined"
              placeholder="http://192.168.1.100:5556"
              autoCapitalize="none"
              autoCorrect={false}
              style={styles.localHostInput}
              disabled={isLoading}
            />
          )}

          <TextInput
            label="Private Key"
            value={privateKey}
            onChangeText={setPrivateKey}
            mode="outlined"
            secureTextEntry
            placeholder="0x..."
            autoCapitalize="none"
            autoCorrect={false}
            style={styles.input}
            disabled={isLoading}
          />

          <HelperText type="error" visible={!!error}>
            {error}
          </HelperText>

          <Button
            mode="contained"
            onPress={handleConnect}
            loading={isLoading}
            disabled={isConnectDisabled}
            style={styles.button}
          >
            Connect
          </Button>

          <Text variant="bodySmall" style={styles.warning}>
            Your private key is stored securely on-device using expo-secure-store and never
            transmitted.
          </Text>
        </Surface>
      </ScrollView>
    </KeyboardAvoidingView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: "#1a1a2e",
  },
  scrollContent: {
    flexGrow: 1,
    justifyContent: "center",
    padding: 20,
  },
  card: {
    padding: 24,
    borderRadius: 16,
    backgroundColor: "#16213e",
  },
  title: {
    textAlign: "center",
    color: "#ffffff",
    marginBottom: 8,
  },
  subtitle: {
    textAlign: "center",
    color: "#a0a0b0",
    marginBottom: 24,
  },
  networkLabel: {
    color: "#E6E1E5",
    marginBottom: 8,
  },
  segmentedButtons: {
    marginBottom: 20,
  },
  localHostInput: {
    marginBottom: 12,
  },
  input: {
    marginBottom: 4,
  },
  button: {
    marginTop: 8,
    marginBottom: 16,
  },
  warning: {
    textAlign: "center",
    color: "#666680",
    fontStyle: "italic",
  },
});
