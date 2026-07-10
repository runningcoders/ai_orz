# Phase 2: Neural Tools Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement neural tools system where agents can independently call built-in tools during reasoning, replacing auto-reply with agent-initiated messaging.

**Architecture:** Extend existing `register_handler_tool` macro with `neural` flag to mark tools as agent-native. Tools with `"neural"` tag are automatically injected during awakening. Agent sends messages via `send_message` neural tool instead of framework auto-reply. Memory handlers call `RuntimeDomain.memory()` (extended RuntimeMemory trait), not DAL directly — strict Handler → Domain → DAL layering.

**Tech Stack:** Rust, Axum, Rig, SQLx, AI Orz macros crate

---

## File Structure

| File | Responsibility | Change Type |
|------|---------------|-------------|
| `ai-orz-macros/src/lib.rs` | Extend `register_handler_tool` macro with `neural` flag and `tags` parameter | Modify |
| `common/src/api/neural_tools.rs` | Create DTOs for neural tool params/responses | Create |
| `src/service/domain/runtime/mod.rs` | Extend `RuntimeMemory` trait with search/query/create/update/delete | Modify |
| `src/service/domain/runtime/memory.rs` | Implement new `RuntimeMemory` methods (delegate to MemoryDal) | Modify |
| `src/service/domain/runtime/awakening.rs` | Filter tools by `"neural"` tag when injecting | Modify |
| `src/consumer/message.rs` | Remove auto-reply logic | Modify |
| `src/handlers/hr/agent/search_memory.rs` | search_memory handler → calls `runtime_domain.memory().search()` | Create |
| `src/handlers/hr/agent/query_memory.rs` | query_memory handler → calls `runtime_domain.memory().query()` | Create |
| `src/handlers/hr/agent/create_memory.rs` | create_memory handler → calls `runtime_domain.memory().create()` | Create |
| `src/handlers/hr/agent/update_memory.rs` | update_memory handler → calls `runtime_domain.memory().update()` | Create |
| `src/handlers/hr/agent/delete_memory.rs` | delete_memory handler → calls `runtime_domain.memory().delete()` | Create |
| `src/handlers/finance/message/send_message.rs` | send_message handler → calls `message_domain.delivery().send_to_user()` | Create |
| `src/handlers/finance/tool/request_tool_call.rs` | request_tool_call handler → calls `runtime_domain.tool_execution()` | Create |
| `src/handlers/project/task/mark_done.rs` | mark_done handler → calls `project_domain.task_manage().complete()` | Create |
| `src/handlers/finance/tool/list_tools.rs` | list_tools handler with neural tool registration | Modify |

---

## Task 1: Extend register_handler_tool macro with neural flag

**Files:**
- Modify: `ai-orz-macros/src/lib.rs:68-229`

- [ ] **Step 1: Write failing test for macro**

```rust
// ai-orz-macros/src/lib.rs (add test at end)
#[cfg(test)]
mod tests {
    use proc_macro::TokenStream;
    use syn::parse_macro_input;
    use super::register_handler_tool;

    #[test]
    fn test_neural_flag_parsing() {
        let args = TokenStream::from(
            r#"id = "test_tool", name = "test_tool", description = "Test", params = "TestParams", neural"#
                .parse()
                .unwrap(),
        );
        let input = TokenStream::from(
            r#"async fn test_handler(ctx: RequestContext, params: TestParams) -> Result<Value, AppError> { Ok(Value::Null) }"#
                .parse()
                .unwrap(),
        );
        let result = register_handler_tool(args, input);
        assert!(result.to_string().contains("neural"));
    }

    #[test]
    fn test_tags_parameter_parsing() {
        let args = TokenStream::from(
            r#"id = "test_tool", name = "test_tool", description = "Test", params = "TestParams", tags = "core,beta""#
                .parse()
                .unwrap(),
        );
        let input = TokenStream::from(
            r#"async fn test_handler(ctx: RequestContext, params: TestParams) -> Result<Value, AppError> { Ok(Value::Null) }"#
                .parse()
                .unwrap(),
        );
        let result = register_handler_tool(args, input);
        assert!(result.to_string().contains("core"));
        assert!(result.to_string().contains("beta"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ai-orz-macros && cargo test`
Expected: FAIL with "neural" not recognized / "tags" not recognized

- [ ] **Step 3: Add neural flag and tags parameter parsing**

Modify `ai-orz-macros/src/lib.rs` around line 74-98:

```rust
let mut id = None;
let mut name = None;
let mut description = None;
let mut params_type = None;
let mut neural = false;
let mut tags = Vec::new();

let parser = syn::meta::parser(|meta| {
    if meta.path.is_ident("id") {
        id = Some(meta.value()?.parse::<LitStr>()?.value());
        Ok(())
    } else if meta.path.is_ident("name") {
        name = Some(meta.value()?.parse::<LitStr>()?.value());
        Ok(())
    } else if meta.path.is_ident("description") {
        description = Some(meta.value()?.parse::<LitStr>()?.value());
        Ok(())
    } else if meta.path.is_ident("params") {
        let s = meta.value()?.parse::<LitStr>()?;
        let ty: Type = syn::parse_str(&s.value()).unwrap();
        params_type = Some(ty);
        Ok(())
    } else if meta.path.is_ident("neural") {
        neural = true;
        Ok(())
    } else if meta.path.is_ident("tags") {
        let s = meta.value()?.parse::<LitStr>()?.value();
        let extra_tags: Vec<String> = s.split(',').map(|s| s.trim().to_string()).collect();
        tags.extend(extra_tags);
        Ok(())
    } else {
        Err(meta.error("unexpected argument"))
    }
});
```

- [ ] **Step 4: Update create_po() to include tags**

Modify `ai-orz-macros/src/lib.rs` around line 185-203:

```rust
// After parsing
let neural_flag = neural;
let extra_tags = tags;

// In expanded code
impl BuiltinToolFactory for #factory_ident {
    fn create_po(&self) -> ToolPo {
        use common::enums::tool::{ControlMode, ToolProtocol, ToolStatus};
        let schema = schemars::schema_for!(#params_type);
        let schema_json = serde_json::to_value(&schema).unwrap();
        
        let mut tags_vec = Vec::new();
        if #neural_flag {
            tags_vec.push("neural".to_string());
        }
        tags_vec.extend(vec![#(#extra_tags.to_string()),*]);
        
        let mut po = ToolPo::new(
            #id.to_string(),
            #name.to_string(),
            #description.to_string(),
            ToolProtocol::Builtin,
            schema_json,
            None,
            tags_vec,
            None,
        );
        po.status = ToolStatus::Enabled;
        po.control_mode = ControlMode::Auto;
        po
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd ai-orz-macros && cargo test`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add ai-orz-macros/src/lib.rs
git commit -m "feat(macro): add neural flag and tags parameter to register_handler_tool"
```

---

## Task 2: Create DTOs for neural tools

**Files:**
- Create: `common/src/api/neural_tools.rs`

- [ ] **Step 1: Create the DTO file**

```rust
use ai_orz_macros::Params;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Params)]
pub struct SearchMemoryParams {
    #[param(source = "body")]
    pub query: String,
    
    #[param(source = "body")]
    pub max_results: Option<i32>,
    
    #[param(source = "body")]
    pub memory_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMemoryResponse {
    pub results: Vec<MemoryResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryResult {
    pub id: String,
    pub content: String,
    pub memory_type: String,
    pub score: Option<f32>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Params)]
pub struct QueryMemoryParams {
    #[param(source = "body")]
    pub agent_id: Option<String>,
    
    #[param(source = "body")]
    pub memory_type: Option<String>,
    
    #[param(source = "body")]
    pub limit: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryMemoryResponse {
    pub results: Vec<MemoryResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Params)]
pub struct CreateMemoryParams {
    #[param(source = "body")]
    pub memory_type: String,
    
    #[param(source = "body")]
    pub content: String,
    
    #[param(source = "body")]
    pub summary: Option<String>,
    
    #[param(source = "body")]
    pub tags: Option<Vec<String>>,
    
    #[param(source = "body")]
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMemoryResponse {
    pub memory_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Params)]
pub struct UpdateMemoryParams {
    #[param(source = "body")]
    pub memory_id: String,
    
    #[param(source = "body")]
    pub content: Option<String>,
    
    #[param(source = "body")]
    pub summary: Option<String>,
    
    #[param(source = "body")]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMemoryResponse {
    pub memory_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Params)]
pub struct DeleteMemoryParams {
    #[param(source = "body")]
    pub memory_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteMemoryResponse {
    pub memory_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Params)]
pub struct SendMessageParams {
    #[param(source = "body")]
    pub to_user_id: String,
    
    #[param(source = "body")]
    pub content: String,
    
    #[param(source = "body")]
    pub project_id: Option<String>,
    
    #[param(source = "body")]
    pub task_id: Option<String>,
    
    #[param(source = "body")]
    pub reply_to_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageResponse {
    pub message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Params)]
pub struct RequestToolCallParams {
    #[param(source = "body")]
    pub tool_id: String,
    
    #[param(source = "body")]
    pub params: serde_json::Value,
    
    #[param(source = "body")]
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestToolCallResponse {
    pub tool_call_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Params)]
pub struct MarkDoneParams {
    #[param(source = "body")]
    pub task_id: String,
    
    #[param(source = "body")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkDoneResponse {
    pub task_id: String,
    pub status: String,
}
```

- [ ] **Step 2: Update common/src/api/mod.rs to re-export**

Modify `common/src/api/mod.rs`:

```rust
pub mod neural_tools;
pub use neural_tools::*;
```

- [ ] **Step 3: Build common crate to verify**

Run: `cd common && cargo build`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add common/src/api/neural_tools.rs common/src/api/mod.rs
git commit -m "feat(common): add DTOs for neural tools"
```

---

## Task 3: Extend RuntimeMemory trait with full CRUD

**Files:**
- Modify: `src/service/domain/runtime/mod.rs` (trait definition)
- Modify: `src/service/domain/runtime/memory.rs` (implementation)

This task adds search/query/create/update/delete methods to the `RuntimeMemory` trait so that Handler layer can call `runtime_domain.memory().search()` etc. without touching DAL directly.

- [ ] **Step 1: Extend RuntimeMemory trait definition**

Modify `src/service/domain/runtime/mod.rs`, add 5 new methods to `RuntimeMemory` trait (after existing `get_recent_context` and `write_thinking_trace`):

```rust
#[async_trait]
pub trait RuntimeMemory: Send + Sync {
    // === 现有方法（内部使用，保持不变） ===
    async fn get_recent_context(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        task_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Memory>>;

    async fn write_thinking_trace(
        &self,
        ctx: RequestContext,
        trace: MemoryTrace,
    ) -> Result<Memory>;

    // === 新增方法（供 Handler/神经工具调用） ===

    /// 混合搜索记忆（关键词 + 向量语义）
    async fn search(
        &self,
        ctx: RequestContext,
        search: crate::service::dao::memory::MemorySearch,
    ) -> Result<Vec<Memory>>;

    /// 通用关系型查询
    async fn query(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::memory::MemoryQuery,
    ) -> Result<Vec<Memory>>;

    /// 创建记忆
    async fn create(
        &self,
        ctx: RequestContext,
        params: crate::models::memory::MemoryCreateParams,
    ) -> Result<Vec<Memory>>;

    /// 更新记忆
    async fn update(
        &self,
        ctx: RequestContext,
        memory: Memory,
    ) -> Result<Memory>;

    /// 删除记忆
    async fn delete(
        &self,
        ctx: RequestContext,
        memory: Memory,
    ) -> Result<()>;
}
```

- [ ] **Step 2: Implement new methods in memory.rs**

Modify `src/service/domain/runtime/memory.rs`, add implementations for the 5 new methods in the `impl RuntimeMemory for RuntimeDomainImpl` block:

```rust
async fn search(
    &self,
    ctx: RequestContext,
    search: crate::service::dao::memory::MemorySearch,
) -> Result<Vec<Memory>> {
    use crate::service::dal::memory::dal;
    dal().search(ctx, search).await
}

async fn query(
    &self,
    ctx: RequestContext,
    query: crate::service::dao::memory::MemoryQuery,
) -> Result<Vec<Memory>> {
    use crate::service::dal::memory::dal;
    dal().query(ctx, query).await
}

async fn create(
    &self,
    ctx: RequestContext,
    params: crate::models::memory::MemoryCreateParams,
) -> Result<Vec<Memory>> {
    use crate::service::dal::memory::dal;
    dal().create(ctx, params).await
}

async fn update(
    &self,
    ctx: RequestContext,
    memory: Memory,
) -> Result<Memory> {
    use crate::service::dal::memory::dal;
    dal().update(ctx, memory).await
}

async fn delete(
    &self,
    ctx: RequestContext,
    memory: Memory,
) -> Result<()> {
    use crate::service::dal::memory::dal;
    dal().delete(ctx, memory).await
}
```

- [ ] **Step 3: Run build to verify**

Run: `cargo build`
Expected: PASS

- [ ] **Step 4: Run existing tests to verify no regression**

Run: `cargo test --lib service::domain::runtime`
Expected: All existing tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/service/domain/runtime/mod.rs src/service/domain/runtime/memory.rs
git commit -m "feat(domain): extend RuntimeMemory trait with search/query/create/update/delete"
```

---

## Task 4: Create search_memory handler

**Files:**
- Create: `src/handlers/hr/agent/search_memory.rs`
- Modify: `src/handlers/hr/agent/mod.rs`

- [ ] **Step 1: Create handler with neural tool registration**

```rust
use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{SearchMemoryParams, SearchMemoryResponse, MemoryResult};
use common::error::Result;
use common::enums::MemoryType;
use crate::pkg::request_context::RequestContext;
use crate::service::dao::memory::{MemoryQuery, MemorySearch};
use crate::service::domain::runtime::domain as runtime_domain;

#[register_handler_tool(
    id = "search_memory",
    name = "search_memory",
    description = "Search memory for relevant information using keyword + vector semantic search",
    params = "common::api::SearchMemoryParams",
    neural,
)]
#[generate_http_handler]
pub async fn search_memory(
    ctx: RequestContext,
    params: SearchMemoryParams,
) -> Result<SearchMemoryResponse> {
    let memory_type = params.memory_type.as_deref()
        .and_then(|t| t.parse::<MemoryType>().ok())
        .unwrap_or(MemoryType::All);
    
    let search = MemorySearch {
        keyword: Some(params.query),
        top_k: Some(params.max_results.unwrap_or(10)),
        filters: MemoryQuery {
            agent_id: ctx.agent_id().map(|id| id.to_string()),
            memory_type: Some(memory_type),
            limit: Some(params.max_results.unwrap_or(10) as usize),
            ..Default::default()
        },
        ..Default::default()
    };
    
    let results = runtime_domain().memory().search(ctx, search).await?;
    
    let response_results = results.into_iter().map(|mem| {
        let (id, content, memory_type, summary, score) = match mem.po {
            crate::models::memory::MemoryPo::ShortTerm(st) => (
                st.id,
                st.summary,
                "short_term".to_string(),
                Some(st.summary),
                mem.search_match.and_then(|m| m.vector_distance.map(|d| 1.0 - d)),
            ),
            crate::models::memory::MemoryPo::KnowledgeNode(kn) => (
                kn.id,
                kn.node_description,
                "knowledge_node".to_string(),
                Some(kn.summary),
                mem.search_match.and_then(|m| m.vector_distance.map(|d| 1.0 - d)),
            ),
            _ => (
                "unknown".to_string(),
                "".to_string(),
                "unknown".to_string(),
                None,
                None,
            ),
        };
        
        MemoryResult {
            id,
            content,
            memory_type,
            score,
            summary,
        }
    }).collect();
    
    Ok(SearchMemoryResponse {
        results: response_results,
    })
}
```

- [ ] **Step 2: Register handler in mod.rs**

Modify `src/handlers/hr/agent/mod.rs`:

```rust
pub mod search_memory;
pub use search_memory::*;
```

- [ ] **Step 3: Run build to verify**

Run: `cargo build`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/handlers/hr/agent/search_memory.rs src/handlers/hr/agent/mod.rs
git commit -m "feat(handler): add search_memory neural tool"
```

---

## Task 5: Create query_memory handler

**Files:**
- Create: `src/handlers/hr/agent/query_memory.rs`
- Modify: `src/handlers/hr/agent/mod.rs`

- [ ] **Step 1: Create handler with neural tool registration**

```rust
use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{QueryMemoryParams, QueryMemoryResponse, MemoryResult};
use common::error::Result;
use common::enums::MemoryType;
use crate::pkg::request_context::RequestContext;
use crate::service::dao::memory::MemoryQuery;
use crate::service::domain::runtime::domain as runtime_domain;

#[register_handler_tool(
    id = "query_memory",
    name = "query_memory",
    description = "Query memory with filters (agent_id, memory_type, limit)",
    params = "common::api::QueryMemoryParams",
    neural,
)]
#[generate_http_handler]
pub async fn query_memory(
    ctx: RequestContext,
    params: QueryMemoryParams,
) -> Result<QueryMemoryResponse> {
    let memory_type = params.memory_type.as_deref()
        .and_then(|t| t.parse::<MemoryType>().ok())
        .unwrap_or(MemoryType::All);
    
    let query = MemoryQuery {
        agent_id: params.agent_id.or_else(|| ctx.agent_id().map(|id| id.to_string())),
        memory_type: Some(memory_type),
        limit: Some(params.limit.unwrap_or(20) as usize),
        ..Default::default()
    };
    
    let results = runtime_domain().memory().query(ctx, query).await?;
    
    let response_results = results.into_iter().map(|mem| {
        let (id, content, memory_type, summary) = match mem.po {
            crate::models::memory::MemoryPo::ShortTerm(st) => (
                st.id,
                st.summary,
                "short_term".to_string(),
                Some(st.summary),
            ),
            crate::models::memory::MemoryPo::KnowledgeNode(kn) => (
                kn.id,
                kn.node_description,
                "knowledge_node".to_string(),
                Some(kn.summary),
            ),
            _ => (
                "unknown".to_string(),
                "".to_string(),
                "unknown".to_string(),
                None,
            ),
        };
        
        MemoryResult {
            id,
            content,
            memory_type,
            score: None,
            summary,
        }
    }).collect();
    
    Ok(QueryMemoryResponse {
        results: response_results,
    })
}
```

- [ ] **Step 2: Register handler in mod.rs**

Modify `src/handlers/hr/agent/mod.rs`:

```rust
pub mod query_memory;
pub use query_memory::*;
```

- [ ] **Step 3: Run build to verify**

Run: `cargo build`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/handlers/hr/agent/query_memory.rs src/handlers/hr/agent/mod.rs
git commit -m "feat(handler): add query_memory neural tool"
```

---

## Task 6: Create create_memory handler

**Files:**
- Create: `src/handlers/hr/agent/create_memory.rs`
- Modify: `src/handlers/hr/agent/mod.rs`

- [ ] **Step 1: Create handler with neural tool registration**

```rust
use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{CreateMemoryParams, CreateMemoryResponse};
use common::error::Result;
use crate::pkg::request_context::RequestContext;
use crate::models::memory::{MemoryCreateParams, ShortTermMemoryIndexPo};
use crate::service::domain::runtime::domain as runtime_domain;

#[register_handler_tool(
    id = "create_memory",
    name = "create_memory",
    description = "Create a new memory (short_term or knowledge_node)",
    params = "common::api::CreateMemoryParams",
    neural,
)]
#[generate_http_handler]
pub async fn create_memory(
    ctx: RequestContext,
    params: CreateMemoryParams,
) -> Result<CreateMemoryResponse> {
    let agent_id = ctx.agent_id()
        .ok_or_else(|| common::error::Error::bad_request("agent_id is required"))?;
    
    let tags_json = params.tags.as_ref()
        .map(|t| serde_json::to_string(t).unwrap_or("[]".to_string()))
        .unwrap_or("[]".to_string());
    
    let memory_type = params.memory_type.as_str();
    let create_params = match memory_type {
        "short_term" => {
            let index = ShortTermMemoryIndexPo::new(
                agent_id.to_string(),
                params.content,
                params.summary.unwrap_or_else(|| params.content.clone()),
                tags_json,
                params.task_id.clone(),
            );
            MemoryCreateParams::CreateShortTerm(index)
        }
        "knowledge_node" => {
            let node = crate::models::memory::LongTermKnowledgeNodePo::new(
                agent_id.to_string(),
                params.content,
                params.summary.unwrap_or_default(),
                tags_json,
                None,
                vec![],
            );
            MemoryCreateParams::CreateKnowledgeNode {
                node,
                references: vec![],
            }
        }
        _ => {
            return Err(common::error::Error::bad_request(format!(
                "unsupported memory_type: {}", memory_type
            )));
        }
    };
    
    let results = runtime_domain().memory().create(ctx, create_params).await?;
    let memory_id = results.first()
        .map(|m| match &m.po {
            crate::models::memory::MemoryPo::ShortTerm(st) => st.id.clone(),
            crate::models::memory::MemoryPo::KnowledgeNode(kn) => kn.id.clone(),
            _ => "unknown".to_string(),
        })
        .unwrap_or("unknown".to_string());
    
    Ok(CreateMemoryResponse { memory_id })
}
```

- [ ] **Step 2: Register handler in mod.rs**

Modify `src/handlers/hr/agent/mod.rs`:

```rust
pub mod create_memory;
pub use create_memory::*;
```

- [ ] **Step 3: Run build to verify**

Run: `cargo build`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/handlers/hr/agent/create_memory.rs src/handlers/hr/agent/mod.rs
git commit -m "feat(handler): add create_memory neural tool"
```

---

## Task 7: Create update_memory handler

**Files:**
- Create: `src/handlers/hr/agent/update_memory.rs`
- Modify: `src/handlers/hr/agent/mod.rs`

- [ ] **Step 1: Create handler with neural tool registration**

```rust
use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{UpdateMemoryParams, UpdateMemoryResponse};
use common::error::Result;
use crate::pkg::request_context::RequestContext;
use crate::service::dao::memory::MemoryQuery;
use crate::service::domain::runtime::domain as runtime_domain;

#[register_handler_tool(
    id = "update_memory",
    name = "update_memory",
    description = "Update an existing memory (short_term or knowledge_node)",
    params = "common::api::UpdateMemoryParams",
    neural,
)]
#[generate_http_handler]
pub async fn update_memory(
    ctx: RequestContext,
    params: UpdateMemoryParams,
) -> Result<UpdateMemoryResponse> {
    let agent_id = ctx.agent_id()
        .ok_or_else(|| common::error::Error::bad_request("agent_id is required"))?;
    
    let query = MemoryQuery {
        ids: Some(vec![params.memory_id.clone()]),
        agent_id: Some(agent_id.to_string()),
        ..Default::default()
    };
    
    let memories = runtime_domain().memory().query(ctx.clone(), query).await?;
    let mut memory = memories.into_iter().next()
        .ok_or_else(|| common::error::Error::not_found("Memory not found"))?;
    
    match &mut memory.po {
        crate::models::memory::MemoryPo::ShortTerm(st) => {
            if let Some(content) = &params.content {
                st.summary = content.clone();
            }
            if let Some(summary) = &params.summary {
                st.summary = summary.clone();
            }
            if let Some(tags) = &params.tags {
                st.tags = serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string());
            }
        }
        crate::models::memory::MemoryPo::KnowledgeNode(kn) => {
            if let Some(content) = &params.content {
                kn.node_description = content.clone();
            }
            if let Some(summary) = &params.summary {
                kn.summary = summary.clone();
            }
            if let Some(tags) = &params.tags {
                kn.tags = serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string());
            }
        }
        _ => {
            return Err(common::error::Error::bad_request(
                "Only short_term and knowledge_node memories can be updated".to_string()
            ));
        }
    }
    
    runtime_domain().memory().update(ctx, memory).await?;
    
    Ok(UpdateMemoryResponse {
        memory_id: params.memory_id,
    })
}
```

- [ ] **Step 2: Register handler in mod.rs**

Modify `src/handlers/hr/agent/mod.rs`:

```rust
pub mod update_memory;
pub use update_memory::*;
```

- [ ] **Step 3: Run build to verify**

Run: `cargo build`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/handlers/hr/agent/update_memory.rs src/handlers/hr/agent/mod.rs
git commit -m "feat(handler): add update_memory neural tool"
```

---

## Task 8: Create delete_memory handler

**Files:**
- Create: `src/handlers/hr/agent/delete_memory.rs`
- Modify: `src/handlers/hr/agent/mod.rs`

- [ ] **Step 1: Create handler with neural tool registration**

```rust
use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{DeleteMemoryParams, DeleteMemoryResponse};
use common::error::Result;
use crate::pkg::request_context::RequestContext;
use crate::service::dao::memory::MemoryQuery;
use crate::service::domain::runtime::domain as runtime_domain;

#[register_handler_tool(
    id = "delete_memory",
    name = "delete_memory",
    description = "Delete a memory (soft delete for short_term, cascade delete for knowledge_node)",
    params = "common::api::DeleteMemoryParams",
    neural,
)]
#[generate_http_handler]
pub async fn delete_memory(
    ctx: RequestContext,
    params: DeleteMemoryParams,
) -> Result<DeleteMemoryResponse> {
    let agent_id = ctx.agent_id()
        .ok_or_else(|| common::error::Error::bad_request("agent_id is required"))?;
    
    let query = MemoryQuery {
        ids: Some(vec![params.memory_id.clone()]),
        agent_id: Some(agent_id.to_string()),
        ..Default::default()
    };
    
    let memories = runtime_domain().memory().query(ctx.clone(), query).await?;
    let memory = memories.into_iter().next()
        .ok_or_else(|| common::error::Error::not_found("Memory not found"))?;
    
    runtime_domain().memory().delete(ctx, memory).await?;
    
    Ok(DeleteMemoryResponse {
        memory_id: params.memory_id,
    })
}
```

- [ ] **Step 2: Register handler in mod.rs**

Modify `src/handlers/hr/agent/mod.rs`:

```rust
pub mod delete_memory;
pub use delete_memory::*;
```

- [ ] **Step 3: Run build to verify**

Run: `cargo build`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/handlers/hr/agent/delete_memory.rs src/handlers/hr/agent/mod.rs
git commit -m "feat(handler): add delete_memory neural tool"
```

---

## Task 9: Create send_message handler

**Files:**
- Create: `src/handlers/finance/message/send_message.rs`
- Modify: `src/handlers/finance/message/mod.rs`

- [ ] **Step 1: Create handler with neural tool registration**

```rust
use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{SendMessageParams, SendMessageResponse};
use common::error::Result;
use crate::pkg::request_context::RequestContext;

#[register_handler_tool(
    id = "send_message",
    name = "send_message",
    description = "Send a message to a user",
    params = "common::api::SendMessageParams",
    neural,
)]
#[generate_http_handler]
pub async fn send_message(
    ctx: RequestContext,
    params: SendMessageParams,
) -> Result<SendMessageResponse> {
    let message_domain = crate::service::domain::message::MessageDomainImpl::new();
    
    let reply_to_id = params.reply_to_id.as_deref();
    let project_id = params.project_id.as_deref();
    let task_id = params.task_id.as_deref();
    
    let cmd = crate::service::domain::message::SendToUserCommand {
        from_agent_id: ctx.agent_id().unwrap_or("system"),
        to_user_id: &params.to_user_id,
        content: &params.content,
        project_id,
        task_id,
        reply_to_id,
    };
    
    let message = message_domain.delivery().send_to_user(ctx, cmd).await?;
    
    Ok(SendMessageResponse {
        message_id: message.po.id,
    })
}
```

- [ ] **Step 2: Register handler in mod.rs**

Modify `src/handlers/finance/message/mod.rs`:

```rust
pub mod send_message;
pub use send_message::*;
```

- [ ] **Step 3: Run build to verify**

Run: `cargo build`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/handlers/finance/message/send_message.rs src/handlers/finance/message/mod.rs
git commit -m "feat(handler): add send_message neural tool"
```

---

## Task 10: Create request_tool_call handler

**Files:**
- Create: `src/handlers/finance/tool/request_tool_call.rs`
- Modify: `src/handlers/finance/tool/mod.rs`

- [ ] **Step 1: Create handler with neural tool registration**

```rust
use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{RequestToolCallParams, RequestToolCallResponse};
use common::error::Result;
use crate::pkg::request_context::RequestContext;

#[register_handler_tool(
    id = "request_tool_call",
    name = "request_tool_call",
    description = "Request an exoskeleton tool call (async)",
    params = "common::api::RequestToolCallParams",
    neural,
)]
#[generate_http_handler]
pub async fn request_tool_call(
    ctx: RequestContext,
    params: RequestToolCallParams,
) -> Result<RequestToolCallResponse> {
    let tool_execution = crate::service::domain::runtime::tool_execution::ToolExecutionImpl::new();
    
    let task_id = params.task_id.as_deref();
    let result = tool_execution.request_execution(
        ctx,
        &params.tool_id,
        params.params,
        task_id,
    ).await?;
    
    Ok(RequestToolCallResponse {
        tool_call_id: result.tool_call_id,
        status: "pending".to_string(),
    })
}
```

- [ ] **Step 2: Register handler in mod.rs**

Modify `src/handlers/finance/tool/mod.rs`:

```rust
pub mod request_tool_call;
pub use request_tool_call::*;
```

- [ ] **Step 3: Run build to verify**

Run: `cargo build`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/handlers/finance/tool/request_tool_call.rs src/handlers/finance/tool/mod.rs
git commit -m "feat(handler): add request_tool_call neural tool"
```

---

## Task 11: Create mark_done handler

**Files:**
- Create: `src/handlers/project/task/mark_done.rs`
- Modify: `src/handlers/project/task/mod.rs`

- [ ] **Step 1: Create handler with neural tool registration**

```rust
use ai_orz_macros::{register_handler_tool, generate_http_handler};
use common::api::{MarkDoneParams, MarkDoneResponse};
use common::error::Result;
use crate::pkg::request_context::RequestContext;

#[register_handler_tool(
    id = "mark_done",
    name = "mark_done",
    description = "Mark a task as completed",
    params = "common::api::MarkDoneParams",
    neural,
)]
#[generate_http_handler]
pub async fn mark_done(
    ctx: RequestContext,
    params: MarkDoneParams,
) -> Result<MarkDoneResponse> {
    let project_domain = crate::service::domain::project::ProjectDomainImpl::new();
    
    let task = project_domain.task_manage()
        .get_task(ctx.clone(), &params.task_id)
        .await?
        .ok_or_else(|| common::error::Error::not_found("Task not found"))?;
    
    let mut task = task;
    project_domain.task_manage()
        .complete(ctx, &mut task)
        .await?;
    
    Ok(MarkDoneResponse {
        task_id: params.task_id,
        status: "completed".to_string(),
    })
}
```

- [ ] **Step 2: Register handler in mod.rs**

Modify `src/handlers/project/task/mod.rs`:

```rust
pub mod mark_done;
pub use mark_done::*;
```

- [ ] **Step 3: Run build to verify**

Run: `cargo build`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/handlers/project/task/mark_done.rs src/handlers/project/task/mod.rs
git commit -m "feat(handler): add mark_done neural tool"
```

---

## Task 12: Mark list_tools as neural tool

**Files:**
- Modify: `src/handlers/finance/tool/list_tools.rs`

- [ ] **Step 1: Read existing list_tools handler**

Run: `cat src/handlers/finance/tool/list_tools.rs`
Expected: See existing `#[register_handler_tool(...)]` macro usage

- [ ] **Step 2: Add neural flag to the macro**

Modify `src/handlers/finance/tool/list_tools.rs`:

```rust
#[register_handler_tool(
    id = "list_tools",
    name = "list_tools",
    description = "List available tools",
    params = "common::api::ListToolsParams",
    neural,  // ← add this
)]
```

- [ ] **Step 3: Run build to verify**

Run: `cargo build`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/handlers/finance/tool/list_tools.rs
git commit -m "feat(handler): mark list_tools as neural tool"
```

---

## Task 13: Filter neural tools in awakening

**Files:**
- Modify: `src/service/domain/runtime/awakening.rs`

- [ ] **Step 1: Read current awakening implementation**

Run: `cat src/service/domain/runtime/awakening.rs`
Expected: See how tools are currently injected

- [ ] **Step 2: Modify tool injection to filter by neural tag**

Add filtering logic when getting tools for the agent:

```rust
// Find where tools are injected (likely in awaken() method)
// Add filter for tools containing "neural" tag

fn get_neural_tools(ctx: RequestContext, agent_id: &str) -> Result<Vec<Box<dyn CoreTool>>> {
    let all_tools = tool_dal.get_tools_for_agent(ctx, agent_id).await?;
    let neural_tools: Vec<Box<dyn CoreTool>> = all_tools
        .into_iter()
        .filter(|tool| {
            let tags: Vec<String> = serde_json::from_str(&tool.po().tags).unwrap_or_default();
            tags.contains(&"neural".to_string())
        })
        .collect();
    Ok(neural_tools)
}
```

- [ ] **Step 3: Run build to verify**

Run: `cargo build`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/service/domain/runtime/awakening.rs
git commit -m "feat(runtime): filter neural tools during awakening"
```

---

## Task 14: Remove auto-reply in message consumer

**Files:**
- Modify: `src/consumer/message.rs`

- [ ] **Step 1: Read current handle_agent_message**

Run: `cat src/consumer/message.rs | grep -A 30 "handle_agent_message"`
Expected: See current implementation with auto-reply

- [ ] **Step 2: Remove auto-reply logic**

Remove the `send_to_user` call after `awaken()`:

```rust
async fn handle_agent_message(&self, message: &Message) -> Result<()> {
    let agent_id = &message.po.to_id;
    if AgentRuntimeStateManager::global().is_unavailable(agent_id) {
        return Err(Error::conflict(format!(
            "Agent {} is busy or resting, message will be retried", agent_id
        )));
    }
    let ctx = self.rebuild_context(message);
    let agent = self.hr_domain.agent_manage()
        .get_agent(ctx.clone(), agent_id).await?
        .ok_or_else(|| Error::not_found(format!("Agent {} not found", agent_id)))?;
    if agent.brain.is_none() {
        return Err(Error::internal(format!("Agent {} 大脑未唤醒", agent_id)));
    }
    let awaken_result = self.runtime_domain.awakening()
        .awaken(ctx.clone(), &agent, message).await?;
    
    // Remove this block:
    // let reply_cmd = crate::service::domain::message::SendToUserCommand {
    //     from_agent_id: &agent.po.id,
    //     to_user_id: &message.po.from_id,
    //     content: &awaken_result.raw_output,
    //     project_id: message.po.project_id.as_deref(),
    //     task_id: message.po.task_id.as_deref(),
    //     reply_to_id: Some(&message.po.id),
    // };
    // let reply_message = self.message_domain.delivery()
    //     .send_to_user(ctx.clone(), reply_cmd).await?;
    
    Ok(())
}
```

- [ ] **Step 3: Run build to verify**

Run: `cargo build`
Expected: PASS

- [ ] **Step 4: Update tests to not expect auto-reply**

Modify `src/consumer/message_tests.rs`:

```rust
// In test_awaken_success_sends_reply, change expectation
// from expecting reply message to just expecting no error
```

- [ ] **Step 5: Run tests to verify**

Run: `cargo test --lib consumer::message_tests`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/consumer/message.rs src/consumer/message_tests.rs
git commit -m "feat(consumer): remove auto-reply, agent sends via send_message tool"
```

---

## Task 15: Final verification

**Files:**
- All files

- [ ] **Step 1: Run all tests**

Run: `cargo test`
Expected: All tests PASS (548+)

- [ ] **Step 2: Run clippy**

Run: `cargo clippy`
Expected: No errors

- [ ] **Step 3: Update documentation**

Run: `git status`
Expected: All changes staged

- [ ] **Step 4: Commit and push**

```bash
git add -A
git commit -m "feat(neural-tools): complete Phase 2 implementation"
git push
```

---

## Self-Review

**1. Spec coverage:**
- ✅ Task 1: Macro extension with neural flag and tags
- ✅ Task 2: DTOs for neural tools (including full memory CRUD)
- ✅ Task 3: Extend RuntimeMemory trait with search/query/create/update/delete
- ✅ Task 4: search_memory tool (calls `runtime_domain.memory().search()`)
- ✅ Task 5: query_memory tool (calls `runtime_domain.memory().query()`)
- ✅ Task 6: create_memory tool (calls `runtime_domain.memory().create()`)
- ✅ Task 7: update_memory tool (calls `runtime_domain.memory().update()`)
- ✅ Task 8: delete_memory tool (calls `runtime_domain.memory().delete()`)
- ✅ Task 9: send_message tool (calls `message_domain.delivery().send_to_user()`)
- ✅ Task 10: request_tool_call tool (calls `runtime_domain.tool_execution()`)
- ✅ Task 11: mark_done tool (calls `project_domain.task_manage().complete()`)
- ✅ Task 12: list_tools marked as neural
- ✅ Task 13: Filter neural tools in awakening
- ✅ Task 14: Remove auto-reply
- ✅ Task 15: Final verification

**2. Placeholder scan:**
- No TBD/TODO in plan
- All code blocks contain complete code
- All test cases have expected output
- All commands are exact

**3. Type consistency:**
- DTO names match between common and handlers
- Tool IDs match between registration and handler names
- Handler signatures follow standard pattern
- Memory CRUD handlers call `runtime_domain().memory()` not `memory_dal()`
- RuntimeMemory trait methods match MemoryDal signatures (delegated)
- Layering: Handler → Domain → DAL (strict, no shortcuts)
