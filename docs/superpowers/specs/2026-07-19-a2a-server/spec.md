# A2A Server 推进方案 Spec

> 配套文档：
> - 设计文档：[docs/a2a_server_design.md](../../a2a_server_design.md)
> - 实施计划：[docs/superpowers/plans/2026-07-19-a2a-server.md](../plans/2026-07-19-a2a-server.md)
> - 验收清单：[checklist.md](./checklist.md)
> - 任务分解：[tasks.md](./tasks.md)

## 1. 目标

让 ai_orz 作为 A2A Server 对外暴露 JSON-RPC 2.0 端点，外部注册用户持 JWT 通过 A2A 协议调用前台 Agent。

同时修复 chat 页面无法发送消息的设计缺陷，将 Agent 路由逻辑收敛为统一 domain 方法。

## 2. 范围

### P0 必交付

- **统一 Agent 路由**：`HrDomain::resolve_agent(ctx)` 兜底路由方法（只接受 ctx，不耦合 project）
- **前台 Agent 查询 API**：`GET /api/v1/hr/agents/reception`（JWT 保护），供前端显示推荐前台 Agent
- **chat 缺陷修复**：`CreateProjectRequest` 支持 `owner_agent_id`、`send_message_to_agent` 支持两种对话上下文（默认对话框无 project + Project 对话框）
- **chat 默认对话框不创建 project**：默认对话框直接发消息（to_agent_id 未指定时后端走 resolve_agent 兜底）；Project 创建由 Agent 内部决策触发，不在本次范围
- **A2A 协议实体**：AgentCard / JsonRpc / A2aTask / A2aMessage 等类型定义
- **A2A 端点**：
  - `GET /.well-known/agent.json`（公开）
  - `POST /a2a` JSON-RPC（JWT 保护）
- **A2A 方法**：`tasks/send`（异步提交）、`tasks/get`（轮询查询）、`tasks/cancel`（取消）
- **集成测试**：覆盖路由、查询不存在、取消不存在等场景

### P1 不在本次范围

- `tasks/sendSubscribe` SSE 流式
- 长任务异步模式
- 推送通知（PushNotifications）

## 3. 架构原则

| 原则 | 说明 |
|------|------|
| Domain 层零侵入 | A2A 协议实体 ↔ ai_orz 实体转换全部在 handler 层 `mapper.rs`，domain 层不感知 A2A |
| Agent 与 Project 维度分离 | `resolve_agent(ctx)` 只返回可用前台 Agent，不感知/查询 project；两个维度由 handler/前端按需组合 |
| 复用而非新增 | 复用现有 JWT 认证、`resolve_agent` 路由、awaken 链路、project/message 管理 |
| 无新增表 | 认证走用户 JWT，A2A task 对应现有 project 表 |
| 统一路由 | Chat 页面、A2A、飞书 IM 三场景共用 `HrDomain::resolve_agent` |
| 协作关系类比（核心理念） | 默认对话框=与前台直接沟通（无 project）；Project 对话框=Agent 识别复杂需求后创建 Project 的上下文沟通。Project 创建由 Agent 内部决策触发，不在前端显式创建 |
| to_agent_id 保持 Option | 用户选定 Agent 时前端显式传入；未指定时后端走 `resolve_agent(ctx)` 兜底 |
| handler 层可调 resolve_agent | resolve_agent 本就是给 handler 用的统一路由方法，handler 层在默认对话框或 project 未绑定 agent 时调 resolve_agent 兜底 |
| tasks/send 异步流程 | handler 只创建 project + message 后立即返回 working 状态，唤醒由 consumer 异步闭环（复用现有链路），客户端通过 tasks/get 轮询。与飞书/前端聊天的唤醒链路完全一致 |

## 4. 统一 Agent 路由策略

`resolve_agent(ctx) -> Result<Option<Agent>>` 路由策略：

**核心原则**：**agent 与 project 是两个维度，不在 hr domain 中融合**。`resolve_agent` 只接受 `ctx`，只负责返回当前可用的前台 Agent，不查询/感知 project。两个维度由调用方（handler 层或前端）按需组合。

**`resolve_agent` 内部路由优先级**：
1. **优先级 1**：`feishu_reception` 角色 Onboarded Agent
2. **优先级 2**：任意 Onboarded Agent

返回 `None` 表示无可用 Agent。

### 前台 Agent 查询 HTTP API

`GET /api/v1/hr/agents/reception`（JWT 保护）：
- 调用 `resolve_agent(ctx)` 返回 `{ agent_id, agent_name }`
- 供前端显示推荐前台 Agent（如默认对话框顶部显示"当前前台：XXX"），或作为用户选定 Agent 的默认选项
- 无可用 agent 时返回 404

### 调用方

| 场景 | 调用方逻辑 | resolve_agent 调用 |
|------|----------------|---------------------|
| 飞书 IM 接收消息 | 后端 consumer 无 project | `resolve_agent(ctx)` |
| chat 默认对话框（无 project） | 前端不创建 project，直接发消息；`to_agent_id` 用户选定时显式传入，未选定时后端兜底 | `resolve_agent(ctx)`（to_agent_id 未指定时） |
| chat Project 对话框（有 project） | handler 查 project，用 `project.owner_agent_id`；若为 None 则调 `resolve_agent(ctx)` 兜底 | `resolve_agent(ctx)`（project.owner_agent_id 为 None 时） |
| A2A `tasks/send`（send_task handler） | handler 调 `resolve_agent(ctx)` 获取 agent → 用 `agent.id` 作为 `owner_agent_id` 创建 project 绑定 | `resolve_agent(ctx)` |
| `send_message_to_agent` handler | `to_agent_id` 显式指定优先；否则从 `project.owner_agent_id` 取；若为 None 或无 project_id 则调 `resolve_agent(ctx)` 兜底 | `resolve_agent(ctx)`（兜底场景） |
| `create_project` handler | 纯粹透传 `owner_agent_id`，**不调 resolve_agent**，不依赖 hr domain | 不调用 |

**协作关系类比说明**：
- **默认对话框** = 与前台 Agent 直接沟通（无 project 上下文）：用户可选定 Agent（to_agent_id 显式传入），未选定时后端走 `resolve_agent(ctx)` 兜底
- **Project 对话框** = Agent 识别复杂需求后创建 Project 的上下文沟通：Project 创建由 Agent 内部决策触发，不在本次 A2A Server 范围
- 类比找公司团队帮忙：前台接待 → 简单需求前台直接处理 → 复杂需求交 PMO 创建 Project

## 5. 文件结构

### 新增文件（12 个）

| 文件 | 职责 |
|------|------|
| `common/src/api/a2a.rs` | A2A 协议实体定义 |
| `common/src/api/a2a_test.rs` | 协议实体序列化测试 |
| `src/handlers/a2a/mod.rs` | A2A handler 模块入口 |
| `src/handlers/a2a/mapper.rs` | A2A ↔ ai_orz 实体转换（纯函数） |
| `src/handlers/a2a/mapper_test.rs` | mapper 单元测试 |
| `src/handlers/a2a/agent_card.rs` | Agent Card 端点 |
| `src/handlers/a2a/jsonrpc.rs` | JSON-RPC 入口 + 方法分发 |
| `src/handlers/a2a/send_task.rs` | `tasks/send` 同步执行 |
| `src/handlers/a2a/get_task.rs` | `tasks/get` 查询 |
| `src/handlers/a2a/cancel_task.rs` | `tasks/cancel` 取消 |
| `src/handlers/a2a/integration_test.rs` | 集成测试 |
| `src/handlers/hr/agent/get_reception_agent.rs` | `GET /api/v1/hr/agents/reception` 前台 Agent 查询 handler |

### 修改文件（15 个）

| 文件 | 改动 |
|------|------|
| `common/src/api/mod.rs` | 注册 `a2a` 模块 |
| `common/src/api/project.rs` | `CreateProjectRequest` 新增 `owner_agent_id` |
| `common/src/api/neural_tools.rs` | `to_agent_id` 改为 `Option<String>` |
| `common/src/config.rs` | 新增 `A2aServerConfig` 配置段 |
| `src/handlers/mod.rs` | 注册 `a2a` handler 模块 |
| `src/handlers/hr/agent/mod.rs` | 注册 `get_reception_agent` handler |
| `src/handlers/project/project/create_project.rs` | handler 纯粹透传 `owner_agent_id`，不调 resolve_agent |
| `src/handlers/finance/message/send_message_to_agent.rs` | handler 支持默认对话框（无 project）+ to_agent_id 未指定时 resolve_agent 兜底 |
| `src/service/domain/hr/mod.rs` | `HrDomain` trait 新增 `resolve_agent(ctx)` + `#[async_trait]` |
| `src/service/domain/project/mod.rs` | `ProjectManage::create` 新增 `owner_agent_id` 参数 |
| `src/service/domain/project/project.rs` | `ProjectDomainImpl::create` 直接绑定 `owner_agent_id` |
| `src/consumer/adapter.rs` | `find_reception_agent_id` 改为调用 `resolve_agent(ctx)` |
| `src/router.rs` | 注册 A2A 路由 + `GET /api/v1/hr/agents/reception` |
| `frontend/src/api/mod.rs` | 新增 `get_reception_agent_api` 函数 |
| `frontend/src/pages/message/chat.rs` | 默认对话框不创建 project，直接发消息；Project 对话框用 project_id |

### 不改动

- `src/service/domain/message/`
- `src/service/domain/runtime/`
- 数据库 schema

## 6. 技术栈

- Rust + Axum + serde + tokio + sqlx
- `async_trait` crate（`HrDomain` trait 新增 async 方法需要）
- `chrono::Utc::now().to_rfc3339()` 生成 ISO 8601 时间戳
- `char_indices().nth(50)` UTF-8 安全的字符级字符串截断

## 7. 关键约束

| 约束 | 来源 |
|------|------|
| JWT 中间件必须先于 RequestContext 中间件执行 | project_memory |
| 公开路由和受保护路由各有自己的 `request_context_middleware` 层 | project_memory |
| `AgentPo` 必须包含 `organization_id` 和 `root_id` | project_memory |
| `RequestContext` 一旦构造即不可变 | project_memory |
| Context 补充只用当前业务已有信息，不为此专门查询 | user_profile |
| DAL 层只暴露简洁接口，不调用其他 DAL | project_memory |

## 8. A2A 协议要点

- **协议版本**：v0.3.0
- **传输**：JSON-RPC 2.0 over HTTP POST
- **认证**：JWT（与现有用户 JWT 共用，HttpOnly Cookie）
- **任务映射**：A2A Task ↔ ai_orz Project
- **消息映射**：A2A Message ↔ ai_orz MessagePo
- **角色映射**：`from_role=User → role="user"`，其余 → `role="agent"`
- **状态映射**：
  - `Active` / `PendingReview` → `Submitted`
  - `InProgress` → `Working`
  - `Completed` → `Completed`
  - `Archived` → `Canceled`
  - `Deleted` → `Failed`

## 9. 验收标准概述

详见 [checklist.md](./checklist.md)。核心验收点：

- 统一 `resolve_agent(ctx)` 方法存在，**只接受 ctx，不耦合 project**（agent 与 project 是两个维度）
- 新增 `GET /api/v1/hr/agents/reception` HTTP API，前端可显示推荐前台 Agent
- `create_project` handler 纯粹透传 `owner_agent_id`，**不调 resolve_agent，不依赖 hr domain**
- `send_message_to_agent` handler 支持**两种对话上下文**：默认对话框（无 project）+ Project 对话框（有 project）；`to_agent_id` 未指定时后端走 `resolve_agent(ctx)` 兜底
- chat 默认对话框**不创建 project**，直接发消息；Project 创建由 Agent 内部决策触发，不在本次范围
- `to_agent_id` 保持 `Option<String>`：用户选定 Agent 时前端显式传入，未指定时后端兜底
- A2A `tasks/send` 走异步流程：handler 层 `resolve_agent(ctx)` 获取 agent → 创建 project 绑定 agent.id → 创建 message（自动入队）→ 立即返回 working 状态；唤醒由 consumer 异步闭环（复用现有链路），客户端通过 `tasks/get` 轮询结果
- A2A `tasks/get` / `tasks/cancel` 正确处理不存在 task 的错误
- 全量测试 PASS，无回归

## 10. 执行策略

按 [tasks.md](./tasks.md) 的顺序执行：

- **Phase 0**（路由收敛 + chat 缺陷修复）：Task 0.1 → 0.2 → 0.3
- **Phase 1**（A2A 基础设施）：Task 1 → 2 → 4 → 5
- **Phase 2**（A2A 方法实现）：Task 6 → 7 → 8 → 9
- **Phase 3**（路由 + 测试 + 文档）：Task 10 → 11 → 12

每个 Task 完成后单独提交，Phase 切换时做编译 + 测试验证。
