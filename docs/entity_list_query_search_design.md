# 实体列表/查询/搜索接口设计规范

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

**页面层**（`frontend/src/pages/{domain}/xxx.rs`）：
```rust
// 三场景切换：无关键词 → list；有关键词 → search
let result = if keyword.trim().is_empty() {
    list_xxx(ListXxxRequest::default()).await.map(|p| p.items)
} else {
    search_xxx(&SearchXxxRequest {
        keyword: Some(keyword),
        ..Default::default()
    }).await.map(|p| p.items)
};
```

**经验**：
- 前端 `search_xxx` 函数签名应接受 `&SearchXxxRequest`（完整结构），不是只接受 `keyword: &str`
- 页面三场景切换：无关键词 → list；有过滤条件 → query；有关键词 → search
- list 和 search 返回类型统一为 `PagedResult`，前端通过 `.map(|p| p.items)` 对齐

---

## 三、改造检查清单

对每个实体改造时，按以下清单逐项确认：

### 后端
- [ ] DTO：新增 `SearchXxxRequest`（keyword + 完整过滤字段 + pagination），`SearchXxxResponse = PagedResult<XxxListItem>`
- [ ] DTO：移除 QueryRequest 中已废弃的 `keyword` 字段（或标记 deprecated）
- [ ] DAO 结构体：`XxxSearch.filters: XxxQuery`（复用 Query）
- [ ] DAO `search_xxx` SQL：复用 `push_query_filters`，支持 LIMIT + OFFSET，默认 limit=20
- [ ] DAO 向量搜索方法：限制 top 20
- [ ] DAL trait：`search` 返回 `PagedResult<T>`
- [ ] DAL 实现：`truncate(20)` + 内存态过滤复用方法（如有）+ 分页
- [ ] Domain trait：暴露 `search_xxx` 方法
- [ ] Handler：新增 `search_xxx.rs`，透传完整过滤字段，注册为 neural tool（如适用）
- [ ] Router：注册 `POST /xxx/search` 路由
- [ ] 测试：更新 search 相关测试断言为 PagedResult 访问（`.items.len()` / `.items[N]`）

### 前端
- [ ] API：新增/修改 `search_xxx(req: &SearchXxxRequest) -> Result<PagedResult<...>>`
- [ ] 页面：三场景切换逻辑（无关键词 → list；有关键词 → search）
- [ ] 页面：list 和 search 返回类型统一对齐

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
