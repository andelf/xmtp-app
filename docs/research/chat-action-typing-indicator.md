# Chat Action / Typing Indicator 调研

## 需求

发送"正在输入"、"正在处理"等实时状态，提升聊天 UX，尤其是 Agent 场景下的长时间思考反馈。

## Telegram 参考

Telegram `sendChatAction` 支持 11 种动作：

| Action | 显示文本 |
|--------|----------|
| typing | "正在输入..." |
| upload_photo | "正在发送照片..." |
| record_video | "正在录制视频..." |
| upload_video | "正在发送视频..." |
| record_voice | "正在录制语音..." |
| upload_voice | "正在发送语音..." |
| upload_document | "正在发送文件..." |
| choose_sticker | "正在选择贴纸..." |
| find_location | "正在查找位置..." |
| record_video_note | "正在录制视频消息..." |
| upload_video_note | "正在发送视频消息..." |

核心机制：**5 秒 TTL 自动过期**，不持久化，发消息后自动清除，长操作需每 5 秒重发。

## XMTP 现状

### Ephemeral Message 基础设施

XMTP MLS v3 已有独立的 ephemeral topic（`gE-` 前缀），消息实时送达但**不持久化到历史**。这正是 chat action 的理想通道。

### XIP-65: Typing Notifications

- 2025 年 5 月提出，社区反馈正面，**尚未正式采纳**
- 基于 ephemeral message 实现
- 动机明确提到 Agent 场景："a big UX boost, especially with agents thinking"

### Rust SDK 支持

当前 `xmtp` crate v0.8.1 **未暴露 ephemeral send/stream API**，无法在 Rust 层使用。

## 结论

**等待标准化是最佳策略**：

1. XMTP 已有 ephemeral 基础设施，XIP-65 方向明确
2. Rust SDK 支持 ephemeral API 后，实现会非常干净——发送端用 ephemeral send，接收端用 streamEphemeral，零历史污染
3. 如果用普通消息模拟，会污染消息历史，需要应用层过滤，迁移成本不值得

### 当 Rust SDK 支持后的实现方案

- 自定义 content type `chatAction:1.0`（或跟随 XIP-65 标准）
- 通过 ephemeral topic 发送，不进入消息历史
- 应用层 5 秒 TTL 自动过期
- Agent 场景下可扩展为 thinking / executing / fetching 等状态

### 跟踪

- [ ] 关注 Rust `xmtp` crate 对 ephemeral message API 的支持
- [ ] 关注 XIP-65 正式采纳进度
