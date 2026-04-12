# XMTP 生态状态与项目影响

Date: 2026-04-06
Last verified against repo: 2026-04-12
Purpose: keep one project-facing status document for high-signal XMTP proposals and adjacent ecosystem features.

## How to use this file

This file is the authoritative overview for:

- which protocol features matter most to this project
- what the external ecosystem status looks like
- what this repo already supports
- what is still only worth tracking

Detailed protocol schemas for payment-related content types live in `xmtp-payment-content-types.md`.
Focused background on typing indicators is retained in `chat-action-typing-indicator.md`, but this document is the authoritative status summary.

## Status legend

- Implemented here: code exists in this repo today
- Track next: not implemented, but directly relevant to current roadmap
- Watch only: useful context, but not currently worth immediate implementation work

## Project-facing summary

| Topic | External status | Repo status | Priority |
| --- | --- | --- | --- |
| Delete / Edit messages | Draft proposals (XIP-76/77) | Not implemented | Track next |
| Actions / Intent | Implemented in libxmtp ecosystem and production variants exist | Implemented end-to-end in daemon / CLI / TUI / mobile | Maintain |
| XIP-51 Agent Messages | Draft, momentum unclear | Not used as primary path | Watch only |
| Wallet Send Calls | Draft | Not implemented in product flow | Track next |
| Transaction Reference | Final | Not implemented as first-class Rust/mobile UX in this repo | Track next |
| Typing notifications / ephemeral UX | Draft direction exists (XIP-65) | Not implemented; Rust public API still the gating factor in our design notes | Track next |
| Atomic Membership | Draft | Not implemented | Watch only |
| Passkey identity | Draft | Not implemented | Watch only |
| Disappearing messages | Draft | Not implemented | Watch only |
| Messaging fees | Final at protocol level, rollout details evolving | No payer-flow implementation here | Watch only |

## Topics most relevant to this project

### 1. XIP-76 / XIP-77: Delete / Edit Messages

Current read:

- user demand is high
- the previous merged proposal path was withdrawn and split into separate delete and edit proposals
- final wire and client behavior are still not stable enough to hard-implement in this repo without churn

Project implication:

- keep message-domain models ready for `edited` and `deleted` style state
- avoid baking assumptions that every message is immutable forever

Recommended next step:

- reserve state in shared message/history types before feature work starts elsewhere

### 2. Actions / Intent

This is no longer just a research topic for this repo.

Current read:

- Actions / Intent has become the practical interaction pattern for structured agent choices
- the older XIP-51 agent-labeling discussion is not the main path we should optimize for

Current repo status:

- daemon decodes Actions / Intent payloads
- CLI ACP bridge can emit and consume structured Actions / Intent flows
- TUI renders Actions payloads
- mobile has Actions buttons and Intent sending support

Operational implication:

- future agent UX work should assume Actions / Intent is the structured-choice baseline
- research follow-up should focus on interoperability, rendering parity, and UX polish, not on proving basic feasibility again

### 3. XIP-51: Agent Messages

Current read:

- the draft explored multiple ways to label agent-originated messages
- momentum appears weak relative to the practical adoption of Actions / Intent
- it may still matter later for metadata or mixed human/agent identity presentation, but it is not the current implementation center of gravity

Project implication:

- do not block agent UX or bridge work on XIP-51 becoming final
- treat it as a future metadata enhancement, not a prerequisite

### 4. XIP-59: Wallet Send Calls

Current read:

- strategically important for payments and agent-triggered transaction UX
- still draft, so wire details and ecosystem ergonomics may continue to move

Project implication:

- relevant to agent payment flows and the wallet-connect plan
- should be evaluated together with `TransactionReference`, not as an isolated content type
- detailed schema and current SDK caveats live in `xmtp-payment-content-types.md`

Recommended next step:

- keep this feature in the roadmap, but ground implementation planning in actual SDK behavior, not just XIP text

### 5. TransactionReference

Current read:

- finalized content type for sharing a completed transaction reference in-chat
- naturally pairs with Wallet Send Calls and wallet-signing UX

Project implication:

- likely the cleaner first-class receipt/result artifact once a transaction is executed
- should be considered part of the same product flow as wallet execution and explorer linking

### 6. XIP-65: Typing Notifications / ephemeral chat action

Current read:

- ephemeral messaging is the right conceptual channel for typing / thinking / processing indicators
- using normal persisted messages to simulate typing would pollute history and add migration pain

Current repo guidance:

- do not fake this with normal messages just to ship something quickly
- wait until the Rust/public SDK surface needed for ephemeral send/stream is practical for this codebase

Recommended next step:

- keep tracking Rust SDK support for ephemeral send/stream APIs
- once the SDK surface exists, design a small transient status model for typing / thinking / tool-running states

### 7. XIP-80: Atomic Membership

Current read:

- conceptually important for multi-installation or multi-instance agent presence in a group
- not urgent for current single-runtime work

Project implication:

- worth tracking for future multi-agent or horizontally scaled agent runtimes
- not a near-term dependency for current bridge or mobile tasks

## Other ecosystem items worth watching

| Topic | Why it matters | Recommended stance |
| --- | --- | --- |
| XIP-55 Passkey Identity | lowers wallet-friction for onboarding | Watch only |
| XIP-58 Disappearing Messages | product-level messaging UX feature | Watch only |
| XIP-63 MIMI | interoperability and regulatory relevance | Watch only |
| XIP-49 Decentralized Backend | protocol architecture and ops implications | Watch only |
| XIP-57 Messaging Fees | direct app/agent operating-cost implications | Watch only |
| Coinbase `actions:1.0` | production precedent for interactive message cards | Relevant reference implementation |

## What changed relative to older notes

- Actions / Intent in this repo is now implemented, so it should no longer appear as a mere research TODO.
- Typing-indicator guidance has been folded into this document's project status summary.
- Payment content types are summarized here, but their schemas and SDK caveats live in `xmtp-payment-content-types.md` to avoid duplication.

## Current action list

- [ ] Reserve edit/delete state in shared message structures before implementation pressure rises
- [ ] Track Rust/public SDK support for ephemeral send/stream APIs
- [ ] Evaluate Wallet Send Calls + TransactionReference as one product flow, not two isolated content types
- [ ] Keep Actions / Intent behavior consistent across daemon, CLI, TUI, and mobile
