# ACP 中断并切换到最新消息实施计划

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** 在 `xmtp-cli acp` 中支持“当前 turn 正在处理时，用户在同一会话发来新消息，则先中断当前 turn，再继续用同一 session 处理最新消息”。

**Architecture:** 保留 ACP session 作为长期上下文容器；把“中断”定义为取消当前 in-flight turn，而不是销毁 session。将当前串行的“收消息后直接 await prompt”模型，改造成“消息接收循环 + turn 执行 task + cancel 控制流”。

**Tech Stack:** Rust, `agent-client-protocol`, `tokio`, 现有 `xmtp-daemon` conversation SSE/history stream, 现有 `xmtp-cli acp` bridge。

---

## 先确认的协议语义

当前调研得到的事实：

- ACP 协议支持 `cancel` / `CancelNotification { session_id }`
- ACP prompt 本身显式绑定 `session_id`
- ACP 也支持 `session_resume`
- 因此协议语义上，“中断当前执行”与“保留 session 上下文”是兼容的

这意味着本功能的目标语义应当是：

> 中断当前 session 上正在执行的 prompt/turn，**不是**删除会话，不是清空历史。

所以：

- 之前已经在 session 中的上下文通常仍然存在
- 被中断的是当前那一轮正在运行的处理
- 新消息应继续在同一个 session 上发送

---

## 产品语义（必须先定清楚）

### 用户可见行为

当同一 conversation/session 中：

1. 消息 A 正在处理中
2. 用户又发来消息 B

系统行为应为：

1. 检测到当前 turn 仍在运行
2. 对该 session 发起 `cancel`
3. 当前 turn 进入 interrupted/cancelled 结束流程
4. 不再继续消息 A 的后续生成
5. 立即开始处理消息 B
6. 处理仍沿用同一个 session

### 关键约束

- **上下文保留**：session 不销毁
- **最新消息优先**：新消息可以打断旧消息
- **单 session 串行执行**：任意时刻最多只有一个 in-flight turn
- **桥必须可重入地处理新消息**：不能被 `prompt().await` 完全阻塞

---

## 非目标

本期不要一起做：

- 多条 pending 用户消息的复杂队列调度
- 跨 conversation 的共享打断策略
- daemon 侧 transport 改协议（SSE -> websocket）
- 完整 GUI/TUI 取消按钮
- action intent 与 interrupt 的统一交互抽象

v1 只做：

- 同一 conversation 内
- 当前 turn 被新消息打断
- 然后继续处理最新消息

---

## 当前架构为什么还不支持

当前 `crates/xmtp-cli/src/acp.rs` 的主流程本质是串行的：

- 收到消息
- 立即 `prompt_agent(...).await`
- 等这一轮完成
- 再处理下一条消息

关键位置：

- `bridge_history_to_acp()`
- `prompt_agent()`
- `prompt_agent()` 内部直接 `conn.prompt(...).await`

这会导致：

- 当一轮 prompt 正在运行时
- bridge 对同一 session 的新消息无法及时进入“控制面”
- 更别说先 `cancel` 再切换到下一条消息

所以这个功能不是“小补丁”，而是**turn lifecycle 重构**。

---

## 推荐设计

## 一、增加显式的 in-flight turn 状态机

为每个 session 增加一个运行状态，建议至少包含：

```rust
enum TurnExecutionState {
    Idle,
    Running {
        source_message_id: String,
        started_at_ms: i64,
        cancel_requested: bool,
    },
}
```

还需要挂接：

- 当前 turn 的 `tokio::task::JoinHandle`（如果使用独立 task）
- 当前 turn 对应的 XMTP source message id
- 取消请求是否已经发出
- 是否已有 pending replacement message

### 最小建议结构

```rust
struct PendingUserMessage {
    item: HistoryItem,
    received_at_ms: i64,
}

struct SessionController {
    state: TurnExecutionState,
    pending_replacement: Option<PendingUserMessage>,
}
```

规则：

- 同一时刻最多 1 个运行中的 turn
- `pending_replacement` 只保留最后一条新消息
- 新消息到来时覆盖旧的 pending replacement

这实现“最新消息优先”。

---

## 二、把“收消息”和“执行 prompt”解耦

### 当前问题

当前模式：

```rust
收到消息 -> prompt_agent(...).await -> 收到下一条消息
```

这不适合中断。

### 新模式

应该改成：

```rust
SSE/history loop 持续收消息
        |
        v
交给 session controller
        |
        +--> 如果 Idle: 启动新 turn task
        |
        +--> 如果 Running: 触发 cancel，并把新消息保存为 pending_replacement
```

也就是说：

- message intake loop 不再被 prompt 阻塞
- prompt 执行在独立 task 中进行
- cancel 走控制面

---

## 三、引入 ACP cancel 调用

新增一个 helper，例如：

```rust
async fn cancel_session_turn(
    conn: &acp::ClientSideConnection,
    session_id: &acp::SessionId,
) -> anyhow::Result<()> {
    conn.cancel(acp::CancelNotification::new(session_id.clone()))
        .await
        .context("ACP cancel")
}
```

说明：

- 具体函数名/构造器以实际 crate API 为准
- 但协议上已经确认 `cancel` 是存在的

### 触发时机

当同一 session：
- 已经有 Running turn
- 又收到新的用户消息

则：
1. 如果尚未 `cancel_requested`
2. 立刻发送 cancel
3. 标记 `cancel_requested = true`
4. 保存新消息为 `pending_replacement`

---

## 四、定义 turn 被打断后的行为

这部分必须先定，否则实现会反复返工。

### 推荐 v1 语义

#### Single reply mode

如果当前是 `ReplyMode::Single`：

- 若 final reply 还未发出：
  - 直接丢弃当前 turn 结果
- 若已经发出了 final reply：
  - 这轮其实已经完成，无需中断

#### Stream reply mode

如果当前是 `ReplyMode::Stream`：

- 已经发送的 partial reply 无法收回
- 取消后不再继续后续输出
- 可选发送一条简短状态说明，例如：
  - `（上一条响应已中断，转而处理你的最新消息）`

### 推荐初版实现

为了减少复杂度：

- **single 模式**：不补额外说明，直接转新消息
- **stream 模式**：增加一条极短 interrupt marker（可选，但推荐）

这样用户能理解为什么上一条回复停住了。

---

## 五、上下文语义说明

中断后继续同一 session，一般意味着：

- 历史消息仍然保留
- session 上下文仍然保留
- 新消息仍是在“已有会话上下文”基础上处理

但不要对用户承诺：

- 被中断那一轮的中间推理状态会 100% 无损保留

正确说法应该是：

> 保留的是 session/history 上下文，不是未完成推理的全部瞬时内部状态。

这是协议层和 agent 实现层的正常边界。

---

## 六、建议日志与可观测性

必须新增明确日志，否则后续很难 debug。

建议新增结构化事件：

```json
{"event":"turn_interrupt_requested", ...}
{"event":"turn_interrupt_sent", ...}
{"event":"turn_interrupt_completed", ...}
{"event":"turn_interrupt_failed", ...}
{"event":"pending_replacement_set", ...}
{"event":"pending_replacement_started", ...}
```

关键字段应包括：

- `session_id`
- `running_source_message_id`
- `replacement_source_message_id`
- `elapsed_ms`
- `reply_mode`
- `streamed_part_count`
- `cancel_requested`

这样后续能清楚区分：

- 正常结束
- prompt hang
- interrupt 成功
- interrupt 发出但 agent 没响应
- interrupt 后 replacement 没启动

---

## 七、推荐实现顺序

### Phase 1：先把当前 turn 执行抽成独立 task

目标：
- 不改产品语义
- 只是把 prompt 执行从 intake loop 中分离

成功标准：
- `acp` 现有行为保持不变
- 同一 session 仍然串行
- 但新消息已经可以在控制层看到

### Phase 2：加 session controller + pending replacement

目标：
- session 内部维护 Running/Idle
- 先不真的 cancel，只记录 replacement

成功标准：
- 新消息到来时不会丢
- replacement 可以排队（只保留最后一条）

### Phase 3：接入 ACP cancel

目标：
- 运行中 turn 遇到新消息时发送 `cancel`
- turn 结束后自动启动 replacement

成功标准：
- 当前 turn 被中止
- 最新消息开始处理
- 仍沿用同一 session

### Phase 4：补 stream/single 语义和用户提示

目标：
- single 模式结果干净
- stream 模式中断语义清晰

### Phase 5：补测试和回归保护

目标：
- 防止后续又退回串行阻塞模型
- 防止 interrupt 造成 session 状态紊乱

---

## 八、需要修改的主要文件

### 核心文件

- `crates/xmtp-cli/src/acp.rs`

如果已经开始 bridge core 重构，则建议同步抽出：

- `crates/xmtp-cli/src/bridge/runtime.rs`
- `crates/xmtp-cli/src/bridge/core.rs`
- `crates/xmtp-cli/src/transport/acp.rs`

### 推荐拆分边界

#### `transport/acp.rs`
放：
- ACP session setup/resume/load
- ACP cancel helper
- ACP event mapping
- `BridgeClient`

#### `bridge/runtime.rs`
放：
- in-flight turn state
- pending replacement state
- active turn snapshots

#### `bridge/core.rs`
放：
- intake loop
- dispatch logic
- replacement scheduling
- post-interrupt restart logic

---

## 九、测试计划

## 1. 单元测试

新增测试覆盖：

- 同一 session Running 时新消息到来，replacement 被记录
- 多次新消息到来时，只保留最后一条 replacement
- interrupt 成功后 replacement 会启动
- interrupt 失败时系统不会丢掉 replacement

## 2. 协议/适配器测试

如果能 mock ACP connection，测试：

- `cancel` 被对正确的 `session_id` 调用
- cancel 只发一次，不重复乱发

## 3. 桥级测试

验证：

- single 模式下，中断后旧 turn 不会再发 final reply
- stream 模式下，中断后不会继续追加旧 turn 的后续分片
- 新 turn 会发送新的 `👀` / progress / final output

## 4. 手工验证

推荐真实会话手测：

1. 发一条会触发长时间工具执行的消息 A
2. 在其 still-active 时立刻发消息 B
3. 观察：
   - 日志出现 `turn_interrupt_requested`
   - A 不再继续输出
   - B 很快开始处理
   - session 不重建
4. 再发消息 C，确认上下文仍延续

---

## 十、风险点

### 1. cancel 发出后 agent 不立刻停

可能出现：
- cancel 已发
- 但 agent 还在继续吐 session updates

处理建议：
- bridge 在 `cancel_requested = true` 后，对旧 turn 的新增输出做严格丢弃或停止发送
- 不要继续把旧 turn 的结果发给用户

### 2. stream 模式 partial output 已经发出

这是不可回滚的。

处理建议：
- 接受已发 partial
- 只停止后续追加
- 必要时发 interrupt marker

### 3. replacement 与 reconnect/catch-up 交错

如果这时又发生 SSE reconnect，状态会复杂。

处理建议：
- 先保证 session controller 的状态是单一可信源
- reconnect 后不要重复启动已取消的旧 turn

### 4. 当前 `prompt_agent()` 无 timeout

这会放大 interrupt 的实现复杂度。

即使有 cancel，也建议后续继续加：
- prompt timeout
- timeout 后 turn fail-safe 清理

---

## 十一、验收标准

以下全部满足才算完成：

- [ ] 同一 session 运行中收到新消息，会触发 interrupt
- [ ] interrupt 后使用同一 session 继续处理新消息
- [ ] 历史上下文仍保留
- [ ] single 模式不会把旧 turn 的未完成结果错误发出
- [ ] stream 模式不会在 interrupt 后继续追加旧 turn 输出
- [ ] replacement 只保留最后一条新消息
- [ ] 有明确结构化日志可追踪整个 interrupt 流程
- [ ] `cargo build --workspace` 通过
- [ ] 相关测试通过

---

## 十二、建议 commit 序列

1. `refactor: isolate ACP turn execution from message intake`
2. `feat: add session controller for pending replacement messages`
3. `feat: interrupt running ACP turns on newer user messages`
4. `feat: handle interrupted stream replies explicitly`
5. `test: add ACP interrupt and replacement coverage`

---

## 最终建议

这个功能是**可行的，而且产品语义合理**。

一句话总结：

> **当新消息到来时，中断的是当前 turn，不是整个 session；因此通常可以在保留上下文的前提下，直接切换去处理最新消息。**

但前提是我们要把当前 bridge 从“串行 await prompt”重构成“可控制的 in-flight turn runtime”。
