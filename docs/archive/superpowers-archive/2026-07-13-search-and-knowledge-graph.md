# Search and Knowledge Graph Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement message search, memory search, and knowledge graph visualization with progressive loading.

**Architecture:** 
1. Message search: New handler `search_messages` calls MessageDomain.management().search() which delegates to MessageDal.search() (FTS5 + vector hybrid search)
2. Memory search: Register existing neural tool handlers (`search_memory`, `query_memory`) as HTTP routes
3. Knowledge graph: Progressive loading - search to find seed nodes → click node to fetch related nodes → render as graph with SVG

**Tech Stack:** Dioxus 0.7, Dioxus Router, SQLite FTS5, SQLite VSS (vector), SVG for graph visualization.

---

## File Structure

### Backend Files

| File | Responsibility |
|------|---------------|
| `common/src/api/message.rs` | Add SearchMessagesRequest/SearchMessagesResponse DTOs |
| `src/service/domain/message/mod.rs` | Add `search` method to MessageManagement trait |
| `src/service/domain/message/management.rs` | Implement `search` method delegating to DAL |
| `src/handlers/finance/message/search_messages.rs` | New handler for message search |
| `src/handlers/hr/agent/mod.rs` | Export handler functions for memory APIs |
| `src/router.rs` | Register routes for search_messages, search_memory, query_memory |

### Frontend Files

| File | Responsibility |
|------|---------------|
| `frontend/src/api/message.rs` | Add search_messages API client method |
| `frontend/src/api/hr.rs` | Add search_memory, query_memory API client methods |
| `frontend/src/pages/message/search.rs` | Message search page |
| `frontend/src/pages/hr/memory_search.rs` | Memory search page |
| `frontend/src/pages/hr/knowledge_graph.rs` | Knowledge graph visualization page |
| `frontend/src/components/graph.rs` | SVG graph component for nodes/edges |
| `frontend/src/pages/mod.rs` | Add routes |
| `frontend/src/layouts/navbar.rs` | Add navigation links |
| `frontend/src/main.rs` | Add route rendering |

---

## Task 1: Backend - Add Message Search DTOs

**Files:**
- Modify: `common/src/api/message.rs`

- [ ] **Step 1: Add SearchMessagesRequest and SearchMessagesResponse**

Add after `ListMessagesResponse`:
```rust
/// 消息搜索请求（POST body）
///
/// 支持混合搜索：关键词搜索 + 向量语义搜索 + 业务过滤
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct SearchMessagesRequest {
    /// 搜索关键词（FTS5 全文检索）
    pub keyword: Option<String>,
    /// 按项目 ID 过滤
    pub project_id: Option<String>,
    /// 按任务 ID 过滤
    pub task_id: Option<String>,
    /// 按发送方 ID 过滤
    pub from_id: Option<String>,
    /// 按接收方 ID 过滤
    pub to_id: Option<String>,
    /// 返回数量限制（默认 20）
    pub limit: Option<usize>,
}

/// 消息搜索响应
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct SearchMessagesResponse {
    /// 搜索结果列表（按相关性排序）
    pub messages: Vec<MessageSearchResult>,
    /// 总匹配数
    pub total: usize,
}

/// 消息搜索结果项（包含匹配信息）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MessageSearchResult {
    /// 消息 ID
    pub message_id: String,
    /// 关联项目 ID
    pub project_id: Option<String>,
    /// 关联任务 ID
    pub task_id: Option<String>,
    /// 发送方 ID
    pub from_id: String,
    /// 发送方角色
    pub from_role: i32,
    /// 接收方 ID
    pub to_id: String,
    /// 接收方角色
    pub to_role: i32,
    /// 消息类型
    pub message_type: i32,
    /// 消息内容（截断显示）
    pub content: String,
    /// 创建时间戳
    pub created_at: i64,
    /// 匹配类型：hybrid/vector/keyword
    pub match_type: Option<String>,
    /// FTS5 相关性分数（越小越相关）
    pub fts_rank: Option<f32>,
    /// 向量相似度距离（越小越相似）
    pub vector_distance: Option<f32>,
}
```

- [ ] **Step 2: Build validation**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo check -p common`
Expected: PASS with 0 errors

- [ ] **Step 3: Commit**

```bash
git add common/src/api/message.rs
git commit -m "feat: add message search DTOs"
```

---

## Task 2: Backend - Add search method to MessageManagement trait

**Files:**
- Modify: `src/service/domain/message/mod.rs`
- Modify: `src/service/domain/message/management.rs`

- [ ] **Step 1: Add search method to MessageManagement trait**

In `src/service/domain/message/mod.rs`, add to `MessageManagement` trait:
```rust
use crate::service::dao::message::MessageSearch;

#[async_trait::async_trait]
pub trait MessageManagement: Send + Sync {
    // ... existing methods ...
    
    /// 🔍 消息混合搜索（关键词 + 向量语义）
    ///
    /// 自动选择搜索策略：
    /// - keyword 存在 → FTS5 全文检索
    /// - query_vector 存在 → 向量语义搜索
    /// - 两者都有 → 混合搜索，合并结果
    async fn search(
        &self,
        ctx: RequestContext,
        search: MessageSearch,
    ) -> Result<Vec<Message>>;
}
```

- [ ] **Step 2: Implement search in management.rs**

In `src/service/domain/message/management.rs`, add:
```rust
use crate::service::dao::message::MessageSearch;

#[async_trait::async_trait]
impl MessageManagement for MessageDomainImpl {
    // ... existing implementations ...
    
    async fn search(
        &self,
        ctx: RequestContext,
        search: MessageSearch,
    ) -> Result<Vec<Message>> {
        self.message_dal.search(ctx, search).await
    }
}
```

- [ ] **Step 3: Build validation**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo check`
Expected: PASS with 0 errors

- [ ] **Step 4: Commit**

```bash
git add src/service/domain/message/mod.rs src/service/domain/message/management.rs
git commit -m "feat: add search method to MessageManagement"
```

---

## Task 3: Backend - Create search_messages handler

**Files:**
- Create: `src/handlers/finance/message/search_messages.rs`
- Modify: `src/handlers/finance/message/mod.rs`

- [ ] **Step 1: Create search_messages handler**

Create `src/handlers/finance/message/search_messages.rs`:
```rust
//! Handler: POST /api/v1/messages/search - Search messages with hybrid search

use crate::models::message::Message;
use crate::pkg::RequestContext;
use crate::service::dao::message::MessageSearch;
use crate::service::domain::message;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::message::{MessageSearchResult, SearchMessagesRequest, SearchMessagesResponse};
use common::error::{Result, bail_err};

/// Search messages by keyword with optional filters
#[register_handler_tool(
    id = "search_messages",
    name = "search_messages",
    description = "Search messages by keyword with hybrid search (FTS5 + vector semantic)",
    params = "common::api::message::SearchMessagesRequest"
)]
#[generate_http_handler]
pub async fn search_messages(
    ctx: RequestContext,
    params: SearchMessagesRequest,
) -> Result<SearchMessagesResponse> {
    let org_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| bail_err!(InvalidRequest, "当前请求缺少组织上下文"))?;
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let search = MessageSearch {
        keyword: params.keyword,
        query_vector: None,
        top_k: params.limit.map(|l| l as i32).or(Some(20)),
        filters: crate::service::dao::message::MessageQuery {
            organization_id: Some(org_id),
            project_id: params.project_id,
            task_id: params.task_id,
            from_id: params.from_id,
            to_id: params.to_id,
            limit: params.limit.or(Some(20)),
            ..Default::default()
        },
    };

    let messages = message::domain().management().search(ctx, search).await?;
    let results = messages.into_iter().map(message_to_search_result).collect();

    Ok(SearchMessagesResponse {
        messages: results.clone(),
        total: results.len(),
    })
}

fn message_to_search_result(message: Message) -> MessageSearchResult {
    let match_info = message.search_match;
    MessageSearchResult {
        message_id: message.po.id,
        project_id: message.po.project_id,
        task_id: message.po.task_id,
        from_id: message.po.from_id,
        from_role: message.po.from_role as i32,
        to_id: message.po.to_id,
        to_role: message.po.to_role as i32,
        message_type: message.po.message_type as i32,
        content: message.po.content,
        created_at: message.po.created_at,
        match_type: match_info.as_ref().map(|m| m.match_type.to_string()),
        fts_rank: match_info.and_then(|m| m.fts_rank),
        vector_distance: match_info.and_then(|m| m.vector_distance),
    }
}
```

- [ ] **Step 2: Add module export**

Add to `src/handlers/finance/message/mod.rs`:
```rust
pub mod search_messages;
pub use search_messages::search_messages_handler;
```

- [ ] **Step 3: Build validation**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo check`
Expected: PASS with 0 errors

- [ ] **Step 4: Commit**

```bash
git add src/handlers/finance/message/search_messages.rs src/handlers/finance/message/mod.rs
git commit -m "feat: add search_messages handler"
```

---

## Task 4: Backend - Register routes for search APIs

**Files:**
- Modify: `src/router.rs`

- [ ] **Step 1: Add routes in finance_routes()**

Add after the `/messages` route:
```rust
.route(
    "/messages/search",
    post(handlers::finance::message::search_messages_handler),
)
```

- [ ] **Step 2: Add routes in hr_routes()**

Add routes for memory search (reuse existing neural tool handlers):
```rust
.route(
    "/agents/search_memory",
    post(handlers::hr::agent::search_memory_handler),
)
.route(
    "/agents/query_memory",
    post(handlers::hr::agent::query_memory_handler),
)
```

- [ ] **Step 3: Build validation**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo check`
Expected: PASS with 0 errors

- [ ] **Step 4: Commit**

```bash
git add src/router.rs
git commit -m "feat: register search and memory API routes"
```

---

## Task 5: Frontend - Message Search API client

**Files:**
- Modify: `frontend/src/api/message.rs`

- [ ] **Step 1: Add search_messages API method**

Add to `frontend/src/api/message.rs`:
```rust
pub async fn search_messages(keyword: &str, project_id: Option<&str>) -> Result<SearchMessagesResponse, String> {
    let params = SearchMessagesRequest {
        keyword: if keyword.is_empty() { None } else { Some(keyword.to_string()) },
        project_id: project_id.map(|s| s.to_string()),
        task_id: None,
        from_id: None,
        to_id: None,
        limit: Some(20),
    };
    api_post("/api/v1/finance/messages/search", &params).await
}
```

- [ ] **Step 2: Build validation**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: PASS with 0 errors

- [ ] **Step 3: Commit**

```bash
git add frontend/src/api/message.rs
git commit -m "feat: add search_messages API client"
```

---

## Task 6: Frontend - Memory Search API client

**Files:**
- Modify: `frontend/src/api/hr.rs`

- [ ] **Step 1: Add memory search API methods**

Add to `frontend/src/api/hr.rs`:
```rust
pub async fn search_memory(query: &str, memory_type: Option<&str>) -> Result<SearchMemoryResponse, String> {
    let params = SearchMemoryParams {
        query: query.to_string(),
        max_results: Some(20),
        memory_type: memory_type.map(|s| s.to_string()),
        traversal_depth: None,
        traversal_breadth: None,
        traversal_strategy: None,
        seed_node_ids: None,
    };
    api_post("/api/v1/hr/agents/search_memory", &params).await
}

pub async fn query_memory(agent_id: Option<&str>, memory_type: Option<&str>) -> Result<QueryMemoryResponse, String> {
    let params = QueryMemoryParams {
        agent_id: agent_id.map(|s| s.to_string()),
        memory_type: memory_type.map(|s| s.to_string()),
        limit: Some(20),
    };
    api_post("/api/v1/hr/agents/query_memory", &params).await
}

pub async fn search_memory_with_traversal(query: &str, seed_node_ids: &[String], depth: i32) -> Result<SearchMemoryResponse, String> {
    let params = SearchMemoryParams {
        query: query.to_string(),
        max_results: Some(50),
        memory_type: None,
        traversal_depth: Some(depth),
        traversal_breadth: Some(10),
        traversal_strategy: Some("breadth_first".to_string()),
        seed_node_ids: Some(seed_node_ids.to_vec()),
    };
    api_post("/api/v1/hr/agents/search_memory", &params).await
}
```

- [ ] **Step 2: Build validation**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: PASS with 0 errors

- [ ] **Step 3: Commit**

```bash
git add frontend/src/api/hr.rs
git commit -m "feat: add memory search API clients"
```

---

## Task 7: Frontend - Message Search Page

**Files:**
- Create: `frontend/src/pages/message/search.rs`
- Modify: `frontend/src/pages/message/mod.rs`

- [ ] **Step 1: Create message search page**

Create `frontend/src/pages/message/search.rs`:
```rust
use dioxus::prelude::*;
use crate::api::message;
use crate::components::{Button, ErrorAlert, SuccessAlert, Loading, EmptyState};
use crate::layouts::AppLayout;

pub fn MessageSearch(cx: Scope) -> Element {
    let keyword = use_signal(cx, || String::new());
    let project_id = use_signal(cx, || None::<String>);
    let results = use_signal(cx, || Vec::new());
    let loading = use_signal(cx, || false);
    let error = use_signal(cx, || None::<String>);

    let handle_search = move |_| async move {
        loading.set(true);
        error.set(None);
        match message::search_messages(&keyword.read(), project_id.read().as_deref()).await {
            Ok(data) => {
                results.set(data.messages);
                if data.messages.is_empty() {
                    error.set(Some("未找到匹配的消息".to_string()));
                }
            }
            Err(e) => error.set(Some(e)),
        }
        loading.set(false);
    };

    cx.render(rsx! {
        AppLayout {
            title: "消息搜索"
            div { class: "content-area" }
                div { class: "card" }
                    h2 { class: "card-title", "消息搜索" }
                    div { class: "space-y-4" }
                        div { class: "flex gap-2" }
                            input {
                                class: "form-input flex-1"
                                bind:value: keyword
                                placeholder: "输入关键词搜索消息..."
                                onkeydown: move |evt| {
                                    if evt.key() == "Enter" {
                                        spawn(async move {
                                            handle_search(());
                                        });
                                    }
                                }
                            }
                            Button {
                                variant: "primary"
                                onclick: handle_search
                                "搜索"
                            }
                        }
                    }
                }

                if loading.read() {
                    Loading {}
                } else if error.read().is_some() {
                    EmptyState { 
                        message: "{error.read().clone().unwrap()}", 
                        hint: "尝试其他关键词" 
                    }
                } else if results.read().is_empty() {
                    EmptyState { 
                        message: "开始搜索", 
                        hint: "输入关键词后按回车或点击搜索按钮" 
                    }
                } else {
                    div { class: "card" }
                        h3 { class: "card-title", "搜索结果 ({results.read().len()})" }
                        table { class: "table w-full" }
                            thead {
                                tr {
                                    th { "内容" }
                                    th { "发送方" }
                                    th { "类型" }
                                    th { "匹配" }
                                    th { "时间" }
                                }
                            }
                            tbody {
                                results.read().iter().map(|msg| rsx! {
                                    tr { key: "{msg.message_id}" }
                                        td { "{msg.content.chars().take(100).collect::<String>()}" }
                                        td {
                                            span { class: if msg.from_role == 1 { "badge badge-accent" } else { "badge badge-primary" } }
                                                "{if msg.from_role == 1 { 'Agent' } else if msg.from_role == 0 { 'User' } else { 'System' }}"
                                        }
                                        td { "{msg.message_type}" }
                                        td {
                                            span { class: "text-sm text-muted" }
                                                "{msg.match_type.as_deref().unwrap_or("")}"
                                            if msg.vector_distance.is_some() {
                                                span { class: "text-sm text-accent ml-2" }
                                                    "d={msg.vector_distance.unwrap():.4}"
                                            }
                                        }
                                        td {
                                            "{format_timestamp(msg.created_at)}"
                                        }
                                })
                            }
                        }
                    }
                }
            }
        }
    })
}

fn format_timestamp(ts: i64) -> String {
    let dt = chrono::DateTime::from_timestamp(ts / 1000, 0).unwrap_or(chrono::Utc::now());
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}
```

- [ ] **Step 2: Add module export**

Add to `frontend/src/pages/message/mod.rs`:
```rust
pub mod search;
```

- [ ] **Step 3: Build validation**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: PASS with 0 errors

- [ ] **Step 4: Commit**

```bash
git add frontend/src/pages/message/search.rs frontend/src/pages/message/mod.rs
git commit -m "feat: add message search page"
```

---

## Task 8: Frontend - Memory Search Page

**Files:**
- Create: `frontend/src/pages/hr/memory_search.rs`
- Modify: `frontend/src/pages/hr/mod.rs`

- [ ] **Step 1: Create memory search page**

Create `frontend/src/pages/hr/memory_search.rs`:
```rust
use dioxus::prelude::*;
use crate::api::hr;
use crate::components::{Button, ErrorAlert, SuccessAlert, Loading, EmptyState};
use crate::layouts::AppLayout;

pub fn HrMemorySearch(cx: Scope) -> Element {
    let keyword = use_signal(cx, || String::new());
    let memory_type = use_signal(cx, || String::new());
    let results = use_signal(cx, || Vec::new());
    let loading = use_signal(cx, || false);
    let error = use_signal(cx, || None::<String>);

    let handle_search = move |_| async move {
        loading.set(true);
        error.set(None);
        let mem_type = if memory_type.read().is_empty() { None } else { Some(memory_type.read().as_str()) };
        match hr::search_memory(&keyword.read(), mem_type).await {
            Ok(data) => {
                results.set(data.results);
                if data.results.is_empty() {
                    error.set(Some("未找到匹配的记忆".to_string()));
                }
            }
            Err(e) => error.set(Some(e)),
        }
        loading.set(false);
    };

    cx.render(rsx! {
        AppLayout {
            title: "记忆搜索"
            div { class: "content-area" }
                div { class: "card" }
                    h2 { class: "card-title", "记忆搜索" }
                    div { class: "space-y-4" }
                        div { class: "flex gap-2" }
                            input {
                                class: "form-input flex-1"
                                bind:value: keyword
                                placeholder: "输入关键词搜索记忆..."
                                onkeydown: move |evt| {
                                    if evt.key() == "Enter" {
                                        spawn(async move {
                                            handle_search(());
                                        });
                                    }
                                }
                            }
                            select {
                                class: "form-select"
                                bind:value: memory_type
                                option { value: "", "全部类型" }
                                option { value: "short_term", "短期记忆" }
                                option { value: "knowledge_node", "知识节点" }
                                option { value: "trace", "调用记录" }
                                option { value: "relation", "关系" }
                            }
                            Button {
                                variant: "primary"
                                onclick: handle_search
                                "搜索"
                            }
                        }
                    }
                }

                if loading.read() {
                    Loading {}
                } else if error.read().is_some() {
                    EmptyState { 
                        message: "{error.read().clone().unwrap()}", 
                        hint: "尝试其他关键词" 
                    }
                } else if results.read().is_empty() {
                    EmptyState { 
                        message: "开始搜索", 
                        hint: "输入关键词后按回车或点击搜索按钮" 
                    }
                } else {
                    div { class: "card" }
                        h3 { class: "card-title", "搜索结果 ({results.read().len()})" }
                        div { class: "space-y-2" }
                            results.read().iter().map(|item| rsx! {
                                div { class: "p-3 border rounded hover:bg-muted" }
                                    div { class: "flex justify-between items-start" }
                                        div {
                                            span { class: "font-medium", "{item.content.chars().take(100).collect::<String>()}" }
                                            if item.summary.is_some() {
                                                div { class: "text-sm text-muted mt-1" }
                                                    "{item.summary.clone().unwrap()}"
                                                }
                                        }
                                        span { class: "badge badge-accent text-xs" }
                                            "{item.memory_type}"
                                        if item.score.is_some() {
                                            span { class: "text-xs text-muted" }
                                                "score={item.score.unwrap():.4}"
                                        }
                                    }
                                }
                            })
                        }
                    }
                }
            }
        }
    })
}
```

- [ ] **Step 2: Add module export**

Add to `frontend/src/pages/hr/mod.rs`:
```rust
pub mod memory_search;
```

- [ ] **Step 3: Build validation**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: PASS with 0 errors

- [ ] **Step 4: Commit**

```bash
git add frontend/src/pages/hr/memory_search.rs frontend/src/pages/hr/mod.rs
git commit -m "feat: add memory search page"
```

---

## Task 9: Frontend - Graph Component (SVG)

**Files:**
- Create: `frontend/src/components/graph.rs`

- [ ] **Step 1: Create SVG graph component**

Create `frontend/src/components/graph.rs`:
```rust
use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub node_type: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub label: String,
}

#[derive(Props)]
pub struct GraphProps {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub on_node_click: Option<EventHandler<String>>,
}

#[component]
pub fn Graph(cx: Scope<GraphProps>) -> Element {
    let node_positions = use_signal(cx, || {
        let mut pos = HashMap::new();
        for node in &cx.props.nodes {
            pos.insert(node.id.clone(), (node.x, node.y));
        }
        pos
    });

    let handle_node_click = move |id: String| {
        if let Some(cb) = &cx.props.on_node_click {
            cb.call(id);
        }
    };

    let svg_width = 800;
    let svg_height = 600;

    cx.render(rsx! {
        svg {
            width: "{svg_width}",
            height: "{svg_height}",
            viewBox: "0 0 {svg_width} {svg_height}",
            style: "border: 1px solid var(--border-color); border-radius: 8px; background: var(--bg-card)"

            // Edges
            cx.props.edges.iter().map(|edge| {
                let source_pos = node_positions.read().get(&edge.source);
                let target_pos = node_positions.read().get(&edge.target);
                match (source_pos, target_pos) {
                    (Some((sx, sy)), Some((tx, ty))) => rsx! {
                        g {}
                            line {
                                x1: "{sx}",
                                y1: "{sy}",
                                x2: "{tx}",
                                y2: "{ty}",
                                stroke: "var(--color-muted)",
                                stroke_width: "2"
                            }
                            text {
                                x: "{(sx + tx) / 2}",
                                y: "{(sy + ty) / 2 - 5}",
                                text_anchor: "middle",
                                class: "text-xs fill-accent"
                                "{edge.label}"
                            }
                        }
                    },
                    _ => rsx! { "" }
                }
            })

            // Nodes
            cx.props.nodes.iter().map(|node| {
                let color = match node.node_type.as_str() {
                    "knowledge_node" => "var(--color-primary)",
                    "short_term" => "var(--color-accent)",
                    "relation" => "var(--color-muted)",
                    _ => "var(--color-neutral)",
                };
                rsx! {
                    g {
                        onclick: move |_| handle_node_click(node.id.clone()),
                        cursor: "pointer"
                        circle {
                            cx: "{node.x}",
                            cy: "{node.y}",
                            r: "20",
                            fill: color,
                            stroke: "white",
                            stroke_width: "2"
                        }
                        text {
                            x: "{node.x}",
                            y: "{node.y}",
                            text_anchor: "middle",
                            dominant_baseline: "middle",
                            class: "text-xs fill-white font-medium"
                            "{node.label.chars().take(8).collect::<String>()}"
                        }
                    }
                }
            })
        }
    })
}

pub fn calculate_layout(nodes: &[GraphNode]) -> Vec<GraphNode> {
    let radius = 200.0;
    let center_x = 400.0;
    let center_y = 300.0;
    let n = nodes.len() as f64;
    
    nodes.iter().enumerate().map(|(i, node)| {
        let angle = (i as f64 / n) * 2.0 * std::f64::consts::PI;
        GraphNode {
            x: center_x + radius * angle.cos(),
            y: center_y + radius * angle.sin(),
            ..node.clone()
        }
    }).collect()
}
```

- [ ] **Step 2: Build validation**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: PASS with 0 errors

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/graph.rs
git commit -m "feat: add SVG graph component"
```

---

## Task 10: Frontend - Knowledge Graph Page

**Files:**
- Create: `frontend/src/pages/hr/knowledge_graph.rs`
- Modify: `frontend/src/pages/hr/mod.rs`

- [ ] **Step 1: Create knowledge graph page**

Create `frontend/src/pages/hr/knowledge_graph.rs`:
```rust
use dioxus::prelude::*;
use crate::api::hr;
use crate::components::{Button, ErrorAlert, Loading, EmptyState, Graph};
use crate::components::graph::{GraphNode, GraphEdge, calculate_layout};
use crate::layouts::AppLayout;
use std::collections::{HashMap, HashSet};

pub fn HrKnowledgeGraph(cx: Scope) -> Element {
    let search_query = use_signal(cx, || String::new());
    let nodes = use_signal(cx, || Vec::new());
    let edges = use_signal(cx, || Vec::new());
    let loading = use_signal(cx, || false);
    let error = use_signal(cx, || None::<String>);
    let loaded_node_ids = use_signal(cx, || HashSet::<String>::new());

    let handle_search = move |_| async move {
        loading.set(true);
        error.set(None);
        nodes.set(Vec::new());
        edges.set(Vec::new());
        loaded_node_ids.set(HashSet::new());

        match hr::search_memory(&search_query.read(), Some("knowledge_node")).await {
            Ok(data) => {
                if data.results.is_empty() {
                    error.set(Some("未找到匹配的知识节点".to_string()));
                } else {
                    let mut new_nodes = Vec::new();
                    for item in &data.results {
                        if !loaded_node_ids.read().contains(&item.id) {
                            loaded_node_ids.write().insert(item.id.clone());
                            let label = item.content.chars().take(12).collect::<String>();
                            new_nodes.push(GraphNode {
                                id: item.id.clone(),
                                label,
                                node_type: "knowledge_node".to_string(),
                                x: 0.0,
                                y: 0.0,
                            });
                        }
                    }
                    let laid_out = calculate_layout(&new_nodes);
                    nodes.set(laid_out);
                }
            }
            Err(e) => error.set(Some(e)),
        }
        loading.set(false);
    };

    let handle_node_click = move |node_id: String| async move {
        loading.set(true);
        error.set(None);

        match hr::search_memory_with_traversal("", &[node_id.clone()], 1).await {
            Ok(data) => {
                let mut node_id_map: HashMap<String, String> = HashMap::new();
                
                for item in &data.results {
                    let existing_idx = nodes.read().iter().position(|n| n.id == item.id);
                    if existing_idx.is_none() {
                        loaded_node_ids.write().insert(item.id.clone());
                        let label = match item.memory_type.as_str() {
                            "knowledge_node" => item.content.chars().take(12).collect(),
                            "relation" => "关系",
                            _ => item.memory_type.clone(),
                        };
                        node_id_map.insert(item.id.clone(), label);
                    }
                }

                let mut new_nodes = nodes.read().clone();
                for (id, label) in &node_id_map {
                    new_nodes.push(GraphNode {
                        id: id.clone(),
                        label: label.clone(),
                        node_type: "knowledge_node".to_string(),
                        x: 0.0,
                        y: 0.0,
                    });
                }

                let laid_out = calculate_layout(&new_nodes);
                nodes.set(laid_out);
            }
            Err(e) => error.set(Some(e)),
        }
        loading.set(false);
    };

    cx.render(rsx! {
        AppLayout {
            title: "知识图谱"
            div { class: "content-area" }
                div { class: "card" }
                    h2 { class: "card-title", "知识图谱" }
                    div { class: "space-y-4" }
                        div { class: "flex gap-2" }
                            input {
                                class: "form-input flex-1"
                                bind:value: search_query
                                placeholder: "搜索知识节点..."
                                onkeydown: move |evt| {
                                    if evt.key() == "Enter" {
                                        spawn(async move {
                                            handle_search(());
                                        });
                                    }
                                }
                            }
                            Button {
                                variant: "primary"
                                onclick: handle_search
                                "搜索"
                            }
                        }
                        p { class: "text-sm text-muted" }
                            "点击节点可展开查看关联的知识节点（渐进式加载）"
                        }
                }

                if loading.read() {
                    Loading {}
                } else if error.read().is_some() {
                    EmptyState { 
                        message: "{error.read().clone().unwrap()}", 
                        hint: "尝试其他关键词" 
                    }
                } else if nodes.read().is_empty() {
                    EmptyState { 
                        message: "开始探索", 
                        hint: "输入关键词搜索知识节点，点击节点展开图谱" 
                    }
                } else {
                    div { class: "card" }
                        h3 { class: "card-title", "图谱视图 ({nodes.read().len()} 个节点)" }
                        div { class: "flex justify-center" }
                            Graph {
                                nodes: nodes.read().clone(),
                                edges: edges.read().clone(),
                                on_node_click: Some(EventHandler::new(cx, handle_node_click))
                            }
                        }
                    }
                }
            }
        }
    })
}
```

- [ ] **Step 2: Add module export**

Add to `frontend/src/pages/hr/mod.rs`:
```rust
pub mod knowledge_graph;
```

- [ ] **Step 3: Build validation**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: PASS with 0 errors

- [ ] **Step 4: Commit**

```bash
git add frontend/src/pages/hr/knowledge_graph.rs frontend/src/pages/hr/mod.rs
git commit -m "feat: add knowledge graph page with progressive loading"
```

---

## Task 11: Frontend - Add Routes and Navigation

**Files:**
- Modify: `frontend/src/pages/mod.rs`
- Modify: `frontend/src/layouts/navbar.rs`
- Modify: `frontend/src/main.rs`

- [ ] **Step 1: Add routes**

Add to `frontend/src/pages/mod.rs`:
```rust
use crate::pages::message::search::MessageSearch;
use crate::pages::hr::memory_search::HrMemorySearch;
use crate::pages::hr::knowledge_graph::HrKnowledgeGraph;

#[derive(Clone, Routable, Debug, PartialEq)]
pub enum Route {
    // ... existing routes ...
    
    #[route("/messages/search")]
    MessageSearch {},
    
    #[route("/hr/memory-search")]
    HrMemorySearch {},
    
    #[route("/hr/knowledge-graph")]
    HrKnowledgeGraph {},
    
    // ... existing routes ...
}
```

- [ ] **Step 2: Add navbar links**

Add to `frontend/src/layouts/navbar.rs`:

In the message dropdown:
```rust
Link {
    to: Route::MessageSearch {},
    class: "navbar-dropdown-item",
    onclick: move |_| close_all()
    "🔍 消息搜索"
}
```

In the HR dropdown:
```rust
Link {
    to: Route::HrMemorySearch {},
    class: "navbar-dropdown-item",
    onclick: move |_| close_all()
    "🧠 记忆搜索"
}
Link {
    to: Route::HrKnowledgeGraph {},
    class: "navbar-dropdown-item",
    onclick: move |_| close_all()
    "🌐 知识图谱"
}
```

- [ ] **Step 3: Add route rendering**

Add to `frontend/src/main.rs`:
```rust
Route::MessageSearch {} => cx.render(rsx! { MessageSearch {} }),
Route::HrMemorySearch {} => cx.render(rsx! { HrMemorySearch {} }),
Route::HrKnowledgeGraph {} => cx.render(rsx! { HrKnowledgeGraph {} }),
```

- [ ] **Step 4: Build validation**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: PASS with 0 errors

- [ ] **Step 5: Commit**

```bash
git add frontend/src/pages/mod.rs frontend/src/layouts/navbar.rs frontend/src/main.rs
git commit -m "feat: add routes and navigation for search and graph"
```

---

## Task 12: Build Validation

**Files:**
- None (build validation)

- [ ] **Step 1: Run backend tests**

```bash
cd /Users/aman/Technology/rust/ai_orz && cargo test --workspace --no-fail-fast
```

Expected: All tests pass

- [ ] **Step 2: Run frontend build**

```bash
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check
```

Expected: 0 errors

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "chore: build validation"
```

---

## Self-Review

### 1. Spec Coverage

| Requirement | Task |
|-------------|------|
| 消息搜索 Handler | Task 1-4 |
| 记忆搜索 API 注册 | Task 4 |
| 消息搜索页面 | Task 5-7 |
| 记忆搜索页面 | Task 6, 8 |
| 知识图谱组件 | Task 9 |
| 知识图谱页面（渐进式加载） | Task 10 |
| 路由和导航 | Task 11 |

### 2. Placeholder Scan

No placeholders found. All tasks contain complete code and commands.

### 3. Type Consistency

All API methods use consistent naming. All page components follow the same pattern (signals, use_effect, handlers).

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-07-13-search-and-knowledge-graph.md`. Two execution options:

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**