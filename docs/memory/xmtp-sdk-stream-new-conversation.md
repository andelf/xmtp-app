# XMTP SDK: streamAllMessages 收到未知会话的消息

## 问题

别人首次给你发消息时，会话列表不会自动出现新会话，且已有会话的 lastMessage 始终显示 "No message yet"。

## 根因

`@xmtp/react-native-sdk` 的两个 stream API 职责不同：

- `client.conversations.stream()` — 只在**本地创建**新会话时触发，**不会**在别人首次给你发消息时触发
- `client.conversations.streamAllMessages()` — 监听**所有会话**的新消息，包括尚未同步到本地的会话

当 `streamAllMessages` 收到一条消息，其 `topic` 不在本地的 `topicToId` Map 中时（即来自未知会话），如果直接 `return` 跳过，就会导致：
1. 新会话永远不出现在列表中
2. 已有会话的 lastMessage 预览不更新

## 修复

在 `streamAllMessages` 回调中，当 `getIdByTopic(topic)` 返回 null 时：

```typescript
if (!conversationId) {
  // 未知会话 → 同步完整会话列表
  await store().fetchAll();
  // 重新查找
  conversationId = store().getIdByTopic(topicStr);
  if (!conversationId) return; // 仍然找不到才放弃
}
```

## 额外修复

`conversationToItem()` 在获取 lastMessage 预览前需要先调用 `conversation.sync()`，否则本地消息库可能是空的，导致 `messages({ limit: 1 })` 返回空数组。

```typescript
try { await conversation.sync(); } catch {}
const messages = await conversation.messages({ limit: 1 });
```

## 教训

不要假设 XMTP SDK 的 conversation stream 和 message stream 覆盖了所有场景。`conversations.stream()` 只监听本地创建的会话，不等于"有新会话到达"。真正的新会话发现要靠 `streamAllMessages` + 按需 `fetchAll()`。
