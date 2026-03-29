# XMTP Mobile — UI Glossary

> Canonical names for screens, regions, and components. Use these terms in all discussions to avoid ambiguity.

---

## Screens

| Term | File | Description |
|------|------|-------------|
| **Login Screen** | `app/login.tsx` | Private key input + network environment selector (Dev / Production / Local). Shown before authentication. |
| **Conversation List Screen** | `app/(main)/conversations.tsx` | Main screen after login. Shows all conversations sorted by recency. |
| **Chat Screen** | `app/(main)/conversation/[id].tsx` | Per-conversation message view with input bar. Inverted FlashList. |
| **New Conversation Screen** | `app/(main)/new-conversation.tsx` | ETH address input to create a new DM. |
| **Settings Screen** | `app/(main)/settings.tsx` | User preferences: custom quick-reaction list, logout. |
| **About Screen** | `app/(main)/about.tsx` | App info: name, version, wallet address, inbox ID, network environment. |
| **DM Detail Screen** | `app/(main)/conversation/dm-detail.tsx` | DM conversation metadata: peer address, peer inbox ID, conversation ID, topic, created at. Accessible from Chat Screen header info button. |
| **Group Detail Screen** | `app/(main)/conversation/group-detail.tsx` | Group conversation metadata: group name, member list with addresses, conversation ID, topic, created at. Accessible from Chat Screen header info button. |

---

## Conversation List Screen — Regions

```
┌──────────────────────────────────┐
│  Appbar                          │
│  ┌────────┐        ┌───┐ ┌───┐  │
│  │Messages│        │ + │ │ ⋮ │  │
│  └────────┘        └───┘ └───┘  │
│                    New   Overflow│
│                    DM    Menu   │
├──────────────────────────────────┤
│  Conversation List               │
│  ┌──────────────────────────────┐│
│  │ Conversation Row             ││
│  │ ┌──────┐ ┌──────────┐ Time  ││
│  │ │Avatar│ │Title      │       ││
│  │ │      │ │Preview    │       ││
│  │ └──────┘ └──────────┘       ││
│  └──────────────────────────────┘│
│  ...                             │
└──────────────────────────────────┘
```

| Term | Description |
|------|-------------|
| **Appbar** | Top bar with title "Messages", action buttons. |
| **New DM Button** | `+` icon in Appbar. Navigates to New Conversation Screen. |
| **Overflow Menu** | `⋮` (dots-vertical) icon in Appbar. Dropdown with: New Group (disabled), Settings, About. |
| **Conversation Row** | Single list item. Component: `ConversationListItem`. |
| **Avatar** | Left circle with initials derived from title. Deterministic color. |
| **Title** | Conversation name: peer address (shortened) for DM, group name for group. |
| **Preview** | Last message text, single line, truncated. |
| **Timestamp** | Relative time of last message (e.g. "2m", "3h", "Yesterday"). |
| **Empty State** | Shown when no conversations exist. Displays address/inboxId info and a "New Conversation" button. |

---

## Chat Screen — Regions

```
┌──────────────────────────────────┐
│  Header Bar                      │
│  ┌──┐ ┌────────────────────┐     │
│  │← │ │ Conversation Title │     │
│  └──┘ └────────────────────┘     │
├──────────────────────────────────┤
│  Message List (inverted)         │
│                                  │
│        ┌──────────────┐          │
│        │ Message       │ ← Bubble│
│        │ Bubble        │         │
│        └──────────────┘          │
│   Sender  12:34       ← Header  │
│   ┌──────────────┐               │
│   │ Bubble        │              │
│   └──────────────┘               │
│   👍 ❤️              ← Reactions │
│                                  │
├──────────────────────────────────┤
│  ↩ Replying to "..."  ✕ ← Reply │
│                          Preview │
├──────────────────────────────────┤
│  Input Bar                       │
│  ┌────────────────────┐  ┌────┐  │
│  │ Message...         │  │Send│  │
│  └────────────────────┘  └────┘  │
└──────────────────────────────────┘
```

| Term | Description |
|------|-------------|
| **Header Bar** | Stack navigator header. Back arrow + conversation title. |
| **Back Button** | `←` in header. Returns to Conversation List Screen. |
| **Message List** | Inverted FlashList. Newest messages at bottom, scroll up for history. |
| **Message Bubble** | Single message container. Component: `MessageBubble`. |
| **Bubble Header** | Sender label (group only) + timestamp + status icon. Shown on first message in a group or after time gap. |
| **Sender Label** | Shortened ETH address of sender. Only in group chats, only for others' messages. Color-coded by sender. |
| **Reaction Badges** | Row of emoji/text badges below a bubble. Each badge shows the reaction + count if > 1. |
| **Reply Quote** | Small bar above a bubble showing the referenced message text. Displayed when message is a reply. |
| **Context Menu** | Floating menu on long-press. Two rows: Quick Reactions (emoji/text bar) + Actions (Copy, Reply). |
| **Quick Reactions** | Top row of Context Menu. Configurable in Settings. Emoji or short text (≤ 4 chars). |
| **Action Row** | Bottom row of Context Menu: Copy and Reply buttons. |
| **Reply Preview** | Bar above Input Bar when replying to a message. Shows referenced text + dismiss button. |
| **Input Bar** | Bottom area. Component: `MessageInput`. TextInput (auto-expanding, multiline) + Send button. |
| **Send Button** | Circular icon button in Input Bar. Enabled when text is non-empty. |

---

## Login Screen — Regions

| Term | Description |
|------|-------------|
| **Login Card** | Centered card containing all login controls. |
| **Network Selector** | Segmented button group: Dev / Production / Local. |
| **Local Host Input** | Text field for custom XMTP node URL. Only visible when "Local" is selected. |
| **Private Key Input** | Secure text field for ETH private key. |
| **Connect Button** | Initiates XMTP client creation. Shows loading spinner during connection. |

---

## Settings Screen — Regions

| Term | Description |
|------|-------------|
| **Reactions Editor** | Grid of reaction slots. Tap to edit, `✕` to remove, `+` to add (max 6). |
| **Reaction Slot** | Single editable reaction item. Supports emoji or text ≤ 4 chars. |
| **Reset Button** | "Reset to defaults" — restores default reaction list (👍 ❤️ 😂 🔥 👀 🙏). |
| **Logout Button** | Outlined destructive button. Shows confirmation alert before logging out. |

---

## About Screen — Regions

| Term | Description |
|------|-------------|
| **App Identity** | App name ("XMTP Messenger") + version number. |
| **Account Section** | Wallet Address + Inbox ID (tap to copy). |
| **Network Section** | Environment (dev/production/local) + protocol version. |

---

## Cross-cutting Terms

| Term | Description |
|------|-------------|
| **DM** | Direct message conversation (1:1). |
| **Group** | Group conversation (multi-party). |
| **Inbox ID** | XMTP-assigned identifier for a user's inbox. Different from wallet address. |
| **Content Type** | XMTP message format: text, markdown, reaction, reply, read receipt, etc. |
| **Optimistic UI** | Sent messages appear immediately in the list before network confirmation. |
| **Stream** | Real-time event subscription from XMTP SDK. Used for live message/conversation updates. |
