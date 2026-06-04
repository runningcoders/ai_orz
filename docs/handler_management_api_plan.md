# Handler 管理面 API 补齐方案

> 记录日期：2026-06-04  
> 背景：Domain 层已经沉淀了较多管理维度能力，但 Handler / Router 暴露不完整。本文记录管理面 CRUD Handler 的补齐边界、覆盖矩阵和分阶段实施计划。

---

## 1. 设计原则

### 1.1 Handler 与用户 Action 对应

Handler 是用户 Action / HTTP API 的入口层，一个接口按自身需求完成请求级编排即可，不做通用 Handler 框架抽象。

Handler 负责：
- 解析 API DTO 与路径/query 参数；
- 从 `RequestContext` 补全组织、用户等请求上下文；
- 将 API DTO 转换为 Domain 的 Command / Query / Params；
- 按当前用户 Action 编排一个或多个 Domain 调用；
- 将业务实体转换为 Response DTO。

Handler 禁止：
- 直接调用 DAL / DAO；
- 承载复杂业务规则、状态流转规则、权限语义；
- Handler 间互调复用逻辑；
- 抽象 `BaseHandler` / `GenericCrudHandler` / `GenericActionHandler`。

复用优先通过两种方式完成：
1. 抽取清晰的 Command / Query / Params 参数对象；
2. 复用 Domain 能力或将共享流程下沉为更明确的 Domain 方法。

### 1.2 按 Domain 能力补接口，不按表结构生成接口

补 Handler 前先盘点 Domain 管理对象和已存在能力，再对照 `src/handlers/**` 与 `src/router.rs` 的实际暴露情况。不要从 DAO/DAL 表结构直接生成 CRUD 路由，避免把持久化细节泄漏到 API 层。

### 1.3 区分管理面与运行面

| 类型 | 范围 | 是否纳入本轮 |
|------|------|--------------|
| 管理面 | 配置、元数据、绑定关系、列表查询、状态更新 | ✅ 纳入 |
| 运行面 | 队列消费、消息投递、Agent 唤醒、模型推理、工具执行 | ❌ 不纳入第一批 |

本轮优先补“可以由用户在管理界面直接操作”的能力；运行时链路后续按 Runtime / Consumer 设计单独推进。

### 1.4 状态变更统一使用状态更新接口

状态变更不再按每个目标状态拆独立路由，例如不新增 `/start`、`/complete`、`/archive`、`/cancel`、`/enable`、`/disable` 这类每状态一路由。

统一模式：

```http
PUT /api/v1/projects/{id}/status
PUT /api/v1/tasks/{id}/status
PUT /api/v1/agents/{id}/status
PUT /api/v1/tools/{id}/status
```

请求体携带目标状态：

```json
{
  "status": "Active"
}
```

状态合法性与状态流转规则由 Domain 校验。这样后续状态枚举扩展时只扩展 enum / Domain 校验，不导致路由膨胀。

### 1.5 敏感信息响应策略

`MessageChannel`、`Tool`、`ModelProvider` 等可能包含密钥、Token、Header、Webhook secret、connection string 的配置对象，Handler Response DTO 不应原样返回敏感字段。

建议响应策略：
- 写入时允许接收配置字段；
- 查询/列表时返回脱敏值或 `has_secret: bool`；
- 文档、日志、错误信息中不得输出真实 secret；
- 如必须示例化敏感值，统一写为 `[REDACTED]`。

---

## 2. 当前覆盖情况

### 2.1 已覆盖较完整的管理面

| Domain | 对象 | 当前 Handler 覆盖 |
|--------|------|-------------------|
| `organization` | Organization / User / Auth / Profile | 已覆盖初始化、组织 CRUD、用户 CRUD、登录、当前用户/组织 |
| `hr` | Agent | 已覆盖 create/get/list/update/delete；状态更新接口待补 |
| `finance` | ModelProvider | 已覆盖 create/get/list/update/delete/test/call |

### 2.2 待补管理面能力矩阵

| 优先级 | Domain | 管理对象 | Domain 已有能力 | Handler 状态 | 说明 |
|--------|--------|----------|-----------------|----------------|------|
| P0 | `finance` | MessageChannel | create/get/query/list/update/delete/test | 已补 create/list/get/update/delete/status/test | 纯配置类，收益高；响应 DTO 已脱敏；状态更新统一 `/status`，测试连接统一 `/test` |
| P0 | `finance` | Tool | create/get/query/list/update/bind/unbind/list_agent_tools/search | 缺失 | 先补基础管理与 Agent 绑定；工具执行不纳入本轮 |
| P0 | `hr` | Agent Status | transition_status/validate_onboard_readiness | 缺失 | 使用统一 `PUT /agents/{id}/status` |
| P1 | `project` | Project | create/get/list_by_user/update_basic/start/complete/archive | 缺失 | 状态方法先在 Handler 层收敛成统一 status action，必要时再补 Domain 统一入口 |
| P1 | `project` | Task | create/get/list_by_project/list_by_agent/start/complete/cancel | 缺失 | 同 Project，统一 status action |
| P1 | `hr` | Skill | create/get/update/delete/query/list/search/install_to_agent/list_for_agent | 缺失 | 涉及文件内容，先补元数据与主内容管理 |
| P2 | `project` | Artifact | create_project_artifact/create_task_artifact/get/list/delete | 缺失 | 受上传/附件存储机制影响，放后 |
| P2 | `message` | MessageManagement | query/list_by_task/list_by_project/get/update_status/delete/cleanup | 缺失 | 只补管理查询与状态，不做投递消费 |
| 暂缓 | `message` | MessageDelivery | send/dequeue/ack/nack/deliver | 不作为 CRUD 补齐 | 运行面能力，跟 Consumer / Runtime 链路单独推进 |
| 暂缓 | `runtime` | Awakening / Brain Think / Tool Execution | awaken/think/tool execution | 不作为 CRUD 补齐 | 运行面能力，不混入管理面接口 |

---

## 3. 推荐路由草案

> 路由命名以用户 Action 清晰为优先，下面为实施时的参考草案；具体字段以 `common/src/api/*` DTO 落地为准。

### 3.1 MessageChannel（P0）

```http
POST   /api/v1/finance/message-channels
GET    /api/v1/finance/message-channels
GET    /api/v1/finance/message-channels/{id}
PUT    /api/v1/finance/message-channels/{id}
DELETE /api/v1/finance/message-channels/{id}
PUT    /api/v1/finance/message-channels/{id}/status
POST   /api/v1/finance/message-channels/{id}/test
```

当前已落地以上七个管理面路由（create/list/get/update/delete/status/test）。查询列表支持 query 参数承载筛选条件，例如 `user_id`、`agent_id`、`channel_type`、`only_enabled`、`limit`、`offset`。响应 DTO 使用 `has_access_token`、`has_secret`、`has_config_secret` 表达敏感配置存在性，不返回 `access_token`、`secret` 或 `config_json` 中的 secret/password/token 明文。

状态更新实现遵循“Handler 编排 + Entity 简单状态规则”模式：Handler 通过 Domain 读取实体、校验请求范围后调用 `MessageChannel::transition_status`，再复用 `update_message_channel` 写回；`Deleted` 不允许通过 `/status` 产生，必须走 `DELETE` action。`test connection` 不是字段更新，已在 Finance Domain 暴露 `test_message_channel` 语义入口，Handler 不越层调用 DAL。

### 3.2 Tool（P0）

```http
POST   /api/v1/tools
GET    /api/v1/tools
GET    /api/v1/tools/{id}
PUT    /api/v1/tools/{id}
PUT    /api/v1/tools/{id}/status
DELETE /api/v1/tools/{id}
GET    /api/v1/tools/search
```

Agent 绑定关系：

```http
GET    /api/v1/agents/{agent_id}/tools
POST   /api/v1/agents/{agent_id}/tools/{tool_id}
DELETE /api/v1/agents/{agent_id}/tools/{tool_id}
```

说明：
- `PUT /tools/{id}/status` 接收目标状态，不拆 `enable/disable` 路由；
- 当前 Domain 中 `enable_tool` / `disable_tool` 如仍为空实现，暴露前先补真实状态更新能力或统一状态更新入口；
- 工具执行、ToolCallRequest / ToolCallResult 不纳入本组管理面接口。

### 3.3 Agent Status（P0）

```http
PUT /api/v1/agents/{id}/status
```

请求体传目标状态，由 Domain 的 `transition_status` 负责状态流转校验。必要时 Handler 可先调用 `validate_onboard_readiness` 做用户 Action 所需的前置提示，但最终业务规则仍归 Domain。

### 3.4 Project / Task（P1）

Project：

```http
POST   /api/v1/projects
GET    /api/v1/projects
GET    /api/v1/projects/{id}
PUT    /api/v1/projects/{id}
PUT    /api/v1/projects/{id}/status
```

Task：

```http
POST   /api/v1/tasks
GET    /api/v1/tasks/{id}
GET    /api/v1/projects/{project_id}/tasks
GET    /api/v1/agents/{agent_id}/tasks
PUT    /api/v1/tasks/{id}
PUT    /api/v1/tasks/{id}/status
```

说明：
- 不新增 `/start`、`/complete`、`/archive`、`/cancel` 路由；
- 如果 Domain 当前只有 `start/complete/archive/cancel` 方法，第一步可由 Handler 根据目标状态调用现有 Domain 方法；第二步再补 Domain 统一 `update_status`/`transition_status` 入口，收敛状态流转规则。

### 3.5 Skill（P1）

```http
POST   /api/v1/skills
GET    /api/v1/skills
GET    /api/v1/skills/{id}
PUT    /api/v1/skills/{id}
DELETE /api/v1/skills/{id}
GET    /api/v1/skills/search
GET    /api/v1/agents/{agent_id}/skills
POST   /api/v1/agents/{agent_id}/skills/{skill_id}
```

说明：
- 第一阶段只补元数据 + 主文件内容相关能力；
- 如文件删除、安装副作用等能力在 Domain/DAL 中仍有 TODO，不先暴露为正式 API；
- Skill 内容可能较大，列表响应使用摘要 DTO，详情响应再返回完整内容。

### 3.6 Artifact / MessageManagement（P2）

Artifact 受文件上传机制影响，建议等附件上传/存储 API 稳定后再补：

```http
POST   /api/v1/projects/{project_id}/artifacts
POST   /api/v1/tasks/{task_id}/artifacts
GET    /api/v1/artifacts/{id}
GET    /api/v1/projects/{project_id}/artifacts
GET    /api/v1/tasks/{task_id}/artifacts
DELETE /api/v1/artifacts/{id}
```

MessageManagement 仅补管理查询与状态更新，不补投递运行面：

```http
GET    /api/v1/messages
GET    /api/v1/messages/{id}
PUT    /api/v1/messages/{id}/status
DELETE /api/v1/messages/{id}
GET    /api/v1/projects/{project_id}/messages
GET    /api/v1/tasks/{task_id}/messages
```

---

## 4. 分阶段实施计划

### Phase 0：准备与约定固化

1. 在 `common/src/api/` 中补充缺失的 Request / Response DTO；
2. 统一状态更新请求 DTO，例如 `UpdateStatusRequest<TStatus>` 或各对象明确的 `UpdateXxxStatusRequest`；
3. 为敏感配置类定义脱敏 Response DTO；
4. 通用响应包装统一使用 `common::api::ApiResponse<T>`，`src/handlers` 不保留本地响应包装；
5. 在 `src/router.rs` 中保持路由分组清晰，不引入通用 CRUD router 宏。

验收：`cargo check` 通过；已有测试不受影响。

### Phase 1：P0 纯配置 / 绑定关系

1. 补 `finance/message_channel` Handler 文件与路由；已完成 `create_message_channel`、`list_message_channels`、`get_message_channel`、`update_message_channel`、`delete_message_channel`、`update_message_channel_status`、`test_message_channel_connection`，并新增/补齐 `common/src/api/message_channel.rs` 脱敏 DTO；
2. 补 `finance/tool` 基础查询、管理与 Agent 绑定 Handler；
3. 补 `hr/agent` 状态更新 Handler；
4. 为新增 Handler 添加最小集成测试或 handler 级契约测试。

验收：新增 API 能通过 Domain 完成真实操作；敏感字段响应不泄漏。

### Phase 2：P1 核心业务对象

1. 补 `project/project` Handler；
2. 补 `project/task` Handler；
3. 补 `hr/skill` Handler；
4. 对 Project/Task 状态更新先复用现有 Domain 状态方法，随后收敛为 Domain 统一状态更新入口。

验收：Project/Task/Skill 管理面可从前端完整操作；状态更新路由不膨胀。

### Phase 3：P2 文件 / 消息管理

1. 在附件上传机制稳定后补 `project/artifact` Handler；
2. 补 `message/management` 查询、状态更新、删除/清理接口；
3. 保持 MessageDelivery、Consumer、Runtime 唤醒链路独立推进。

验收：文件和消息管理能力可用，但不把运行面队列/投递逻辑混入 CRUD Handler。

---

## 5. 实施检查清单

每补一组 Handler，必须检查：

- [ ] Handler 只调用 Domain，不调用 DAL/DAO；
- [ ] 每个文件对应一个明确用户 Action；
- [ ] DTO 没有直接下传到 Domain；
- [ ] 复用通过 Command/Query/Params 或 Domain 方法完成；
- [ ] 状态变更使用统一 status action；
- [ ] 敏感字段不在 Response、日志、错误信息中泄漏；
- [ ] `src/router.rs` 暴露路由与文档一致；
- [ ] `cargo check` 通过；
- [ ] 新增或更新对应测试；
- [ ] 实现后同步更新本文档状态。
