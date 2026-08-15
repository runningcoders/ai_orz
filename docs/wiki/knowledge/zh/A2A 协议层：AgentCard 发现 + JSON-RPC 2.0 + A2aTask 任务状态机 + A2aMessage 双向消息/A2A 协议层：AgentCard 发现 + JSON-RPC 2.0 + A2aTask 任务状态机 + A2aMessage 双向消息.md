---
kind: wiki_knowledge_card
name: A2A 协议层：AgentCard 发现 + JSON-RPC 2.0 + A2aTask 任务状态机 + A2aMessage 双向消息
category: common API 协议层
scope:
  - "common/src/api/a2a.rs"
  - "common/src/enums/external_agent*.rs"
source_files:
  - common/src/api/a2a.rs:Ln-Lm（AgentCard + Capabilities + JsonRpcRequest/Response + A2aTask + A2aMessage + SendTask/GetTask/CancelTask/SubscribeParams）
  - src/handlers/a2a/jsonrpc.rs:Ln-Lm（JSON-RPC 2.0 方法路由分发，按 method: params 映射）
  - src/handlers/a2a/agent_card.rs:Ln-Lm（GET /.well-known/agent.json AgentCard 公开端点）
  - docs/design/a2a_server_architecture_design.md
  - （占位：待 ai-orz-doc-maintainer 落地后回填真实 Plan 路径）
  - docs/wiki/zh/content/API 参考/API 参考.md
  - docs/wiki/zh/content/API 参考/A2A 协议/Agent 发现机制.md
  - docs/wiki/zh/content/核心模块/处理器层/A2A协议处理器/Agent卡片发现.md
---

# A2A 协议层（common DTO 单一事实源）

## §1 整体方案
遵循 Google A2A 协议规范（v0.3.0），所有协议类型统一定义在 `common/src/api/a2a.rs`（前后端 + A2A Client/Server 复用），实现「契约一处定义全链路复用」。核心 5 类对象：

(a) **AgentCard 组织能力发现**：公开端点 `GET /.well-known/agent.json`（无鉴权），对外只暴露一个**组织级统一入口**（不列出内部具体 Agent），含 name/description/version/协议端点 url/capabilities（声明支持 tasks/send、tasks/get、tasks/cancel、tasks/subscribe 方法）/skills（组织级技能）/default_input_modes/output_modes。客户端先 GET AgentCard 再发任务，实现服务发现解耦。

(b) **JSON-RPC 2.0 统一信封**：所有 `/a2a` POST 请求走 JsonRpcRequest（jsonrpc="2.0"/id/method/params），响应走 JsonRpcResponse（id/result/error 三选一，严格符合 JSON-RPC 规范）。方法路由在 `handlers/a2a/jsonrpc.rs` 统一分发，支持 5 个 A2A 标准方法：`tasks/send`（提交任务）、`tasks/get`（轮询状态）、`tasks/cancel`（取消任务）、`tasks/subscribe`（SSE 订阅状态变更）、`artifacts/get`（产物下载）。

(c) **A2aTask 任务状态机**：id/session_id/metadata/state（working/completed/failed/canceled/unknown）/messages[]（消息历史，最新在尾）/artifacts[] + 可选 progress。设计原则：**消息历史含所有 role=user/agent 消息**（客户端可直接渲染任务完整对话，无需再次拉取），进度字段可选（前端不显示即忽略）。

(d) **A2aMessage 双向消息对齐内部 MessagePo**：role（user/agent/system 三选一）+ parts[]（统一 A2aMessagePart：Text/Audio/Image/Artifact/Metadata 变体）+ message_id（= ai_orz 内部 message id，便于回查）+ task_id。与内部消息 1:1 映射——入站外部 A2aMessage → 内部 User 消息；出站内部 Agent 消息 → A2aMessage。

(e) **参数对象分离**：SendTaskParams（任务提交：id/message/session_id/metadata/notification_url Push 回调地址可选）/ GetTaskParams（轮询：task_id）/ CancelTaskParams（取消：task_id）/ SubscribeParams（SSE 订阅：task_id/session_id）。每个参数结构体独立，新增字段时不影响其他方法的签名。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构/入口 |
|------|------|-------------|
| [common/src/api/a2a.rs](common/src/api/a2a.rs) | 协议单一事实源（前后端+Client/Server 共用）| AgentCard / AgentCapabilities / AgentSkill；JsonRpcRequest<T> / JsonRpcResponse<T, E>；A2aTask（含 state 枚举）/ A2aMessage / A2aMessagePart；SendTaskParams / GetTaskParams / CancelTaskParams / SubscribeParams；单元测试：JSON 序列化 roundtrip |
| [src/handlers/a2a/jsonrpc.rs](src/handlers/a2a/jsonrpc.rs) | JSON-RPC 2.0 方法路由分发 | extract_method() → match "tasks/send" | "tasks/get" | "tasks/cancel" | "tasks/subscribe" | "artifacts/get" → 对应 Handler 函数；错误统一包装成 JsonRpc error（code/message/data）|
| [src/handlers/a2a/agent_card.rs](src/handlers/a2a/agent_card.rs) | GET /.well-known/agent.json 发现端点 | 组织配置（name/desc/URL）拼装 AgentCard JSON 返回；**无 JWT 鉴权**（公开发现端点）；Capabilities 声明当前 Server 支持的方法集 |
| [src/handlers/a2a/mapper.rs](src/handlers/a2a/mapper.rs) | 内部实体 ⇄ A2A 协议对象 互转 | Task/Message → A2aTask/A2aMessage；A2aMessage → 内部 Message 字段映射；**DTO mapper 独立文件**（Handler 不写字段级映射代码）|
| 【① Design 定稿】a2a_server_architecture_design.md | 协议架构选型 + 状态机设计 | docs/design/a2a_server_architecture_design.md |
| 【③ Wiki 长文 1】API 参考.md §A2A 协议接口 | 路由表 + 任务生命周期时序图 | docs/wiki/zh/content/API 参考/API 参考.md |
| 【③ Wiki 长文 2】Agent 发现机制.md | AgentCard/.well-known 端点说明 | docs/wiki/zh/content/API 参考/A2A 协议/Agent 发现机制.md |
| 【平行卡 1】A2A Server Handler 层（JSON-RPC 分发 + 7 Handler） | 服务端实现 | docs/wiki/knowledge/zh/A2A%20Server%20Handler%20层：JSON-RPC%20方法路由%20+%20公开无鉴权路由%20+%20notification_url%20回调渠道自动创建/A2A%20Server%20Handler%20层：JSON-RPC%20方法路由%20+%20公开无鉴权路由%20+%20notification_url%20回调渠道自动创建.md |
| 【平行卡 2】A2A Client + 外部 Agent Runtime | 客户端实现 | docs/wiki/knowledge/zh/A2A%20Client%20+%20外部%20Agent%20Runtime：A2aRuntimeDao%20HTTP%20调用%20+%20ExternalCortexDao%20桥接%20+%20A2aCallbackDao%20Push%20推送/A2A%20Client%20+%20外部%20Agent%20Runtime：A2aRuntimeDao%20HTTP%20调用%20+%20ExternalCortexDao%20桥接%20+%20A2aCallbackDao%20Push%20推送.md |

## §3 架构约定

1. **common 是协议单一事实源（强约束）**：所有 A2A JSON-RPC 方法的请求参数（SendTaskParams 等）+ 响应结果（A2aTask 等）**必须在 common/src/api/a2a.rs 定义**，禁止 Handler 层本地定义 struct 或用 HashMap 裸提取；禁止前端手动写镜像 struct（直接 re-export common）。新增 A2A 方法时，先在 common 加 Params/Result 结构体再动 Handler。
2. **JSON-RPC 错误语义分层**：协议级错误（method 不存在 / JSON 解析失败 / id 缺失）→ 用标准 JSON-RPC error codes：-32700 Parse / -32600 InvalidRequest / -32601 MethodNotFound / -32602 InvalidParams。业务级错误（task_id 不存在 / 任务已完成无法取消 / 权限错误）→ code=-32000 以上 Server error 区间 + data 字段带 ai_orz 内部错误码与详细文案。**绝不混用 HTTP 状态码表达 JSON-RPC 内部错误**（HTTP 永远 200 OK，具体错误在 JSON-RPC envelope 内）。
3. **A2aTask.state 枚举只能单向转移**：working → completed/failed/canceled（允许直接到终态）；canceled → 任何终态（不允许；取消是终态）；completed/failed → 不允许回转。内部任务状态变更时由 Domain 层确保转移合法，Handler 层不做校验；A2A Server 对外永远只暴露合法终态，中间态仅 working。
4. **notification_url 与 SSE subscribe 双轨并行，不互相绑定**：tasks/send 的 notification_url 为**可选参数**；客户端可以不注册回调只 tasks/get 轮询（30s 间隔兜底）；也可以不注册回调只 tasks/subscribe SSE 流式；也可以同时注册 notification_url + SSE；Server 侧两条推送路径独立实现，互不阻塞。
5. **AgentCard Capabilities 是声明，不是强制**：Capabilities 列表声明支持的方法集合，但 Server 端**必须额外显式校验** method 是否实际实现（防止 Capabilities 声明和真实代码漂移）。未知 method 一律返回 MethodNotFound(-32601)，不降级到其他逻辑。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止 A2A Handler 层或 Server 层接受 common 以外自定义协议类型**：禁止在 Handler 签名中出现 `Json<Value>` 或手写本地 Request struct；所有请求参数必须走 SendTaskParams/GetTaskParams 等 common 结构体反序列化，自定义字段会直接被拒绝。
2. ❌ **禁止 A2aMessage role 除 "user"/"agent"/"system" 三值外的字符串**：协议序列化/反序列化必须严格校验 role 值，未知 role → 直接丢弃并返回 InvalidParams。内部到外部映射时 MessageRole 枚举必须显式 `match` 三态，不允许 `_ => unreachable!()` 以外的 fallback（避免新增枚举值时静默传递非法 role）。
3. ❌ **禁止 A2aTask messages 数组包含明文敏感字段**：推送回调（notification_url + SSE subscribe）对外时，任何附件/凭证/secret 类字段必须被剥离或 hash。A2A 协议消息 parts 中**永不包含外部平台凭证的 app_secret 等密文或明文**。
4. ✅ **JSON-RPC id 字段必须原样回带（强约束）**：客户端请求 id="xxx"，响应 id 必须精确相同（数字/字符串类型保持一致）；批量请求数组 id 逐条对应。客户端用 id 匹配请求响应，丢失 id = 客户端异步任务永远挂起。
5. ✅ **向前兼容强约束：新增字段必须 serde(default) + Option**：A2aTask 新增 progress 字段时，`#[serde(default)] Option<Progress>`；SendTaskParams 新增 metadata 时 `#[serde(default)] Option<Value>`，老客户端不传字段不会反序列化失败。**禁止删字段、禁止重命名已有字段**。
6. ✅ **四类互引闭环**：本卡 source_files[] 含 3 篇 wiki 长文绝对路径 + 1 Design（a2a_server_architecture）+ Plan 占位 + 2 平行卡（Server 层/Client 层）；对应 Wiki 长文 cite 段必须回链本卡 + Design + 平行卡。
