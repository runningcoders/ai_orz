# 实体列表/查询/搜索接口设计规范

> 📦 归档标记（2026-08-16）：归档冻结。保留原因：entity_list_query_search_design 设计文档归档冻结，设计决策已沉淀至 wiki 长文。生效方案：见源码和 wiki 长文。

> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 整体分层架构（Handler 层接口约定）
> - [pagination_and_count_convention.md](./pagination_and_count_convention.md) — 分页参数与通用 count 查询规范（本规范的分页部分的底层）
> - [full_entity_fts5_search_design.md](./full_entity_fts5_search_design.md) — 全实体 FTS5 + 向量混合搜索统一设计（search 接口底层）
> - [vector_search_architecture.md](./vector_search_architecture.md) — 向量搜索底层架构
> - 【② Plan 落地】[批量查询与通用Query接口增强重构.md](docs/archive/plan-archive/批量查询与通用Query接口增强重构.md) — query 核心/list 语法糖原则 + 5 实体三层对称模式
> - 【② Plan 落地】[Query接口分页与List接口简化重构.md](docs/archive/plan-archive/Query接口分页与List接口简化重构.md) — 姊妹计划：分页参数统一 + list 接口简化
> - 【③ Wiki 长文】[Agent 搜索与查询.md](docs/wiki/zh/content/项目概述/核心功能特性/Agent 全生命周期管理/Agent 搜索与查询.md) — Agent 实体 list/query/search 三接口样板
> - 【③ Wiki 长文】[Agent 搜索与推荐.md](docs/wiki/zh/content/功能模块/AI Agent 管理/Agent 搜索与推荐.md) — 前端搜索面板场景映射
> - 【③ Wiki 长文】[Agent 实体.md](docs/wiki/zh/content/数据模型/Agent 和技能模型/Agent 实体.md) — §AgentQuery DTO 过滤条件
> - 【③ Wiki 长文（Batch10 追加）】[API协议规范.md](docs/wiki/zh/content/架构设计/API协议规范/API协议规范.md) — 三分接口 HTTP 路由签名约定
> - 【③ Wiki 长文（Batch10 追加）】[数据对象层 (DAO).md](docs/wiki/zh/content/核心模块/服务层/数据对象层%20(DAO)/数据对象层%20(DAO).md) — push_query_filters 复用 WHERE 子句实现
> - 【③ Wiki 长文（Batch10 追加）】[知识图谱搜索.md](docs/wiki/zh/content/项目概述/核心功能特性/综合搜索能力/知识图谱搜索.md) — 综合搜索场景映射
> - 【③ Wiki 长文（Batch10 追加）】[RESTful API.md](docs/wiki/zh/content/API%20参考/RESTful%20API/RESTful%20API.md) — 三接口总览
> - 【④ RAG 卡 2 张（已有）】
>   - [三位一体混合搜索：FTS5 关键词 + 向量语义 + 合并排序（6 DAO 统一 search 模式 + 向量失败降级）](docs/wiki/knowledge/zh/三位一体混合搜索：FTS5%20关键词%20+%20向量语义%20+%20合并排序%20（6%20DAO%20统一%20search%20模式%20+%20向量失败降级）/三位一体混合搜索：FTS5%20关键词%20+%20向量语义%20+%20合并排序%20（6%20DAO%20统一%20search%20模式%20+%20向量失败降级）.md) — search 模式样板 + 加权融合
>   - [向量存储抽象 VectorStore + 多后端 + Vectorizable trait 统一索引入口 + embed_entity](docs/wiki/knowledge/zh/向量存储抽象%20VectorStore%20+%20多后端%20+%20Vectorizable%20trait%20统一索引入口%20+%20embed_entity/向量存储抽象%20VectorStore%20+%20多后端%20+%20Vectorizable%20trait%20统一索引入口%20+%20embed_entity.md) — Vectorizable 底层支撑 search 的向量层
> - 【④ RAG 卡（Batch10 新增）】[Entity Query List Search 三分查询模式：push_query_filters 复用 WHERE + PagedResult T map 全链路 + list query search 三 Handler 职责二分](docs/wiki/knowledge/zh/Entity%20Query%20List%20Search%20三分查询模式：push_query_filters%20复用%20WHERE%20+%20PagedResult%20T%20map%20全链路%20+%20list%20query%20search%20三%20Handler%20职责二分/Entity%20Query%20List%20Search%20三分查询模式：push_query_filters%20复用%20WHERE%20+%20PagedResult%20T%20map%20全链路%20+%20list%20query%20search%20三%20Handler%20职责二分.md) — 全链路三分查询规范 + 8 条硬约束红线

> 本文档总结 Agent 实体统一改造中沉淀的三场景设计经验，作为其他实体（Tool/Skill/Project/Task 等）改造的规范基准。
> 关联计划：[2026-07-31-unify-agent-search-query.md](superpowers/plans/2026-07-31-unify-agent-search-query.md)

---

## 一、三场景规范

所有拥有列表/查询/搜索功能的实体，都应提供三种接口对应三种场景：

| 接口 | HTTP | 场景 | 说明 |
|------|------|------|------|
| `list_xxx` | `GET /api/v1/{domain}/{entities}` | 默认列表 | 无条件获取，最简场景，仅支持分页 |
| `query_xxx` | `POST /api/v1/{domain}/{entities}/query` | 条件过滤 | 按 status/roles/tags 等条件精确筛选 |
| `search_xxx` | `POST /api/v1/{domain}/{entities}/search` | 关键词搜索 | FTS5 + 向量语义混合搜索，**同时支持完整过滤条件** |

**核心原则**：
1. 三种接口都返回 `PagedResult<T>`（含 `items` + `total`），支持分页
2. `search` 复用 `query` 的过滤条件（通过 `Search.filters: Query` 结构），在其基础上增加 keyword
3. 前端根据场景切换：**无条件 → list；有过滤条件 → query；有关键词 → search**
4. 搜索场景限制最大返回结果数 `MAX_SEARCH_RESULTS = 20`，避免关键词失控导致性能浪费

---

## 二、分层改造要点

### 2.1 DTO 层（`common/src/api/{entity}.rs`）

```rust
// 1. ListRequest：仅分页参数
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListXxxRequest {
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}

// 2. QueryRequest：完整过滤条件 + 分页
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct XxxQueryRequest {
    pub ids: Option<Vec<String>>,
    pub status: Option<XxxStatus>,
    // ... 其他业务过滤字段
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

// 3. SearchRequest：keyword + 完整过滤条件 + 分页（POST body）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct SearchXxxRequest {
    pub keyword: Option<String>,
    // 完整复用 QueryRequest 的过滤字段
    pub status: Option<XxxStatus>,
    // ... 其他业务过滤字段
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

// 4. Response 统一为 PagedResult 别名
pub type SearchXxxResponse = PagedResult<XxxListItem>;
```

**经验**：
- `SearchXxxResponse` 直接用 `PagedResult<XxxListItem>` 类型别名，避免定义独立的 Response 结构体
- `SearchXxxRequest` 字段应**完整复用** QueryRequest 的过滤字段（不要遗漏 ids/tags 等）
- 不要在 QueryRequest 暴露 `keyword` 字段（keyword 是 search 专属，query 中应标记 deprecated 或移除）

### 2.2 DAO 层（`src/service/dao/{entity}/`）

**结构体定义**（`mod.rs`）：

```rust
// Query 结构体：完整过滤条件 + 分页
pub struct XxxQuery {
    pub ids: Option<Vec<String>>,
    pub status: Option<XxxStatus>,
    pub exclude_status: Option<XxxStatus>,
    // ... 其他业务过滤字段
    pub pagination: common::api::PaginationParams,
}

// Search 结构体：keyword + 向量参数 + filters 复用 Query
pub struct XxxSearch {
    pub keyword: Option<String>,
    pub query_vector: Option<Vec<f32>>,
    pub top_k: usize,
    pub vector_distance_threshold: Option<f32>,
    pub filters: XxxQuery,  // ✅ 复用 Query
}
```

**SQLite 实现**（`sqlite.rs`）：

```rust
// 1. query 方法：COUNT + LIST 复用 push_query_filters
async fn query(&self, ctx, query: XxxQuery) -> Result<PagedResult<XxxPo>> {
    let mut count_builder = QueryBuilder::new("SELECT COUNT(*) FROM xxx WHERE 1=1");
    push_query_filters(&mut count_builder, &query);
    let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;

    let mut list_builder = QueryBuilder::new("SELECT ... FROM xxx WHERE 1=1");
    push_query_filters(&mut list_builder, &query);
    list_builder.push(" ORDER BY created_at DESC");
    // LIMIT + OFFSET 支持
    if let Some(limit) = query.pagination.limit {
        list_builder.push(" LIMIT ").push_bind(limit as i64);
    } else if query.pagination.offset.is_some() {
        list_builder.push(" LIMIT -1");  // SQLite 特殊语义：无上限但允许 OFFSET
    }
    if let Some(offset) = query.pagination.offset {
        list_builder.push(" OFFSET ").push_bind(offset as i64);
    }
    let items = list_builder.build_query_as().fetch_all(pool).await?;
    Ok(PagedResult { items, total: total as usize })
}

// 2. search 方法：FTS5 + 复用 push_query_filters + LIMIT + OFFSET
async fn search_xxx(&self, ctx, search: XxxSearch) -> Result<Vec<(XxxPo, Option<f32>)>> {
    let keyword = search.keyword.unwrap_or_default();
    if keyword.trim().is_empty() { return Ok(Vec::new()); }
    let escaped = escape_fts5_keyword(&keyword);
    let filters = search.filters;

    let mut builder = QueryBuilder::new(
        r#"SELECT m.*, xxx_fts.rank as fts_rank
           FROM xxx_fts JOIN xxx m ON xxx_fts.rowid = m.rowid
           WHERE xxx_fts MATCH "#
    );
    builder.push_bind(escaped);

    // ✅ 复用 push_query_filters（与 query 共享过滤逻辑）
    push_query_filters(&mut builder, &filters);

    builder.push(" ORDER BY xxx_fts.rank");
    // 搜索场景限制最大返回数量
    let search_limit = std::cmp::min(filters.pagination.limit.unwrap_or(20), 20);
    builder.push(" LIMIT ").push_bind(search_limit as i64);
    if let Some(offset) = filters.pagination.offset {
        builder.push(" OFFSET ").push_bind(offset as i64);
    }
    // ...
}
```

**经验**：
- `push_query_filters` 函数必须被 query 和 search **共同复用**，避免过滤条件在两处维护导致遗漏
- search 方法必须支持 OFFSET（与 query 一致的分页模式）
- search 的 LIMIT 默认 20，上限 20（`std::cmp::min(limit.unwrap_or(20), 20)`）
- 向量搜索方法（`search_vector`）也限制 top 20

### 2.3 DAL 层（`src/service/dal/{entity}.rs`）

```rust
#[async_trait::async_trait]
pub trait XxxDal: Send + Sync {
    async fn query(&self, ctx, query: XxxQuery) -> Result<PagedResult<Xxx>>;
    // ✅ search 返回 PagedResult（与 query 一致）
    async fn search(&self, ctx, search: XxxSearch) -> Result<PagedResult<Xxx>>;
    // ...
}

impl XxxDal for XxxDalImpl {
    async fn search(&self, ctx, search: XxxSearch) -> Result<PagedResult<Xxx>> {
        // ... 向量搜索 + FTS5 聚合 + 三态匹配 + 综合排序 ...

        // Step 8: 截断到 MAX_SEARCH_RESULTS + 内存态过滤 + 分页
        entities.truncate(20);

        let runtime_state_filter = search.filters.runtime_state;
        let pagination = search.filters.pagination.clone();
        let result = if let Some(target_state) = runtime_state_filter {
            // 内存态过滤复用方法（如有 runtime_state 类字段）
            Self::apply_runtime_state_filter(entities, target_state, pagination)
        } else {
            let total = entities.len();
            let offset = pagination.offset.unwrap_or(0);
            let limit = pagination.limit.unwrap_or(20);
            let items = entities.into_iter().skip(offset).take(limit).collect();
            PagedResult { items, total }
        };

        Ok(result)
    }
}
```

**经验**：
- search 返回 `PagedResult<T>`，`total` = 截断后的实际条数（最大 20）
- 内存态过滤（如 `runtime_state`）抽取为 `apply_runtime_state_filter` 内部方法，query 和 search 复用
- 向量搜索限制从 50 改为 20（与 FTS5 限制一致）
- 三层（FTS5 / 向量 / 聚合后）统一限制 MAX_SEARCH_RESULTS=20

### 2.4 Domain 层（`src/service/domain/{domain}/mod.rs`）

```rust
#[async_trait::async_trait]
pub trait XxxManage: Send + Sync {
    async fn query_xxx(&self, ctx, query: XxxQuery) -> Result<PagedResult<Xxx>>;
    // ✅ Domain trait 暴露 search 入口
    async fn search_xxx(&self, ctx, search: XxxSearch) -> Result<PagedResult<Xxx>>;
}
```

**经验**：
- Domain trait 必须暴露 `search` 方法（Agent 已有，其他实体常遗漏）
- 实现层直接委托 DAL：`self.xxx_dal.search(ctx, search).await`

### 2.5 Handler 层（`src/handlers/{domain}/{entity}/`）

```rust
// search_xxx.rs
#[register_handler_tool(
    id = "search_xxx",
    name = "search_xxx",
    description = "Search xxx by keyword with full filtering support.",
    params = "common::api::SearchXxxRequest",
    tags = "..."
)]
#[generate_http_handler]
pub async fn search_xxx(
    ctx: RequestContext,
    params: SearchXxxRequest,
) -> Result<PagedResult<XxxListItem>> {
    let search = XxxSearch {
        keyword: params.keyword,
        filters: XxxQuery {
            status: params.status,
            exclude_status: Some(XxxStatus::Deleted),  // 默认排除软删除
            // ... 完整透传过滤字段
            pagination: params.pagination,
            ..Default::default()
        },
        ..Default::default()
    };

    let page = domain().xxx_manage().search_xxx(ctx, search).await?;
    Ok(page.map(|entity| XxxListItem { /* 字段映射 */ }))
}
```

**路由注册**（`router.rs`）：
```rust
// search 路由必须是 POST（与 query 一致），不是 GET
.route("/xxx/search", post(handlers::xxx::search_xxx_handler))
```

**经验**：
- search handler 必须透传**完整过滤字段**到 `XxxQuery`，不能只传 keyword
- 默认 `exclude_status = Some(Deleted)` 排除软删除
- 路由用 POST（body 传参），不是 GET query string

### 2.6 前端

**API 层**（`frontend/src/api/{domain}.rs`）：
```rust
pub async fn search_xxx(req: &SearchXxxRequest) -> Result<PagedResult<XxxListItem>, ApiError> {
    api_post("/api/v1/{domain}/xxx/search", req).await
}
```

**页面层 - 筛选区域 UI 模式**（`frontend/src/pages/{domain}/xxx.rs`）：

所有实体的列表页面统一采用「独立筛选卡片 + filter-row + filter-item」结构，与标题行、列表卡片分离：

```rust
// 1. 标题行（独立）：标题 + 创建按钮
div { class: "flex justify-between items-center mb-4",
    h2 { class: "text-xl font-bold", "Xxx 管理" }
    button { class: "btn btn-primary btn-sm", "+ 创建" }
}

// 2. 筛选卡片（独立）：filter-row 横向排列 filter-item
div { class: "card bg-base-100 shadow-md mb-4",
    div { class: "card-body",
        div { class: "flex flex-wrap gap-4 items-end",  // filter-row
            // 每个 filter-item：label + 控件垂直排列
            div { class: "flex flex-col gap-1 min-w-[140px] flex-1",  // filter-item
                label { class: "form-label", "状态" }
                select {
                    class: "select select-bordered w-full",
                    onchange: move |e| { /* set signal + load_data() */ },
                    option { value: "-1", "全部" }
                    // 选项...
                }
            }
            // 搜索框也是 filter-item
            div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                label { class: "form-label", "搜索" }
                input {
                    class: "input input-bordered w-full",
                    placeholder: "搜索...",
                    oninput: move |e| { /* 300ms 防抖 */ }
                }
            }
        }
    }
}

// 3. 列表卡片（独立）
div { class: "card bg-base-100 shadow-md", /* 表格 */ }
```

> **注意**：`filter-row` / `filter-item` CSS 类在样式表中未定义，统一使用 Tailwind 内联类替代：
> - `filter-row` → `flex flex-wrap gap-4 items-end`
> - `filter-item` → `flex flex-col gap-1 min-w-[140px] flex-1`

**页面层 - 搜索框 300ms 防抖模式**：

所有搜索框统一使用 300ms 防抖 + `search_request_id` 竞态防护（不再用 Enter 键触发）：

```rust
oninput: move |e| {
    search_keyword.set(e.value());
    let my_id = search_request_id() + 1;
    search_request_id.set(my_id);
    spawn(async move {
        gloo_timers::future::TimeoutFuture::new(300).await;
        if search_request_id() != my_id { return; }  // 丢弃过期请求
        load_data();
    });
}
```

> select 下拉的 `onchange` 直接触发 `load_data()`，无需防抖。

**页面层 - 三场景切换 load_data 函数**：

```rust
let load_data = move || {
    spawn(async move {
        let keyword = search_keyword();
        let has_filter = /* 检查各过滤 signal 是否有值 */;

        let my_id = search_request_id() + 1;
        search_request_id.set(my_id);

        let result = if keyword.trim().is_empty() && !has_filter {
            // 场景1：无关键词 + 无过滤 → list
            list_xxx(ListXxxRequest::default()).await.map(|p| p.items)
        } else if keyword.trim().is_empty() {
            // 场景2：有过滤 + 无关键词 → query
            query_xxx(&XxxQueryRequest {
                status: /* 过滤字段 */,
                ..Default::default()
            }).await.map(|p| p.items)
        } else {
            // 场景3：有关键词 → search（带完整过滤条件）
            search_xxx(&SearchXxxRequest {
                keyword: Some(keyword),
                status: /* 过滤字段 */,
                ..Default::default()
            }).await.map(|p| p.items)
        };

        if search_request_id() != my_id { return; }  // 丢弃过期请求
        match result {
            Ok(v) => xxx_list.set(v),
            Err(e) => toast.error(&e),
        }
        loading.set(false);
    });
};
```

**各实体前端过滤字段**：

| 实体 | 过滤字段 | 说明 |
|------|----------|------|
| Agent | status | 面试中/待入职/已入职/已离职/待离职（不展示 Deleted） |
| Tool | protocol, status | 协议（内置/HTTP/MCP）+ 状态（启用/禁用，不展示 Stale） |
| Skill | category, status | 分类（文本输入）+ 状态（已发布/草稿，不展示 Expired） |
| Project | status | 活跃/待审核/进行中/已完成/已归档 |
| Task | project_id, status, assignee_type | 项目下拉 + 状态 + 负责人类型 |

**经验**：
- 前端 `search_xxx` 函数签名应接受 `&SearchXxxRequest`（完整结构），不是只接受 `keyword: &str`
- 页面三场景切换必须齐全：**无关键词无过滤 → list；有过滤无关键词 → query；有关键词 → search**（query 不能省略）
- search 场景必须**同时携带完整过滤条件**（keyword + 各过滤字段），不是只传 keyword
- list/query/search 返回类型统一为 `PagedResult`，前端通过 `.map(|p| p.items)` 对齐
- 筛选区域采用独立卡片 + filter-row 模式，与标题行和列表卡片分离，UI 统一
- 搜索框统一 300ms 防抖 + `search_request_id` 竞态防护，select 下拉 onchange 直接触发
- 过滤字段用 `-1`（i32）或空字符串表示「全部」，转换时判断 `>= 0` 或 `!is_empty()`
- 过滤选项不展示异常状态（如 Tool 的 Stale、Skill 的 Expired、Agent 的 Deleted）
- 操作（启用/禁用/删除等）后调用 `load_data()` 刷新，保留当前搜索/过滤状态

---

## 三、改造检查清单（实体改造逐项确认模板）

> 本清单为每个实体重构时的确认模板（非单份落地 checklist）；落地现状以 [三位一体查询 wiki 长文](docs/wiki/zh/content/功能模块/查询与搜索/实体三位一体查询模式list-query-search.md) 与 [PagedResult/分页/RAG 知识卡](docs/wiki/knowledge/zh/查询与搜索体系/实体三位一体查询模式 list query search 规范.md) 为准。每个实体落地时按以下条目逐项核实。

### 后端
- DTO：新增 `SearchXxxRequest`（keyword + 完整过滤字段 + pagination），`SearchXxxResponse = PagedResult<XxxListItem>`
- DTO：移除 QueryRequest 中已废弃的 `keyword` 字段（或标记 deprecated）
- DAO 结构体：`XxxSearch.filters: XxxQuery`（复用 Query）
- DAO `search_xxx` SQL：复用 `push_query_filters`，支持 LIMIT + OFFSET，默认 limit=20
- DAO 向量搜索方法：限制 top 20
- DAL trait：`search` 返回 `PagedResult<T>`
- DAL 实现：`truncate(20)` + 内存态过滤复用方法（如有）+ 分页
- Domain trait：暴露 `search_xxx` 方法
- Handler：新增 `search_xxx.rs`，透传完整过滤字段，注册为 neural tool（如适用）
- Router：注册 `POST /xxx/search` 路由
- 测试：更新 search 相关测试断言为 PagedResult 访问（`.items.len()` / `.items[N]`）

### 前端
- API：新增/修改 `search_xxx(req: &SearchXxxRequest) -> Result<PagedResult<...>>`（POST）
- API：新增/修改 `query_xxx(req: &XxxQueryRequest) -> Result<PagedResult<...>>`（POST）
- 页面：三场景切换逻辑齐全（无关键词无过滤 → list；有过滤无关键词 → query；有关键词 → search）
- 页面：search 场景同时携带完整过滤条件（keyword + 各过滤字段）
- 页面：list/query/search 返回类型统一为 `PagedResult`，通过 `.map(|p| p.items)` 对齐
- 页面：筛选区域采用独立卡片 + filter-row + filter-item UI 模式
- 页面：搜索框 300ms 防抖 + `search_request_id` 竞态防护
- 页面：select 下拉 onchange 直接触发 load_data
- 页面：常用过滤字段已暴露（如 status/protocol/category 等，不展示异常状态）
- 页面：操作后调用 `load_data()` 刷新，保留当前搜索/过滤状态

---

## 四、各实体现状与改造范围

| 实体 | list | query | search handler | search 返回 PagedResult | search 复用 query 过滤 | 改造状态 |
|------|:-:|:-:|:-:|:-:|:-:|:-:|
| Agent | ✅ | ✅ | ✅ | ✅ | ✅ | 已完成 |
| Tool | ✅ | ✅ | ✅ | ✅ | ✅ | 已完成 |
| Skill | ✅ | ✅ | ✅ | ✅ | ✅ | 已完成 |
| Project | ✅ | ✅ | ✅ | ✅ | ✅ | 已完成 |
| Task | ✅ | ✅ | ✅ | ✅ | ✅ | 已完成 |

**已修复的关键缺陷**：
- ~~Project 的 `search_projects` SQL 未复用 `push_query_filters`~~ → 已修复，复用 push_query_filters + OFFSET + LIMIT 20
- ~~Tool 的 `ToolSearch` 是独立结构体（不复用 `ToolQuery`）~~ → 已修复，ToolSearch.filters 复用 ToolQuery
- ~~Skill 的 search_skills 是 GET 且返回 Vec~~ → 已修复，改为 POST + PagedResult + 完整过滤条件

---

## 五、设计原则总结

1. **三场景分离**：list（最简）、query（精确过滤）、search（语义搜索）各司其职，前端按场景切换
2. **search 复用 query 过滤**：通过 `Search.filters: Query` 结构复用，避免过滤条件在两处维护
3. **统一分页返回**：三接口都返回 `PagedResult<T>`，前端无需处理多种返回类型
4. **搜索结果限制**：`MAX_SEARCH_RESULTS=20`，搜不到应换关键词而非无限分页
5. **内存态过滤复用**：如 `runtime_state` 抽取为 `apply_runtime_state_filter`，query 和 search 共享
6. **SQL 过滤复用**：`push_query_filters` 函数被 query 和 search 共同复用，避免遗漏
7. **软删除默认排除**：search 和 query 默认 `exclude_status = Some(Deleted)`
8. **路由统一 POST**：search 和 query 都用 POST body 传参（不是 GET query string）
9. **前端 UI 统一**：筛选区域采用独立卡片 + filter-row + filter-item 模式，搜索框 300ms 防抖 + 竞态防护，三场景切换齐全（query 不可省略）
10. **前端过滤字段暴露**：常用过滤条件（如 status/protocol/category）应在 UI 暴露，异常状态（Stale/Expired/Deleted）不在选项中展示