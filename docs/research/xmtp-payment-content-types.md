# XMTP 支付相关内容类型调研

## 状态总览

| XIP | 状态 | JS SDK | Rust SDK | 实际可用 |
|-----|------|--------|----------|----------|
| XIP-21 TransactionReference | Final | v2.0.2 | 不支持 (Content::Unknown) | JS 端可用 |
| XIP-59 WalletSendCalls | Draft | v2.0.0 | 不支持 (Content::Unknown) | JS 端可用 |
| XIP-57 Messaging Fees | Final | N/A (协议层) | N/A | 未执行，等审计 |
| x402 | 非 XIP | npm 生态 | 无 | 100M+ 交易 |

## XIP-21: Transaction Reference (Final)

链上交易引用，用于在聊天中分享/展示一笔已完成的交易。

```
Content Type: xmtp.org/transactionReference:1.0

{
  chainId: number          // EIP-155 chain ID (必填)
  reference: string        // transaction hash (必填)
  networkId?: number
  metadata?: {
    transactionType: string  // e.g. "payment"
    currency: string         // e.g. "ETH", "USDC"
    amount: bigint
    decimals: number
    fromAddress: string
    toAddress: string
  }
}
```

## XIP-59: Wallet Send Calls (Draft)

在消息中触发链上交易，对齐 EIP-5792 `wallet_sendCalls`。

```
Content Type: xmtp.org/walletSendCalls:1.0

{
  version: string
  chainId: "0x..." (hex)
  from: "0x..." (hex)
  calls: [{
    to?: "0x..."
    data?: "0x..."
    value?: "0x..." (hex wei)
    gas?: "0x..."
    metadata?: { description, transactionType, ... }
  }]
  capabilities?: { paymasters, bundling, ... }
}
```

## XIP-57: Messaging Fees (Final, 未执行)

- 付费方: App/Agent (Payer)，非终端用户
- 三部分费用: 每条消息固定费 + 每字节天存储费 + 拥塞费
- 支付: USDC on Base，最低 $10 存入 Payer Registry 合约
- 现状: 主网已上线但费用未强制执行，等 Trail of Bits + Octane 审计完成

## x402 (Coinbase 支付协议)

非 XMTP 原生，是独立的 HTTP 402 支付协议。与 XMTP 为应用层集成：Agent 收到聊天请求 → HTTP 402 → EIP-3009 签名授权 → 返回结果。

- V2 已发布 (2025-12)，支持可重用 session、多链、服务发现
- x402 Foundation 由 Coinbase + Cloudflare 共同创立，Google/Visa 加入
- 研究计划见 `.cache/research/x402-xmtp-integration-research.md`

## Rust 实现路径

要在 daemon/TUI 中支持这些内容类型，需在 `xmtp-fork/xmtp/src/content.rs` 的 `Content` enum 中添加对应 variant 并实现 protobuf content bytes 的反序列化。当前均落入 `Content::Unknown { content_type, raw }`。
