# XMTP Mobile App — Backlog & Roadmap

> Last updated: 2026-04-02

## Current State (MVP Complete)

### What's Working
- Private key login with SecureStore persistence
- Server environment selector (dev / production / local)
- Conversation list with real-time stream updates (new convos + lastMessage preview)
- Chat screen with inverted FlashList
- Text message send/receive with optimistic UI + dedup
- New DM creation with address validation + canMessage check
- Keyboard avoidance (react-native-keyboard-controller, translate-with-padding)
- Auto-scroll to bottom on new messages (respects scroll position)
- Foreground sync + network recovery (useAppState / useNetworkState)
- Stream auto-reconnect via onClose callbacks

### Known Issues
- [x] ~~App startup shows empty conversation list briefly before fetchAll completes (loading state missing)~~ (fixed 2026-03-30: skeleton loader)
- [ ] OPPO/OnePlus HansManager freezes app process immediately on background → streams die, no recovery until manual foreground return
- [x] ~~Stream reconnect storm (useMessages onClose recursive with no delay) causing OOM crash~~ (fixed 2026-03-30: exponential backoff)
- [x] ~~conversationCache not cleared on logout — stale SDK objects across sessions~~ (fixed 2026-03-30)
- [x] ~~Stream reconnect counter never resets on success — permanent death after 10 disconnects~~ (fixed 2026-03-30)
- [x] ~~Reaction badges overflow~~ (verified: flexWrap already handles multi-line display)
- [x] ~~No unread message count / badge on conversation list items~~ (fixed 2026-03-30)
- [ ] `expo-file-system` write doesn't work in release builds (logger workaround: dev bundle + adb logcat)
- [ ] package-lock.json not committed (node_modules reproducibility)
- [ ] android/ directory in .gitignore — prebuild artifacts not tracked
- [ ] Back button (hardware + header) randomly stops working after returning from background/other apps. Never triggered long-press menu. Suspects: keyboard-controller state desync on resume, KAV translate-with-padding causing header touch offset, or expo-router nav state corruption. Needs `adb logcat` diagnosis during repro.

---

## P1 — High Priority (Next Sprint)

### P1.1 Push Notifications (FCM)
**Why**: Without push, users miss all messages when app is backgrounded. Core chat UX.

Tasks:
- [ ] Install `@react-native-firebase/messaging` + `expo-notifications`
- [ ] FCM token registration on login
- [ ] Investigate XMTP push notification server integration (SDK built-in `client.registerPushToken()` vs self-hosted `notification-server-go`)
- [ ] Background notification display (title: sender, body: message preview)
- [ ] Notification tap → deep link to conversation
- [ ] Foreground: suppress notification (stream already handles it)

### ~~P1.2 Reply (Quoted Reply)~~ ✅ DONE
Implemented: long-press → Reply action, quoted preview above input, send via reply content type, display reference in bubble.
- [ ] Tap quoted section → scroll to original message (remaining)

### ~~P1.3 Reaction (Emoji)~~ ✅ DONE
Implemented: long-press → 5-emoji quick-react row, send/receive reactions, badge display on bubbles.
- [x] Tap same emoji to toggle/remove reaction (2026-04)
- [x] Reaction badge overflow wrap/collapse (verified: flexWrap already handles)

### ~~P1.4 Message Action Menu~~ ✅ DONE
Implemented: long-press context menu with emoji quick-react row, Copy, Reply actions.

### P1.5 Unread Count + New Message Indicator (partial ✅)
**Why**: Users can't tell which conversations have new messages.

Tasks:
- [x] Track unreadCount per conversation in store (2026-03-30)
- [x] Calculate unread count on conversation list items (2026-03-30)
- [x] Badge display on ConversationListItem (2026-03-30)
- [x] Mark as read when entering conversation (2026-03-30)
- [ ] "New messages" floating chip when scrolled up in chat

---

## P2 — Medium Priority

### ~~P2.1 New Group Creation~~ ✅ DONE
Implemented: DM/Group mode toggle on new-conversation page, group name input, multi-member address input with chip UI, address validation, group creation via SDK, auto-navigate to new group. Content type registry added for proper codec handling. (2026-03-31~04-01)

### P2.2 Read Receipt (DM only) ✅ DONE
**Why**: Sender knows if message was seen. Optional feature with toggle.

Implemented: global toggle (default off, controls sending only), throttled send (3s per conversation), read status indicators (○ published / ● read) on own messages in DM.
- [x] Global toggle in settings (default off, SecureStore persisted) (2026-03-31)
- [x] Throttled sendReadReceipt (3s per conversation) (2026-03-31)
- [x] Send read receipt on DM open (if unread) + on new peer messages (2026-03-31)
- [x] Display read status indicator on own messages (○/● dots) (2026-03-31)
- [x] Handle incoming read receipts in message stream (2026-03-31)
- Not implemented: group read receipts (by design)

### ~~P2.3 Markdown Rendering~~ ✅ DONE
Implemented: react-native-enriched-markdown with GitHub flavor, dual theme, table horizontal scroll patch.

### P2.4 Message Detail View
**Why**: Inspect message metadata (full sender address, timestamps, delivery status).

Tasks:
- [ ] "Details" action in message action menu
- [ ] Bottom sheet showing: sender full address, message ID, sent timestamp, delivery status, content type
- [ ] List of reactions with sender info

### P2.5 Conversation Search
**Why**: Find conversations by name or address as list grows.

Tasks:
- [ ] Search bar at top of conversation list
- [ ] Filter by conversation title / peer address
- [ ] Highlight matching text

### P2.6 Loading States & Error UX (partial ✅)
**Why**: App feels broken without feedback during network operations.

Tasks:
- [x] Skeleton loader on conversation list during initial fetch (2026-03-30)
- [ ] Loading indicator on chat screen while fetching messages
- [ ] Error toast/snackbar for failed operations (send fail, network error)
- [x] Retry button on failed messages (2026-03-30)
- [ ] Connection status indicator (connected / reconnecting / offline)

---

## P3 — Low Priority (Future)

### ~~P3.1 Group Management~~ ✅ DONE (mostly)
Implemented (2026-03-31~04): Group management UI with full CRUD.
- [x] View group info (name, description, member count)
- [x] View member list
- [x] Add members (dedicated add-member screen with address validation)
- [x] Remove members (permission-aware, role-gated)
- [x] Rename group + edit description (EditableField component)
- [x] Leave group with confirmation alert
- [ ] Group permissions edit UI (currently read-only via policy checks)

### P3.2 Attachment Support
- [ ] Image send/receive (camera + gallery picker)
- [ ] File attachment support
- [ ] Inline image preview in chat
- [ ] Download/save attachment to device

### P3.3 WalletConnect Integration
- [ ] Replace raw private key login with WalletConnect v2
- [ ] Support MetaMask, Coinbase Wallet, Rainbow
- [ ] Proper wallet signature UX flow
- [ ] Remove private key input (or keep as "advanced" option)

### P3.4 ENS Resolution
- [ ] Resolve ENS names in new conversation address input
- [ ] Display ENS name as conversation title when available
- [ ] ENS avatar display

### P3.5 Disappearing Messages
- [ ] Set message expiration per conversation
- [ ] Timer display on expiring messages
- [ ] Auto-delete expired messages locally

### P3.6 Performance Optimization
- [ ] Profile and optimize FlashList rendering (large message lists)
- [ ] Lazy load conversation list (virtual scroll)
- [ ] Image caching strategy
- [ ] Reduce bundle size (tree-shake unused react-native-paper components)

### P3.7 iOS Support
- [ ] Expo prebuild for iOS
- [ ] Test all screens on iOS
- [ ] APNs push notification integration
- [ ] iOS keyboard handling verification
- [ ] App Store preparation

---

## Technical Debt

- [ ] Remove `debuggable true` from release build.gradle before production
- [ ] Remove dev bundle export workflow — use production .hbc for release
- [ ] Clean up unused imports across components
- [x] ~~Add TypeScript strict mode~~ (already enabled)
- [ ] Add unit tests for store logic (messages dedup, conversation sorting)
- [ ] Add integration tests for XMTP SDK flows
- [ ] Set up CI/CD (EAS Build)
- [ ] Proper app icon and splash screen
- [ ] Configure proper package name (not `com.anonymous.xmtpmobile`)

---

## Architecture Notes

- **No daemon dependency**: App uses XMTP React Native SDK directly (Plan C)
- **State management**: Zustand with getState() pattern in hooks to avoid selector re-render loops
- **Keyboard**: react-native-keyboard-controller with `behavior="translate-with-padding"` (edge-to-edge compatible)
- **Lists**: @shopify/flash-list with inverted mode for chat
- **Storage**: expo-secure-store for credentials, XMTP SDK's built-in SQLite for messages
- **Streaming**: SDK event-based streams with onClose auto-reconnect
