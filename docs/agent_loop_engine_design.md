# Agent 循环驱动引擎设计

> 🎯 **本文档定位**：设计一套让 Agent 能够自主跟进项目进度、上报任务状态、在必要时通知用户的驱动机制。
>
> 关联文档：
> - [runtime_design.md](./runtime_design.md) — Agent 唤醒机制
> - [task_scheduler_design.md](./task_scheduler_design.md) — CronTrigger 定时任务
> - [consumer_architecture.md](./consumer_architecture.md) — AOP 事件中心
> - [project_management_design.md](./project_management_design.md) — 项目/任务管理
> - [agent_loop_engine_plan.md](./agent_loop_engine_plan.md) — 详细实施计划（逐 Task 步骤）

---

## 一、问题背景

### 当前状态

| 维度 | 现状 | 缺口 |
|------|------|------|
| **Task 实体** | 有 `progress`(0-100) + `status` 枚举，无执行计划/结果字段 | Agent 无法结构化记录"打算做什么"和"做完了什么" |
| **Project 实体** | 有 `status` + `owner_agent_id`，无进度字段、无巡检时间记录 | 项目进度只能人工推断，Owner Agent 无自主跟进能力 |
| **通知机制** | `MessageDelivery::send_to_user` 可发消息，但无 Task/Project 级事件 | 任务完成/状态变更不会自动通知 |
| **定时任务** | `CronTrigger` 仅支持 `agent_rest` action | 无法定时驱动 Agent 做项目跟进 |
| **AOP 事件** | 6 种事件，无 Task/Project 相关事件 | 无法事件驱动地响应任务状态变更 |

### 目标

1. **任务状态上报**：Agent 执行任务时记录执行计划和结果到 Task/Project 实体
2. **实时通知**：任务状态变更时通过 AOP 事件通知 Project Owner Agent 或用户
3. **补偿机制**：定时扫描进行中的项目，唤醒 Owner Agent 跟进进度、总结状态、决定下一步
4. **系统级 Seed**：首次启动自动创建睡眠 + 巡检两个默认定时任务

---

## 二、整体架构

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Layer 1: 任务状态上报                         │
│                                                                     │
│  ProjectPo 新增字段:                                                 │
│  ├─ execution_plan: Option<String>   (Owner Agent 制定阶段计划)      │
│  ├─ execution_result: Option<String>  (项目完成后总结)               │
│  └─ last_followup_at: Option<i64>    (上次巡检时间，关键指标)        │
│                                                                     │
│  TaskPo 新增字段:                                                    │
│  ├─ execution_plan: Option<String>   (Agent 执行前写入微观计划)      │
│  └─ execution_result: Option<String>  (Agent 执行后写入结果)         │
│                                                                     │
│  Project 业务实体 新增:                                              │
│  └─ progress_summary: Option<ProjectProgressSummary>                │
│     (不持久化，实时 from_tasks() 计算，注入在 get_project 中)         │
│                                                                     │
│  Agent 通过 update_task / update_project 神经工具更新                │
└───────────────────────────┬─────────────────────────────────────────┘
                            │ 状态变更（DAL 层 update_status 内部捕获）
                            ▼
┌─────────────────────────────────────────────────────────────────────┐
│                   Layer 2: AOP 事件通知（异步消费）                   │
│                                                                     │
│  DAL::update_status() → publish(TaskStatusChangedEvent)              │
│     (在 DAL 层统一发布，确保所有 status 变更都触发，含绕过 domain 的)  │
│                                                                     │
│  TaskEventConsumer [ConsumeMode::Async]                             │
│  ├─ 仅处理 Completed 状态变更                                        │
│  ├─ 合并去重：检查同 Agent 是否已有 Pending TaskDispatchNotification  │
│  ├─ send_to_agent(Owner Agent) → 消息类型 TaskDispatchNotification  │
│  │   ├─ 消息内容：结构化意图指令文本（嵌入行动清单）                   │
│  │   └─ 字段：project_id + task_id（触发上下文自动补充）              │
│  └─ 发送方不关注结果（Async，进度推进场景异步）                       │
└───────────────────────────┬─────────────────────────────────────────┘
                            │ 异步通知可能失败 / Consumer 崩溃
                            ▼
┌─────────────────────────────────────────────────────────────────────┐
│              Layer 3: 补偿机制（定时扫描，System Seed 自动创建）       │
│                                                                     │
│  CronTrigger 两个系统级默认触发器（consumer::init 时幂等创建）：       │
│  ├─ agent_rest: 4h/次 → load_and_settle (有预检查，Busy 则跳过)      │
│  └─ project_followup: 1h/次 → handle_project_followup                │
│                                                                     │
│  handle_project_followup:                                           │
│  1. list_in_progress_with_owner (系统级查询，无 user 过滤)            │
│  2. 对每个项目 Owner Agent 预检查状态 → 非空闲跳过（避免无意义 nack）   │
│  3. send_to_agent → 消息类型 ProjectFollowupNotification             │
│     ├─ 消息内容：结构化意图指令文本（嵌入行动清单）                    │
│     └─ 字段：project_id（触发上下文自动补充）                         │
│  4. save_message → publish(MessageCreatedEvent)                     │
│  5. MessageConsumer.handle_agent_message → awaken Owner Agent       │
│     （自动根据 project_id 补充 Project + Tasks 上下文）               │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 三、核心设计决策（与初稿对比）

### 3.1 意图传递：嵌入消息内容，不新增 AwakenIntent

**核心决策**：不新增 `AwakenIntent` 枚举 + `ThinkingOptions.intent`。意图指令直接嵌入**消息内容本身**。

| 维度 | 初稿方案（AwakenIntent） | 实际实现（嵌入消息内容） |
|------|------------------------|----------------------|
| **意图传递** | 枚举 + ThinkingOptions.intent + PromptBuilder 注入 | 消息内容中的 Markdown 行动指令 |
| **Prompt 注入** | determine_intent 路由 + builder.intent_context() | 无需特殊注入，Agent 读消息即可 |
| **上下文补充** | 需在 determine_intent 中额外处理 | 复用现有 MessageConsumer project/task 自动补充 |
| **复杂度** | 需新增枚举、路由、PromptBuilder 扩展 | 零新增，复用现有链路 |
| **可追溯性** | 意图在内存中的 ThinkingOptions 对象中，不持久化 | 意图写入消息内容，持久化、可查询历史 |

**为什么这样做**：
1. **流程统一**：场景 2/3 的系统通知消息和场景 1 的普通消息走完全相同的链路，MessageConsumer 无需分支。
2. **Agent 端一致**：Agent 唤醒后看到消息内容里有"📋 任务调度通知，请执行以下调度职责：1. 更新进度 2. 检查计划..."，直接按指令执行即可。
3. **上下文自动补充**：消息填充了 `project_id` + `task_id`，MessageConsumer 会自动补充 Project 实体，与场景 1 完全一致。

### 3.2 事件发布层：DAL 层，非 Domain 层

**核心决策**：`TaskStatusChangedEvent` 在 **DAL 层的 `update_status`** 中发布，而非 Domain 层的 `transition_status`。

原因：
1. **确保所有变更都触发**：即使有人绕过 domain 层直接调 DAL `update_status`（如 handler 快捷更新），事件也会发布。
2. **old_status 捕获方便**：DAL 层在 UPDATE 前 `find_by_id` 读旧状态，逻辑更内聚。
3. **发布条件**：`old_status != new_status` 时才发布，避免 no-op 过渡。

### 3.3 Consumer 模式：Async，非 Sync

**核心决策**：`TaskEventConsumer` 使用 `ConsumeMode::Async`，发送事件的一方**无需关注结果**。

```
初稿：Sync → DAL publish 时阻塞等待 TaskEventConsumer 完成（含 send_to_agent DB 写入）
实际：Async → DAL publish 立即返回，进度推进场景异步消费
```

### 3.4 Project 进度获取：get_project(with_progress_summary=true)

**核心决策**：不新增 `get_project_progress` 独立工具。扩展现有 `ProjectFetchOptions`：

```rust
pub struct ProjectFetchOptions {
    // ... 现有 with_stats, with_model_call_stats, with_tasks ...
    pub with_progress_summary: Option<bool>,  // 新增
}
```

`ProjectServiceImpl::get_project` 中，当 `with_progress_summary == Some(true)` 时：
1. 查询项目下所有 Task
2. 调用 `progress_summary_from_tasks(&tasks)` 构建 `ProjectProgressSummary`
3. 注入到 `Project.progress_summary`

`GetProjectResponse` 新增 `progress_summary: Option<ProjectProgressSummary>` 字段返回给调用方（包括 Agent 神经工具）。

Agent 跟进项目调用：`get_project(with_progress_summary=true)`

### 3.5 Seed 去重：按 action 类型，非 trigger ID

**核心决策**：系统级触发器的幂等创建，检查现有 trigger 的 `payload` 中是否包含相同的 `action`（如 `"agent_rest"`），而非检查 ID。

```rust
fn ensure_system_cron_triggers(ctx) {
    let existing = cron_manager.list_triggers(ctx, CronTriggerQuery::default()).await?;
    let has_agent_rest = existing.iter().any(|t| t.payload.contains("\"agent_rest\""));
    let has_project_followup = existing.iter().any(|t| t.payload.contains("\"project_followup\""));

    if !has_agent_rest { /* create */ }
    if !has_project_followup { /* create */ }
}
```

原因：用户可能已经手动创建了 `agent_rest` 触发器（ID 不同），检查 action 能正确识别用户已有的同类型触发器，不重复创建。

---

## 四、Layer 1：任务与项目状态上报

### 4.1 实体层级关系

```
Project (宏观)
├─ execution_plan: 项目执行计划（Owner Agent 制定，描述阶段划分和推进策略）
├─ execution_result: 项目总结（项目完成后写入）
├─ last_followup_at: i64（上次巡检时间戳，关键指标）
├─ progress_summary: 进度汇总（❌ 不持久化，实时计算注入）
│
└─ Task × N (微观)
   ├─ execution_plan: 任务执行计划（执行 Agent 制定）
   ├─ execution_result: 任务执行结果（执行 Agent 完成后写入）
   └─ progress: 任务进度 0-100（持久化）
```

**核心原则**：最真实的项目进度 = 项目执行计划 + 所有任务状态/进度实时汇总，不是手动填的字段。

### 4.2 ProjectPo 字段扩展

在 `src/models/project.rs` 的 `ProjectPo` 中新增三个字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| `execution_plan` | `Option<String>` | Owner Agent 制定的项目执行计划（阶段划分、推进策略） |
| `execution_result` | `Option<String>` | 项目完成后的总结（成果概述、产出清单、经验教训） |
| `last_followup_at` | `Option<i64>` | 上次项目巡检/跟进时间戳（由 Layer 3 更新或 Agent 更新） |

**与现有字段区分**：
- `workflow`：用户配置的协作方式（"前端用 Dioxus，后端用 Axum"）—— 人写的
- `guidance`：用户对项目的指导建议（"优先实现核心功能"）—— 人写的
- `execution_plan`：Agent 制定的执行计划（"Phase 1: 搭建脚手架 → Phase 2"）—— Agent 写的
- `execution_result`：Agent 完成后的总结 —— Agent 写的

### 4.3 TaskPo 字段扩展

在 `src/models/task.rs` 的 `TaskPo` 中新增两个字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| `execution_plan` | `Option<String>` | Agent 执行任务前写入的计划（微观，"我打算怎么做这个任务"） |
| `execution_result` | `Option<String>` | Agent 执行完成后写入的结果总结 |

### 4.4 数据库迁移

```sql
-- migrations/20260806000001_execution_plan_result.sql
ALTER TABLE projects ADD COLUMN execution_plan TEXT;
ALTER TABLE projects ADD COLUMN execution_result TEXT;
ALTER TABLE projects ADD COLUMN last_followup_at INTEGER;
ALTER TABLE tasks ADD COLUMN execution_plan TEXT;
ALTER TABLE tasks ADD COLUMN execution_result TEXT;
```

### 4.5 ProjectProgressSummary（实时计算，不持久化）

**位置**：`common/src/models/stats.rs`（放 common 因为 `GetProjectResponse` 需要引用）

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
pub struct ProjectProgressSummary {
    pub total_tasks: usize,
    pub completed: usize,
    pub in_progress: usize,
    pub pending: usize,
    pub blocked: usize,
    pub cancelled: usize,
    pub overall_percent: u32,   // Σ(task.progress) / total_tasks
}
```

**注**：由于 Rust orphan 规则（`ProjectProgressSummary` 定义在 common crate），`from_tasks()` 作为 `src/models/project.rs` 中的**自由函数** `progress_summary_from_tasks()`，而非 inherent impl。

在 `src/models/project.rs` 的 `Project` 业务实体中新增：

```rust
pub struct Project {
    pub po: ProjectPo,
    pub search_match: Option<SearchMatchInfo>,
    pub stats: Option<ProjectStats>,
    pub model_call_stats: Option<ModelCallStats>,
    pub task_graph: Option<String>,
    pub artifacts: Option<Vec<ArtifactDetail>>,
    pub progress_summary: Option<ProjectProgressSummary>,  // 新增
}
```

### 4.6 工具层扩展

**UpdateTaskRequest / UpdateTask 工具**：新增可选参数 `execution_plan` / `execution_result`

**UpdateProjectRequest / UpdateProject 工具**：新增可选参数 `execution_plan` / `execution_result`

**GetProjectRequest**：新增 query 参数 `with_progress_summary: Option<bool>`

**GetProjectResponse**：新增 `progress_summary: Option<ProjectProgressSummary>`

### 4.7 技能文档更新

已更新 `seed/skills/TEMPLATE_PROJECT_MANAGEMENT/skill.md`，新增三个章节：
- **任务执行规范**：开始前写 execution_plan，完成后写 execution_result，更新 progress/status
- **项目执行规范（Owner Agent 专属）**：启动时写 execution_plan，跟进时调 `get_project(with_progress_summary=true)`，完成后写 execution_result
- **系统通知响应**：收到「任务调度通知」或「项目进度定期检查」消息时按消息行动指令执行

---

## 五、Layer 2：AOP 事件通知

### 5.1 TaskStatusChangedEvent

**位置**：`src/models/events/task_status.rs`

```rust
pub struct TaskStatusChangedEvent {
    pub event_id: String,         // Uuid::now_v7()
    pub task_id: String,
    pub task_title: String,
    pub project_id: Option<String>,
    pub assignee_id: String,
    pub old_status: TaskStatus,
    pub new_status: TaskStatus,
    pub progress: i32,
    pub created_at: i64,          // current_timestamp_ms()
}

impl Event for TaskStatusChangedEvent {
    fn kind(&self) -> EventKind { EventKind::new("task.status_changed") }
    fn id(&self) -> &str { &self.event_id }
    fn order_key(&self) -> &str { &self.task_id }
    fn created_at(&self) -> i64 { self.created_at }
}
```

### 5.2 发布时机（DAL 层）

在 `TaskDalImpl::update_status`（`src/service/dal/task.rs`）中：

```rust
async fn update_status(&self, ctx: &RequestContext, task_id: &str, new_status: TaskStatus) -> Result<()> {
    // 1. 先读旧状态
    let old_task = self.task_dao.find_by_id(ctx, task_id).await?
        .ok_or_else(|| Error::not_found("task"))?;
    let old_status = old_task.status;

    if old_status == new_status {
        return Ok(());  // no-op，不发事件
    }

    // 2. 执行 SQL UPDATE
    self.task_dao.update_status(ctx, task_id, new_status).await?;

    // 3. DAL 层发布事件（统一入口，所有变更都触发）
    let _ = crate::pkg::aop::publish(TaskStatusChangedEvent::new(
        task_id,
        &old_task.title,
        old_task.project_id.as_deref(),
        &old_task.assignee_id,
        old_status,
        new_status,
        old_task.progress,
    )).await;

    Ok(())
}
```

### 5.3 消息通信 vs 任务状态流转：分层而非重复

| 机制 | 类比 | 语义 | 触发方 |
|------|------|------|--------|
| **消息** (Message) | 聊天/即时通讯 | "做完了，结果是..."（有内容、有上下文） | Agent 主动发送 |
| **任务状态流转** (Task transition) | 看板/Ticket | 卡片从 InProgress → Done（结构化、可查询） | Agent 调用工具更新 |
| **execution_plan/result** | 任务卡片备注 | 计划和总结文档（非实时、供读取） | Agent 写入 Task 字段 |

三层递进兜底：

```
Layer 0: 子 Agent 主动发消息（正常路径，~95% 的情况）
    ↓ 失败/遗漏（Agent 崩溃、消息发送失败）
Layer 2: TaskStatusChangedEvent 补偿判断（检查是否已有 Pending 调度通知，~5%）
    ↓ Consumer 也挂了
Layer 3: 定时扫描兜底（最终防线，<1%）
```

### 5.4 TaskEventConsumer [Async]

**位置**：`src/consumer/task_event_consumer.rs`

```rust
pub struct TaskEventConsumer;

impl Consumer for TaskEventConsumer {
    fn name(&self) -> &str { "task_event" }
    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::new("task.status_changed")]
    }
    fn consume_mode(&self) -> ConsumeMode { ConsumeMode::Async }  // 异步，发送方不等待

    async fn on_event(&self, event: serde_json::Value) -> Result<()> {
        let event: TaskStatusChangedEvent = serde_json::from_value(event)?;

        // 仅处理任务完成事件（其他状态变更暂不通知）
        if event.new_status != TaskStatus::Completed { return Ok(()); }
        let Some(project_id) = &event.project_id else { return Ok(()); };

        let ctx = RequestContext::new_system();

        // 查询项目 Owner Agent
        let project = project_domain().project_manage().get(ctx.clone(), project_id).await?;
        let project = project.ok_or_else(|| Error::not_found("project"))?;
        let Some(owner_agent_id) = &project.po.owner_agent_id else { return Ok(()); };

        // 合并去重：检查是否已有同 Agent 的 Pending TaskDispatchNotification
        // 如果 Agent 已有未处理的调度消息，说明它已经在工作了
        let has_pending = message_domain()
            .has_pending_message_for_agent(ctx.clone(), owner_agent_id, MessageType::TaskDispatchNotification)
            .await?;
        if has_pending {
            debug!("已有 Pending 的 TaskDispatch 消息，跳过本次通知");
            return Ok(());
        }

        // 构建消息内容（意图指令直接嵌入消息本体）
        // Agent 读消息内容就知道"为什么被叫醒、该做什么"
        let content = message::builder::build_task_dispatch_content(
            &event.task_title, event.new_status, event.progress,
        );

        // 发送消息：填充 project_id + task_id → MessageConsumer 自动补充上下文
        let cmd = SendToAgentCommand {
            from_id: "system",
            from_role: MessageRole::System,
            to_agent_id: owner_agent_id,
            content: &content,
            project_id: Some(project_id),
            task_id: Some(&event.task_id),
            reply_to_id: None,
            attachment_ids: None,
            message_type: MessageType::TaskDispatchNotification,  // 场景 2 消息类型
        };
        message_domain().delivery().send_to_agent(ctx, cmd).await?;

        Ok(())
    }
}
```

### 5.5 has_pending_message_for_agent

新增于 `MessageDomain` trait（`src/service/domain/message/mod.rs`），`MessageDal` 实现。底层通过扩展 `MessageQuery` 增加 `to_role: Option<MessageRole>` 和 `message_type: Option<MessageType>` 过滤器，复用现有 `count()` 方法：

```rust
async fn has_pending_message_for_agent(
    &self, ctx: RequestContext, agent_id: &str, message_type: MessageType,
) -> Result<bool> {
    let count = self.message_dal.count(ctx, MessageQuery {
        to_id: Some(agent_id.to_string()),
        to_role: Some(MessageRole::Agent),
        message_type: Some(message_type),
        status_in: Some(vec![MessageStatus::Pending]),
        ..Default::default()
    }).await?;
    Ok(count > 0)
}
```

---

## 六、Layer 3：补偿机制（核心）

### 6.1 CronTriggerConsumer 新增 project_followup 分支

在 `src/consumer/scheduler.rs` 的 `CronTriggerConsumer::on_event` 中：

```rust
match payload.action.as_str() {
    "agent_rest" => self.handle_agent_rest(&event, &payload.extra).await?,
    "project_followup" => self.handle_project_followup(&payload.extra).await?,  // 新增
    _ => sys_warn!("未知 action: {}", payload.action),
}
```

### 6.2 handle_project_followup 实现

```rust
async fn handle_project_followup(&self, _extra: &Value) -> Result<()> {
    let ctx = RequestContext::new_system();

    // 1. 查询所有 InProgress 且有 owner_agent_id 的项目（系统级查询，无 user 过滤）
    let projects = project_domain()
        .project_manage()
        .list_in_progress_with_owner(ctx.clone())
        .await?;

    for project in projects {
        let owner_agent_id = match &project.po.owner_agent_id {
            Some(id) => id,
            None => continue,
        };

        // 2. 预检查：Agent 非空闲跳过（避免无意义 nack 堆积）
        // 定时任务是周期性的，Agent 忙碌等下一轮即可
        let state = AgentRuntimeStateManager::global().get_state(owner_agent_id);
        if state.is_unavailable() {
            sys_info!("Agent {} 当前 {:?}，跳过项目跟进", owner_agent_id, state);
            continue;
        }

        // 3. 构建消息内容（意图指令嵌入消息本体）
        let content = message::builder::build_project_followup_content(&project.po.name);

        // 4. 发送消息：project_id 填充 → MessageConsumer 自动补充 Project + Tasks
        let cmd = SendToAgentCommand {
            from_id: "system",
            from_role: MessageRole::System,
            to_agent_id: owner_agent_id,
            content: &content,
            project_id: Some(&project.po.id),
            task_id: None,
            reply_to_id: None,
            attachment_ids: None,
            message_type: MessageType::ProjectFollowupNotification,  // 场景 3 类型
        };
        if let Err(e) = message_domain().delivery().send_to_agent(ctx.clone(), cmd).await {
            sys_warn!("发送项目跟进消息失败: agent={}, err={}", owner_agent_id, e);
        }
    }

    Ok(())
}
```

### 6.3 list_in_progress_with_owner

Domain 方法（`ProjectManage::list_in_progress_with_owner`），内部调用 DAL `list_all_by_status(ctx, ProjectStatus::InProgress, None)`（系统级查询，不按 `root_user_id` 过滤），然后过滤 `owner_agent_id.is_some()`。

DAL 层 `list_all_by_status` 是 query 的语法糖：`query(ctx, ProjectQuery { status_in: Some(vec![status]), root_user_id: None, .. })`。

### 6.4 System Seed：系统级默认定时任务自动创建

**位置**：`src/service/domain/system/mod.rs` → `ensure_system_cron_triggers(ctx)`

**调用时机**：`consumer::init()` 末尾（async 环境，Consumer 已注册完毕）。即使创建失败，只打印 warn，不终止系统启动。

创建两个触发器（按 action 去重，用户已有同类型则跳过）：

| 触发器 | interval | payload |
|--------|----------|---------|
| **系统默认-Agent 睡眠沉淀** | 4h | `{"action":"agent_rest","extra":{"settle_limit":10}}` |
| **系统默认-项目进度巡检** | 1h | `{"action":"project_followup","extra":{}}` |

用户可在管理面自行修改间隔、禁用，或删除后重启恢复（重新触发 ensure）。

---

## 七、MessageType 扩展 + 消息内容构建器

### 7.1 MessageType 新增

在 `common/src/enums/message.rs`：

```rust
pub enum MessageType {
    // ... 现有 0-9 (Text, Image, File, ..., TaskAssignment=9) ...
    /// 任务调度通知（System→Agent，任务状态变更触发）
    TaskDispatchNotification = 10,
    /// 项目跟进通知（System→Agent，定时补偿触发）
    ProjectFollowupNotification = 11,
}
```

`to_prompt()` 中映射：
- `TaskDispatchNotification => "任务调度通知"`
- `ProjectFollowupNotification => "项目跟进通知"`

### 7.2 SendToAgentCommand 新增 message_type 字段

```rust
pub struct SendToAgentCommand<'a> {
    // ... 现有 from_id, from_role, to_agent_id, content, project_id, task_id ...
    /// 消息类型（默认 Text，系统通知可指定 TaskDispatchNotification 等）
    pub message_type: common::enums::message::MessageType,
}
```

`delivery.send_to_agent` 内部使用 `cmd.message_type` 替代原硬编码的 `MessageType::Text`。

所有现有调用方（共 11 处）都补充了 `message_type: MessageType::Text` 以保持兼容。

### 7.3 消息内容构建器（message/builder.rs）

意图指令**直接嵌入消息内容**的核心实现。Agent 读取消息内容就知道该做什么，不需要额外的 AwakenIntent 路由。

**build_task_dispatch_content(task_title, new_status, progress) → String**：

```
📋 任务调度通知
任务：「{task_title}」状态变更为「{new_status_label}」（进度：{progress}%）

作为项目 Owner Agent，请执行以下调度职责：

1. **更新进度**：调用 get_project(with_progress_summary=true) 获取最新进度汇总
2. **检查计划**：对比 execution_plan，判断当前进展是否符合预期
3. **调度下一任务**：
   - 检查是否有后续任务的依赖已满足（前置任务已完成）
   - 如有，通过 send_to_agent 通知对应 Agent 开始执行
   - 如无后续任务，检查是否所有任务已完成 → 更新项目状态为 Completed
4. **通知用户**（仅在必要时）：阶段性里程碑达成、发现阻塞风险需要用户决策
```

**build_project_followup_content(project_name) → String**：

```
📊 项目进度定期检查
项目：「{project_name}」

系统定时触发了项目跟进检查，请执行以下检查：

1. **获取进度**：调用 get_project(with_progress_summary=true) 获取整体进度
2. **识别阻塞**：
   - 检查 InProgress 任务是否有长时间无更新的（可能卡住了）
   - 检查 Pending 任务是否因依赖阻塞无法启动
3. **对比计划**：对照 execution_plan，判断当前阶段是否正常推进
4. **采取行动**：
   - 阻塞任务 → 分析原因，调整分配或通知用户
   - 全部完成 → 更新项目状态为 Completed
   - 进展正常 → 如有阶段性进展，通知用户
   - 需要调整计划 → 更新 execution_plan
```

---

## 八、并发安全修复：load_and_settle 预检查

**问题**：`load_and_settle` 会调用 `set_resting()`，如果 Agent 当前是 Busy（正在处理消息），会覆盖 Busy 状态。

**修复**（`src/handlers/hr/agent/settle_memory.rs`）：

```rust
pub(crate) async fn load_and_settle(
    ctx: RequestContext, agent_id: &str, settle_limit: usize,
) -> Result<usize> {
    // 预检查：Agent 必须空闲才能进入睡眠
    let state = AgentRuntimeStateManager::global().get_state(agent_id);
    if state.is_unavailable() {  // Busy | Resting
        tracing::info!("Agent {} 当前 {:?}，跳过睡眠", agent_id, state);
        return Ok(0);
    }

    // ... 后续逻辑不变 ...
}
```

与场景 3（project_followup）的预检查模式一致：Agent 非空闲就跳过，等下一次定时触发。

---

## 九、防重策略总览

| 场景 | 策略 | 原理 |
|------|------|------|
| **场景 1** Agent 间通信 | 自然去重（频率自限） | Agent 自己决定何时通信，不会狂发 |
| **场景 2** TaskDispatch | has_pending_message_for_agent 合并去重 | 已有 Pending 调度通知就跳过，Agent 处理时会看到最新状态 |
| **场景 2** TaskDispatch（nack） | MessageConsumer try_set_busy 失败 → nack → 自动重试 | 任务完成是重要信号，不能因 Agent 忙碌跳过，排队等 |
| **场景 3** ProjectFollowup | 发送前 AgentState 预检查，非空闲跳过 | 定时是周期性的，等下一轮即可 |
| **Seed 触发器** | 按 action 去重 | 用户已有同类型触发器不重复创建 |

---

## 十、四种唤醒方式的统一视图

```
                ┌─────────────────────────────────────────┐
                │           Agent 唤醒方式                  │
                └────────┬──────────┬──────────┬──────────┘
                         │          │          │
                    真消息链路    直接调用    直接调用
                         │          │          │
                ┌────────▼──┐  ┌────▼────┐  ┌──▼──────────┐
                │ 场景 1/2/3 │  │  睡眠   │  │ 内部 summary │
                │           │  │         │  │             │
                │ msg_type: │  │ scene:  │  │ scene:      │
                │ Text/TaskD│  │ Settle  │  │ Summary     │
                │ isp/      │  │         │  │             │
                │ Followup  │  │ Agent   │  │ 思考轮次    │
                │           │  │ 状态预  │  │ 耗尽触发    │
                │ 意图嵌入  │  │ 检查    │  │             │
                │ 消息本体  │  └─────────┘  └─────────────┘
                │ content+  │
                │ project_id│
                │ /task_id  │
                │ → 自动补充│
                │ 上下文    │
                └───────────┘
```

**关键统一**：场景 1（自由对话）、场景 2（任务调度）、场景 3（定时跟进）三者统一通过 `send_to_agent` 走完整的消息链路，区别仅在 `message_type` 和消息内容（意图指令）。消息链路的持久化、nack 重试、上下文补充机制对所有场景完全一致。

---

## 十一、完整调用链路

```
[CronTrigger 每 1h 扫描]
    │
    ▼
[CronTriggerProducer.poll()]
    │ list_due_triggers → publish(CronTriggerEvent)
    ▼
[CronTriggerConsumer.on_event()]                    ← Layer 3
    │ action = "project_followup"
    │ → handle_project_followup()
    │   → list_in_progress_with_owner
    │   → AgentState 预检查（非空闲跳过）
    │   → send_to_agent(msg_type=ProjectFollowupNotification,
    │                    content=意图指令文本, project_id=...)
    ▼
[MessageDal.save_message()]
    │ → publish(MessageCreatedEvent)
    ▼
[MessageConsumer.handle_agent_message()]             ← 统一入口
    │ → try_set_busy(agent_id)
    │   失败 → nack → 消息回 Pending → 自动重试
    │ → wake_agent_brain()（如需要）
    │ → 检测 message.project_id → 查询 Project + Tasks → 注入
    │ → awakening().awaken(ctx, agent, message, options.scene=Awaken)
    ▼
[Agent 思考循环]
    │ 读取 Prompt：
    │   - 项目管理技能（含任务执行规范 + 系统通知响应）
    │   - 项目上下文（Project 实体，含 execution_plan）
    │   - 消息内容（场景 3 结构化跟进指令 + 行动清单）
    │   - 任务上下文（如有 task_id）
    │ 调用工具: get_project(with_progress_summary=true) /
    │          update_task / update_project / send_to_agent
    │
    ├── 更新 task.execution_result + task.status = Completed
    │   → DAL::update_status()
    │       → publish(TaskStatusChangedEvent)        ← Layer 2
    │           → TaskEventConsumer [Async]
    │               → 仅 Completed 处理
    │               → has_pending_message_for_agent 合并去重
    │               → send_to_agent(Owner Agent,
    │                     msg_type=TaskDispatchNotification,
    │                     content=意图指令文本,
    │                     project_id + task_id)
    │               → 重新进入 MessageConsumer（和上面同路径）
    │
    ├── 更新 project.execution_result + project.status = Completed
    │
    └── send_to_user（阶段性里程碑）
        → deliver_message → 飞书/SSE/推送渠道
```

---

## 十二、设计文档 vs 实际实现对照表

| 设计初稿章节 | 实际实现差异 |
|------------|------------|
| **2. 整体架构** | 已对齐，补充 last_followup_at 字段说明 |
| **3.1 Project 字段** | 新增 last_followup_at 字段 |
| **3.5 ProgressSummary 位置** | 移到 common/src/models/stats.rs（GetProjectResponse 需要引用），from_tasks 改为自由函数 progress_summary_from_tasks（orphan 规则） |
| **3.6 新增 get_project_progress 工具** | ❌ 移除。改为 get_project(with_progress_summary=true) |
| **4.2 发布时机 domain/transition_status** | ❌ 改为 DAL/update_status（统一入口） |
| **4.4 TaskEventConsumer Sync + has_reported** | ❌ 改为 Async + has_pending_message_for_agent（按同类型 Pending 消息合并去重） |
| **5.2 CronTrigger 手动创建** | ❌ 改为 consumer::init() 幂等自动创建，按 action 去重 |
| **6.1 AwakenIntent 枚举 + ThinkingOptions.intent** | ❌ 移除。意图直接嵌入消息内容本体 |
| **6.2 MessageConsumer determine_intent 路由** | ❌ 移除。统一链路，无特殊分支 |
| **6.5 Prompt Builder 注入** | ❌ 移除。Agent 读消息内容即可 |
| **6.6 消息结构化内容** | ✅ 扩展为完整行动清单（build_task_dispatch_content / build_project_followup_content），不只是简报 |
| **7. 技能文档补充** | ✅ 已更新 skill.md，新增三个章节（任务执行规范 / 项目执行规范 / 系统通知响应） |
| （新增） | load_and_settle 并发预检查修复 |
