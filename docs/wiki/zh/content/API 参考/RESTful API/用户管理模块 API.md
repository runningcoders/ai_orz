# 用户管理模块 API

<cite>
**本文引用的文件**
- [src/handlers/user/mod.rs](src/handlers/user/mod.rs)
- [src/handlers/user/profile/mod.rs](src/handlers/user/profile/mod.rs)
- [src/handlers/user/profile/get_current_user.rs](src/handlers/user/profile/get_current_user.rs)
- [src/handlers/user/profile/update_current_user.rs](src/handlers/user/profile/update_current_user.rs)
- [common/src/api/user.rs](common/src/api/user.rs)
- [src/models/user.rs](src/models/user.rs)
- [src/service/domain/organization/user.rs](src/service/domain/organization/user.rs)
- [common/src/enums/user.rs](common/src/enums/user.rs)
- [migrations/20260810000000_users_preferences.sql](migrations/20260810000000_users_preferences.sql)
- [frontend/src/pages/user/profile.rs](frontend/src/pages/user/profile.rs)
</cite>

## 更新摘要
**变更内容**
- 新增用户偏好双源机制的核心功能，支持用户自述偏好的声明式画像
- 在 UpdateCurrentUserRequest 中增加 preferences 可选字段，允许用户设置个人偏好
- 实现 Agent 安全守卫，防止 Agent 通过工具调用修改用户偏好数据
- 前端个人资料页新增偏好编辑界面，支持 Markdown 格式的偏好展示与编辑
- 扩展用户信息响应，包含用户偏好信息的返回

## 目录
1. [简介](#简介)
2. [项目结构](#项目结构)
3. [核心组件](#核心组件)
4. [架构总览](#架构总览)
5. [详细组件分析](#详细组件分析)
6. [依赖关系分析](#依赖关系分析)
7. [性能考虑](#性能考虑)
8. [故障排查指南](#故障排查指南)
9. [结论](#结论)
10. [附录：完整示例与最佳实践](#附录：完整示例与最佳实践)

## 简介
本章节面向 AI Orz 的用户管理模块，聚焦"当前用户个人信息获取与更新"的 HTTP API。该模块遵循四层单向调用（Adapter → Domain → DAL → DAO），Handler 仅负责参数解析、鉴权上下文提取与响应封装；业务规则集中在 Domain 层，数据访问在 DAL/DAO 层完成。当前实现包含两个接口：
- 获取当前用户信息
- 更新当前用户信息（显示名称、邮箱、密码哈希、**用户偏好**）

**重要更新**：新增用户偏好双源机制，支持用户通过 Markdown 格式设置个人偏好，Agent 观察总结的偏好走知识图谱，两者独立演进。

说明：
- 头像上传不属于当前用户管理模块范围，属于附件/资源管理范畴，可参考附件相关接口。
- 偏好设置现已作为核心功能实现，用户可通过前端界面或 API 设置个人偏好。

## 项目结构
用户管理模块位于 handlers/user 下，按功能子模块组织：
- profile：当前用户个人信息的读取与更新，**新增偏好设置功能**
- 公共 DTO 定义于 common/src/api/user.rs，**已扩展 preferences 字段**
- 领域能力通过 organization 域暴露（user_manage 等）
- 持久化对象 UserPo 位于 models/user.rs，**已包含 preferences 字段**
- 角色与状态枚举位于 common/src/enums/user.rs

```mermaid
graph TB
A["HTTP Handler<br/>get_current_user"] --> B["Domain: organization.user_manage"]
C["HTTP Handler<br/>update_current_user"] --> B
B --> D["DAL: user_dal"]
D --> E["DAO: SQL/存储"]
A -.-> F["DTOs: common::api::user"]
C -.-> F
B -.-> G["Models: UserPo"]
F -.-> H["Enums: UserRole, UserStatus"]
I["Database<br/>users.preferences"] -.-> D
J["Frontend<br/>Profile Page"] -.-> C
```

**图表来源**
- [src/handlers/user/profile/get_current_user.rs:1-71](src/handlers/user/profile/get_current_user.rs#L1-L71)
- [src/handlers/user/profile/update_current_user.rs:1-195](src/handlers/user/profile/update_current_user.rs#L1-L195)
- [common/src/api/user.rs:1-228](common/src/api/user.rs#L1-L228)
- [src/models/user.rs:1-146](src/models/user.rs#L1-L146)
- [src/service/domain/organization/user.rs:1-124](src/service/domain/organization/user.rs#L1-L124)
- [migrations/20260810000000_users_preferences.sql:1-13](migrations/20260810000000_users_preferences.sql#L1-L13)
- [frontend/src/pages/user/profile.rs:1-148](frontend/src/pages/user/profile.rs#L1-L148)

章节来源
- [src/handlers/user/mod.rs:1-4](src/handlers/user/mod.rs#L1-L4)
- [src/handlers/user/profile/mod.rs:1-8](src/handlers/user/profile/mod.rs#L1-L8)

## 核心组件
- 请求/响应 DTO
  - GetCurrentUserRequest：无参，用于获取当前用户信息
  - GetCurrentUserResponse：包含 UserInfoResponse
  - UpdateCurrentUserRequest：可选字段 display_name、email、password_hash、**preferences**
  - UpdateCurrentUserResponse：返回更新后的 UserInfoResponse
  - UserInfoResponse：用户基本信息（id、username、display_name、email、organization_id、role、role_name、status、**preferences**）
- 领域服务
  - organization::domain().user_manage()：提供 get_user_by_id、update_user 等方法
- 模型与枚举
  - UserPo：持久化对象，包含角色、状态、创建/修改信息、**用户偏好**
  - UserRole、UserStatus：角色与状态枚举，支持转换与权限判断

**更新**：所有 DTO 和模型已扩展支持用户偏好字段，支持 Markdown 格式的偏好内容。

章节来源
- [common/src/api/user.rs:8-60](common/src/api/user.rs#L8-L60)
- [src/models/user.rs:10-42](src/models/user.rs#L10-L42)
- [src/service/domain/organization/user.rs:119-122](src/service/domain/organization/user.rs#L119-L122)
- [common/src/enums/user.rs:8-21](common/src/enums/user.rs#L8-L21)

## 架构总览
本模块严格遵循 Adapter → Domain → DAL → DAO 的单向调用链：
- Adapter（HTTP Handler）：从 RequestContext 提取当前用户 ID，调用 Domain
- Domain：组织域的用户管理能力，封装业务规则与校验
- DAL/DAO：数据访问与持久化

```mermaid
sequenceDiagram
participant Client as "客户端"
participant Handler as "Handler : get_current_user"
participant Domain as "Domain : organization.user_manage"
participant DAL as "DAL : user_dal"
participant DB as "数据库"
Client->>Handler : "GET /api/v1/user/me"
Handler->>Handler : "从 RequestContext 取 user_id"
Handler->>Domain : "get_user_by_id(user_id)"
Domain->>DAL : "find_by_id(user_id)"
DAL->>DB : "查询用户记录(含 preferences)"
DB-->>DAL : "UserPo"
DAL-->>Domain : "UserPo"
Domain-->>Handler : "UserPo"
Handler-->>Client : "UserInfoResponse(含 preferences)"
```

**图表来源**
- [src/handlers/user/profile/get_current_user.rs:18-70](src/handlers/user/profile/get_current_user.rs#L18-L70)
- [src/service/domain/organization/user.rs:119-122](src/service/domain/organization/user.rs#L119-L122)

章节来源
- [src/handlers/user/profile/get_current_user.rs:1-71](src/handlers/user/profile/get_current_user.rs#L1-L71)
- [src/service/domain/organization/user.rs:1-124](src/service/domain/organization/user.rs#L1-L124)

## 详细组件分析

### 获取当前用户信息（GET /api/v1/user/me）
- 功能：返回当前已认证用户的详细信息，**包含用户偏好**
- 输入：无请求体参数，身份由 JWT/中间件注入到 RequestContext
- 处理流程：
  - 从 RequestContext 获取 user_id，未登录则返回错误
  - 调用 Domain 层获取用户实体
  - 将内部实体转换为 UserInfoResponse（空字符串转为 None，角色转中文名称，**偏好非空时包含**）
- 输出：GetCurrentUserResponse

```mermaid
flowchart TD
Start(["进入 get_current_user"]) --> CheckCtx["检查 RequestContext.user_id"]
CheckCtx --> |为空| Err400["返回 400 未登录"]
CheckCtx --> |存在| CallDomain["调用 domain.user_manage.get_user_by_id"]
CallDomain --> Found{"找到用户?"}
Found --> |否| Err404["返回 404 用户不存在"]
Found --> |是| BuildResp["构建 UserInfoResponse<br/>包含 preferences 字段"]
BuildResp --> Return["返回 GetCurrentUserResponse"]
```

**图表来源**
- [src/handlers/user/profile/get_current_user.rs:18-70](src/handlers/user/profile/get_current_user.rs#L18-L70)

章节来源
- [src/handlers/user/profile/get_current_user.rs:1-71](src/handlers/user/profile/get_current_user.rs#L1-L71)
- [common/src/api/user.rs:8-40](common/src/api/user.rs#L8-L40)

### 更新当前用户信息（PUT /api/v1/user/me）
- 功能：允许当前用户更新显示名称、邮箱、密码哈希、**用户偏好**
- 输入：UpdateCurrentUserRequest（可选字段，**包含 preferences**）
- 处理流程：
  - 从 RequestContext 获取 user_id
  - 获取当前用户实体
  - 仅允许更新指定字段（禁止修改角色、状态、组织ID等敏感字段）
  - **Agent 安全守卫**：当 ctx.agent_id 为 Some（Agent 上下文调用）时忽略 preferences 字段
  - 更新 updated_at 与 modified_by
  - 调用 Domain 层 update_user 持久化
  - 返回更新后的 UserInfoResponse
- 安全要点：
  - 身份由 JWT 保证，无需额外权限校验
  - 只更新白名单字段，防止越权修改
  - **Agent 无法通过工具调用修改用户偏好**

```mermaid
sequenceDiagram
participant Client as "客户端"
participant Handler as "Handler : update_current_user"
participant Domain as "Domain : organization.user_manage"
participant DAL as "DAL : user_dal"
participant DB as "数据库"
Client->>Handler : "PUT /api/v1/user/me {display_name?, email?, password_hash?, preferences?}"
Handler->>Handler : "校验并提取 user_id"
Handler->>Handler : "Agent 安全守卫检查"
alt Agent 上下文
Handler->>Handler : "忽略 preferences 字段"
else 真人会话
Handler->>Handler : "允许更新 preferences"
end
Handler->>Domain : "get_user_by_id(user_id)"
Domain->>DAL : "find_by_id(user_id)"
DAL-->>Domain : "UserPo"
Domain-->>Handler : "UserPo"
Handler->>Handler : "仅更新白名单字段"
Handler->>Domain : "update_user(UserPo)"
Domain->>DAL : "update(UserPo)"
DAL->>DB : "写入变更(含 preferences)"
DB-->>DAL : "成功"
DAL-->>Domain : "成功"
Domain-->>Handler : "成功"
Handler-->>Client : "UpdateCurrentUserResponse"
```

**图表来源**
- [src/handlers/user/profile/update_current_user.rs:18-102](src/handlers/user/profile/update_current_user.rs#L18-L102)
- [src/service/domain/organization/user.rs:48-51](src/service/domain/organization/user.rs#L48-L51)

章节来源
- [src/handlers/user/profile/update_current_user.rs:1-195](src/handlers/user/profile/update_current_user.rs#L1-L195)
- [common/src/api/user.rs:42-60](common/src/api/user.rs#L42-L60)

### 数据模型与枚举
- UserPo：包含 id、organization_id、username、display_name、email、password_hash、role、status、created_by、modified_by、created_at、updated_at、**preferences**
- UserRole：SuperAdmin、Admin、Member，支持父角色查找与权限判断
- UserStatus：Active、Disabled，支持 i32 互转

```mermaid
classDiagram
class UserPo {
+string id
+string organization_id
+string username
+string display_name
+string email
+string password_hash
+UserRole role
+UserStatus status
+string created_by
+string modified_by
+i64 created_at
+i64 updated_at
+string preferences
}
class UserRole {
+to_i32() i32
+from_i32(v) UserRole
+parent() Option<UserRole>
+find_root() UserRole
+has_permission(user_role, min_role) bool
}
class UserStatus {
+to_i32() i32
+from_i32(v) UserStatus
}
UserPo --> UserRole : "使用"
UserPo --> UserStatus : "使用"
```

**图表来源**
- [src/models/user.rs:10-42](src/models/user.rs#L10-L42)
- [common/src/enums/user.rs:8-21](common/src/enums/user.rs#L8-L21)
- [common/src/enums/user.rs:114-147](common/src/enums/user.rs#L114-L147)

章节来源
- [src/models/user.rs:1-146](src/models/user.rs#L1-L146)
- [common/src/enums/user.rs:1-212](common/src/enums/user.rs#L1-L212)

### 数据库迁移与存储
- **迁移文件**：`migrations/20260810000000_users_preferences.sql`
- **字段类型**：TEXT NOT NULL DEFAULT ''（STRICT 表，空字符串表示未设置）
- **用途**：承载用户自述偏好（声明式画像），内容为自由文本（Markdown 书写）
- **访问控制**：只允许用户本人通过 update_current_user 修改，Agent 无写入路径

章节来源
- [migrations/20260810000000_users_preferences.sql:1-13](migrations/20260810000000_users_preferences.sql#L1-L13)

### 前端界面增强
- **个人资料页面**：新增"我的偏好"区块，支持 Markdown 格式编辑与展示
- **编辑模式**：textarea 编辑态与 MarkdownRenderer 展示态切换
- **保存逻辑**：通过 update_current_user API 保存偏好设置
- **用户体验**：支持空状态提示、编辑按钮、完成编辑等操作

章节来源
- [frontend/src/pages/user/profile.rs:1-148](frontend/src/pages/user/profile.rs#L1-L148)

## 依赖关系分析
- Handler 依赖：
  - RequestContext（来自 pkg）
  - Domain（organization::domain().user_manage()）
  - DTO（common::api::user）
  - 错误类型（common::error）
- Domain 依赖：
  - DAL（user_dal）
  - Models（UserPo）
  - Enums（UserRole、UserStatus）
- 耦合与内聚：
  - Handler 与 Domain 解耦良好，职责清晰
  - Domain 对 DAL 的依赖通过 trait 抽象，便于替换实现
  - DTO 与 Models 分离，避免 PO 泄露到上层

```mermaid
graph LR
H["Handlers (profile)"] --> D["Domain (organization.user_manage)"]
D --> L["DAL (user_dal)"]
L --> S["DAO (SQL/存储)"]
H --> DTO["DTOs (common::api::user)"]
D --> M["Models (UserPo)"]
DTO --> E["Enums (UserRole, UserStatus)"]
M --> P["Preferences Field"]
```

**图表来源**
- [src/handlers/user/profile/get_current_user.rs:1-71](src/handlers/user/profile/get_current_user.rs#L1-L71)
- [src/handlers/user/profile/update_current_user.rs:1-195](src/handlers/user/profile/update_current_user.rs#L1-L195)
- [src/service/domain/organization/user.rs:1-124](src/service/domain/organization/user.rs#L1-L124)
- [common/src/api/user.rs:1-228](common/src/api/user.rs#L1-L228)
- [src/models/user.rs:1-146](src/models/user.rs#L1-L146)
- [common/src/enums/user.rs:1-212](common/src/enums/user.rs#L1-L212)

章节来源
- [src/handlers/user/profile/get_current_user.rs:1-71](src/handlers/user/profile/get_current_user.rs#L1-L71)
- [src/handlers/user/profile/update_current_user.rs:1-195](src/handlers/user/profile/update_current_user.rs#L1-L195)
- [src/service/domain/organization/user.rs:1-124](src/service/domain/organization/user.rs#L1-L124)

## 性能考虑
- 单次查询：获取与更新均基于用户 ID 精确查询，时间复杂度 O(1)
- 缓存建议：若高频读取当前用户信息，可在应用层引入短期缓存（如内存缓存），注意失效策略
- 批量操作：当前接口不涉及批量，如需列表或统计，建议在 Domain/DAL 层增加分页与索引优化
- 序列化开销：DTO 与 Model 映射简单，开销较低
- **偏好字段优化**：preferences 字段为 TEXT 类型，大文本内容可能影响查询性能，建议合理使用

[本节为通用指导，不直接分析具体文件]

## 故障排查指南
- 常见错误
  - 未登录：RequestContext 缺少 user_id，返回 400
  - 用户不存在：根据 user_id 查询不到记录，返回 404
  - 更新失败：Domain/DAL 层异常，需检查数据库连接与事务
  - **Agent 安全守卫触发**：Agent 上下文调用时 preferences 字段被忽略
- 排查步骤
  - 确认 JWT 是否正确注入 RequestContext
  - 检查 Domain 层是否抛出业务异常
  - 查看 DAL/DAO 层日志与 SQL 执行结果
  - 验证请求体字段是否符合 DTO 定义
  - **检查 agent_id 上下文，确认是否为 Agent 调用**

章节来源
- [src/handlers/user/profile/get_current_user.rs:22-35](src/handlers/user/profile/get_current_user.rs#L22-L35)
- [src/handlers/user/profile/update_current_user.rs:23-57](src/handlers/user/profile/update_current_user.rs#L23-L57)

## 结论
当前用户管理模块提供了安全的个人信息获取与更新能力，**新增用户偏好双源机制**，支持用户通过 Markdown 格式设置个人偏好。模块遵循严格的分层架构与最小权限原则，实现了 Agent 安全守卫，确保只有真人用户才能修改偏好数据。后续可扩展头像上传等功能，保持与现有 DTO 和 Domain 的一致性。

[本节为总结性内容，不直接分析具体文件]

## 附录：完整示例与最佳实践

### API 端点与数据契约
- GET /api/v1/user/me
  - 请求：无请求体
  - 响应：GetCurrentUserResponse.data = UserInfoResponse（**包含 preferences 字段**）
- PUT /api/v1/user/me
  - 请求：UpdateCurrentUserRequest（可选字段：display_name、email、password_hash、**preferences**）
  - 响应：UpdateCurrentUserResponse.data = UserInfoResponse（**包含 preferences 字段**）

**更新**：所有响应现在都包含用户偏好信息，支持 Markdown 格式内容。

章节来源
- [common/src/api/user.rs:8-60](common/src/api/user.rs#L8-L60)

### 调用序列图（获取与更新）
```mermaid
sequenceDiagram
participant C as "客户端"
participant H1 as "Handler : get_current_user"
participant H2 as "Handler : update_current_user"
participant D as "Domain : user_manage"
participant DL as "DAL : user_dal"
C->>H1 : "GET /api/v1/user/me"
H1->>D : "get_user_by_id(user_id)"
D->>DL : "find_by_id(user_id)"
DL-->>D : "UserPo(含 preferences)"
D-->>H1 : "UserPo"
H1-->>C : "UserInfoResponse(含 preferences)"
C->>H2 : "PUT /api/v1/user/me {display_name?, email?, password_hash?, preferences?}"
H2->>H2 : "Agent 安全守卫检查"
H2->>D : "get_user_by_id(user_id)"
D-->>H2 : "UserPo"
H2->>D : "update_user(UserPo)"
D-->>H2 : "成功"
H2-->>C : "UpdateCurrentUserResponse"
```

**图表来源**
- [src/handlers/user/profile/get_current_user.rs:18-70](src/handlers/user/profile/get_current_user.rs#L18-L70)
- [src/handlers/user/profile/update_current_user.rs:18-102](src/handlers/user/profile/update_current_user.rs#L18-L102)
- [src/service/domain/organization/user.rs:119-122](src/service/domain/organization/user.rs#L119-L122)

### 安全与隐私保护
- 身份认证：JWT 中间件确保 RequestContext.user_id 有效
- 最小权限：仅允许更新白名单字段，禁止修改角色、状态、组织ID
- 敏感信息：密码以哈希形式存储，不在响应中暴露
- 审计追踪：updated_at 与 modified_by 记录更新时间与操作者
- **Agent 安全守卫**：防止 Agent 通过工具调用修改用户偏好数据
- **偏好数据隔离**：用户自述偏好与 Agent 观察总结的偏好走不同数据源

章节来源
- [src/handlers/user/profile/update_current_user.rs:40-57](src/handlers/user/profile/update_current_user.rs#L40-L57)
- [src/models/user.rs:37-41](src/models/user.rs#L37-L41)

### 用户偏好最佳实践
- **内容格式**：使用 Markdown 格式编写偏好，支持列表、强调、链接等语法
- **内容长度**：建议控制在合理范围内，避免过长的偏好描述影响性能
- **内容质量**：明确表达沟通习惯、回复风格、专业领域等关键信息
- **更新频率**：根据需要定期更新偏好，确保与当前需求保持一致
- **隐私考虑**：避免在偏好中包含敏感个人信息

章节来源
- [frontend/src/pages/user/profile.rs:81-112](frontend/src/pages/user/profile.rs#L81-L112)
- [migrations/20260810000000_users_preferences.sql:1-13](migrations/20260810000000_users_preferences.sql#L1-L13)

### 扩展建议（头像上传）
- 头像上传：建议复用附件/资源管理模块，新增头像专用用途标识与访问控制
- 偏好设置：已完成实现，支持 Markdown 格式的个性化偏好配置

[本节为概念性扩展建议，不直接分析具体文件]