/**
 * New conversation page -- supports both DM and Group creation modes.
 *
 * Accepts ?mode=group URL param to pre-select Group tab.
 * Group mode uses a Card Stack layout (Material Design 3 style).
 */
import React, { useCallback, useState } from "react";
import { View, StyleSheet, Platform, ScrollView, Pressable } from "react-native";
import { KeyboardAwareScrollView } from "react-native-keyboard-controller";
import {
  Appbar,
  TextInput,
  Button,
  Text,
  HelperText,
  SegmentedButtons,
  Avatar,
  IconButton,
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
// Permission Option Card
// ---------------------------------------------------------------------------

function PermissionCard({
  icon,
  title,
  subtitle,
  selected,
  onPress,
  disabled,
}: {
  icon: string;
  title: string;
  subtitle: string;
  selected: boolean;
  onPress: () => void;
  disabled?: boolean;
}) {
  return (
    <Pressable
      onPress={disabled ? undefined : onPress}
      style={[
        styles.permCard,
        selected && styles.permCardSelected,
        disabled && styles.permCardDisabled,
      ]}
    >
      <View style={styles.permCardHeader}>
        <IconButton
          icon={icon}
          size={20}
          iconColor={selected ? "#D0BCFF" : "#938F99"}
          style={styles.permCardIcon}
        />
        {selected && (
          <IconButton icon="check-circle" size={16} iconColor="#D0BCFF" style={styles.permCheck} />
        )}
      </View>
      <Text variant="titleSmall" style={[styles.permTitle, selected && styles.permTitleSelected]}>
        {title}
      </Text>
      <Text variant="bodySmall" style={styles.permSubtitle}>
        {subtitle}
      </Text>
    </Pressable>
  );
}

// ---------------------------------------------------------------------------
// Member Row
// ---------------------------------------------------------------------------

function MemberItem({
  address,
  onRemove,
  disabled,
}: {
  address: string;
  onRemove: () => void;
  disabled?: boolean;
}) {
  const initials = address.slice(2, 4).toUpperCase();
  return (
    <View style={styles.memberItem}>
      <Avatar.Text
        size={36}
        label={initials}
        style={styles.memberAvatar}
        labelStyle={styles.memberAvatarLabel}
      />
      <View style={styles.memberInfo}>
        <Text variant="bodyMedium" style={styles.memberAddr} numberOfLines={1}>
          {shortenAddress(address)}
        </Text>
        <Text variant="bodySmall" style={styles.memberFull} numberOfLines={1}>
          {address}
        </Text>
      </View>
      <IconButton
        icon="close-circle-outline"
        size={20}
        iconColor="#938F99"
        onPress={disabled ? undefined : onRemove}
        style={styles.memberRemove}
      />
    </View>
  );
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
  const [permissionLevel, setPermissionLevel] = useState<"all_members" | "admin_only">(
    "all_members"
  );

  // --- DM handlers ---

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
    <View style={styles.container}>
      {/* AppBar */}
      <Appbar.Header style={styles.appbar} elevated>
        <Appbar.BackAction onPress={handleBack} iconColor="#E6E1E5" />
        <Appbar.Content title="New Conversation" titleStyle={styles.appbarTitle} />
      </Appbar.Header>

      <KeyboardAwareScrollView
        style={styles.scrollView}
        contentContainerStyle={styles.scrollContent}
        keyboardShouldPersistTaps="handled"
        bottomOffset={20}
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

            {validationError && (
              <HelperText type="error" visible style={styles.helperText}>
                {validationError}
              </HelperText>
            )}

            {error && !validationError && (
              <HelperText type="error" visible style={styles.helperText}>
                {error}
              </HelperText>
            )}

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
        {/* Group Mode — Card Stack Layout                                 */}
        {/* ============================================================= */}
        {mode === "group" && (
          <View>
            {/* Card 1: Group Info */}
            <View style={styles.card}>
              <Text variant="titleSmall" style={styles.cardTitle}>
                Group Info
              </Text>

              <TextInput
                label="Group Name"
                placeholder="e.g. XMTP Builders"
                value={groupName}
                onChangeText={setGroupName}
                mode="outlined"
                autoCapitalize="sentences"
                autoCorrect={false}
                style={styles.cardInput}
                outlineColor="#49454F"
                activeOutlineColor="#D0BCFF"
                textColor="#E6E1E5"
                placeholderTextColor="#938F99"
                disabled={groupLoading}
                left={<TextInput.Icon icon="pencil-outline" color="#938F99" />}
              />

              {/* Permission cards */}
              <Text variant="bodySmall" style={styles.permLabel}>
                Permissions
              </Text>
              <View style={styles.permRow}>
                <PermissionCard
                  icon="earth"
                  title="Open"
                  subtitle="All members can manage"
                  selected={permissionLevel === "all_members"}
                  onPress={() => setPermissionLevel("all_members")}
                  disabled={groupLoading}
                />
                <PermissionCard
                  icon="shield-lock-outline"
                  title="Admin"
                  subtitle="Only admins can manage"
                  selected={permissionLevel === "admin_only"}
                  onPress={() => setPermissionLevel("admin_only")}
                  disabled={groupLoading}
                />
              </View>
            </View>

            {/* Card 2: Members */}
            <View style={styles.card}>
              <View style={styles.cardTitleRow}>
                <Text variant="titleSmall" style={styles.cardTitle}>
                  Members
                </Text>
                {members.length > 0 && (
                  <View style={styles.badge}>
                    <Text variant="labelSmall" style={styles.badgeText}>
                      {members.length}
                    </Text>
                  </View>
                )}
              </View>

              {/* Search-style input */}
              <TextInput
                placeholder="Add by ETH address"
                value={memberInput}
                onChangeText={(text) => {
                  setMemberInput(text);
                  if (memberError) setMemberError(null);
                }}
                onSubmitEditing={handleAddMember}
                mode="outlined"
                autoCapitalize="none"
                autoCorrect={false}
                style={styles.cardInput}
                contentStyle={styles.inputContent}
                outlineColor="#49454F"
                activeOutlineColor="#D0BCFF"
                textColor="#E6E1E5"
                placeholderTextColor="#938F99"
                disabled={groupLoading}
                error={!!memberError}
                left={<TextInput.Icon icon="magnify" color="#938F99" />}
                right={
                  memberInput.trim() ? (
                    <TextInput.Icon icon="plus-circle" color="#D0BCFF" onPress={handleAddMember} />
                  ) : undefined
                }
              />

              {memberError && (
                <HelperText type="error" visible style={styles.helperText}>
                  {memberError}
                </HelperText>
              )}

              {/* Member list */}
              {members.length > 0 ? (
                <View style={styles.memberList}>
                  {members.map((addr) => (
                    <MemberItem
                      key={addr}
                      address={addr}
                      onRemove={() => handleRemoveMember(addr)}
                      disabled={groupLoading}
                    />
                  ))}
                </View>
              ) : (
                <View style={styles.emptyMembers}>
                  <IconButton icon="account-plus-outline" size={32} iconColor="#49454F" />
                  <Text variant="bodySmall" style={styles.emptyText}>
                    Add at least one member to create a group
                  </Text>
                </View>
              )}
            </View>

            {/* Group error */}
            {groupError && (
              <HelperText type="error" visible style={styles.helperText}>
                {groupError}
              </HelperText>
            )}

            {/* Create button — full width, elevated feel */}
            <Button
              mode="contained"
              onPress={handleCreateGroup}
              loading={groupLoading}
              disabled={groupLoading || members.length === 0}
              style={styles.createButton}
              contentStyle={styles.createButtonContent}
              labelStyle={styles.buttonLabel}
              icon={groupLoading ? undefined : "account-group"}
            >
              {groupLoading ? "Creating..." : "Create Group"}
            </Button>
          </View>
        )}
      </KeyboardAwareScrollView>
    </View>
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
    paddingHorizontal: 20,
    paddingTop: 20,
    paddingBottom: 48,
  },
  segmentedButtons: {
    marginBottom: 20,
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

  // --- Card Stack ---
  card: {
    backgroundColor: "#16213e",
    borderRadius: 16,
    padding: 16,
    marginBottom: 16,
    borderWidth: 1,
    borderColor: "#1f2b47",
  },
  cardTitle: {
    color: "#E6E1E5",
    fontWeight: "600",
    marginBottom: 12,
  },
  cardTitleRow: {
    flexDirection: "row",
    alignItems: "center",
    marginBottom: 12,
    gap: 8,
  },
  cardInput: {
    backgroundColor: "#1a1a2e",
  },

  // --- Permission Cards ---
  permLabel: {
    color: "#938F99",
    marginTop: 16,
    marginBottom: 8,
  },
  permRow: {
    flexDirection: "row",
    gap: 10,
  },
  permCard: {
    flex: 1,
    backgroundColor: "#1a1a2e",
    borderRadius: 12,
    padding: 12,
    borderWidth: 1.5,
    borderColor: "#2B2930",
  },
  permCardSelected: {
    borderColor: "#6750A4",
    backgroundColor: "rgba(103, 80, 164, 0.08)",
  },
  permCardDisabled: {
    opacity: 0.5,
  },
  permCardHeader: {
    flexDirection: "row",
    justifyContent: "space-between",
    alignItems: "center",
    marginBottom: 4,
  },
  permCardIcon: {
    margin: 0,
  },
  permCheck: {
    margin: 0,
  },
  permTitle: {
    color: "#E6E1E5",
    fontWeight: "600",
  },
  permTitleSelected: {
    color: "#D0BCFF",
  },
  permSubtitle: {
    color: "#938F99",
    marginTop: 2,
  },

  // --- Member List ---
  badge: {
    backgroundColor: "#6750A4",
    borderRadius: 10,
    paddingHorizontal: 7,
    paddingVertical: 1,
    minWidth: 20,
    alignItems: "center",
  },
  badgeText: {
    color: "#FFFFFF",
    fontWeight: "700",
    fontSize: 11,
  },
  memberList: {
    marginTop: 12,
    gap: 4,
  },
  memberItem: {
    flexDirection: "row",
    alignItems: "center",
    paddingVertical: 8,
    paddingHorizontal: 8,
    borderRadius: 10,
    backgroundColor: "#1a1a2e",
  },
  memberAvatar: {
    backgroundColor: "#6750A4",
  },
  memberAvatarLabel: {
    fontSize: 13,
    fontWeight: "700",
    color: "#E6E1E5",
  },
  memberInfo: {
    flex: 1,
    marginLeft: 12,
  },
  memberAddr: {
    color: "#E6E1E5",
    fontWeight: "500",
  },
  memberFull: {
    color: "#938F99",
    fontFamily: Platform.OS === "ios" ? "Menlo" : "monospace",
    fontSize: 11,
    marginTop: 1,
  },
  memberRemove: {
    margin: 0,
  },
  emptyMembers: {
    alignItems: "center",
    paddingVertical: 20,
  },
  emptyText: {
    color: "#938F99",
    marginTop: 4,
  },

  // --- Create Button ---
  createButton: {
    marginTop: 8,
    borderRadius: 16,
  },
  createButtonContent: {
    paddingVertical: 8,
  },
});
