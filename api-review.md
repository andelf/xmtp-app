# API Review

日期: 2026-03-26

本次 review 覆盖当前 `daemon` 暴露的全部 REST + SSE API，并对照 `CLI` / `TUI` 的实际调用情况做检查。

## 1. 必要性

### 1.1 已实现端点总览

| Method | Path | CLI 使用 | TUI 使用 | 结论 |
| --- | --- | --- | --- | --- |
| `POST` | `/v1/login` | 是 | 否 | 必要 |
| `POST` | `/v1/shutdown` | 是 | 否 | 必要 |
| `GET` | `/v1/status` | 是 | 否 | 必要 |
| `GET` | `/v1/conversations` | 是 | 否 | 必要 |
| `GET` | `/v1/conversations/:id` | 是 | 是 | 必要 |
| `GET` | `/v1/conversations/:id/history` | 否 | 是 | 必要 |
| `POST` | `/v1/direct-message/open` | 否 | 是 | 必要 |
| `POST` | `/v1/direct-message/send` | 是 | 是 | 必要 |
| `POST` | `/v1/groups` | 是 | 是 | 必要 |
| `GET` | `/v1/groups/:id` | 是 | 是 | 必要 |
| `GET` | `/v1/groups/:id/members` | 是 | 是 | 必要 |
| `POST` | `/v1/groups/:id/members` | 是 | 是 | 必要 |
| `DELETE` | `/v1/groups/:id/members` | 是 | 是 | 必要 |
| `PATCH` | `/v1/groups/:id` | 是 | 是 | 必要 |
| `POST` | `/v1/groups/:id/send` | 是 | 是 | 必要 |
| `POST` | `/v1/conversations/:id/leave` | 是 | 否 | 保留，但当前后端明确不支持执行 |
| `GET` | `/v1/messages/:id` | 是 | 否 | 必要 |
| `POST` | `/v1/messages/:id/reply` | 是 | 是 | 必要 |
| `POST` | `/v1/messages/:id/react` | 是 | 是 | 必要 |
| `POST` | `/v1/messages/:id/unreact` | 是 | 否 | 必要 |
| `GET` | `/v1/events` | 是 | 是 | 必要 |
| `GET` | `/v1/events/history/:id` | 是 | 是 | 必要 |

### 1.2 本次发现并已修复

- 之前同时存在两套群组管理路径：
  - canonical: `/v1/groups/:id`, `/v1/groups/:id/members`
  - legacy: `/v1/groups/:id/rename`, `/v1/groups/:id/members/add`, `/v1/groups/:id/members/remove`
- 这是实质性问题，不只是风格问题，因为会让客户端继续依赖重复路径。
- 本次已修复：
  - `CLI` 已迁移到 canonical 路径
  - `daemon` 已移除 legacy 路由

### 1.3 仍然缺失或后续可考虑的业务场景

- `group leave` 当前只有端点，没有真正可用的底层执行能力
- 没有分页版 `history`
- 没有分页版 `group members`
- 没有 message edit / delete / pin 等能力
- 没有更细粒度的 group metadata 更新事件

## 2. 命名和路径结构

### 2.1 当前整体评价

整体已经基本统一为“资源路径 + HTTP 方法表达动作”，比之前的 RPC 风格明显更清楚。

### 2.2 当前较一致的部分

- conversation 资源:
  - `/v1/conversations`
  - `/v1/conversations/:id`
  - `/v1/conversations/:id/history`
- group 资源:
  - `/v1/groups`
  - `/v1/groups/:id`
  - `/v1/groups/:id/members`
  - `/v1/groups/:id/send`
- message 资源:
  - `/v1/messages/:id`
  - `/v1/messages/:id/reply`
  - `/v1/messages/:id/react`

### 2.3 仍有语义不完全统一的地方

- `/v1/conversations/:id/leave`
  - 从资源归属角度看，leave 更像 membership 行为
  - 但它也确实可以理解为“离开一个 conversation”
  - 当前可以接受，但后续如果 group-only 语义更明确，可以考虑收敛到 `/v1/groups/:id/leave`

- `/v1/direct-message/open` 与 `/v1/direct-message/send`
  - 这两个路径是动作导向，不是纯资源导向
  - 但考虑到 DM 并不是一个稳定的独立资源 ID，而是“按 recipient 打开或发送”，当前设计仍然合理

## 3. 请求 / 响应结构

### 3.1 request body 是否最小化

整体上是最小化的：

- `login`: `env`, `api_url`
- `group create`: `name`, `members`
- `rename`: `name`
- `members update`: `members`
- `send/reply`: `message`
- `react/unreact`: `emoji`

没有明显冗余字段。

### 3.2 响应结构

成功响应整体可接受：

- 状态型接口返回明确结构
- 动作型接口统一返回 `conversation_id` / `message_id`

### 3.3 当前主要问题

- 错误响应格式还没有统一 schema
  - 当前主要是 HTTP status + 纯文本错误字符串
  - 对 CLI 可用
  - 对 TUI 和未来自动化集成不够理想

建议后续收敛成统一结构，例如：

```json
{
  "error": {
    "code": "group_leave_unsupported",
    "message": "Leave group is not supported in this version"
  }
}
```

### 3.4 分页需求

当前两个列表类接口值得考虑分页：

- `/v1/conversations/:id/history`
- `/v1/groups/:id/members`

当前项目体量下还没成为瓶颈，但如果会话和成员数量继续增长，不分页会让：

- daemon 响应时间变长
- TUI 首次加载变慢
- SSE 订阅恢复后的全量补快照代价变高

## 4. SSE 事件模型

### 4.1 当前职责划分

当前划分是清楚的：

- `/v1/events`
  - 全局应用级事件
  - 主要是 `status`、`conversation_list`、`daemon_error`

- `/v1/events/history/:id`
  - 单会话消息事件流
  - 只负责该会话的 `HistoryItem`

这条边界目前是合理的。

### 4.2 当前不足

全局事件类型还偏少。现在如果不额外查详情，客户端无法知道：

- 某个 group metadata 是否更新
- 某个 group 成员是否变更
- 某个 conversation 是否被 rename

### 4.3 后续建议补充的事件

- `conversation_updated`
  - 名称、描述、成员数变化

- `group_members_updated`
  - 新增成员 / 移除成员

- `message_delivery_updated`
  - 如果后面要加强消息生命周期展示

- `sync_state_updated`
  - 如果后面要把状态栏做得更强

当前不建议一口气补太多事件类型，但 `conversation_updated` 和 `group_members_updated` 是最值得优先补的。

## 5. 结论

### 5.1 当前 API 面的整体结论

- 现在这套 API 已经能支撑当前 CLI + TUI 的实际需求
- REST / SSE 的职责边界已经比之前清楚很多
- 当前最主要的实质性问题是重复路径和错误响应结构

### 5.2 本次已修复的问题

- 删除了重复的 legacy group 管理路由
- `CLI` 已切换到 canonical group 管理路径

### 5.3 后续建议优先级

1. 统一错误响应 schema
2. 为 `history` / `members` 预留分页参数
3. 补充 `conversation_updated` / `group_members_updated` 全局事件
4. 等 XMTP SDK 稳定后，再重新评估 `leave group` 的真正可用性
