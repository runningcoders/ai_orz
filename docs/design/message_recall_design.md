# 消息撤回与取消能力设计

> 🎯 **本文档定位**：消息域「撤回排队消息 + 取消在飞处理」的 ① Design 设计大纲与关键决策（为什么这样做）；字段级实现细节以代码为准（发送入口 priority 透传已顺延下一期，方案存档于 §3.3）
> 状态：Phase 1 已落地（2026-09-30；priority 顺延下一期）
> 查阅场景：理解「为什么需要撤回/取消」「权限与粒度边界怎么划」「队列层怎么落地」时打开；字段级细节跳代码
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 分层架构与「单向依赖」红线
> - [aop_producer_consumer_contract_design.md](./aop_producer_consumer_contract_design.md) — 「Discard 复用 queue.ack、零新队列机制」先例，本设计在事件中心的落点
> - [runtime_design.md](./runtime_design.md) — `cancel_thinking` / `AgentThinkRuntime` / Agent 三态状态机
> - 落地计划：[docs/plan/消息撤回与取消能力.md](../plan/消息撤回与取消能力.md)

---

## 一、设计目标 / 设计哲学

### 1.1 要解决的问题

Agent 之间靠消息驱动，而**发往同一接收 Agent 的消息在 AOP 事件中心严格 FIFO 串行**：

- `MessageCreatedEvent::order_key()` 对 Agent 接收者取 `to_id`（agent_id），同一 Agent 同一时刻只有一条消息能上堆 —— 见 [message.rs::order_key](src/models/events/message.rs#L28-L48)
- 门闩**只由 `ack` 释放**；`nack`（重试）继续占着门闩 —— 见 [in_memory.rs::ack / nack](src/pkg/aop/queue/in_memory.rs#L303-L346)

由此产生三类「卡住」形态：

| 形态 | 触发条件 | 现有手段 |
|------|---------|---------|
| ① 慢消息占位 | 当前消息跑长 think loop / 大工具调用 | `cancel_thinking` 可中断（轮次边界） |
| ② 重试毒药 | 当前消息反复 `nack` | 重投**仍占门闩**，最长 8 次期间该 Agent 队列完全堵死 |
| ③ 过期积压 | 当前消息跑完 | 后续过期消息**逐条 FIFO 处理**；新纠正消息排在**最后** |

形态 ③ 是「无法纠正错误 / 过时消息」的根因：**没有任何机制能把已入队但未处理的消息撤下来，也没有机制让纠正消息越过旧消息**。

### 1.2 设计哲学

1. **两个正交原语，不合成一个 API**
   - **撤回 = 状态语义**：把 `messages.status` 置 `Recalled`（逻辑作废），落在持久层。
   - **取消 = 信号语义**：对正在处理该消息的思考循环发 `cancel_flag`（尽力中断），落在内存运行时。
   二者可组合（撤回在飞消息 = 先取消、后标撤回），但实现与边界不同，混成一个 API 会让"未处理 vs 在飞"的分支语义糊在签名里。
2. **最大化复用，零新机制**：`MessageStatus::Recalled` 枚举、`cancel_thinking`、`MessageDal::update_status` 均已存在；Phase 1 队列层不加新原语（沿用「Discard 走 `queue.ack`」先例）。
3. **撤回是"阻止后续"，不是"时间倒流"**：已被 Agent 读进上下文的消息无法撤销其**已产生的影响**；撤回 + 取消只保证"不再继续处理"。此边界必须写进工具描述，避免模型误以为撤回能改写已发生的事实。

### 1.3 关键设计决策表

| # | 问题 | 方案 | 原因（为什么不选 B） |
|---|------|------|-------------------|
| D1 | 撤回用什么状态承载？ | 复用既有 `MessageStatus::Recalled` | 枚举已存在，`awakening.rs` 已按它过滤上下文窗口、向量 `status_in` 已排除；新造状态会造成双语义与双过滤口径 |
| D2 | 队列层 Phase 1 怎么落？ | **消费者入口守卫**：出队后 `status == Recalled → ack 跳过` | 对齐「Discard 复用 ack / 零新队列机制」先例；不触碰 `has_active_message` 门闩不变量，风险最低 |
| D3 | 在飞消息怎么撤？ | 复用 `cancel_thinking(agent_id)` + 按 message_id 定位 busy agent | `AgentRuntimeInfo.current_message_id` 已可查；先校验"正在处理的就是这条"可避免误取消更新的消息 |
| D4 | 权限边界 | 发送方 + **收件方（仅未处理）** + 归属用户 + Owner Agent + SuperAdmin | 用户已拍板「放宽到 Owner / 用户」：撤回是纠错动作，需治理面可介入；纯发送方自治覆盖不了"用户发现 Agent 间跑偏"。**另补收件方判据**：队列按收件方串行、只认先来后到，收件方对「这条还该不该做」掌握最直接信息，不给它拒收权则「卡在过期消息上」只能靠发送方事后发现 |
| D5 | 可撤回的状态范围 | 仅 `Pending`（未处理）与 `Processing`（在飞） | `Processed / Failed` 撤回无意义（已消费）；`Recalled` 幂等返回，不报 error |
| D6 | 是否做 priority 超车（P3） | **本期不做**（撤回 + 取消先行，priority 顺延下一期） | 队列侧通路本就完备（`Event::priority()` + 堆序），加字段只需一路透传；但 priority **无法让在飞消息让位**（门闩只由 `ack` 释放），那是 `recall_message` 的职责 —— 两者互补。先落 recall / 取消这对主场景（它们解决「卡在旧消息上」的核心痛点），priority 的取值语义（谁该提权）留待有真实诉求时再定，避免过早引入默认提权改变既有 FIFO 语义 |
| D7 | 是否做 reply 链级联撤回 | **本期不做** | 用户选择"放宽到 Owner / 用户"而非级联；级联边界（跨 root 停在哪）需单独拍板，留待后续 |
| D8 | 撤回消息仍会唤醒 Agent？ | 由 D2 消费者守卫拦截 | 当前 `handle_message` **不检查 status**，不拦则撤回的 Pending 消息出队后照样唤醒 Agent |
| D9 | 撤回能力怎么暴露给 Agent？ | handler 双宏标注（`register_handler_tool` + `generate_http_handler`），工具侧打 `neural` tag，并登记进消息工具可达性护栏 | 同链路的 `send_message` / `send_task_assignment_message` / `search_messages` 均为 `neural`；只挂 `messaging` / `collaboration` 会**不在任何 Agent 的工具面**（死工具），模型只能退回 `send_message`，症状是静默丢消息而非报错 |
| D10 | 工具调用主体怎么解析？ | 统一用 `ctx.message_sender_id()` / `message_sender_role()`；权限 gate 同时认「用户」与「Agent」 | Agent 侧 ctx（唤醒态 / AOP 沉淀态）的 `caller_type` 可能是 `System`、**只有 `agent_id`、天生无 `user_id`**；只认 `user_id` 的 gate 会让工具对 Agent 恒被拒 |

## 二、架构思路

**Phase 1：撤回一条 Pending 消息**

```
撤回请求（Agent tool / REST / 前端）
   │
   ▼
MessageDomain::recall_message(ctx, message_id)
   │  ├─ 权限 gate（发送方 / 收件方（仅未处理） / 归属用户 / Owner Agent / SuperAdmin）
   │  ├─ 状态 gate（Pending→标 Recalled；Processing→转取消链路；终态→幂等/Conflict）
   │  └─ MessageDal::update_status(Recalled)
   ▼
（Phase 1 不动物理队列）该消息的事件仍在 AOP 队列中
   │
   ▼ 事件出队
MessageConsumer::handle_message
   │  └─【新增守卫】`find_by_id` 返回 None（撤回态被软删除过滤挡掉）
   │       └─ `find_by_id_with_recalled` 确认「确实已撤回」→ return Ok（等价 ack，零 LLM）
   │            └─ 确认不了（行真不存在）→ 才上抛 not_found
   ▼
门闩推进 → 后继消息出队
```

**在飞消息（Processing）取消链路**

**在飞消息（Processing）取消链路**

```
recall_message(正在处理的消息)
   │  └─ AgentRuntimeStateManager::find_busy_agent_by_message(message_id)
   │       （**不看 messages.status**：该状态无写入路径，在飞消息在库里仍是 Pending）
   │       └─ cancel_thinking(agent_id)              ← 复用既有能力
   ▼
think_loop 下一轮开头检查 is_cancelled → 退出（轮次边界）
   ▼
BusyGuard Drop → set_idle → 事件 ack → 门闩推进
```

关键结构 / 入口：

- 状态写入：`MessageDal::update_status` —— 见 [dal/message.rs::update_status](src/service/dal/message.rs#L378-L387)
- 含撤回态的读路径：`MessageDal::find_by_id_with_recalled` —— 见 [dal/message.rs](src/service/dal/message.rs)（撤回幂等校验 + 消费入口守卫共用）
- 撤销回退守卫：`MessageDalImpl::is_recalled` —— 见 [dal/message.rs](src/service/dal/message.rs)（`on_consumed` / `on_failed` 两条收尾路径都过它）
- 取消信号：`AgentRuntimeStateManager::cancel_thinking` —— 见 [agent_runtime_state.rs::cancel_thinking](src/pkg/agent_runtime_state.rs#L272-L279)
- 在飞定位（反向索引）：`AgentRuntimeStateManager::find_busy_agent_by_message` —— 见 [agent_runtime_state.rs](src/pkg/agent_runtime_state.rs)
- 取消感知：`think_loop` 每轮开头 `is_cancelled` —— 见 [think_loop.rs](src/service/domain/runtime/think_loop.rs#L360-L375)
- 消费者守卫插入点：`handle_message` 入口 —— 见 [consumer/message.rs::handle_message](src/consumer/message.rs#L180-L213)
- 启动恢复口径（只扫 Pending）：见 [dal/message.rs](src/service/dal/message.rs#L808-L832)

## 三、涉及文件与工具暴露面

| 分层 | 文件 | 角色 | 内容摘要 |
|------|------|------|---------|
| Domain | `src/service/domain/message/recall.rs`（新建） | 撤回编排 | `recall_message`：权限 gate + 状态 gate + 在飞转取消；`RecallOutcome` 承载幂等语义 |
| Domain trait | [message/mod.rs](src/service/domain/message/mod.rs) | 能力声明 | `MessageDomain` 增 `recall_message`（Handler / Tool 复用同一实现）；`MessageDomainImpl` 新注入 `project_dal`（权限判据 ②③ 需读项目） |
| DAL | [dal/message.rs](src/service/dal/message.rs) | 读 + 状态写入 | 新增 `find_by_id_with_recalled`（含撤回态的读路径）；`is_recalled` 守卫；`on_consumed` 与 `on_failed` **双守卫** |
| 运行时状态 | [pkg/agent_runtime_state.rs](src/pkg/agent_runtime_state.rs) | 在飞定位 | 新增 `find_busy_agent_by_message`（`current_message_id` 反向索引，撤回在飞消息的定位依据） |
| Adapter | `src/handlers/finance/message/recall_message.rs`（新建） | REST + 工具 | 双宏标注（`register_handler_tool` + `generate_http_handler`）；工具侧自动注册、路由侧须手动注册，详见 §3.1 |
| Adapter 导出 | [handlers/finance/message/mod.rs](src/handlers/finance/message/mod.rs#L3-L15) | 模块导出 | 增 `pub mod recall_message;` + `pub use recall_message::recall_message_handler;` |
| 工具护栏 | [pkg/tool_registry/builtin.rs](src/pkg/tool_registry/builtin.rs#L117-L137) | 护栏测试 | `message_tools_are_neural_reachable` 清单增 `recall_message` + `cancel_thinking` |
| 路由 | [router.rs](src/router.rs#L908-L923) | 路由注册 | 消息路由组手加 `POST /messages/recall` |
| Consumer | [consumer/message.rs](src/consumer/message.rs#L180-L213) | 队列跳过 | `handle_message` 入口加守卫（`None` 分支区分「已撤回」与「不存在」，覆盖三种 `to_role`） |
| 取消入口可达性 | [handlers/hr/agent/cancel_thinking.rs](src/handlers/hr/agent/cancel_thinking.rs) | 工具可达性 | 补 `neural`（原只挂 `collaboration` ⇒ 死工具）；描述与 `recall_message` 互补分工 |
| Common 枚举 | [common/src/enums/message.rs](common/src/enums/message.rs#L142-L154) | 枚举 | `Recalled` 复用（零改动） |
| Common DTO | `common/src/api/message.rs` | DTO | 新增 `RecallMessageRequest` / `RecallMessageResponse`（幂等语义放在 `outcome` 字段，不靠错误码） |
| 测试 | `src/service/domain/message/recall_test.rs`（新建）+ `dal/message_test.rs` + `pkg/agent_runtime_state.rs` | 回归 | 权限双主体 / 在飞取消 / 幂等 no-op / 双收尾守卫 / 读路径分离 / 反向索引 |
| Frontend | `frontend/src/pages/message/`（消息页） | 入口 | 「撤回」按钮 + 已撤回状态徽章（**本期未做**，用户拍板后置） |
| **零改动面** | AOP 队列（`pkg/aop/queue/*`）、`think_loop`、`EventTopic`、消息向量库、`.sqlx`（未新增任何 prepared query） | — | Phase 1 不动队列机制、不动 think_loop；新读路径复用既有 `MessageQuery`（动态 `QueryBuilder`） |

### 3.1 工具暴露面（Agent 可调用）

撤回能力对 Agent 暴露为内置工具 `recall_message`，与 HTTP 接口**共用同一个 handler 函数**（双宏，禁两套实现）：

```rust
#[register_handler_tool(
    id = "recall_message",
    name = "Recall Message",
    description = "...",
    params = "common::api::RecallMessageRequest",
    neural,
    tags = "messaging"
)]
#[generate_http_handler]
pub async fn recall_message(
    ctx: RequestContext,
    params: RecallMessageRequest,
) -> Result<RecallMessageResponse> { ... }
```

| 关注点 | 结论 | 依据 |
|-------|------|------|
| 工具侧注册 | **自动**：宏生成 `BuiltinToolFactory` + `ctor` 启动即注册，不需要在别处登记 | [ai-orz-macros/src/lib.rs](ai-orz-macros/src/lib.rs#L157-L228) |
| HTTP 侧注册 | **手动**：`generate_http_handler` 只生成 `recall_message_handler` 函数、**不注册路由** ⇒ 必须在 [router.rs](src/router.rs#L908-L923) 消息路由组手加一条，否则接口静默 404 | `send_message_to_agent` / `cancel_thinking` 先例 |
| 路由方法 | `post(...)`（JSON body 形态）：params DTO 未标 `#[param(source = "query")]` 时宏按 body 生成，错配 `get(...)` 会在解析阶段失败 | [router.rs](src/router.rs#L1013) 同款说明 |
| 可达性 tag | `neural`（+ `tags = "messaging"`）⇒ 进入**所有 Agent** 的工具面（`绑定工具 ∪ neural ∪ installed_tags`） | [builtin.rs 护栏](src/pkg/tool_registry/builtin.rs#L103-L137) |
| 参数 schema | 宏内已走 `common::llm_schema::schema_for_llm`（inline `$ref` + 折叠可空枚举）⇒ 天然满足「LLM-LCD 无 `$ref`/`anyOf`」护栏；DTO 内不要再嵌判别联合 | [lib.rs](ai-orz-macros/src/lib.rs#L188-L197) |
| 只挂工具、不挂路由的先例 | `send_task_assignment_message` 只注册工具；撤回前端有「撤回」按钮 ⇒ **两者都挂** | [router.rs](src/router.rs#L908-L911) |

**工具描述必须写清的 3 条边界**（否则模型会误用 / 误承诺）：

1. 只对 **未处理（`Pending`）/ 正在处理（`Processing`）** 的消息有效；已处理 / 失败的消息撤回是 **no-op**（返回 `not_recallable`，不报错）。
2. 撤回是**阻止后续消费**，不是时间倒流：**无法**消除该消息对已读上下文的影响，且**不可撤销**。
3. `Processing` 是**尽力取消**，在**轮次边界**生效，**不保证立即停止**当前 LLM 调用。

### 3.2 调用主体与权限解析

撤回请求有三类来源，身份解析入口统一为 `RequestContext`：

| 来源 | ctx 形态 | 身份取值 |
|------|---------|---------|
| 用户直调（前端 / HTTP） | `caller_type = User` | `ctx.uid()` |
| Agent 直调（思考中调工具） | `caller_type = Agent`，`agent_id` 有值 | `ctx.message_sender_id()` |
| Agent 唤醒态 / 系统链路（AOP 沉淀） | `caller_type = System`，**只有 `agent_id`、无 `user_id`** | `ctx.message_sender_id()`（优先 `agent_id`） |

**核心红线**：权限 gate 必须认「**用户 或 Agent**」双主体 —— 链路级 ctx 经 AOP carrier 还原后天生没有 `user_id`，只认用户的 gate 会让整族工具在沉淀期恒被拒。

权限判据（满足其一即可，Domain 层单点实现）：

| 判据 | 取值 |
|------|------|
| ① 发送方 | `ctx.message_sender_id() == message.from_id` |
| ② 归属用户 | 消息所属项目 `project.root_user_id == ctx.uid()`（无 `project_id` 时该判据不成立） |
| ③ Owner Agent | 消息所属项目 `project.owner_agent_id == ctx.agent_id()` |
| ④ SuperAdmin | `ctx.user_role() == Some(UserRole::SuperAdmin)` |
| ⑤ 收件方 | `ctx.agent_id() == message.to_id` —— 有权**拒收发给自己的消息**（下游状态 gate 把终态归一为 `NotRecallable`，故实际只对未处理消息生效） |

**判据 ⑤ 的动机（收件侧解法）**：消息按接收方串行排队、队列只认先来后到。需求变更后若收件方自己队列里堆着一批已作废的指令，**它原本没有任何手段把它们丢掉**——只能等发送方 / Owner 发现。收件方是被唤醒的那一方，把拒收权交给它，才闭合「Agent 卡在旧消息流程上」这一原始痛点。

**判据 ②③④ 在工具侧会被链路继承的身份放宽**（⚠️ 系统级既有形态，不在本能力内单独收紧）：`ContextCarrier` 携带并沿链路还原 `user_id` / `user_role`，而 [consumer/message.rs::rebuild_context](src/consumer/message.rs) 只覆写 `agent_id`（= `to_id`）、不覆写这两个字段 ⇒ 工具侧它们的语义是「这条链路最初由谁发起」。链路上游为项目归属用户 → 下游 Agent 继承该项目全量撤回权；上游为 SuperAdmin → 下游 Agent 同样带 SuperAdmin 角色（命中判据 ④）。该写法在 [hr/skill.rs](src/service/domain/hr/skill.rs) 的「Admin Bypass」有先例。

**对称工具 `cancel_thinking` 的权限取向**（另见 [runtime/cancel.rs](src/service/domain/runtime/cancel.rs)）：**只做存在性校验、不设关系门**，与既有跨 Agent 工具（`send_message_to_agent` / `send_task_assignment_message`）一致——`handlers/hr/agent/*` 全族无 `forbidden` 判据，Agent 工具面本就不设授权层；给取消单独加门会造出「能发消息、能派任务，却停不了对方」的能力错位。取消是**无持久化副作用**的信号，最坏后果只是让对方提前结束本轮。存在性校验的作用是消除语义歧义（`NotFound` vs 正常 no-op `NotThinking`），**不是收紧权限**。⚠️ 租户门做不了：`agents` 表无 `organization_id` 列、全库无 Agent↔组织关联表。

### 3.3 发送入口优先级字段（priority 透传）—— ⏸ 本期未落地

> **状态**：方案保留，**落地顺延到下一期**（2026-09-30 拍板）。本期只交付「撤回 + 取消」这对主场景；
> 本节作为下一期的落点清单与语义边界存档，避免届时重新推导。

**现状核对：全链路无 priority，只有队列侧通路是活的**

| 层 | 位置 | priority |
|---|------|---------|
| 发送 DTO | `common/src/api/neural_tools.rs` 的 `SendMessageParams` / `SendMessageToAgentParams` / `SendTaskAssignmentMessageParams` | ❌ 无 |
| Domain Command | `src/service/domain/message/mod.rs` 的 `SendToUserCommand` / `SendToAgentCommand` / `SendTaskAssignmentCommand` | ❌ 无 |
| PO | [models/message.rs](src/models/message.rs) `MessagePo` | ❌ 无 |
| 表 | [migrations/20260420000000_initial.sql](migrations/20260420000000_initial.sql#L218-L240) `messages` | ❌ 无（`priority` 列只存在于 `tasks` / `projects`） |
| 事件 | [models/events/message.rs](src/models/events/message.rs#L19-L53) `MessageCreatedEvent` | ❌ 未覆写 `Event::priority()` ⇒ 恒 0 |
| 队列 | [aop/core/event.rs](src/pkg/aop/core/event.rs#L11-L13) `Event::priority()` 默认 0；[in_memory.rs::Ord](src/pkg/aop/queue/in_memory.rs#L49-L55) 堆序 `(priority desc, created_at asc)`；封套注入见 [registry.rs](src/pkg/aop/core/registry.rs#L165-L187) | ✅ **通路完备、恒 0** |

⇒ **现在没有**；但只需把值从发送入口一路透传到事件，**队列侧零改动**。

**落点（4 处）**

| # | 落点 | 内容 |
|---|------|------|
| 1 | `messages.priority INTEGER NOT NULL DEFAULT 0` + `MessagePo.priority: u8` | 新迁移；`DEFAULT 0` 保证存量行与既有行为完全不变 |
| 2 | `MessageCreatedEvent.priority: u8` + `fn priority(&self) -> u8 { self.priority }` | 封套顶层 `priority` 由 `registry.rs` 从该方法注入，无需改框架 |
| 3 | 三个发送 DTO 增 `priority: Option<u8>` | `#[serde(default, skip_serializing_if = "Option::is_none")]` + 折叠常量 `DEFAULT_MESSAGE_PRIORITY = 0` |
| 4 | Command / `MessagePo` 构造透传 | 用 builder（`.with_priority(p)`）而非改 `new` 签名，避免既有构造点大面积改动 |

**语义边界（必须同时写进工具描述，否则会被误用）**

1. **只重排「等待中」的消息，不能抢占「在飞」**：门闩只由 `ack` 释放 ⇒ 同 key 已有在飞消息时，高优先级消息最多排到**下一条**。要打断在飞 → 必须并用 `recall_message`。
2. **默认 0 = 现行为**：不给任何消息默认提权，否则等于悄悄改掉既有 FIFO 语义。
3. **越大越优先**（对齐 `EventRef::Ord` 的 `self.priority.cmp(&other.priority)`）；取值 `u8`，不引入 `PriorityLevel` 枚举——避免 LLM 参数 schema 出现判别联合，也避免多一层值映射。



## 四、关键边界 / 行为红线

1. 撤回写库是**终态**，**两条**收尾写入都不得覆盖它（这是 Phase 1 唯一需要改动既有逻辑的地方）：
   - `on_consumed` —— ack / Discard 后由框架回调（[registry.rs](src/pkg/aop/core/registry.rs#L871)），**当前无条件**写 `Processed`（[dal/message.rs](src/service/dal/message.rs#L798-L807)）⇒ 撤回的消息出队 → 守卫跳过 → `on_consumed` 照样把 `Recalled` 抹成 `Processed`（前端徽章退回「已处理」，撤回痕迹消失）；
   - `on_failed` —— `Retry` 时**当前无条件**写回 `Pending`（[dal/message.rs](src/service/dal/message.rs#L818-L835)）⇒ 重投收尾会把撤回撤销。
   ⇒ 两处都必须加「当前状态为 `Recalled` 则跳过写入」的守卫。
2. 撤回只在 `Pending` / `Processing` 生效；`Processed` / `Failed` 的撤回请求**幂等返回**（`already_recalled` / `not_recallable` 用字段区分，不报 error）。
3. 消费者守卫必须在**分发前**（`handle_message` 入口），覆盖 User / Agent / System 三种 `to_role`。
4. `Processing` 撤回是**尽力而为**：`cancel_thinking` 在**轮次边界**生效，当前轮 LLM 调用不可抢占；工具描述不得承诺"立即停止"。
5. 撤回**不物删**消息、不删 `messages` 行：`Recalled` 是逻辑作废（上下文窗口已按它过滤）。
6. 权限 gate 在 Domain 层**单点**实现，Handler 与 Tool 都调同一方法（禁两套查询逻辑，防字段漂移）。
7. **当前并无「启动恢复重投」在跑**（实测：`MessageDal::list_by_status` 无生产调用点、`MessageCreatedEvent` 只在 `save_message` 一处构造、`EventQueue` 只有 `InMemoryEventQueue`）⇒ 撤回语义**不依赖**恢复；一旦日后接线恢复（口径为只扫 `status = Pending`），`Recalled` 天然被排除。⚠️ 反过来这也意味着 `Pending` 消息在进程重启后会**静默滞留**——独立隐患，不在本能力范围，另行处置。
8. 撤回 `Processing` 时，必须**按 message_id 校验**当前 busy agent 正在处理的就是这条消息，避免误取消更新的消息。
9. 撤回 `TaskAssignment` / 系统通知类消息不改变任务状态机本身；任务级取消仍走 `TaskStatus::Cancelled`（本能力不替代它）。
10. **工具可达性与同链路工具一致**：`recall_message` 必须标 `neural`，并进 `message_tools_are_neural_reachable` 护栏清单；只挂 `messaging` 会让它不在任何 Agent 的工具面（死工具），且症状是静默失效而非报错。
11. **权限 gate 必须同时认「用户」与「Agent」**：Agent 侧调用（唤醒态 `caller_type = System`、只有 `agent_id`）不得因缺 `user_id` 被拒；错误文案必须**点名正确工具名 + 具体原因**（原样回灌给模型）。
12. **撤回者不得是「正在处理这条消息的那个 Agent 自己」**：`Processing` 撤回前若目标消息的 `current_message_id` 正是调用者自己，直接拒绝（避免用工具把自己打断成半截状态；自我中断应走 `cancel_thinking` 语义）。
13. **priority 默认必须为 0**（迁移对存量行也补 0）⇒ 不改变任何既有消息的消费顺序；只在发送方 / 工具显式指定时生效。
14. **priority 不得被表述为「抢占在飞」**：门闩只由 `ack` 释放，高优先级仅作用于同 key 的**等待队列**；工具描述不得承诺「打断当前处理」（那是 `recall_message` 的职责）。
15. **撤回态在业务读路径上「不存在」**：`MessageQuery` 有软删除默认过滤（`status != 0`，见 [dao/message/sqlite.rs::push_query_filters](src/service/dao/message/sqlite.rs#L630-L638)），而 `find_by_id` 也照此过滤 ⇒ 撤回的消息在消费者眼里是 `None`。**因此消费入口守卫必须在 `None` 分支上区分「已撤回」与「行不存在」**（靠一条不过滤撤回态的读路径，见 [dal/message.rs::find_by_id_with_recalled](src/service/dal/message.rs)），而不是在 `Some(m)` 之后判 `status`（那一段永远读不到撤回态，守卫形同虚设）。
16. **撤回的消息出队必须 ack（返回 `Ok`）**：若把它当「消息不存在」上抛，框架会 nack 重投最多 8 次，期间该 Agent 的队列被完全堵死 —— 与撤回的初衷（腾出队列）正好相反。
17. **在飞判定以运行时状态为准，不看 `messages.status`**：`MessageStatus::Processing` **当前没有任何写入路径**（生产只写 `Pending` / `Processed`），在飞消息在库里仍是 `Pending`。只看 DB 会把「正在跑」误判成「排队中」，于是只改状态、不发取消信号，Agent 继续跑完并回复。判定入口：[agent_runtime_state.rs::find_busy_agent_by_message](src/pkg/agent_runtime_state.rs)。
18. **撤回 / 取消这对工具必须对每个 Agent 可达**：`recall_message` 与 `cancel_thinking` 都要带 `neural`（`collaboration` / `messaging` 从未进过任何 Agent 的 `installed_tags` ⇒ 只挂它们等于**死工具**，且症状是静默失效）。`recall_message` 拒绝「撤回自己正在处理的消息」时会点名 `cancel_thinking` 作为替代 —— 后者不可达，这条引导就是空话。
19. **收件方拒收权（判据 ⑤）不得叠加 status 校验**：`ctx.agent_id() == message.to_id` 即放行，**不要**在这里再判 `status == Pending`。理由：下游状态 gate 已把 `Processed` / `Failed` 归一为 `NotRecallable`（no-op），提前加校验属重复且易与状态机漂移；且若这条消息正是收件方自己在处理的，步骤 ④ 会以其持有者身份命中「不得撤回自己正在处理的消息」并引导改用 `cancel_thinking`，语义闭环。
20. **`cancel_thinking` 的存在性校验归 Domain，不留在 Handler**：`CancelOutcome`（`Cancelled` / `NotThinking`）与 `NotFound` 的分发在 [runtime/cancel.rs](src/service/domain/runtime/cancel.rs) 单点完成，Handler 退化为纯 DTO 映射。❗不得把 `find_by_id` 校验写回 handler —— 那样 REST 与神经工具两个入口会各自漂移。⚠️ 该校验**不是**权限门，不要顺手在里面加关系判据（见 §3.2 权限取向）。

## 五、扩展模式

### 5.1 后续要把 priority 从「透传」扩到「抢占」

**本期的 priority 仍停留在「未落地」**：方案见 §3.3（发送入口 `Option<u8>` → 事件 `priority()` → 队列堆序），下一期落地时**先做透传**（默认 0 = 行为不变）；
**剩余未做的只有「让在飞消息让位」**这一件：需给 `EventQueue` 增抢占 / 取消原语，语义与 §5.3 同构（都是「把已上堆的那条从门闩上摘下来」）。

### 5.2 后续要加"reply 链级联撤回"

步骤 1 → `recall_message` 增 `cascade: bool` 参数 → 按 `root_id` 拉取该链下未处理消息批量撤回
步骤 2 → 级联边界（跨 root 停在哪 / 是否跳过已处理）在 §四 补红线

### 5.3 后续要把 Phase 1 升级为队列级 `cancel(event_id)`

步骤 1 → `EventQueue` trait 增 `cancel(ctx, event_id)`：移除**等待中**事件 + 维护门闩不变量（与 `ack` 的 pop-successor 逻辑同构）—— 参考 [in_memory.rs::ack](src/pkg/aop/queue/in_memory.rs#L303-L346)
步骤 2 → `message.created` 的 `event_id == message_id`（见 [message_dal.rs::message_id_of](src/service/dal/message.rs#L766-L772)），撤回天然可定位事件，无需额外映射
