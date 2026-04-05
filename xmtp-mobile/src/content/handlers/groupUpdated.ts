/**
 * Handler for group_updated content type.
 *
 * Renders membership changes (added/removed) and metadata updates
 * as system messages in the chat view.
 */
import { getNativeContent } from "../../utils/nativeContent";
import { shortenAddress } from "../../utils/address";
import type { ContentTypeHandler, DecodeResult, DecodedMessageLike } from "../types";

interface GroupUpdatedMemberEntry {
  inboxId: string;
}

interface GroupUpdatedMetadataEntry {
  fieldName: string;
  oldValue: string;
  newValue: string;
}

interface GroupUpdatedContent {
  initiatedByInboxId?: string;
  membersAdded?: GroupUpdatedMemberEntry[];
  membersRemoved?: GroupUpdatedMemberEntry[];
  metadataFieldsChanged?: GroupUpdatedMetadataEntry[];
}

function describeUpdate(update: GroupUpdatedContent): string | null {
  const parts: string[] = [];

  if (update.membersAdded?.length) {
    const names = update.membersAdded.map((m) => shortenAddress(m.inboxId)).join(", ");
    parts.push(`${names} joined the group`);
  }

  if (update.membersRemoved?.length) {
    const names = update.membersRemoved.map((m) => shortenAddress(m.inboxId)).join(", ");
    parts.push(`${names} left the group`);
  }

  if (update.metadataFieldsChanged?.length) {
    for (const field of update.metadataFieldsChanged) {
      if (field.fieldName === "group_name") {
        parts.push(`Group name changed to "${field.newValue}"`);
      } else if (field.fieldName === "description") {
        parts.push(`Description updated`);
      } else if (field.fieldName === "group_image_url") {
        parts.push(`Group image updated`);
      } else {
        parts.push(`${field.fieldName} updated`);
      }
    }
  }

  return parts.length > 0 ? parts.join("\n") : null;
}

export const groupUpdatedHandler: ContentTypeHandler = {
  typeId: "xmtp.org/group_updated:1.0",

  decode(msg: DecodedMessageLike): DecodeResult {
    const nc = getNativeContent(msg as any);
    if (!nc?.groupUpdated) return { kind: "skip" };

    const text = describeUpdate(nc.groupUpdated as GroupUpdatedContent);
    if (!text) return { kind: "skip" };

    return { kind: "message", text };
  },

  preview(msg: DecodedMessageLike): string | null {
    const nc = getNativeContent(msg as any);
    if (!nc?.groupUpdated) return null;
    return describeUpdate(nc.groupUpdated as GroupUpdatedContent);
  },
};
