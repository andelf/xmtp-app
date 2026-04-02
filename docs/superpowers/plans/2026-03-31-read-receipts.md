# Read Receipts (DM Only) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add read receipt support for DM conversations — show whether own messages have been read by the peer, with a global toggle controlling whether we send read receipts.

**Architecture:** Settings store gets a `readReceipts` boolean (default false) controlling sending only. Receiving/displaying is always on. A throttled `sendReadReceipt` function (max 1 per 3s per conversation) prevents message storms. Incoming read receipts (via `streamMessages`) update the referenced message's status to `"read"`. MessageBubble shows `○` (gray, published) or `●` (green, read) in the header area for own messages. Historical messages without read receipts stay as `published` — no backfill.

**Tech Stack:** React Native, Zustand, XMTP React Native SDK v5, expo-secure-store

**Risk Mitigations:**
- **Message storm**: Per-conversation throttle (3s) on sendReadReceipt — covers enter-conversation, stream, and reconnect scenarios
- **Infinite loop**: Read receipt check happens BEFORE append in stream callback — read receipts never trigger sending another read receipt
- **Historical compatibility**: fetchMessages does NOT scan for read receipts; only real-time stream updates status
- **Render storm**: markReadByPeer has early-return when no published own messages exist

---

### Task 1: Add readReceipts toggle to settings store

**Files:**
- Modify: `xmtp-mobile/src/store/settings.ts`

- [ ] **Step 1: Add readReceipts state and action to the store**

Add a new SecureStore key constant after the existing ones:

```typescript
const READ_RECEIPTS_KEY = "settings_read_receipts";
```

Add `readReceipts: boolean` to `SettingsState` interface.

Add `toggleReadReceipts: () => Promise<void>` to `SettingsActions` interface.

Update the store implementation — add default, load from SecureStore, and toggle action:

```typescript
export const useSettingsStore = create<SettingsStore>((set, get) => ({
  quickReactions: DEFAULT_REACTIONS,
  readReceipts: false,
  isLoaded: false,

  load: async () => {
    try {
      const raw = await SecureStore.getItemAsync(REACTIONS_KEY);
      let quickReactions = DEFAULT_REACTIONS;
      if (raw) {
        const parsed = JSON.parse(raw);
        if (Array.isArray(parsed) && parsed.length > 0) {
          quickReactions = parsed;
        }
      }
      const rrRaw = await SecureStore.getItemAsync(READ_RECEIPTS_KEY);
      const readReceipts = rrRaw === "true";

      set({ quickReactions, readReceipts, isLoaded: true });
    } catch {
      set({ isLoaded: true });
    }
  },

  toggleReadReceipts: async () => {
    const next = !get().readReceipts;
    set({ readReceipts: next });
    await SecureStore.setItemAsync(READ_RECEIPTS_KEY, String(next));
  },

  setQuickReactions: async (reactions: string[]) => {
    const valid = reactions.filter((r) => r.length > 0 && r.length <= 4);
    if (valid.length === 0) return;
    set({ quickReactions: valid });
    await SecureStore.setItemAsync(REACTIONS_KEY, JSON.stringify(valid));
  },
}));
```

- [ ] **Step 2: Verify build**

Run: `cd xmtp-mobile && npx tsc --noEmit`
Expected: No type errors

- [ ] **Step 3: Commit**

```bash
git add xmtp-mobile/src/store/settings.ts
git commit -m "feat(mobile): add readReceipts toggle to settings store"
```

---

### Task 2: Add throttled sendReadReceipt function

**Files:**
- Modify: `xmtp-mobile/src/xmtp/messages.ts`

- [ ] **Step 1: Add sendReadReceipt with per-conversation throttle**

Add at the end of `messages.ts`:

```typescript
/**
 * Send a read receipt for a conversation.
 * Uses NativeMessageContent { readReceipt: {} } — the native bridge handles it.
 */
async function sendReadReceiptRaw(conversationId: string): Promise<boolean> {
  try {
    const convo = await findConversation(conversationId);
    if (!convo) return false;

    await convo.send({ readReceipt: {} } as any);
    return true;
  } catch (err) {
    console.error("[sendReadReceipt] Failed:", err);
    return false;
  }
}

/**
 * Throttled read receipt sender — max 1 per THROTTLE_MS per conversation.
 * Prevents message storms when stream replays multiple messages on connect/reconnect.
 */
const READ_RECEIPT_THROTTLE_MS = 3000;
const lastReadReceiptSent = new Map<string, number>();

export function sendReadReceipt(conversationId: string): Promise<boolean> {
  const now = Date.now();
  const last = lastReadReceiptSent.get(conversationId) ?? 0;
  if (now - last < READ_RECEIPT_THROTTLE_MS) {
    return Promise.resolve(false);
  }
  lastReadReceiptSent.set(conversationId, now);
  return sendReadReceiptRaw(conversationId);
}
```

- [ ] **Step 2: Verify build**

Run: `cd xmtp-mobile && npx tsc --noEmit`
Expected: No type errors

- [ ] **Step 3: Commit**

```bash
git add xmtp-mobile/src/xmtp/messages.ts
git commit -m "feat(mobile): add throttled sendReadReceipt function (3s per conversation)"
```

---

### Task 3: Extract ReadReceiptInfo from incoming messages and add markReadByPeer

**Files:**
- Modify: `xmtp-mobile/src/utils/messageDecoder.ts`
- Modify: `xmtp-mobile/src/store/messages.ts`

- [ ] **Step 1: Add decodedToReadReceipt extractor in messageDecoder.ts**

Add after `decodedToReaction`:

```typescript
export interface ReadReceiptInfo {
  conversationId: string;
  senderInboxId: string;
}

/**
 * Extract a read receipt from a DecodedMessage, if it is one.
 * Returns null for non-read-receipt messages.
 */
export function decodedToReadReceipt(
  msg: DecodedMessageLike,
  conversationId: string
): ReadReceiptInfo | null {
  const nc = getNativeContent(msg as any);
  if (!nc) return null;
  if (nc.readReceipt === undefined) return null;
  return {
    conversationId,
    senderInboxId: msg.senderInboxId,
  };
}
```

- [ ] **Step 2: Add markReadByPeer action to message store**

In `messages.ts`, add `decodedToReadReceipt` and `ReadReceiptInfo` to both the re-export block and the import block.

Add `markReadByPeer: (conversationId: string) => void` to `MessageActions` interface.

Implement in the store:

```typescript
markReadByPeer: (conversationId) => {
  set((state) => {
    const key = conversationId as string;
    const existing = state.byConversation[key];
    if (!existing) return state;
    const hasPublished = existing.some((m) => m.isOwn && m.status === "published");
    if (!hasPublished) return state;
    return {
      byConversation: {
        ...state.byConversation,
        [key]: existing.map((m) =>
          m.isOwn && m.status === "published" ? { ...m, status: "read" } : m
        ),
      },
    };
  });
},
```

- [ ] **Step 3: Verify build**

Run: `cd xmtp-mobile && npx tsc --noEmit`
Expected: No type errors

- [ ] **Step 4: Commit**

```bash
git add xmtp-mobile/src/utils/messageDecoder.ts xmtp-mobile/src/store/messages.ts
git commit -m "feat(mobile): add read receipt extraction and markReadByPeer store action"
```

---

### Task 4: Handle incoming read receipts in useMessages stream

**Files:**
- Modify: `xmtp-mobile/src/hooks/useMessages.ts`

- [ ] **Step 1: Process read receipts in the stream callback**

Add to imports:

```typescript
import { useMessageStore, decodedToMessageItem, decodedToReaction, decodedToReadReceipt } from "../store/messages";
```

In the `streamMessages` callback, **after** the reaction check and **before** the `decodedToMessageItem` call, add:

```typescript
// Check for read receipt — MUST be before append to prevent loop
const readReceipt = decodedToReadReceipt(decodedMsg, conversationId);
if (readReceipt) {
  const myInboxId = useAuthStore.getState().inboxId;
  if (readReceipt.senderInboxId !== myInboxId) {
    useMessageStore.getState().markReadByPeer(conversationId);
  }
  return;
}
```

**IMPORTANT**: No read receipt scanning in `fetchMessages` — historical messages stay `published`. The `fetchMessages` method already filters out read receipts via `decodedToMessageItem` returning null, which is correct. No changes needed there.

- [ ] **Step 2: Verify build**

Run: `cd xmtp-mobile && npx tsc --noEmit`
Expected: No type errors

- [ ] **Step 3: Commit**

```bash
git add xmtp-mobile/src/hooks/useMessages.ts
git commit -m "feat(mobile): handle incoming read receipts in message stream"
```

---

### Task 5: Send read receipt on DM conversation open + on new peer messages

**Design:**
- **On open**: Send one read receipt when entering a DM with unread messages (after fetchMessages completes). This covers the "saw it in chat list, tapped in" scenario.
- **On new message**: Send read receipt when a new peer message arrives in an open DM.
- **Throttle**: Both paths go through the same throttled `sendReadReceipt` (3s), so rapid stream replay won't cause storms.
- **Historical**: Only current session's messages get read-receipted. No backfill.

**Files:**
- Modify: `xmtp-mobile/src/hooks/useMessages.ts`
- Modify: `xmtp-mobile/app/(main)/conversation/[id].tsx`

- [ ] **Step 1: Add options to useMessages and send read receipt on new peer messages**

Update `useMessages` signature:

```typescript
interface UseMessagesOptions {
  /** If true, send read receipts for new peer messages. */
  sendReadReceipts?: boolean;
  /** Whether this is a DM conversation (read receipts only apply to DMs). */
  isDm?: boolean;
}

export function useMessages(
  conversationId: ConversationId | null,
  options?: UseMessagesOptions
) {
```

In the `streamMessages` callback, after `useMessageStore.getState().append(item)`, add:

```typescript
if (item) {
  useMessageStore.getState().append(item);
  // Send read receipt for new peer messages in DM (if enabled)
  if (!item.isOwn && options?.sendReadReceipts && options?.isDm) {
    sendReadReceipt(conversationId as string).catch(() => {});
  }
}
```

Add import:

```typescript
import { findConversation, sendReadReceipt } from "../xmtp/messages";
```

- [ ] **Step 2: Send read receipt on conversation open (after fetch)**

In `app/(main)/conversation/[id].tsx`, add imports:

```typescript
import { sendMessage, sendReply, sendReadReceipt } from "../../../src/xmtp/messages";
import { useSettingsStore } from "../../../src/store/settings";
```

Add readReceipts selector before useMessages:

```typescript
const readReceiptsEnabled = useSettingsStore((s) => s.readReceipts);
```

Update useMessages call:

```typescript
const { isLoading: messagesLoading, fetchMore } = useMessages(conversationId, {
  sendReadReceipts: readReceiptsEnabled,
  isDm: !isGroup,
});
```

In the existing `useEffect` that calls `store.markRead(id)`, add send-on-enter:

```typescript
useEffect(() => {
  if (!id) return;
  const store = useConversationStore.getState();
  store.setActiveConversation(id);
  store.markRead(id);

  // Send read receipt on enter if enabled and DM with unread messages
  const convoItem = store.items.get(id);
  if (
    useSettingsStore.getState().readReceipts &&
    convoItem?.kind === "dm" &&
    (convoItem?.unreadCount ?? 0) > 0
  ) {
    sendReadReceipt(id);
  }

  return () => useConversationStore.getState().setActiveConversation(null);
}, [id]);
```

- [ ] **Step 3: Verify build**

Run: `cd xmtp-mobile && npx tsc --noEmit`
Expected: No type errors

- [ ] **Step 4: Commit**

```bash
git add xmtp-mobile/src/hooks/useMessages.ts xmtp-mobile/app/\(main\)/conversation/\[id\].tsx
git commit -m "feat(mobile): send read receipt on DM open and on new peer messages"
```

---

### Task 6: Show published/read indicators in MessageBubble

**Files:**
- Modify: `xmtp-mobile/src/components/MessageBubble.tsx`

- [ ] **Step 1: Add status dot icons in the header for own messages**

In the header rendering section (after the `isFailed` block, around line 277), add:

```tsx
{isSending && <Icon source="clock-outline" size={11} color="#938F99" />}
{isFailed && (
  <Pressable onPress={() => onRetry?.(item)} style={styles.retryBtn}>
    <Icon source="alert-circle-outline" size={11} color="#F2B8B5" />
    <Text style={styles.retryText}>Retry</Text>
  </Pressable>
)}
{isOwn && !isSending && !isFailed && item.status === "published" && (
  <Icon source="circle-outline" size={10} color="#938F99" />
)}
{isOwn && !isSending && !isFailed && item.status === "read" && (
  <Icon source="circle-slice-8" size={10} color="#4CAF50" />
)}
```

Icons from Material Community Icons (bundled with react-native-paper):
- `circle-outline` — empty circle ○ for published (gray `#938F99`)
- `circle-slice-8` — filled circle ● for read (green `#4CAF50`)

- [ ] **Step 2: Verify build**

Run: `cd xmtp-mobile && npx tsc --noEmit`
Expected: No type errors

- [ ] **Step 3: Commit**

```bash
git add xmtp-mobile/src/components/MessageBubble.tsx
git commit -m "feat(mobile): show read receipt indicators on own messages"
```

---

### Task 7: Add unit tests

**Files:**
- Modify: `xmtp-mobile/src/__tests__/decodedToMessageItem.test.ts`

- [ ] **Step 1: Add tests for decodedToReadReceipt**

Add import:

```typescript
import {
  decodedToMessageItem,
  decodedToReadReceipt,
  type DecodedMessageLike,
} from "../utils/messageDecoder";
```

Add test cases at the end of the file:

```typescript
describe("decodedToReadReceipt", () => {
  it("extracts a read receipt", () => {
    const msg = fakeMsg({ readReceipt: {} });
    const rr = decodedToReadReceipt(msg, CONV_ID);
    expect(rr).not.toBeNull();
    expect(rr!.conversationId).toBe(CONV_ID);
    expect(rr!.senderInboxId).toBe(OTHER_INBOX);
  });

  it("returns null for non-read-receipt messages", () => {
    const msg = fakeMsg({ text: "hello" });
    expect(decodedToReadReceipt(msg, CONV_ID)).toBeNull();
  });

  it("returns null for reactions", () => {
    const msg = fakeMsg({
      reaction: { reference: "msg-1", action: "added", schema: "unicode", content: "👍" },
    });
    expect(decodedToReadReceipt(msg, CONV_ID)).toBeNull();
  });
});
```

- [ ] **Step 2: Run tests**

Run: `cd xmtp-mobile && npx jest src/__tests__/decodedToMessageItem.test.ts`
Expected: All tests pass including the 3 new ones

- [ ] **Step 3: Commit**

```bash
git add xmtp-mobile/src/__tests__/decodedToMessageItem.test.ts
git commit -m "test(mobile): add unit tests for read receipt extraction"
```

---

### Task 8: Update backlog

**Files:**
- Modify: `docs/mobile-app-backlog.md`

- [ ] **Step 1: Mark P2.2 tasks as done**

Update the P2.2 section to reflect completed work:

```markdown
### P2.2 Read Receipt (DM only) ✅ DONE
**Why**: Sender knows if message was seen. Optional feature with toggle.

Implemented: global toggle (default off, controls sending only), throttled send (3s per conversation), read status indicators (○ published / ● read) on own messages in DM.
- [x] Global toggle in settings (default off, SecureStore persisted) (2026-03-31)
- [x] Throttled sendReadReceipt (3s per conversation) (2026-03-31)
- [x] Send read receipt on DM open (if unread) + on new peer messages (2026-03-31)
- [x] Display read status indicator on own messages (○/● dots) (2026-03-31)
- [x] Handle incoming read receipts in message stream (2026-03-31)
- Not implemented: group read receipts (by design)
```

- [ ] **Step 2: Commit**

```bash
git add docs/mobile-app-backlog.md
git commit -m "docs: update backlog — P2.2 read receipts implemented (DM only)"
```
