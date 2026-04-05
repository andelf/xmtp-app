/**
 * MemberRow -- displays a single group member with role badge.
 *
 * Shows avatar, address, role chip, and creator indicator.
 * Long-press triggers the action sheet for admin operations.
 */
import React from "react";
import { View, StyleSheet, Pressable } from "react-native";
import { Avatar, Text } from "react-native-paper";

import type { PermissionLevel, GroupMember } from "../xmtp/groups";
import { shortenAddress } from "../utils/address";

export interface MemberRowProps {
  member: GroupMember;
  isCreator: boolean;
  isMe?: boolean;
  onPress?: () => void;
  onLongPress?: () => void;
}

function roleLabel(level: PermissionLevel): string | null {
  if (level === "super_admin") return "Owner";
  if (level === "admin") return "Admin";
  return null;
}

function roleColor(level: PermissionLevel): string {
  if (level === "super_admin") return "#6750A4";
  if (level === "admin") return "#4A90D9";
  return "transparent";
}

function avatarLabel(addr: string): string {
  if (addr.startsWith("0x") && addr.length >= 6) return addr.slice(2, 4).toUpperCase();
  return addr.slice(0, 2).toUpperCase();
}

export function MemberRow({ member, isCreator, isMe, onPress, onLongPress }: MemberRowProps) {
  const label = roleLabel(member.permissionLevel);
  const color = roleColor(member.permissionLevel);

  return (
    <Pressable
      onPress={onPress}
      onLongPress={onLongPress}
      style={({ pressed }) => [styles.container, pressed && styles.pressed]}
    >
      <Avatar.Text
        size={40}
        label={avatarLabel(member.address)}
        style={[styles.avatar, { backgroundColor: color === "transparent" ? "#49454F" : color }]}
        labelStyle={styles.avatarLabel}
      />

      <View style={styles.info}>
        <Text variant="bodyMedium" style={styles.address} numberOfLines={1}>
          {shortenAddress(member.address)}
        </Text>
        {isCreator && (
          <Text variant="bodySmall" style={styles.subtitle}>
            Creator
          </Text>
        )}
      </View>

      {isMe && (
        <View style={[styles.badge, styles.meBadge]}>
          <Text style={styles.badgeText}>Me</Text>
        </View>
      )}
      {label && (
        <View style={[styles.badge, { backgroundColor: color }, isMe && styles.badgeGap]}>
          <Text style={styles.badgeText}>{label}</Text>
        </View>
      )}
    </Pressable>
  );
}

const styles = StyleSheet.create({
  container: {
    flexDirection: "row",
    alignItems: "center",
    paddingVertical: 10,
    paddingHorizontal: 12,
    borderRadius: 8,
    marginBottom: 4,
  },
  pressed: {
    backgroundColor: "rgba(255,255,255,0.05)",
  },
  avatar: {
    marginRight: 12,
  },
  avatarLabel: {
    fontSize: 14,
    fontWeight: "600",
    color: "#E6E1E5",
  },
  info: {
    flex: 1,
  },
  address: {
    color: "#E6E1E5",
  },
  subtitle: {
    color: "#938F99",
    marginTop: 1,
  },
  badge: {
    paddingHorizontal: 10,
    paddingVertical: 4,
    borderRadius: 12,
    alignItems: "center",
    justifyContent: "center",
  },
  meBadge: {
    backgroundColor: "#3B7A57",
  },
  badgeGap: {
    marginLeft: 6,
  },
  badgeText: {
    color: "#E6E1E5",
    fontSize: 11,
    fontWeight: "600",
    lineHeight: 14,
  },
});
