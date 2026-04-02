/**
 * New conversation page -- supports both DM and Group creation modes.
 *
 * Accepts ?mode=group URL param to pre-select Group tab.
 */
import React, { useCallback, useState } from "react";
import { View, StyleSheet, KeyboardAvoidingView, Platform, ScrollView } from "react-native";
import {
  Appbar,
  TextInput,
  Button,
  Text,
  HelperText,
  SegmentedButtons,
  Chip,
  RadioButton,
} from "react-native-paper";
import { useRouter, useLocalSearchParams } from "expo-router";

import { getClient } from "../../src/xmtp/client";
import { useConversationStore, conversationToItem } from "../../src/store/conversations";
import { PublicIdentity } from "@xmtp/react-native-sdk";
import { createGroup } from "../../src/xmtp/groups";
import { isValidEthAddress, shortenAddress } from "../../src/utils/address";

// ---------------------------------------------------------------------------
// Validation (DM mode)
// ---------------------------------------------------------------------------

function validateAddress(addr: string): string | null {
  if (!addr) return "Address is required";
  if (!addr.startsWith("0x")) return "Address must start with 0x";
  if (addr.length !== 42) return "Address must be 42 characters";
  if (!isValidEthAddress(addr)) return "Invalid Ethereum address";
  return null;
}

// ---------------------------------------------------------------------------
// Screen
// ---------------------------------------------------------------------------

export default function NewConversationScreen() {
  const router = useRouter();
  const { mode: initialMode } = useLocalSearchParams<{ mode?: string }>();
  const upsert = useConversationStore((s) => s.upsert);

  // Mode toggle
  const [mode, setMode] = useState<"dm" | "group">(initialMode === "group" ? "group" : "dm");

  // --- DM state ---
  const [address, setAddress] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [touched, setTouched] = useState(false);
  const validationError = touched ? validateAddress(address.trim()) : null;

  // --- Group state ---
  const [groupName, setGroupName] = useState("");
  const [memberInput, setMemberInput] = useState("");
  const [members, setMembers] = useState<string[]>([]);
  const [groupLoading, setGroupLoading] = useState(false);
  const [groupError, setGroupError] = useState<string | null>(null);
  const [memberError, setMemberError] = useState<string | null>(null);
  const [permissionLevel, setPermissionLevel] = useState<"all_members" | "admin_only">("all_members");

  // --- DM handlers (preserved exactly) ---

  const handleCreateDm = useCallback(async () => {
    const trimmed = address.trim();
    const valErr = validateAddress(trimmed);
    setTouched(true);
    if (valErr) {
      setError(null);
      return;
    }

    const client = getClient();
    if (!client) {
      setError("XMTP client not initialised. Please log in again.");
      return;
    }

    setLoading(true);
    setError(null);

    try {
      const peerIdentity = new PublicIdentity(trimmed, "ETHEREUM");
      const canMessageResult = await client.canMessage([peerIdentity]);
      const canReach = Object.values(canMessageResult)[0] ?? false;
      if (!canReach) {
        setError(
          "This address is not registered on the XMTP network. The recipient must activate XMTP first."
        );
        setLoading(false);
        return;
      }

      const dm = await client.conversations.findOrCreateDmWithIdentity(peerIdentity);
      const item = await conversationToItem(dm, client.inboxId);
      upsert(item);
      router.replace(`/conversation/${dm.id}` as any);
    } catch (err: any) {
      console.error("[NewConversation] create failed:", err);
      setError(err?.message ?? "Failed to create conversation");
    } finally {
      setLoading(false);
    }
  }, [address, upsert, router]);

  // --- Group handlers ---

  const handleAddMember = useCallback(() => {
    const trimmed = memberInput.trim();
    setMemberError(null);

    if (!trimmed) return;

    if (!isValidEthAddress(trimmed)) {
      setMemberError("Invalid Ethereum address");
      return;
    }

    if (members.some((a) => a.toLowerCase() === trimmed.toLowerCase())) {
      setMemberError("Address already added");
      return;
    }

    setMembers((prev) => [...prev, trimmed]);
    setMemberInput("");
  }, [memberInput, members]);

  const handleRemoveMember = useCallback((addr: string) => {
    setMembers((prev) => prev.filter((a) => a !== addr));
  }, []);

  const handleCreateGroup = useCallback(async () => {
    if (members.length === 0) return;

    setGroupLoading(true);
    setGroupError(null);

    try {
      const result = await createGroup(members, {
        name: groupName.trim() || undefined,
        permissionLevel,
      });

      if (!result.ok) {
        setGroupError(result.error);
        setGroupLoading(false);
        return;
      }

      // Find the new group and add to conversation store
      const client = getClient();
      if (client) {
        const newGroup = await client.conversations.findGroup(result.data as any);
        if (newGroup) {
          const item = await conversationToItem(newGroup, client.inboxId);
          upsert(item);
        }
      }

      router.replace(`/conversation/${result.data}` as any);
    } catch (err: any) {
      console.error("[NewConversation] group create failed:", err);
      setGroupError(err?.message ?? "Failed to create group");
    } finally {
      setGroupLoading(false);
    }
  }, [members, groupName, permissionLevel, upsert, router]);

  const handleBack = useCallback(() => {
    router.back();
  }, [router]);

  return (
    <KeyboardAvoidingView
      style={styles.container}
      behavior={Platform.OS === "ios" ? "padding" : undefined}
    >
      {/* AppBar */}
      <Appbar.Header style={styles.appbar} elevated>
        <Appbar.BackAction onPress={handleBack} iconColor="#E6E1E5" />
        <Appbar.Content title="New Conversation" titleStyle={styles.appbarTitle} />
      </Appbar.Header>

      <ScrollView
        style={styles.scrollView}
        contentContainerStyle={styles.scrollContent}
        keyboardShouldPersistTaps="handled"
      >
        {/* Mode toggle */}
        <SegmentedButtons
          value={mode}
          onValueChange={(v) => {
            setMode(v as "dm" | "group");
            setError(null);
            setGroupError(null);
            setMemberError(null);
          }}
          buttons={[
            {
              value: "dm",
              label: "Direct Message",
              checkedColor: "#D0BCFF",
              uncheckedColor: "#938F99",
            },
            {
              value: "group",
              label: "Group",
              checkedColor: "#D0BCFF",
              uncheckedColor: "#938F99",
            },
          ]}
          style={styles.segmentedButtons}
          density="regular"
        />

        {/* ============================================================= */}
        {/* DM Mode                                                        */}
        {/* ============================================================= */}
        {mode === "dm" && (
          <View>
            <Text variant="bodyMedium" style={styles.description}>
              Enter an Ethereum address to start a direct message conversation.
            </Text>

            {/* Address input */}
            <TextInput
              label="ETH Address"
              placeholder="0x..."
              value={address}
              onChangeText={(text) => {
                setAddress(text);
                if (error) setError(null);
              }}
              onBlur={() => setTouched(true)}
              mode="outlined"
              autoCapitalize="none"
              autoCorrect={false}
              style={styles.input}
              contentStyle={styles.inputContent}
              outlineColor="#49454F"
              activeOutlineColor="#D0BCFF"
              textColor="#E6E1E5"
              placeholderTextColor="#938F99"
              disabled={loading}
              error={!!(validationError || error)}
            />

            {/* Validation helper text */}
            {validationError && (
              <HelperText type="error" visible style={styles.helperText}>
                {validationError}
              </HelperText>
            )}

            {/* API error */}
            {error && !validationError && (
              <HelperText type="error" visible style={styles.helperText}>
                {error}
              </HelperText>
            )}

            {/* Submit button */}
            <Button
              mode="contained"
              onPress={handleCreateDm}
              loading={loading}
              disabled={loading}
              style={styles.button}
              contentStyle={styles.buttonContent}
              labelStyle={styles.buttonLabel}
              icon={loading ? undefined : "message-plus"}
            >
              {loading ? "Creating..." : "Start Conversation"}
            </Button>
          </View>
        )}

        {/* ============================================================= */}
        {/* Group Mode                                                     */}
        {/* ============================================================= */}
        {mode === "group" && (
          <View>
            {/* Group name */}
            <TextInput
              label="Group Name"
              placeholder="Group name (optional)"
              value={groupName}
              onChangeText={setGroupName}
              mode="outlined"
              autoCapitalize="sentences"
              autoCorrect={false}
              style={styles.input}
              outlineColor="#49454F"
              activeOutlineColor="#D0BCFF"
              textColor="#E6E1E5"
              placeholderTextColor="#938F99"
              disabled={groupLoading}
            />

            {/* Members section */}
            <Text variant="titleSmall" style={styles.sectionTitle}>
              Members
            </Text>

            {/* Address input row */}
            <View style={styles.memberInputRow}>
              <TextInput
                label="ETH Address"
                placeholder="0x..."
                value={memberInput}
                onChangeText={(text) => {
                  setMemberInput(text);
                  if (memberError) setMemberError(null);
                }}
                onSubmitEditing={handleAddMember}
                mode="outlined"
                autoCapitalize="none"
                autoCorrect={false}
                style={styles.memberInput}
                contentStyle={styles.inputContent}
                outlineColor="#49454F"
                activeOutlineColor="#D0BCFF"
                textColor="#E6E1E5"
                placeholderTextColor="#938F99"
                disabled={groupLoading}
                error={!!memberError}
              />
              <Button
                mode="contained-tonal"
                onPress={handleAddMember}
                disabled={groupLoading || !memberInput.trim()}
                style={styles.addButton}
                labelStyle={styles.addButtonLabel}
                compact
              >
                Add
              </Button>
            </View>

            {memberError && (
              <HelperText type="error" visible style={styles.helperText}>
                {memberError}
              </HelperText>
            )}

            {/* Member chips */}
            {members.length > 0 && (
              <View style={styles.chipContainer}>
                {members.map((addr) => (
                  <Chip
                    key={addr}
                    onClose={() => handleRemoveMember(addr)}
                    style={styles.chip}
                    textStyle={styles.chipText}
                    disabled={groupLoading}
                  >
                    {shortenAddress(addr)}
                  </Chip>
                ))}
              </View>
            )}

            {members.length === 0 && (
              <Text variant="bodySmall" style={styles.hint}>
                Add at least one member to create a group.
              </Text>
            )}

            {/* Permission level */}
            <Text variant="titleSmall" style={styles.sectionTitle}>
              Permission Level
            </Text>

            <RadioButton.Group
              value={permissionLevel}
              onValueChange={(v) => setPermissionLevel(v as "all_members" | "admin_only")}
            >
              <View style={styles.radioRow}>
                <RadioButton.Android
                  value="all_members"
                  color="#D0BCFF"
                  uncheckedColor="#938F99"
                  disabled={groupLoading}
                />
                <Text
                  style={styles.radioLabel}
                  onPress={() => !groupLoading && setPermissionLevel("all_members")}
                >
                  All Members — anyone can manage
                </Text>
              </View>
              <View style={styles.radioRow}>
                <RadioButton.Android
                  value="admin_only"
                  color="#D0BCFF"
                  uncheckedColor="#938F99"
                  disabled={groupLoading}
                />
                <Text
                  style={styles.radioLabel}
                  onPress={() => !groupLoading && setPermissionLevel("admin_only")}
                >
                  Admin Only — only admins can manage
                </Text>
              </View>
            </RadioButton.Group>

            {/* Group error */}
            {groupError && (
              <HelperText type="error" visible style={styles.helperText}>
                {groupError}
              </HelperText>
            )}

            {/* Create button */}
            <Button
              mode="contained"
              onPress={handleCreateGroup}
              loading={groupLoading}
              disabled={groupLoading || members.length === 0}
              style={styles.button}
              contentStyle={styles.buttonContent}
              labelStyle={styles.buttonLabel}
              icon={groupLoading ? undefined : "account-group"}
            >
              {groupLoading ? "Creating..." : "Create Group"}
            </Button>
          </View>
        )}
      </ScrollView>
    </KeyboardAvoidingView>
  );
}

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: "#1a1a2e",
  },
  appbar: {
    backgroundColor: "#1a1a2e",
  },
  appbarTitle: {
    color: "#E6E1E5",
    fontWeight: "700",
  },
  scrollView: {
    flex: 1,
  },
  scrollContent: {
    paddingHorizontal: 24,
    paddingTop: 24,
    paddingBottom: 48,
  },
  segmentedButtons: {
    marginBottom: 24,
  },
  description: {
    color: "#938F99",
    marginBottom: 24,
  },
  input: {
    backgroundColor: "#1a1a2e",
  },
  inputContent: {
    fontFamily: Platform.OS === "ios" ? "Menlo" : "monospace",
    fontSize: 14,
  },
  helperText: {
    paddingHorizontal: 0,
  },
  button: {
    marginTop: 24,
    borderRadius: 20,
  },
  buttonContent: {
    paddingVertical: 6,
  },
  buttonLabel: {
    fontSize: 16,
    fontWeight: "600",
  },
  sectionTitle: {
    color: "#E6E1E5",
    marginTop: 20,
    marginBottom: 8,
  },
  memberInputRow: {
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
  },
  memberInput: {
    flex: 1,
    backgroundColor: "#1a1a2e",
  },
  addButton: {
    marginTop: 6,
    borderRadius: 16,
  },
  addButtonLabel: {
    fontSize: 14,
    fontWeight: "600",
  },
  chipContainer: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: 8,
    marginTop: 12,
  },
  chip: {
    backgroundColor: "#2B2930",
  },
  chipText: {
    color: "#E6E1E5",
    fontFamily: Platform.OS === "ios" ? "Menlo" : "monospace",
    fontSize: 13,
  },
  hint: {
    color: "#938F99",
    marginTop: 8,
  },
  radioRow: {
    flexDirection: "row",
    alignItems: "center",
    marginVertical: 2,
  },
  radioLabel: {
    color: "#E6E1E5",
    fontSize: 14,
    flex: 1,
  },
});
