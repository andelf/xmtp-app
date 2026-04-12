# XMTP 身份解析调研

Date: 2026-04-06
Last verified against repo: 2026-04-12
Purpose: evaluate practical identity-resolution options without confusing protocol identity with app-level profile systems.

## 现状

XMTP 协议不支持用户档案（无 display name、avatar），这是刻意设计：协议负责通信，身份展示由应用层决定。

- **Inbox ID** 是核心标识符，不自带 profile 字段
- **XIP-60 Private Contacts**（Draft）更像跨设备同步的私有备注，不是公共 profile
- **XIP-14** 曾提议标准化 `displayInfo.prettyName` / `profileImage`，但已停滞

## 当前仓库状态

截至当前代码库：

- 尚未看到 Airstack 或 ENS 直连集成落地
- 当前默认展示策略仍以地址截断和本地派生头像为主
- 因此这份文档仍是“未来身份增强方案评估”，不是对现状功能的描述

## 方案对比

| 方案 | 类型 | 数据 | 依赖 | 隐私 |
|------|------|------|------|------|
| ENS 反向解析 | 链上 RPC | ENS name only | Ethereum RPC（公共/Infura/Alchemy） | 好（RPC 可自建） |
| Airstack | 聚合 GraphQL API | ENS + Farcaster + Lens + 头像 + XMTP enabled | API key + 中心化服务 | 差（地址发给第三方） |
| Everyname | 聚合 REST API | 多命名服务解析 | API key + 中心化服务 | 差 |
| Convos Quickname | 自建系统 | 自定义名称/头像 | Convos 后端 | 中 |

## Airstack 详情

- 接口：GraphQL POST `https://api.airstack.xyz/graphql`，需 `Authorization` header
- 免费额度：有，无需信用卡，CLI 场景够用
- 一次查询返回：ENS primary name、所有 ENS 域名、Farcaster username/PFP、Lens handle/PFP、XMTP enabled 状态
- Rust 实现：无官方 SDK，用 `reqwest + serde_json` 即可
- 延迟：约 `200-500ms/次`

顾虑：

- 隐私：每次解析地址都要发给第三方
- 中心化：单一商业依赖
- API key：需要用户配置或托管，带来安全与产品复杂度
- 数据新鲜度：链上/社交数据并非总是实时同步

## ENS 直连详情

- Rust 实现：`alloy` 库支持 ENS 解析，`provider.lookup_address(addr)` 即可完成基本反查
- 依赖：任意 Ethereum RPC
- 局限：只有 ENS，没有 Farcaster/Lens 等聚合社交资料

## 建议路径

1. **当前继续保持简单默认值**：地址截断显示 `0x1234...5678`
2. **中期优先考虑 Airstack**：如果产品确实需要 richer identity，再用用户配置的 API key 换取更完整资料
3. **备选方案**：ENS 直连作为无 API key 时的轻量 fallback
4. **缓存**：无论走哪条路，都应有本地 LRU/TTL 缓存，减少重复解析和第三方请求

## 选择标准

如果未来真的要做身份增强，建议用这几个问题做决策：

- 我们更看重隐私，还是更看重资料丰富度？
- CLI / TUI / mobile 是否都需要一致身份视图？
- 用户是否愿意提供第三方 API key？
- profile 只是展示增强，还是要成为排序/搜索/联系人管理的基础？

## 相关 XIPs

| XIP | 标题 | 状态 | 关系 |
|-----|------|------|------|
| XIP-46 | Multi-Wallet Identity | Final | Inbox ID 身份模型 |
| XIP-60 | Private Contacts | Draft | 私有联系人昵称 |
| XIP-51 | Agent Messages | Draft | Agent 自我标识相关，但不是用户 profile 方案 |
| XIP-14 | Conversation Context Metadata | Stagnant | 曾尝试定义 profile schema |

## 当前结论

这份调研目前仍有效：

- 协议层没有现成 profile 可以直接拿来用
- 现阶段继续使用地址截断显示是合理的
- 如果要增强，Airstack 是功能最强但隐私最弱的方案
- ENS 直连是更朴素、更去中心化的 fallback
