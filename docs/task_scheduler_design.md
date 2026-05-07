# 定时任务系统设计文档

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

```
┌─────────────────────────────────────────────────────────────┐
│                     Scheduler Worker                          │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  定时扫描触发器（每分钟执行一次）                      │  │
│  │    SELECT * FROM task_triggers WHERE next_run_at <= now  │  │
│  └───────────────────────────────────────────────────────┘  │
└────────────────────────────────────────┬─────────────────────┘
                                         │
                                         ▼
                    ┌─────────────────────────────────┐
                    │   创建任务实例到 tasks 表        │
                    │   (复用现有任务执行逻辑)         │
                    └───────────────┬─────────────────┘
                                    │
                                    ▼
                    ┌─────────────────────────────────┐
                    │   更新触发器：next_run_at ++     │
                    │   last_run_at = now              │
                    └─────────────────────────────────┘
```

---

## 🗄️ 数据库设计

### 1. task_triggers 表（新增定时触发器）

```sql
CREATE TABLE IF NOT EXISTS task_triggers (
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
CREATE INDEX idx_task_triggers_next_run ON task_triggers(is_enabled, next_run_at);
CREATE INDEX idx_task_triggers_org_id ON task_triggers(org_id);
CREATE INDEX idx_task_triggers_user_id ON task_triggers(user_id);
CREATE INDEX idx_task_triggers_trigger_type ON task_triggers(trigger_type);
```

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
// common/src/enums/task_trigger.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum TriggerType {
    Once,       // 一次性定时
    Cron,       // Cron 表达式
    Interval,   // 固定间隔
}

impl TriggerType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TriggerType::Once => "once",
            TriggerType::Cron => "cron",
            TriggerType::Interval => "interval",
        }
    }
}
```

---

## 🧩 分层设计

### 1. DAO 层（数据访问）

```rust
// src/dao/task_trigger/mod.rs
#[async_trait]
pub trait TaskTriggerDao {
    // CRUD
    async fn create(&self, ctx: RequestContext, trigger: &TaskTriggerPo) -> Result<TaskTriggerPo, AppError>;
    async fn update(&self, ctx: RequestContext, trigger: &TaskTriggerPo) -> Result<TaskTriggerPo, AppError>;
    async fn delete(&self, ctx: RequestContext, trigger_id: &str) -> Result<(), AppError>;
    async fn get_by_id(&self, ctx: RequestContext, trigger_id: &str) -> Result<Option<TaskTriggerPo>, AppError>;
    
    // 查询用户的所有触发器
    async fn list_by_user(&self, ctx: RequestContext, user_id: &str) -> Result<Vec<TaskTriggerPo>, AppError>;
    
    // 调度器查询：获取所有到期需要执行的触发器
    // SELECT * FROM task_triggers 
    // WHERE is_enabled = 1 AND next_run_at <= ?
    // ORDER BY next_run_at ASC
    async fn list_due_triggers(&self, ctx: RequestContext, now: i64) -> Result<Vec<TaskTriggerPo>, AppError>;
    
    // 标记执行成功，计算下次执行时间
    async fn mark_executed(
        &self,
        ctx: RequestContext,
        trigger_id: &str,
        last_run_at: i64,
        next_run_at: Option<i64>,
    ) -> Result<(), AppError>;
    
    // 标记执行失败
    async fn mark_failed(&self, ctx: RequestContext, trigger_id: &str, error: &str) -> Result<(), AppError>;
    
    // 启用/禁用
    async fn set_enabled(&self, ctx: RequestContext, trigger_id: &str, is_enabled: bool) -> Result<(), AppError>;
}
```

### 2. DAL 层（业务组合）

```rust
// src/dal/task_trigger.rs
#[async_trait]
pub trait TaskTriggerDal {
    // 创建一次性定时任务
    async fn create_once_trigger(&self, ctx: RequestContext, req: CreateOnceTriggerRequest) -> Result<TaskTriggerPo, AppError>;
    
    // 创建 Cron 定时任务
    async fn create_cron_trigger(&self, ctx: RequestContext, req: CreateCronTriggerRequest) -> Result<TaskTriggerPo, AppError>;
    
    // 创建间隔定时任务
    async fn create_interval_trigger(&self, ctx: RequestContext, req: CreateIntervalTriggerRequest) -> Result<TaskTriggerPo, AppError>;
    
    // 计算触发器的下次执行时间
    fn calculate_next_run(&self, trigger: &TaskTriggerPo, from_time: i64) -> Option<i64>;
    
    // 执行触发器：创建任务 + 更新执行状态
    async fn execute_trigger(&self, ctx: RequestContext, trigger_id: &str) -> Result<TaskPo, AppError>;
    
    // 批量执行所有到期触发器
    async fn execute_due_triggers(&self, ctx: RequestContext) -> Result<Vec<TaskPo>, AppError>;
}
```

### 3. Domain 层（核心业务）

```rust
// src/domain/task_trigger/mod.rs
pub struct TaskTriggerDomain {
    trigger_dal: Arc<dyn TaskTriggerDal>,
    task_dal: Arc<dyn TaskDal>,
}

impl TaskTriggerDomain {
    // 创建触发器
    pub async fn create_trigger(&self, ctx: RequestContext, req: CreateTriggerRequest) -> Result<TaskTriggerDto, AppError>;
    
    // 暂停触发器
    pub async fn pause_trigger(&self, ctx: RequestContext, trigger_id: &str) -> Result<(), AppError>;
    
    // 恢复触发器
    pub async fn resume_trigger(&self, ctx: RequestContext, trigger_id: &str) -> Result<(), AppError>;
    
    // 删除触发器
    pub async fn delete_trigger(&self, ctx: RequestContext, trigger_id: &str) -> Result<(), AppError>;
    
    // 列出用户所有触发器
    pub async fn list_user_triggers(&self, ctx: RequestContext, user_id: &str) -> Result<Vec<TaskTriggerDto>, AppError>;
    
    // 手动立即触发
    pub async fn trigger_now(&self, ctx: RequestContext, trigger_id: &str) -> Result<TaskDto, AppError>;
}
```

---

## ⏰ 调度器实现

### 后台定时扫描

```rust
// src/scheduler/task_scheduler.rs
pub struct TaskScheduler {
    trigger_domain: Arc<TaskTriggerDomain>,
    interval: Duration,
}

impl TaskScheduler {
    // 启动调度器
    pub async fn start(self: Arc<Self>) {
        tracing::info!("task scheduler started");
        
        let mut interval = tokio::time::interval(self.interval);
        
        loop {
            interval.tick().await;
            
            if let Err(e) = self.execute_due_triggers().await {
                tracing::error!("scheduler execution failed: {}", e);
            }
        }
    }
    
    // 执行所有到期触发器
    async fn execute_due_triggers(&self) -> Result<(), AppError> {
        let now = chrono::Utc::now().timestamp_millis();
        
        // 1. 查询所有到期触发器
        let triggers = self.trigger_dal
            .list_due_triggers(RequestContext::system(), now)
            .await?;
        
        if triggers.is_empty() {
            return Ok(());
        }
        
        tracing::info!("found {} due triggers to execute", triggers.len());
        
        // 2. 并发执行所有触发器
        let mut tasks = Vec::new();
        for trigger in triggers {
            let self_clone = self.clone();
            
            tasks.push(tokio::spawn(async move {
                let result = self_clone
                    .trigger_dal
                    .execute_trigger(RequestContext::system(), &trigger.id)
                    .await;
                
                match result {
                    Ok(task) => tracing::info!("trigger {} created task {}", trigger.id, task.id),
                    Err(e) => tracing::error!("trigger {} execution failed: {}", trigger.id, e),
                }
            }));
        }
        
        // 等待所有执行完成
        for task in tasks {
            let _ = task.await;
        }
        
        Ok(())
    }
}

// 在 main.rs 中启动
pub async fn init_schedulers(config: &AppConfig, trigger_domain: Arc<TaskTriggerDomain>) {
    let scheduler = Arc::new(TaskScheduler {
        trigger_domain,
        interval: Duration::from_secs(60),  // 每分钟扫描一次
    });
    
    tokio::spawn(async move {
        scheduler.start().await;
    });
}
```

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

## 📋 实现任务清单

- [ ] 创建 `common/src/enums/task_trigger.rs` 枚举
- [ ] 创建 `src/models/task_trigger.rs` PO 结构体
- [ ] 创建数据库迁移脚本
- [ ] 实现 `TaskTriggerDao` SQLite 实现
- [ ] 实现 `TaskTriggerDal`（含 Cron 计算）
- [ ] 实现 `TaskTriggerDomain`
- [ ] 实现 `TaskScheduler` 后台调度器
- [ ] 在 main.rs 中集成启动调度器
- [ ] 编写单元测试

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
