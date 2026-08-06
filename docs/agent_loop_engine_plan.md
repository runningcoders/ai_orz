# Agent 循环驱动引擎 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 Agent 能自主跟进项目进度、上报任务状态、在必要时通知用户，通过消息内容携带意图指令实现三种场景的统一处理。

**Architecture:** 三层递进兜底——Layer 1 任务状态上报（实体字段扩展）、Layer 2 AOP 事件通知（DAL 层发布 TaskStatusChangedEvent + Async Consumer 补偿去重）、Layer 3 定时补偿（CronTrigger project_followup 扫描 InProgress 项目唤醒 Owner Agent）。**核心设计**：不引入 AwakenIntent 枚举，意图指令直接嵌入消息内容，复用现有 MessageConsumer → awaken 链路自动补充 project/task 上下文。

**Tech Stack:** Rust, Axum, sqlx (SQLite), AOP event system, CronTrigger

**Design Doc:** [agent_loop_engine_design.md](./agent_loop_engine_design.md)

---

## Part 1: 消息链路扩展

### Task 1: MessageType 扩展 + SendToAgentCommand.message_type + 消息内容构建器

> **设计决策**：不新增 AwakenIntent 枚举。意图指令通过消息内容本身传递（场景 2/3 的系统通知消息包含结构化的行动指令文本），复用现有 MessageConsumer → awaken 链路自动补充 project/task 上下文。MessageType 新增变体仅用于去重判断和未来分析，不用于 prompt 路由。

**Files:**
- Modify: `common/src/enums/message.rs` (MessageType 新增两个变体)
- Modify: `src/service/domain/message/mod.rs` (SendToAgentCommand 新增 message_type 字段)
- Modify: `src/service/domain/message/delivery.rs` (send_to_agent 使用 cmd.message_type)
- Create: `src/service/domain/message/builder.rs` (消息内容构建器)
- Modify: 所有 `SendToAgentCommand` 调用方（补充 message_type: MessageType::Text）

- [ ] **Step 1: 在 `common/src/enums/message.rs` 新增 MessageType 变体**

在 `MessageType` 枚举中，`TaskAssignment = 9` 之后新增：

```rust
    /// 任务调度通知（System→Agent，任务状态变更触发）
    TaskDispatchNotification = 10,
    /// 项目跟进通知（System→Agent，定时补偿触发）
    ProjectFollowupNotification = 11,
```

- [ ] **Step 2: 在 SendToAgentCommand 新增 message_type 字段**

在 `src/service/domain/message/mod.rs` 的 `SendToAgentCommand` 结构体中新增：

```rust
pub struct SendToAgentCommand<'a> {
    // ... 现有字段 ...
    /// 消息类型（默认 Text，系统通知可指定 TaskDispatchNotification 等）
    pub message_type: common::enums::message::MessageType,
}
```

- [ ] **Step 3: 更新 send_to_agent 实现使用 cmd.message_type**

在 `src/service/domain/message/delivery.rs` 的 `send_to_agent` 方法中，将 `MessageType::Text` 替换为 `cmd.message_type`。

- [ ] **Step 4: 更新所有 SendToAgentCommand 调用方**

全局搜索 `SendToAgentCommand {` 的所有使用处，为每个添加 `message_type: MessageType::Text`（保持现有行为不变）。重点关注：
- `src/handlers/agent/tools/send_tool_call_message.rs`
- `src/handlers/agent/tools/request_tool_call.rs`
- 其他使用 `send_to_agent` 的 handler

- [ ] **Step 5: 创建消息内容构建器**

创建 `src/service/domain/message/builder.rs`：

```rust
use common::enums::task::TaskStatus;

/// 构建任务调度通知消息内容（场景 2：任务状态变更触发）
///
/// 意图指令嵌入消息本体，Agent 读取后自动执行调度职责。
/// MessageConsumer 会根据 message.project_id / task_id 自动补充上下文。
pub fn build_task_dispatch_content(
    task_title: &str,
    new_status: TaskStatus,
    progress: i32,
) -> String {
    format!(
        "📋 任务调度通知\n\
         任务：「{}」状态变更为「{}」（进度：{}%）\n\n\
         作为项目 Owner Agent，请执行以下调度职责：\n\n\
         1. **更新进度**：调用 get_project(with_progress_summary=true) 获取最新进度汇总\n\
         2. **检查计划**：对比 execution_plan，判断当前进展是否符合预期\n\
         3. **调度下一任务**：\n\
            - 检查是否有后续任务的依赖已满足（前置任务已完成）\n\
            - 如有，通过 send_to_agent 通知对应 Agent 开始执行\n\
            - 如无后续任务，检查是否所有任务已完成 → 更新项目状态为 Completed\n\
         4. **通知用户**（仅在必要时）：阶段性里程碑达成、发现阻塞风险需要用户决策",
        task_title,
        task_status_label(new_status),
        progress,
    )
}

/// 构建项目跟进通知消息内容（场景 3：定时补偿触发）
pub fn build_project_followup_content(project_name: &str) -> String {
    format!(
        "📊 项目进度定期检查\n\
         项目：「{}」\n\n\
         系统定时触发了项目跟进检查，请执行以下检查：\n\n\
         1. **获取进度**：调用 get_project(with_progress_summary=true) 获取整体进度\n\
         2. **识别阻塞**：\n\
            - 检查 InProgress 任务是否有长时间无更新的（可能卡住了）\n\
            - 检查 Pending 任务是否因依赖阻塞无法启动\n\
         3. **对比计划**：对照 execution_plan，判断当前阶段是否正常推进\n\
         4. **采取行动**：\n\
            - 阻塞任务 → 分析原因，调整分配或通知用户\n\
            - 全部完成 → 更新项目状态为 Completed\n\
            - 进展正常 → 如有阶段性进展，通知用户\n\
            - 需要调整计划 → 更新 execution_plan",
        project_name,
    )
}

fn task_status_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Cancelled => "已取消",
        TaskStatus::PendingReview => "待审核",
        TaskStatus::Pending => "待开始",
        TaskStatus::InProgress => "进行中",
        TaskStatus::Completed => "已完成",
        TaskStatus::Archived => "已归档",
    }
}
```

- [ ] **Step 6: 在 message mod.rs 中注册 builder 模块**

在 `src/service/domain/message/mod.rs` 中新增：

```rust
pub mod builder;
```

- [ ] **Step 7: cargo check 全量编译**

Run: `cargo check --all`
Expected: 无编译错误（如有 SendToAgentCommand 调用方遗漏 message_type 字段，根据编译错误逐一修复）

- [ ] **Step 8: 编写单元测试**

在 `src/service/domain/message/builder.rs` 中新增 `#[cfg(test)]` 模块：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_task_dispatch_content() {
        let content = build_task_dispatch_content("搭建脚手架", TaskStatus::Completed, 100);
        assert!(content.contains("任务调度通知"));
        assert!(content.contains("搭建脚手架"));
        assert!(content.contains("已完成"));
        assert!(content.contains("get_project"));
    }

    #[test]
    fn test_build_project_followup_content() {
        let content = build_project_followup_content("AI 助手开发");
        assert!(content.contains("项目进度定期检查"));
        assert!(content.contains("AI 助手开发"));
        assert!(content.contains("识别阻塞"));
    }
}
```

- [ ] **Step 9: 运行测试**

Run: `cargo test --lib -- message::builder`
Expected: 2 tests passed

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "feat(message): MessageType 扩展 + SendToAgentCommand.message_type + 消息内容构建器"
```

---

## Part 2: 补偿机制

### Task 2: load_and_settle 并发安全修复

**Files:**
- Modify: `src/handlers/hr/agent/settle_memory.rs` (load_and_settle 预检查)

- [ ] **Step 1: 在 load_and_settle 入口添加状态预检查**

在 `src/handlers/hr/agent/settle_memory.rs` 的 `load_and_settle` 函数开头（`build_pending_memories_summary` 之前）新增：

```rust
pub(crate) async fn load_and_settle(
    ctx: RequestContext,
    agent_id: &str,
    settle_limit: usize,
) -> Result<usize> {
    // 预检查：Agent 必须空闲才能进入睡眠，避免覆盖 Busy 状态
    let state = crate::pkg::agent_runtime_state::AgentRuntimeStateManager::global()
        .get_state(agent_id);
    if state.is_unavailable() {
        tracing::info!("Agent {} 当前 {:?}，跳过睡眠", agent_id, state);
        return Ok(0);
    }

    // 1. Query unsettled short-term memories + build numbered summary
    let (summary, pending_count) =
```

- [ ] **Step 2: 运行现有测试确保不回归**

Run: `cargo test --lib -- settle_memory`
Expected: 现有测试全部通过

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "fix(settle): load_and_settle 预检查 Agent 状态避免覆盖 Busy"
```

---

### Task 3: ProjectDomain list_in_progress_projects_with_owner

**Files:**
- Modify: `src/service/domain/project/mod.rs` (ProjectManage trait 新增方法)
- Modify: `src/service/domain/project/service.rs` (实现)
- Test: `tests/integration/`

- [ ] **Step 1: 在 ProjectManage trait 新增方法**

在 `src/service/domain/project/mod.rs` 的 `ProjectManage` trait 中新增：

```rust
    /// 查询所有进行中且有 Owner Agent 的项目
    async fn list_in_progress_with_owner(
        &self,
        ctx: RequestContext,
    ) -> Result<Vec<Project>>;
```

- [ ] **Step 2: 在 ProjectServiceImpl 中实现**

在 `src/service/domain/project/service.rs` 中实现。注意系统调用时需要查询所有用户的项目，不能按 root_user_id 过滤。如果现有 `list` 方法强制按 root_user_id 过滤，需要添加一个不带用户过滤的内部查询方法。

- [ ] **Step 3: 编写测试**

```rust
#[sqlx::test]
async fn test_list_in_progress_with_owner(pool: SqlitePool) {
    // 创建项目（有 owner_agent_id + InProgress）→ 应返回
    // 创建项目（无 owner_agent_id）→ 应被过滤
    // 创建项目（有 owner_agent_id + Completed）→ 应被过滤
}
```

- [ ] **Step 4: 运行测试**

Run: `cargo test --test integration -- test_list_in_progress_with_owner`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(project): 新增 list_in_progress_with_owner 查询有 Owner Agent 的进行中项目"
```

---

### Task 4: CronTriggerConsumer project_followup action

**Files:**
- Modify: `src/consumer/scheduler.rs` (新增 project_followup 分支)
- Test: `tests/integration/`

- [ ] **Step 1: 在 CronTriggerConsumer::on_event 新增 project_followup 分支**

在 `src/consumer/scheduler.rs` 的 `on_event` 方法中，`"agent_rest"` 分支之后新增：

```rust
    match payload.action.as_str() {
        "agent_rest" => self.handle_agent_rest(&event, &payload.extra).await?,
        "project_followup" => self.handle_project_followup(&payload.extra).await?,
        _ => warn!("未知 action: {}", payload.action),
    }
```

- [ ] **Step 2: 实现 handle_project_followup 方法**

在 `CronTriggerConsumer` impl 中新增。关键点：预检查 Agent 状态（非空闲跳过），发送消息时填充 `project_id` 上下文字段，消息类型为 `ProjectFollowupNotification`，消息内容使用 `build_project_followup_content` 构建：

```rust
    async fn handle_project_followup(&self, _extra: &Value) -> Result<()> {
        use crate::service::domain::project::domain as project_domain;
        use crate::service::domain::message::domain as message_domain;
        use crate::service::domain::message::SendToAgentCommand;
        use crate::service::domain::message::builder::build_project_followup_content;
        use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
        use common::enums::message::{MessageType, MessageRole};

        let ctx = RequestContext::new_system();

        // 1. 查询所有进行中且有 Owner Agent 的项目
        let projects = project_domain()
            .project_manage()
            .list_in_progress_with_owner(ctx.clone())
            .await?;

        for project in projects {
            let owner_agent_id = match &project.po.owner_agent_id {
                Some(id) => id,
                None => continue,
            };

            // 2. 预检查：Agent 必须空闲才发送，避免无意义 nack 堆积
            let state = AgentRuntimeStateManager::global().get_state(owner_agent_id);
            if state.is_unavailable() {
                info!("Agent {} 当前 {:?}，跳过项目跟进", owner_agent_id, state);
                continue;
            }

            // 3. 构建消息内容（意图指令嵌入消息本体）
            let content = build_project_followup_content(&project.po.name);

            // 4. 发送消息（填充 project_id 上下文，MessageConsumer 会自动补充 project 信息）
            let cmd = SendToAgentCommand {
                from_id: "system",
                from_role: MessageRole::System,
                to_agent_id: owner_agent_id,
                content: &content,
                project_id: Some(&project.po.id),
                task_id: None,
                reply_to_id: None,
                attachment_ids: None,
                message_type: MessageType::ProjectFollowupNotification,
            };

            if let Err(e) = message_domain().send_to_agent(ctx.clone(), cmd).await {
                warn!("发送项目跟进消息失败: agent={}, err={}", owner_agent_id, e);
            }
        }

        Ok(())
    }
```

- [ ] **Step 3: 编写集成测试**

```rust
#[sqlx::test]
async fn test_project_followup_sends_message(pool: SqlitePool) {
    // 1. 创建一个 InProgress 项目（有 owner_agent_id）
    // 2. 确保 Owner Agent 状态为 Idle
    // 3. 构造 project_followup CronTriggerEvent
    // 4. 调用 CronTriggerConsumer::on_event
    // 5. 断言：Owner Agent 收到一条 ProjectFollowupNotification 消息
    // 6. 断言：消息的 message_type = ProjectFollowupNotification
    // 7. 断言：消息的 project_id 已填充
}
```

- [ ] **Step 4: 运行测试**

Run: `cargo test --test integration -- test_project_followup`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(scheduler): CronTriggerConsumer 新增 project_followup 定时项目跟进"
```

---

## Part 3: 实体扩展

### Task 5: ProjectPo/TaskPo execution_plan/execution_result + last_followup_at + ProjectProgressSummary

**Files:**
- Create: `migrations/20260806000001_execution_plan_result.sql`
- Modify: `src/models/project.rs` (ProjectPo 新增 execution_plan/result/last_followup_at + Project 新增 progress_summary)
- Modify: `src/models/task.rs` (TaskPo 新增 execution_plan/execution_result)
- Modify: `src/service/dal/project/` (DAO/DAL 读写新字段)
- Modify: `src/service/dal/task/` (DAO/DAL 读写新字段)
- Test: `tests/integration/`

- [ ] **Step 1: 创建数据库迁移**

Create `migrations/20260806000001_execution_plan_result.sql`:

```sql
ALTER TABLE projects ADD COLUMN execution_plan TEXT;
ALTER TABLE projects ADD COLUMN execution_result TEXT;
ALTER TABLE projects ADD COLUMN last_followup_at INTEGER;
ALTER TABLE tasks ADD COLUMN execution_plan TEXT;
ALTER TABLE tasks ADD COLUMN execution_result TEXT;
```

- [ ] **Step 2: 在 ProjectPo 新增字段**

在 `src/models/project.rs` 的 `ProjectPo` 结构体中新增：

```rust
pub struct ProjectPo {
    // ... 现有字段 ...
    pub execution_plan: Option<String>,
    pub execution_result: Option<String>,
    /// 上次项目跟进时间（定时巡检触发时更新）
    pub last_followup_at: Option<i64>,
}
```

在 `ProjectPo::new()` 中初始化为 `None`。

- [ ] **Step 3: 在 TaskPo 新增字段**

在 `src/models/task.rs` 的 `TaskPo` 结构体中新增：

```rust
pub struct TaskPo {
    // ... 现有字段 ...
    pub execution_plan: Option<String>,
    pub execution_result: Option<String>,
}
```

在 `TaskPo::new()` 中初始化为 `None`。

- [ ] **Step 4: 新增 ProjectProgressSummary 结构体**

在 `src/models/project.rs` 中新增：

```rust
/// 项目进度汇总（实时计算，不持久化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectProgressSummary {
    pub total_tasks: usize,
    pub completed: usize,
    pub in_progress: usize,
    pub pending: usize,
    pub blocked: usize,
    pub cancelled: usize,
    pub overall_percent: u32,
}

impl ProjectProgressSummary {
    pub fn from_tasks(tasks: &[crate::models::task::Task]) -> Self {
        let total = tasks.len();
        let mut completed = 0;
        let mut in_progress = 0;
        let mut pending = 0;
        let mut cancelled = 0;
        let mut total_progress: u32 = 0;

        for task in tasks {
            total_progress += task.po.progress as u32;
            match task.po.status {
                common::enums::task::TaskStatus::Completed => completed += 1,
                common::enums::task::TaskStatus::InProgress => in_progress += 1,
                common::enums::task::TaskStatus::Pending
                | common::enums::task::TaskStatus::PendingReview => pending += 1,
                common::enums::task::TaskStatus::Cancelled => cancelled += 1,
                common::enums::task::TaskStatus::Archived => {}
            }
        }

        Self {
            total_tasks: total,
            completed,
            in_progress,
            pending,
            blocked: 0,
            cancelled,
            overall_percent: if total > 0 { total_progress / total as u32 } else { 0 },
        }
    }
}
```

- [ ] **Step 5: 在 Project 业务实体中新增 progress_summary 字段**

```rust
pub struct Project {
    pub po: ProjectPo,
    pub search_match: Option<SearchMatchInfo>,
    pub stats: Option<ProjectStats>,
    pub model_call_stats: Option<ModelCallStats>,
    pub task_graph: Option<String>,
    pub artifacts: Option<Vec<ArtifactDetail>>,
    pub progress_summary: Option<ProjectProgressSummary>,
}
```

在 `Project::from_po()` 中初始化为 `None`。

- [ ] **Step 6: 更新 DAO/DAL 层读写新字段**

在 `src/service/dal/project/` 和 `src/service/dal/task/` 的 SQL 查询中，确保 `execution_plan`、`execution_result`、`last_followup_at` 字段被包含在 SELECT、INSERT、UPDATE 语句中。

- [ ] **Step 7: 编写测试**

```rust
#[sqlx::test]
async fn test_execution_plan_result_fields(pool: SqlitePool) {
    // 创建项目 → 更新 execution_plan → 查询验证
    // 创建任务 → 更新 execution_plan/execution_result → 查询验证
}

#[test]
fn test_project_progress_summary_from_tasks() {
    // 3 个任务：Completed(100) + InProgress(50) + Pending(0)
    // overall_percent = (100+50+0)/3 = 50
}
```

- [ ] **Step 8: 运行测试**

Run: `cargo test -- test_execution_plan_result && cargo test -- test_project_progress_summary`
Expected: PASS

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "feat(models): ProjectPo/TaskPo 新增 execution_plan/result/last_followup_at + ProjectProgressSummary"
```

---

### Task 6: update_task/update_project 工具扩展 + get_project 进度汇总注入

> **设计决策**：不新增 `get_project_progress` 工具。通过扩展现有 `ProjectFetchOptions` 新增 `with_progress_summary` 选项，`get_project` 接口按需返回进度汇总。Task 的新字段作为 TaskPo 字段自动随 get_task 返回。

**Files:**
- Modify: `common/src/api/task.rs` (UpdateTaskRequest 新增字段)
- Modify: `common/src/api/project.rs` (UpdateProjectRequest 新增字段 + GetProjectRequest 新增 with_progress_summary)
- Modify: `src/handlers/project/task/update_task.rs` (传递新字段)
- Modify: `src/handlers/project/update_project.rs` (传递新字段)
- Modify: `src/handlers/project/projects/get_project.rs` (传递 with_progress_summary)
- Modify: `src/service/dal/project.rs` (ProjectFetchOptions 新增 with_progress_summary)
- Modify: `src/service/domain/project/mod.rs` (TaskManage/ProjectManage trait 扩展)
- Modify: `src/service/domain/project/service.rs` (get_project 注入 progress_summary)
- Modify: `src/service/domain/project/task.rs` (update_basic 新增参数)
- Modify: `src/models/project.rs` (GetProjectResponse 新增 progress_summary)

- [ ] **Step 1: 在 UpdateTaskRequest 新增字段**

在 `common/src/api/task.rs` 的 `UpdateTaskRequest` 中新增 `execution_plan: Option<String>` 和 `execution_result: Option<String>`。

- [ ] **Step 2: 在 UpdateProjectRequest 新增字段**

在 `common/src/api/project.rs` 的 `UpdateProjectRequest` 中新增 `execution_plan: Option<String>` 和 `execution_result: Option<String>`。

- [ ] **Step 3: 扩展 TaskManage::update_basic trait 方法**

新增 `execution_plan` 和 `execution_result` 参数。同步更新 `TaskServiceImpl` 实现。

- [ ] **Step 4: 扩展 ProjectManage::update_basic trait 方法**

新增 `execution_plan` 和 `execution_result` 参数。同步更新 `ProjectServiceImpl` 实现。

- [ ] **Step 5: 更新 update_task / update_project handler**

将新参数传递给 domain 层。

- [ ] **Step 6: 在 ProjectFetchOptions 新增 with_progress_summary**

```rust
pub struct ProjectFetchOptions {
    // ... 现有字段 ...
    pub with_progress_summary: Option<bool>,
}
```

- [ ] **Step 7: 在 GetProjectRequest 新增 with_progress_summary 参数**

```rust
pub struct GetProjectRequest {
    // ... 现有字段 ...
    #[param(source = "query")]
    pub with_progress_summary: Option<bool>,
}
```

- [ ] **Step 8: 在 get_project handler 中传递 with_progress_summary**

- [ ] **Step 9: 在 ProjectServiceImpl::get_project 中注入 progress_summary**

当 `options.with_progress_summary == Some(true)` 时，查询项目关联的 tasks 并调用 `ProjectProgressSummary::from_tasks()`。

- [ ] **Step 10: 在 GetProjectResponse 中新增 progress_summary 字段**

- [ ] **Step 11: 编写测试**

```rust
#[sqlx::test]
async fn test_update_task_execution_plan(pool: SqlitePool) {
    // 创建任务 → update_task(execution_plan=...) → 查询验证
}

#[sqlx::test]
async fn test_get_project_with_progress_summary(pool: SqlitePool) {
    // 创建项目 + 3 个任务（不同状态）
    // 调用 get_project(with_progress_summary=true)
    // 验证 response.progress_summary 计数正确
}
```

- [ ] **Step 12: 运行测试**

Run: `cargo test --test integration -- test_update_task_execution_plan test_get_project_with_progress`
Expected: PASS

- [ ] **Step 13: Commit**

```bash
git add -A
git commit -m "feat(tools): 扩展 update_task/update_project + get_project 支持 progress_summary 注入"
```

---

## Part 4: 事件驱动

### Task 7: TaskStatusChangedEvent + DAL 层发布 + TaskEventConsumer

> **设计决策**：
> 1. 事件在 DAL 层发布（确保所有 status 变更都触发，无论调用方是 domain 还是 handler）
> 2. TaskEventConsumer 使用 `ConsumeMode::Async`（进度推进场景异步消费，发送方无需关注结果）
> 3. Consumer 发送的消息填充 `project_id` + `task_id` 上下文字段，消息内容使用 `build_task_dispatch_content` 构建意图指令
> 4. 消息进入 MessageConsumer 后自动补充 project/task 上下文，流程统一

**Files:**
- Create: `src/models/events/task_status.rs` (新事件)
- Modify: `src/models/events/mod.rs` (注册新事件)
- Modify: `src/service/dal/task/` (DAL 层 update_status 中发布事件)
- Create: `src/consumer/task_event_consumer.rs` (新 Async Consumer)
- Modify: `src/consumer/mod.rs` (注册 Consumer)
- Modify: `src/service/domain/message/mod.rs` (MessageDomain 新增 has_pending_message_for_agent 方法)

- [ ] **Step 1: 创建 TaskStatusChangedEvent**

创建 `src/models/events/task_status.rs`：

```rust
use serde::{Deserialize, Serialize};
use crate::pkg::aop::core::event::{Event, EventKind};
use common::enums::task::TaskStatus;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusChangedEvent {
    pub event_id: String,
    pub task_id: String,
    pub task_title: String,
    pub project_id: Option<String>,
    pub assignee_id: String,
    pub old_status: TaskStatus,
    pub new_status: TaskStatus,
    pub progress: i32,
    pub created_at: i64,
}

impl TaskStatusChangedEvent {
    pub fn new(
        task_id: &str,
        task_title: &str,
        project_id: Option<&str>,
        assignee_id: &str,
        old_status: TaskStatus,
        new_status: TaskStatus,
        progress: i32,
    ) -> Self {
        Self {
            event_id: crate::models::generate_id(),
            task_id: task_id.to_string(),
            task_title: task_title.to_string(),
            project_id: project_id.map(|s| s.to_string()),
            assignee_id: assignee_id.to_string(),
            old_status,
            new_status,
            progress,
            created_at: chrono::Utc::now().timestamp_millis(),
        }
    }
}

impl Event for TaskStatusChangedEvent {
    fn kind(&self) -> EventKind { EventKind::new("task.status_changed") }
    fn id(&self) -> &str { &self.event_id }
    fn order_key(&self) -> &str { &self.task_id }
    fn created_at(&self) -> i64 { self.created_at }
}
```

- [ ] **Step 2: 在 events/mod.rs 注册新事件**

```rust
pub mod task_status;
pub use task_status::TaskStatusChangedEvent;
```

- [ ] **Step 3: 在 DAL 层 update_status 中发布事件**

在 `src/service/dal/task/` 的 task DAO/DAL 的 `update_status` 方法中，状态变更成功后（SQL UPDATE 之后）发布事件。DAL 层需要获取 old_status（UPDATE 前查询或 SQL RETURNING），构造 `TaskStatusChangedEvent` 并 `aop::publish()`。

```rust
    // 在 update_status 实现中，UPDATE 成功后：
    let _ = crate::pkg::aop::publish(TaskStatusChangedEvent::new(
        &task_id,
        &task.title,
        task.project_id.as_deref(),
        &task.assignee_id,
        old_status,
        new_status,
        task.progress,
    )).await;
```

注意：`aop::publish()` 在 pkg 层，DAL 调用 pkg 不违反 "DAL must not call other DALs" 约束。

- [ ] **Step 4: 在 MessageDomain 新增 has_pending_message_for_agent 方法**

在 `src/service/domain/message/mod.rs` 的 `MessageDomain` trait 中新增：

```rust
    /// 检查指定 Agent 是否有 Pending 状态的指定类型消息
    async fn has_pending_message_for_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        message_type: common::enums::message::MessageType,
    ) -> Result<bool>;
```

在 `src/service/dal/message/` 中实现：查询 `messages WHERE to_role=Agent AND to_id=agent_id AND status=Pending AND message_type=?`。

- [ ] **Step 5: 创建 TaskEventConsumer**

创建 `src/consumer/task_event_consumer.rs`。关键设计：
- `ConsumeMode::Async`（异步消费，发送方不阻塞）
- 仅处理 `Completed` 状态变更（其他状态变更暂不通知）
- 发送前去重：检查是否已有同 Agent 的 Pending TaskDispatchNotification
- 消息内容使用 `build_task_dispatch_content` 构建
- 消息填充 `project_id` + `task_id` 上下文字段

```rust
use async_trait::async_trait;
use crate::pkg::aop::core::consumer::{Consumer, ConsumeMode};
use crate::pkg::aop::core::event::EventKind;
use crate::pkg::aop::Result;
use crate::models::events::TaskStatusChangedEvent;
use common::enums::task::TaskStatus;
use common::enums::message::{MessageType, MessageRole};

pub struct TaskEventConsumer;

#[async_trait]
impl Consumer for TaskEventConsumer {
    fn name(&self) -> &str { "task_event" }
    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::new("task.status_changed")]
    }
    fn consume_mode(&self) -> ConsumeMode { ConsumeMode::Async }

    async fn on_event(&self, event: serde_json::Value) -> Result<()> {
        let event: TaskStatusChangedEvent = serde_json::from_value(event)?;

        // 仅处理任务完成事件
        if event.new_status != TaskStatus::Completed {
            return Ok(());
        }

        let Some(project_id) = &event.project_id else {
            return Ok(());
        };

        let ctx = common::context::RequestContext::new_system();

        // 查询项目的 Owner Agent
        let project = crate::service::domain::project::domain()
            .project_manage()
            .get(ctx.clone(), project_id)
            .await?;

        let Some(project) = project else { return Ok(()); };
        let Some(owner_agent_id) = &project.po.owner_agent_id else { return Ok(()); };

        // 合并去重：检查是否已有同 Agent 的 Pending TaskDispatchNotification
        let message_domain = crate::service::domain::message::domain();
        let has_pending = message_domain
            .has_pending_message_for_agent(
                ctx.clone(),
                owner_agent_id,
                MessageType::TaskDispatchNotification,
            )
            .await?;

        if has_pending {
            tracing::debug!("已有 Pending 的 TaskDispatch 消息，跳过本次通知");
            return Ok(());
        }

        // 构建消息内容（意图指令嵌入消息本体）
        let content = crate::service::domain::message::builder::build_task_dispatch_content(
            &event.task_title,
            event.new_status,
            event.progress,
        );

        // 发送消息（填充 project_id + task_id，MessageConsumer 自动补充上下文）
        let cmd = crate::service::domain::message::SendToAgentCommand {
            from_id: "system",
            from_role: MessageRole::System,
            to_agent_id: owner_agent_id,
            content: &content,
            project_id: Some(project_id),
            task_id: Some(&event.task_id),
            reply_to_id: None,
            attachment_ids: None,
            message_type: MessageType::TaskDispatchNotification,
        };

        if let Err(e) = message_domain.send_to_agent(ctx, cmd).await {
            tracing::warn!("发送任务调度通知失败: {}", e);
        }

        Ok(())
    }
}
```

- [ ] **Step 6: 注册 TaskEventConsumer**

在 `src/consumer/mod.rs` 的 `init()` 中注册：

```rust
    registry.register(Arc::new(TaskEventConsumer));
```

- [ ] **Step 7: 编写测试**

```rust
#[sqlx::test]
async fn test_task_status_changed_event_published(pool: SqlitePool) {
    // 创建任务 → update_status(InProgress → Completed)
    // 验证 TaskStatusChangedEvent 被发布
}

#[sqlx::test]
async fn test_task_event_consumer_sends_notification(pool: SqlitePool) {
    // 手动触发 TaskStatusChangedEvent
    // 验证 Owner Agent 收到 TaskDispatchNotification 消息
    // 验证消息的 project_id + task_id 已填充
}
```

- [ ] **Step 8: 运行测试**

Run: `cargo test --test integration -- test_task_status_changed test_task_event_consumer`
Expected: PASS

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "feat(events): TaskStatusChangedEvent DAL 层发布 + TaskEventConsumer 异步补偿通知"
```

---

## Part 5: 收尾

### Task 8: 更新项目管理技能文档

**Files:**
- Modify: 项目管理技能的 prompt 文件（搜索 skills/ 或 seed 中的技能定义）

- [ ] **Step 1: 定位项目管理技能文档**

搜索 `skills/` 目录或 seed 数据中项目管理技能的 prompt 内容。关键词：`project_management`、`项目管理`、`list_tasks`、`update_task`。

- [ ] **Step 2: 在技能 prompt 中新增任务执行规范**

```markdown
## 任务执行规范

1. 开始执行任务前，调用 update_task 写入 execution_plan：
   - 简述执行步骤和预期产出
   - 如有依赖任务，说明等待策略

2. 任务完成后，调用 update_task 写入 execution_result：
   - 总结实际完成情况
   - 列出产出物（artifact）链接
   - 如未完成，说明阻塞原因和下一步建议

3. 更新 task progress 和 status：
   - 开始执行 → status=InProgress, progress=10
   - 有阶段性产出 → progress 按实际更新
   - 完成 → status=Completed, progress=100

## 项目执行规范（Owner Agent 专属）

1. 项目启动时，调用 update_project 写入 execution_plan：
   - 划分项目阶段（Phase 1/2/3...）
   - 每个阶段的目标和关键任务

2. 项目跟进时，调用 get_project(with_progress_summary=true) 获取实时进度：
   - 进度由系统根据所有 Task 状态自动汇总

3. 项目完成时，调用 update_project 写入 execution_result

## 系统通知响应

当收到「任务调度通知」或「项目进度定期检查」消息时，按照消息中的行动指令执行，不要当作普通对话处理。
```

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "docs(skills): 更新项目管理技能 prompt 补充任务执行和系统通知响应规范"
```

---

### Task 9: Seed 系统级默认定时任务（agent_rest + project_followup）

> **设计决策**：如果用户已有同类型（同 action）的触发器则不重复添加。系统仅提供初始化默认值，用户可自行修改间隔或禁用。

**Files:**
- Modify: `src/service/domain/system/` (新增 ensure_system_cron_triggers 函数)
- Modify: `src/service/init.rs` 或 `src/main.rs` (启动时调用)
- Test: `tests/integration/`

- [ ] **Step 1: 新增 ensure_system_cron_triggers 函数**

在 `src/service/domain/system/` 中新增函数。按 action 去重（而非 ID 去重），检查已有 trigger 的 payload 是否包含相同 action：

```rust
/// 确保系统级默认定时任务存在（幂等，已有同类型则跳过）
pub async fn ensure_system_cron_triggers(ctx: &RequestContext) -> Result<()> {
    let cron_manager = domain().cron_manager();

    // 获取所有现有 trigger
    let existing = cron_manager.list_all(ctx).await?;
    let has_agent_rest = existing.iter().any(|t| t.payload.contains("\"agent_rest\""));
    let has_project_followup = existing.iter().any(|t| t.payload.contains("\"project_followup\""));

    // 1. agent_rest：默认每 4 小时执行一次睡眠沉淀
    if !has_agent_rest {
        let mut trigger = CronTriggerPo::new(
            crate::models::generate_id(),
            "系统默认-Agent 睡眠沉淀".into(),
            TriggerType::Interval,
            chrono::Utc::now().timestamp_millis() + 4 * 3600_000,
            "system".into(),
        );
        trigger.interval_seconds = Some(4 * 3600);
        trigger.payload = r#"{"action":"agent_rest","extra":{"settle_limit":10}}"#.into();
        trigger.is_enabled = 1;
        cron_manager.create_trigger(ctx, trigger).await?;
        info!("已创建系统级定时任务: agent_rest");
    }

    // 2. project_followup：默认每 1 小时执行一次项目进度巡检
    if !has_project_followup {
        let mut trigger = CronTriggerPo::new(
            crate::models::generate_id(),
            "系统默认-项目进度巡检".into(),
            TriggerType::Interval,
            chrono::Utc::now().timestamp_millis() + 3600_000,
            "system".into(),
        );
        trigger.interval_seconds = Some(3600);
        trigger.payload = r#"{"action":"project_followup","extra":{}}"#.into();
        trigger.is_enabled = 1;
        cron_manager.create_trigger(ctx, trigger).await?;
        info!("已创建系统级定时任务: project_followup");
    }

    Ok(())
}
```

- [ ] **Step 2: 在系统启动时调用**

在 `service::init()` 或 `consumer::init()` 末尾调用：

```rust
    let ctx = RequestContext::new_system();
    if let Err(e) = system::ensure_system_cron_triggers(&ctx).await {
        warn!("创建系统级定时任务失败: {}", e);
    }
```

- [ ] **Step 3: 编写测试**

```rust
#[sqlx::test]
async fn test_system_cron_triggers_created(pool: SqlitePool) {
    let app = init_full_test_env(pool).await;
    // 验证两个系统级定时任务都已创建
    // 验证 payload 包含正确的 action
}

#[sqlx::test]
async fn test_system_cron_triggers_no_duplicate(pool: SqlitePool) {
    // 先手动创建一个 agent_rest trigger
    // 调用 ensure_system_cron_triggers
    // 验证不会重复创建 agent_rest，但 project_followup 仍会创建
}
```

- [ ] **Step 4: 运行测试**

Run: `cargo test --test integration -- test_system_cron_triggers`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(seed): 系统初始化自动创建 agent_rest + project_followup 默认定时任务（按 action 去重）"
```

---

## 验收清单

- [ ] `cargo check --all` 无编译错误
- [ ] `cargo clippy --all -- -D warnings` 无警告
- [ ] `cargo fmt --all -- --check` 格式检查通过
- [ ] `cargo test --all` 所有测试通过
- [ ] `cargo test --test integration` 集成测试通过

## 实施顺序总结

| 顺序 | Task | 内容 | 依赖 |
|------|------|------|------|
| 1 | Task 1 | MessageType 扩展 + SendToAgentCommand.message_type + 消息内容构建器 | 无 |
| 2 | Task 2 | load_and_settle 并发修复 | 无 |
| 3 | Task 3 | list_in_progress_with_owner | 无 |
| 4 | Task 4 | CronTriggerConsumer project_followup | Task 1, 3 |
| 5 | Task 5 | ProjectPo/TaskPo 字段扩展 + ProgressSummary + last_followup_at | 无 |
| 6 | Task 6 | update_task/project 扩展 + get_project 进度注入 | Task 5 |
| 7 | Task 7 | TaskStatusChangedEvent DAL 层发布 + TaskEventConsumer | Task 1, 5 |
| 8 | Task 8 | 技能文档更新 | Task 1-7 |
| 9 | Task 9 | Seed 系统级定时任务（agent_rest + project_followup） | Task 4 |

## 核心架构对比（修改前 → 修改后）

| 维度 | 原方案 | 改进后 |
|------|--------|--------|
| 意图传递 | AwakenIntent 枚举 + ThinkingOptions.intent + PromptBuilder 注入 | 消息内容本身携带意图指令文本 |
| Prompt 注入 | determine_intent 路由 + builder.intent_context | 无需特殊注入，Agent 读取消息内容即可 |
| 上下文补充 | 需在 determine_intent 中额外处理 | 复用现有 MessageConsumer 的 project/task 自动补充 |
| 事件发布层 | Domain 层 transition_status | DAL 层 update_status（确保所有变更都触发） |
| Consumer 模式 | Sync | Async（进度推进异步消费，发送方不阻塞） |
| Seed 去重 | 按 trigger ID | 按 action 类型（兼容用户自建） |
