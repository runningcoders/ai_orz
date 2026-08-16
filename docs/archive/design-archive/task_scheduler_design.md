# 定时任务系统设计文档

> 📦 归档标记（2026-08-16）：归档冻结。保留原因：task_scheduler_design 设计文档归档冻结，设计决策已沉淀至 wiki 长文。生效方案：见源码和 wiki 长文。

> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 整体分层架构
> - [task_design.md](./task_design.md) — 任务实体基础（定时触发创建的目标实例）
> - [agent_loop_engine_design.md](./agent_loop_engine_design.md) — Agent 自主循环驱动（CronTrigger 作为补偿链路）
> - [runtime_design.md](./runtime_design.md) — Agent 唤醒/休息状态机（Resting 期间 agent_rest 不让唤醒）
> - [consumer_architecture.md](./consumer_architecture.md) — AOP SchedulerConsumer 注册顺序和消费模式
> - 【② Plan 落地】[唤醒上下文与睡眠约束.md](../plan/唤醒上下文与睡眠约束.md) — agent_rest 每天 4 点触发 + Resting 期间 pending_message 不唤醒
> - 【② Plan 落地】[统计图表Phase1基础设施与时序图展示重构.md](../plan/统计图表Phase1基础设施与时序图展示重构.md) — stats_collect 每 5 分钟 cron → RuntimeStatsCollector flush 到 DuckDB
> - 【② Plan 落地】[通用后台任务模块与Seed异步化重构.md](../plan/通用后台任务模块与Seed异步化重构.md) — 自定义 cron CRUD + 5 字段表达式校验 + <1 分钟拦截
> - 【② Plan 落地】[AOP生产消费事件中心重构.md](../plan/AOP生产消费事件中心重构.md) — SchedulerTriggerFiredEvent 事件 → Consumer 分发处理链路
> - 【③ Wiki 百科】[定时任务管理.md](docs/wiki/zh/content/项目概述/核心功能特性/系统管理功能/定时任务管理.md) — SystemTriggers 页面：2 条系统默认 readonly + 用户 CRUD + 立即触发按钮
> - 【③ Wiki 百科】[定时任务调度.md](docs/wiki/zh/content/功能模块/系统管理/定时任务调度.md) — 调度总览：5 字段 cron + 轮询 + AOP 事件分发 + SchedulerConsumer 处理
> - 【④ RAG 知识卡】[Cron 任务调度与系统启动顺序](docs/wiki/knowledge/zh/Cron%20%E4%BB%BB%E5%8A%A1%E8%B0%83%E5%BA%A6%E4%B8%8E%E7%B3%BB%E7%BB%9F%E5%90%AF%E5%8A%A8%E9%A1%BA%E5%BA%8F%EF%BC%9A5%E5%AD%97%E6%AE%B5cron%E8%A7%A3%E6%9E%90%20+%20ensure_system_cron_triggers%202%E6%9D%A1%E6%B3%A8%E5%85%A5%20+%20CronTriggerConsumer%20%E4%BA%8B%E4%BB%B6%20+%20%E4%B8%A4%E9%98%B6%E6%AE%B5init/aop%E4%B8%A5%E6%A0%BC%E5%88%86%E7%A6%BB/Cron%20%E4%BB%BB%E5%8A%A1%E8%B0%83%E5%BA%A6%E4%B8%8E%E7%B3%BB%E7%BB%9F%E5%90%AF%E5%8A%A8%E9%A1%BA%E5%BA%8F%EF%BC%9A5%E5%AD%97%E6%AE%B5cron%E8%A7%A3%E6%9E%90%20+%20ensure_system_cron_triggers%202%E6%9D%A1%E6%B3%A8%E5%85%A5%20+%20CronTriggerConsumer%20%E4%BA%8B%E4%BB%B6%20+%20%E4%B8%A4%E9%98%B6%E6%AE%B5init/aop%E4%B8%A5%E6%A0%BC%E5%88%86%E7%A6%BB.md) — §启动 6 步顺序 §ensure_system_cron_triggers 2 条默认 §8 条红线

## 📌 设计目标

在现有任务系统基础上扩展定时触发能力，支持：

1. **多种触发方式**：一次性定时、周期性（Cron 表达式）、固定间隔
2. **任务调度**：定时扫描到期任务，自动触发执行
3. **状态管理**：记录下次执行时间、执行历史、错误信息
4. **可暂停/恢复**：支持临时暂停定时任务，之后可恢复
5. **分层架构**：严格遵循 DAO → DAL → Domain 分层设计
6. **复用现有任务**：定时触发的本质是创建任务实例，复用现有任务执行逻辑

---

## 🏗️ 整体架构

> 💡 **架构演进**：调度器已通过 AOP 事件中心重构。`CronTriggerProducer` 作为 AOP Producer 注册到 Registry，每 60 秒 poll 一次到期触发器并 publish `CronTriggerEvent`；`CronTriggerConsumer` 订阅 `cron_trigger` topic，按 `payload.action` 分发到对应 Domain 处理。已实现的 Action 模板包括 `agent_rest`（触发 Agent 休息与记忆沉淀），可通过扩展 Consumer 的 action 分支支持更多定时业务场景。

```
┌─────────────────────────────────────────────────────────────┐
│              CronTriggerProducer（AOP Producer）              │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  定时扫描触发器（poll_interval_secs = 60s）            │  │
│  │    SELECT * FROM cron_triggers WHERE next_run_at <= now  │  │
│  └───────────────────────────────────────────────────────┘  │
└────────────────────────────────────────┬─────────────────────┘
                                         │ publish(CronTriggerEvent)
                                         ▼
                    ┌─────────────────────────────────┐
                    │   AOP 事件中心（Registry）        │
                    │   topic = "cron_trigger"         │
                    └───────────────┬─────────────────┘
                                    │
                                    ▼
                    ┌─────────────────────────────────┐
                    │  CronTriggerConsumer（Consumer）  │
                    │  按 payload.action 分发到 Domain   │
                    │  - agent_rest：触发记忆沉淀        │
                    │  - 自定义 action：扩展处理          │
                    └───────────────┬─────────────────┘
                                    │
                                    ▼
                    ┌─────────────────────────────────┐
                    │   更新触发器：next_run_at ++     │
                    │   last_run_at = now              │
                    │   （Producer 端 mark_executed）   │
                    └─────────────────────────────────┘
```

---

## 🗄️ 数据库设计

### 1. cron_triggers 表（新增定时触发器）

```sql
CREATE TABLE IF NOT EXISTS cron_triggers (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL,
    user_id TEXT NOT NULL,                    -- 创建者/归属用户
    trigger_type TEXT NOT NULL,               -- 触发类型：once / cron / interval
    name TEXT NOT NULL,                       -- 触发器名称
    description TEXT NOT NULL DEFAULT '',     -- 触发器描述
    
    -- 任务模板（触发时使用这些字段创建任务）
    task_title TEXT NOT NULL,                 -- 任务标题模板
    task_description TEXT NOT NULL DEFAULT '', -- 任务描述模板
    task_assignee_type INTEGER NOT NULL,      -- 任务分配对象类型
    task_assignee_id TEXT NOT NULL,           -- 任务分配对象 ID
    task_project_id TEXT,                     -- 关联项目 ID
    task_priority INTEGER NOT NULL DEFAULT 0, -- 任务优先级
    task_tags TEXT NOT NULL DEFAULT '[]',     -- 任务标签(JSON 数组)
    
    -- 定时配置
    cron_expression TEXT,                     -- Cron 表达式（cron 类型使用）
    interval_seconds INTEGER,                 -- 间隔秒数（interval 类型使用）
    one_time_run_at INTEGER,                  -- 一次性执行时间戳（once 类型使用）
    
    -- 执行控制
    start_at INTEGER,                         -- 生效开始时间（可选）
    end_at INTEGER,                           -- 生效结束时间（可选，NULL 表示永久）
    max_runs INTEGER,                         -- 最大执行次数（NULL 表示不限）
    remaining_runs INTEGER,                    -- 剩余执行次数
    
    -- 执行状态
    is_enabled INTEGER NOT NULL DEFAULT 1,    -- 是否启用：0=禁用，1=启用
    last_run_at INTEGER,                       -- 上次执行时间
    next_run_at INTEGER NOT NULL,              -- 下次执行时间（调度器扫描的关键字段）
    total_runs INTEGER NOT NULL DEFAULT 0,     -- 累计执行次数
    last_error TEXT,                           -- 上次执行错误信息
    
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    created_by TEXT NOT NULL,
    modified_by TEXT NOT NULL
);

-- 索引：调度器核心查询
CREATE INDEX idx_cron_triggers_next_run ON cron_triggers(is_enabled, next_run_at);
CREATE INDEX idx_cron_triggers_org_id ON cron_triggers(org_id);
CREATE INDEX idx_cron_triggers_user_id ON cron_triggers(user_id);
CREATE INDEX idx_cron_triggers_trigger_type ON cron_triggers(trigger_type);
```

> 💡 **实现演进**：实际迁移脚本（`migrations/20260711000000_cron_triggers.sql`）已精简，移除了原设计的任务模板字段（`task_*`、`org_id`、`user_id`、`start_at`/`end_at`/`max_runs`/`remaining_runs`/`total_runs`/`last_error` 等），改为单一的 `payload TEXT NOT NULL DEFAULT '{}'` 字段承载 `action` + `extra`（JSON）。触发器不再直接创建任务实例，而是通过 `payload.action` 分发到对应 Domain 处理（如 `agent_rest` 触发记忆沉淀）。`trigger_type` 字段类型为 `INTEGER`（配合 `TriggerType` 枚举的 `#[repr(i32)]`），而非上文的 `TEXT`。

### 2. 字段说明

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `id` | TEXT | ✅ | 触发器 ID（UUID v7） |
| `org_id` | TEXT | ✅ | 组织 ID，多租户隔离 |
| `user_id` | TEXT | ✅ | 创建/归属用户 ID |
| `trigger_type` | TEXT | ✅ | 触发类型：once / cron / interval |
| `name` | TEXT | ✅ | 触发器名称 |
| `task_*` | TEXT/INTEGER | ✅ | 任务模板，触发时用于创建任务 |
| `cron_expression` | TEXT | ❌ | Cron 表达式（cron 类型） |
| `interval_seconds` | INTEGER | ❌ | 间隔秒数（interval 类型） |
| `one_time_run_at` | INTEGER | ❌ | 一次性执行时间（once 类型） |
| `start_at` | INTEGER | ❌ | 生效开始时间 |
| `end_at` | INTEGER | ❌ | 生效结束时间 |
| `max_runs` | INTEGER | ❌ | 最大执行次数 |
| `remaining_runs` | INTEGER | ❌ | 剩余执行次数 |
| `is_enabled` | INTEGER | ✅ | 是否启用 |
| `last_run_at` | INTEGER | ❌ | 上次执行时间 |
| `next_run_at` | INTEGER | ✅ | 下次执行时间（核心调度字段） |
| `total_runs` | INTEGER | ✅ | 累计执行次数 |
| `last_error` | TEXT | ❌ | 上次执行错误 |

---

## 📐 枚举定义

### TriggerType 触发类型

```rust
// common/src/enums/cron_trigger.rs
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum TriggerType {
    Once = 0,       // 一次性定时
    Cron = 1,       // Cron 表达式
    Interval = 2,   // 固定间隔
}

impl Default for TriggerType {
    fn default() -> Self { Self::Cron }
}

impl From<i32> for TriggerType { /* ... */ }
impl From<i64> for TriggerType { /* ... */ }  // 适配 sqlx 类型推断

impl TriggerType {
    pub fn to_i32(&self) -> i32 { *self as i32 }
}
```

> 💡 **注意**：实际实现使用 `INTEGER` 存储（而非文档早期版本的 `TEXT`），遵循 [AGENTS.md 4.3 枚举类型安全](../AGENTS.md) 规范，所有数据库枚举字段必须使用 Rust 枚举 + `#[repr(i32)]` + `#[derive(sqlx::Type)]`，并实现 `From<i64>` 适配 sqlx 类型推断。

---

## 🧩 分层设计

### 1. DAO 层（数据访问）

```rust
// src/service/dao/cron_trigger/mod.rs
#[async_trait]
pub trait CronTriggerDao {
    // CRUD
    async fn create(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()>;
    async fn update(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()>;
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;
    async fn get_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<CronTriggerPo>>;
    
    // 通用查询
    async fn list(&self, ctx: RequestContext, query: CronTriggerQuery) -> Result<Vec<CronTriggerPo>>;
    
    // 调度器查询：获取所有到期需要执行的触发器
    // SELECT * FROM cron_triggers 
    // WHERE is_enabled = 1 AND next_run_at <= ?
    // ORDER BY next_run_at ASC
    async fn list_due(&self, ctx: RequestContext, now: i64, limit: i32) -> Result<Vec<CronTriggerPo>>;
    
    // 更新下次执行时间（含 last_run_at）
    async fn update_next_run_at(
        &self,
        ctx: RequestContext,
        id: &str,
        next_run_at: i64,
        last_run_at: i64,
    ) -> Result<()>;
    
    // 启用/禁用（软删除：is_enabled = 0）
    async fn set_enabled(&self, ctx: RequestContext, id: &str, is_enabled: bool) -> Result<()>;
}
```

### 2. DAL 层（业务组合）

```rust
// src/service/dal/cron_trigger.rs
#[async_trait]
pub trait CronTriggerDal {
    // 创建触发器（一次性 / Cron / 间隔由 trigger_type 字段决定）
    async fn create(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()>;
    
    // 根据 ID 获取
    async fn get_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<CronTriggerPo>>;
    
    // 列表查询
    async fn list(&self, ctx: RequestContext, query: CronTriggerQuery) -> Result<Vec<CronTriggerPo>>;
    
    // 更新
    async fn update(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()>;
    
    // 暂停/恢复（软删除：is_enabled = 0）
    async fn pause(&self, ctx: RequestContext, id: &str) -> Result<()>;
    async fn resume(&self, ctx: RequestContext, id: &str) -> Result<()>;
    
    // 获取到期触发器
    async fn list_due(&self, ctx: RequestContext, now: i64, limit: i32) -> Result<Vec<CronTriggerPo>>;
    
    // 标记执行成功，更新 next_run_at / last_run_at
    async fn mark_executed(&self, ctx: RequestContext, id: &str, executed_at: i64) -> Result<()>;
}
```

### 3. Domain 层（核心业务）

> 💡 **架构归属**：触发器管理归属于 `SystemDomain`（`src/service/domain/system/`），通过 `cron_manager()` 子能力对外暴露。这与文档原设计的独立 `TaskTriggerDomain` 不同——触发器属于系统领域基础设施，与 AOP 监控同属 SystemDomain。

```rust
// src/service/domain/system/mod.rs
// SystemDomain 的 cron_manager 子能力
pub trait CronManager: Send + Sync {
    // 创建触发器
    async fn create_trigger(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()>;
    
    // 获取触发器
    async fn get_trigger(&self, ctx: RequestContext, id: &str) -> Result<Option<CronTriggerPo>>;
    
    // 列出触发器
    async fn list_triggers(&self, ctx: RequestContext, query: CronTriggerQuery) -> Result<Vec<CronTriggerPo>>;
    
    // 更新触发器
    async fn update_trigger(&self, ctx: RequestContext, trigger: &CronTriggerPo) -> Result<()>;
    
    // 暂停触发器
    async fn pause_trigger(&self, ctx: RequestContext, id: &str) -> Result<()>;
    
    // 恢复触发器
    async fn resume_trigger(&self, ctx: RequestContext, id: &str) -> Result<()>;
    
    // 获取到期触发器（供 Producer 调用）
    async fn list_due_triggers(&self, ctx: RequestContext, now: i64, limit: i32) -> Result<Vec<CronTriggerPo>>;
    
    // 标记触发器已执行（Producer publish 后调用）
    async fn mark_trigger_executed(&self, ctx: RequestContext, id: &str, executed_at: i64) -> Result<()>;
}

// SystemDomain 暴露 cron_manager 能力
impl SystemDomain {
    pub fn cron_manager(&self) -> Arc<dyn CronManager> { ... }
}
```

---

## ⏰ 调度器实现

### AOP Producer 模式

> 💡 **架构演进**：原设计的独立 `TaskScheduler` 已被 AOP 事件中心的 Producer/Consumer 模式取代。`CronTriggerProducer` 实现 `Producer` trait，注册到 Registry 后由框架按 `poll_interval_secs()` 周期性调用 `poll()`。Producer 只负责扫描到期触发器并 publish 事件，业务处理由 Consumer 完成，二者完全解耦。

```rust
// src/producer/cron_trigger.rs
pub struct CronTriggerProducer {
    registry: RwLock<Option<Arc<Registry>>>,
}

#[async_trait]
impl Producer for CronTriggerProducer {
    fn name(&self) -> &str { "cron_trigger" }

    async fn register(&self, registry: Arc<Registry>) -> Result<()> {
        let mut reg = self.registry.write().unwrap();
        *reg = Some(registry);
        Ok(())
    }

    fn poll_interval_secs(&self) -> u64 { 60 }

    async fn poll(&self) -> Result<()> {
        let registry = self.registry.read().unwrap().clone();
        let Some(registry) = registry else {
            return Err(err!(Internal, "registry not registered"));
        };

        let ctx = RequestContext::new(None, None);
        let now = current_timestamp();

        // 1. 查询到期触发器
        let triggers = system::domain()
            .cron_manager()
            .list_due_triggers(ctx.clone(), now, 100)
            .await?;

        if triggers.is_empty() {
            return Ok(());
        }

        log_debug!("cron producer found {} due triggers", triggers.len());

        // 2. publish 事件 + 标记执行
        for trigger in &triggers {
            let event = CronTriggerEvent {
                event_id: format!("{}-{}", trigger.id, now),
                trigger_id: trigger.id.clone(),
                trigger_name: trigger.name.clone(),
                payload: trigger.payload.clone(),
                created_at: current_timestamp(),
            };
            registry.publish(event).await;

            system::domain()
                .cron_manager()
                .mark_trigger_executed(ctx.clone(), &trigger.id, now)
                .await?;
        }

        log_info!("cron producer published {} trigger events", triggers.len());
        Ok(())
    }
}
```

### Consumer 处理（按 action 分发）

```rust
// src/consumer/scheduler.rs
pub struct CronTriggerConsumer;

impl Consumer for CronTriggerConsumer {
    async fn handle(&self, event: CronTriggerEvent) -> Result<()> {
        log_info!(
            "cron trigger fired: {} (trigger_id: {}, action: {})",
            event.trigger_name, event.trigger_id, payload.action
        );

        match payload.action.as_str() {
            // agent_rest：触发 Agent 休息与记忆沉淀（定时触发记忆沉淀）
            "agent_rest" => {
                self.handle_agent_rest(&event, &payload.extra).await?;
            }
            other => {
                log_warn!("unknown action '{}' for trigger {} (id: {})",
                    payload.action, event.trigger_name, event.trigger_id);
            }
        }
        Ok(())
    }
}
```

### 注册到 AOP 事件中心

```rust
// src/producer/mod.rs
pub fn init(registry: Arc<Registry>) {
    registry
        .register_producer(Arc::new(cron_trigger::CronTriggerProducer::new()))
        // ... 其他 producer
        ;
}
```

> 💡 **关键事件流**：`CronTriggerProducer.poll()` → `Registry.publish(CronTriggerEvent)` → `CronTriggerConsumer.handle()` → 按 `action` 分发到 Domain（如 `agent_rest` 调用 RuntimeDomain 执行 Agent 休息与记忆沉淀）。

---

## 🎯 Cron 表达式支持

### 使用 Cron 库

推荐使用 `cron` crate 解析和计算下次执行时间：

```rust
use cron::Schedule;
use chrono::{Utc, TimeZone};

fn calculate_cron_next(cron_expr: &str, from_time: i64) -> Option<i64> {
    let schedule = cron_expr.parse::<Schedule>().ok()?;
    let from_datetime = Utc.timestamp_millis_opt(from_time).single()?;
    
    schedule
        .upcoming(Utc)
        .next()
        .map(|next| next.timestamp_millis())
}
```

### 常用 Cron 表达式示例

| 表达式 | 含义 |
|--------|------|
| `0 0 * * * *` | 每分钟执行一次 |
| `0 0 9 * * *` | 每天早上 9 点执行 |
| `0 0 9 * * MON-FRI` | 工作日早上 9 点执行 |
| `0 0 9 1 * *` | 每月 1 号早上 9 点执行 |

---

## 📋 实现任务清单（落地完成态）

> ✅ 全部已实现（2026-07-12 落地，2026-07-15 前端体验优化）；落地现状以 [Cron 触发器 wiki 长文](docs/wiki/zh/content/功能模块/调度与触发/Cron触发器与定时任务调度.md) 与 [CronManager/RAG 知识卡](docs/wiki/knowledge/zh/调度与触发体系/Cron 触发器与 CronManager 任务调度.md) 为准。

- 创建 `common/src/enums/` 下 TriggerType 枚举
- 创建 `src/models/cron_trigger.rs` PO 结构体（含 `payload` 字段承载 action + extra）
- 创建数据库迁移脚本
- 实现 `CronTriggerDao` SQLite 实现（`src/service/dao/cron_trigger/sqlite.rs`）
- 实现 `CronTriggerDal`（`src/service/dal/cron_trigger.rs`）
- 实现 `CronManager` 子能力并归属 `SystemDomain`（`src/service/domain/system/`）
- 实现 `CronTriggerProducer`（AOP Producer 模式，`src/producer/cron_trigger.rs`）
- 实现 `CronTriggerConsumer`（按 `payload.action` 分发，`src/consumer/scheduler.rs`）
- 注册 Producer 到 AOP 事件中心（`src/producer/mod.rs`）
- 实现 `agent_rest` Action 模板（触发 Agent 休息与记忆沉淀，定时触发记忆沉淀）
- 前端定时触发器页面（7 列展示 + Action 模板化 + Cron 预设按钮 + 编辑复用创建弹窗）
- 编写单元测试

---

## 💡 设计思考

### 为什么不直接在 tasks 表加字段？

1. **职责分离**：任务是「执行实例」，触发器是「调度规则」，两者概念不同
2. **一对多关系**：一个触发器可以生成多个任务实例
3. **生命周期独立**：触发器可以长期存在，任务执行完就结束
4. **查询效率**：调度器只需要扫描触发器表（数据量小），不需要扫描任务表

### 为什么每分钟扫描一次？

1. **平衡精度和性能**：对于绝大多数场景，分钟级精度足够
2. **降低数据库压力**：每分钟一次查询，压力极小
3. **实现简单**：不需要复杂的时间轮算法
4. **可配置**：未来可以根据需要调整扫描间隔

### 执行失败怎么办？

1. **默认不重试**：定时任务本次执行失败就等下一次
2. **错误记录**：`last_error` 字段记录错误信息，用户可以查看
3. **手动触发**：用户可以手动点击「立即执行」重试
4. **告警通知**：未来可以集成消息渠道，执行失败时通知用户