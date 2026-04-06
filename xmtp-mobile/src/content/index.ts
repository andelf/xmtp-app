/**
 * Content type system: registry + all built-in handlers.
 *
 * Import this module once at startup to register all handlers.
 */
// Register all built-in handlers on import
import { registerContentType } from "./registry";
import { textHandler } from "./handlers/text";
import { replyHandler } from "./handlers/reply";
import { reactionHandler } from "./handlers/reaction";
import { readReceiptHandler } from "./handlers/readReceipt";
import { groupUpdatedHandler } from "./handlers/groupUpdated";
import { markdownHandler } from "./handlers/markdown";
import { actionsHandler, intentHandler } from "./handlers/actions";

export { registerContentType, decodeMessage, previewMessage } from "./registry";
export type {
  DecodeResult,
  ContentTypeHandler,
  DecodedMessageLike,
  ActionsPayload,
  ActionItem,
} from "./types";

registerContentType(textHandler);
registerContentType(replyHandler);
registerContentType(reactionHandler);
registerContentType(readReceiptHandler);
registerContentType(groupUpdatedHandler);
registerContentType(markdownHandler);
registerContentType(actionsHandler);
registerContentType(intentHandler);
