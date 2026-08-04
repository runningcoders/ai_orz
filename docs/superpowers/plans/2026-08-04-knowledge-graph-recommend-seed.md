# 知识图谱推荐起点 + Agent 详情页复用 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为知识图谱页面增加"推荐起点"功能（按节点关联数 Top N），并拆出可复用的 `KnowledgeGraph` 子组件嵌入 Agent 详情页（自动定死 agent_id）。

**Architecture:**
- 后端新增 `recommend_seed_nodes` 接口：DAL 层组合 `query_knowledge_nodes` + `list_relations_batch`，应用层统计度数倒序取 Top N；通过 RuntimeMemory trait 暴露给 handler。
- 前端拆分 `knowledge_graph.rs`：路由入口 `HrKnowledgeGraph`（含 AppLayout + Agent 选择器）+ 可复用子组件 `KnowledgeGraph { agent_id: Option<String> }`。
- 推荐起点区域在 KnowledgeGraph 子组件内部，根据 agent_id 自动加载；点击推荐卡片直接触发 seed_node_ids 遍历（复用子组件内部 handle_node_click 逻辑）。
- Agent 详情页新增"知识图谱" tab，嵌入 `KnowledgeGraph { agent_id: Some(id.clone()) }`，自动定死 agent_id。

**Tech Stack:** Rust（axum + sqlx + async-trait），Dioxus 0.6（前端），SQLite（FTS5 + 关系查询）

---

## 文件结构

### 后端

| 文件 | 责任 | 状态 |
|------|------|------|
| `common/src/api/neural_tools.rs` | 新增 `RecommendSeedNodesParams` / `RecommendSeedNodesResponse` / `SeedNodeRecommendation` API DTO | 修改 |
| `src/models/memory.rs` | 新增 domain 层 `SeedNodeRecommendation` 结构（节点 + 度数） | 修改 |
| `src/service/dal/memory.rs` | `MemoryDal` trait 新增 `recommend_seed_nodes` 方法 + 实现 | 修改 |
| `src/service/domain/runtime/memory.rs` | `RuntimeMemory` trait 新增 `recommend_seed_nodes` 方法 + 委托实现 | 修改 |
| `src/service/domain/runtime/mod.rs` | `RuntimeMemory` trait 新增方法签名 | 修改 |
| `src/handlers/hr/agent/recommend_seed_nodes.rs` | 新建 handler，调用 runtime domain + 转换 DTO | 新建 |
| `src/handlers/hr/agent/mod.rs` | 注册新模块 + 导出 handler | 修改 |

### 前端

| 文件 | 责任 | 状态 |
|------|------|------|
| `frontend/src/api/hr.rs` | 新增 `recommend_seed_nodes` API 函数 | 修改 |
| `frontend/src/pages/hr/knowledge_graph.rs` | 拆分：`KnowledgeGraph` 子组件（接收 agent_id prop，内含推荐起点 + 搜索 + 图谱渲染） + `HrKnowledgeGraph` 路由入口（AppLayout + Agent 选择器 + 子组件） | 修改 |
| `frontend/src/pages/hr/agent_detail.rs` | 新增 "知识图谱" tab，嵌入 `KnowledgeGraph { agent_id: Some(id) }` | 修改 |

---

## Task 1: 后端 - 新增 API DTO

**Files:**
- Modify: `common/src/api/neural_tools.rs`（在 `DeleteMemoryResponse` 后追加）

- [ ] **Step 1: 追加 API DTO 定义**

在 `common/src/api/neural_tools.rs` 文件中，定位到 `DeleteMemoryResponse` 结构体之后（约第 151 行），追加以下三个 DTO：

```rust
/// 推荐知识图谱起点节点请求参数。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct RecommendSeedNodesParams {
    /// 指定 Agent ID。
    /// 不传则跨 Agent 全局推荐（仅考虑 published 节点）。
    pub agent_id: Option<String>,
    /// 返回推荐节点数量上限，默认 5。
    pub limit: Option<usize>,
}

/// 推荐起点响应。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RecommendSeedNodesResponse {
    /// 推荐节点列表（按关联度数倒序）。
    pub recommendations: Vec<SeedNodeRecommendation>,
}

/// 单个推荐起点节点。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq)]
pub struct SeedNodeRecommendation {
    /// 节点 ID。
    pub node_id: String,
    /// 节点名称。
    pub node_name: String,
    /// 节点描述。
    pub node_description: String,
    /// 节点类型（concept/fact/skill/pattern...）。
    pub node_type: String,
    /// 节点摘要。
    pub summary: String,
    /// 标签列表。
    pub tags: Vec<String>,
    /// 关联度数（入边 + 出边总数）。
    pub degree: usize,
    /// 入边数（被其他节点引用的次数）。
    pub incoming_count: usize,
    /// 出边数（引用其他节点的次数）。
    pub outgoing_count: usize,
}
```

注意：`SeedNodeRecommendation` 需要 `PartialEq` derive，因为 Dioxus 组件 Props 要求 PartialEq。

- [ ] **Step 2: 验证编译**

Run: `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH" cargo check -p common 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add common/src/api/neural_tools.rs
git commit -m "feat(api): add RecommendSeedNodes DTOs for knowledge graph seed recommendation"
```

---

## Task 2: 后端 - domain 层新增 SeedNodeRecommendation 结构

**Files:**
- Modify: `src/models/memory.rs`（在文件末尾追加）

- [ ] **Step 1: 追加 domain 层结构**

在 `src/models/memory.rs` 文件末尾追加：

```rust
/// 知识图谱推荐起点（domain 层结构）
///
/// 包含知识节点 PO + 关联度数统计信息，
/// 由 DAL 层 `recommend_seed_nodes` 方法返回。
#[derive(Debug, Clone)]
pub struct SeedNodeRecommendation {
    /// 知识节点 PO
    pub node: LongTermKnowledgeNodePo,
    /// 关联度数（入边 + 出边总数）
    pub degree: usize,
    /// 入边数（被其他节点引用次数）
    pub incoming_count: usize,
    /// 出边数（引用其他节点次数）
    pub outgoing_count: usize,
}
```

- [ ] **Step 2: 验证编译**

Run: `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH" cargo check 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/models/memory.rs
git commit -m "feat(domain): add SeedNodeRecommendation domain struct"
```

---

## Task 3: 后端 - DAL 层新增 recommend_seed_nodes 方法

**Files:**
- Modify: `src/service/dal/memory.rs`（trait 定义 + 实现）

- [ ] **Step 1: 在 MemoryDal trait 新增方法签名**

在 `src/service/dal/memory.rs` 的 `MemoryDal` trait 定义中，定位到 `async fn query(...)` 方法之后（约第 86 行），追加：

```rust
    /// 🎯 推荐知识图谱起点节点
    ///
    /// 按节点关联度数（入边 + 出边总数）倒序返回 Top N 节点。
    /// 用于知识图谱页面"推荐起点"功能，帮助用户快速定位核心节点。
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - agent_id: 指定 Agent ID；None 时跨 Agent 全局推荐（仅 published 节点）
    /// - limit: 返回数量上限，默认 5
    async fn recommend_seed_nodes(
        &self,
        ctx: RequestContext,
        agent_id: Option<String>,
        limit: usize,
    ) -> Result<Vec<crate::models::memory::SeedNodeRecommendation>>;
```

- [ ] **Step 2: 在 MemoryDalImpl 实现该方法**

在 `src/service/dal/memory.rs` 的 `impl MemoryDal for MemoryDalImpl` 块中，定位到 `async fn query(...)` 实现之后，追加：

```rust
    async fn recommend_seed_nodes(
        &self,
        ctx: RequestContext,
        agent_id: Option<String>,
        limit: usize,
    ) -> Result<Vec<crate::models::memory::SeedNodeRecommendation>> {
        use crate::models::memory::SeedNodeRecommendation;
        use crate::service::dao::memory::MemoryQuery;
        use common::enums::{MemoryStatus, MemoryType};

        // 1. 拉取知识节点（agent_id 为空时走全局 published 路径）
        let query = MemoryQuery {
            memory_type: Some(MemoryType::KnowledgeNode),
            agent_id: agent_id.clone(),
            status: Some(MemoryStatus::Active),
            exclude_status: Some(MemoryStatus::Forgotten),
            limit: Some(500), // 上限保护，避免节点过多拖慢统计
            include_shared: true, // 全局推荐时包含 published 节点
            ..Default::default()
        };
        let nodes = self
            .memory_dao
            .query_knowledge_nodes(ctx.clone(), query)
            .await?;

        if nodes.is_empty() {
            return Ok(Vec::new());
        }

        // 2. 批量查询这批节点的所有关系
        let node_ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
        let relations = self
            .memory_dao
            .list_relations_batch(ctx, &node_ids)
            .await?;

        // 3. 应用层统计每个节点的度数
        use std::collections::HashMap;
        let mut degree_map: HashMap<String, (usize, usize)> = HashMap::new();
        for rel in &relations {
            // 出边：rel.source_node_id 指向 rel.target_node_id
            degree_map
                .entry(rel.source_node_id.clone())
                .or_default()
                .1 += 1;
            // 入边：rel.target_node_id 被 rel.source_node_id 引用
            degree_map
                .entry(rel.target_node_id.clone())
                .or_default()
                .0 += 1;
        }

        // 4. 组装推荐列表并按度数倒序
        let mut recommendations: Vec<SeedNodeRecommendation> = nodes
            .into_iter()
            .map(|node| {
                let (incoming, outgoing) = degree_map.get(&node.id).copied().unwrap_or((0, 0));
                SeedNodeRecommendation {
                    degree: incoming + outgoing,
                    incoming_count: incoming,
                    outgoing_count: outgoing,
                    node,
                }
            })
            .collect();
        recommendations.sort_by(|a, b| b.degree.cmp(&a.degree));

        // 5. 截断到 limit
        recommendations.truncate(limit);
        Ok(recommendations)
    }
```

- [ ] **Step 3: 验证编译**

Run: `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH" cargo check 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/service/dal/memory.rs
git commit -m "feat(dal): add recommend_seed_nodes method to MemoryDal"
```

---

## Task 4: 后端 - RuntimeMemory trait 新增方法 + 委托实现

**Files:**
- Modify: `src/service/domain/runtime/mod.rs`（trait 定义）
- Modify: `src/service/domain/runtime/memory.rs`（实现）

- [ ] **Step 1: 在 RuntimeMemory trait 新增方法签名**

在 `src/service/domain/runtime/mod.rs` 的 `RuntimeMemory` trait 定义中，定位到 `async fn query(...)` 之后（约第 78 行），追加：

```rust
    /// 推荐知识图谱起点节点（按关联度数 Top N）
    async fn recommend_seed_nodes(
        &self,
        ctx: RequestContext,
        agent_id: Option<String>,
        limit: usize,
    ) -> Result<Vec<crate::models::memory::SeedNodeRecommendation>>;
```

- [ ] **Step 2: 在 runtime/memory.rs 实现中追加委托**

在 `src/service/domain/runtime/memory.rs` 的 `impl RuntimeMemory for RuntimeMemoryImpl` 块中，定位到 `async fn query(...)` 实现之后（约第 79 行），追加：

```rust
    async fn recommend_seed_nodes(
        &self,
        ctx: RequestContext,
        agent_id: Option<String>,
        limit: usize,
    ) -> Result<Vec<crate::models::memory::SeedNodeRecommendation>> {
        use crate::service::dal::memory::dal;
        dal().recommend_seed_nodes(ctx, agent_id, limit).await
    }
```

- [ ] **Step 3: 验证编译**

Run: `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH" cargo check 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/service/domain/runtime/mod.rs src/service/domain/runtime/memory.rs
git commit -m "feat(runtime): expose recommend_seed_nodes via RuntimeMemory trait"
```

---

## Task 5: 后端 - 新建 recommend_seed_nodes handler

**Files:**
- Create: `src/handlers/hr/agent/recommend_seed_nodes.rs`
- Modify: `src/handlers/hr/agent/mod.rs`（注册模块 + 导出）

**设计说明**：handler 放在 `src/handlers/hr/agent/` 下，与现有的 `query_memory`/`search_memory`/`save_long_term_memory` 等 memory handler 一致。虽然语义上推荐起点是知识图谱的能力，但保持与现有 memory handler 相同的目录结构，避免 memory handler 分散。

- [ ] **Step 1: 新建 handler 文件**

创建 `src/handlers/hr/agent/recommend_seed_nodes.rs`：

```rust
//! Handler: 推荐知识图谱起点节点
//!
//! 按节点关联度数（入边 + 出边总数）倒序返回 Top N 知识节点。
//! 用于知识图谱页面"推荐起点"功能，帮助用户快速定位核心节点。
//!
//! 语义上属于 memory domain（知识图谱能力），agent_id 只是过滤条件之一。
//! 文件位置与 query_memory/search_memory 等 memory handler 一致，放在 agent/ 下。

use crate::models::memory::SeedNodeRecommendation;
use crate::pkg::RequestContext;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::generate_http_handler;
use common::api::{
    RecommendSeedNodesParams, RecommendSeedNodesResponse,
    SeedNodeRecommendation as ApiSeedNodeRecommendation,
};
use common::error::Result;

/// 推荐知识图谱起点节点（按关联度数 Top N）
#[generate_http_handler]
pub async fn recommend_seed_nodes(
    ctx: RequestContext,
    params: RecommendSeedNodesParams,
) -> Result<RecommendSeedNodesResponse> {
    let limit = params.limit.unwrap_or(5).min(50);
    let recommendations = runtime_domain()
        .memory()
        .recommend_seed_nodes(ctx, params.agent_id, limit)
        .await?;

    let results = recommendations.into_iter().map(to_api).collect();
    Ok(RecommendSeedNodesResponse {
        recommendations: results,
    })
}

/// 解析 tags JSON 数组字符串为 Vec<String>，解析失败返回空 Vec
fn parse_tags_json(tags_json: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(tags_json).unwrap_or_default()
}

/// domain 层 SeedNodeRecommendation → API DTO
fn to_api(rec: SeedNodeRecommendation) -> ApiSeedNodeRecommendation {
    ApiSeedNodeRecommendation {
        node_id: rec.node.id.clone(),
        node_name: rec.node.node_name,
        node_description: rec.node.node_description,
        node_type: rec.node.node_type,
        summary: rec.node.summary,
        tags: parse_tags_json(&rec.node.tags),
        degree: rec.degree,
        incoming_count: rec.incoming_count,
        outgoing_count: rec.outgoing_count,
    }
}
```

注意：此 handler 不使用 `#[register_handler_tool]` 宏（非 neural tool，仅供前端 HTTP 调用），仅用 `#[generate_http_handler]` 注册 HTTP 路由。

- [ ] **Step 2: 在 mod.rs 注册模块和导出**

在 `src/handlers/hr/agent/mod.rs` 中：

1. 在 `pub mod query_memory;` 之后追加：

```rust
pub mod recommend_seed_nodes;
```

2. 在 `pub use query_memory::query_memory_handler;` 之后追加：

```rust
pub use recommend_seed_nodes::recommend_seed_nodes_handler;
```

- [ ] **Step 3: 验证编译**

Run: `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH" cargo check 2>&1 | tail -10`
Expected: PASS（路由由 `#[generate_http_handler]` 宏自动注册）

- [ ] **Step 4: 确认路由路径**

Run: `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH" cargo check 2>&1 | grep -i "recommend" | head -5`

确认宏生成的路由路径（预计为 `/api/v1/hr/agents/recommend_seed_nodes`，与现有 `query_memory` 路径 `/api/v1/hr/agents/query_memory` 格式一致）。前端 API 路径需要与此一致。

- [ ] **Step 5: Commit**

```bash
git add src/handlers/hr/agent/recommend_seed_nodes.rs src/handlers/hr/agent/mod.rs
git commit -m "feat(handler): add recommend_seed_nodes endpoint for knowledge graph"
```

---

## Task 6: 前端 - 新增 API 函数

**Files:**
- Modify: `frontend/src/api/hr.rs`

- [ ] **Step 1: 追加 API 函数**

在 `frontend/src/api/hr.rs` 中，定位到 `search_memory_with_traversal` 函数附近，追加：

```rust
/// 推荐知识图谱起点节点（按关联度数 Top N）
pub async fn recommend_seed_nodes(
    req: &RecommendSeedNodesParams,
) -> Result<RecommendSeedNodesResponse, ApiError> {
    api_post("/api/v1/hr/agents/recommend_seed_nodes", req).await
}
```

注意：URL 路径 `/api/v1/hr/agents/recommend_seed_nodes` 与现有 `query_memory` 路径 `/api/v1/hr/agents/query_memory` 格式一致（下划线分隔）。若宏生成路径不同，需调整此处 URL。

同时在文件顶部的 `use common::api::{...}` 导入中追加 `RecommendSeedNodesParams` 和 `RecommendSeedNodesResponse`。

- [ ] **Step 2: 验证编译**

Run: `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH" cargo check -p frontend 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add frontend/src/api/hr.rs
git commit -m "feat(frontend-api): add recommend_seed_nodes API function"
```

---

## Task 7: 前端 - 重构 knowledge_graph.rs 为通用 KnowledgeGraph 组件

**Files:**
- Modify: `frontend/src/pages/hr/knowledge_graph.rs`

这是本计划最核心的修改。当前 `HrKnowledgeGraph` 是单一组件，包含 AppLayout + 搜索 + 节点点击 + 图谱渲染。需要重构为：

1. **`KnowledgeGraph` 通用组件**：自包含的知识图谱组件，内置所有过滤器（关键词、Agent 选择器、标签等）+ 推荐起点 + 图谱渲染。接收 `agent_id: Option<String>` prop：
   - `agent_id = Some(id)`：锁定 Agent，不显示 Agent 选择器（Agent 详情页场景）
   - `agent_id = None`：显示 Agent 选择器，用户可动态切换（独立页面场景）

2. **`HrKnowledgeGraph` 路由入口**：仅 AppLayout + `<KnowledgeGraph agent_id={None} />`

**设计原则**：组件天然带关键词+各种筛选框过滤器，在特定展示场景复用时默认填好不可变的过滤字段。组件名去掉 hr 前缀，成为通用的 KnowledgeGraph 组件。

### Step 1: 更新 import 区

在 `frontend/src/pages/hr/knowledge_graph.rs` 文件顶部 import 区，替换为：

```rust
use dioxus::prelude::*;
use std::collections::HashSet;

use crate::api::hr::{query_agents, recommend_seed_nodes, search_memory_with_traversal};
use crate::components::button::Button;
use crate::components::graph::{Graph, GraphEdge, GraphNode, calculate_layout, expand_layout};
use crate::components::graph_canvas::KnowledgeGraphCanvas;
use crate::components::state::{EmptyState, Loading};
use crate::components::SearchableSelect;
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{
    AgentListItem, AgentQueryRequest, MemoryResult, RecommendSeedNodesParams,
    SearchMemoryParams, SeedNodeRecommendation,
};
```

保留原有的 `build_graph_from_results`、`type_label`、`type_badge_class`、`GraphStyle` 辅助函数不变。

- [ ] **Step 2: 新增 KnowledgeGraph 子组件**

在 `frontend/src/pages/hr/knowledge_graph.rs` 中，在 `HrKnowledgeGraph` 函数之前新增 `KnowledgeGraph` 子组件。该组件包含原 `HrKnowledgeGraph` 的搜索、节点点击、图谱渲染逻辑，并新增推荐起点区域。所有 `agent_id: None` 替换为 `agent_id: agent_id_prop.clone()`。

```rust
/// 可复用的知识图谱子组件
///
/// - agent_id = Some(id): 锁定到指定 Agent 的知识图谱（Agent 详情页场景）
/// - agent_id = None: 全局知识图谱（独立页面场景，包含所有 published 节点）
///
/// 内部自动根据 agent_id 加载推荐起点（按关联度数 Top N）。
/// 点击推荐卡片直接触发 seed_node_ids 遍历。
#[component]
pub fn KnowledgeGraph(agent_id: Option<String>) -> Element {
    let mut keyword = use_signal(String::new);
    let mut tags_input = use_signal(String::new);
    let mut nodes = use_signal(Vec::<GraphNode>::new);
    let mut edges = use_signal(Vec::<GraphEdge>::new);
    let mut loading = use_signal(|| false);
    let toast = use_toast();
    let mut expanded_nodes = use_signal(HashSet::<String>::new);
    let mut selected_node_id = use_signal(|| None::<String>);
    let mut selected_node_data = use_signal(|| None::<MemoryResult>);
    let mut search_history = use_signal(Vec::<String>::new);
    let mut highlighted_node_ids = use_signal(Vec::<String>::new);
    let mut detail_map = use_signal(std::collections::HashMap::<String, MemoryResult>::new);
    let mut click_request_id = use_signal(|| 0u32);
    let mut graph_style = use_signal(|| GraphStyle::Canvas);
    // 推荐起点
    let mut recommendations = use_signal(Vec::<SeedNodeRecommendation>::new);
    let mut rec_loading = use_signal(|| false);

    // 当前生效的 agent_id
    let agent_id_prop = agent_id.clone();

    // 加载推荐起点
    let load_recommendations = move || {
        let aid = agent_id_prop.clone();
        rec_loading.set(true);
        spawn(async move {
            let params = RecommendSeedNodesParams {
                agent_id: aid,
                limit: Some(5),
            };
            match recommend_seed_nodes(&params).await {
                Ok(resp) => recommendations.set(resp.recommendations),
                Err(e) => toast.error(format!("加载推荐起点失败: {}", e)),
            }
            rec_loading.set(false);
        });
    };

    // 首次渲染加载推荐起点
    use_effect(move || {
        load_recommendations();
    });

    let mut handle_search = move |_| {
        let kw = keyword().clone();
        if kw.is_empty() {
            return;
        }
        let tags_raw = tags_input().clone();
        loading.set(true);
        expanded_nodes.set(HashSet::new());
        selected_node_id.set(None);
        selected_node_data.set(None);

        let mut history = search_history.read().clone();
        if !history.contains(&kw) {
            history.insert(0, kw.clone());
            if history.len() > 10 {
                history.pop();
            }
            search_history.set(history);
        }

        let aid = agent_id_prop.clone();
        spawn(async move {
            let tags_vec: Vec<String> = if tags_raw.trim().is_empty() {
                Vec::new()
            } else {
                tags_raw
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            };
            let tags_field: Option<Vec<String>> = if tags_vec.is_empty() {
                None
            } else {
                Some(tags_vec)
            };
            let params = SearchMemoryParams {
                query: kw,
                max_results: Some(50),
                memory_type: None,
                traversal_depth: Some(1),
                traversal_breadth: Some(10),
                traversal_strategy: Some("breadth_first".to_string()),
                seed_node_ids: Some(Vec::new()),
                tags: tags_field,
                task_id: None,
                agent_id: aid,
            };
            match search_memory_with_traversal(params).await {
                Ok(data) => {
                    let mut map = std::collections::HashMap::new();
                    let mut highlights = Vec::new();
                    for item in &data.results {
                        if item.memory_type != "relation" {
                            map.insert(item.id.clone(), item.clone());
                            highlights.push(item.id.clone());
                        }
                    }
                    detail_map.set(map);
                    highlighted_node_ids.set(highlights);

                    let (new_nodes, new_edges) = build_graph_from_results(&data.results);
                    if new_nodes.is_empty() {
                        toast.error("未找到匹配的知识节点");
                        nodes.set(Vec::new());
                        edges.set(Vec::new());
                    } else {
                        let laid = calculate_layout(&new_nodes, None);
                        nodes.set(laid);
                        edges.set(new_edges);
                    }
                }
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    };

    // 点击节点展开关联（复用原 handle_node_click 逻辑）
    let handle_node_click = move |node_id: String| {
        selected_node_id.set(Some(node_id.clone()));

        if let Some(detail) = detail_map.read().get(&node_id) {
            selected_node_data.set(Some(detail.clone()));
        }

        if expanded_nodes.read().contains(&node_id) {
            return;
        }

        loading.set(true);
        let seed_ids = vec![node_id.clone()];
        let my_request_id = click_request_id() + 1;
        click_request_id.set(my_request_id);
        let aid = agent_id_prop.clone();
        spawn(async move {
            let params = SearchMemoryParams {
                query: "".to_string(),
                max_results: Some(50),
                memory_type: None,
                traversal_depth: Some(1),
                traversal_breadth: Some(10),
                traversal_strategy: Some("breadth_first".to_string()),
                seed_node_ids: Some(seed_ids.clone()),
                tags: None,
                task_id: None,
                agent_id: aid,
            };
            match search_memory_with_traversal(params).await {
                Ok(data) => {
                    if click_request_id() != my_request_id {
                        loading.set(false);
                        return;
                    }
                    let mut map = detail_map.read().clone();
                    for item in &data.results {
                        if item.memory_type != "relation" {
                            map.insert(item.id.clone(), item.clone());
                        }
                    }
                    if map.len() > 200 {
                        let valid_ids: HashSet<String> =
                            nodes.read().iter().map(|n| n.id.clone()).collect();
                        map.retain(|id, _| valid_ids.contains(id));
                    }
                    detail_map.set(map);

                    let existing_ids: HashSet<String> =
                        nodes.read().iter().map(|n| n.id.clone()).collect();
                    let (mut new_nodes, new_edges) = build_graph_from_results(&data.results);
                    new_nodes.retain(|n| !existing_ids.contains(&n.id));

                    if !new_nodes.is_empty() {
                        let current_nodes = nodes.read().clone();
                        let current_edges = edges.read().clone();
                        let updated_nodes =
                            expand_layout(&current_nodes, &new_nodes, &seed_ids[0]);
                        let mut updated_edges = current_edges;
                        let existing_edge_keys: HashSet<(String, String)> = updated_edges
                            .iter()
                            .map(|e| (e.source.clone(), e.target.clone()))
                            .collect();
                        for e in new_edges {
                            let key = (e.source.clone(), e.target.clone());
                            if !existing_edge_keys.contains(&key) {
                                updated_edges.push(e);
                            }
                        }
                        nodes.set(updated_nodes);
                        edges.set(updated_edges);
                    }

                    expanded_nodes.write().insert(seed_ids[0].clone());
                }
                Err(e) => {
                    toast.error(format!("加载节点关联失败: {}", e));
                }
            }
            loading.set(false);
        });
    };

    let current_nodes = nodes.read().clone();
    let current_edges = edges.read().clone();
    let selected_id = selected_node_id.read().clone();
    let selected_detail = selected_node_data.read().clone();
    let recs = recommendations.read().clone();

    rsx! {
        div { class: "space-y-4",
            // 推荐起点卡片区
            {if rec_loading() {
                Some(rsx! {
                    div { class: "card bg-base-100 shadow-md",
                        div { class: "card-body py-3",
                            span { class: "text-sm text-base-content/70", "正在计算推荐起点..." }
                        }
                    }
                })
            } else if !recs.is_empty() {
                Some(rsx! {
                    div { class: "card bg-base-100 shadow-md",
                        div { class: "card-body py-3",
                            h4 { class: "text-sm font-semibold mb-2", "🎯 推荐起点（按关联度数 Top {recs.len()}）" }
                            div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-2",
                                for rec in recs.into_iter() {
                                    {
                                        let nid = rec.node_id.clone();
                                        let name = rec.node_name.clone();
                                        let desc = rec.node_description.clone();
                                        let degree = rec.degree;
                                        let incoming = rec.incoming_count;
                                        let outgoing = rec.outgoing_count;
                                        let tags = rec.tags.clone();
                                        rsx! {
                                            button {
                                                class: "card bg-base-200 hover:bg-base-300 transition-colors text-left p-2 rounded-lg cursor-pointer",
                                                onclick: move |_| handle_node_click(nid.clone()),
                                                div { class: "flex flex-col gap-1",
                                                    span { class: "font-medium text-sm truncate", "{name}" }
                                                    span { class: "text-xs text-base-content/70 line-clamp-2", "{desc}" }
                                                    div { class: "flex flex-wrap gap-1 mt-1",
                                                        span { class: "badge badge-primary badge-sm", "度数 {degree}" }
                                                        span { class: "badge badge-ghost badge-sm", "入 {incoming}" }
                                                        span { class: "badge badge-ghost badge-sm", "出 {outgoing}" }
                                                        for tag in tags.iter().take(3) {
                                                            span { class: "badge badge-neutral badge-sm", "{tag}" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                })
            } else {
                None
            }}

            // 搜索框区
            div { class: "card bg-base-100 shadow-md",
                div { class: "card-body",
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
                            class: "input input-bordered sm:w-56",
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
                    if !search_history().is_empty() {
                        {
                            let history_list = search_history().clone();
                            rsx! {
                                div { class: "flex flex-wrap gap-2 items-center mt-2",
                                    span { class: "text-xs text-base-content/70", "搜索历史:" }
                                    for kw in history_list.into_iter() {
                                        button {
                                            class: "btn btn-xs btn-ghost",
                                            onclick: move |_| {
                                                keyword.set(kw.clone());
                                                handle_search(());
                                            },
                                            "{kw}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 图谱视图 + 节点详情
            if loading() {
                Loading {}
            } else if current_nodes.is_empty() {
                EmptyState { message: "开始搜索知识节点或点击推荐起点".to_string() }
            } else {
                div { class: "flex flex-col lg:flex-row gap-4",
                    div { class: "flex-1 min-h-96",
                        div { class: "card bg-base-100 shadow-md h-full",
                            div { class: "card-body",
                                {
                                    let canvas_btn_class = if graph_style() == GraphStyle::Canvas { "btn btn-xs join-item btn-primary" } else { "btn btn-xs join-item btn-ghost" };
                                    let svg_btn_class = if graph_style() == GraphStyle::Svg { "btn btn-xs join-item btn-primary" } else { "btn btn-xs join-item btn-ghost" };
                                    rsx! {
                                        div { class: "flex justify-between items-center mb-4",
                                            h3 { class: "card-title", "图谱视图 ({current_nodes.len()} 节点, {current_edges.len()} 关系)" }
                                            div { class: "join",
                                                button {
                                                    class: "{canvas_btn_class}",
                                                    onclick: move |_| graph_style.set(GraphStyle::Canvas),
                                                    "Canvas"
                                                }
                                                button {
                                                    class: "{svg_btn_class}",
                                                    onclick: move |_| graph_style.set(GraphStyle::Svg),
                                                    "SVG"
                                                }
                                            }
                                        }
                                    }
                                }
                                {match graph_style() {
                                    GraphStyle::Canvas => rsx! {
                                        KnowledgeGraphCanvas {
                                            nodes: current_nodes,
                                            edges: current_edges,
                                            selected_node_id: selected_id,
                                            highlighted_node_ids: Some(highlighted_node_ids()),
                                            on_node_click: handle_node_click,
                                        }
                                    },
                                    GraphStyle::Svg => rsx! {
                                        Graph {
                                            nodes: current_nodes,
                                            edges: current_edges,
                                            selected_node_id: selected_id,
                                            highlighted_node_ids: Some(highlighted_node_ids()),
                                            on_node_click: handle_node_click,
                                        }
                                    },
                                }}
                            }
                        }
                    }

                    if let Some(detail) = &selected_detail {
                        div { class: "w-full lg:w-96",
                            div { class: "card bg-base-100 shadow-md",
                                div { class: "card-body",
                                    div { class: "flex justify-between items-start mb-4",
                                        h3 { class: "card-title", "节点详情" }
                                        button {
                                            class: "btn btn-ghost btn-sm btn-circle",
                                            onclick: move |_| {
                                                selected_node_id.set(None);
                                                selected_node_data.set(None);
                                            },
                                            "✕"
                                        }
                                    }
                                    div { class: "space-y-4",
                                        div { class: "grid grid-cols-2 gap-4",
                                            div {
                                                label { class: "label",
                                                    span { class: "label-text font-medium", "类型" }
                                                }
                                                span { class: "{type_badge_class(&detail.memory_type)}", "{type_label(&detail.memory_type)}" }
                                            }
                                            div {
                                                label { class: "label",
                                                    span { class: "label-text font-medium", "匹配分数" }
                                                }
                                                if let Some(score) = detail.score {
                                                    span { class: "font-mono text-sm", "{score:.4}" }
                                                } else {
                                                    span { class: "text-base-content/70", "N/A" }
                                                }
                                            }
                                        }
                                        div {
                                            label { class: "label",
                                                span { class: "label-text font-medium", "内容" }
                                            }
                                            div { class: "p-3 bg-base-200 rounded-lg",
                                                p { class: "text-sm", "{detail.content}" }
                                            }
                                        }
                                        if let Some(summary) = &detail.summary {
                                            div {
                                                label { class: "label",
                                                    span { class: "label-text font-medium", "摘要" }
                                                }
                                                div { class: "p-3 bg-base-200 rounded-lg text-base-content/70",
                                                    p { class: "text-sm", "{summary}" }
                                                }
                                            }
                                        }
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
                                        div { class: "border-t border-base-300 pt-4",
                                            label { class: "label",
                                                span { class: "label-text font-medium", "ID" }
                                            }
                                            span { class: "font-mono text-xs text-base-content/70 break-all", "{detail.id}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 3: 重写 HrKnowledgeGraph 路由入口**

将原 `HrKnowledgeGraph` 函数替换为精简的路由入口（AppLayout + Agent 选择器 + 嵌入 KnowledgeGraph 子组件）：

```rust
#[component]
pub fn HrKnowledgeGraph() -> Element {
    // Agent 选择器状态（None = 全局知识图谱）
    let mut selected_agent_id = use_signal(|| None::<String>);
    let mut agent_search_results = use_signal(Vec::<AgentListItem>::new);
    let mut agent_search_loading = use_signal(|| false);
    let toast = use_toast();

    // 搜索 Agent（动态模式）
    let handle_agent_search = move |keyword: String| {
        if keyword.trim().is_empty() {
            agent_search_results.set(Vec::new());
            return;
        }
        agent_search_loading.set(true);
        spawn(async move {
            let req = AgentQueryRequest {
                keyword: Some(keyword),
                ..Default::default()
            };
            match query_agents(&req).await {
                Ok(resp) => agent_search_results.set(resp.items),
                Err(e) => toast.error(format!("搜索 Agent 失败: {}", e)),
            }
            agent_search_loading.set(false);
        });
    };

    // 选中 Agent
    let handle_agent_select = move |selection: String| {
        // 从 "name (id)" 格式中提取 id
        let agent_id = if let Some(id_start) = selection.rfind('(') {
            selection[id_start + 1..selection.len() - 1].to_string()
        } else {
            selection
        };
        selected_agent_id.set(Some(agent_id));
    };

    let agent_options = agent_search_results
        .read()
        .iter()
        .map(|a| format!("{} ({})", a.name, a.id))
        .collect::<Vec<String>>();

    rsx! {
        AppLayout {
            div { class: "space-y-4",
                // Agent 选择器
                div { class: "card bg-base-100 shadow-md",
                    div { class: "card-body py-3",
                        div { class: "flex gap-2 items-center",
                            span { class: "text-sm font-medium whitespace-nowrap", "Agent:" }
                            div { class: "flex-1 max-w-md",
                                SearchableSelect {
                                    placeholder: "选择 Agent（留空=全局知识图谱）...".to_string(),
                                    selected: None,
                                    options: agent_options,
                                    on_select: EventHandler::new(move |s: String| handle_agent_select(s)),
                                    on_search: Some(EventHandler::new(move |kw: String| handle_agent_search(kw))),
                                    loading: agent_search_loading(),
                                }
                            }
                            if selected_agent_id().is_some() {
                                button {
                                    class: "btn btn-ghost btn-sm",
                                    onclick: move |_| {
                                        selected_agent_id.set(None);
                                    },
                                    "✕ 清除"
                                }
                            }
                        }
                    }
                }

                // 嵌入可复用子组件（agent_id 变化时子组件自动重新加载推荐起点）
                KnowledgeGraph { agent_id: selected_agent_id() }
            }
        }
    }
}
```

- [ ] **Step 4: 验证编译**

Run: `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH" cargo check -p frontend 2>&1 | tail -20`
Expected: PASS（如有错误，根据错误信息调整）

- [ ] **Step 5: clippy 检查**

Run: `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH" cargo clippy -p frontend --target wasm32-unknown-unknown --all-targets -- -D warnings 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add frontend/src/pages/hr/knowledge_graph.rs
git commit -m "refactor(frontend): split KnowledgeGraph into reusable component with seed recommendations"
```

---

## Task 8: 前端 - Agent 详情页新增"知识图谱" tab

**Files:**
- Modify: `frontend/src/pages/hr/agent_detail.rs`

- [ ] **Step 1: 新增 import**

在 `frontend/src/pages/hr/agent_detail.rs` 顶部 import 区追加：

```rust
use crate::pages::hr::knowledge_graph::KnowledgeGraph;
```

- [ ] **Step 2: 新增 tab5 class 定义**

在 `active_tab` 的 tab class 定义区（约第 344 行 `tab4_class` 之后）追加：

```rust
            let tab5_class = if active_tab() == 5 { "tab tab-lg tab-active" } else { "tab tab-lg" };
```

- [ ] **Step 3: 新增 tab5 按钮**

在 tab 按钮区（约第 394-398 行 `tab4` 按钮之后）追加：

```rust
                            button {
                                class: "{tab5_class}",
                                onclick: move |_| active_tab.set(5),
                                "🧠 知识图谱"
                            }
```

- [ ] **Step 4: 新增 tab5 内容分支**

在 `match active_tab()` 的 `4 => rsx! {...}` 之后追加 `5 => rsx! {...}` 分支：

```rust
                            5 => rsx! {
                                KnowledgeGraph { agent_id: Some(id.clone()) }
                            },
```

- [ ] **Step 5: 验证编译**

Run: `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH" cargo check -p frontend 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 6: clippy 检查**

Run: `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH" cargo clippy -p frontend --target wasm32-unknown-unknown --all-targets -- -D warnings 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add frontend/src/pages/hr/agent_detail.rs
git commit -m "feat(frontend): add Knowledge Graph tab to agent detail page"
```

---

## Task 9: fmt + clippy 全量验证 + 推送

- [ ] **Step 1: cargo fmt**

Run: `cargo fmt --all`

- [ ] **Step 2: 后端 clippy**

Run: `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH" cargo clippy --all-targets -- -D warnings 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 3: 前端 clippy**

Run: `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH" cargo clippy -p frontend --target wasm32-unknown-unknown --all-targets -- -D warnings 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 4: fmt check**

Run: `cargo fmt --all -- --check`
Expected: PASS

- [ ] **Step 5: 推送**

```bash
git push
```

---

## Self-Review 检查清单

### Spec coverage
- ✅ 推荐起点功能（按节点度数 Top N）→ Task 1-5（后端）+ Task 7 Step 2（前端展示 + 点击交互）
- ✅ Agent 选择器（页面内下拉）→ Task 7 Step 3（HrKnowledgeGraph 路由入口）
- ✅ Agent 详情页复用知识图谱组件（自动定死 agent_id）→ Task 8
- ✅ 推荐策略：关联数最多的节点 → Task 3（DAL 层度数统计）
- ✅ 推荐起点在子组件内部，点击卡片直接触发 seed_node_ids 遍历 → Task 7 Step 2（handle_node_click 复用）

### Placeholder scan
- 无 "TBD"/"TODO"/"实现略" 占位符
- 所有代码块均为完整实现

### Type consistency
- `SeedNodeRecommendation`（domain 层 `src/models/memory.rs`）vs `SeedNodeRecommendation`（API 层 `common/src/api/neural_tools.rs`）：handler 中用 `as ApiSeedNodeRecommendation` 别名区分
- `RecommendSeedNodesParams.agent_id: Option<String>`：前后端一致
- `KnowledgeGraph` 组件 prop `agent_id: Option<String>`：HrKnowledgeGraph 和 agent_detail.rs 调用方式一致
- `SeedNodeRecommendation` API DTO 需要 `PartialEq` derive（Dioxus Props 要求）→ Task 1 已包含

### 潜在风险点
1. **后端路由路径**：`#[generate_http_handler]` 宏自动生成路由路径，Task 6 中前端 URL `/api/v1/hr/agents/recommend_seed_nodes` 需与宏生成路径一致。handler 在 `src/handlers/hr/agent/` 下，宏路径预计为 `/api/v1/hr/agents/recommend_seed_nodes`（与现有 `query_memory` 路径格式一致）。若不一致，编译通过但运行时 404。**应对**：Task 5 Step 4 已加确认步骤，若路径不符需调整 Task 6 URL。
2. **use_effect 触发推荐加载**：Task 7 Step 2 用 `use_effect` 在首次渲染时加载推荐。当 `agent_id` prop 变化（HrKnowledgeGraph 中切换 Agent），`use_effect` 会重新触发，自动重新加载推荐。Agent 详情页 agent_id 固定，只加载一次。
3. **SearchableSelect options 是 `Vec<String>`**：通过 `"name (id)"` 格式编码，选中后 `rfind('(')` 提取 id（参考 agent_detail.rs 已有用法）。
4. **handle_node_click 闭包借用**：推荐卡片 onclick 调用 `handle_node_click(nid.clone())`，需确保 `handle_node_click` 在推荐卡片渲染前已定义。Task 7 Step 2 代码中 `handle_node_click` 定义在推荐卡片 rsx 之前，顺序正确。
5. **load_recommendations 闭包**：`use_effect` 中调用 `load_recommendations()`，闭包捕获 `agent_id_prop`。当 prop 变化时，`use_effect` 依赖 `agent_id` 自动重新执行。

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-04-knowledge-graph-recommend-seed.md`. Two execution options:

**1. Subagent-Driven (recommended)** - 每个 Task 派发独立 subagent，任务间 review，快速迭代

**2. Inline Execution** - 在当前会话内顺序执行，带 checkpoint review

**Which approach?**
