# XMTP 社区活跃提案调研

## 与本项目最相关的提案

### XIP-76/77: Delete / Edit Messages (Draft, 讨论热烈)

用户呼声最高的功能。XIP-61 (合并版) 已 Withdrawn，拆分为独立的 Delete 和 Edit 提案。

**对我们的启发**: 消息数据结构应预留 `edited` / `deleted` 状态字段，等定稿后实现成本低。

### XIP-51: Agent Messages (Draft, 战略核心)

提出三种 agent 标识方案，均未定论：
1. 专用 content type `agentMessage` — 向后兼容好，但只能标识文本
2. per-message `isAgent: true` — 所有类型都能标，但可选择不标
3. client 级 `isAgent` — 一次声明全部标识，但无法混合模式

**实际状态**: 参考实现已 404，社区无后续讨论。XMTP 团队转向了 Actions/Intent 作为 agent 交互标准。

### Actions / Intent (已在 libxmtp 实现，非 XIP-51 内容)

Agent 交互的实际标准：

```
Actions { id, description, actions: [{ id, label, style: Primary|Secondary|Danger, expires_at_ns }] }
Intent  { id, action_id, metadata: { key: string|number|boolean } }
```

流程: Agent 发 Actions (结构化按钮) → 用户点击 → 客户端发 Intent → Agent 处理。

**对我们的启发**: ACP bridge 可用 Actions 发送结构化选项卡而非纯文本交互，UX 远优于文字选项。

### XIP-59: Wallet Send Calls (Draft)

在消息中通过 EIP-5792 `wallet_sendCalls` 触发链上交易。npm 包 `@xmtp/content-type-wallet-send-calls` 已发布。

**对我们的启发**: ACP bridge 涉及支付场景时可直接利用此内容类型。

### XIP-80: Atomic Membership (Draft)

允许同一个 inboxId 在群组中有多个 installation 同时在线，解锁 agent 多实例架构。

### XIP-65: Typing Notifications (Draft)

基于 ephemeral message 实现。详见 [chat-action-typing-indicator.md](./chat-action-typing-indicator.md)。

## 值得关注但非紧急

| 提案 | 状态 | 说明 |
|------|------|------|
| XIP-55 Passkey Identity | Draft | 无需钱包即可用 XMTP，降低用户门槛 |
| XIP-58 Disappearing Messages | Draft | 阅后即焚 |
| XIP-63 MIMI | Draft | IETF 互操作标准，欧盟 DMA 驱动 |
| XIP-49 Decentralized Backend | Draft | 主网去中心化架构 |
| XIP-57 Messaging Fee | Final | 主网消息收费 ~$5/10万条 |
| Coinbase `actions:1.0` | 生产中 | 非标准但 Base App 生态广泛使用的交互式卡片 |

## 主网进展

- Ephemera 完成 $20M Series B，估值 $750M
- 去中心化主网三件套: XIP-49 (后端) + XIP-57 (收费) + XIP-54 (节点资质)
- Phase 1: 7 个精选节点运营商

## 我们的行动项

- [ ] 消息结构预留 edit/delete 状态 (XIP-76/77)
- [ ] 研究 Actions/Intent 在 ACP bridge 中的应用
- [ ] 跟踪 Rust SDK 对 ephemeral API 的支持 (XIP-65)
- [ ] 评估 Wallet Send Calls 在支付场景的集成 (XIP-59)
