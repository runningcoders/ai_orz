# API 客户端

<cite>
**本文引用的文件**
- [frontend/src/api/mod.rs](file://frontend/src/api/mod.rs)
- [frontend/src/api/auth.rs](file://frontend/src/api/auth.rs)
- [frontend/src/api/organization.rs](file://frontend/src/api/organization.rs)
- [frontend/src/api/project.rs](file://frontend/src/api/project.rs)
- [frontend/src/api/hr.rs](file://frontend/src/api/hr.rs)
- [frontend/src/store/auth.rs](file://frontend/src/store/auth.rs)
- [frontend/src/config.rs](file://frontend/src/config.rs)
- [frontend/Cargo.toml](file://frontend/Cargo.toml)
- [common/src/api/mod.rs](file://common/src/api/mod.rs)
</cite>

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
10. [附录：接口清单与示例](#附录接口清单与示例)

## 简介
本文件面向 AI Orz 前端应用，系统化说明基于 Rust 的前端 HTTP 客户端实现与业务 API 调用方式。内容涵盖统一请求封装、认证令牌管理（基于 HttpOnly Cookie）、错误处理机制、响应拦截器、各业务模块的 API 接口定义（认证、用户、组织、项目、Agent、工具等）、状态管理模式（全局状态存储、异步数据加载、缓存策略），以及调试与测试指南和性能优化建议。

## 项目结构
前端采用 Dioxus + reqwest 构建，API 客户端集中在 frontend/src/api 下，按业务域拆分模块；通用类型与统一响应格式在 common/src/api 中共享；配置与运行时 URL 拼接在 frontend/src/config.rs；全局状态（登录态、角色）在 frontend/src/store 中管理。

```mermaid
graph TB
A["前端页面/组件"] --> B["api 模块<br/>auth / organization / project / hr / finance ..."]
B --> C["统一请求封装<br/>api_get / api_post / api_put / api_delete"]
C --> D["HTTP 客户端<br/>reqwest Client"]
C --> E["配置中心<br/>current_config().api_url()"]
C --> F["错误处理<br/>ApiError / handle_unauthorized"]
B --> G["业务 DTO<br/>common::api::*"]
H["全局状态<br/>store/auth.rs"] --> A
```

图表来源
- [frontend/src/api/mod.rs:28-35](file://frontend/src/api/mod.rs#L28-L35)
- [frontend/src/config.rs:76-79](file://frontend/src/config.rs#L76-L79)
- [common/src/api/mod.rs:6-49](file://common/src/api/mod.rs#L6-L49)

章节来源
- [frontend/src/api/mod.rs:1-433](file://frontend/src/api/mod.rs#L1-L433)
- [frontend/src/config.rs:1-85](file://frontend/src/config.rs#L1-L85)
- [common/src/api/mod.rs:1-156](file://common/src/api/mod.rs#L1-L156)

## 核心组件
- 统一请求封装：提供 GET/POST/PUT/DELETE 及文本、Multipart 上传等能力，统一解析 ApiResponse<T>，并处理 401 重定向。
- 错误模型：统一的 ApiError，包含 http_status、error_code、message，便于上层统一处理。
- 配置与 URL 构造：通过 FrontendConfig 动态拼接后端 API Base URL，支持编译时默认值与 localStorage 覆盖。
- 认证状态：基于 HttpOnly Cookie 的 JWT，前端仅持久化登录标志与角色到 localStorage，并在 401 时自动清理并重定向。
- 分页与查询参数：提供 build_pagination_url 与 build_query_string 辅助函数，统一构造查询串。

章节来源
- [frontend/src/api/mod.rs:87-287](file://frontend/src/api/mod.rs#L87-L287)
- [frontend/src/api/mod.rs:289-318](file://frontend/src/api/mod.rs#L289-L318)
- [frontend/src/api/mod.rs:320-403](file://frontend/src/api/mod.rs#L320-L403)
- [frontend/src/api/mod.rs:405-432](file://frontend/src/api/mod.rs#L405-L432)
- [frontend/src/config.rs:12-85](file://frontend/src/config.rs#L12-L85)
- [frontend/src/store/auth.rs:13-57](file://frontend/src/store/auth.rs#L13-L57)
- [common/src/api/mod.rs:6-73](file://common/src/api/mod.rs#L6-L73)

## 架构总览
前端通过业务 API 模块调用统一请求封装，统一封装负责：
- 组装 URL（基于当前配置）
- 发送 HTTP 请求
- 检查 HTTP 状态码，必要时触发 401 处理
- 解析 ApiResponse<T>，提取 data 或返回错误
- 将网络层错误与业务错误统一为 ApiError

```mermaid
sequenceDiagram
participant UI as "页面/组件"
participant API as "业务API模块"
participant Core as "统一请求封装"
participant HTTP as "reqwest Client"
participant CFG as "配置中心"
participant ERR as "错误处理"
UI->>API : 调用具体接口(如 list_projects)
API->>Core : api_get / api_post / api_put / api_delete
Core->>CFG : current_config().api_url(path)
Core->>HTTP : 发送请求
HTTP-->>Core : Response
Core->>ERR : 非成功状态? 是则 handle_unauthorized + parse_error_response
Core-->>API : Result<T, ApiError>
API-->>UI : 返回数据或错误
```

图表来源
- [frontend/src/api/mod.rs:28-35](file://frontend/src/api/mod.rs#L28-L35)
- [frontend/src/api/mod.rs:87-114](file://frontend/src/api/mod.rs#L87-L114)
- [frontend/src/config.rs:76-79](file://frontend/src/config.rs#L76-L79)

## 详细组件分析

### 统一请求封装与错误处理
- 方法族：api_get、api_get_or_default、api_post、api_post_empty、api_put、api_put_empty、api_delete、api_get_text、api_post_multipart。
- 错误处理：
  - 网络错误：network_err 包装为 ApiError。
  - HTTP 错误：parse_error_response 读取 body，尝试解析 error_code 与 message。
  - 401 处理：handle_unauthorized 清除登录态并跳转至登录页。
- 响应体：统一解析 ApiResponse<T>，若 code != 0 则视为业务错误。

```mermaid
flowchart TD
Start(["进入请求封装"]) --> BuildURL["构造URL<br/>current_config().api_url(path)"]
BuildURL --> Send["发送HTTP请求"]
Send --> CheckStatus{"HTTP 成功?"}
CheckStatus -- 否 --> Handle401["handle_unauthorized(status)"]
Handle401 --> ParseErr["parse_error_response(resp)"]
ParseErr --> ReturnErr["返回 ApiError"]
CheckStatus -- 是 --> ParseJSON["解析 ApiResponse<T>"]
ParseJSON --> IsSuccess{"is_success() ?"}
IsSuccess -- 否 --> ReturnBizErr["返回 ApiError(业务错误)"]
IsSuccess -- 是 --> ExtractData["提取 data"]
ExtractData --> ReturnOk["返回 T"]
```

图表来源
- [frontend/src/api/mod.rs:87-114](file://frontend/src/api/mod.rs#L87-L114)
- [frontend/src/api/mod.rs:143-174](file://frontend/src/api/mod.rs#L143-L174)
- [frontend/src/api/mod.rs:203-234](file://frontend/src/api/mod.rs#L203-L234)
- [frontend/src/api/mod.rs:263-287](file://frontend/src/api/mod.rs#L263-L287)
- [frontend/src/api/mod.rs:38-45](file://frontend/src/api/mod.rs#L38-L45)
- [frontend/src/api/mod.rs:47-85](file://frontend/src/api/mod.rs#L47-L85)

章节来源
- [frontend/src/api/mod.rs:87-287](file://frontend/src/api/mod.rs#L87-L287)
- [frontend/src/api/mod.rs:289-318](file://frontend/src/api/mod.rs#L289-L318)

### 认证与令牌管理
- 认证方式：基于 HttpOnly Cookie 的 JWT，前端不直接持有 token。
- 本地持久化：localStorage 保存登录标志与角色，用于 UI 判断与刷新恢复。
- 登出流程：clear_login_state 清理本地标记；logout 同时重置内存中的 AuthState。
- 401 处理：统一在请求封装中触发，自动清理登录态并跳转登录页。

```mermaid
sequenceDiagram
participant UI as "页面"
participant Store as "AuthState"
participant API as "统一请求封装"
participant Backend as "后端服务"
UI->>Store : restore() 恢复登录态与角色
UI->>API : 发起受保护接口
API->>Backend : 携带 Cookie 的请求
Backend-->>API : 401 未授权
API->>Store : clear_login_state()
API-->>UI : ApiError(含错误信息)
UI->>UI : 跳转 /login
```

图表来源
- [frontend/src/store/auth.rs:13-43](file://frontend/src/store/auth.rs#L13-L43)
- [frontend/src/api/mod.rs:38-45](file://frontend/src/api/mod.rs#L38-L45)

章节来源
- [frontend/src/store/auth.rs:1-94](file://frontend/src/store/auth.rs#L1-L94)
- [frontend/src/api/mod.rs:38-45](file://frontend/src/api/mod.rs#L38-L45)

### 配置与 URL 构造
- 配置优先级：localStorage 覆盖 > 编译时默认值。
- 默认 Base URL：从编译时生成的 server.listen_addr 推导，自动处理 0.0.0.0 替换为 localhost。
- URL 拼接：api_path 通过 trim_end_matches('/') 后拼接，避免重复斜杠。

章节来源
- [frontend/src/config.rs:12-85](file://frontend/src/config.rs#L12-L85)

### 业务模块 API 概览
- 认证模块：系统初始化检查、异步初始化、登录、登出。
- 组织与用户：获取当前组织、更新组织、列出用户、创建/更新/删除用户、当前用户信息与更新。
- 项目与任务：项目列表/查询/搜索/详情/创建/更新/状态变更；任务列表/查询/搜索/详情/创建/更新/状态/进度；产物 CRUD 与内容获取。
- HR（Agent/技能/工具）：Agent 列表/查询/搜索/详情/创建/更新/状态/删除；外部 Agent；工具包/技能包安装卸载；单技能安装卸载；技能库 CRUD；Skill 文件管理；Agent 工具绑定/解绑；记忆搜索与推荐节点。

章节来源
- [frontend/src/api/auth.rs:10-40](file://frontend/src/api/auth.rs#L10-L40)
- [frontend/src/api/organization.rs:13-61](file://frontend/src/api/organization.rs#L13-L61)
- [frontend/src/api/project.rs:16-162](file://frontend/src/api/project.rs#L16-L162)
- [frontend/src/api/hr.rs:25-310](file://frontend/src/api/hr.rs#L25-L310)

### 分页与查询参数构造
- 分页：build_pagination_url 将 PaginationParams(limit/offset) 转为 query string。
- 查询：build_query_string 过滤 None 项并拼接 key=value。

章节来源
- [frontend/src/api/mod.rs:405-432](file://frontend/src/api/mod.rs#L405-L432)
- [common/src/api/mod.rs:55-73](file://common/src/api/mod.rs#L55-L73)

### 多部分表单上传
- 使用 web-sys 原生 fetch 与 FormData，设置 SameOrigin 凭据，解析 ApiResponse<T>。
- 错误路径：构造 Request、fetch、Response 转换、JSON 解析均捕获并包装为 ApiError。

章节来源
- [frontend/src/api/mod.rs:320-403](file://frontend/src/api/mod.rs#L320-L403)

## 依赖关系分析
- 运行时依赖：Dioxus（UI框架）、reqwest（HTTP 客户端）、web-sys/wasm-bindgen（浏览器 API 桥接）。
- 共享类型：common::api 提供统一响应格式、分页结构与业务 DTO，前后端共用保证一致性。
- 配置注入：编译期生成配置，运行期通过 current_config() 获取。

```mermaid
graph LR
FE["前端应用"] --> REQ["reqwest"]
FE --> WS["web-sys / wasm-bindgen"]
FE --> DX["dioxus"]
FE --> COM["common::api (DTOs)"]
FE --> CFG["config.rs"]
```

图表来源
- [frontend/Cargo.toml:11-25](file://frontend/Cargo.toml#L11-L25)
- [common/src/api/mod.rs:1-156](file://common/src/api/mod.rs#L1-L156)

章节来源
- [frontend/Cargo.toml:1-26](file://frontend/Cargo.toml#L1-L26)
- [common/src/api/mod.rs:1-156](file://common/src/api/mod.rs#L1-L156)

## 性能考虑
- 连接复用：reqwest Client 使用 OnceLock 全局单例，减少握手开销。
- 默认响应：对可选列表接口优先使用 api_get_or_default，降低空响应分支复杂度。
- 查询裁剪：通过 build_query_string 精确传递 with_stats、with_artifacts 等开关，减少不必要的数据传输。
- 上传优化：multipart 上传使用原生 fetch，避免额外序列化开销。
- 缓存策略：建议在页面层结合 Signal/缓存层对高频只读数据进行短期缓存（例如组织信息、当前用户信息），以减少重复请求。

[本节为通用指导，不直接分析具体文件]

## 故障排查指南
- 401 未授权：确认 Cookie 是否有效；检查 handle_unauthorized 是否触发；确认登出流程是否正确清理本地状态。
- JSON 解析失败：检查后端返回结构是否为 ApiResponse<T>；确认字段映射与类型一致。
- 网络错误：检查跨域与代理配置；确认 base URL 正确且可访问。
- 上传失败：确认 FormData 构造正确；检查浏览器环境 window/fetch 可用性。

章节来源
- [frontend/src/api/mod.rs:38-85](file://frontend/src/api/mod.rs#L38-L85)
- [frontend/src/api/mod.rs:320-403](file://frontend/src/api/mod.rs#L320-L403)

## 结论
该 API 客户端以统一封装为核心，结合共享 DTO 与配置管理，实现了稳定、一致的 HTTP 交互体验。通过集中式错误处理与 401 拦截，提升了用户体验与安全性。各业务模块清晰分层，便于扩展与维护。建议在生产环境中结合缓存与重试策略进一步优化性能与鲁棒性。

[本节为总结，不直接分析具体文件]

## 附录：接口清单与示例

### 认证
- 检查系统初始化：GET /api/v1/organization/initialize/check
- 异步初始化系统：POST /api/v1/organization/initialize
- 登录：POST /api/v1/organization/auth/login
- 登出：POST /api/v1/organization/auth/logout

章节来源
- [frontend/src/api/auth.rs:10-40](file://frontend/src/api/auth.rs#L10-L40)

### 组织与用户
- 公开获取组织列表：GET /api/v1/organization/list
- 获取当前组织：GET /api/v1/organization/me
- 更新当前组织：PUT /api/v1/organization/me
- 获取当前用户信息：GET /api/v1/user/me
- 更新当前用户信息：PUT /api/v1/user/me
- 列出用户：GET /api/v1/organization/user/me/list
- 创建用户：POST /api/v1/organization/user/
- 更新用户：PUT /api/v1/organization/user/update
- 删除用户：DELETE /api/v1/organization/user/id/{user_id}

章节来源
- [frontend/src/api/organization.rs:13-61](file://frontend/src/api/organization.rs#L13-L61)

### 项目与任务
- 项目列表：GET /api/v1/projects?limit=&offset=
- 项目查询：POST /api/v1/projects/query
- 项目搜索：POST /api/v1/projects/search
- 项目详情：GET /api/v1/projects/{id}?with_stats=&with_model_call_stats=&stats_time_start=&stats_time_end=&stats_interval=&with_artifacts=
- 创建项目：POST /api/v1/projects
- 更新项目：PUT /api/v1/projects/{id}
- 更新项目状态：PUT /api/v1/projects/{id}/status
- 任务列表（项目内）：GET /api/v1/projects/{project_id}/tasks
- 任务查询：POST /api/v1/tasks/query
- 任务搜索：POST /api/v1/tasks/search
- 任务详情：GET /api/v1/tasks/{id}?with_stats=&with_model_call_stats=&stats_time_start=&stats_time_end=&stats_interval=&with_artifacts=
- 创建任务：POST /api/v1/tasks
- 更新任务：PUT /api/v1/tasks/{id}
- 更新任务状态：PUT /api/v1/tasks/{id}/status
- 更新任务进度：PUT /api/v1/tasks/{id}/progress
- 产物列表：GET /api/v1/project/artifacts?project_id=
- 创建产物：POST /api/v1/project/artifacts
- 删除产物：DELETE /api/v1/project/artifacts/{id}
- 获取产物内容：GET /api/v1/project/artifacts/{id}/content
- 更新产物：PUT /api/v1/project/artifacts/{artifact_id}

章节来源
- [frontend/src/api/project.rs:16-162](file://frontend/src/api/project.rs#L16-L162)

### HR（Agent/技能/工具）
- Agent 列表：GET /api/v1/hr/agents?limit=&offset=
- Agent 查询：POST /api/v1/hr/agents/query
- 前台可用 Agent：GET /api/v1/hr/agents/reception
- Agent 搜索：POST /api/v1/hr/agents/search
- Agent 详情：GET /api/v1/hr/agents/{id}?with_stats=&with_model_call_stats=&stats_time_start=&stats_time_end=&stats_interval=
- 创建 Agent：POST /api/v1/hr/agents
- 创建外部 Agent：POST /api/v1/hr/agents/external
- 更新 Agent：PUT /api/v1/hr/agents/{id}
- 更新 Agent 状态：PUT /api/v1/hr/agents/{id}/status
- 删除 Agent：DELETE /api/v1/hr/agents/{id}
- 工具包安装/卸载：POST/DELETE /api/v1/hr/agents/{agent_id}/tool-packs/{tag}
- 技能包安装/卸载：POST/DELETE /api/v1/hr/agents/{agent_id}/skill-packs/{tag}
- 单技能安装/卸载：POST/DELETE /api/v1/hr/agents/{agent_id}/skills/{skill_id}
- 技能库列表/查询/搜索：GET/POST /api/v1/hr/skills
- 技能详情：GET /api/v1/hr/skills/{id}
- 技能文件列表/内容/更新：GET/GET/PUT /api/v1/hr/skills/{skill_id}/files{...}
- 工具绑定/解绑：POST/DELETE /api/v1/hr/agents/{agent_id}/tools/{tool_id}/bind
- 记忆搜索/查询/遍历：POST /api/v1/hr/agents/search_memory, /query_memory, /search_memory
- 推荐种子节点：POST /api/v1/hr/agents/recommend_seed_nodes

章节来源
- [frontend/src/api/hr.rs:25-310](file://frontend/src/api/hr.rs#L25-L310)

### 统一响应与分页
- 统一响应：ApiResponse<T> { code, message, data }
- 分页参数：PaginationParams { limit, offset }
- 分页结果：PagedResult<T> { items, total }

章节来源
- [common/src/api/mod.rs:6-73](file://common/src/api/mod.rs#L6-L73)