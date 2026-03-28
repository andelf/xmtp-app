/**
 * Login page -- private key input to create XMTP client.
 */
import { useState } from "react";
import {
  View,
  StyleSheet,
  KeyboardAvoidingView,
  Platform,
  ScrollView,
} from "react-native";
import {
  TextInput,
  Button,
  Text,
  HelperText,
  Surface,
} from "react-native-paper";
import { useRouter } from "expo-router";
import { useAuthStore } from "../src/store/auth";

export default function LoginScreen() {
  const [privateKey, setPrivateKey] = useState("");
  const init = useAuthStore((s) => s.init);
  const isLoading = useAuthStore((s) => s.isLoading);
  const error = useAuthStore((s) => s.error);
  const isReady = useAuthStore((s) => s.isReady);
  const router = useRouter();

  const handleConnect = async () => {
    const trimmed = privateKey.trim();
    if (!trimmed) return;
    await init(trimmed);
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
      <ScrollView
        contentContainerStyle={styles.scrollContent}
        keyboardShouldPersistTaps="handled"
      >
        <Surface style={styles.card} elevation={2}>
          <Text variant="headlineMedium" style={styles.title}>
            XMTP Mobile
          </Text>
          <Text variant="bodyMedium" style={styles.subtitle}>
            Enter your Ethereum private key to connect to the XMTP dev network.
          </Text>

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
            disabled={isLoading || !privateKey.trim()}
            style={styles.button}
          >
            Connect
          </Button>

          <Text variant="bodySmall" style={styles.warning}>
            Your private key is stored securely on-device using expo-secure-store
            and never transmitted.
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
