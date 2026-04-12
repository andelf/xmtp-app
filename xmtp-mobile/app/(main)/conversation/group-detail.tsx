/**
 * Group Detail Screen -- group info, member management, and admin controls.
 *
 * Accessible from the Chat Screen header info button for group conversations.
 */
import React, { useCallback, useEffect, useMemo, useState } from "react";
import { View, StyleSheet, ScrollView, Alert, Clipboard } from "react-native";
import { Text, Divider, ActivityIndicator, Avatar, Button, List } from "react-native-paper";
import { Stack, useLocalSearchParams, useRouter } from "expo-router";

import { useConversationStore } from "../../../src/store/conversations";
import {
  getGroupInfo,
  getGroupMembers,
  getMyRole,
  updateGroupName,
  updateGroupDescription,
  leaveGroup,
  promoteToAdmin,
  demoteAdmin,
  removeMembers,
  type GroupInfo,
  type GroupMember,
  type PermissionLevel,
} from "../../../src/xmtp/groups";
import { useAuthStore } from "../../../src/store/auth";
import { EditableField } from "../../../src/components/EditableField";
import { MemberRow } from "../../../src/components/MemberRow";
import { MemberActionSheet } from "../../../src/components/MemberActionSheet";
import { InfoRow } from "../../../src/components/InfoRow";
import { ScreenHeader } from "../../../src/components/ScreenHeader";
import { formatDateTime } from "../../../src/utils/time";

const HEADER_OPTIONS = {
  headerShown: false,
} as const;

export default function GroupDetailScreen() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const router = useRouter();

  const conversation = useConversationStore((s) => (id ? s.items.get(id) : undefined));
  const myInboxId = useAuthStore((s) => s.inboxId);

  // ---------------------------------------------------------------------------
  // State
  // ---------------------------------------------------------------------------
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<GroupInfo | null>(null);
  const [members, setMembers] = useState<GroupMember[]>([]);
  const [myRole, setMyRole] = useState<PermissionLevel>("member");

  // Action sheet
  const [sheetMember, setSheetMember] = useState<GroupMember | null>(null);
  const [sheetVisible, setSheetVisible] = useState(false);

  // ---------------------------------------------------------------------------
  // Load data
  // ---------------------------------------------------------------------------
  const loadData = useCallback(async () => {
    if (!id) return;
    setLoading(true);
    setError(null);

    const [infoRes, membersRes, roleRes] = await Promise.all([
      getGroupInfo(id),
      getGroupMembers(id),
      getMyRole(id),
    ]);

    if (!infoRes.ok) {
      setError(infoRes.error);
      setLoading(false);
      return;
    }

    setInfo(infoRes.data);
    if (membersRes.ok) setMembers(membersRes.data);
    if (roleRes.ok) setMyRole(roleRes.data);
    setLoading(false);
  }, [id]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  // ---------------------------------------------------------------------------
  // Permission helpers
  // ---------------------------------------------------------------------------
  const policies = info?.policies;
  const canEditName =
    myRole === "super_admin" || myRole === "admin" || policies?.updateGroupNamePolicy === "allow";
  const canEditDescription =
    myRole === "super_admin" ||
    myRole === "admin" ||
    policies?.updateGroupDescriptionPolicy === "allow";
  const canAddMember =
    myRole === "super_admin" || myRole === "admin" || policies?.addMemberPolicy === "allow";

  // ---------------------------------------------------------------------------
  // Handlers
  // ---------------------------------------------------------------------------
  const handleSaveName = useCallback(
    async (newName: string) => {
      if (!id) return;
      const res = await updateGroupName(id, newName);
      if (!res.ok) Alert.alert("Error", res.error);
      else {
        setInfo((prev) => (prev ? { ...prev, name: newName } : prev));
        // Update conversation store title
        const store = useConversationStore.getState();
        const item = store.items.get(id);
        if (item) store.upsert({ ...item, title: newName || "Unnamed Group" });
      }
    },
    [id]
  );

  const handleSaveDescription = useCallback(
    async (newDesc: string) => {
      if (!id) return;
      const res = await updateGroupDescription(id, newDesc);
      if (!res.ok) Alert.alert("Error", res.error);
      else setInfo((prev) => (prev ? { ...prev, description: newDesc } : prev));
    },
    [id]
  );

  const handleLeaveGroup = useCallback(() => {
    if (!id) return;
    Alert.alert(
      "Leave Group",
      "Are you sure? You won't receive messages from this group anymore.",
      [
        { text: "Cancel", style: "cancel" },
        {
          text: "Leave",
          style: "destructive",
          onPress: async () => {
            const res = await leaveGroup(id);
            if (!res.ok) {
              Alert.alert("Error", res.error);
              return;
            }
            useConversationStore.getState().remove(id);
            router.replace("/(main)/conversations");
          },
        },
      ]
    );
  }, [id, router]);

  const handleCopyAddress = useCallback((addr: string) => {
    Clipboard.setString(addr);
  }, []);

  const handleMemberLongPress = useCallback((member: GroupMember) => {
    setSheetMember(member);
    setSheetVisible(true);
  }, []);

  const handlePromoteAdmin = useCallback(
    async (inboxId: string) => {
      if (!id) return;
      const res = await promoteToAdmin(id, inboxId);
      if (!res.ok) Alert.alert("Error", res.error);
      else loadData(); // Refresh to show updated roles
    },
    [id, loadData]
  );

  const handleDemoteAdmin = useCallback(
    async (inboxId: string) => {
      if (!id) return;
      const res = await demoteAdmin(id, inboxId);
      if (!res.ok) Alert.alert("Error", res.error);
      else loadData();
    },
    [id, loadData]
  );

  const handleRemoveMember = useCallback(
    (inboxId: string) => {
      if (!id) return;
      Alert.alert("Remove Member", "Remove this member from the group?", [
        { text: "Cancel", style: "cancel" },
        {
          text: "Remove",
          style: "destructive",
          onPress: async () => {
            const res = await removeMembers(id, [inboxId]);
            if (!res.ok) Alert.alert("Error", res.error);
            else loadData();
          },
        },
      ]);
    },
    [id, loadData]
  );

  const handleAddMember = useCallback(() => {
    if (!id) return;
    router.push({ pathname: "/(main)/conversation/add-member", params: { id } });
  }, [id, router]);

  // ---------------------------------------------------------------------------
  // Render
  // ---------------------------------------------------------------------------
  const creatorInboxId = info?.creatorInboxId ?? "";
  const creatorMember = members.find((m) => m.inboxId === creatorInboxId);

  const sortedMembers = useMemo(() => {
    const order: Record<PermissionLevel, number> = { super_admin: 0, admin: 1, member: 2 };
    return [...members].sort(
      (a, b) => (order[a.permissionLevel] ?? 2) - (order[b.permissionLevel] ?? 2)
    );
  }, [members]);

  if (loading) {
    return (
      <>
        <Stack.Screen options={HEADER_OPTIONS} />
        <View style={styles.container}>
          <ScreenHeader title="Group Details" />
          <View style={styles.loadingContainer}>
            <ActivityIndicator size="large" color="#6750A4" />
          </View>
        </View>
      </>
    );
  }

  if (error) {
    return (
      <>
        <Stack.Screen options={HEADER_OPTIONS} />
        <View style={styles.container}>
          <ScreenHeader title="Group Details" />
          <View style={styles.loadingContainer}>
            <Text variant="bodyMedium" style={styles.errorText}>
              {error}
            </Text>
          </View>
        </View>
      </>
    );
  }

  return (
    <>
      <Stack.Screen options={HEADER_OPTIONS} />
      <View style={styles.container}>
        <ScreenHeader title="Group Details" />
        <ScrollView style={styles.scrollView} contentContainerStyle={styles.content}>
          {/* Header: Avatar + Name + Description */}
          <View style={styles.header}>
            <Avatar.Text
              size={64}
              label={(info?.name || "G").slice(0, 2).toUpperCase()}
              style={styles.avatar}
              labelStyle={styles.avatarLabel}
            />
          </View>

          <EditableField
            label="Group Name"
            value={info?.name || "Unnamed Group"}
            placeholder="Enter group name"
            editable={canEditName}
            onSave={handleSaveName}
          />

          <EditableField
            label="Description"
            value={info?.description || ""}
            placeholder="Add a description"
            editable={canEditDescription}
            multiline
            onSave={handleSaveDescription}
          />

          <Text variant="bodySmall" style={styles.memberCount}>
            {members.length} member{members.length !== 1 ? "s" : ""}
          </Text>

          <Divider style={styles.divider} />

          {/* Members */}
          <Text variant="titleMedium" style={styles.sectionTitle}>
            Members
          </Text>

          {canAddMember && (
            <List.Item
              title="Add Member"
              titleStyle={styles.addMemberTitle}
              left={(props) => <List.Icon {...props} icon="account-plus" color="#6750A4" />}
              onPress={handleAddMember}
              style={styles.addMemberRow}
            />
          )}

          {sortedMembers.map((member) => (
            <MemberRow
              key={member.inboxId}
              member={member}
              isCreator={member.inboxId === creatorInboxId}
              isMe={member.inboxId === myInboxId}
              onPress={() => handleCopyAddress(member.address)}
              onLongPress={() => handleMemberLongPress(member)}
            />
          ))}

          <Divider style={styles.divider} />

          {/* Group Info */}
          <Text variant="titleMedium" style={styles.sectionTitle}>
            Group Info
          </Text>

          {creatorMember && <InfoRow label="Created by" value={creatorMember.address} />}
          <InfoRow
            label="Created At"
            value={conversation ? formatDateTime(conversation.createdAt) : null}
          />
          <InfoRow label="Conversation ID" value={id ?? null} numberOfLines={3} />

          <Divider style={styles.divider} />

          {/* Leave Group */}
          <Button
            mode="outlined"
            textColor="#F2B8B5"
            style={styles.leaveButton}
            icon="logout"
            onPress={handleLeaveGroup}
          >
            Leave Group
          </Button>

          <View style={styles.bottomSpacer} />
        </ScrollView>
      </View>

      {/* Member action sheet */}
      <MemberActionSheet
        visible={sheetVisible}
        member={sheetMember}
        myRole={myRole}
        onDismiss={() => setSheetVisible(false)}
        onCopyAddress={handleCopyAddress}
        onPromoteAdmin={handlePromoteAdmin}
        onDemoteAdmin={handleDemoteAdmin}
        onRemoveMember={handleRemoveMember}
      />
    </>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: "#1a1a2e",
  },
  scrollView: {
    flex: 1,
    backgroundColor: "#1a1a2e",
  },
  content: {
    padding: 20,
  },
  loadingContainer: {
    flex: 1,
    justifyContent: "center",
    alignItems: "center",
  },
  errorText: {
    color: "#F2B8B5",
  },
  header: {
    alignItems: "center",
    marginBottom: 20,
  },
  avatar: {
    backgroundColor: "#6750A4",
  },
  avatarLabel: {
    fontSize: 24,
    fontWeight: "700",
    color: "#E6E1E5",
  },
  memberCount: {
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
  addMemberRow: {
    marginBottom: 4,
    borderRadius: 8,
    backgroundColor: "rgba(103, 80, 164, 0.1)",
  },
  addMemberTitle: {
    color: "#6750A4",
    fontWeight: "600",
  },
  leaveButton: {
    borderColor: "#F2B8B5",
    borderRadius: 12,
  },
  bottomSpacer: {
    height: 40,
  },
});
