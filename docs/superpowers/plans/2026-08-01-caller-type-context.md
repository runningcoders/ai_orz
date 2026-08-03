# RequestContext caller_type 字段实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 RequestContext 中新增 `caller_type: CallerType` 字段，显式标识调用方身份（User/Agent/System），替换当前散落在各处的 `ctx.agent_id().is_some()` 隐式推断逻辑。

**Architecture:** 新增 `CallerType` 枚举（User/Agent/System），放入 `common/src/enums/`。RequestContext 结构体 + Builder + from_headers 同步改造。入口层（HTTP 中间件、Consumer rebuild、Producer）显式设置 caller_type，enrich_ctx 链路不覆盖。现有隐式推断点逐步替换为读 `ctx.caller_type`。

**Tech Stack:** Rust (axum + sqlx + tokio), 自研 RequestContext + enrich_ctx 宏

---

## 设计决策（已确认）

1. **caller_type 语义**：表示"谁触发了本次操作"，与现有 `operator_type` 语义一致
2. **默认值**：`User`（与 HTTP 中间件行为一致，HTTP 入口都是用户）
3. **enrich 不覆盖 caller_type**：caller_type 只由入口设置，enrich_ctx 链路透传
4. **与 MessageRole 关系**：数值对齐（0=User, 1=Agent, 2=System），但语义层次不同
5. **与 user_role 关系**：正交概念，user_role 是权限角色（admin/member），不合并
6. **新增工厂方法**：`RequestContext::new_system()` 便捷创建 System ctx

---

## File Structure

| 文件 | 责任 | 改动类型 |
|------|------|---------|
| `common/src/enums/mod.rs` | 注册 CallerType 枚举 | 修改 |
| `common/src/enums/caller_type.rs` | CallerType 枚举定义 | 新建 |
| `src/pkg/request_context.rs` | RequestContext 结构体 + Builder + from_headers | 修改 |
| `src/middleware/jwt_auth.rs` | JWT 注入 X-Caller-Type header | 修改 |
| `src/middleware/request_context.rs` | from_headers 解析 caller_type | 修改 |
| `src/consumer/message.rs` | rebuild_context + handle_tool_call_request 设置 caller_type | 修改 |
| `src/producer/a2a_polling.rs` | System 触发点 | 修改 |
| `src/producer/cron_trigger.rs` | System 触发点 | 修改 |
| `src/producer/message_channel.rs` | System 触发点 | 修改 |
| `src/consumer/scheduler.rs` | System 触发点 | 修改 |
| `src/pkg/aop/core/registry.rs` | System 触发点（4 处） | 修改 |
| `src/pkg/tool_registry/http_fetch.rs` | System 触发点（4 处） | 修改 |
| `src/service/dao/cortex/external.rs` | System 触发点 | 修改 |
| `src/service/dao/agent_runtime/codex.rs` | System 触发点 | 修改 |
| `src/service/dal/lark.rs` | System 触发点 | 修改 |
| `src/service/dal/message_push.rs` | System 触发点 | 修改 |
| `src/service/dao/message_push.rs` | System 触发点 | 修改 |
| `src/handlers/a2a/callback.rs` | A2A callback System 触发 | 修改 |
| `src/handlers/finance/message/send_message_to_agent.rs` | 替换隐式推断 | 修改 |
| `src/handlers/finance/message/send_message.rs` | 替换隐式推断 | 修改 |
| `src/handlers/finance/message/send_task_assignment_message.rs` | 替换隐式推断 | 修改 |
| `src/handlers/project/task/mark_done.rs` | 替换隐式推断 | 修改 |
| `src/service/domain/project/service.rs` | stats operator_type（5 处） | 修改 |
| `src/service/domain/project/task.rs` | stats operator_type（6 处） | 修改 |

---

## Task 1: 新增 CallerType 枚举

**Files:**
- Create: `common/src/enums/caller_type.rs`
- Modify: `common/src/enums/mod.rs`

- [ ] **Step 1: 创建 CallerType 枚举文件**

创建 `common/src/enums/caller_type.rs`：

```rust
//! Caller type enum - 标识 RequestContext 的触发方身份

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[cfg(feature = "sqlx")]
use sqlx::Type;

/// 调用方类型 - 标识谁触发了本次操作
///
/// 语义：caller_type 表示"谁触发了本次操作"，与 stats 的 operator_type 语义一致。
/// 数值与 MessageRole 对齐（0=User, 1=Agent, 2=System），但语义层次不同：
/// - MessageRole 是消息字段（from_role/to_role）
/// - CallerType 是 ctx 字段（标识当前操作链路的触发方）
///
/// 设置时机：
/// - HTTP 中间件：默认 User（JWT 验证通过的用户请求）
/// - Consumer rebuild_context：根据 message.from_role() 设置
/// - Producer/Cron/A2A callback：显式 System
/// - enrich_ctx 链路：不覆盖（透传入口设置的值）
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum CallerType {
    /// User - 用户触发（HTTP 请求、用户消息）
    #[default]
    User = 0,
    /// Agent - Agent 触发（Agent 主动调用工具、发送消息）
    Agent = 1,
    /// System - 系统触发（Cron、A2A 回调、AOP 调度、后台轮询）
    System = 2,
}

impl CallerType {
    /// 转为字符串（用于 stats operator_type）
    pub fn as_str(&self) -> &'static str {
        match self {
            CallerType::User => "User",
            CallerType::Agent => "Agent",
            CallerType::System => "System",
        }
    }
}

impl From<i32> for CallerType {
    fn from(v: i32) -> Self {
        match v {
            0 => CallerType::User,
            1 => CallerType::Agent,
            2 => CallerType::System,
            _ => CallerType::default(),
        }
    }
}

impl From<CallerType> for i32 {
    fn from(c: CallerType) -> i32 {
        c as i32
    }
}

impl std::fmt::Display for CallerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
```

- [ ] **Step 2: 在 enums/mod.rs 注册**

修改 `common/src/enums/mod.rs`，在适当位置添加：

```rust
pub mod caller_type;
pub use caller_type::CallerType;
```

- [ ] **Step 3: 验证编译**

Run: `cargo check -p common`
Expected: 编译通过，无错误

- [ ] **Step 4: Commit**

```bash
git add common/src/enums/caller_type.rs common/src/enums/mod.rs
git commit -m "feat: add CallerType enum (User/Agent/System)"
```

---

## Task 2: RequestContext 结构体 + Builder 改造

**Files:**
- Modify: `src/pkg/request_context.rs`

- [ ] **Step 1: 在 RequestContext 结构体添加 caller_type 字段**

在 `src/pkg/request_context.rs` 的 `RequestContext` 结构体定义中（约第 22-57 行），在 `user_role` 字段后添加：

```rust
    #[log_field] pub user_role: Option<i32>,
    /// 调用方类型（User/Agent/System），默认 User
    #[log_field] pub caller_type: common::enums::CallerType,
```

- [ ] **Step 2: 在 RequestContextBuilder 添加 caller_type 字段**

在 `RequestContextBuilder` 结构体定义中（约第 78-90 行），添加：

```rust
    pub caller_type: Option<CallerType>,
```

同时在 `RequestContextBuilder::new()`（约第 93-107 行）初始化：

```rust
    pub fn new() -> Self {
        Self {
            // ... 现有字段 ...
            caller_type: None,
        }
    }
```

- [ ] **Step 3: 添加 builder 方法**

在 builder 方法区域（约第 109-220 行），添加：

```rust
    /// 设置调用方类型
    pub fn caller_type(mut self, ct: CallerType) -> Self {
        self.caller_type = Some(ct);
        self
    }

    /// 条件设置调用方类型（None 时跳过）
    pub fn try_caller_type(mut self, ct: Option<impl Into<CallerType>>) -> Self {
        if let Some(c) = ct {
            self.caller_type = Some(c.into());
        }
        self
    }
```

- [ ] **Step 4: 修改 build 方法**

在 `build` 方法中（约第 227-241 行），添加 caller_type 字段：

```rust
    pub fn build(self) -> RequestContext {
        RequestContext {
            // ... 现有字段 ...
            caller_type: self.caller_type.unwrap_or_default(),
        }
    }
```

- [ ] **Step 5: 修改 to_builder 方法**

在 `to_builder` 方法中（约第 257-271 行），保留 caller_type：

```rust
    pub fn to_builder(&self) -> RequestContextBuilder {
        RequestContextBuilder {
            // ... 现有字段 ...
            caller_type: Some(self.caller_type),
        }
    }
```

- [ ] **Step 6: 添加 caller_type 访问方法**

在 RequestContext 的 impl 块中添加：

```rust
    /// 获取调用方类型
    pub fn caller_type(&self) -> CallerType {
        self.caller_type
    }
```

- [ ] **Step 7: 修改 from_headers 方法**

在 `from_headers` 方法中（约第 274-321 行），添加 caller_type 解析逻辑。在现有 user_role 解析后添加：

```rust
    // 解析 caller_type：优先从 X-Caller-Type header 读取
    // 默认 User（HTTP 入口都是用户请求）
    let caller_type = headers
        .get(http_header::CALLER_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| match s.to_lowercase().as_str() {
            "user" | "0" => CallerType::User,
            "agent" | "1" => CallerType::Agent,
            "system" | "2" => CallerType::System,
            _ => CallerType::User,
        })
        .unwrap_or(CallerType::User);
```

然后在 builder 链中添加 `.caller_type(caller_type)`。

- [ ] **Step 8: 新增 new_system 工厂方法**

在 RequestContext 的 impl 块中添加：

```rust
    /// 创建 System 调用方的 ctx（用于 Cron、A2A 回调、AOP 调度等系统触发场景）
    pub fn new_system() -> Self {
        Self::builder().caller_type(CallerType::System).build()
    }
```

- [ ] **Step 9: 添加 http_header 常量**

在 `src/pkg/http_header.rs`（或 request_context.rs 中 http_header 模块）添加：

```rust
    pub const CALLER_TYPE: HeaderName = HeaderName::from_static("x-caller-type");
```

- [ ] **Step 10: 验证编译**

Run: `cargo check -p ai_orz`
Expected: 编译通过（可能有 unused warning，后续 task 会用到）

- [ ] **Step 11: Commit**

```bash
git add src/pkg/request_context.rs src/pkg/http_header.rs
git commit -m "feat: add caller_type field to RequestContext + Builder + from_headers"
```

---

## Task 3: HTTP 中间件注入 caller_type

**Files:**
- Modify: `src/middleware/jwt_auth.rs`
- Modify: `src/middleware/request_context.rs`

- [ ] **Step 1: JWT 中间件注入 X-Caller-Type header**

在 `src/middleware/jwt_auth.rs` 的第 56-80 行区域，JWT 解码成功后注入用户信息时，添加 caller_type header：

```rust
    // 3. 将用户信息添加到请求头
    if !claims.user_id.is_empty()
        && let Ok(header_value) = HeaderValue::from_str(&claims.user_id)
    {
        req.headers_mut().insert(http_header::USER_ID, header_value);
    }
    // ... 现有 username / organization_id / role 注入 ...

    // 注入 caller_type = User（JWT 验证通过的都是用户请求）
    if let Ok(header_value) = HeaderValue::from_static("user") {
        req.headers_mut().insert(http_header::CALLER_TYPE, header_value);
    }
```

- [ ] **Step 2: 验证 from_headers 已支持 caller_type**

确认 Task 2 的 Step 7 已让 `from_headers` 读取 `X-Caller-Type` header。`request_context.rs` 中间件无需额外改动，因为它已调用 `RequestContext::from_headers(headers)`。

- [ ] **Step 3: 验证编译 + 运行现有测试**

Run: `cargo check -p ai_orz && cargo test -p ai_orz --lib request_context`
Expected: 编译通过，现有测试不破坏

- [ ] **Step 4: Commit**

```bash
git add src/middleware/jwt_auth.rs
git commit -m "feat: JWT middleware injects X-Caller-Type=user header"
```

---

## Task 4: Consumer rebuild_context 设置 caller_type

**Files:**
- Modify: `src/consumer/message.rs`

- [ ] **Step 1: 修改 rebuild_context 根据 from_role 设置 caller_type**

在 `src/consumer/message.rs` 的 `rebuild_context` 方法中（约第 472-494 行），改造为：

```rust
fn rebuild_context(&self, message: &Message) -> RequestContext {
    let mut builder = RequestContext::builder();
    if let Some(org_id) = &message.po.organization_id {
        builder = builder.organization_id(org_id.clone());
    }
    // 根据 from_role 设置 caller_type 和 user_id
    let from_role = message.from_role();
    builder = builder.caller_type(match from_role {
        MessageRole::User => CallerType::User,
        MessageRole::Agent => CallerType::Agent,
        MessageRole::System => CallerType::System,
    });
    if from_role == MessageRole::User {
        builder = builder.user_id(message.po.from_id.clone());
    }
    if let Some(project_id) = &message.po.project_id {
        builder = builder.project_id(project_id.clone());
    }
    if let Some(task_id) = &message.po.task_id {
        builder = builder.task_id(task_id.clone());
    }
    builder = builder.agent_id(message.po.to_id.clone());
    builder.build()
}
```

- [ ] **Step 2: 修改 handle_tool_call_request 设置 caller_type**

在 `src/consumer/message.rs` 的 `handle_tool_call_request` 方法中（约第 397-429 行），在 builder 链中添加：

```rust
    let mut builder = RequestContext::builder();
    builder = builder.agent_id(tool_call.from_id.clone());
    // ToolCallRequest 一定由 Agent 发起
    builder = builder.caller_type(CallerType::Agent);
    // ... 后续 project_id / task_id / org_id / user_id / model 字段 ...
```

- [ ] **Step 3: 修改 on_event / ack / nack 入口 ctx**

在 `src/consumer/message.rs` 中所有 `RequestContext::new(None, None)` 调用点（约第 82、114、122 行），改为：

```rust
    let ctx = RequestContext::new_system();
```

- [ ] **Step 4: 验证编译**

Run: `cargo check -p ai_orz`
Expected: 编译通过

- [ ] **Step 5: Commit**

```bash
git add src/consumer/message.rs
git commit -m "feat: consumer rebuild_context sets caller_type from message from_role"
```

---

## Task 5: Producer / Cron / AOP System 触发点

**Files:**
- Modify: `src/producer/a2a_polling.rs`
- Modify: `src/producer/cron_trigger.rs`
- Modify: `src/producer/message_channel.rs`
- Modify: `src/consumer/scheduler.rs`
- Modify: `src/pkg/aop/core/registry.rs`
- Modify: `src/pkg/tool_registry/http_fetch.rs`
- Modify: `src/service/dao/cortex/external.rs`
- Modify: `src/service/dao/agent_runtime/codex.rs`
- Modify: `src/service/dal/lark.rs`
- Modify: `src/service/dal/message_push.rs`
- Modify: `src/service/dao/message_push.rs`

- [ ] **Step 1: 替换所有 RequestContext::new(None, None) 为 new_system()**

在以下文件中，将所有 `RequestContext::new(None, None)` 替换为 `RequestContext::new_system()`：

1. `src/producer/a2a_polling.rs:61` - A2A 轮询入口
2. `src/producer/cron_trigger.rs:52` - Cron 触发器
3. `src/producer/message_channel.rs:27` - 消息渠道适配器
4. `src/consumer/scheduler.rs:118` - CronTriggerConsumer
5. `src/pkg/aop/core/registry.rs:198, 221, 239, 256` - AOP 调度器（4 处）
6. `src/pkg/tool_registry/http_fetch.rs:165, 191, 213, 234` - HTTP fetch 工具（4 处）
7. `src/service/dao/cortex/external.rs:109` - 外部 Cortex 调用
8. `src/service/dao/agent_runtime/codex.rs:108` - CLI 执行日志
9. `src/service/dal/lark.rs:313` - Lark 事件处理
10. `src/service/dal/message_push.rs:126` - SSE 推送
11. `src/service/dao/message_push.rs:147` - SSE 推送 DAO

对于 `src/producer/message_channel.rs:27`，需要额外设置 user_id（从 msg.from_id）：

```rust
async fn on_message(&self, msg: AdaptedMessage) -> Result<()> {
    let ctx = RequestContext::builder()
        .caller_type(CallerType::User)
        .try_user_id(msg.from_id.as_deref())
        .build();
```

- [ ] **Step 2: A2A polling 的 task_ctx 设置 caller_type**

在 `src/producer/a2a_polling.rs:130-136`，task_ctx 构造时添加 caller_type：

```rust
    let task_ctx = RequestContext::builder()
        .caller_type(CallerType::System)
        .agent_id(agent.po.id.clone())
        .task_id(task.po.id.clone())
        .try_project_id(task.po.project_id.as_deref())
        .build();
```

- [ ] **Step 3: 验证编译**

Run: `cargo check -p ai_orz`
Expected: 编译通过

- [ ] **Step 4: Commit**

```bash
git add src/producer/ src/consumer/scheduler.rs src/pkg/aop/ src/pkg/tool_registry/ src/service/dao/ src/service/dal/
git commit -m "feat: system trigger points use RequestContext::new_system()"
```

---

## Task 6: A2A callback handler 设置 caller_type

**Files:**
- Modify: `src/handlers/a2a/callback.rs`

- [ ] **Step 1: callback handler 中 task_ctx 设置 System**

在 `src/handlers/a2a/callback.rs:56-62`，task_ctx 构造时添加：

```rust
    let task_ctx = RequestContext::builder()
        .caller_type(CallerType::System)
        .agent_id(agent_id.clone())
        .task_id(task_id.clone())
        .try_project_id(local_task.po.project_id.as_deref())
        .build();
```

- [ ] **Step 2: 验证 A2A callback 路由是否经过 JWT 中间件**

检查 `src/router.rs` 中 `/a2a/` 路由的中间件层叠。如果 callback 端点经过 JWT 中间件，需要确认：
- JWT 中间件会注入 `X-Caller-Type: user`
- from_headers 会读取该 header 设为 User
- callback handler 内部用 `to_builder().caller_type(CallerType::System).build()` 覆盖

如果 callback 端点不经过 JWT（公开端点），from_headers 默认 User，仍需在 handler 内覆盖为 System。

- [ ] **Step 3: 验证编译**

Run: `cargo check -p ai_orz`
Expected: 编译通过

- [ ] **Step 4: Commit**

```bash
git add src/handlers/a2a/callback.rs
git commit -m "feat: A2A callback handler sets caller_type=System"
```

---

## Task 7: 替换 handler 层隐式推断（消息发送类）

**Files:**
- Modify: `src/handlers/finance/message/send_message_to_agent.rs`
- Modify: `src/handlers/finance/message/send_message.rs`
- Modify: `src/handlers/finance/message/send_task_assignment_message.rs`
- Modify: `src/handlers/project/task/mark_done.rs`

- [ ] **Step 1: 改造 send_message_to_agent**

在 `src/handlers/finance/message/send_message_to_agent.rs:34-41`，替换三段式 if-else：

```rust
    // 改造前：
    // let (from_id, from_role) = if let Some(aid) = ctx.agent_id() {
    //     (aid.to_string(), MessageRole::Agent)
    // } else if !ctx.uid().is_empty() {
    //     (ctx.uid(), MessageRole::User)
    // } else {
    //     ("system".to_string(), MessageRole::System)
    // };

    // 改造后：直接从 ctx.caller_type 读取
    let (from_id, from_role) = match ctx.caller_type() {
        CallerType::Agent => (
            ctx.agent_id().map(|s| s.to_string()).unwrap_or_default(),
            MessageRole::Agent,
        ),
        CallerType::User => (
            ctx.uid().to_string(),
            MessageRole::User,
        ),
        CallerType::System => ("system".to_string(), MessageRole::System),
    };
```

- [ ] **Step 2: 改造 send_message**

在 `src/handlers/finance/message/send_message.rs:23-26`，替换 Agent 优先 fallback：

```rust
    // 改造前：
    // let from_agent_id = ctx
    //     .agent_id()
    //     .map(|s| s.to_string())
    //     .unwrap_or_else(|| "system".to_string());

    // 改造后：根据 caller_type 选择 from_id 来源
    let from_agent_id = match ctx.caller_type() {
        CallerType::Agent => ctx.agent_id().map(|s| s.to_string()).unwrap_or_default(),
        CallerType::User => ctx.uid().to_string(),
        CallerType::System => "system".to_string(),
    };
```

- [ ] **Step 3: 改造 send_task_assignment_message**

在 `src/handlers/finance/message/send_task_assignment_message.rs:26-35`，替换双分支判断：

```rust
    // 改造前：
    // let from_id = ctx
    //     .agent_id()
    //     .map(|s| s.to_string())
    //     .unwrap_or_else(|| ctx.uid().to_string());
    // let from_role = if ctx.agent_id().is_some() {
    //     common::enums::MessageRole::Agent
    // } else {
    //     common::enums::MessageRole::User
    // };

    // 改造后：补齐 System 分支
    let (from_id, from_role) = match ctx.caller_type() {
        CallerType::Agent => (
            ctx.agent_id().map(|s| s.to_string()).unwrap_or_default(),
            common::enums::MessageRole::Agent,
        ),
        CallerType::User => (ctx.uid().to_string(), common::enums::MessageRole::User),
        CallerType::System => (
            "system".to_string(),
            common::enums::MessageRole::System,
        ),
    };
```

- [ ] **Step 4: 改造 mark_done**

在 `src/handlers/project/task/mark_done.rs:27-31`，替换 fallback 逻辑：

```rust
    // 改造前：
    // let modified_by = ctx
    //     .agent_id()
    //     .cloned()
    //     .or_else(|| ctx.user_id().cloned())
    //     .unwrap_or_else(|| "system".to_string());

    // 改造后：根据 caller_type 选择 modified_by
    let modified_by = match ctx.caller_type() {
        CallerType::Agent => ctx.agent_id().cloned().unwrap_or_default(),
        CallerType::User => ctx.uid().to_string(),
        CallerType::System => "system".to_string(),
    };
```

- [ ] **Step 5: 验证编译 + 运行测试**

Run: `cargo check -p ai_orz && cargo test -p ai_orz --lib --tests`
Expected: 编译通过，现有测试不破坏

- [ ] **Step 6: Commit**

```bash
git add src/handlers/finance/message/ src/handlers/project/task/mark_done.rs
git commit -m "refactor: replace implicit caller inference with ctx.caller_type in message handlers"
```

---

## Task 8: 替换 stats operator_type 推断

**Files:**
- Modify: `src/service/domain/project/service.rs`
- Modify: `src/service/domain/project/task.rs`

- [ ] **Step 1: 改造 project/service.rs 的 5 处 operator_type**

在 `src/service/domain/project/service.rs` 中，找到所有（5 处，约第 62-67、212-217、259-264、306-311、438-443 行）如下模式：

```rust
    // 改造前：
    operator_type: Some(if ctx.agent_id().is_some() {
        "Agent".to_string()
    } else {
        "User".to_string()
    }),
    operator_id: ctx.agent_id().cloned().or_else(|| ctx.user_id().cloned()),
```

替换为：

```rust
    // 改造后：直接用 ctx.caller_type
    operator_type: Some(ctx.caller_type().as_str().to_string()),
    operator_id: match ctx.caller_type() {
        CallerType::Agent => ctx.agent_id().cloned(),
        CallerType::User => ctx.user_id().cloned(),
        CallerType::System => None,
    },
```

- [ ] **Step 2: 改造 project/task.rs 的 6 处 operator_type**

在 `src/service/domain/project/task.rs` 中，找到所有（6 处，约第 95-100、297-302、342-347、382-387、476-481、522-527 行）相同模式，用相同方式替换。

- [ ] **Step 3: 验证编译 + 运行测试**

Run: `cargo check -p ai_orz && cargo test -p ai_orz --lib --tests`
Expected: 编译通过，现有测试不破坏

- [ ] **Step 4: Commit**

```bash
git add src/service/domain/project/service.rs src/service/domain/project/task.rs
git commit -m "refactor: stats operator_type uses ctx.caller_type instead of implicit inference"
```

---

## Task 9: 最终验证与推送

**Files:**
- 无文件改动，仅验证

- [ ] **Step 1: 全量编译检查**

Run: `cargo check --all-targets`
Expected: 编译通过

- [ ] **Step 2: clippy 检查**

Run: `cargo clippy -p ai_orz --lib --tests -- -D warnings && cargo clippy -p frontend --target wasm32-unknown-unknown --all-targets -- -D warnings`
Expected: 无 warning

- [ ] **Step 3: fmt 检查**

Run: `cargo fmt --all -- --check`
Expected: 无 diff

- [ ] **Step 4: 全量测试**

Run: `cargo test -p ai_orz --lib --tests`
Expected: 现有测试全部通过

- [ ] **Step 5: 推送**

```bash
git push origin main
```

---

## Self-Review 清单

### Spec coverage

- [x] 新增 CallerType 枚举（User/Agent/System）→ Task 1
- [x] RequestContext 结构体 + Builder + from_headers 改造 → Task 2
- [x] HTTP 中间件默认 User → Task 3
- [x] Consumer rebuild_context 根据 from_role 设置 → Task 4
- [x] Producer/Cron/AOP System 触发点 → Task 5
- [x] A2A callback System 触发 → Task 6
- [x] 替换 handler 层隐式推断 → Task 7
- [x] 替换 stats operator_type 推断 → Task 8
- [x] 最终验证 → Task 9

### 关键设计原则

1. **enrich_ctx 不覆盖 caller_type**：所有 EnrichContext 实现保持不变，caller_type 只由入口设置
2. **默认 User**：from_headers 无 X-Caller-Type header 时默认 User
3. **new_system() 工厂**：便捷创建 System ctx，替换所有 `RequestContext::new(None, None)`
4. **caller_type 透传**：to_builder 保留 caller_type，enrich_ctx 不覆盖

### Type consistency

- `CallerType` 枚举：User=0, Agent=1, System=2（与 MessageRole 数值对齐）
- `ctx.caller_type()` 方法：返回 `CallerType`
- `ctx.caller_type().as_str()`：返回 `"User"` / `"Agent"` / `"System"`
- `RequestContext::new_system()`：返回 `caller_type=System` 的 ctx
- Builder 方法：`caller_type(CallerType)` 和 `try_caller_type(Option<impl Into<CallerType>>)`
