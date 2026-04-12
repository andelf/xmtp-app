# ACP Interrupt: Cancel 与新 Prompt 的时序竞争

## 问题

在 ACP bridge 中实现"新消息打断当前 turn"时，`cancel` notification 会与新 prompt 产生时序竞争，导致新 prompt 被误杀返回空结果。

## 根因

claude-agent-acp 的 `promptQueueing` 机制导致：

1. Bridge 发送 `cancel` notification
2. Agent 设置 `session.cancelled = true`，resolve 所有 pending queue 中的 prompt 为 `cancelled`
3. Bridge 紧接着发送新 prompt
4. 新 prompt 到达时，如果旧 prompt 还在运行（`session.promptRunning = true`），新 prompt 进入 pending queue
5. 但 cancel 已经把 pending queue 全部 resolve 为 cancelled —— **新 prompt 也被误杀**

即使旧 prompt 已经自然完成（`tokio::select!` 的 race condition），cancel notification 仍然会到达 agent，因为它是异步的。agent 内部的 `prompt()` 方法第一行虽然是 `session.cancelled = false`，但时序上 cancel 可能在新 prompt 处理前到达。

### 时序图

```
Bridge                          Agent (claude-agent-acp)
  |                                |
  |--- cancel notification ------->|  session.cancelled = true
  |--- prompt_fut.await ---------->|  (旧 prompt 完成)
  |--- new prompt() -------------->|  promptRunning 可能还是 true
  |                                |  新 prompt 进入 pending queue
  |                                |  cancel 已经 resolve 了 queue
  |<--- stopReason: cancelled -----|  新 prompt 被误杀（1ms 返回空）
```

## 症状

日志中可见：
```
turn interrupted by new message interrupted_msg=... new_msg=...
prompting agent ... source_message_id="<新消息>"
agent responded ... elapsed_ms=1    ← 极快返回
WARN agent returned empty reply
```

## 解决方案

**两层防御：cancel 后等 500ms + 检测误杀自动重试**。

### 第一层：cancel 后固定等待 500ms

```rust
let _ = conn.cancel(...).await;
let _ = prompt_fut.await;
sleep(Duration::from_millis(500)).await;  // 等 agent 完成 cancel 清理
// 然后才发新 prompt
```

500ms 足够让 agent 完成 cancel 清理（重置 `cancelled` flag、清空 pending queue、`promptRunning = false`）。避免浪费一次无效的 prompt round-trip。

### 第二层：检测被误杀的 prompt 并自动重试（兜底）

```rust
if reply.full_text.trim().is_empty()
    && reply.streamed_parts == 0
    && t_prompt_start.elapsed().as_millis() < 500
{
    info!("prompt returned empty too fast, retrying after delay");
    sleep(Duration::from_millis(300)).await;
    continue;  // retry the prompt
}
```

以防 500ms 不够或其他异常情况，检测到空返回 + 极快完成时自动重试。

## 尝试过但失败的方案

### 方案 1：在 cancel 后加 200ms 延迟（太短）
在 `cancel` 和新 prompt 之间加 200ms `sleep`。**失败**：旧 prompt 可能已经自然完成但 `prompt_fut.await` 还在执行内部清理（sleep 100ms + take chunks），总延迟不够。

### 方案 2：检测旧 prompt 是否已完成再决定是否 cancel
用 `poll_fn` 检查 `prompt_fut` 是否已经 Ready。**失败**：`agent responded` 日志是在 `session_notification` callback 中记录的（response 到达），但 `prompt_fut`（`prompt_agent` 函数）内部还有 sleep + buffer 读取等后续操作，此时 poll 仍返回 Pending。即使不发 cancel，agent 进程异步接收到的 cancel 仍然会污染状态。

### 方案 3：只检测+重试不加等待（浪费 prompt 调用）
不加等待，每次 interrupt 后新 prompt 必定被误杀一次，检测到再重试。**可行但浪费**：每次 interrupt 多一次无效 prompt round-trip，且依赖启发式（<500ms 判定）可能误判。改为先等 500ms 避免绝大多数误杀。

## 教训

1. **ACP cancel 是全局的**：它不只取消"当前正在执行的 prompt"，还会取消 pending queue 中排队的所有 prompt
2. **不要依赖时序来保证 cancel 安全**：agent 是独立进程，cancel notification 到达时机不可控
3. **两层防御最可靠**：500ms 等待避免绝大多数误杀 + 检测重试兜底极端情况
4. **`tokio::select!` race condition**：当两个 arm 同时就绪时，select 随机选择一个——不能假设 prompt 完成一定会被 prompt arm 捕获
