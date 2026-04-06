# Actions & Intent Content Type Support

## Implementation Summary (2026-04-06)

**Status: CLI/TUI/Mobile — Complete.**

Coinbase Actions (`coinbase.com/actions:1.0`) and Intent (`coinbase.com/intent:1.0`) content types are fully supported across all platforms: Rust daemon/TUI/CLI, ACP agent bridge, and React Native mobile app.

---

### Protocol Overview

Actions and Intent are a request/response pair for structured user choices over XMTP:

```
Agent/Bot                              User
   │                                     │
   │──── Actions ────────────────────────>│  "What do you want for dinner?"
   │     {id, description, actions[]}     │   [1] Hotpot  [2] Sushi  [3] BBQ
   │                                     │
   │<──── Intent ────────────────────────│  User taps "Sushi"
   │     {id (=actions.id), actionId}     │
   │                                     │
```

- **Actions** (`coinbase.com/actions:1.0`): JSON payload with `id`, `description`, and up to 10 `actions[]` (each with `id`, `label`, optional `style`/`imageUrl`/`expiresAt`).
- **Intent** (`coinbase.com/intent:1.0`): JSON payload with `id` (matching the Actions set) and `actionId` (the selected action). Optional `metadata` (max 10KB).
- Both are encoded as `EncodedContent` protobuf with `ContentTypeId { authority_id: "coinbase.com", type_id: "actions"|"intent" }`.
- `fallback` text is always set for clients that don't support these types.

---

### Architecture: End-to-End Data Flow

```
                    ┌─────────────────────────────────────────┐
                    │              XMTP Network                │
                    └──────┬──────────────────┬────────────────┘
                           │                  │
              ┌────────────▼──────┐   ┌───────▼────────────┐
              │   Rust Daemon     │   │  React Native SDK  │
              │ (xmtp crate 0.8) │   │ (@xmtp/rn-sdk v5)  │
              └────┬──────────────┘   └───────┬────────────┘
                   │                          │
         ┌────────▼────────┐       ┌──────────▼──────────┐
         │ Content::Unknown │       │ nativeContent.unknown│
         │ → decode JSON    │       │ → registry handler   │
         └────────┬────────┘       └──────────┬──────────┘
                  │                           │
    ┌─────────────┼──────────────┐    ┌───────▼──────────┐
    │             │              │    │  ActionButtons    │
    ▼             ▼              ▼    │  component        │
  TUI          CLI/ACP        IPC    └──────────────────┘
  (ratatui)    (clap)        (HTTP)
```

---

### Rust Stack (Daemon + TUI + CLI + ACP)

#### Daemon (`crates/xmtp-daemon/src/lib.rs`)
- **Decode**: Intercepts `Content::Unknown` when `content_type` matches `coinbase.com/actions:1.0` or `coinbase.com/intent:1.0`. Parses inner JSON via `decode_coinbase_actions()` / `decode_coinbase_intent()`.
- **Encode**: `encode_coinbase_content<T: CoinbaseFallback>(json_str, type_id)` — generic function that validates JSON, builds `EncodedContent` protobuf with correct `ContentTypeId` and auto-generated fallback text.
- **Send**: `send_typed_content()` dispatches `content_type="actions"|"intent"` to the correct encoder; shared by all send paths (DM, group, conversation).
- **Display**: `format_actions_summary()` / `format_intent_summary()` produce human-readable text, shared across history and summaries.
- **IPC**: `HistoryItem` carries optional `actions_payload: ActionsPayload` and `intent_payload: IntentPayload` for structured data transport.

#### TUI (`crates/xmtp-tui/`)
- Actions render as yellow description + numbered buttons (green=primary, red=danger, yellow=default)
- Press 1-9 with empty input to select an action → sends Intent
- Intent renders as magenta "Selected action: …"
- `color_for_message`: `"actions"` → Yellow, `"intent"` → Magenta

#### CLI (`crates/xmtp-cli/src/main.rs`)
- `send-actions` command for testing: `xmtp-cli send-actions <conv_id> "description" --action id:label --action id2:label2`
- Generates unique `id` from timestamp, sends via daemon HTTP API

#### ACP Bridge (`crates/xmtp-cli/src/acp.rs`)
- **Forwarding**: `should_forward_item` includes `"actions"` and `"intent"` content kinds
- **Intent → Agent**: Formatted as `[Intent] User selected action_id="…" from actions_id="…"` so LLM agents understand the selection
- **Agent → Actions**: Bootstrap prompt teaches agents the XML format:
  ```xml
  <actions id="unique_id" description="What would you like to do?">
    <action id="opt_a" label="Option A" style="primary"/>
    <action id="opt_b" label="Option B"/>
    <action id="cancel" label="Cancel" style="danger"/>
  </actions>
  ```
- **XML parser**: `parse_reply_segments()` splits agent replies into text + actions segments. `<actions>` blocks can appear anywhere in the reply — text before/after becomes separate markdown messages.
- **Message splitting**: Agent reply → multiple XMTP messages in order (markdown + actions + markdown + …)

---

### Mobile App (`xmtp-mobile/`)

#### Content Type Registry (`src/content/`)
- **Actions handler** (`handlers/actions.ts`): Decodes JSON from `extractRawContent()` → `ActionsPayload` with structured `actions[]` array. Returns `DecodeResult { kind: "actions" }`.
- **Intent handler** (same file): Decodes to `DecodeResult { kind: "intent", actionsId, actionId }`.
- **Registry routing** (`registry.ts`): Unknown content types are routed via `nc.unknown.contentTypeId` or `nc.encoded.type` field detection, matching to registered handlers.

#### JS Codecs (`src/xmtp/coinbaseCodecs.ts`)
- `CoinbaseActionsCodec` and `CoinbaseIntentCodec` implement `JSContentCodec<T>` — required for the SDK's native bridge to accept `convo.send(content, { contentType })`.
- Registered at `Client.create()` via `codecs: [new CoinbaseActionsCodec(), new CoinbaseIntentCodec()]`.

#### Data Model
- `MessageItem.actionsPayload?: ActionsPayload` — structured actions data for rendering
- `MessageItem.intentRef?: { actionsId, actionId }` — links intent to its actions set
- Intent messages are **hidden from the message list** (filtered in conversation screen) — they manifest only through the ActionButtons state

#### UI Flow (`src/components/ActionButtons.tsx`)
Three states:
1. **Idle** — All buttons visible and tappable
2. **Pending** — User tapped a button. Other buttons disappear. Selected button shows `ActivityIndicator` spinner. Intent is being sent to network.
3. **Confirmed** — Intent message received back via stream (matched by `intentMap`). Spinner replaced with `✓` prefix. Button styled green. No further interaction possible.

```
[Idle]                    [Pending]              [Confirmed]
┌──────────────────┐     ┌──────────────────┐   ┌──────────────────┐
│ 今天晚上吃什么？  │     │ 今天晚上吃什么？  │   │ 今天晚上吃什么？  │
│                  │     │                  │   │                  │
│ ┌──────────────┐ │     │ ┌──────────────┐ │   │ ┌──────────────┐ │
│ │   火锅       │ │     │ │ ◌ 寿司       │ │   │ │ ✓ 寿司       │ │
│ ├──────────────┤ │     │ └──────────────┘ │   │ └──────────────┘ │
│ │   寿司       │ │  →  │                  │ → │                  │
│ ├──────────────┤ │     │                  │   │                  │
│ │   烧烤       │ │     │                  │   │                  │
│ └──────────────┘ │     │                  │   │                  │
└──────────────────┘     └──────────────────┘   └──────────────────┘
```

#### Intent Map (Conversation Screen)
- `useMemo` builds `intentMap: Map<actionsId, actionId>` from all messages (including hidden intents)
- Each `MessageBubble` with `actionsPayload` receives `respondedActionId = intentMap.get(payload.id)`
- On app restart, intent history is loaded → previously selected actions render directly in confirmed state

---

### File Index

| File | Platform | Role |
|------|----------|------|
| `crates/xmtp-ipc/src/lib.rs` | Rust | `ActionsPayload`, `IntentPayload` IPC types |
| `crates/xmtp-daemon/src/lib.rs` | Rust | Decode/encode/send, `CoinbaseFallback` trait |
| `crates/xmtp-tui/src/app.rs` | Rust | TUI key handling (1-9 select), color mapping |
| `crates/xmtp-tui/src/ui.rs` | Rust | TUI button list rendering |
| `crates/xmtp-cli/src/main.rs` | Rust | `send-actions` CLI command |
| `crates/xmtp-cli/src/acp.rs` | Rust | XML parser, bootstrap prompt, segment splitting |
| `xmtp-mobile/src/xmtp/coinbaseCodecs.ts` | Mobile | JS Codecs for SDK send support |
| `xmtp-mobile/src/xmtp/client.ts` | Mobile | Codec registration at Client.create() |
| `xmtp-mobile/src/xmtp/messages.ts` | Mobile | `sendIntent()` via codec path |
| `xmtp-mobile/src/content/handlers/actions.ts` | Mobile | Decode handlers for actions + intent |
| `xmtp-mobile/src/content/registry.ts` | Mobile | Unknown type routing via nc.unknown |
| `xmtp-mobile/src/content/types.ts` | Mobile | `DecodeResult` with "actions" and "intent" kinds |
| `xmtp-mobile/src/utils/messageDecoder.ts` | Mobile | `MessageItem.intentRef` field |
| `xmtp-mobile/src/components/ActionButtons.tsx` | Mobile | 3-state button UI (idle/pending/confirmed) |
| `xmtp-mobile/app/(main)/conversation/[id].tsx` | Mobile | `intentMap`, intent message filtering |

---

### Key Design Decisions

1. **Daemon decodes, not SDK** — The Rust daemon intercepts `Content::Unknown` and parses the inner JSON. The xmtp crate doesn't natively support Actions/Intent, so we work at the application layer.

2. **JS Codecs for mobile send** — The React Native SDK's native bridge rejects unknown content types in `XMTP.sendMessage`. Registering `JSContentCodec` at `Client.create()` enables `convo.send(content, { contentType })` to go through `_sendWithJSCodec` which handles encoding and FFI.

3. **Intent messages are invisible** — Intent is a protocol-level signal, not user-facing content. It's filtered from the message list and only manifests through the ActionButtons state transition (idle → pending → confirmed).

4. **XML tags for ACP agents** — LLM agents can't easily construct raw JSON with exact field names. XML `<actions>` tags are more natural for LLMs and can appear anywhere in a reply. The ACP bridge parses them and converts to proper Coinbase Actions JSON.

5. **Single-selection, no undo** — Once a user selects an action, the choice is locked. The protocol doesn't prevent multiple intents, but the UI enforces single selection to match the expected UX.

6. **Fallback text always set** — Both Actions and Intent include human-readable fallback text so clients that don't support these types can still display something meaningful.

---

## Original Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Support Coinbase Actions/Intent content types end-to-end: daemon decode → IPC → TUI render + user selection → send Intent back → ACP agent can also send Actions via bootstrap prompt injection.

**Architecture:** Intercept `Content::Unknown` in the daemon when `content_type` matches `coinbase.com/actions:1.0` or `coinbase.com/intent:1.0`. Decode the JSON payload into typed structs. Carry structured data through IPC as optional fields on `HistoryItem`. TUI renders Actions as a numbered button list with style colors, and lets the user press a number key to send an Intent back. ACP bootstrap prompt is extended with Actions/Intent format documentation so agents can emit Actions natively.

**Tech Stack:** Rust, serde_json, prost (for EncodedContent decode), xmtp crate v0.8.1 (`conversation.send()` for raw bytes), ratatui (TUI rendering).

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/xmtp-ipc/src/lib.rs` | Modify | Add `ActionsPayload`, `IntentPayload` structs; add optional fields to `HistoryItem` |
| `crates/xmtp-daemon/src/lib.rs` | Modify | Decode Actions/Intent from `Content::Unknown`; populate new IPC fields; support sending Actions/Intent via `content_type` param |
| `crates/xmtp-tui/src/app.rs` | Modify | Handle `content_kind = "actions"` in normalization, color, and user selection keybinding |
| `crates/xmtp-tui/src/ui.rs` | Modify | Render Actions as styled button list; render Intent as selection summary |
| `crates/xmtp-cli/src/acp.rs` | Modify | Forward `"actions"` / `"intent"` in `should_forward_item`; extend bootstrap prompt with Actions/Intent format docs |

---

### Task 1: IPC Type Definitions

**Files:**
- Modify: `crates/xmtp-ipc/src/lib.rs`

- [ ] **Step 1: Add Actions/Intent payload structs**

Add these structs after `ReactionDetail` (around line 165):

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionItem {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionsPayload {
    pub id: String,
    pub description: String,
    pub actions: Vec<ActionItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntentPayload {
    pub id: String,
    pub action_id: String,
}
```

- [ ] **Step 2: Add optional fields to HistoryItem**

Add two new optional fields to `HistoryItem` after `read_by`:

```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actions_payload: Option<ActionsPayload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_payload: Option<IntentPayload>,
```

- [ ] **Step 3: Verify build**

Run: `cargo build -p xmtp-ipc`
Expected: compiles with warnings about unused fields (OK, consumers come in later tasks)

- [ ] **Step 4: Commit**

```bash
git add crates/xmtp-ipc/src/lib.rs
git commit -m "feat(ipc): add ActionsPayload and IntentPayload types to HistoryItem"
```

---

### Task 2: Daemon Decode — Actions & Intent from Content::Unknown

**Files:**
- Modify: `crates/xmtp-daemon/src/lib.rs`

- [ ] **Step 1: Add serde structs for Coinbase JSON decode**

Add near the existing protobuf struct definitions (around line 100), after `DecodedReactionPayload`:

```rust
/// Coinbase Actions JSON payload (coinbase.com/actions:1.0)
#[derive(Debug, Clone, Deserialize)]
struct CoinbaseActions {
    id: String,
    description: String,
    actions: Vec<CoinbaseAction>,
}

#[derive(Debug, Clone, Deserialize)]
struct CoinbaseAction {
    id: String,
    label: String,
    #[serde(default, alias = "imageUrl")]
    image_url: Option<String>,
    #[serde(default)]
    style: Option<String>,
}

/// Coinbase Intent JSON payload (coinbase.com/intent:1.0)
#[derive(Debug, Clone, Deserialize)]
struct CoinbaseIntent {
    id: String,
    #[serde(alias = "actionId")]
    action_id: String,
}
```

- [ ] **Step 2: Add decode helper functions**

Add after `decode_group_updated`:

```rust
fn decode_coinbase_actions(raw: &[u8]) -> Option<CoinbaseActions> {
    let encoded = xmtp::content::EncodedContent::decode(raw).ok()?;
    serde_json::from_slice(&encoded.content).ok()
}

fn decode_coinbase_intent(raw: &[u8]) -> Option<CoinbaseIntent> {
    let encoded = xmtp::content::EncodedContent::decode(raw).ok()?;
    serde_json::from_slice(&encoded.content).ok()
}
```

- [ ] **Step 3: Update the Content::Unknown match arm in the main decode block**

In the main message decode block (around line 1136), replace the `Content::Unknown` arm. The existing code sets `content_kind = "unknown"` for all unknown types. Change it to detect actions/intent and set the appropriate `content_kind`, `content`, and new payload fields.

The current code returns a 6-tuple `(content_kind, content, reply_target, reaction_target, reaction_emoji, reaction_action)`. We need to also thread through the new payload fields. The simplest approach: expand the tuple to 8 elements, adding `actions_payload: Option<ActionsPayload>` and `intent_payload: Option<IntentPayload>`.

Update the tuple definition and every match arm to include two extra `None` values for the new fields. Then in the `Content::Unknown` arm:

```rust
Ok(Content::Unknown { content_type, raw }) => {
    log_unknown_message_type(
        Some(&message.id),
        &content_type,
        &raw,
        message.fallback.as_ref(),
    );
    if content_type == "coinbase.com/actions:1.0" {
        if let Some(actions) = decode_coinbase_actions(&raw) {
            let summary = format!(
                "{}\n{}",
                actions.description,
                actions.actions.iter().enumerate()
                    .map(|(i, a)| format!("[{}] {}", i + 1, a.label))
                    .collect::<Vec<_>>().join("\n")
            );
            let payload = ActionsPayload {
                id: actions.id,
                description: actions.description,
                actions: actions.actions.iter().map(|a| ActionItem {
                    id: a.id.clone(),
                    label: a.label.clone(),
                    style: a.style.clone(),
                    image_url: a.image_url.clone(),
                }).collect(),
            };
            (
                "actions".to_owned(),
                summary,
                None, None, None, None,
                Some(payload),
                None,
            )
        } else {
            ("unknown".to_owned(),
             message.fallback.clone().filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| summarize_unknown_content(&content_type, &raw)),
             None, None, None, None, None, None)
        }
    } else if content_type == "coinbase.com/intent:1.0" {
        if let Some(intent) = decode_coinbase_intent(&raw) {
            let summary = format!("Selected action: {}", intent.action_id);
            let payload = IntentPayload {
                id: intent.id,
                action_id: intent.action_id,
            };
            (
                "intent".to_owned(),
                summary,
                None, None, None, None,
                None,
                Some(payload),
            )
        } else {
            ("unknown".to_owned(),
             message.fallback.clone().filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| summarize_unknown_content(&content_type, &raw)),
             None, None, None, None, None, None)
        }
    } else {
        (
            "unknown".to_owned(),
            message.fallback.clone().filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| summarize_unknown_content(&content_type, &raw)),
            None, None, None, None, None, None,
        )
    }
}
```

- [ ] **Step 4: Wire payload fields into HistoryItem construction**

Where the `HistoryItem` is constructed from the tuple (around line 968-980), add the new fields:

```rust
    actions_payload,
    intent_payload,
```

And update the destructuring of the tuple to include the two new variables.

- [ ] **Step 5: Update `summarize_message_content` for actions/intent**

In `summarize_message_content()` (around line 1228), add handling before the existing `Content::Unknown` match:

```rust
Ok(Content::Unknown { content_type, raw }) => {
    if content_type == "coinbase.com/actions:1.0" {
        if let Some(actions) = decode_coinbase_actions(&raw) {
            return format!(
                "{}\n{}",
                actions.description,
                actions.actions.iter().enumerate()
                    .map(|(i, a)| format!("[{}] {}", i + 1, a.label))
                    .collect::<Vec<_>>().join("\n")
            );
        }
    } else if content_type == "coinbase.com/intent:1.0" {
        if let Some(intent) = decode_coinbase_intent(&raw) {
            return format!("Selected action: {}", intent.action_id);
        }
    }
    log_unknown_message_type(None, &content_type, &raw, message.fallback.as_ref());
    // ... existing fallback logic
}
```

- [ ] **Step 6: Verify build**

Run: `cargo build -p xmtp-daemon`
Expected: compiles. May need to add `use xmtp_ipc::{ActionsPayload, ActionItem, IntentPayload};` at the top.

- [ ] **Step 7: Commit**

```bash
git add crates/xmtp-daemon/src/lib.rs
git commit -m "feat(daemon): decode Coinbase Actions/Intent from Content::Unknown"
```

---

### Task 3: Daemon Send — Actions & Intent Content Types

**Files:**
- Modify: `crates/xmtp-daemon/src/lib.rs`

- [ ] **Step 1: Add encode helper functions**

Add after the decode helpers:

```rust
fn encode_coinbase_actions(json_str: &str) -> anyhow::Result<Vec<u8>> {
    // Validate JSON parses as Actions
    let _: CoinbaseActions = serde_json::from_str(json_str)
        .context("invalid Actions JSON")?;
    let encoded = xmtp::content::EncodedContent {
        r#type: Some(xmtp::content::ContentTypeId {
            authority_id: "coinbase.com".into(),
            type_id: "actions".into(),
            version_major: 1,
            version_minor: 0,
        }),
        parameters: std::collections::HashMap::new(),
        fallback: None, // SDK generates fallback from actions
        content: json_str.as_bytes().to_vec(),
        compression: None,
    };
    use prost::Message;
    Ok(encoded.encode_to_vec())
}

fn encode_coinbase_intent(json_str: &str) -> anyhow::Result<Vec<u8>> {
    let _: CoinbaseIntent = serde_json::from_str(json_str)
        .context("invalid Intent JSON")?;
    let encoded = xmtp::content::EncodedContent {
        r#type: Some(xmtp::content::ContentTypeId {
            authority_id: "coinbase.com".into(),
            type_id: "intent".into(),
            version_major: 1,
            version_minor: 0,
        }),
        parameters: std::collections::HashMap::new(),
        fallback: None,
        content: json_str.as_bytes().to_vec(),
        compression: None,
    };
    use prost::Message;
    Ok(encoded.encode_to_vec())
}
```

- [ ] **Step 2: Extend send functions to handle "actions" and "intent" content_type**

In `send_conversation_with_client` (around line 604), extend the `content_type` match:

```rust
let message_id = match content_type {
    Some("markdown") => conversation
        .send_markdown(text)
        .context("send conversation markdown")?,
    Some("actions") => {
        let bytes = encode_coinbase_actions(text)
            .context("encode actions content")?;
        conversation.send(&bytes).context("send conversation actions")?
    }
    Some("intent") => {
        let bytes = encode_coinbase_intent(text)
            .context("encode intent content")?;
        conversation.send(&bytes).context("send conversation intent")?
    }
    _ => conversation
        .send_text(text)
        .context("send conversation text")?,
};
```

Apply the same pattern to `send_dm_with_client` (line 535) and `send_group_with_client_logged` (line 625).

- [ ] **Step 3: Verify build**

Run: `cargo build -p xmtp-daemon`

- [ ] **Step 4: Commit**

```bash
git add crates/xmtp-daemon/src/lib.rs
git commit -m "feat(daemon): support sending Actions/Intent via content_type param"
```

---

### Task 4: TUI — Render Actions & Intent Messages

**Files:**
- Modify: `crates/xmtp-tui/src/ui.rs`
- Modify: `crates/xmtp-tui/src/app.rs`

- [ ] **Step 1: Update color_for_message to handle actions/intent**

In `app.rs`, `color_for_message` (around line 1528), add before the `"unknown"` check:

```rust
if item.content_kind == "actions" {
    return Color::Yellow;
}
if item.content_kind == "intent" {
    return Color::Magenta;
}
```

- [ ] **Step 2: Add Actions rendering in ui.rs**

In `ui.rs`, where content lines are built (around line 273-306), add an `"actions"` branch before the markdown check:

```rust
let mut content_lines = if item.content_kind == "actions" {
    if let Some(ref actions) = item.actions_payload {
        let mut lines = vec![
            Line::from(Span::styled(
                &actions.description,
                Style::default().fg(Color::Yellow).bg(row_bg),
            )),
        ];
        for (i, action) in actions.actions.iter().enumerate() {
            let style_color = match action.style.as_deref() {
                Some("primary") => Color::Green,
                Some("danger") => Color::Red,
                _ => Color::Yellow,
            };
            lines.push(Line::from(Span::styled(
                format!("  [{}] {}", i + 1, action.label),
                Style::default().fg(style_color).bg(row_bg),
            )));
        }
        lines
    } else {
        wrap_text_lines(&content, wrap_width)
            .into_iter()
            .map(|segment| Line::from(Span::styled(
                segment,
                Style::default().fg(Color::Yellow).bg(row_bg),
            )))
            .collect()
    }
} else if item.content_kind == "markdown" {
    // ... existing markdown rendering
```

- [ ] **Step 3: Verify build**

Run: `cargo build -p xmtp-tui`

- [ ] **Step 4: Commit**

```bash
git add crates/xmtp-tui/src/ui.rs crates/xmtp-tui/src/app.rs
git commit -m "feat(tui): render Actions as styled button list, color-code intent"
```

---

### Task 5: TUI — User Selection Sends Intent

**Files:**
- Modify: `crates/xmtp-tui/src/app.rs`

- [ ] **Step 1: Find the latest Actions message for the current conversation**

Add a helper method on `App`:

```rust
fn latest_actions_payload(&self) -> Option<&ActionsPayload> {
    self.history.iter().rev()
        .find(|item| item.content_kind == "actions" && item.actions_payload.is_some())
        .and_then(|item| item.actions_payload.as_ref())
}
```

- [ ] **Step 2: Handle number key press to send Intent**

In the event handling section where keyboard input is processed for the conversation view, add handling for digit keys 1-9. When a digit N is pressed and there is a pending Actions message with at least N options, emit an `Effect` to send an Intent message:

```rust
KeyCode::Char(c @ '1'..='9') if self.input_text.is_empty() => {
    let index = (c as u8 - b'1') as usize;
    if let Some(actions) = self.latest_actions_payload() {
        if let Some(action) = actions.actions.get(index) {
            let intent_json = serde_json::json!({
                "id": actions.id,
                "actionId": action.id,
            });
            return vec![Effect::SendMessage {
                conversation_id: conversation_id.to_owned(),
                message: intent_json.to_string(),
                content_type: Some("intent".to_owned()),
            }];
        }
    }
    vec![]
}
```

Note: Check how `Effect::SendMessage` is defined. If it doesn't have a `content_type` field yet, add one:

```rust
SendMessage {
    conversation_id: String,
    message: String,
    content_type: Option<String>,  // NEW
},
```

And update `Runtime::apply_effects` to pass `content_type` through to the daemon send API.

- [ ] **Step 3: Verify build**

Run: `cargo build -p xmtp-tui`

- [ ] **Step 4: Commit**

```bash
git add crates/xmtp-tui/src/app.rs
git commit -m "feat(tui): press 1-9 to select action and send Intent"
```

---

### Task 6: ACP Bridge — Forward Actions/Intent & Bootstrap Prompt (XML Tags)

**Files:**
- Modify: `crates/xmtp-cli/src/acp.rs`

**Design:** Agent replies can contain `<actions>` XML tags anywhere in the response body.
The ACP bridge parses the reply, splits it into text segments and actions blocks, and sends
each segment as a separate XMTP message in order. This allows the agent to write explanatory
text before/after/between structured action menus.

Example agent reply:
```
Here's what I found. Based on the analysis:

<actions id="next_step" description="Choose next step">
  <action id="deploy" label="Deploy to production" style="primary"/>
  <action id="test" label="Run tests first"/>
  <action id="cancel" label="Cancel" style="danger"/>
</actions>

Let me know if you need more details on any option.
```

This produces 3 XMTP messages in order:
1. markdown: "Here's what I found. Based on the analysis:"
2. actions: `{"id":"next_step","description":"Choose next step","actions":[...]}`
3. markdown: "Let me know if you need more details on any option."

- [ ] **Step 1: Update should_forward_item to include actions and intent**

In `should_forward_item` (line 1249), change:

```rust
fn should_forward_item(item: &HistoryItem, self_inbox_id: Option<&str>) -> bool {
    if self_inbox_id == Some(item.sender_inbox_id.as_str()) {
        return false;
    }
    matches!(item.content_kind.as_str(), "text" | "markdown" | "reply" | "actions" | "intent")
}
```

- [ ] **Step 2: Format actions/intent messages for the agent**

In `prompt_agent` (line 1115), when forwarding an intent message to the agent, include
structured context. Update the content construction:

```rust
let content = if context_prefix {
    let prefix = format!("[{}]", sender_short_id(&item.sender_inbox_id));
    if item.content_kind == "intent" {
        if let Some(ref intent) = item.intent_payload {
            format!("{prefix} [Intent] User selected action_id=\"{}\" from actions_id=\"{}\"",
                intent.action_id, intent.id)
        } else {
            format!("{prefix} {}", item.content)
        }
    } else {
        format!("{prefix} {}", item.content)
    }
} else {
    item.content.clone()
};
```

- [ ] **Step 3: Extend bootstrap prompt with Actions/Intent XML format documentation**

In `send_bootstrap_prompt` (line 1192), append Actions/Intent documentation to the
bootstrap string. The key instruction: use XML `<actions>` tags that can appear anywhere
in a reply.

```rust
let bootstrap = format!(
    "{bootstrap}\n\n\
## Interactive Actions\n\
\n\
When you need the user to make a structured choice, embed an <actions> XML block \
anywhere in your reply. You can include normal text before and after it.\n\
\n\
Format:\n\
```\n\
<actions id=\"unique_id\" description=\"What would you like to do?\">\n\
  <action id=\"opt_a\" label=\"Option A\" style=\"primary\"/>\n\
  <action id=\"opt_b\" label=\"Option B\"/>\n\
  <action id=\"cancel\" label=\"Cancel\" style=\"danger\"/>\n\
</actions>\n\
```\n\
\n\
Rules:\n\
- id: unique identifier for this action set\n\
- description: shown above the button list\n\
- Each <action> needs id and label; style is optional: primary | secondary | danger\n\
- Maximum 10 actions per block, unique ids\n\
- You may include multiple <actions> blocks in one reply\n\
- Text outside <actions> tags is sent as normal markdown\n\
- When the user selects, you receive: [Intent] User selected action_id=\"opt_a\" from actions_id=\"unique_id\"\n\
- Use Actions only when structured choices genuinely help; prefer plain text for open-ended questions"
);
```

- [ ] **Step 4: Add XML parsing helper to extract actions blocks from agent reply**

Add a function that splits an agent reply into ordered segments:

```rust
enum ReplySegment {
    Text(String),
    Actions(String), // JSON string ready to send as content_type="actions"
}

fn parse_reply_segments(reply: &str) -> Vec<ReplySegment> {
    let mut segments = Vec::new();
    let mut remaining = reply;

    while let Some(start) = remaining.find("<actions") {
        // Text before the tag
        let before = remaining[..start].trim();
        if !before.is_empty() {
            segments.push(ReplySegment::Text(before.to_owned()));
        }

        // Find closing tag
        let after_start = &remaining[start..];
        if let Some(end_offset) = after_start.find("</actions>") {
            let tag_content = &after_start[..end_offset + "</actions>".len()];
            if let Some(json) = parse_actions_xml(tag_content) {
                segments.push(ReplySegment::Actions(json));
            } else {
                // Malformed tag — send as text
                segments.push(ReplySegment::Text(tag_content.to_owned()));
            }
            remaining = &after_start[end_offset + "</actions>".len()..];
        } else {
            // No closing tag — treat rest as text
            segments.push(ReplySegment::Text(remaining[start..].to_owned()));
            remaining = "";
        }
    }

    // Remaining text after last tag
    let tail = remaining.trim();
    if !tail.is_empty() {
        segments.push(ReplySegment::Text(tail.to_owned()));
    }

    // If no segments were found, return the whole thing as text
    if segments.is_empty() && !reply.trim().is_empty() {
        segments.push(ReplySegment::Text(reply.to_owned()));
    }

    segments
}
```

- [ ] **Step 5: Add XML-to-JSON converter for a single `<actions>` block**

```rust
fn parse_actions_xml(xml: &str) -> Option<String> {
    // Extract attributes from <actions id="..." description="...">
    let id = extract_xml_attr(xml, "id")?;
    let description = extract_xml_attr(xml, "description").unwrap_or_default();

    // Extract all <action .../> self-closing tags
    let mut actions = Vec::new();
    let mut search = xml;
    while let Some(pos) = search.find("<action ") {
        let tag_start = &search[pos..];
        let tag_end = tag_start.find("/>").or_else(|| tag_start.find(">"))?;
        let tag = &tag_start[..tag_end + if tag_start[tag_end..].starts_with("/>") { 2 } else { 1 }];

        let action_id = extract_xml_attr(tag, "id")?;
        let label = extract_xml_attr(tag, "label")?;
        let style = extract_xml_attr(tag, "style");
        actions.push(serde_json::json!({
            "id": action_id,
            "label": label,
            "style": style,
        }));
        search = &search[pos + tag.len()..];
    }

    if actions.is_empty() || actions.len() > 10 {
        return None;
    }

    Some(serde_json::json!({
        "id": id,
        "description": description,
        "actions": actions,
    }).to_string())
}

fn extract_xml_attr(tag: &str, attr_name: &str) -> Option<String> {
    let pattern = format!("{attr_name}=\"");
    let start = tag.find(&pattern)? + pattern.len();
    let end = start + tag[start..].find('"')?;
    Some(tag[start..end].to_owned())
}
```

- [ ] **Step 6: Update send_reply_part to use segment parsing**

Replace `send_reply_part` to handle mixed text+actions replies:

```rust
async fn send_reply_part(
    data_dir: &Path,
    conversation_id: &str,
    reply_part: &str,
    event_name: &str,
) -> anyhow::Result<String> {
    let segments = parse_reply_segments(reply_part);
    let mut last_message_id = String::new();

    for segment in segments {
        let (text, content_type) = match segment {
            ReplySegment::Text(t) => (t, Some("markdown")),
            ReplySegment::Actions(json) => (json, Some("actions")),
        };
        let sent = daemon_send_conversation(data_dir, conversation_id, &text, content_type)
            .await
            .with_context(|| {
                format!("send ACP reply to conversation {conversation_id}")
            })?;
        log_acp_event(
            data_dir,
            conversation_id,
            serde_json::json!({
                "event": event_name,
                "conversation_id": conversation_id,
                "message_id": sent.message_id,
                "content_type": content_type,
            }),
        );
        last_message_id = sent.message_id;
    }

    Ok(last_message_id)
}
```

- [ ] **Step 7: Verify build**

Run: `cargo build -p xmtp-cli`

- [ ] **Step 8: Commit**

```bash
git add crates/xmtp-cli/src/acp.rs
git commit -m "feat(acp): forward actions/intent, XML-based Actions in agent replies"
```

---

### Task 7: Integration Smoke Test

- [ ] **Step 1: Build the full workspace**

Run: `cargo build --workspace`

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace`
Fix any warnings.

- [ ] **Step 3: Run existing tests**

Run: `cargo test -p xmtp-tui --lib`
Run: `cargo test -p xmtp-ipc --lib`
Expected: all existing tests pass.

- [ ] **Step 4: Manual verification checklist**

1. Start daemon, open TUI to a conversation with a Coinbase agent (or any agent that sends Actions)
2. Verify Actions messages render as a yellow description line + numbered green/red buttons
3. Press a number key → verify Intent is sent and appears as magenta "Selected action: ..."
4. Start ACP bridge with an LLM agent → verify bootstrap prompt includes Actions XML format
5. Ask the agent to present options → verify it replies with `<actions>` XML → verify the reply is split into text + actions messages correctly
6. Verify text before/after `<actions>` tags arrives as separate markdown messages
7. Verify malformed `<actions>` tags fall back to plain text (no crash)

- [ ] **Step 5: Final commit if any fixes needed**

```bash
git add -u
git commit -m "fix: clippy and integration fixes for Actions/Intent support"
```
