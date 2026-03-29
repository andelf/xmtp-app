# Plan: Chat Detail Screen

> DM Detail 和 Group Detail 两个独立页面，从 Chat Screen Header Bar 进入。

## 需要修改的文件

### 新增文件

| 文件 | 说明 |
|------|------|
| `app/(main)/conversation/dm-detail.tsx` | DM Detail Screen：peer 完整地址、inbox ID、conversation ID、topic、创建时间 |
| `app/(main)/conversation/group-detail.tsx` | Group Detail Screen：群名、成员数、成员列表、conversation ID、topic、创建时间 |

### 修改文件

| 文件 | 改动 |
|------|------|
| `app/(main)/_layout.tsx` | Stack 注册 `conversation/dm-detail` 和 `conversation/group-detail` 两个路由 |
| `app/(main)/conversation/[id].tsx` | Header Bar 右侧加 `ℹ` 按钮，根据 `kind` 跳转 dm-detail 或 group-detail，传递 conversation id |
| `docs/mobile-glossary.md` | 新增 DM Detail Screen、Group Detail Screen 术语定义 |

### 不需要修改的文件

- `src/store/conversations.ts` — `ConversationItem` 已有 `id`、`topic`、`kind`、`peerInboxId`、`createdAt`，数据足够
- `src/store/settings.ts` — 无关
- `src/components/MessageBubble.tsx` — 无关

## 数据获取方案

### DM Detail

| 字段 | 来源 |
|------|------|
| Peer Full Address | 进入页面时调用 SDK：`dm.members()` → 找到非自己的 member → `member.identities[0].identifier` |
| Peer Inbox ID | `ConversationItem.peerInboxId`（store 已有）|
| Conversation ID | `ConversationItem.id` |
| Conversation Topic | `ConversationItem.topic` |
| Created At | `ConversationItem.createdAt`（epoch ms → 格式化） |

### Group Detail

| 字段 | 来源 |
|------|------|
| Group Name | `ConversationItem.title` |
| Member List | 进入页面时调用 SDK：`group.members()` → 遍历取 `identities[0].identifier` |
| Member Count | `members.length` |
| Conversation ID | `ConversationItem.id` |
| Conversation Topic | `ConversationItem.topic` |
| Created At | `ConversationItem.createdAt` |

> 注意：`members()` 是异步 SDK 调用，页面需要 loading 状态。

## 实施步骤

1. 创建 `dm-detail.tsx` — InfoRow 组件复用 About 页面模式（label + selectable value + tap to copy）
2. 创建 `group-detail.tsx` — 同上 + FlatList 展示成员列表
3. 修改 `_layout.tsx` — 注册两个新路由
4. 修改 `[id].tsx` — Header Bar 加 `headerRight` 按钮，读取 conversation kind 决定跳转目标
5. 更新 `mobile-glossary.md` — 新增两个 Screen 定义
6. ESLint + TypeScript 检查
7. 构建验证

## 验证方案

### 自动化验证

```bash
# Lint
cd xmtp-mobile && npx eslint "app/(main)/conversation/dm-detail.tsx" "app/(main)/conversation/group-detail.tsx" "app/(main)/conversation/[id].tsx" "app/(main)/_layout.tsx"

# TypeScript
npx tsc --noEmit

# Tests
npx jest --passWithNoTests
```

### 手动验证（需要设备）

| 场景 | 预期 |
|------|------|
| 打开 DM 聊天 → 点击 ℹ | 进入 DM Detail，显示完整地址、inbox ID 等 |
| 打开 Group 聊天 → 点击 ℹ | 进入 Group Detail，显示群名、成员列表等 |
| DM Detail 点击地址 | 复制到剪贴板 |
| Group Detail 成员列表 | 显示所有成员地址，可点击复制 |
| 返回按钮 | 正常返回 Chat Screen |
| DM Detail 加载中 | 显示 loading indicator（members() 异步） |

## 风险

- `members()` 在网络差时可能较慢或失败 → 需要 error state + 重试或提示
- expo-router 动态路由 `conversation/dm-detail` 和 `conversation/[id]` 在同级目录 → 需确认 expo-router 不会把 `dm-detail` 误匹配为 `[id]`（应该不会，静态路由优先于动态路由）
