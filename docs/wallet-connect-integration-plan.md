# WalletConnect + XMTP On-Chain Transaction Integration Plan

> Last updated: 2026-03-29

## Goal

Enable mobile-to-mobile crypto payments within XMTP chat conversations. An agent or user sends a transaction request via XMTP message; the recipient reviews, approves via their wallet app, and sends back a transaction receipt — all within the chat flow.

## XMTP Transaction Content Types

This implementation plan intentionally does not duplicate protocol schemas in full.

Authoritative background now lives in:

- `docs/research/xmtp-payment-content-types.md`

For this plan, the only required product-level mapping is:

| Content Type | Direction in the intended UX | Role in the flow |
| --- | --- | --- |
| `WalletSendCalls` | Agent → User | Ask the recipient wallet to execute one or more calls |
| `TransactionReference` | User → Agent | Send back the resulting transaction reference / receipt |

Working assumptions for this repo:

- `WalletSendCalls` and `TransactionReference` should be treated as one product flow, not two unrelated message renderers
- this repo's mobile path should assume encoded or unknown-content fallback handling unless current SDK behavior has been re-verified
- protocol details may evolve independently of this implementation plan, so schema-level edits should happen in the research document above

## Wallet Integration: Reown AppKit

WalletConnect has rebranded to **Reown**. The current React Native SDK is `@reown/appkit-react-native` (v2.0.x). The legacy `@walletconnect/modal-react-native` was archived Dec 2025.

### Package Requirements

```
@reown/appkit-react-native
@reown/appkit-ethers-react-native
@walletconnect/react-native-compat    # MUST be imported first
@react-native-async-storage/async-storage
react-native-get-random-values
react-native-svg
@react-native-community/netinfo
```

### Prerequisites

- **Project ID** from [cloud.reown.com](https://cloud.reown.com) (free)
- App deep link scheme registered (e.g. `xmtpmobile://`)
- Custom dev client (not Expo Go) — already the case for xmtp-mobile

---

## Architecture

```
┌──────────────────────────────────────────────────┐
│                 Chat Screen                       │
│                                                   │
│  ┌─────────────────────────────────────────────┐  │
│  │  Agent message:                             │  │
│  │  [WalletSendCalls]                          │  │
│  │  ┌────────────────────────────────────────┐ │  │
│  │  │ 💰 Transfer 0.01 ETH                  │ │  │
│  │  │ To: 0xAbC...dEf                        │ │  │
│  │  │ Chain: Ethereum Mainnet                │ │  │
│  │  │                                        │ │  │
│  │  │  [ Review & Sign ]                     │ │  │
│  │  └────────────────────────────────────────┘ │  │
│  └─────────────────────────────────────────────┘  │
│                                                   │
│  User taps "Review & Sign"                        │
│  ├─ If wallet connected → deep link to wallet     │
│  └─ If not connected → open AppKit modal          │
│                                                   │
│  After wallet approval:                           │
│  ┌─────────────────────────────────────────────┐  │
│  │  [TransactionReference] (auto-sent)         │  │
│  │  ✅ Transaction sent                        │  │
│  │  0xabc123...  (tap to view on explorer)     │  │
│  └─────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────┘
```

## Implementation Phases

### Phase 1: Content Type Parsing & Display

**Goal:** Render WalletSendCalls and TransactionReference messages in chat bubbles.

**Changes:**

1. **`src/utils/nativeContent.ts`** — Add detection for transaction content types
   - These arrive as `nc.encoded` or `nc.unknown` with `contentTypeId` matching `xmtp.org/walletSendCalls:1.0` or `xmtp.org/transactionReference:1.0`
   - Decode the JSON payload from base64 (same path as markdown)

2. **`src/store/messages.ts`** — Parse into `MessageItem`
   - New `contentType` values: `"xmtp.org/walletSendCalls:1.0"`, `"xmtp.org/transactionReference:1.0"`
   - Store the full decoded JSON in a new `MessageItem.payload?: Record<string, any>` field
   - `text` field gets a human-readable summary for preview/copy

3. **`src/components/TransactionBubble.tsx`** — New component
   - **WalletSendCalls**: Card showing amount, recipient, chain, description, "Review & Sign" button
   - **TransactionReference**: Card showing tx hash (truncated), status indicator, "View on Explorer" link
   - Renders inside MessageBubble when `contentType` matches

4. **`src/components/MessageBubble.tsx`** — Route to TransactionBubble
   ```typescript
   const isTx = item.contentType?.includes("walletSendCalls") ||
                item.contentType?.includes("transactionReference");
   // if (isTx) render <TransactionBubble /> instead of text
   ```

### Phase 2: Wallet Connection (Reown AppKit)

**Goal:** Allow users to connect their wallet for signing transactions.

**Changes:**

1. **`app/_layout.tsx`** — Initialize AppKit provider
   ```typescript
   import '@walletconnect/react-native-compat'; // MUST be first import
   // ... wrap app in AppKitProvider
   ```

2. **`src/xmtp/wallet.ts`** — New module for wallet state
   - `initAppKit(projectId, metadata)` — initialize once
   - Re-export hooks: `useAppKit`, `useAccount`, `useProvider`

3. **`app.json`** — Register scheme and wallet query schemes
   ```json
   {
     "scheme": "xmtpmobile",
     "ios": {
       "infoPlist": {
         "LSApplicationQueriesSchemes": ["metamask", "rainbow", "cbwallet", "trust"]
       }
     }
   }
   ```

4. **Wallet connect button** — Add to chat screen header or settings
   - Show connected wallet address (truncated) or "Connect Wallet" button
   - Use `useAppKit().open()` for wallet selection modal

### Phase 3: Transaction Execution

**Goal:** Tap "Review & Sign" → wallet signs → send TransactionReference back.

**Changes:**

1. **`src/xmtp/transactions.ts`** — Core transaction logic
   ```typescript
   async function executeWalletSendCalls(
     conversationId: ConversationId,
     walletSendCalls: WalletSendCalls,
     walletProvider: EIP1193Provider
   ): Promise<string> // returns tx hash
   ```
   - Parse calls from WalletSendCalls payload
   - Build ethers transaction from `calls[0]` (single call) or batch via EIP-5792
   - Call `signer.sendTransaction(tx)` — this deep links to wallet for approval
   - On success: auto-send TransactionReference message back to conversation
   - On failure/reject: show error toast, no message sent

2. **`src/components/TransactionBubble.tsx`** — Wire up "Review & Sign"
   - Check wallet connection state
   - If not connected: prompt connection first
   - If connected: call `executeWalletSendCalls()`
   - Show loading state while waiting for wallet approval
   - Display "Waiting for wallet..." with manual-open fallback

3. **Sending TransactionReference** — Via NativeMessageContent
   ```typescript
   // Same pattern as sendReaction — direct NativeMessageContent
   convo.send({
     // The exact shape depends on how the native bridge accepts it.
     // May need to use the encoded path if no native codec exists.
   });
   ```
   - If native bridge doesn't have a dedicated field: encode as JSON, base64, and send via the encoded content type path
   - Fallback: send as text message with structured format

### Phase 4: UX Polish

1. **Transaction status polling** — After sending, poll etherscan/block explorer API for confirmation
2. **Explorer links** — Tap tx hash → open block explorer (etherscan, basescan, etc.) based on chainId
3. **Chain name resolution** — Map chainId to human-readable name (1 → Ethereum, 8453 → Base, etc.)
4. **Amount formatting** — Parse hex value + decimals into human-readable amount (e.g. "0.01 ETH")
5. **Security warnings** — Show caution banner for unverified agents, large amounts, or unfamiliar contracts
6. **Conversation preview** — Show `[tx] Transfer 0.01 ETH` in conversation list

---

## Known Risks & Mitigations

### Wallet Redirect Unreliability

**Problem:** After wallet approval, the wallet may not redirect back to the app. This is a known ecosystem issue (WC GitHub #3543, #4785).

**Mitigation:**
- Show "Waiting for wallet approval..." with a countdown timer
- After 15s: show "Still waiting? Open your wallet manually" with button
- After 60s: show "Timed out — try again"
- Poll for tx hash independently via `provider.waitForTransaction()`

### Auto-Switch Loop

**Problem:** MetaMask takes 5-10s to render the approval dialog. If user switches back, the dApp re-triggers the deep link, creating a loop.

**Mitigation:**
- Debounce wallet deep-link triggers (minimum 10s between attempts)
- Show instruction: "Please stay in your wallet until the approval prompt appears"

### Content Type Not Natively Supported

**Problem:** `@xmtp/react-native-sdk` v5 doesn't have JS codecs for WalletSendCalls or TransactionReference. They arrive as `unknown` or `encoded`.

**Mitigation:**
- Already solved: our `nativeContent.encoded` decoding path handles this
- Parse the base64 JSON payload and type-check the structure
- Sending back: use the encoded path or raw NativeMessageContent

### Session Expiry

**Problem:** WC sessions expire (inactive: 5min, active: 30 days).

**Mitigation:**
- Persist sessions via AsyncStorage
- On session expiry: prompt reconnection, don't silently fail
- Check connection state before every transaction attempt

---

## File Structure (New Files)

```
src/
├── xmtp/
│   ├── wallet.ts              # AppKit initialization, hooks re-export
│   └── transactions.ts        # executeWalletSendCalls, sendTransactionReference
├── components/
│   └── TransactionBubble.tsx   # Render WalletSendCalls / TransactionReference cards
└── utils/
    └── chains.ts              # chainId → name/explorer URL mapping
```

## Dependencies (New)

```
@reown/appkit-react-native
@reown/appkit-ethers-react-native
@walletconnect/react-native-compat
@react-native-async-storage/async-storage   # likely already present
react-native-get-random-values
react-native-svg                            # likely already present
@react-native-community/netinfo
```

## Out of Scope (for now)

- **Batch transactions** (multi-call in WalletSendCalls) — handle only `calls[0]` initially
- **WalletConnect Pay** (merchant gateway) — consider later for POS use cases
- **Transaction simulation** (Tenderly/Alchemy) — nice-to-have for security
- **ERC-20 token transfers** — start with native ETH, add token support later
- **Replace login with WalletConnect** (P3.3 backlog) — separate effort, can share AppKit setup
