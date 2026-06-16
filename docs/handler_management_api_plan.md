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
PUT /api/v1/hr/agents/{id}/status
PUT /api/v1/finance/tools/{id}/status
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
| P0 | `finance` | Tool | create/get/query/list/update/bind/unbind/list_agent_tools/search | 已补 create/list/get/update/delete/status/agent-bind | 已补基础管理与 Agent 绑定；搜索通过列表 query 的 `keyword` 承载，工具执行不纳入本轮 |
| P0 | `hr` | Agent Status | transition_status/validate_onboard_readiness | 已补 status | 使用统一 `PUT /api/v1/hr/agents/{id}/status`，状态流转由 HR Domain 校验 |
| P1 | `project` | Project | create/get/list_by_user/update_basic/start/complete/archive/transition_status | 已补 create/list/get/update/status | Batch 2.1 已落地；状态更新使用 Domain 统一 `transition_status`，Handler 只调用统一 status action |
| P1 | `project` | Task | create/get/list_by_project/list_by_agent/update_basic/start/complete/cancel/transition_status | 已补 create/list/get/update/status | Batch 2.2 已落地；状态更新使用 Domain 统一 `transition_status`，Handler 只调用统一 status action |
| P1 | `hr` | Skill | create/get/update/delete/query/list/search/install_to_agent/list_for_agent | 已补 create/list/get/update/delete/search/install/list_agent_skills | Batch 2.3 已落地；先补元数据、主内容、搜索与安装；路由统一归入 HR 前缀 |
| P1.5 | `finance` | Attachment | upload/get/query/delete | 已补 upload/list/get/delete | Batch 2.4 已落地；Attachment 作为用户资产，按 DAO/DAL/Finance Domain/Handler 分层实现通用上传，供 Skill/Message/Artifact/Tool 复用 |
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
POST   /api/v1/finance/tools
GET    /api/v1/finance/tools
GET    /api/v1/finance/tools/{id}
PUT    /api/v1/finance/tools/{id}
PUT    /api/v1/finance/tools/{id}/status
DELETE /api/v1/finance/tools/{id}
POST   /api/v1/finance/tools/{id}/agent-bind
DELETE /api/v1/finance/tools/{id}/agent-bind
```

当前已落地以上八个管理面路由（create/list/get/update/delete/status/agent-bind/agent-unbind）。列表查询使用 query 参数承载筛选条件：`keyword`、`enabled_only`、`agent_id`、`limit`；不单独暴露 `/search` 路由，避免把搜索作为独立运行面动作膨胀。

Agent 绑定关系通过 Tool 管理面 action 表达，请求体携带 `agent_id`：

```json
{
  "agent_id": "agent_xxx"
}
```

说明：
- `PUT /api/v1/finance/tools/{id}/status` 接收目标状态，不拆 `enable/disable` 路由；
- `enable_tool` / `disable_tool` 薄方法已移除，状态变更统一走 Entity `transition_status` + Domain `update_tool` 写回；
- `Builtin` Tool 由系统同步，管理面禁止 create/update/delete 内置工具；
- Tool Response DTO 仅返回 `has_config`，不返回 `config` 原文，避免泄漏 header/token/connection string 等敏感配置；
- 工具执行、ToolCallRequest / ToolCallResult 不纳入本组管理面接口。

### 3.3 Agent Status（P0）

```http
PUT /api/v1/hr/agents/{id}/status
```

当前已落地该状态更新路由。请求体使用 `UpdateAgentStatusRequest { status: AgentStatus }`，响应复用 Agent 详情契约 `UpdateAgentStatusResponse = GetAgentResponse`，便于前端拿到状态更新后的完整展示数据。

Handler 仅负责读取 Agent、调用 HR Domain 的 `transition_status` 并返回 DTO；状态流转合法性由 Domain 校验。必要时后续可在确认入职类用户 Action 中调用 `validate_onboard_readiness` 做前置提示，但最终业务规则仍归 Domain。

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
- Batch 2.1 已落地 Project 管理面 API：`POST/GET /api/v1/projects`、`GET/PUT /api/v1/projects/{id}`、`PUT /api/v1/projects/{id}/status`；
- Batch 2.2 已落地 Task 管理面 API：`POST /api/v1/tasks`、`GET/PUT /api/v1/tasks/{id}`、`GET /api/v1/projects/{project_id}/tasks`、`GET /api/v1/agents/{agent_id}/tasks`、`PUT /api/v1/tasks/{id}/status`；
- 新增 `common/src/api/project.rs` 与 `common/src/api/task.rs` 作为前后端共享 DTO；Project 列表支持 `root_user_id`、`status`、`limit` query 参数，Task 列表支持 `status`、`limit` query 参数；
- 不新增 `/start`、`/complete`、`/archive`、`/cancel` 路由；
- Project 状态更新使用 `ProjectDomain::transition_status(ctx, &mut project, target_status)`，Task 状态更新使用 `TaskDomain::transition_status(ctx, &mut task, target_status)`，由 Domain 根据目标状态校验合法性并持久化；
- Handler 只解析目标状态并调用统一 Domain 方法，不在 Handler 层把目标状态分发到 `start/complete/archive/cancel` 等具体业务方法。

### 3.5 Skill（P1）

```http
POST   /api/v1/hr/skills
GET    /api/v1/hr/skills
GET    /api/v1/hr/skills/{id}
PUT    /api/v1/hr/skills/{id}
DELETE /api/v1/hr/skills/{id}
GET    /api/v1/hr/skills/search
GET    /api/v1/hr/agents/{agent_id}/skills
POST   /api/v1/hr/agents/{agent_id}/skills/{skill_id}
```

说明：
- Skill 属于 HR Domain，管理面路由统一使用 `/api/v1/hr/...` 前缀，与 Agent 管理面保持一致；
- 当前已落地以上八个管理面路由（create/list/get/update/delete/search/list_agent_skills/install_to_agent）；
- 第一阶段只补元数据、主文件内容、搜索和安装到 Agent；
- `install_to_agent` 已完成 Domain/DAL 分层边界收敛，通过 HR Domain 返回完整 `Skill` 业务实体，因此纳入 Batch 2.3 正式 API；
- `POST/PUT` 支持主内容 `content` 写入，列表与搜索响应使用摘要 DTO，不返回大内容；详情、创建、更新、安装响应返回完整详情 DTO；
- 文件删除、附件级读写等复杂文件副作用等 Domain/DAL 语义稳定后再补。

### 3.6 Finance Attachment（P1.5）

Attachment 作为 Finance Domain 下的用户资产管理能力，提供通用上传与基础查询接口。业务域不直接接收 multipart，而是引用已上传的 `attachment_id`。

```http
POST   /api/v1/finance/attachments/upload
GET    /api/v1/finance/attachments
GET    /api/v1/finance/attachments/{id}
DELETE /api/v1/finance/attachments/{id}
```

说明：
- 通用上传能力归属 `finance`，因为它是跨 Skill / Message / Project / Tool 复用的用户资产，而不是某个业务域私有能力；
- 当前已落地以上四个管理面路由（upload/list/get/delete），上传使用 `multipart/form-data`，列表查询支持 `purpose`、`file_type`、`limit`；
- 实现必须遵循 `handler → finance domain → attachment dal → attachment dao`；
- 新增 `attachments` 元数据表，文件物理存储继续复用 `<base_data_path>/attachments/YYYYMMDD/{file_id}{extension}`；
- Handler 只解析 multipart 和 query/path 参数，并调用 Finance Domain；不直接写文件，不直接调用 DAL/DAO；
- Attachment DAO 负责 `AttachmentPo` 持久化和给定路径的文件系统基础读写；
- Attachment DAL 负责生成 ID/存储路径、推断文件类型、写文件、插入元数据并组装 `Attachment` 业务实体；
- 后续 Skill 文件更新通过 `attachment_id + target_path` 引用导入，不让 Skill API 直接接收文件流。

### 3.7 Artifact / MessageManagement（P2）

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
2. 补 `finance/tool` 基础查询、管理与 Agent 绑定 Handler；已完成 `create_tool`、`list_tools`、`get_tool`、`update_tool`、`delete_tool`、`update_tool_status`、`bind_tool_to_agent`、`unbind_tool_from_agent`，并新增 `common/src/api/tool.rs` 脱敏 DTO；
3. 补 `hr/agent` 状态更新 Handler；已完成 `update_agent_status`、`UpdateAgentStatusRequest`、`UpdateAgentStatusResponse` 与路由；
4. 为新增 Handler 添加最小集成测试或 handler 级契约测试；已补 Agent 状态 DTO 契约测试，并补充 HR Domain 状态流转成功/失败测试。

验收：新增 API 能通过 Domain 完成真实操作；敏感字段响应不泄漏。

### Phase 2：P1 核心业务对象

> 当前计划：Phase 2 按 Project → Task → Skill 三个小批次推进，每批独立完成 DTO、Handler、Router、最小契约/Domain 测试、文档更新、`cargo fmt/check/test` 与阶段性提交。这样避免一次性改动过大，也便于前端逐步接入。

#### Batch 2.1：Project 管理面 API

目标：先补最简单、闭环清晰的 Project 管理面接口。

范围：
1. 新增 `common/src/api/project.rs`，定义 create/list/get/update/status 的 Request / Response DTO；
2. 新增 `src/handlers/project/project/` 下的单 action Handler 文件；
3. 在 `src/handlers/project/mod.rs` 与 `src/router.rs` 暴露 Project 路由；
4. `PUT /api/v1/projects/{id}/status` 先补/使用 Domain 统一状态入口（如 `update_status` / `transition_status`），由 Domain 根据目标 `ProjectStatus` 执行合法性校验与流转；Handler 不分发到 `start/complete/archive`，也不新增 `/start`、`/complete`、`/archive` 路由；
5. 补 DTO 契约测试，必要时补 Domain 状态流转覆盖；
6. 更新本文档和 README/API 速览。

验收：Project 可创建、查询、列表、基础更新、状态更新；Handler 不越层调用 DAL/DAO。

#### Batch 2.2：Task 管理面 API

目标：复用 Project 的 Handler / DTO 模式，补齐 Task 管理面接口。

范围：
1. 新增 `common/src/api/task.rs`，定义 create/get/list/update/status 的 Request / Response DTO；
2. 新增 `src/handlers/project/task/` 下的单 action Handler 文件；
3. 在 Router 暴露：`POST /api/v1/tasks`、`GET /api/v1/tasks/{id}`、`GET /api/v1/projects/{project_id}/tasks`、`GET /api/v1/agents/{agent_id}/tasks`、`PUT /api/v1/tasks/{id}`、`PUT /api/v1/tasks/{id}/status`；
4. `PUT /api/v1/tasks/{id}/status` 先补/使用 Domain 统一状态入口（如 `update_status` / `transition_status`），由 Domain 根据目标 `TaskStatus` 执行合法性校验与流转；Handler 不分发到 `start/complete/cancel`，也不新增 `/start`、`/complete`、`/cancel` 路由；
5. 补 DTO 契约测试和关键 Domain 状态测试；
6. 更新本文档和 README/API 速览。

验收：Task 可创建、查询、按 Project/Agent 列表、基础更新、状态更新；状态路由不膨胀。

#### Batch 2.3：Skill 管理面 API

目标：补 HR Skill 管理面，先覆盖元数据 + 主内容 + 安装到 Agent，暂不扩展复杂文件副作用。

范围：
1. 新增 `common/src/api/skill.rs`，定义列表摘要 DTO、详情完整 DTO、创建/更新/搜索/安装请求 DTO；
2. 新增 `src/handlers/hr/skill/` 下的单 action Handler 文件；
3. 暴露：`POST /api/v1/hr/skills`、`GET /api/v1/hr/skills`、`GET /api/v1/hr/skills/{id}`、`PUT /api/v1/hr/skills/{id}`、`DELETE /api/v1/hr/skills/{id}`、`GET /api/v1/hr/skills/search`、`GET /api/v1/hr/agents/{agent_id}/skills`、`POST /api/v1/hr/agents/{agent_id}/skills/{skill_id}`；
4. 列表响应使用摘要 DTO，详情响应返回完整主内容，避免列表接口返回大字段；
5. 安装到 Agent 使用已收敛的 `install_to_agent` Domain/DAL 能力，返回完整 `Skill` 业务实体；更新时只暴露元数据和主内容写入；文件删除、附件级读写等复杂文件能力等 Domain/DAL 语义稳定后再补；
6. 补 DTO 契约测试和关键 Domain/Handler 契约覆盖；
7. 更新本文档和 README/API 速览。

验收：Skill 元数据、主内容、查询搜索、安装到 Agent 可从管理面操作；列表不返回大内容；Handler 不承载文件业务规则。

#### Batch 2.4：Finance Attachment 通用上传 API

目标：补 Finance Domain 下的通用 Attachment 上传与基础查询能力，为后续 Skill 文件导入、Message 附件、Project Artifact、Tool 大结果附件提供统一前置能力。

范围：
1. 更新 `docs/attachment_storage.md`，确认 Attachment 是 Finance Domain 用户资产，并记录 DAO/DAL/Domain/Handler 分层边界；
2. 新增 `attachments` 表迁移，记录上传文件元数据：`id/original_name/stored_name/relative_path/mime_type/file_type/size/purpose/status/root_user_id/created_by/modified_by/timestamps`；
3. 新增 `src/models/attachment.rs`，定义 `AttachmentPo`、`Attachment`、`AttachmentQuery`、`AttachmentUpload` 等模型；
4. 新增 `src/service/dao/attachment/`，实现 `AttachmentDao`：元数据 CRUD / query / soft delete，以及给定相对路径的文件基础读写；DAO 不承载业务归属、用途解释、跨领域导入规则；
5. 新增 `src/service/dal/attachment.rs`，实现 `AttachmentDal`：生成 ID/存储文件名/日期相对路径，推断文件类型，写文件并插入元数据，返回 `Attachment` 业务实体；
6. 在 Finance Domain 新增 `attachment_manage()` / `AttachmentManage` 能力，暴露 `upload_attachment/get_attachment/list_attachments/delete_attachment`；
7. 新增 `common/src/api/attachment.rs`，定义 `AttachmentDetail`、`AttachmentListQuery`、`UploadAttachmentResponse` 等前后端共享 DTO；
8. 新增 `src/handlers/finance/attachment/` 单 action Handler：`upload_attachment`、`get_attachment`、`list_attachments`、`delete_attachment`；
9. 在 `src/router.rs` 暴露 `POST /api/v1/finance/attachments/upload`、`GET /api/v1/finance/attachments`、`GET /api/v1/finance/attachments/{id}`、`DELETE /api/v1/finance/attachments/{id}`；
10. 补 DAO/DAL/DTO 契约测试，必要时补 Finance Domain 测试；
11. 更新 README/API 速览与本文档进度。

验收：Attachment 文件可通过 Finance 管理面上传、查询、列表、软删除；返回 `attachment_id` 可被后续业务接口引用；Handler 不越层，不直接写文件；路径生成和读取防止路径穿越；`cargo fmt/check/test` 通过。

#### Batch 2.5：Skill 文件引用导入

目标：在 Attachment 能力稳定后，扩展 Skill 更新接口，使 Skill 附加文件通过 `attachment_id + target_path` 引用导入。

架构决策：采用 **方案 B：Handler 层跨 Domain 编排**，避免 HR Skill Domain 直接依赖 Finance Domain。

职责边界：
- Finance Domain：负责 Attachment 用户资产归属校验、metadata 查询、按需装配文件读取结果；
- HR Skill Domain：负责 Skill 业务规则、`target_path` 安全校验、导入文件写入 Skill 内容目录；
- Skill Handler：负责解析用户请求、调用 Finance Domain 获取已装配附件、转换为 HR Domain 的 `SkillFileImport`，再调用 HR Domain 完成更新；
- Handler 不直接调用 DAO/DAL，不直接读写文件系统，不承载路径安全业务规则。

范围：
1. 扩展 `common/src/api/skill.rs`，新增 `SkillFileInput { attachment_id, target_path }`，并在更新请求中增加 `files: Option<Vec<SkillFileInput>>`；
2. 扩展 Finance `AttachmentManage::get_attachment`，增加 `AttachmentGetOptions { include_file_content }`；默认只返回 metadata，内部编排场景可要求装配 `AttachmentReadResult`；
3. 扩展 `Attachment` 业务实体，支持 `read_results: Vec<AttachmentReadResult>`，当前单附件只装配一个读取结果，后续可兼容多文件资产；
4. HR Skill Domain 新增 `SkillFileImport` 入参模型，只接收已读取文件 bytes 和目标路径，不接收 `attachment_id`，避免 Finance 概念泄漏进 HR Domain；
5. HR Skill Domain 校验 target path 安全性，并将文件写入 Skill 的 `content_path` 目录；
6. Skill Handler 对 `files` 做跨域编排：逐个调用 Finance get(include_file_content=true)，转换为 `SkillFileImport`，再调用 HR Skill Domain；
7. 保持主文件 `skill.md` 仍由 `content` 字段更新，附加文件不能绕过该语义直接覆盖主内容；
8. 更新 Skill DTO 契约测试、Finance Attachment Domain 测试、HR Skill Domain 文件导入测试与 `docs/skill_design.md`。

计划链路：

```text
PUT /api/v1/hr/skills/{id}
  body.files = [{ attachment_id, target_path }]
    ↓
Skill Handler
    ├─ Finance Domain get_attachment(include_file_content = true)
    │   └─ 校验 root_user_id，装配 AttachmentReadResult(bytes)
    ↓
    └─ HR Skill Domain update/import_files(SkillFileImport)
        └─ 校验 target_path，写入 skill.content_path/target_path
```

验收：用户可先上传文件获取 `attachment_id`，再通过 Skill 更新接口导入附加文件；Skill Handler 不直接接收 multipart，不直接访问 Attachment DAO/DAL。

#### Phase 2 进度跟踪

| 批次 | 对象 | 状态 | 交付物 | 验证 |
|------|------|------|--------|------|
| Batch 2.1 | Project | 已完成 | `common/src/api/project.rs`、`src/handlers/project/project/*`、`src/router.rs`、Project Domain `transition_status`、DTO/Domain 测试、文档更新 | `cargo fmt --all && cargo check && cargo test -p common api::project_test && cargo test --lib service::domain::project::project_test` |
| Batch 2.2 | Task | 已完成 | `common/src/api/task.rs`、`src/handlers/project/task/*`、`src/router.rs`、Task Domain `create_with_options/list/update_basic/transition_status`、DTO/Domain 测试、文档更新 | `cargo fmt --all && cargo check && cargo test -p common api::task_test && cargo test --lib service::domain::project::project_test` |
| Batch 2.3 | Skill | 已完成 | `common/src/api/skill.rs`、`src/handlers/hr/skill/*`、`src/router.rs`、DTO/Domain 测试、文档更新 | `cargo fmt --all -- --check && cargo check && cargo test -p common api::skill_test && cargo test --lib service::domain::hr::skill_test` |
| Batch 2.4 | Finance Attachment | 已完成 | `attachments` migration、`common/src/api/attachment.rs`、`src/models/attachment.rs`、`src/service/dao/attachment/*`、`src/service/dal/attachment.rs`、Finance Domain attachment manage、`src/handlers/finance/attachment/*`、Router 路由、测试与文档 | `cargo fmt --all -- --check && SQLX_OFFLINE=true cargo check && SQLX_OFFLINE=true cargo test` |
| Batch 2.5 | Skill 文件引用导入 | 已完成 | 扩展 Skill update DTO，Finance get 按需装配文件读取结果，Skill Handler 编排 Finance+HR，HR Domain 导入 `SkillFileImport` | `cargo fmt --all -- --check && SQLX_OFFLINE=true cargo check --bin ai_orz && SQLX_OFFLINE=true cargo test -p common api::skill_test && SQLX_OFFLINE=true cargo test service::domain::finance::attachment_test && SQLX_OFFLINE=true cargo test service::domain::hr::skill_test` |

统一约束：
- Handler 只调用 Domain，不直接调用 DAL/DAO；
- 每个 Handler 文件对应一个明确用户 Action；
- API DTO 保持在 `common/src/api/`，不把 Handler 本地结构泄漏给前端；
- 状态更新统一使用 `/status` action，不拆目标状态路由；
- Project/Task 状态更新优先补/使用 Domain 统一 `update_status` / `transition_status` 入口，Handler 不承载状态分发语义；
- Skill 管理面路由统一使用 HR 前缀 `/api/v1/hr/skills...`，安装和 Agent 技能列表也归入 `/api/v1/hr/agents/{agent_id}/skills...`；
- Attachment 通用上传归属 Finance Domain，路由统一使用 `/api/v1/finance/attachments...`，业务域通过 `attachment_id` 引用，不直接接收 multipart；
- 每批完成后同步更新本文档、README 和架构状态文档。

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
