# Handler 管理面 API 补齐方案

> 📦 归档标记（2026-08-16）：归档冻结。保留原因：handler_management_api_plan 归档冻结，历史快照。生效方案：见源码和 wiki 长文。

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
- Project 状态更新使用 `ProjectDomain::project_manage().transition_status(ctx, &mut project, target_status)`，Task 状态更新使用 `ProjectDomain::task_manage().transition_status(ctx, &mut task, target_status)`，由 Domain 根据目标状态校验合法性并持久化；
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

Phase 3 从 Artifact 开始推进，再补 Message 管理面。Artifact 虽然归属 Project Domain，但不是 `Project` 的纯嵌套资源：它可以是项目级产物，也可以绑定到某个 Task。因此 Artifact 管理面采用 **独立资源集合 API**，不使用 `/projects/{project_id}/artifacts` 作为主入口。

#### Batch 3.1：Project Artifact 独立资源 API

目标：补齐 Project Domain 下的 Artifact 管理面 API，使产物可以通过统一入口创建、查询、详情、删除，并通过参数表达项目/任务归属关系。API path 保持独立资源集合方案不变；Batch 3.1 仅落地 `attachment` 引用型创建闭环，`generated_content` 与 `remote_url` 只作为 DTO/枚举契约预留，创建 Handler 暂返回 `Unsupported`。

核心决策：
- API 路径按资源类型建模：`/api/v1/project/artifacts...`；
- Domain 归属仍是 Project Domain，不拆独立 Artifact Domain；
- `project_id` 是 Artifact 的必填归属字段，`task_id` 可选；
- `task_id = None` 表示项目级产物，`task_id = Some(...)` 表示任务级产物；
- 如果传入 `task_id`，Domain 必须校验该 Task 属于同一个 `project_id`；
- Artifact 来源枚举预留三类：
  - `attachment`：前端/用户先上传 Finance Attachment，再用 `attachment_id` 创建 Artifact 引用；Batch 3.1 已落地；
  - `generated_content`：Agent 在执行项目/任务时，直接通过 Artifact API 写入文本类产物（方案、报告、代码片段、执行记录等），用于过程留痕和长期归档；当前仅在 DTO 契约中预留，创建 Handler 暂返回 Unsupported；
  - `remote_url`：远程 URL 类产物引用，当前仅预留枚举；
- Handler 可编排 Finance Domain + Project Domain，但不能直接调用 DAO/DAL，也不能直接读写文件系统；
- `attachment` 来源不做二次文件复制/搬运，只记录文件元数据引用；`generated_content` 后续落地时再由 Project Domain / Artifact DAL 写入 artifact 专属存储路径；
- Artifact 文件路径按逻辑根前缀记录在 `file_meta.file_path`：Batch 3.1 的 Attachment 引用使用 `attachments/{relative_path}`；Agent 生成内容后续使用 `artifacts/projects/{project_id}/{artifact_id}/{file_name}`。

路由状态：

```http
POST   /api/v1/project/artifacts       # ✅ 已落地：attachment 引用型创建
GET    /api/v1/project/artifacts       # ✅ 已落地：按 project/task/file/source/limit 查询
GET    /api/v1/project/artifacts/{id}  # ✅ 已落地：详情查询
DELETE /api/v1/project/artifacts/{id}  # ✅ 已落地：软删除
```

暂不在 Batch 3.1 引入复杂编辑接口；如需修改名称、描述、tags，可后续补：

```http
PUT    /api/v1/project/artifacts/{id}
```

创建请求采用同一个入口、按 `source_type` 区分来源模式；Batch 3.1 已落地 `attachment` 引用型创建，`generated_content` 与 `remote_url` 先保留契约/枚举，后续补存储与元数据策略。

Attachment 引用模式：

```json
{
  "project_id": "proj_xxx",
  "task_id": "task_xxx",
  "source_type": "attachment",
  "attachment_id": "att_xxx",
  "name": "设计稿",
  "description": "第一版方案",
  "tags": ["design", "draft"]
}
```

Agent 生成内容模式（仅契约预留，Batch 3.1 创建暂返回 `Unsupported`）：

```json
{
  "project_id": "proj_xxx",
  "task_id": "task_xxx",
  "source_type": "generated_content",
  "file_name": "implementation-plan.md",
  "content": "# 实施方案\n...",
  "mime_type": "text/markdown",
  "name": "实施方案",
  "description": "Agent 执行任务前沉淀的方案",
  "tags": ["agent-generated", "plan"]
}
```

字段语义：
- `project_id`：必填，用于权限校验、归属校验和存储路径组织；
- `task_id`：可选，传入时必须属于同一个 `project_id`；
- `source_type`：必填，`attachment` 表示引用已上传文件，`generated_content` 表示 Agent 直接写入产物内容（当前 DTO 预留，创建暂不落地），`remote_url` 表示远程 URL 引用（当前仅预留枚举）；
- `attachment_id`：仅 `source_type = attachment` 时必填，指向 Finance Attachment 用户资产；Handler 只读取 Attachment metadata，不直接读写文件；
- `content/file_name/mime_type`：仅 `source_type = generated_content` 时使用；当前仅 DTO 契约预留，创建 Handler 暂返回 `Unsupported`；后续落地时 Handler 不直接写文件，只把内容交给 Project Domain，最终由 Artifact DAL 写入 artifact 专属存储；
- `name/description/tags`：Artifact 业务元数据；Batch 3.1 不新增独立 `version` 字段，确需表达版本时可先放入 `name/description/tags`，后续再评估是否扩展模型。

校验规则：`attachment_id` 与 `content` 二选一，必须与 `source_type` 匹配；Batch 3.1 创建 Handler 仅接受 `attachment_id` 引用型产物，`generated_content` / `remote_url` 暂返回 Unsupported；后续落地 `generated_content` 时应只面向文本类内容，并限制单次请求最大内容长度，避免把大文件上传能力绕过 Attachment。

后续扩展：Batch 4.2 在 Finance Attachment 自身补充 `POST /api/v1/finance/attachments/text`，让 Attachment 除 multipart 上传外也支持由 Agent/系统通过 JSON 创建小型 UTF-8 文本文件资产；该能力不改变 Artifact API path，也不纳入 Batch 3.1。本批次只预留 `generated_content` 请求形态，暂不实现写入闭环。

列表查询建议：

```http
GET /api/v1/project/artifacts?project_id=proj_xxx
GET /api/v1/project/artifacts?project_id=proj_xxx&task_id=task_xxx
GET /api/v1/project/artifacts?project_id=proj_xxx&source_type=attachment
GET /api/v1/project/artifacts?project_id=proj_xxx&limit=50
```

`project_id` 作为列表查询必填参数，避免无边界扫全库；`task_id/file_type/source_type/limit` 作为可选过滤条件。`source_type` 按 API 枚举字符串传递（如 `attachment`、`generated_content`、`remote_url`）；`file_type` 遵循共享 `FileType` 的 serde 枚举格式。后续如需要 Agent 视角或全文搜索，可在同一资源集合上扩展 query 参数，而不是新增嵌套路由。

实现范围：
1. 新增 `common/src/api/artifact.rs`，定义 `CreateArtifactRequest`、`ArtifactSourceType`、`ArtifactDetail`、`ArtifactListQuery`、`ArtifactListItem`、`ListArtifactsResponse`、`GetArtifactResponse`、`DeleteArtifactResponse` 等 DTO，并在 `common/src/api/mod.rs` 导出；
2. 新增 `common/src/api/artifact_test.rs`，覆盖 DTO JSON 契约：项目级/任务级、`attachment` 来源、`generated_content` 预留请求形态；
3. 扩展 `src/models/artifact.rs` / artifacts 表，补 `source_type` 字段（整数枚举：1=`attachment`，2=`generated_content`，3=`remote_url`），避免只靠 `file_meta.file_path` 前缀反推来源；
4. 扩展 `src/service/domain/project/artifact.rs`，在 Project Domain 的 `artifact_manage()` 子能力下提供面向 Handler 的统一创建入口，例如 `create_attachment_artifact(...)` / `list(ctx, ListArtifactsParams)`；`ProjectDomainImpl` 组合 `ProjectDal` / `TaskDal` / `ArtifactDal`，在 Domain 内部校验 `project_id` 存在、`task_id` 归属一致，并按 `source_type` 组装 Artifact；
5. Artifact 文件存储能力暂不纳入 Batch 3.1；后续落地 `generated_content` 时，位置建议在 Artifact DAL 或其下沉的文件存储辅助模块中，负责把内容写入 `artifacts/projects/{project_id}/{artifact_id}/{file_name}`，并返回 `FileMeta`；DAO 仍只负责 artifacts 表持久化；
6. 如现有 Artifact Domain 只能按 project/task 分别查询，补统一 `list_artifacts(ctx, ArtifactListParams)`，转成 DAL/DAO `ArtifactQuery`；`ArtifactQuery` 需补 `file_type: Option<FileType>` 与 `source_type: Option<ArtifactSourceType>` 以匹配管理面查询参数；
7. 新增 `src/handlers/project/artifact/`，每个用户 action 单独文件：`create_artifact.rs`、`list_artifacts.rs`、`get_artifact.rs`、`delete_artifact.rs`、`response.rs`、`mod.rs`；
8. `create_artifact` Handler 按 `source_type` 分支：`attachment` 时调用 Finance Domain 获取 Attachment metadata（`include_file_content = false`）并转换为 `FileMeta`；`generated_content` / `remote_url` 在 Batch 3.1 暂返回 `Unsupported`；Handler 不直接读文件、不直接写 artifact 文件；
9. 在 `src/handlers/project/mod.rs` 导出 artifact handlers；
10. 在 `src/router.rs` 增加 `artifact_routes()`，暴露 `/project/artifacts...` 路由；
11. 更新 `docs/project_management_design.md`、本文档、README/API 速览；
12. 运行 fmt/check/相关测试并阶段性提交。

计划链路：

```text
POST /api/v1/project/artifacts
  body = { project_id, task_id?, source_type, ... }
    ↓
Artifact Handler
    ├─ 校验 source_type 与请求字段匹配
    ├─ attachment 来源：Finance Domain get_attachment(include_file_content = false)
    │   └─ 校验 attachment 属于当前用户，返回 metadata
    ├─ generated_content / remote_url 来源：Batch 3.1 暂返回 Unsupported
    ↓
    └─ Project Domain create_artifact(CreateArtifactParams)
        ├─ 校验 project 存在且属于当前用户上下文
        ├─ 若 task_id 存在，校验 task.project_id == project_id
        ├─ attachment 来源：组装 Attachment 引用型 FileMeta
        ├─ 组装 Artifact 业务实体
        └─ 调用 Artifact DAL 持久化 metadata
```

Attachment 引用来源也可以先通过通用上传获得 `attachment_id`：

```text
POST /api/v1/finance/attachments/upload
  → 得到 attachment_id
POST /api/v1/project/artifacts
  body = { project_id, task_id?, source_type: "attachment", attachment_id, name, description?, tags? }
```

验收：Artifact 可通过独立资源 API 创建、按项目/任务查询、查看详情、软删除；项目级与任务级产物均可表达；Batch 3.1 创建闭环仅支持引用 Finance Attachment，Agent 写入文本类归档内容仅保留 DTO/枚举契约且暂返回 `Unsupported`；Task 与 Project 不一致时由 Domain 拒绝；Handler 不越层、不直接触碰文件系统。

#### Batch 3.2：Message 管理面 API

1. 补 `message/management` 查询、状态更新、删除/清理接口；
2. 保持 MessageDelivery、Consumer、Runtime 唤醒链路独立推进；
3. 管理面只做 CRUD/查询/清理，不混入运行面队列投递逻辑。

验收：文件和消息管理能力可用，但不把运行面队列/投递逻辑混入 CRUD Handler。

### Phase 4：简单文本内容编辑 API

Phase 4 目标是在不引入通用 FileEdit Domain 的前提下，为 Finance Attachment、HR Skill、Project Artifact 三类已有资源补充“小文本文件内容读取/全量替换”能力。三个资源共享 DTO 与校验思路，但仍分别落在所属 Domain，避免跨领域副作用：

- Attachment：用户资产，归属 Finance Domain；
- Skill：技能主内容与附加文件，归属 HR Domain；
- Artifact：项目/任务产物，归属 Project Domain；
- Handler 只调用 Domain，不直接调用 DAL/DAO，不直接拼接或读写文件系统路径。

#### Batch 4.1：共享文本内容 DTO 与校验边界

共享 DTO 放在 `common/src/api/text_content.rs`，并由 `attachment.rs`、`artifact.rs`、`skill.rs` 组合使用：

```rust
pub struct TextContentResponse {
    pub content: String,
    pub encoding: String, // "utf-8"
    pub size: u64,
    pub updated_at: i64,
}

pub struct UpdateTextContentRequest {
    pub content: String,
    pub expected_updated_at: Option<i64>,
}
```

统一约束：
- 第一版只支持 UTF-8 简单文本；
- 默认最大内容 `64KB`，避免绕过 Attachment 大文件上传能力；
- `PUT` 采用全量替换，不做 patch/diff/version；
- `expected_updated_at` 可选，传入时执行乐观锁，不匹配返回 `409 Conflict`；
- 可编辑类型初期限制为 `text/*`、`.txt`、`.md`、`.json`、`.yaml`、`.yml`、`.toml`、`.csv`；
- 所有路径和文件名校验必须在对应 Domain 内完成，Handler 不承载路径安全规则。

#### Batch 4.2：Finance Attachment 文本内容创建与编辑

路由：

```http
POST /api/v1/finance/attachments/text
GET  /api/v1/finance/attachments/{id}/content
PUT  /api/v1/finance/attachments/{id}/content
```

范围：
1. 扩展 `common/src/api/attachment.rs`：`CreateTextAttachmentRequest { file_name, content, mime_type?, purpose? }`、`CreateTextAttachmentResponse = AttachmentDetail`、`AttachmentContentResponse { attachment, text }`，并复用 `UpdateTextContentRequest`；
2. 扩展 Finance Domain `AttachmentManage`：`create_text_attachment` / `get_text_content` / `update_text_content`；
3. `POST /attachments/text` 仅用于 JSON 传入的小型 UTF-8 文本内容，默认最大 `64KB`；大文件、二进制仍走 multipart upload；
4. Domain 校验 `file_name` 安全（禁止 `/`、`\`、`..`、绝对路径、空路径、目录目标）、`mime_type`/扩展名属于文本类型、内容为 UTF-8、大小不超限、Attachment 属于当前 `root_user_id`；
5. 读取/更新时继续校验 Attachment 存在、未删除、是可编辑文本、小于阈值、UTF-8、乐观锁匹配；
6. AttachmentDal 当前已有 `create_from_upload` 与 `read_file`，需补 `create_from_text` / `update_text_content`，负责创建时生成 `id/stored_name/relative_path/FileType/size`，写入文件与 metadata；更新时完成文件覆盖与 metadata 刷新；
7. AttachmentDao 当前已有 `write_file/read_file/file_exists` 文件 primitive 与 `insert/find/query/delete` metadata primitive，需补更新 metadata 的 primitive（如更新 `size/mime_type/file_type/modified_by/updated_at`）；DAO 不做 UTF-8、MIME、归属、乐观锁等业务判断；
8. 新增 `src/handlers/finance/attachment/create_text_attachment.rs`、`get_attachment_content.rs` 与 `update_attachment_content.rs`，并注册路由。

验收：可以通过 JSON 直接创建小文本 Attachment，也可以读取/替换已存在小文本 Attachment；危险文件名、二进制、超限、跨用户、乐观锁冲突均被拒绝；Handler 不读写文件、不拼接物理路径。

#### Batch 4.3：HR Skill 文本文件内容编辑

路由：

```http
GET /api/v1/hr/skills/{id}/files/content?path=skill.md
PUT /api/v1/hr/skills/{id}/files/content?path=references/guide.md
```

范围：
1. 扩展 `common/src/api/skill.rs`：`SkillFileContentQuery`、`SkillFileContentResponse`；
2. 扩展 HR Domain `SkillManage`：`get_text_file_content` / `update_text_file_content`；
3. Domain 校验 Skill 存在、可编辑、path 安全、文本大小/UTF-8/乐观锁；
4. 显式内容编辑接口允许编辑 `skill.md`，但 Batch 2.5 的 Attachment 导入接口仍禁止覆盖 `skill.md`；
5. 附加文本文件可通过安全相对路径直接全量写入；第一版不做删除、重命名、批量编辑。

验收：可以读取/替换 `skill.md` 和 `references/*.md` 等附加文本；路径穿越、反斜杠、目录目标、超限、非 UTF-8 被拒绝。

#### Batch 4.4：Project Artifact generated_content 创建与文本内容编辑

路由：

```http
POST /api/v1/project/artifacts                # 扩展支持 source_type=generated_content
GET  /api/v1/project/artifacts/{id}/content   # 仅 generated_content
PUT  /api/v1/project/artifacts/{id}/content   # 仅 generated_content
```

范围：
1. 扩展 `common/src/api/artifact.rs`：`ArtifactContentResponse`，复用 `UpdateTextContentRequest`；
2. Project Domain `artifact_manage()` 补 `create_generated_content_artifact`、`get_text_content`、`update_text_content`；
3. Domain 继续校验 `project_id` 存在、`task_id` 归属一致；
4. ArtifactDal 或其内部文件存储辅助模块负责写入/读取 `artifacts/projects/{project_id}/{artifact_id}/{file_name}` 并返回/更新 `FileMeta`；ArtifactDao 仍只负责 artifacts 表持久化；
5. `attachment` 来源 Artifact 不允许通过 Artifact API 编辑底层文件，需改用 Finance Attachment 内容编辑 API；`remote_url` 仍返回 Unsupported。

验收：Agent/用户可以创建 generated_content 文本产物并后续读取/替换；attachment-backed Artifact 不会被 Artifact API 间接修改原始 Attachment。

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