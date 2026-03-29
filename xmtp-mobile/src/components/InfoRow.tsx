/**
 * InfoRow -- label + selectable value, tap to copy to clipboard.
 * Shared across About, DM Detail, and Group Detail screens.
 */
import React from "react";
import { View, StyleSheet, Clipboard } from "react-native";
import { Text } from "react-native-paper";

export interface InfoRowProps {
  label: string;
  value: string | null;
  numberOfLines?: number;
}

export function InfoRow({ label, value, numberOfLines = 2 }: InfoRowProps) {
  const handlePress = () => {
    if (value) Clipboard.setString(value);
  };

  return (
    <View style={styles.infoRow}>
      <Text variant="bodySmall" style={styles.infoLabel}>
        {label}
      </Text>
      <Text
        variant="bodyMedium"
        style={styles.infoValue}
        selectable
        onPress={handlePress}
        numberOfLines={numberOfLines}
      >
        {value || "\u2014"}
      </Text>
    </View>
  );
}

export const styles = StyleSheet.create({
  infoRow: {
    marginBottom: 16,
  },
  infoLabel: {
    color: "#938F99",
    marginBottom: 2,
  },
  infoValue: {
    color: "#E6E1E5",
  },
});
