/**
 * MemberActionSheet -- bottom modal with context actions for a group member.
 *
 * Actions are conditionally shown based on the current user's role and
 * the target member's role.
 */
import React from "react";
import { View, StyleSheet } from "react-native";
import { Modal, Portal, List, Text, Divider } from "react-native-paper";

import type { PermissionLevel, GroupMember } from "../xmtp/groups";

export interface MemberActionSheetProps {
  visible: boolean;
  member: GroupMember | null;
  myRole: PermissionLevel;
  onDismiss: () => void;
  onCopyAddress: (address: string) => void;
  onPromoteAdmin?: (inboxId: string) => void;
  onDemoteAdmin?: (inboxId: string) => void;
  onRemoveMember?: (inboxId: string) => void;
}

export function MemberActionSheet({
  visible,
  member,
  myRole,
  onDismiss,
  onCopyAddress,
  onPromoteAdmin,
  onDemoteAdmin,
  onRemoveMember,
}: MemberActionSheetProps) {
  if (!member) return null;

  const canPromote =
    myRole === "super_admin" && member.permissionLevel === "member";
  const canDemote =
    myRole === "super_admin" && member.permissionLevel === "admin";
  const canRemove =
    (myRole === "super_admin" || myRole === "admin") &&
    member.permissionLevel !== "super_admin";

  return (
    <Portal>
      <Modal
        visible={visible}
        onDismiss={onDismiss}
        contentContainerStyle={styles.modal}
      >
        <Text variant="titleSmall" style={styles.title} numberOfLines={1}>
          {member.address}
        </Text>
        <Divider style={styles.divider} />

        <List.Item
          title="Copy Address"
          titleStyle={styles.itemTitle}
          left={(props) => <List.Icon {...props} icon="content-copy" color="#E6E1E5" />}
          onPress={() => {
            onCopyAddress(member.address);
            onDismiss();
          }}
        />

        {canPromote && onPromoteAdmin && (
          <List.Item
            title="Promote to Admin"
            titleStyle={styles.itemTitle}
            left={(props) => <List.Icon {...props} icon="shield-plus" color="#4A90D9" />}
            onPress={() => {
              onPromoteAdmin(member.inboxId);
              onDismiss();
            }}
          />
        )}

        {canDemote && onDemoteAdmin && (
          <List.Item
            title="Demote Admin"
            titleStyle={styles.itemTitle}
            left={(props) => <List.Icon {...props} icon="shield-remove" color="#F9A825" />}
            onPress={() => {
              onDemoteAdmin(member.inboxId);
              onDismiss();
            }}
          />
        )}

        {canRemove && onRemoveMember && (
          <List.Item
            title="Remove from Group"
            titleStyle={[styles.itemTitle, styles.dangerText]}
            left={(props) => <List.Icon {...props} icon="account-remove" color="#F2B8B5" />}
            onPress={() => {
              onRemoveMember(member.inboxId);
              onDismiss();
            }}
          />
        )}
      </Modal>
    </Portal>
  );
}

const styles = StyleSheet.create({
  modal: {
    backgroundColor: "#2B2930",
    marginHorizontal: 20,
    borderRadius: 16,
    paddingVertical: 16,
    paddingBottom: 8,
  },
  title: {
    color: "#938F99",
    textAlign: "center",
    paddingHorizontal: 20,
    marginBottom: 8,
  },
  divider: {
    backgroundColor: "#49454F",
    marginBottom: 4,
  },
  itemTitle: {
    color: "#E6E1E5",
  },
  dangerText: {
    color: "#F2B8B5",
  },
});
