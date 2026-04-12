# Chat Action / Typing Indicator 调研

Date: 2026-04-06
Last verified against repo: 2026-04-12
Status: focused appendix; authoritative project status now lives in `xmtp-active-proposals.md`

## Why this file still exists

This file preserves the useful reasoning behind the typing-indicator decision, but it is no longer the primary status document. Use `xmtp-active-proposals.md` for current project-facing status.

## 需求

发送"正在输入"、"正在处理"等实时状态，提升聊天 UX，尤其是 Agent 场景下的长时间思考反馈。

## 可保留的经验结论

### 1. 参考产品机制

Telegram `sendChatAction` 的关键经验不是动作种类，而是机制：

- 短 TTL
- 自动过期
- 不进入历史
- 长操作期间周期性刷新

这些经验仍然适用于 XMTP agent UX 设计。

### 2. 正确通道应是 ephemeral，而不是普通消息

XMTP MLS v3 已有独立的 ephemeral topic（`gE-` 前缀），消息实时送达但不持久化到历史。对 typing / thinking / processing 这类状态来说，这是正确方向。

### 3. 不要用普通消息模拟 typing

如果用普通消息模拟：

- 会污染聊天历史
- 需要额外过滤逻辑
- 日后迁移到真正 ephemeral API 时会留下兼容负担

### 4. 当前 Rust 公共 SDK 仍是主要门槛

在最初调研时，`xmtp` crate v0.8.1 未暴露可直接用于这类功能的 ephemeral send/stream API。这个判断仍然是本项目设计上的保守前提：没有合适 API 时，不要为了“先有个效果”做脏实现。

## 当相关 SDK 能力成熟后的设计方向

- 使用 ephemeral 通道承载 typing / thinking / executing 状态
- TTL 由应用层管理
- Agent 场景下可扩展为 thinking / executing / fetching 等细粒度状态
- 不应写入常规消息历史

## 当前结论

这份专题仍保留其核心结论：

- 等待合适的 ephemeral API 是更干净的实现路径
- 经验值得保留
- 当前项目状态请以 `xmtp-active-proposals.md` 为准
