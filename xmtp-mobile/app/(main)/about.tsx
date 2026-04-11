/**
 * About page -- displays app version, XMTP environment, and account info.
 */
import React from "react";
import { View, StyleSheet, ScrollView } from "react-native";
import { Text, Divider } from "react-native-paper";
import { Stack } from "expo-router";
import Constants from "expo-constants";
import { useAuthStore } from "../../src/store/auth";
import { InfoRow } from "../../src/components/InfoRow";

export default function AboutScreen() {
  const address = useAuthStore((s) => s.address);
  const inboxId = useAuthStore((s) => s.inboxId);
  const env = useAuthStore((s) => s.env);
  const gitCommit = Constants.expoConfig?.extra?.gitCommit ?? "unknown";
  const buildTime = Constants.expoConfig?.extra?.buildTime ?? "";

  return (
    <>
      <Stack.Screen
        options={{
          headerShown: true,
          title: "About",
          headerStyle: { backgroundColor: "#1a1a2e" },
          headerTintColor: "#E6E1E5",
          headerTitleStyle: { fontWeight: "600", fontSize: 18 },
        }}
      />

      <ScrollView style={styles.container} contentContainerStyle={styles.content}>
        {/* App identity */}
        <View style={styles.logoSection}>
          <Text variant="headlineMedium" style={styles.appName}>
            XMTP Messenger
          </Text>
          <Text variant="bodySmall" style={styles.version}>
            v0.1.0 ({gitCommit})
          </Text>
        </View>

        <Divider style={styles.divider} />

        {/* Account info */}
        <Text variant="titleMedium" style={styles.sectionTitle}>
          Account
        </Text>
        <InfoRow label="Wallet Address" value={address} />
        <InfoRow label="Inbox ID" value={inboxId} />

        <Divider style={styles.divider} />

        {/* Network */}
        <Text variant="titleMedium" style={styles.sectionTitle}>
          Network
        </Text>
        <InfoRow label="Environment" value={env} />
        <InfoRow label="Protocol" value="XMTP v3 (MLS)" />

        <Divider style={styles.divider} />

        {/* Build */}
        <Text variant="titleMedium" style={styles.sectionTitle}>
          Build
        </Text>
        <InfoRow label="Commit" value={gitCommit} />
        {buildTime ? <InfoRow label="Build Time" value={buildTime} /> : null}

        <Divider style={styles.divider} />

        {/* Credits */}
        <Text variant="bodySmall" style={styles.credits}>
          Built with XMTP React Native SDK
        </Text>
      </ScrollView>
    </>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: "#1a1a2e",
  },
  content: {
    padding: 20,
  },
  logoSection: {
    alignItems: "center",
    paddingVertical: 24,
  },
  appName: {
    color: "#E6E1E5",
    fontWeight: "700",
  },
  version: {
    color: "#938F99",
    marginTop: 4,
  },
  sectionTitle: {
    color: "#E6E1E5",
    fontWeight: "600",
    marginBottom: 12,
  },
  divider: {
    backgroundColor: "#49454F",
    marginVertical: 20,
  },
  credits: {
    color: "#938F99",
    textAlign: "center",
    marginTop: 8,
  },
});
