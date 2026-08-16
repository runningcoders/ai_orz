---
kind: wiki_knowledge_card
name: A2A Server Handler 层：JSON-RPC 方法路由 + 公开无鉴权路由 + notification_url 回调渠道自动创建
category: a2a 适配器层（公开 Handler）
scope:
  - "src/handlers/a2a/*.rs"
  - "src/router.rs"
source_files:
  - src/handlers/a2a/jsonrpc.rs:Ln-Lm（JSON-RPC 2.0 方法分发 + 5 标准 method → Handler 函数）
  - src/handlers/a2a/send_task.rs:Ln-Lm（tasks/send：创建任务 + 可选 notification_url A2aCallback 渠道自动创建）
  - src/handlers/a2a/get_task.rs:Ln-Lm（tasks/get：按 task_id 轮询任务状态）
  - src/handlers/a2a/cancel_task.rs:Ln-Lm（tasks/cancel：取消任务，幂等）
  - src/handlers/a2a/send_subscribe.rs:Ln-Lm（tasks/subscribe：SSE 流式订阅任务变更）
  - src/handlers/a2a/callback.rs:Ln-Lm（A2A 外部通知回调：第三方 Server 推送到我方 notification_url 入口）
  - src/router.rs:Ln-Lm（公开路由：/.well-known/agent.json、/a2a POST、/a2a/subscribe SSE、/a2a/callback — 无 JWT 鉴权）
  - docs/archive/design-archive/a2a_server_architecture_design.md
  - （占位：待 ai-orz-doc-maintainer 落地后回填真实 Plan 路径）
  - docs/wiki/zh/content/API 参考/API 参考.md
  - docs/wiki/zh/content/核心模块/处理器层/A2A协议处理器/Agent卡片发现.md
---

# A2A Server Handler 层（公开无鉴权路由）

## §1 整体方案
A2A 协议是**面向外部第三方 Agent 系统**的公开接入面，所有 Handler 路由（/.well-known、/a2a/*）**均不挂 JWT 鉴权中间件**（走独立认证机制：可选 Bearer Token 白名单校验 + notification_url webhook secret HMAC 签名校验）。Handler 严格按 AGENTS 规范每方法一个独立文件，共 7 Handler + 1 分发器：

(a) **JSON-RPC 分发模式**：所有 POST `/a2a` 请求统一进 `jsonrpc.rs` 分发器 → 反序列化为 JsonRpcRequest → 按 method 字符串分发到对应 Handler 函数（tasks/send → send_task.rs / tasks/get → get_task.rs / tasks/cancel → cancel_task.rs / tasks/subscribe → send_subscribe.rs SSE / artifacts/get → 预留）。**分发器本身不写业务**，只负责把 JsonRpcRequest.params 解析成对应 Params struct 并调用 handler 返回 JsonRpcResponse。

(b) **任务提交 tasks/send 核心流程**：反序列化 SendTaskParams → 校验 A2aMessage role=user → 内部 Task 创建（project_id 来自 A2a 渠道 scope_project 配置，若 notification_url 存在则**自动创建 A2aCallback 消息渠道**关联 webhook_url + scope_project，后续任务任何消息变更自动 Push 到该 URL）→ 立即返回 state=working 的 A2aTask（不阻塞等待任务执行）。回调渠道创建成功与否：成功则写入渠道表；失败则只 log_warn! 并正常返回任务（客户端退化为 tasks/get 轮询 + tasks/subscribe SSE，主流程不阻断）。

(c) **轮询 tasks/get + 取消 tasks/cancel（幂等）**：get_task 按 task_id 查内部任务状态 → 组装 A2aTask（包含完整消息历史 + state）→ 返回。取消任务：调用 Domain task.cancel() → 无论任务原本是否处于可取消状态（已完成/已取消），统一返回 state=canceled 或已存在的终态 — **取消请求永不报错（幂等语义）**；如果任务实际上已完成，返回的 A2aTask.state=completed，客户端根据 state 自行判断是否取消生效。

(d) **SSE 订阅 tasks/subscribe**：send_subscribe.rs 返回 SSE 响应，Content-Type=text/event-stream；任务状态/消息变更时通过 AOP 事件订阅实时推送到 SSE 流；data 事件负载 = A2aTask JSON；客户端断线自动按 Last-Event-ID 重连（从断点继续，不丢事件）。

(e) **A2A 回调入口 callback.rs（入站 Push）**：当我方作为 A2A Client 调用外部 A2A Server 时，若对方支持 Push 回调，notification_url = 我方 `/a2a/callback` 端点。校验 HMAC-SHA256 webhook secret（X-A2A-Signature header）→ 反序列化 A2aTask → 找到对应内部任务 → 更新状态/追加消息 → 触发内部事件驱动下游（任务完成自动推进执行计划）。HMAC 校验失败直接 401（避免伪造回调污染任务状态）。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键改写内容 |
|------|------|------------|
| [handlers/a2a/jsonrpc.rs](src/handlers/a2a/jsonrpc.rs) | 统一 JSON-RPC 分发 | parse body → JsonRpcRequest → match method → handler(params) → JsonRpcResponse；错误映射到标准 code；单元测试：mapper_test.rs |
| [handlers/a2a/send_task.rs](src/handlers/a2a/send_task.rs) | tasks/send 任务提交 | SendTaskParams → 创建内部 Task +（notification_url 存在时）创建 A2aCallback 渠道 → 返回 state=working A2aTask |
| [handlers/a2a/get_task.rs](src/handlers/a2a/get_task.rs) | tasks/get 轮询 | task_id → 查内部 Task + Message 列表 → 组装 A2aTask（state/messages/artifacts）|
| [handlers/a2a/cancel_task.rs](src/handlers/a2a/cancel_task.rs) | tasks/cancel 取消（幂等）| cancel_task → 不校验原状态，直接尝试取消 → 返回最新 A2aTask state |
| [handlers/a2a/send_subscribe.rs](src/handlers/a2a/send_subscribe.rs) | tasks/subscribe SSE | SSE stream handler；订阅 AOP 任务消息变更事件；每变更推一条 data: A2aTask JSON；retry: 3000 |
| [handlers/a2a/callback.rs](src/handlers/a2a/callback.rs) | 入站回调（第三方推过来）| HMAC X-A2A-Signature 校验 → 解析 A2aTask → 更新内部 task；401 拒绝伪造回调 |
| [handlers/a2a/agent_card.rs](src/handlers/a2a/agent_card.rs) | /.well-known/agent.json 发现 | 公开静态 JSON；**无任何鉴权、无 DB 读**（直接从组织配置拼装）|
| [router.rs](src/router.rs) | 公开路由分组 | `/a2a` POST、`/a2a/subscribe` GET SSE、`/.well-known/agent.json` GET、`/a2a/callback` POST — 统一挂载在公开路由组（不经过 JWT middleware）|
| 【① Design】a2a_server_architecture_design.md §三 Handler 架构 | 无鉴权路由设计 + 幂等语义 | docs/archive/design-archive/a2a_server_architecture_design.md |
| 【③ Wiki 长文】API 参考.md §A2A 协议接口时序图 | tasks/send→working→notification Push / 30s tasks/get 兜底双重时序 | docs/wiki/zh/content/API 参考/API 参考.md |
| 【平行卡 1】协议层 | AgentCard/JsonRpc/A2aTask 定义 | docs/wiki/knowledge/zh/A2A%20协议层：AgentCard%20发现%20+%20JSON-RPC%202.0%20+%20A2aTask%20任务状态机%20+%20A2aMessage%20双向消息/A2A%20协议层：AgentCard%20发现%20+%20JSON-RPC%202.0%20+%20A2aTask%20任务状态机%20+%20A2aMessage%20双向消息.md |
| 【平行卡 2】A2A Client + 外部 Agent Runtime | A2aRuntimeDao 出站调用 | docs/wiki/knowledge/zh/A2A%20Client%20+%20外部%20Agent%20Runtime：A2aRuntimeDao%20HTTP%20调用%20+%20ExternalCortexDao%20桥接%20+%20A2aCallbackDao%20Push%20推送/A2A%20Client%20+%20外部%20Agent%20Runtime：A2aRuntimeDao%20HTTP%20调用%20+%20ExternalCortexDao%20桥接%20+%20A2aCallbackDao%20Push%20推送.md |

## §3 架构约定

1. **A2A Handler 不挂 JWT，单独 Bearer Token + HMAC 双轨认证**：
   - 出站 JSON-RPC 请求（tasks/send 等）→ 可选 Authorization: Bearer <token>（由 A2aRuntimeConfig.auth_token 控制）；Server 侧验证 token 是否在组织级 A2A 接入白名单内。
   - 入站 Push 回调（/a2a/callback）→ 必须校验 X-A2A-Signature: sha256=<HMAC-SHA256(webhook_secret, raw_body)>；secret 按「渠道 ID + 对方服务端」维度独立存储（不全局共享）。**任何缺失 signature 或验证失败直接 401，不进入后续业务流程。**
2. **幂等强约束（A2A 跨网络重试必备）**：tasks/send 使用客户端提供的 params.id（若存在）作为幂等 key — 如果同一个幂等 key 5 分钟内重复提交，返回**第一次创建的 A2aTask**（不重复创建任务）；tasks/cancel 同样幂等（重复取消不报错、返回最新状态）。30s 轮询 tasks/get 天然幂等。
3. **SSE 断线不丢事件：Last-Event-ID + 事件序**：SSE Handler 收到客户端 Last-Event-ID → 从上次已推的事件序号之后开始推送后续 A2aTask 快照；客户端不发 Last-Event-ID（首次连接）→ 从当前任务最新状态开始推。
4. **A2aCallback 渠道创建失败不阻断任务主流程**：notification_url 存在时自动创建渠道是「软联动」— 创建成功则后续走 Push；失败则仅 log_warn!（告警但不返回客户端 Err），任务照常 state=working 返回。原因：网络抖动导致渠道创建失败不应让客户端任务无法提交（客户端有轮询/SSE 兜底）。
5. **参数结构体化（AGENTS §4.11）**：所有 path/query/body 参数均使用 common/src/api/a2a.rs 中对应 Request 结构体（如 CancelTaskParams { task_id }），禁止 Handler 签名裸 `Path<String>` 或 `Query<HashMap>`；**每 Handler 独立一个参数结构体**（禁止 Send/Get 复用同一个 Params 造成字段可选漂移）。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止把 A2A 公开 Handler 挂载到 /api/v1 受保护路由组**（/api/v1 所有路由走 JWT 鉴权 middleware，A2A 必须在 Router 顶层独立公开路由组）。一旦错挂 → 所有外部 A2A 请求 401 无法接入。
2. ❌ **禁止在 Handler 中直接拼 A2aTask 字段**：Handler 只负责调用 mapper.rs（独立 mapper 文件）完成 Task/Message → A2aTask/A2aMessage 的转换；禁止 Handler 文件内出现逐个字段赋值（字段新增时会漏改）。
3. ❌ **禁止 HMAC 验证通过前把回调 Body 写入任何日志/存储**：raw_body 读取后先校验签名，签名不过 → 立即 401 + drop body（防止伪造回调的恶意 payload 被落库或写入日志）。
4. ✅ **响应禁止裸 JSON Value**：所有 JSON-RPC 响应必须通过 JsonRpcResponse<T> 序列化（result = 具体类型如 A2aTask），禁止 `Json(serde_json::json!({...}))` 裸 Value 返回（违反 AGENTS §4.11 裸响应禁止）。
5. ✅ **SSE 心跳保活（中间盒兼容）**：send_subscribe 每 15s 推送一条 `: keep-alive` 注释行（无业务意义、客户端忽略），防止中间层 LB/CDN 因长连接无数据自动断流。
6. ✅ **四类互引闭环**：本卡 source_files[] 含 2 篇 wiki 长文（API 参考 + Agent 卡片发现）+ 1 Design 定稿 + Plan 占位 + 2 平行卡（协议层/Client Runtime 层）；对应 Wiki 长文 cite 段回链本卡 + Design + 平行卡。
