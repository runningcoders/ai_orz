# 知识图谱 tags 展示与过滤 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为知识图谱（及短期记忆）补齐 tags 字段的后端查询/搜索过滤能力与前端展示/过滤 UI，对齐 Tool/Skill 已有的 tags 过滤范式。

**Architecture:** 后端在 DTO 层新增 tags 字段（请求 + 响应），DAO 层 `MemoryQuery` 新增 tags 字段并通过 SQLite `json_each` 实现 OR 语义过滤（参照 Tool/Skill），Handler 层透传 tags 并在响应中回填。前端在知识图谱节点详情面板展示 tags 徽章，并在搜索区新增 tags 过滤输入框。

**Tech Stack:** Rust (sqlx + SQLite FTS5 + json_each), Dioxus 0.7.9 + Tailwind CSS v4 + DaisyUI v5

---

## 文件结构

| 文件 | 职责 | 操作 |
|------|------|------|
| `common/src/api/neural_tools.rs` | 前后端共享 DTO | 修改：SearchMemoryParams / QueryMemoryParams / MemoryResult 加 tags |
| `src/service/dao/memory/mod.rs` | MemoryQuery 查询结构 | 修改：加 tags 字段 |
| `src/service/dao/memory/sqlite.rs` | DAO SQL 实现 | 修改：4 个查询/搜索方法加 tags 过滤 |
| `src/handlers/hr/agent/search_memory.rs` | search_memory handler | 修改：透传 tags + memory_to_result 回填 tags |
| `src/handlers/hr/agent/query_memory.rs` | query_memory handler | 修改：透传 tags + memory_to_result 回填 tags |
| `src/service/dao/memory/sqlite_test.rs` | DAO 层测试 | 修改：新增 tags 过滤测试 |
| `frontend/src/api/hr.rs` | 前端 API 客户端 | 修改：search_memory_with_traversal 加 tags 参数 |
| `frontend/src/pages/hr/knowledge_graph.rs` | 知识图谱页面 | 修改：节点详情展示 tags + 搜索区 tags 过滤输入框 |
| `frontend/src/pages/hr/memory_search.rs` | 短期记忆搜索页 | 修改：结果项展示 tags |
| `frontend/src/pages/hr/agent_memory_panel.rs` | Agent 记忆面板 | 修改：结果项展示 tags |

---

### Task 1: 后端 DTO 扩展

**Files:**
- Modify: `common/src/api/neural_tools.rs:9-24` (SearchMemoryParams)
- Modify: `common/src/api/neural_tools.rs:35-55` (MemoryResult)
- Modify: `common/src/api/neural_tools.rs:58-66` (QueryMemoryParams)

- [ ] **Step 1: 为 SearchMemoryParams 增加 tags 字段**

在 `common/src/api/neural_tools.rs` 的 `SearchMemoryParams` 结构体中，在 `seed_node_ids` 字段后增加：

```rust
    /// 标签过滤（OR 语义，命中任一 tag 即可）。
    pub tags: Option<Vec<String>>,
```

完整的 `SearchMemoryParams` 应为：

```rust
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct SearchMemoryParams {
    /// 搜索关键词。
    pub query: String,
    /// 返回最大结果数。
    pub max_results: Option<i32>,
    /// 记忆类型筛选。
    pub memory_type: Option<String>,
    /// 图谱遍历深度，默认0=不遍历。
    pub traversal_depth: Option<i32>,
    /// 每层展开广度，默认0=不限制。
    pub traversal_breadth: Option<i32>,
    /// 遍历策略：breadth_first / depth_first。
    pub traversal_strategy: Option<String>,
    /// 种子节点ID列表，跳过语义搜索直接遍历。
    pub seed_node_ids: Option<Vec<String>>,
    /// 标签过滤（OR 语义，命中任一 tag 即可）。
    pub tags: Option<Vec<String>>,
}
```

- [ ] **Step 2: 为 MemoryResult 增加 tags 字段**

在 `MemoryResult` 结构体中，在 `relation_type` 字段后增加：

```rust
    /// 标签列表（仅 short_term / knowledge_node 类型有值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
```

完整的 `MemoryResult` 应为：

```rust
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct MemoryResult {
    /// 记忆 ID。
    pub id: String,
    /// 记忆内容。
    pub content: String,
    /// 记忆类型。
    pub memory_type: String,
    /// 匹配分数。
    pub score: Option<f32>,
    /// 记忆摘要。
    pub summary: Option<String>,
    /// 关系类型：源节点 ID（仅 relation 类型有值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_node_id: Option<String>,
    /// 关系类型：目标节点 ID（仅 relation 类型有值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_node_id: Option<String>,
    /// 关系类型名称（仅 relation 类型有值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_type: Option<String>,
    /// 标签列表（仅 short_term / knowledge_node 类型有值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}
```

- [ ] **Step 3: 为 QueryMemoryParams 增加 tags 字段**

```rust
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct QueryMemoryParams {
    /// Agent ID 筛选。
    pub agent_id: Option<String>,
    /// 记忆类型筛选。
    pub memory_type: Option<String>,
    /// 返回数量限制。
    pub limit: Option<i32>,
    /// 标签过滤（OR 语义，命中任一 tag 即可）。
    pub tags: Option<Vec<String>>,
}
```

- [ ] **Step 4: 编译验证**

Run: `cargo check --workspace 2>&1 | grep -E "^error" | head -20`
Expected: 编译报错（因为 handler 中构造 MemoryResult 时缺少 tags 字段），这是预期的，Task 4 会修复。

- [ ] **Step 5: 暂不提交，继续 Task 2**

---

### Task 2: 后端 DAO 查询结构扩展

**Files:**
- Modify: `src/service/dao/memory/mod.rs:17-32` (MemoryQuery)

- [ ] **Step 1: 为 MemoryQuery 增加 tags 字段**

在 `src/service/dao/memory/mod.rs` 的 `MemoryQuery` 结构体中，在 `memory_type` 字段后增加：

```rust
    /// 按 tags 过滤（OR 语义，命中任一 tag 即可，JSON 数组列）
    pub tags: Option<Vec<String>>,
```

完整的 `MemoryQuery` 应为：

```rust
#[derive(Debug, Clone, Default)]
pub struct MemoryQuery {
    /// 按 ID 批量查询（向量搜索的核心过滤）
    pub ids: Option<Vec<String>>,
    /// 按 Agent ID 过滤
    pub agent_id: Option<String>,
    /// 按状态过滤
    pub status: Option<MemoryStatus>,
    /// 排除特定状态（软删除专用）
    pub exclude_status: Option<MemoryStatus>,
    /// 关键词搜索（用于传统 LIKE/MATCH 匹配）
    pub keyword: Option<String>,
    /// 最大返回条数
    pub limit: Option<usize>,
    /// ✅ 按记忆类型过滤
    pub memory_type: Option<MemoryType>,
    /// 按 tags 过滤（OR 语义，命中任一 tag 即可，JSON 数组列）
    pub tags: Option<Vec<String>>,
}
```

- [ ] **Step 2: 暂不提交，继续 Task 3**

---

### Task 3: 后端 DAO SQL 实现（4 个方法加 tags 过滤）

**Files:**
- Modify: `src/service/dao/memory/sqlite.rs:287-351` (query_short_term)
- Modify: `src/service/dao/memory/sqlite.rs:353-415` (search_short_term)
- Modify: `src/service/dao/memory/sqlite.rs:646-715` (query_knowledge_nodes)
- Modify: `src/service/dao/memory/sqlite.rs:717-779` (search_knowledge_nodes)

参照 Tool/Skill 的 `json_each` 过滤范式（OR 语义，命中任一 tag 即可）。

- [ ] **Step 1: query_short_term 加 tags 过滤**

在 `query_short_term` 方法中，在 `keyword` 的 deprecated warn 块之后、`ORDER BY` 之前，插入 tags 过滤分支：

```rust
        // tag 过滤（OR 语义：包含任一 tag 即可命中）
        if let Some(tags) = &query.tags {
            if !tags.is_empty() {
                builder.push(" AND EXISTS (SELECT 1 FROM json_each(tags) WHERE json_each.value IN (");
                let mut separated = builder.separated(", ");
                for tag in tags {
                    separated.push_bind(tag);
                }
                separated.push_unseparated("))");
            }
        }
```

注意：`query_short_term` 的 SQL 为 `FROM short_term_memory_index WHERE 1=1`（无表别名），所以 `json_each(tags)` 直接用列名，不需要加表别名前缀。

插入位置在 `if let Some(keyword) = &query.keyword { ... }` 块之后，`builder.push(" ORDER BY created_at DESC");` 之前。

- [ ] **Step 2: query_knowledge_nodes 加 tags 过滤**

在 `query_knowledge_nodes` 方法中，同样在 `keyword` 的 deprecated warn 块之后、`ORDER BY` 之前，插入 tags 过滤分支：

```rust
        // tag 过滤（OR 语义：包含任一 tag 即可命中）
        if let Some(tags) = &query.tags {
            if !tags.is_empty() {
                builder.push(" AND EXISTS (SELECT 1 FROM json_each(tags) WHERE json_each.value IN (");
                let mut separated = builder.separated(", ");
                for tag in tags {
                    separated.push_bind(tag);
                }
                separated.push_unseparated("))");
            }
        }
```

注意：`query_knowledge_nodes` 的 SQL 为 `FROM long_term_knowledge_node WHERE 1=1`（无表别名），所以 `json_each(tags)` 直接用列名。

插入位置在 `if let Some(keyword) = &query.keyword { ... }` 块之后，`builder.push(" ORDER BY updated_at DESC");` 之前。

- [ ] **Step 3: search_short_term 加 tags 过滤**

`search_short_term` 使用硬编码 SQL（非 QueryBuilder）。需要将 SQL 从静态字符串改为动态拼接，或者使用条件追加的方式。

由于 `search_short_term` 当前 SQL 是 `sqlx::query_as` 硬编码字符串，最简单的改法是在 SQL 的 WHERE 子句中通过条件判断追加 tags 过滤。

将 `search_short_term` 方法的 SQL 部分改为动态拼接（参照下方完整实现）：

```rust
    async fn search_short_term(
        &self,
        ctx: RequestContext,
        search: MemorySearch,
    ) -> Result<Vec<(ShortTermMemoryIndexPo, Option<f32>)>> {
        let pool = self.pool(ctx);

        // 从 MemorySearch 提取参数
        let agent_id = search.filters.agent_id.unwrap_or_default();
        let keyword = search.keyword.unwrap_or_default();
        let limit_i64 = search.filters.limit.unwrap_or(50) as i64;
        let tags = search.filters.tags.clone().unwrap_or_default();

        // 空关键词直接返回空结果（FTS5 MATCH 空字符串会报错）
        if keyword.trim().is_empty() {
            return Ok(Vec::new());
        }

        // 转义关键词为 FTS5 短语匹配
        let escaped_keyword = escape_fts5_keyword(&keyword);

        // 构建带可选 tags 过滤的 SQL
        let has_tags = !tags.is_empty();
        let tags_clause = if has_tags {
            let placeholders = tags.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            format!(" AND EXISTS (SELECT 1 FROM json_each(m.tags) WHERE json_each.value IN ({placeholders}))")
        } else {
            String::new()
        };

        let sql = format!(
            r#"
SELECT m.id, m.agent_id, m.task_id, m.role, m.summary, m.tags, m.trace_ids,
       m.status, m.created_at, m.updated_at,
       short_term_memory_fts.rank as fts_rank
FROM short_term_memory_fts
JOIN short_term_memory_index m ON short_term_memory_fts.rowid = m.rowid
WHERE short_term_memory_fts MATCH ?
  AND m.agent_id = ?
  AND m.status != 0{tags_clause}
ORDER BY short_term_memory_fts.rank
LIMIT ?
"#
        );

        let mut query = sqlx::query_as::<_, ShortTermSearchRow>(&sql)
            .bind(escaped_keyword)
            .bind(agent_id);

        // 绑定 tags 参数（如果有）
        if has_tags {
            for tag in &tags {
                query = query.bind(tag);
            }
        }

        let rows: Vec<ShortTermSearchRow> = query
            .bind(limit_i64)
            .fetch_all(&pool)
            .await?;

        let results = rows
            .into_iter()
            .map(|row| {
                let po = ShortTermMemoryIndexPo {
                    id: row.id,
                    agent_id: row.agent_id,
                    task_id: row.task_id,
                    role: row.role,
                    summary: row.summary,
                    tags: row.tags,
                    trace_ids: row.trace_ids,
                    status: row.status,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                };
                (po, row.fts_rank)
            })
            .collect();

        Ok(results)
    }
```

注意：`json_each(m.tags)` 使用表别名 `m`（因为 JOIN 后表别名为 `m`）。参数绑定顺序为：keyword → agent_id → tags... → limit。

- [ ] **Step 4: search_knowledge_nodes 加 tags 过滤**

将 `search_knowledge_nodes` 方法同样改为动态拼接 SQL：

```rust
    async fn search_knowledge_nodes(
        &self,
        ctx: RequestContext,
        search: MemorySearch,
    ) -> Result<Vec<(LongTermKnowledgeNodePo, Option<f32>)>> {
        let pool = self.pool(ctx);

        // 从 MemorySearch 提取参数
        let agent_id = search.filters.agent_id.unwrap_or_default();
        let keyword = search.keyword.unwrap_or_default();
        let limit_i64 = search.filters.limit.unwrap_or(50) as i64;
        let tags = search.filters.tags.clone().unwrap_or_default();

        // 空关键词直接返回空结果（FTS5 MATCH 空字符串会报错）
        if keyword.trim().is_empty() {
            return Ok(Vec::new());
        }

        // 转义关键词为 FTS5 短语匹配
        let escaped_keyword = escape_fts5_keyword(&keyword);

        // 构建带可选 tags 过滤的 SQL
        let has_tags = !tags.is_empty();
        let tags_clause = if has_tags {
            let placeholders = tags.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            format!(" AND EXISTS (SELECT 1 FROM json_each(m.tags) WHERE json_each.value IN ({placeholders}))")
        } else {
            String::new()
        };

        let sql = format!(
            r#"
SELECT m.id, m.agent_id, m.node_name, m.node_description, m.node_type, m.summary, m.tags,
       m.status, m.created_at, m.updated_at,
       knowledge_node_fts.rank as fts_rank
FROM knowledge_node_fts
JOIN long_term_knowledge_node m ON knowledge_node_fts.rowid = m.rowid
WHERE knowledge_node_fts MATCH ?
  AND m.agent_id = ?
  AND m.status != 0{tags_clause}
ORDER BY knowledge_node_fts.rank
LIMIT ?
"#
        );

        let mut query = sqlx::query_as::<_, KnowledgeNodeSearchRow>(&sql)
            .bind(escaped_keyword)
            .bind(agent_id);

        // 绑定 tags 参数（如果有）
        if has_tags {
            for tag in &tags {
                query = query.bind(tag);
            }
        }

        let rows: Vec<KnowledgeNodeSearchRow> = query
            .bind(limit_i64)
            .fetch_all(&pool)
            .await?;

        let results = rows
            .into_iter()
            .map(|row| {
                let po = LongTermKnowledgeNodePo {
                    id: row.id,
                    agent_id: row.agent_id,
                    node_name: row.node_name,
                    node_description: row.node_description,
                    node_type: row.node_type,
                    summary: row.summary,
                    tags: row.tags,
                    status: row.status,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                };
                (po, row.fts_rank)
            })
            .collect();

        Ok(results)
    }
```

- [ ] **Step 5: 编译验证**

Run: `cargo check --workspace 2>&1 | grep -E "^error" | head -20`
Expected: 编译报错（handler 中构造 MemoryResult 缺少 tags 字段），这是预期的，Task 4 会修复。

- [ ] **Step 6: 暂不提交，继续 Task 4**

---

### Task 4: 后端 Handler 透传 + 响应回填

**Files:**
- Modify: `src/handlers/hr/agent/search_memory.rs:24-208`
- Modify: `src/handlers/hr/agent/query_memory.rs:21-117`

- [ ] **Step 1: search_memory.rs — 透传 tags 到 MemorySearch.filters**

在 `src/handlers/hr/agent/search_memory.rs` 中，有 3 处构造 `MemorySearch` 的地方（line 74、line 111），需要把 `params.tags` 透传到 `filters.tags`。

第一处（line 71-80，`has_seeds && do_traversal` 分支不涉及搜索，跳过）。
第二处（line 108-117，`!has_seeds && do_traversal` 分支）：

```rust
        let search = MemorySearch {
            keyword: Some(params.query.clone()),
            top_k: params.max_results,
            filters: crate::service::dao::memory::MemoryQuery {
                memory_type: Some(MemoryType::KnowledgeNode),
                limit: params.max_results.map(|l| l as usize),
                tags: params.tags.clone(),
                ..Default::default()
            },
            ..Default::default()
        };
```

第三处（line 108-117 之后的 else 分支）：

```rust
        let search = MemorySearch {
            keyword: Some(params.query.clone()),
            top_k: params.max_results,
            filters: crate::service::dao::memory::MemoryQuery {
                memory_type: Some(memory_type),
                limit: params.max_results.map(|l| l as usize),
                tags: params.tags.clone(),
                ..Default::default()
            },
            ..Default::default()
        };
```

- [ ] **Step 2: search_memory.rs — memory_to_result 回填 tags**

修改 `memory_to_result` 函数（line 153-208），为 ShortTerm 和 KnowledgeNode 分支回填 tags：

```rust
fn memory_to_result(memory: &Memory) -> MemoryResult {
    match &memory.po {
        MemoryPo::Trace(trace) => MemoryResult {
            id: trace.id.clone(),
            content: trace.input.clone(),
            memory_type: "trace".to_string(),
            score: memory
                .search_match
                .as_ref()
                .and_then(|m| m.vector_distance),
            summary: None,
            source_node_id: None,
            target_node_id: None,
            relation_type: None,
            tags: None,
        },
        MemoryPo::ShortTerm(st) => MemoryResult {
            id: st.id.clone(),
            content: st.summary.clone(),
            memory_type: "short_term".to_string(),
            score: memory
                .search_match
                .as_ref()
                .and_then(|m| m.vector_distance),
            summary: Some(st.summary.clone()),
            source_node_id: None,
            target_node_id: None,
            relation_type: None,
            tags: Some(parse_tags_json(&st.tags)),
        },
        MemoryPo::KnowledgeNode(kn) => MemoryResult {
            id: kn.id.clone(),
            content: kn.node_description.clone(),
            memory_type: "knowledge_node".to_string(),
            score: memory
                .search_match
                .as_ref()
                .and_then(|m| m.vector_distance),
            summary: Some(kn.summary.clone()),
            source_node_id: None,
            target_node_id: None,
            relation_type: None,
            tags: Some(parse_tags_json(&kn.tags)),
        },
        MemoryPo::Relation(rel) => MemoryResult {
            id: rel.id.clone(),
            content: format!("{:?}", rel.relation_type),
            memory_type: "relation".to_string(),
            score: memory
                .search_match
                .as_ref()
                .and_then(|m| m.vector_distance),
            summary: None,
            source_node_id: Some(rel.source_node_id.clone()),
            target_node_id: Some(rel.target_node_id.clone()),
            relation_type: Some(format!("{:?}", rel.relation_type)),
            tags: None,
        },
    }
}
```

在文件末尾（`memory_to_result` 函数之后）添加 `parse_tags_json` 辅助函数：

```rust
/// 解析 tags JSON 数组字符串为 Vec<String>，解析失败返回空 Vec
fn parse_tags_json(tags_json: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(tags_json).unwrap_or_default()
}
```

- [ ] **Step 3: query_memory.rs — 透传 tags 到 MemoryQuery**

在 `src/handlers/hr/agent/query_memory.rs` 中，修改 `MemoryQuery` 构造（line 42-47）：

```rust
    let query = MemoryQuery {
        agent_id: params.agent_id.clone(),
        memory_type: Some(memory_type),
        limit: params.limit.map(|l| l as usize),
        tags: params.tags.clone(),
        ..Default::default()
    };
```

- [ ] **Step 4: query_memory.rs — memory_to_result 回填 tags**

修改 `query_memory.rs` 中的 `memory_to_result` 函数（line 62-117），与 search_memory.rs 的改动完全一致（为 ShortTerm 和 KnowledgeNode 分支回填 tags，其他分支 tags: None）。

在文件末尾添加同样的 `parse_tags_json` 辅助函数：

```rust
/// 解析 tags JSON 数组字符串为 Vec<String>，解析失败返回空 Vec
fn parse_tags_json(tags_json: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(tags_json).unwrap_or_default()
}
```

- [ ] **Step 5: 编译验证**

Run: `cargo check --workspace 2>&1 | grep -E "^error" | head -20`
Expected: 无编译错误（输出为空）。

- [ ] **Step 6: 运行已有测试**

Run: `cargo test --workspace 2>&1 | grep -E "^test result|FAILED|error\[|^error" | head -20`
Expected: 745 测试全部通过（无回归）。

- [ ] **Step 7: 提交**

```bash
git add common/src/api/neural_tools.rs src/service/dao/memory/mod.rs src/service/dao/memory/sqlite.rs src/handlers/hr/agent/search_memory.rs src/handlers/hr/agent/query_memory.rs
git commit -m "feat(memory): add tags field to search/query APIs and DAO filtering

- SearchMemoryParams / QueryMemoryParams / MemoryResult 新增 tags 字段
- MemoryQuery 新增 tags 字段
- query_short_term / query_knowledge_nodes / search_short_term / search_knowledge_nodes
  增加 tags 过滤（OR 语义，参照 Tool/Skill 的 json_each 范式）
- search_memory / query_memory handler 透传 tags 并在响应中回填"
```

---

### Task 5: 后端 DAO 层 tags 过滤测试

**Files:**
- Modify: `src/service/dao/memory/sqlite_test.rs`

- [ ] **Step 1: 确认测试文件中的已有测试模式**

Run: `grep -n "fn test_" src/service/dao/memory/sqlite_test.rs | head -10`
Expected: 列出已有的测试函数名，了解命名模式。

- [ ] **Step 2: 新增 query_knowledge_nodes tags 过滤测试**

在 `src/service/dao/memory/sqlite_test.rs` 中新增测试函数。使用 `#[sqlx::test]` 宏（独立内存数据库），先插入带不同 tags 的知识节点，再用 tags 过滤查询验证。

```rust
    #[sqlx::test]
    async fn test_query_knowledge_nodes_tags_filter(pool: sqlx::SqlitePool) {
        let dao = MemoryDaoImpl { pool: pool.clone() };
        let ctx = test_ctx();

        // 插入 3 个知识节点，带不同 tags
        let node1 = LongTermKnowledgeNodePo {
            id: "kn-tags-1".to_string(),
            agent_id: "test-agent".to_string(),
            node_name: "Rust 基础".to_string(),
            node_description: "Rust 所有权与借用".to_string(),
            node_type: "concept".to_string(),
            summary: "Rust 内存安全".to_string(),
            tags: r#"["rust","memory"]"#.to_string(),
            status: MemoryStatus::Active,
            created_at: 1000,
            updated_at: 1000,
        };
        let node2 = LongTermKnowledgeNodePo {
            id: "kn-tags-2".to_string(),
            agent_id: "test-agent".to_string(),
            node_name: "React Hooks".to_string(),
            node_description: "React 状态管理".to_string(),
            node_type: "concept".to_string(),
            summary: "前端状态".to_string(),
            tags: r#"["react","frontend"]"#.to_string(),
            status: MemoryStatus::Active,
            created_at: 2000,
            updated_at: 2000,
        };
        let node3 = LongTermKnowledgeNodePo {
            id: "kn-tags-3".to_string(),
            agent_id: "test-agent".to_string(),
            node_name: "WASM 互操作".to_string(),
            node_description: "Rust 与 JS 互操作".to_string(),
            node_type: "concept".to_string(),
            summary: "Rust 编译到 WASM".to_string(),
            tags: r#"["rust","frontend"]"#.to_string(),
            status: MemoryStatus::Active,
            created_at: 3000,
            updated_at: 3000,
        };

        dao.create_knowledge_node(ctx.clone(), node1).await.unwrap();
        dao.create_knowledge_node(ctx.clone(), node2).await.unwrap();
        dao.create_knowledge_node(ctx.clone(), node3).await.unwrap();

        // 按 "rust" tag 过滤 → 应返回 node1 和 node3
        let query_rust = MemoryQuery {
            agent_id: Some("test-agent".to_string()),
            tags: Some(vec!["rust".to_string()]),
            ..Default::default()
        };
        let results = dao.query_knowledge_nodes(ctx.clone(), query_rust).await.unwrap();
        assert_eq!(results.len(), 2, "按 rust tag 过滤应返回 2 个节点");
        let ids: Vec<&str> = results.iter().map(|n| n.id.as_str()).collect();
        assert!(ids.contains(&"kn-tags-1"));
        assert!(ids.contains(&"kn-tags-3"));

        // 按 "frontend" tag 过滤 → 应返回 node2 和 node3
        let query_frontend = MemoryQuery {
            agent_id: Some("test-agent".to_string()),
            tags: Some(vec!["frontend".to_string()]),
            ..Default::default()
        };
        let results = dao.query_knowledge_nodes(ctx.clone(), query_frontend).await.unwrap();
        assert_eq!(results.len(), 2, "按 frontend tag 过滤应返回 2 个节点");
        let ids: Vec<&str> = results.iter().map(|n| n.id.as_str()).collect();
        assert!(ids.contains(&"kn-tags-2"));
        assert!(ids.contains(&"kn-tags-3"));

        // 按 OR 语义过滤 "rust" + "react" → 应返回全部 3 个
        let query_multi = MemoryQuery {
            agent_id: Some("test-agent".to_string()),
            tags: Some(vec!["rust".to_string(), "react".to_string()]),
            ..Default::default()
        };
        let results = dao.query_knowledge_nodes(ctx.clone(), query_multi).await.unwrap();
        assert_eq!(results.len(), 3, "按 rust+react OR 语义过滤应返回 3 个节点");

        // 无 tags 过滤 → 应返回全部 3 个
        let query_none = MemoryQuery {
            agent_id: Some("test-agent".to_string()),
            ..Default::default()
        };
        let results = dao.query_knowledge_nodes(ctx, query_none).await.unwrap();
        assert_eq!(results.len(), 3, "无 tags 过滤应返回全部 3 个节点");
    }
```

注意：需要确认 `test_ctx()` 函数在测试文件中如何定义（可能在 `sqlite_test.rs` 或其引用的模块中）。如果不存在，参照同文件中其他测试的 ctx 构造方式。

- [ ] **Step 3: 运行新测试验证通过**

Run: `cargo test --workspace test_query_knowledge_nodes_tags_filter 2>&1 | tail -5`
Expected: `test result: ok. 1 passed`

- [ ] **Step 4: 运行全部测试确保无回归**

Run: `cargo test --workspace 2>&1 | grep -E "^test result|FAILED|error\[|^error" | head -20`
Expected: 746 测试全部通过（+1 新增）。

- [ ] **Step 5: 提交**

```bash
git add src/service/dao/memory/sqlite_test.rs
git commit -m "test(memory): add tags filter test for query_knowledge_nodes"
```

---

### Task 6: 前端知识图谱页面 tags 展示 + 过滤 UI

**Files:**
- Modify: `frontend/src/api/hr.rs:180-213`
- Modify: `frontend/src/pages/hr/knowledge_graph.rs:92-458`

- [ ] **Step 1: 修改 search_memory_with_traversal 增加 tags 参数**

在 `frontend/src/api/hr.rs` 中，修改 `search_memory_with_traversal` 函数签名，增加 `tags: Option<&[String]>` 参数：

```rust
pub async fn search_memory_with_traversal(
    query: &str,
    seed_node_ids: &[String],
    depth: i32,
    tags: Option<&[String]>,
) -> Result<SearchMemoryResponse, ApiError> {
    let params = SearchMemoryParams {
        query: query.to_string(),
        max_results: Some(50),
        memory_type: None,
        traversal_depth: Some(depth),
        traversal_breadth: Some(10),
        traversal_strategy: Some("breadth_first".to_string()),
        seed_node_ids: Some(seed_node_ids.to_vec()),
        tags: tags.map(|t| t.to_vec()),
    };
    api_post("/api/v1/hr/agents/search_memory", &params).await
}
```

同时修改 `search_memory` 和 `query_memory` 函数，增加 tags 参数：

```rust
pub async fn search_memory(
    query: &str,
    memory_type: Option<&str>,
    tags: Option<&[String]>,
) -> Result<SearchMemoryResponse, ApiError> {
    let params = SearchMemoryParams {
        query: query.to_string(),
        max_results: Some(20),
        memory_type: memory_type.map(|s| s.to_string()),
        traversal_depth: None,
        traversal_breadth: None,
        traversal_strategy: None,
        seed_node_ids: None,
        tags: tags.map(|t| t.to_vec()),
    };
    api_post("/api/v1/hr/agents/search_memory", &params).await
}

pub async fn query_memory(
    agent_id: Option<&str>,
    memory_type: Option<&str>,
    tags: Option<&[String]>,
) -> Result<QueryMemoryResponse, ApiError> {
    let params = QueryMemoryParams {
        agent_id: agent_id.map(|s| s.to_string()),
        memory_type: memory_type.map(|s| s.to_string()),
        limit: Some(20),
        tags: tags.map(|t| t.to_vec()),
    };
    api_post("/api/v1/hr/agents/query_memory", &params).await
}
```

- [ ] **Step 2: 更新 knowledge_graph.rs 中所有 search_memory_with_traversal 调用点**

在 `frontend/src/pages/hr/knowledge_graph.rs` 中，有两处调用 `search_memory_with_traversal`：

1. `handle_search`（line 107-155）中：
   - `search_memory_with_traversal(&kw, &[], 1)` → 改为 `search_memory_with_traversal(&kw, &[], 1, tags_opt())`

2. `handle_node_click`（line 157-225）中：
   - `search_memory_with_traversal("", &seed_ids, 1)` → 改为 `search_memory_with_traversal("", &seed_ids, 1, None)`（展开操作不做 tags 过滤）

其中 `tags_opt()` 需要根据 tags 输入框的状态获取。需要新增一个 Signal 来存储 tags 输入值。

在组件函数开头（约 line 93-100，已有 `let keyword = use_signal(String::new);` 等定义的地方），增加：

```rust
    let tags_input = use_signal(String::new);
```

然后定义一个辅助函数将 tags 输入字符串解析为 `Option<Vec<String>>`：

```rust
    let tags_opt = move || {
        let raw = tags_input.read().trim().to_string();
        if raw.is_empty() {
            None
        } else {
            let tags: Vec<String> = raw
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if tags.is_empty() { None } else { Some(tags) }
        }
    };
```

修改 `handle_search` 中的调用（第一处）：

```rust
        let result = search_memory_with_traversal(&kw, &[], 1, tags_opt().map(|t| t).as_deref()).await;
```

注意：由于 `tags_opt()` 返回 `Option<Vec<String>>`，需要转为 `Option<&[String]>`。如果类型转换复杂，可直接传 `tags_opt().as_deref()`。

修改 `handle_node_click` 中的调用（第二处）：

```rust
        let result = search_memory_with_traversal("", &seed_ids, 1, None).await;
```

- [ ] **Step 3: 在搜索区增加 tags 过滤输入框**

在 `knowledge_graph.rs` 的搜索 UI 区域（约 line 238-254，关键词输入框之后），增加 tags 输入框：

```rust
                        div { class: "flex flex-col sm:flex-row gap-2",
                            input {
                                class: "input input-bordered flex-1",
                                value: "{keyword}",
                                oninput: move |e| keyword.set(e.value()),
                                placeholder: "搜索知识节点...",
                                onkeydown: move |evt| {
                                    if evt.key() == Key::Enter {
                                        handle_search(());
                                    }
                                }
                            }
                            input {
                                class: "input input-bordered sm:w-48",
                                value: "{tags_input}",
                                oninput: move |e| tags_input.set(e.value()),
                                placeholder: "标签过滤（逗号分隔）...",
                                onkeydown: move |evt| {
                                    if evt.key() == Key::Enter {
                                        handle_search(());
                                    }
                                }
                            }
                            Button {
                                onclick: move |_| handle_search(()),
                                "搜索"
                            }
                        }
```

- [ ] **Step 4: 在节点详情面板展示 tags**

在 `knowledge_graph.rs` 的节点详情侧边栏（约 line 348-453），在"摘要"区块之后、"关系"区块之前，增加 tags 展示区块：

```rust
                                        if let Some(tags) = &detail.tags {
                                            if !tags.is_empty() {
                                                div {
                                                    label { class: "label",
                                                        span { class: "label-text font-medium", "标签" }
                                                    }
                                                    div { class: "flex flex-wrap gap-2",
                                                        for tag in tags.iter() {
                                                            span { class: "badge badge-neutral", "{tag}" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
```

插入位置：在 `if let Some(summary) = &detail.summary { ... }` 块之后，`if detail.memory_type == "relation" { ... }` 块之前。

- [ ] **Step 5: 查找并更新其他调用 search_memory / query_memory 的位置**

Run: `grep -rn "search_memory\b\|query_memory\b" frontend/src/ --include="*.rs" | grep -v "api/hr.rs" | grep -v "search_memory_with_traversal"`

对于找到的每个调用点，更新参数列表增加 `None`（不传 tags 过滤）。

可能的调用点：
- `frontend/src/pages/hr/memory_search.rs`（`search_memory` 调用）
- `frontend/src/pages/hr/agent_memory_panel.rs`（`query_memory` 或 `search_memory` 调用）

- [ ] **Step 6: 编译验证**

Run: `cd frontend && cargo check 2>&1 | grep -E "^error" | head -20`
Expected: 无编译错误。

- [ ] **Step 7: 提交**

```bash
git add frontend/src/api/hr.rs frontend/src/pages/hr/knowledge_graph.rs frontend/src/pages/hr/memory_search.rs frontend/src/pages/hr/agent_memory_panel.rs
git commit -m "feat(frontend): add tags display and filter UI to knowledge graph page

- API 客户端 search_memory/query_memory/search_memory_with_traversal 增加 tags 参数
- 知识图谱节点详情面板展示 tags 徽章
- 搜索区新增 tags 过滤输入框（逗号分隔，OR 语义）"
```

---

### Task 7: 前端短期记忆页面 tags 展示

**Files:**
- Modify: `frontend/src/pages/hr/memory_search.rs:83-103`
- Modify: `frontend/src/pages/hr/agent_memory_panel.rs` (结果展示区域)

- [ ] **Step 1: memory_search.rs 结果项展示 tags**

在 `frontend/src/pages/hr/memory_search.rs` 的搜索结果项（line 83-103），在类型徽章和 score 之后，增加 tags 展示：

```rust
                            for item in &results() {
                                div { class: "p-3 border border-base-300 rounded hover:bg-base-200",
                                    div { class: "flex flex-col sm:flex-row justify-between items-start gap-2",
                                        div { class: "flex-1",
                                            span { class: "font-medium", "{item.content.chars().take(100).collect::<String>()}" }
                                            if let Some(summary) = &item.summary {
                                                div { class: "text-sm text-base-content/70 mt-1",
                                                    "{summary}"
                                                }
                                            }
                                            if let Some(tags) = &item.tags {
                                                if !tags.is_empty() {
                                                    div { class: "flex flex-wrap gap-1 mt-2",
                                                        for tag in tags.iter() {
                                                            span { class: "badge badge-neutral badge-xs", "{tag}" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        div { class: "flex items-center gap-2 shrink-0",
                                            span { class: "badge badge-accent text-xs", "{item.memory_type}" }
                                            if let Some(score) = item.score {
                                                span { class: "text-xs text-base-content/70", "score={score:.4}" }
                                            }
                                        }
                                    }
                                }
                            }
```

- [ ] **Step 2: agent_memory_panel.rs 结果项展示 tags**

在 `frontend/src/pages/hr/agent_memory_panel.rs` 中找到记忆展示区域（参照 memory_search.rs 的模式），在内容/摘要之后增加同样的 tags 展示：

```rust
                                            if let Some(tags) = &item.tags {
                                                if !tags.is_empty() {
                                                    div { class: "flex flex-wrap gap-1 mt-2",
                                                        for tag in tags.iter() {
                                                            span { class: "badge badge-neutral badge-xs", "{tag}" }
                                                        }
                                                    }
                                                }
                                            }
```

具体插入位置需根据 `agent_memory_panel.rs` 的现有结构确定，通常在 summary 展示之后。

- [ ] **Step 3: 编译验证**

Run: `cd frontend && cargo build --release 2>&1 | grep -E "^error|warning" | head -20`
Expected: 无编译错误（可能有预存的 2 个 warning，与本次改动无关）。

- [ ] **Step 4: 提交**

```bash
git add frontend/src/pages/hr/memory_search.rs frontend/src/pages/hr/agent_memory_panel.rs
git commit -m "feat(frontend): display tags in memory search results and agent panel"
```

---

## Self-Review

**1. Spec coverage:**
- ✅ 后端查询/搜索接口支持 tags 字段过滤 → Task 1-4
- ✅ 后端响应返回 tags 字段 → Task 1 (MemoryResult) + Task 4 (memory_to_result 回填)
- ✅ 前端知识图谱展示 tags → Task 6 Step 4
- ✅ 前端知识图谱过滤功能 → Task 6 Step 3
- ✅ 短期记忆页面也展示 tags（一致性）→ Task 7
- ✅ 后端测试 → Task 5

**2. Placeholder scan:**
- 无 TBD / TODO / "implement later"
- 所有代码块均为完整实现
- Task 6 Step 5 提到"查找其他调用点"，这是因为无法确定所有调用点的确切位置，但给出了 grep 命令和可能的文件列表

**3. Type consistency:**
- `MemoryResult.tags: Option<Vec<String>>` — DTO 层定义与 handler 回填一致
- `MemoryQuery.tags: Option<Vec<String>>` — DAO 查询结构定义与 SQL 绑定一致
- `SearchMemoryParams.tags: Option<Vec<String>>` / `QueryMemoryParams.tags: Option<Vec<String>>` — 请求 DTO 一致
- 前端 `search_memory_with_traversal(query, seed_node_ids, depth, tags: Option<&[String]>)` — 参数类型一致
- `parse_tags_json(tags_json: &str) -> Vec<String>` — 在 search_memory.rs 和 query_memory.rs 中定义一致

**4. 已知限制:**
- `search_short_term` 和 `search_knowledge_nodes` 改为动态 SQL 拼接（`format!`），使用 `?` 占位符 + `bind`，不存在 SQL 注入风险
- tags 过滤为 OR 语义（命中任一 tag 即可），与 Tool/Skill 范式一致
- 向量搜索场景下的 tags 过滤：DAL 层 `search_knowledge_nodes_internal` 会先做向量搜索拿 ID，再用 `query_knowledge_nodes` 过滤补全，因此 tags 过滤在 query 层生效（向量命中的节点如果不满足 tags 条件会被过滤掉）
