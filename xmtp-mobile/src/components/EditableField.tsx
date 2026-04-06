/**
 * EditableField -- displays a label + value that can be tapped to enter edit mode.
 *
 * Used for group name and description editing in group-detail.
 */
import React, { useCallback, useState } from "react";
import { View, StyleSheet, TextInput } from "react-native";
import { IconButton, Text, ActivityIndicator } from "react-native-paper";

export interface EditableFieldProps {
  label: string;
  value: string;
  placeholder?: string;
  editable?: boolean;
  multiline?: boolean;
  onSave: (newValue: string) => Promise<void>;
}

export function EditableField({
  label,
  value,
  placeholder,
  editable = false,
  multiline = false,
  onSave,
}: EditableFieldProps) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);
  const [saving, setSaving] = useState(false);

  const handleStartEdit = useCallback(() => {
    setDraft(value);
    setEditing(true);
  }, [value]);

  const handleCancel = useCallback(() => {
    setEditing(false);
    setDraft(value);
  }, [value]);

  const handleSave = useCallback(async () => {
    const trimmed = draft.trim();
    if (trimmed === value) {
      setEditing(false);
      return;
    }
    setSaving(true);
    try {
      await onSave(trimmed);
      setEditing(false);
    } finally {
      setSaving(false);
    }
  }, [draft, value, onSave]);

  if (editing) {
    return (
      <View style={styles.container}>
        <Text variant="bodySmall" style={styles.label}>
          {label}
        </Text>
        <View style={styles.editRow}>
          <TextInput
            style={[styles.input, multiline && styles.inputMultiline]}
            value={draft}
            onChangeText={setDraft}
            placeholder={placeholder}
            placeholderTextColor="#938F99"
            multiline={multiline}
            autoFocus
            editable={!saving}
          />
          {saving ? (
            <ActivityIndicator size={20} color="#6750A4" style={styles.actionBtn} />
          ) : (
            <View style={styles.actions}>
              <IconButton icon="check" iconColor="#81C784" size={20} onPress={handleSave} />
              <IconButton icon="close" iconColor="#938F99" size={20} onPress={handleCancel} />
            </View>
          )}
        </View>
      </View>
    );
  }

  return (
    <View style={styles.container}>
      <Text variant="bodySmall" style={styles.label}>
        {label}
      </Text>
      <View style={styles.displayRow}>
        <Text variant="bodyLarge" style={styles.value} numberOfLines={multiline ? 4 : 1}>
          {value || placeholder || "\u2014"}
        </Text>
        {editable && (
          <IconButton icon="pencil" iconColor="#938F99" size={18} onPress={handleStartEdit} />
        )}
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    marginBottom: 16,
  },
  label: {
    color: "#938F99",
    marginBottom: 4,
  },
  displayRow: {
    flexDirection: "row",
    alignItems: "center",
  },
  value: {
    color: "#E6E1E5",
    flex: 1,
  },
  editRow: {
    flexDirection: "row",
    alignItems: "flex-start",
  },
  input: {
    flex: 1,
    color: "#E6E1E5",
    backgroundColor: "#16213e",
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 10,
    fontSize: 16,
    borderWidth: 1,
    borderColor: "#6750A4",
  },
  inputMultiline: {
    minHeight: 60,
    textAlignVertical: "top",
  },
  actions: {
    flexDirection: "row",
  },
  actionBtn: {
    marginLeft: 8,
    marginTop: 10,
  },
});
