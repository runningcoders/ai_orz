# SSE 消息推送实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现 SSE（Server-Sent Events）消息推送机制，替代现有的 3 秒短轮询，实现实时消息推送。

**Architecture:** SSE 作为一种消息渠道，直接融入现有 `deliver_message()` 流程。DAO 层直接管理 SSE 连接 + 推送，DAL 层负责消息加工，Domain 层负责订阅管理和多渠道分发。

**Tech Stack:** Rust + Axum SSE + tokio::sync::broadcast + async_stream

---

## 文件结构

### 新增文件

```
src/
├── service/
│   ├── dao/
│   │   └── message_push.rs              # SSE 推送 DAO（连接管理 + 推送）
│   │
│   └── dal/
│       └── message_push.rs              # 消息推送 DAL（消息加工）
│
└── handlers/
    └── finance/
        └── message/
            └── subscribe_sse.rs         # SSE 订阅端点
```

### 修改文件

```
src/
├── service/
│   ├── dao/
│   │   └── mod.rs                       # 导出 SsePushDao
│   ├── dal/
│   │   └── mod.rs                       # 导出 MessagePushDal
│   └── domain/
│       └── message/
│           ├── mod.rs                   # MessageDelivery trait 新增 subscribe/unsubscribe
│           └── delivery.rs              # deliver_message 加入 SSE 推送
│
├── handlers/
│   └── finance/
│       └── message/
│           └── mod.rs                   # 导出 subscribe_sse_handler
│
├── router.rs                            # 添加 SSE 路由
└── consumer/
    └── message.rs                       # handle_user_message 调用 deliver_message
```

---

## Task 1: DAO 层 - SSE 推送 DAO

**Files:**
- Create: `src/service/dao/message_push.rs`
- Modify: `src/service/dao/mod.rs`

**职责:** SSE 连接管理 + 消息推送。

- [ ] **Step 1: 创建 SSE 推送 DAO**

```rust
//! SSE 推送 DAO
//!
//! SSE 连接管理 + 消息推送

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use tokio::sync::{RwLock, broadcast};
use async_trait::async_trait;
use crate::pkg::RequestContext;

#[derive(Debug, Clone)]
pub struct SsePushResult {
    pub success: bool,
    pub delivered_count: usize,
    pub error: Option<String>,
}

#[async_trait]
pub trait SsePushDao: Send + Sync {
    async fn push(
        &self,
        ctx: RequestContext,
        user_id: &str,
        payload: &str,
    ) -> crate::error::Result<SsePushResult>;

    async fn register(
        &self,
        ctx: RequestContext,
        user_id: &str,
        connection_id: &str,
    ) -> broadcast::Receiver<String>;

    async fn unregister(&self, ctx: RequestContext, connection_id: &str);

    async fn connection_count(&self, ctx: RequestContext, user_id: &str) -> usize;
}

pub struct SsePushDaoImpl {
    connections: Arc<RwLock<HashMap<String, broadcast::Sender<String>>>>,
    user_connections: Arc<RwLock<HashMap<String, HashSet<String>>>>,
}

impl SsePushDaoImpl {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            user_connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for SsePushDaoImpl {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl SsePushDao for SsePushDaoImpl {
    async fn push(
        &self,
        _ctx: RequestContext,
        user_id: &str,
        payload: &str,
    ) -> crate::error::Result<SsePushResult> {
        let user_connections = self.user_connections.read().await;
        let connection_ids = user_connections.get(user_id).cloned().unwrap_or_default();
        let connections = self.connections.read().await;

        let mut success_count = 0;
        for conn_id in connection_ids {
            if let Some(tx) = connections.get(&conn_id) {
                if tx.send(payload.to_string()).is_ok() {
                    success_count += 1;
                }
            }
        }

        Ok(SsePushResult {
            success: success_count > 0,
            delivered_count: success_count,
            error: None,
        })
    }

    async fn register(
        &self,
        _ctx: RequestContext,
        user_id: &str,
        connection_id: &str,
    ) -> broadcast::Receiver<String> {
        let (tx, rx) = broadcast::channel(100);
        self.connections.write().await.insert(connection_id.to_string(), tx);
        self.user_connections
            .write()
            .await
            .entry(user_id.to_string())
            .or_insert_with(HashSet::new)
            .insert(connection_id.to_string());
        rx
    }

    async fn unregister(&self, _ctx: RequestContext, connection_id: &str) {
        if let Some(_) = self.connections.write().await.remove(connection_id) {
            let mut user_connections = self.user_connections.write().await;
            for (_, conn_set) in user_connections.iter_mut() {
                conn_set.remove(connection_id);
            }
        }
    }

    async fn connection_count(&self, _ctx: RequestContext, user_id: &str) -> usize {
        self.user_connections
            .read()
            .await
            .get(user_id)
            .map(|s| s.len())
            .unwrap_or(0)
    }
}

pub fn dao() -> Arc<dyn SsePushDao> {
    static DAO: OnceLock<Arc<dyn SsePushDao>> = OnceLock::new();
    DAO.get_or_init(|| Arc::new(SsePushDaoImpl::new())).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_ctx() -> RequestContext { RequestContext::new(None, None) }

    #[tokio::test]
    async fn test_register_and_unregister() {
        let dao = SsePushDaoImpl::new();
        let ctx = new_ctx();
        let _rx = dao.register(ctx.clone(), "user_1", "conn_1").await;
        assert_eq!(dao.connection_count(ctx.clone(), "user_1").await, 1);
        dao.unregister(ctx.clone(), "conn_1").await;
        assert_eq!(dao.connection_count(ctx.clone(), "user_1").await, 0);
    }

    #[tokio::test]
    async fn test_push() {
        let dao = SsePushDaoImpl::new();
        let ctx = new_ctx();
        let mut rx = dao.register(ctx.clone(), "user_1", "conn_1").await;
        let result = dao.push(ctx.clone(), "user_1", "hello").await.unwrap();
        assert_eq!(result.success, true);
        assert_eq!(result.delivered_count, 1);
        assert_eq!(rx.try_recv().unwrap(), "hello");
    }
}
```

- [ ] **Step 2: 在 DAO mod.rs 中导出**

修改 `src/service/dao/mod.rs`，添加：

```rust
pub mod message_push;
pub use message_push::{SsePushDao, SsePushDaoImpl, SsePushResult};
```

- [ ] **Step 3: 在 src/service/dao/mod.rs 中添加 init_all 调用**

检查 `init_all()` 函数，确保 `message_push::dao()` 被调用（如果有 init_all 函数的话）。

- [ ] **Step 4: 运行测试验证**

Run: `cargo test --package ai-orz --lib service::dao::message_push::tests -- --nocapture`

Expected: 测试通过

- [ ] **Step 5: 提交**

```bash
git add src/service/dao/message_push.rs src/service/dao/mod.rs
git commit -m "feat: add SSE push DAO with connection management"
```

---

## Task 2: DAL 层 - 消息推送 DAL

**Files:**
- Create: `src/service/dal/message_push.rs`
- Modify: `src/service/dal/mod.rs`

**职责:** SSE 消息加工 + 推送。

- [ ] **Step 1: 创建消息推送 DAL**

```rust
//! 消息推送 DAL
//!
//! SSE 消息加工 + 推送

use crate::models::message::Message;
use crate::pkg::RequestContext;
use crate::service::dao::message_push::SsePushDao;
use async_trait::async_trait;
use std::sync::{Arc, OnceLock};

#[derive(Debug, Clone)]
pub struct SsePushResult {
    pub success: bool,
    pub delivered_count: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EnrichedMessage {
    pub message: Message,
    pub sender_name: Option<String>,
    pub project_name: Option<String>,
}

#[async_trait]
pub trait MessagePushDal: Send + Sync {
    async fn push_to_sse(
        &self,
        ctx: RequestContext,
        user_id: &str,
        message: &Message,
    ) -> crate::error::Result<SsePushResult>;

    async fn subscribe_sse(
        &self,
        ctx: RequestContext,
        user_id: &str,
        connection_id: &str,
    ) -> tokio::sync::broadcast::Receiver<String>;

    async fn unsubscribe_sse(&self, ctx: RequestContext, connection_id: &str);

    async fn sse_connection_count(&self, ctx: RequestContext, user_id: &str) -> usize;
}

pub struct MessagePushDalImpl {
    sse_push_dao: Arc<dyn SsePushDao>,
}

impl MessagePushDalImpl {
    pub fn new(sse_push_dao: Arc<dyn SsePushDao>) -> Self {
        Self { sse_push_dao }
    }
}

#[async_trait]
impl MessagePushDal for MessagePushDalImpl {
    async fn push_to_sse(
        &self,
        ctx: RequestContext,
        user_id: &str,
        message: &Message,
    ) -> crate::error::Result<SsePushResult> {
        let enriched = EnrichedMessage {
            message: message.clone(),
            sender_name: None,
            project_name: None,
        };
        let payload = serde_json::to_string(&enriched)
            .map_err(|e| common::error::Error::internal(format!("failed to serialize message: {}", e)))?;
        let dao_result = self.sse_push_dao.push(ctx, user_id, &payload).await?;
        Ok(SsePushResult {
            success: dao_result.success,
            delivered_count: dao_result.delivered_count,
            error: dao_result.error,
        })
    }

    async fn subscribe_sse(
        &self,
        ctx: RequestContext,
        user_id: &str,
        connection_id: &str,
    ) -> tokio::sync::broadcast::Receiver<String> {
        self.sse_push_dao.register(ctx, user_id, connection_id).await
    }

    async fn unsubscribe_sse(&self, ctx: RequestContext, connection_id: &str) {
        self.sse_push_dao.unregister(ctx, connection_id).await
    }

    async fn sse_connection_count(&self, ctx: RequestContext, user_id: &str) -> usize {
        self.sse_push_dao.connection_count(ctx, user_id).await
    }
}

pub fn dal() -> Arc<dyn MessagePushDal> {
    static DAL: OnceLock<Arc<dyn MessagePushDal>> = OnceLock::new();
    DAL.get_or_init(|| {
        Arc::new(MessagePushDalImpl::new(crate::service::dao::message_push::dao()))
    }).clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::dao::message_push::SsePushDaoImpl;

    fn new_dal() -> Arc<dyn MessagePushDal> {
        Arc::new(MessagePushDalImpl::new(Arc::new(SsePushDaoImpl::new())))
    }

    #[tokio::test]
    async fn test_push_to_sse_no_connection() {
        let dal = new_dal();
        let ctx = RequestContext::new(None, None);
        let result = dal.push_to_sse(ctx, "user_1", &crate::models::message::Message {
            po: crate::models::message::MessagePo {
                id: "msg_1".to_string(),
                organization_id: None,
                root_id: "root_1".to_string(),
                parent_id: None,
                from_id: "agent_1".to_string(),
                to_id: "user_1".to_string(),
                from_role: common::enums::MessageRole::Agent as i32,
                to_role: common::enums::MessageRole::User as i32,
                message_type: common::enums::MessageType::UserMessage as i32,
                content: "Hello".to_string(),
                project_id: None,
                task_id: None,
                tool_call_id: None,
                reply_to_id: None,
                status: common::enums::MessageStatus::Completed as i32,
                created_at: 1234567890,
            },
        }).await.unwrap();

        assert_eq!(result.success, false);
        assert_eq!(result.delivered_count, 0);
    }
}
```

- [ ] **Step 2: 在 DAL mod.rs 中导出**

修改 `src/service/dal/mod.rs`，添加：

```rust
pub mod message_push;
pub use message_push::{MessagePushDal, MessagePushDalImpl, SsePushResult};
```

- [ ] **Step 3: 在 src/service/dal/mod.rs 中添加 init_all 调用**

检查 `init_all()` 函数，确保 `message_push::dal()` 被调用。

- [ ] **Step 4: 运行编译验证**

Run: `cargo check`

Expected: 编译通过

- [ ] **Step 5: 提交**

```bash
git add src/service/dal/message_push.rs src/service/dal/mod.rs
git commit -m "feat: add message push DAL for SSE"
```

---

## Task 3: Domain 层 - 扩展 MessageDelivery

**Files:**
- Modify: `src/service/domain/message/mod.rs`
- Modify: `src/service/domain/message/delivery.rs`

**职责:** 在 MessageDelivery trait 中新增 subscribe/unsubscribe，扩展 deliver_message 加入 SSE 推送。

- [ ] **Step 1: 读取现有 delivery.rs 和 mod.rs 内容**

Read both files to understand current structure.

- [ ] **Step 2: 在 mod.rs 中扩展 MessageDelivery trait**

添加 `SubscribeResult` 结构体和 `subscribe`/`unsubscribe` 方法到 `MessageDelivery` trait：

```rust
#[derive(Debug, Clone)]
pub struct SubscribeResult {
    pub connection_id: String,
    pub user_id: String,
    pub subscribed_at: i64,
}

#[async_trait::async_trait]
pub trait MessageDelivery: Send + Sync {
    // ... 现有方法 ...

    /// 订阅消息推送（SSE）
    async fn subscribe(
        &self,
        ctx: RequestContext,
        user_id: &str,
        connection_id: &str,
    ) -> Result<tokio::sync::broadcast::Receiver<String>>;

    /// 取消订阅（SSE）
    async fn unsubscribe(
        &self,
        ctx: RequestContext,
        connection_id: &str,
    ) -> Result<()>;
}
```

- [ ] **Step 3: 修改 MessageDomainImpl 注入 MessagePushDal**

```rust
struct MessageDomainImpl {
    message_dal: Arc<dyn MessageDal>,
    message_channel_dal: Arc<dyn MessageChannelDal>,
    message_push_dal: Arc<dyn crate::service::dal::message_push::MessagePushDal>, // 新增
}

impl MessageDomainImpl {
    fn new(
        message_dal: Arc<dyn MessageDal>,
        message_channel_dal: Arc<dyn MessageChannelDal>,
        message_push_dal: Arc<dyn crate::service::dal::message_push::MessagePushDal>, // 新增
    ) -> Self {
        Self { message_dal, message_channel_dal, message_push_dal }
    }
}
```

- [ ] **Step 4: 修改 init() 和 new() 函数**

修改 `init()` 和 `new()` 函数，注入 `message_push_dal`。

- [ ] **Step 5: 在 delivery.rs 中实现新方法**

在 `impl MessageDelivery for MessageDomainImpl` 中添加：

```rust
async fn subscribe(
    &self,
    ctx: RequestContext,
    user_id: &str,
    connection_id: &str,
) -> Result<tokio::sync::broadcast::Receiver<String>> {
    let rx = self.message_push_dal.subscribe_sse(ctx, user_id, connection_id).await;
    Ok(rx)
}

async fn unsubscribe(
    &self,
    ctx: RequestContext,
    connection_id: &str,
) -> Result<()> {
    self.message_push_dal.unsubscribe_sse(ctx, connection_id).await;
    Ok(())
}
```

- [ ] **Step 6: 扩展 deliver_message 加入 SSE 推送**

在 `delivery.rs` 的 `deliver_message` 方法中，在传统渠道推送后添加 SSE 推送：

```rust
// 1. 传统渠道推送
let channel_result = self.message_channel_dal
    .deliver_message(ctx.clone(), message, user_id)
    .await?;

// 2. SSE 渠道推送（新增）
let sse_result = self.message_push_dal
    .push_to_sse(ctx.clone(), user_id, message)
    .await?;

// 3. 合并结果
let mut details = channel_result.details;
details.push(crate::service::dal::message_channel::DeliveryDetail {
    channel_type: "sse".to_string(),
    success: sse_result.success,
    delivered_count: sse_result.delivered_count,
    error: sse_result.error,
});

Ok(DeliveryResult { details })
```

- [ ] **Step 7: 运行测试验证**

Run: `cargo test --package ai-orz --lib service::domain::message -- --nocapture`

Expected: 所有测试通过

- [ ] **Step 8: 提交**

```bash
git add src/service/domain/message/mod.rs src/service/domain/message/delivery.rs
git commit -m "feat: extend MessageDelivery with SSE subscribe and push"
```

---

## Task 4: Handler 层 - SSE 订阅端点

**Files:**
- Create: `src/handlers/finance/message/subscribe_sse.rs`
- Modify: `src/handlers/finance/message/mod.rs`
- Modify: `src/router.rs`

**职责:** 提供 SSE 连接端点，与 finance/message 模块其他路由对齐。

- [ ] **Step 1: 创建 SSE Handler**

```rust
//! SSE 消息推送订阅端点

use axum::{extract::Path, response::sse::{Event, Sse}, response::IntoResponse};
use std::convert::Infallible;
use crate::pkg::RequestContext;

/// SSE 消息推送订阅端点
/// GET /api/v1/finance/messages/sse/{user_id}
pub async fn subscribe_sse_handler(Path(user_id): Path<String>) -> impl IntoResponse {
    let connection_id = uuid::Uuid::new_v4().to_string();
    let ctx = RequestContext::new(None, None);

    let rx = crate::service::domain::message::domain()
        .delivery()
        .subscribe(ctx, &user_id, &connection_id)
        .await
        .unwrap();

    let stream = async_stream::stream! {
        yield Ok(Event::default().event("connected").data(&connection_id));

        loop {
            match rx.recv().await {
                Ok(msg) => yield Ok(Event::default().event("message").data(&msg)),
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
    };

    Sse::new(stream)
}
```

- [ ] **Step 2: 在 message/mod.rs 中导出**

修改 `src/handlers/finance/message/mod.rs`，添加模块和导出：

```rust
pub mod subscribe_sse;
pub use subscribe_sse::subscribe_sse_handler;
```

- [ ] **Step 3: 在 router.rs 中添加路由**

修改 `src/router.rs` 的 `finance_routes()` 函数，在消息相关路由后添加：

```rust
.route(
    "/messages/sse/{user_id}",
    get(handlers::finance::message::subscribe_sse_handler),
)
```

- [ ] **Step 4: 运行编译验证**

Run: `cargo check`

Expected: 编译通过

- [ ] **Step 5: 提交**

```bash
git add src/handlers/finance/message/subscribe_sse.rs src/handlers/finance/message/mod.rs src/router.rs
git commit -m "feat: add SSE subscribe endpoint at /api/v1/finance/messages/sse/{user_id}"
```

---

## Task 5: Consumer 层 - 调用 deliver_message

**Files:**
- Modify: `src/consumer/message.rs`

**职责:** 在消息消费者中调用 deliver_message。

- [ ] **Step 1: 读取现有 consumer/message.rs**

了解当前 `handle_user_message` 的实现。

- [ ] **Step 2: 修改 handle_user_message**

```rust
async fn handle_user_message(&self, message: &Message) -> Result<()> {
    let ctx = self.rebuild_context(message);
    let user_id = &message.po.to_id;

    let cmd = DeliverMessageCommand {
        message,
        user_id,
    };

    let result = self.message_domain.delivery().deliver_message(ctx.clone(), cmd).await?;

    log_info!(
        &ctx,
        "handle_user_message",
        "delivered message {} to user {} via {} channels",
        message.po.id,
        user_id,
        result.details.len()
    );

    Ok(())
}
```

- [ ] **Step 3: 运行编译验证**

Run: `cargo check`

Expected: 编译通过

- [ ] **Step 4: 运行所有测试**

Run: `cargo test`

Expected: 693 个测试全部通过

- [ ] **Step 5: 提交**

```bash
git add src/consumer/message.rs
git commit -m "feat: integrate SSE push in message consumer"
```

---

## 自检清单

| 需求 | 任务 | 状态 |
|------|------|------|
| SSE 推送 DAO（连接管理 + 推送） | Task 1 | ✅ |
| 消息推送 DAL（消息加工） | Task 2 | ✅ |
| MessageDelivery 扩展 | Task 3 | ✅ |
| SSE 端点 Handler | Task 4 | ✅ |
| Consumer 调用推送 | Task 5 | ✅ |

---

**计划完成。**