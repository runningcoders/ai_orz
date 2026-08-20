---
kind: knowledge_card
name: Entity Query List Search 三分查询模式：push_query_filters 复用 WHERE + PagedResult T
  map 全链路 + list query search 三 Handler 职责二分
category: 知识架构
scope:
- common/src/api/**/*.rs
- src/service/dao/*/sqlite.rs
- src/service/dal/*.rs
- src/service/domain/*/mod.rs
- src/handlers/**/*.rs
source_files:
- common/src/api/mod.rs#L55-L83
- common/src/api/agent.rs#L282-L344
- src/service/dao/agent/sqlite.rs#L109-L382
- src/service/dal/agent.rs#L98-L193
- src/handlers/hr/agent/list_agents.rs#L1-L61
- src/handlers/hr/agent/query_agents.rs#L1-L68
- src/handlers/hr/agent/search_agents.rs#L1-L67
- docs/archive/design-archive/entity_list_query_search_design.md
- docs/archive/plan-archive/批量查询与通用Query接口增强重构.md
- docs/archive/plan-archive/Query接口分页与List接口简化重构.md
- docs/wiki/zh/content/架构设计/API协议规范/API协议规范.md
- docs/wiki/zh/content/核心模块/服务层/数据对象层 (DAO)/数据对象层 (DAO).md
- docs/wiki/zh/content/项目概述/核心功能特性/综合搜索能力/知识图谱搜索.md
- docs/wiki/zh/content/API 参考/RESTful API/RESTful API.md

---

# §1 概述与定位

本知识卡沉淀 AI Orz 全实体（Agent/Tool/Skill/Project/Task）统一采用的「三分查询模式」设计规范：将实体查询场景拆分为 **list（默认分页列表）**、**query（条件精确过滤）**、**search（语义关键词搜索）** 三种职责清晰的接口。三者共享同一套 `PagedResult<T>` 分页返回类型与 `push_query_filters` WHERE 子句构造逻辑，避免过滤条件在多处维护导致遗漏。全链路从 DTO → DAO → DAL → Domain → Handler → Frontend 采用对称模式，前端根据「无关键词无过滤→list；有过滤无关键词→query；有关键词→search」三场景自动切换调用。

# §2 关键文件表

| 角色 | 路径 | 关键锚点 |
|------|------|----------|
| PagedResult 泛型 + PaginationParams | common/src/api/mod.rs | L55-L83 `PaginationParams`（limit/offset）与 `PagedResult<T>`（items+total）定义，含 `.map()` 保持 total 不变的泛型转换方法 |
| Agent DTO 三分请求结构 | common/src/api/agent.rs | L282-L344 `ListAgentsRequest`（仅分页）/`AgentQueryRequest`（完整过滤+分页）/`SearchAgentsRequest`（keyword+过滤+分页）/`SearchAgentsResponse=PagedResult<AgentListItem>` |
| Agent DAO push_query_filters + query/list/count/search | src/service/dao/agent/sqlite.rs | L109-L382 `query` 方法 COUNT+LIST 复用 `push_query_filters`（L331-L382）；`search_agents` FTS5 JOIN + 复用过滤条件 + LIMIT 20 + OFFSET；`count` 复用 push_query_filters |
| Agent DAL query/count/search 透传 | src/service/dal/agent.rs | L98-L193 trait 签名：`query→PagedResult<Agent>`、`count→u64`、`search→PagedResult<Agent>`（search 封装三态匹配+综合排序+内存态过滤） |
| list_agents Handler | src/handlers/hr/agent/list_agents.rs | L1-L61 GET 请求，语法糖：构造 `AgentQuery{exclude_status=Deleted, pagination}` 统一走 Domain.query，返回 PagedResult.map |
| query_agents Handler | src/handlers/hr/agent/query_agents.rs | L1-L68 POST body，完整过滤字段透传 AgentQuery，返回 PagedResult.map |
| search_agents Handler | src/handlers/hr/agent/search_agents.rs | L1-L67 POST body，构造 `AgentSearch{keyword, filters=AgentQuery{...}}` 调用 Domain.search_agents |
| Design 规范文档 | docs/archive/design-archive/entity_list_query_search_design.md | 三分场景定义 + 六层（DTO/DAO/DAL/Domain/Handler/Frontend）改造要点 + 检查清单 + 各实体状态表 |
| Plan 重构计划 1 | docs/archive/plan-archive/批量查询与通用Query接口增强重构.md | query 是核心/list 是语法糖原则 + 5 实体三层对称模式 + IN 400 分块上限红线 |
| Plan 重构计划 2 | docs/archive/plan-archive/Query接口分页与List接口简化重构.md | 姊妹计划：分页参数统一 + list 接口简化延续同一原则 |

# §3 架构与约定

## 3.1 三接口 HTTP 签名与职责二分

| 接口 | HTTP Method | 路径 | 参数载体 | 核心职责 |
|------|-------------|------|----------|----------|
| list_xxx | GET | `/api/v1/{domain}/{entities}` | query string（仅 pagination + ids） | 语法糖最简场景；内部固定排除 Deleted；封装成 XxxQuery 走 Domain.query |
| query_xxx | POST | `/api/v1/{domain}/{entities}/query` | JSON body（完整过滤字段 + pagination） | 条件精确过滤；业务层复杂筛选；不涉及关键词语义 |
| search_xxx | POST | `/api/v1/{domain}/{entities}/search` | JSON body（keyword + 完整过滤字段 + pagination） | 语义相关性；FTS5 关键词 + 向量混合搜索；**复用 query 全部过滤字段** |

**三分职责红线**：
- list 是 query 超集语法糖（内部 100% 走 Domain.query，**禁止**在 handler 中再写 DAO 查询或内存 filter）
- query **禁止**含 keyword 字段（keyword = search 专属；若 query 有 keyword 仅作兼容并打印 log_warn 标记 deprecated）
- search 的 filters 字段必须完整包含 query 的全部过滤能力（通过 `Search.filters: XxxQuery` 结构体复用，不得重新定义一套字段）

## 3.2 push_query_filters 复用 WHERE 子句

每个实体 DAO 的 sqlite.rs 必须抽取独立的 `push_query_filters(builder, query)` 纯函数，被以下三个方法**共同复用**：
1. `query` → COUNT 语句 push 一次 + LIST 语句 push 一次
2. `count` → COUNT 语句 push 一次
3. `search` → FTS5 MATCH 后追加 push 一次（与 query 共享过滤逻辑）

这意味着「新增一个过滤字段」只需在 `push_query_filters` 中追加一次条件，query/count/search 三处自动生效，消除三处手写导致遗漏的缺陷。

## 3.3 PagedResult<T> 全链路统一泛型

三接口返回类型统一为 `PagedResult<T> { items: Vec<T>, total: usize }`：
- DAO 层：返回 `PagedResult<XxxPo>`（PO 类型）
- DAL/Domain 层：返回 `PagedResult<Xxx>`（领域对象类型）
- Handler 层：调用 `page.map(|entity| XxxListItem {...})` 保持 total 不变，仅 items 字段映射
- 前端：三接口返回类型完全对齐，统一通过 `.map(|p| p.items)` 取列表，`p.total` 用于分页器

`.map()` 方法在 common/src/api/mod.rs#L77-L82 提供，保证 total 不会被误改。

## 3.4 搜索上限 MAX_SEARCH_RESULTS = 20

search 方法（DAO/DAL/Domain 三层）均严格限制最大返回 20 条：
- DAO 层：`std::cmp::min(filters.pagination.limit.unwrap_or(20), 20)`
- DAL 层：`entities.truncate(20)` 向量聚合后截断
- 前端：搜索框输入提示语引导用户换关键词而非翻页

# §4 硬约束与红线

1. **过滤条件单一来源红线**：每个实体的 WHERE 过滤条件必须唯一存在于 `push_query_filters` 函数中；query/count/search 三处均必须调用该函数，**禁止**在任何方法内手写独立的 WHERE 条件分支
2. **list 语法糖红线**：list handler **绝对不能**出现 Rust 层内存 `.filter()` 或调用 DAO.find_all 后手动分页；必须构造 XxxQuery（含 exclude_status=Deleted）统一走 Domain.query
3. **search POST 路由红线**：search handler 路由必须注册为 POST body，**禁止**使用 GET query string（keyword + 过滤条件组合会超出 URL 长度限制且语义不安全）
4. **search filters 复用红线**：`XxxSearch` 结构体必须持有 `filters: XxxQuery` 字段复用过滤条件；**禁止**在 Search 结构体中重新独立定义 status/ids/tags 等过滤字段（易与 Query 版本漂移）
5. **PagedResult.map 不变 total 红线**：Handler 层从领域对象映射到 ListItem 时，**必须**使用 `PagedResult::map()`，禁止手动解构后重新构造（有 total 算错风险）
6. **前端三场景切换齐全红线**：前端 load_data 函数必须实现三条分支：无关键词+无过滤→list；有过滤无关键词→query；有关键词→search；**禁止省略 query 分支**（仅 list+search 二分）
7. **search 携带完整过滤条件红线**：前端 search 请求必须同时携带 keyword + 当前 UI 所有过滤字段（status/roles/category 等），**禁止**仅传 keyword 导致筛选条件丢失
