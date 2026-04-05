# XMTP 身份解析调研

## 现状

XMTP 协议**不支持用户档案**（无 display name、avatar），这是刻意设计——"协议管通信，身份归应用"。

- **Inbox ID** 是核心标识符（钱包地址 SHA256），不含 profile 字段
- **XIP-60 Private Contacts**（Draft）：给别人设私有昵称，跨设备同步——是"备注名"，不是用户 Profile
- **XIP-14** 曾提议标准化 `displayInfo.prettyName` / `profileImage`，已 Stagnant

## 方案对比

| 方案 | 类型 | 数据 | 依赖 | 隐私 |
|------|------|------|------|------|
| ENS 反向解析 | 链上 RPC | ENS name only | Ethereum RPC（公共/Infura/Alchemy） | 好（RPC 可自建） |
| Airstack | 聚合 GraphQL API | ENS + Farcaster + Lens + 头像 + XMTP enabled | API key + 中心化服务 | 差（地址发给第三方） |
| Everyname | 聚合 REST API | 多命名服务解析 | API key + 中心化服务 | 差 |
| Convos Quickname | 自建系统 | 自定义名称/头像 | Convos 后端 | 中 |

## Airstack 详情

- **接口**: GraphQL POST `https://api.airstack.xyz/graphql`，需 `Authorization` header
- **免费额度**: 有，无需信用卡，CLI 场景够用
- **一次查询返回**: ENS primary name、所有 ENS 域名、Farcaster username/PFP、Lens handle/PFP、XMTP enabled 状态
- **Rust 实现**: 无官方 SDK，用 reqwest + serde_json 即可
- **延迟**: ~200-500ms/次

**顾虑**:
- 隐私：每次解析地址发给 Airstack 服务器
- 中心化：单一商业依赖
- API key：需要用户自行获取或内嵌（安全风险）
- 数据新鲜度：链上变更有分钟级延迟

## ENS 直连详情

- **Rust 实现**: `alloy` 库内置 ENS resolver，`provider.lookup_address(addr)` 一行搞定
- **依赖**: 任意 Ethereum RPC（公共免费节点即可）
- **局限**: 只有 ENS，无 Farcaster/Lens

## 建议路径

1. **当前**: 截断地址显示 `0x1234...5678`（已实现）
2. **中期**: Airstack 集成，API key 走用户配置，拿到最丰富的身份聚合
3. **备选**: ENS 直连作为无 API key 时的 fallback
4. **缓存**: 本地 LRU 缓存解析结果，ENS/社交名称变化不频繁

## 相关 XIPs

| XIP | 标题 | 状态 | 关系 |
|-----|------|------|------|
| XIP-46 | Multi-Wallet Identity | Final | Inbox ID 身份模型 |
| XIP-60 | Private Contacts | Draft | 私有联系人昵称 |
| XIP-51 | Agent Messages | Draft | Agent 自我标识 |
| XIP-14 | Conversation Context Metadata | Stagnant | 曾提议 profile schema |
