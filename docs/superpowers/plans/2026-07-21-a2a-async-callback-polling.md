# A2A 异步回调与轮询 Producer 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 完善 A2A 协议 Client 端的异步处理能力：提供回调接收 API 承接外部 Agent 推送，并新增轮询 Producer 定期检查分配给外部 Agent 的任务状态，统一通过事件中心投递更新。

**Architecture:**
- **数据模型对应：** A2A Task ↔ 我们的 Task（委托给外部 Agent 的工作单元）
- **外部 task_id 存储：** 通过 Task.tags 存储（格式 `"a2a_task_id:xxx"`）
- **回调端点：** `POST /a2a/callback/:task_id`（公开路由），URL 中包含我们的 task_id，校验后发布 `A2aTaskUpdateEvent`
- **轮询 Producer：** 注册到 AOP，每 30 秒通过 hr_domain 获取所有 Remote Agent，通过 task_manage().list() 查询分配给这些 Agent 的 InProgress Task，从 tags 解析外部 a2a_task_id，调用 `tasks/get`，有更新时发布事件
- **Consumer：** 消费事件，处理新消息（通过 send_to_user 发送给用户）和状态变更（通过 transition_status 更新 Task 状态）
- **本次范围：** 搭好回调+轮询+事件处理框架；外部 Agent 调用流程（创建Task、构造notification_url、异步处理）后续改造

**Tech Stack:** Rust, axum (HTTP), AOP Event Center, tokio (async), serde_json

**约定：**
- Producer 通过 Domain 层查询（hr_domain + task_manage），不直接操作 DAO
- 外部 a2a_task_id 存储在 Task.tags 中，格式 `"a2a_task_id:{uuid}"`
- 轮询只查 `assignee_type=Agent, status=InProgress` 的 Task
- Consumer ack/nack 为空实现（事件处理幂等由业务层保证）

---

## 文件结构概览

```
src/
├── models/events/
│   ├── mod.rs                 # [MODIFY] 新增 A2aTaskUpdateEvent 导出
│   └── a2a_task_update.rs     # [CREATE] A2aTaskUpdateEvent 事件定义
├── handlers/a2a/
│   ├── mod.rs                 # [MODIFY] 注册 callback 模块
│   └── callback.rs            # [CREATE] A2A 回调接收端点
├── producer/
│   ├── mod.rs                 # [MODIFY] 注册 A2aPollingProducer
│   └── a2a_polling.rs         # [CREATE] A2A 任务轮询 Producer
├── consumer/
│   ├── mod.rs                 # [MODIFY] 注册 A2aTaskUpdateConsumer
│   └── a2a_task_update.rs     # [CREATE] A2A 任务更新消费者
├── service/dao/agent_runtime/
│   └── a2a.rs                 # [MODIFY] 新增 fetch_a2a_task() 方法
└── router.rs                  # [MODIFY] 添加公开回调路由
```

---

### Task 1: 定义 A2aTaskUpdateEvent 事件

**Files:**
- Create: `src/models/events/a2a_task_update.rs`
- Modify: `src/models/events/mod.rs`

事件携带：本地 task_id、远程 agent_id、来源（回调/轮询）、完整 A2aTask JSON。order_key 用 local_task_id 保证同一任务的事件顺序处理。

- [ ] **Step 1: 创建事件文件**

创建 `src/models/events/a2a_task_update.rs`：

```rust
use serde::{Deserialize, Serialize};
use crate::pkg::aop::{Event, EventKind};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum A2aUpdateSource {
    Callback,
    Polling,
}

/// A2A 任务更新事件
///
/// 当委托给外部 A2A Agent 的任务状态更新时发布此事件。
/// 来源：1) 外部 Agent PushNotification 回调；2) 轮询 Producer 主动查询。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2aTaskUpdateEvent {
    pub event_id: String,
    /// 我们的本地 Task ID（path 参数或轮询时直接获取）
    pub local_task_id: String,
    /// 分配的外部 Agent ID
    pub remote_agent_id: String,
    pub source: A2aUpdateSource,
    /// 完整的 A2aTask JSON 字符串
    pub task_json: String,
    pub created_at: i64,
}

/// 从 Task tags 中解析外部 A2A task_id 的常量
pub const A2A_TASK_ID_TAG_PREFIX: &str = "a2a_task_id:";

pub fn extract_a2a_task_id(tags: &[String]) -> Option<String> {
    tags.iter()
        .find(|t| t.starts_with(A2A_TASK_ID_TAG_PREFIX))
        .map(|t| t[A2A_TASK_ID_TAG_PREFIX.len()..].to_string())
}

pub fn make_a2a_task_tag(external_task_id: &str) -> String {
    format!("{}{}", A2A_TASK_ID_TAG_PREFIX, external_task_id)
}

impl Event for A2aTaskUpdateEvent {
    fn kind(&self) -> EventKind {
        EventKind::new("a2a.task.update")
    }

    fn id(&self) -> &str {
        &self.event_id
    }

    fn order_key(&self) -> &str {
        &self.local_task_id
    }

    fn priority(&self) -> u8 {
        match self.source {
            A2aUpdateSource::Callback => 10,
            A2aUpdateSource::Polling => 5,
        }
    }

    fn created_at(&self) -> i64 {
        self.created_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_basic() {
        let event = A2aTaskUpdateEvent {
            event_id: "evt-1".to_string(),
            local_task_id: "task-1".to_string(),
            remote_agent_id: "agent-1".to_string(),
            source: A2aUpdateSource::Callback,
            task_json: "{}".to_string(),
            created_at: 1234567890,
        };
        assert_eq!(event.kind().0, "a2a.task.update");
        assert_eq!(event.id(), "evt-1");
        assert_eq!(event.order_key(), "task-1");
        assert_eq!(event.priority(), 10);
    }

    #[test]
    fn test_polling_priority() {
        let event = A2aTaskUpdateEvent {
            event_id: "evt-2".to_string(),
            local_task_id: "task-2".to_string(),
            remote_agent_id: "agent-2".to_string(),
            source: A2aUpdateSource::Polling,
            task_json: "{}".to_string(),
            created_at: 1234567890,
        };
        assert_eq!(event.priority(), 5);
    }

    #[test]
    fn test_extract_a2a_task_id() {
        let tags = vec!["a2a_task_id:ext-123".to_string(), "other_tag".to_string()];
        assert_eq!(extract_a2a_task_id(&tags), Some("ext-123".to_string()));

        let no_tag = vec!["other_tag".to_string()];
        assert_eq!(extract_a2a_task_id(&no_tag), None);

        let empty: Vec<String> = vec![];
        assert_eq!(extract_a2a_task_id(&empty), None);
    }

    #[test]
    fn test_make_a2a_task_tag() {
        assert_eq!(make_a2a_task_tag("ext-456"), "a2a_task_id:ext-456");
    }
}
```

- [ ] **Step 2: 在 events/mod.rs 中导出**

修改 `src/models/events/mod.rs`：

```rust
pub mod message;
pub mod cron_trigger;
pub mod a2a_task_update;

pub use message::MessageCreatedEvent;
pub use cron_trigger::CronTriggerEvent;
pub use a2a_task_update::{A2aTaskUpdateEvent, A2aUpdateSource, extract_a2a_task_id, make_a2a_task_tag, A2A_TASK_ID_TAG_PREFIX};
```

- [ ] **Step 3: 编译并运行测试**

Run: `cargo test a2a_task_update -p ai_orz 2>&1 | tail -15`
Expected: 4 个测试全部 PASS

---

### Task 2: 为 A2A Runtime DAO 添加 fetch_a2a_task 方法

**Files:**
- Modify: `src/service/dao/agent_runtime/a2a.rs`

Producer 需要调用远程 `tasks/get` 接口检查任务状态。

- [ ] **Step 1: 在 a2a.rs 中添加 fetch_a2a_task 函数**

在 `execute_a2a` 函数之后、tests 模块之前添加：

```rust
/// 执行 A2A tasks/get 调用，获取远程任务状态
pub async fn fetch_a2a_task(
    http: &Client,
    agent_id: &str,
    endpoint: &str,
    auth_token: &Option<String>,
    external_task_id: &str,
) -> Result<common::api::a2a::A2aTask> {
    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: "tasks/get".to_string(),
        params: serde_json::json!({ "id": external_task_id }),
        id: next_request_id(),
    };

    let mut req_builder = http
        .post(endpoint)
        .header("Content-Type", "application/json");

    if let Some(token) = auth_token {
        req_builder = req_builder.bearer_auth(token);
    }

    let response = req_builder.json(&request).send().await.map_err(|e| {
        err!(Internal, "Agent {}: fetch_task HTTP failed: {}", agent_id, e)
    })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(err!(Internal, "Agent {}: fetch_task HTTP {}: {}", agent_id, status, body));
    }

    let rpc_response: JsonRpcResponse = response.json().await.map_err(|e| {
        err!(Internal, "Agent {}: fetch_task parse failed: {}", agent_id, e)
    })?;

    if let Some(e) = rpc_response.error {
        return Err(err!(Internal, "Agent {}: fetch_task RPC error {}: {}", agent_id, e.code, e.message));
    }

    let result = rpc_response.result.unwrap_or_default();
    let task: common::api::a2a::A2aTask = serde_json::from_value(result)
        .map_err(|e| err!(Internal, "Agent {}: deserialize A2aTask failed: {}", agent_id, e))?;

    Ok(task)
}
```

- [ ] **Step 2: 在 tests 模块末尾（最后一个 `}` 之前）添加测试**

注意测试模块已有 `use serde_json::json;` 导入。

```rust
    #[test]
    fn test_fetch_task_params() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "tasks/get".to_string(),
            params: json!({"id": "ext-task-1"}),
            id: Value::Number(100.into()),
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["method"], "tasks/get");
        assert_eq!(v["params"]["id"], "ext-task-1");
    }

    #[test]
    fn test_deserialize_task_working() {
        let resp = json!({
            "jsonrpc": "2.0",
            "result": {
                "id": "ext-task-1",
                "status": {"state": "working", "timestamp": "2024-01-01T00:00:00Z"},
                "messages": [{"role": "agent", "parts": [{"type": "text", "text": "Processing..."}]}]
            },
            "id": 1
        });
        let r: JsonRpcResponse = serde_json::from_value(resp).unwrap();
        let task: common::api::a2a::A2aTask = serde_json::from_value(r.result.unwrap()).unwrap();
        assert_eq!(task.id, "ext-task-1");
        assert_eq!(task.status.state, common::api::a2a::A2aTaskState::Working);
        assert_eq!(task.messages.len(), 1);
    }

    #[test]
    fn test_deserialize_task_completed() {
        let resp = json!({
            "jsonrpc": "2.0",
            "result": {
                "id": "ext-task-2",
                "status": {"state": "completed", "timestamp": "2024-01-01T00:00:00Z"},
                "messages": [
                    {"role": "user", "parts": [{"type": "text", "text": "Do X"}]},
                    {"role": "agent", "parts": [{"type": "text", "text": "Done: X is complete"}]}
                ],
                "artifacts": []
            },
            "id": 2
        });
        let r: JsonRpcResponse = serde_json::from_value(resp).unwrap();
        let task: common::api::a2a::A2aTask = serde_json::from_value(r.result.unwrap()).unwrap();
        assert_eq!(task.status.state, common::api::a2a::A2aTaskState::Completed);
    }
```

- [ ] **Step 3: 运行测试**

Run: `cargo test service::dao::agent_runtime::a2a -p ai_orz 2>&1 | tail -15`
Expected: 所有测试 PASS（原有 9 + 新增 3 = 12 个）

---

### Task 3: 实现 A2A 回调接收端点

**Files:**
- Create: `src/handlers/a2a/callback.rs`
- Modify: `src/handlers/a2a/mod.rs`
- Modify: `src/router.rs`

公开路由 `POST /a2a/callback/:task_id`，接收外部 Agent 的 PushNotification 推送，校验后发布事件。
URL 中的 `:task_id` 是我们的本地 Task ID（后续调用流程改造时构造 notification_url 会带上）。

- [ ] **Step 1: 创建 callback.rs**

```rust
//! A2A PushNotifications 回调接收端点
//!
//! POST /a2a/callback/:task_id
//! 公开路由，无需 JWT。外部 Agent 完成任务或有新消息时向此 URL 推送 A2aTask。
//!
//! URL 中的 :task_id 是我们的本地 Task ID。外部 Agent 从我们调用 tasks/send
//! 时传入的 notification_url 中获取此 URL（后续调用流程改造时构造）。

use axum::extract::Path;
use axum::response::{IntoResponse, Json};
use common::api::a2a::A2aTask;
use common::enums::{AssigneeType, TaskStatus};
use serde_json::json;

use crate::models::events::{A2aTaskUpdateEvent, A2aUpdateSource, extract_a2a_task_id};
use crate::pkg::aop::publish;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain as project_domain;

pub async fn handle_a2a_callback(
    axum::Extension(ctx): axum::Extension<RequestContext>,
    Path(task_id): Path<String>,
    Json(task): Json<A2aTask>,
) -> common::error::Result<impl IntoResponse> {
    // 1. 校验本地 Task 存在且分配给 Agent
    let Some(local_task) = project_domain().task_manage().get(ctx.clone(), &task_id).await? else {
        return Err(common::error::Error::not_found(format!("Task {} not found", task_id)));
    };

    // 已经是终态的任务不再接受回调
    if matches!(
        local_task.po.status,
        TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::Archived
    ) {
        return Ok(Json(json!({"ok": true, "skipped": true, "reason": "task already terminal"})));
    }

    // 校验是分配给 Agent 的任务
    if local_task.po.assignee_type != AssigneeType::Agent {
        return Err(common::error::Error::bad_request(
            "Callback only accepted for Agent-assigned tasks"
        ));
    }

    // 校验外部 task_id 与本地记录一致（安全校验）
    let tags = local_task.po.get_tags();
    if let Some(expected_ext_id) = extract_a2a_task_id(&tags) {
        if task.id != expected_ext_id {
            return Err(common::error::Error::bad_request(format!(
                "External task ID mismatch: expected {}, got {}",
                expected_ext_id, task.id
            )));
        }
    }

    let agent_id = local_task.po.assignee_id.clone();

    // 2. 发布事件
    let now = common::constants::utils::current_timestamp();
    let event = A2aTaskUpdateEvent {
        event_id: format!("{}-cb-{}", task_id, now),
        local_task_id: task_id.clone(),
        remote_agent_id: agent_id,
        source: A2aUpdateSource::Callback,
        task_json: serde_json::to_string(&task)
            .map_err(|e| common::error::err!(Internal, "serialize task: {}", e))?,
        created_at: now,
    };
    publish(event).await;

    log_info!(&ctx, "a2a_cb", "task={} state={:?} msgs={}", task_id, task.status.state, task.messages.len());

    Ok(Json(json!({"ok": true})))
}
```

- [ ] **Step 2: 修改 handlers/a2a/mod.rs 添加模块声明**

在文件现有 `pub mod xxx;` 列表末尾添加：
```rust
pub mod callback;
```

- [ ] **Step 3: 修改 router.rs 添加公开回调路由**

找到 create_router 函数中 A2A 相关路由（`/.well-known/agent.json` 附近），在其后添加：

```rust
        // A2A PushNotification 回调（公开端点，无需 JWT）
        .route(
            "/a2a/callback/:task_id",
            post(handlers::a2a::callback::handle_a2a_callback)
                .layer(axum::middleware::from_fn({
                    let config = config.clone();
                    move |req, next| request_context_middleware(config.clone(), req, next)
                })),
        )
```

注意确认 `post` 已从 axum 导入。

- [ ] **Step 4: 编译验证**

Run: `cargo check -p ai_orz 2>&1 | grep -E "^error" | head -20`
Expected: 无错误（如 `task_manage()` 访问方式不对，根据实际 domain 导出修正）

---

### Task 4: 实现 A2A 轮询 Producer

**Files:**
- Create: `src/producer/a2a_polling.rs`
- Modify: `src/producer/mod.rs`

Producer 每 30 秒：
1. 通过 hr_domain 获取所有 Remote 类型 Agent
2. 对每个 Agent，通过 task_manage().list() 查询分配给它的 InProgress Task
3. 从 Task tags 中解析外部 a2a_task_id
4. 调用 fetch_a2a_task 获取最新状态
5. 有新消息或终态则发布 A2aTaskUpdateEvent

- [ ] **Step 1: 创建 a2a_polling.rs**

```rust
use common::enums::{AgentKind, AssigneeType, TaskStatus};
use common::error::Result;
use crate::models::events::{
    A2aTaskUpdateEvent, A2aUpdateSource, extract_a2a_task_id,
};
use crate::pkg::RequestContext;
use crate::pkg::aop::{Producer, Registry};
use crate::service::dao::agent_runtime::a2a::{self, A2aRuntimeConfig};
use crate::service::domain::hr::domain as hr_domain;
use crate::service::domain::project::domain as project_domain;
use reqwest::Client;
use std::sync::{Arc, RwLock};
use std::time::Duration;

pub struct A2aPollingProducer {
    registry: RwLock<Option<Arc<Registry>>>,
    http: Client,
}

impl A2aPollingProducer {
    pub fn new() -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            registry: RwLock::new(None),
            http,
        }
    }

    fn extract_a2a_config(agent: &crate::models::agent::Agent) -> Option<A2aRuntimeConfig> {
        let cfg = agent.po.get_external_config()?;
        match cfg {
            crate::models::agent::ExternalAgentConfig::Remote {
                endpoint, agent_name, auth_token, timeout_secs
            } => Some(A2aRuntimeConfig { endpoint, agent_name, auth_token, timeout_secs }),
            _ => None,
        }
    }
}

#[async_trait::async_trait]
impl Producer for A2aPollingProducer {
    fn name(&self) -> &str { "a2a_polling" }

    async fn register(&self, registry: Arc<Registry>) -> Result<()> {
        *self.registry.write().unwrap() = Some(registry);
        Ok(())
    }

    fn poll_interval_secs(&self) -> u64 { 30 }

    async fn poll(&self) -> Result<()> {
        let registry = self.registry.read().unwrap().clone();
        let Some(registry) = registry else {
            return Err(common::error::err!(Internal, "registry not registered"));
        };

        let ctx = RequestContext::new(None, None);

        // 1. 获取所有 Remote 类型 Agent
        let all_agents = hr_domain().agent_manage().find_all(ctx.clone()).await?;
        let remote_agents: Vec<_> = all_agents.into_iter()
            .filter(|a| a.po.kind == AgentKind::Remote)
            .collect();

        if remote_agents.is_empty() {
            return Ok(());
        }

        let mut total_checked = 0;
        let mut total_updated = 0;

        // 2. 对每个 Remote Agent，查询分配给它的 InProgress Task
        for agent in &remote_agents {
            let Some(config) = Self::extract_a2a_config(agent) else { continue };

            let tasks = project_domain().task_manage().list(
                ctx.clone(),
                None,
                Some(AssigneeType::Agent),
                Some(&agent.po.id),
                Some(TaskStatus::InProgress),
                Some(100),
            ).await?;

            for task in tasks {
                let tags = task.po.get_tags();
                let Some(ext_task_id) = extract_a2a_task_id(&tags) else { continue };

                total_checked += 1;

                // 3. 调用远程 tasks/get
                let remote_task = match a2a::fetch_a2a_task(
                    &self.http,
                    &agent.po.id,
                    &config.endpoint,
                    &config.auth_token,
                    &ext_task_id,
                ).await {
                    Ok(t) => t,
                    Err(e) => {
                        log_warn!(&ctx, "a2a_poll", "fetch task={} ext_id={} err={}", task.po.id, ext_task_id, e);
                        continue;
                    }
                };

                // 4. 判断是否有更新（有消息或终态）
                let has_messages = !remote_task.messages.is_empty();
                let is_terminal = matches!(
                    remote_task.status.state,
                    common::api::a2a::A2aTaskState::Completed
                    | common::api::a2a::A2aTaskState::Failed
                    | common::api::a2a::A2aTaskState::Canceled
                );

                if has_messages || is_terminal {
                    let now = common::constants::utils::current_timestamp();
                    let event = A2aTaskUpdateEvent {
                        event_id: format!("{}-poll-{}", task.po.id, now),
                        local_task_id: task.po.id.clone(),
                        remote_agent_id: agent.po.id.clone(),
                        source: A2aUpdateSource::Polling,
                        task_json: serde_json::to_string(&remote_task)
                            .map_err(|e| common::error::err!(Internal, "serialize: {}", e))?,
                        created_at: now,
                    };
                    registry.publish(event).await;
                    total_updated += 1;
                }
            }
        }

        if total_checked > 0 {
            log_debug!("a2a polling: checked={} updated={}", total_checked, total_updated);
        }
        Ok(())
    }
}
```

- [ ] **Step 2: 修改 producer/mod.rs 注册 Producer**

将文件更新为：

```rust
pub mod cron_trigger;
pub mod message_channel;
pub mod a2a_polling;

use common::error::Result;
use crate::pkg::aop;
use std::sync::Arc;

pub async fn init() -> Result<()> {
    sys_info!("registering business producers to AOP event center...");

    aop::registry()
        .register_producer(Arc::new(cron_trigger::CronTriggerProducer::new()))
        .await?;

    aop::registry()
        .register_producer(Arc::new(a2a_polling::A2aPollingProducer::new()))
        .await?;

    sys_info!("all business producers registered");

    sys_info!("starting message channel producers...");
    message_channel::init().await?;

    Ok(())
}
```

- [ ] **Step 3: 编译验证**

Run: `cargo check -p ai_orz 2>&1 | grep -E "^error" | head -20`
Expected: 无错误（如果 `find_all` 或 `task_manage` 签名不匹配，根据编译器错误修正）

---

### Task 5: 实现 A2A 任务更新 Consumer

**Files:**
- Create: `src/consumer/a2a_task_update.rs`
- Modify: `src/consumer/mod.rs`

Consumer 消费 `A2aTaskUpdateEvent`：
1. **新消息处理：** 遍历 task.messages，提取 agent 角色的文本消息，通过 `MessageDomain.delivery().send_to_user()` 发送给任务创建者（task.root_user_id）
2. **状态变更：** 若远程任务为终态，调用 `task_manage().transition_status()` 更新本地 Task 状态

- [ ] **Step 1: 先确认 SendToUserCommand 字段和 task_manage transition_status 签名**

Run: `grep -A 12 "struct SendToUserCommand" src/service/domain/message/delivery.rs`
Run: `grep -A 5 "fn transition_status" src/service/domain/project/mod.rs`

如果字段与下方代码不一致，按照实际字段调整。

- [ ] **Step 2: 创建 a2a_task_update.rs**

```rust
//! A2A 任务更新消费者
//!
//! 订阅 a2a.task.update 事件（来自回调或轮询），处理：
//! 1. 新 agent 消息 → 通过 MessageDomain 发送给任务创建者
//! 2. 任务终态 → 更新本地 Task 状态

use async_trait::async_trait;
use common::api::a2a::{A2aMessagePart, A2aTaskState};
use common::enums::TaskStatus;
use common::error::Result;
use serde_json::Value;
use std::sync::Arc;

use crate::models::events::A2aTaskUpdateEvent;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};
use crate::pkg::RequestContext;
use crate::service::domain::message::{self as message_domain, SendToUserCommand};
use crate::service::domain::project::domain as project_domain;

pub struct A2aTaskUpdateConsumer {
    message_domain: Arc<dyn message_domain::MessageDomain>,
}

impl A2aTaskUpdateConsumer {
    pub fn new() -> Self {
        Self { message_domain: message_domain::domain() }
    }
}

#[async_trait]
impl Consumer for A2aTaskUpdateConsumer {
    fn name(&self) -> &str { "a2a.task.update" }

    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::new("a2a.task.update")]
    }

    fn consume_mode(&self) -> ConsumeMode { ConsumeMode::Async }

    async fn on_event(&self, event: Value) -> Result<()> {
        let update: A2aTaskUpdateEvent = serde_json::from_value(event)?;
        let remote_task: common::api::a2a::A2aTask = serde_json::from_str(&update.task_json)
            .map_err(|e| common::error::err!(InvalidRequest, "invalid task_json: {}", e))?;

        let ctx = RequestContext::builder()
            .task_id(update.local_task_id.clone())
            .agent_id(update.remote_agent_id.clone())
            .build();

        self.process_messages(&ctx, &update, &remote_task).await?;
        self.process_status(&ctx, &update, &remote_task).await?;
        Ok(())
    }

    async fn ack(&self, _event_id: &str) -> Result<()> { Ok(()) }
    async fn nack(&self, _event_id: &str) -> Result<()> { Ok(()) }
    fn concurrency(&self) -> usize { 2 }
    fn empty_queue_sleep_ms(&self) -> u64 { 200 }
    fn error_retry_sleep_ms(&self) -> u64 { 2000 }
}

impl A2aTaskUpdateConsumer {
    async fn process_messages(
        &self,
        ctx: &RequestContext,
        update: &A2aTaskUpdateEvent,
        remote_task: &common::api::a2a::A2aTask,
    ) -> Result<()> {
        let Some(mut local_task) = project_domain().task_manage().get(ctx.clone(), &update.local_task_id).await? else {
            return Ok(());
        };

        let to_user_id = &local_task.po.root_user_id;

        for msg in &remote_task.messages {
            if msg.role != "agent" && msg.role != "assistant" { continue; }
            let text = extract_text(&msg.parts);
            if text.is_empty() { continue; }

            let cmd = SendToUserCommand {
                from_agent_id: &update.remote_agent_id,
                to_user_id,
                content: &text,
                project_id: local_task.po.project_id.as_deref(),
                task_id: Some(&update.local_task_id),
                reply_to_id: None,
            };

            if let Err(e) = self.message_domain.delivery().send_to_user(ctx.clone(), cmd).await {
                log_warn!(ctx, "a2a_consumer", "deliver msg for task={} failed: {}", update.local_task_id, e);
            }
        }
        Ok(())
    }

    async fn process_status(
        &self,
        ctx: &RequestContext,
        update: &A2aTaskUpdateEvent,
        remote_task: &common::api::a2a::A2aTask,
    ) -> Result<()> {
        let Some(mut local_task) = project_domain().task_manage().get(ctx.clone(), &update.local_task_id).await? else {
            return Ok(());
        };

        if matches!(
            local_task.po.status,
            TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::Archived
        ) {
            return Ok(());
        }

        let target = match remote_task.status.state {
            A2aTaskState::Completed => Some(TaskStatus::Completed),
            A2aTaskState::Failed | A2aTaskState::Canceled | A2aTaskState::Rejected => Some(TaskStatus::Cancelled),
            _ => None,
        };

        if let Some(status) = target {
            project_domain().task_manage().transition_status(ctx.clone(), &mut local_task, status).await?;
            log_info!(ctx, "a2a_consumer", "task {} -> {:?}", update.local_task_id, status);
        }
        Ok(())
    }
}

fn extract_text(parts: &[A2aMessagePart]) -> String {
    parts.iter()
        .filter_map(|p| match p {
            A2aMessagePart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
```

- [ ] **Step 3: 修改 consumer/mod.rs 注册 Consumer（同步方法，不加 .await）**

```rust
pub mod message;
pub mod scheduler;
pub mod a2a_task_update;

use common::error::Result;
use std::sync::Arc;
use crate::pkg::aop;

pub async fn init() -> Result<()> {
    sys_info!("registering business consumers to AOP event center...");

    aop::registry().register_consumer(Arc::new(message::MessageConsumer::new()))?;
    aop::registry().register_consumer(Arc::new(scheduler::CronTriggerConsumer::new()))?;
    aop::registry().register_consumer(Arc::new(a2a_task_update::A2aTaskUpdateConsumer::new()))?;

    sys_info!("all business consumers registered");
    Ok(())
}
```

- [ ] **Step 4: 编译验证，根据错误修正**

Run: `cargo check -p ai_orz 2>&1 | grep -E "^error" | head -30`

可能的修正点：
- `SendToUserCommand` 字段名不对 → 查看 delivery.rs 中实际定义调整
- `A2aTaskState::Rejected` 不存在 → 查看 common/src/api/a2a.rs 中实际枚举变体删除不存在的
- `task_manage().get()` 返回类型不对 → 查看实际签名
- RequestContext builder 方法不对 → 查看实际 builder 方法

- [ ] **Step 5: 编译通过**

Run: `cargo check -p ai_orz 2>&1 | grep -E "^error" | head -10`
Expected: 无错误

---

### Task 6: 全量测试验证

- [ ] **Step 1: 运行 A2A 相关测试**

Run: `cargo test a2a -p ai_orz 2>&1 | grep -E "test result:|running"`

- [ ] **Step 2: 运行 events 测试**

Run: `cargo test events -p ai_orz 2>&1 | grep "test result:"`

- [ ] **Step 3: 运行全量测试**

Run: `cargo test -p ai_orz 2>&1 | grep "test result:"`
Expected: 所有测试 PASS，0 failed

- [ ] **Step 4: 检查无新 warnings**

Run: `cargo check -p ai_orz 2>&1 | grep "warning:" | grep -v "unused import" | head -20`

---

### Task 7: 最终确认

- [ ] **Step 1: 确认 agent_card.rs 中 push_notifications 为 true**（之前 review 已修复）

- [ ] **Step 2: 最终全量测试**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo test 2>&1 | grep "test result:"`

---

## 自我审查

**Spec 覆盖率：**
- ✅ 专用回调 API 承接外部 A2A 回调（Task 3）
- ✅ 回调转内部事件投递（Task 3 发事件，Task 5 Consumer 处理）
- ✅ 消息转消息：process_messages 调用 send_to_user（Task 5）
- ✅ 状态变更转状态更新：process_status 调用 transition_status（Task 5）
- ✅ 轮询注册为 Producer，通过 Domain 层查询（Task 4）
- ✅ 外部 task_id 通过 tags 存储，提供 extract/make 工具函数（Task 1）

**架构对齐：**
- ✅ Producer 通过 hr_domain + task_manage（Domain层）查询，不直接用 DAO
- ✅ 轮询对象是分配给 Remote Agent 的 InProgress Task（不是 Project）
- ✅ Agent ID 列表通过 hr_domain 获取再逐个查询
- ✅ register_consumer 是同步方法（参考现有 consumer/mod.rs）
- ✅ order_key 用 local_task_id 保证同任务事件顺序处理
- ✅ 回调URL包含 local_task_id，直接定位，tag 中外部id用于安全校验

**已知简化（后续迭代）：**
1. 调用流程改造（execute_a2a 支持异步模式、创建本地 Task、构造含 task_id 的 notification_url）
2. 消息去重（后续可在 MessagePo 增加 external_message_id 或通过消息内容+时间窗口去重）
3. 轮询性能优化（当前逐个 Agent 查询，后续可批量查询所有 InProgress Agent 类型任务再过滤）
4. Consumer ack/nack 状态跟踪
