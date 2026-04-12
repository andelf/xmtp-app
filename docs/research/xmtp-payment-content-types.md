# XMTP 支付相关内容类型调研

Date: 2026-04-06
Last verified against repo: 2026-04-12
Purpose: keep one authoritative note for XMTP payment-adjacent content types and their practical implications for this repo.

## How to use this file

Use this document for:

- payment-related XMTP content-type schemas
- current support caveats in this repo
- protocol-vs-project distinctions that are easy to get wrong

If you are planning mobile wallet UX work, pair this file with `../wallet-connect-integration-plan.md`.

## Important correction

Earlier notes in this area mixed broad ecosystem support with this repo's actual implementation status. This version separates them explicitly:

- a content type being available somewhere in the XMTP JS ecosystem does not mean `@xmtp/react-native-sdk` v5 gives this repo native codec support
- a content type being unsupported in the Rust crate's typed decode path does not mean the app cannot add an application-layer fallback
- this repo does not currently use an `xmtp-fork/` path; implementation guidance must be expressed against the actual workspace and dependency layout

## Status overview

| Item | Protocol status | Broader ecosystem note | Current repo status |
| --- | --- | --- | --- |
| XIP-21 TransactionReference | Final | ecosystem support exists, but integration details vary by SDK | not implemented as a first-class UX flow in this repo |
| XIP-59 WalletSendCalls | Draft | ecosystem experimentation exists, but SDK ergonomics vary | not implemented as a first-class UX flow in this repo |
| XIP-57 Messaging Fees | Final at protocol level | rollout and enforcement details still matter operationally | no payer-flow implementation here |
| x402 | Not an XMTP XIP | independent payment protocol with possible app-layer composition | separate research area, not an XMTP-native content type |

## Project reality by runtime

### Rust daemon / TUI / CLI

Current repo reality:

- workspace dependency is `xmtp = 0.8.1`
- unsupported or not-yet-handled content types may surface through `Content::Unknown` or equivalent fallback handling paths
- adding product support in this repo is an application-layer integration problem first, not a matter of editing a nonexistent local `xmtp-fork/` path

Implication:

- when implementing a new payment content type, first verify what the current dependency already exposes
- then decide whether the repo should decode at the application layer, wait for upstream support, or both

### Mobile / React Native

Current repo reality:

- this repo's mobile app uses `@xmtp/react-native-sdk` v5 directly
- the companion `wallet-connect-integration-plan.md` already notes that WalletSendCalls and TransactionReference do not arrive with convenient native JS codec support in this project today
- in practice, mobile work here should assume encoded/unknown/native-content fallback paths until proven otherwise

Implication:

- do not write project docs that say simply “JS 端可用” without qualifying which SDK and integration path is meant

## XIP-21: TransactionReference

用途：在聊天中分享或回传一笔已经完成的链上交易。

### Canonical shape

Content type: `xmtp.org/transactionReference:1.0`

```json
{
  "chainId": 1,
  "reference": "0x...",
  "networkId": 1,
  "metadata": {
    "transactionType": "payment",
    "currency": "USDC",
    "amount": "1000000",
    "decimals": 6,
    "fromAddress": "0x...",
    "toAddress": "0x..."
  }
}
```

### Why it matters here

- best fit for “transaction completed” receipts inside a chat flow
- natural companion to WalletSendCalls-initiated execution flows
- likely the user-visible result object once a wallet signs and broadcasts

### Current repo guidance

- evaluate this together with WalletSendCalls and wallet deep-link UX
- if implemented on mobile first, the renderer should show explorer-friendly information and a safe summary, not just raw JSON

## XIP-59: WalletSendCalls

用途：在消息中请求对方钱包执行链上交易，对齐 EIP-5792 `wallet_sendCalls`。

### Canonical shape

Content type: `xmtp.org/walletSendCalls:1.0`

```json
{
  "version": "1",
  "chainId": "0x1",
  "from": "0x...",
  "calls": [
    {
      "to": "0x...",
      "data": "0x...",
      "value": "0x...",
      "gas": "0x...",
      "metadata": {
        "description": "Transfer 1 USDC",
        "transactionType": "transfer"
      }
    }
  ],
  "capabilities": {
    "paymasters": {},
    "bundling": {}
  }
}
```

### Why it matters here

- this is the most direct bridge between agent UX and wallet execution UX
- if the repo grows payment-oriented agent flows, this content type becomes strategically important

### Current repo guidance

- treat it as a product flow with TransactionReference, not a standalone renderer task
- the first implementation should prioritize safe rendering, clear user intent, and wallet handoff reliability over supporting every schema edge case

## XIP-57: Messaging Fees

- payer is the app / agent, not the end user
- fee model affects operating cost and production economics more than day-one UI
- implementation work here should wait for concrete business or infra need, not be bundled into wallet UX by default

## x402

x402 is not an XMTP-native content type. It is a separate HTTP 402 payment protocol that can be composed with chat-driven agent flows at the application layer.

Why keep it in this note at all:

- it is adjacent to agent payment design
- people may otherwise confuse it with an XMTP content type

But operationally:

- do not mix x402 assumptions into XMTP content-type implementation docs
- treat x402 as adjacent architecture research, not protocol decoding work

## Recommended implementation posture for this repo

1. Keep protocol schema knowledge here.
2. Keep mobile execution and wallet UX steps in `../wallet-connect-integration-plan.md`.
3. Verify actual SDK behavior before claiming support at the project level.
4. Prefer explicit wording such as:
   - “supported by some XMTP JS tooling”
   - “not natively ergonomic in this repo's React Native path today”
   - “requires application-layer decoding in this repo”

## What was intentionally removed from older notes

- the incorrect `xmtp-fork/xmtp/src/content.rs` implementation path
- overly broad “JS 端可用” wording that blurred ecosystem support with this repo's actual SDK situation
