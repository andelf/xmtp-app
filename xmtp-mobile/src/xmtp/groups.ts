/**
 * Group management SDK operations.
 *
 * Wraps @xmtp/react-native-sdk Group methods with error handling.
 * All public functions return { ok, data?, error? } for consistent UI consumption.
 */
import type { Group } from "@xmtp/react-native-sdk/build/lib/Group";
import type { Member } from "@xmtp/react-native-sdk/build/lib/Member";
import type { PermissionPolicySet } from "@xmtp/react-native-sdk/build/lib/types/PermissionPolicySet";

import { findConversation } from "./messages";
import { getClient } from "./client";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type PermissionLevel = "member" | "admin" | "super_admin";

export interface GroupInfo {
  name: string;
  description: string;
  imageUrl: string;
  creatorInboxId: string;
  policies: PermissionPolicySet;
}

export interface GroupMember {
  inboxId: string;
  address: string;
  permissionLevel: PermissionLevel;
}

type Result<T = void> =
  | { ok: true; data: T }
  | { ok: false; error: string };

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async function findGroup(conversationId: string): Promise<Group | null> {
  const convo = await findConversation(conversationId);
  if (!convo) return null;
  return convo as Group;
}

function memberToGroupMember(m: Member): GroupMember {
  return {
    inboxId: m.inboxId as string,
    address: (m.identities?.[0] as any)?.identifier ?? (m.inboxId as string),
    permissionLevel: (m.permissionLevel as PermissionLevel) ?? "member",
  };
}

/** Run an action on a Group, returning a Result. Handles findGroup + error wrapping. */
async function withGroup<T = void>(
  conversationId: string,
  action: (group: Group) => Promise<T>,
  fallbackError: string
): Promise<Result<T>> {
  try {
    const group = await findGroup(conversationId);
    if (!group) return { ok: false, error: "Group not found" };
    const data = await action(group);
    return { ok: true, data };
  } catch (err: any) {
    return { ok: false, error: err?.message ?? fallbackError };
  }
}

// ---------------------------------------------------------------------------
// Read operations
// ---------------------------------------------------------------------------

export function getGroupInfo(conversationId: string): Promise<Result<GroupInfo>> {
  return withGroup(
    conversationId,
    async (group) => {
      await group.sync();
      const [name, description, imageUrl, creatorInboxId, policies] = await Promise.all([
        group.name(),
        group.description(),
        group.imageUrl(),
        group.creatorInboxId(),
        group.permissionPolicySet(),
      ]);
      return {
        name: name || "",
        description: description || "",
        imageUrl: imageUrl || "",
        creatorInboxId: creatorInboxId as string,
        policies,
      };
    },
    "Failed to load group info"
  );
}

export function getGroupMembers(conversationId: string): Promise<Result<GroupMember[]>> {
  return withGroup(
    conversationId,
    async (group) => {
      const raw = await group.members();
      return raw.map(memberToGroupMember);
    },
    "Failed to load members"
  );
}

export async function getMyRole(conversationId: string): Promise<Result<PermissionLevel>> {
  const client = getClient();
  if (!client) return { ok: false, error: "Client not initialised" };

  return withGroup(
    conversationId,
    async (group) => {
      const myInboxId = client.inboxId as string;
      const [isSuperAdmin, isAdmin] = await Promise.all([
        group.isSuperAdmin(myInboxId),
        group.isAdmin(myInboxId),
      ]);
      if (isSuperAdmin) return "super_admin";
      if (isAdmin) return "admin";
      return "member";
    },
    "Failed to check role"
  );
}

// ---------------------------------------------------------------------------
// Write operations
// ---------------------------------------------------------------------------

export function addMembers(conversationId: string, addresses: string[]): Promise<Result> {
  return withGroup(
    conversationId,
    (group) =>
      group.addMembersByIdentity(
        addresses.map((addr) => ({ kind: "ETHEREUM", identifier: addr }) as any)
      ) as Promise<any>,
    "Failed to add members"
  );
}

export function removeMembers(conversationId: string, inboxIds: string[]): Promise<Result> {
  return withGroup(conversationId, (g) => g.removeMembers(inboxIds), "Failed to remove members");
}

export function leaveGroup(conversationId: string): Promise<Result> {
  return withGroup(conversationId, (g) => g.leaveGroup(), "Failed to leave group");
}

export function updateGroupName(conversationId: string, name: string): Promise<Result> {
  return withGroup(conversationId, (g) => g.updateName(name), "Failed to update name");
}

export function updateGroupDescription(conversationId: string, description: string): Promise<Result> {
  return withGroup(conversationId, (g) => g.updateDescription(description), "Failed to update description");
}

export function promoteToAdmin(conversationId: string, inboxId: string): Promise<Result> {
  return withGroup(conversationId, (g) => g.addAdmin(inboxId), "Failed to promote");
}

export function demoteAdmin(conversationId: string, inboxId: string): Promise<Result> {
  return withGroup(conversationId, (g) => g.removeAdmin(inboxId), "Failed to demote");
}

export async function createGroup(
  addresses: string[],
  opts?: { name?: string; description?: string; permissionLevel?: "all_members" | "admin_only" }
): Promise<Result<string>> {
  try {
    const client = getClient();
    if (!client) return { ok: false, error: "Client not initialised" };

    const { PublicIdentity } = await import("@xmtp/react-native-sdk");
    const identities = addresses.map(
      (addr) => new PublicIdentity(addr, "ETHEREUM")
    );

    // Validate all addresses are on XMTP before creating
    const canMsg = await client.canMessage(identities);
    const unreachable = addresses.filter((_, i) => !Object.values(canMsg)[i]);
    if (unreachable.length > 0) {
      const short = unreachable.map((a) => `${a.slice(0, 6)}...${a.slice(-4)}`);
      return { ok: false, error: `Not on XMTP network: ${short.join(", ")}` };
    }

    const group = await client.conversations.newGroupWithIdentities(identities, {
      name: opts?.name || undefined,
      description: opts?.description || undefined,
      permissionLevel: opts?.permissionLevel,
    });

    return { ok: true, data: group.id as string };
  } catch (err: any) {
    return { ok: false, error: err?.message ?? "Failed to create group" };
  }
}
