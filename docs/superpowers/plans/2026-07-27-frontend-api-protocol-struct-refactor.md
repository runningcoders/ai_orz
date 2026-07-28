# 前端 API 协议结构体统一改造实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 `frontend/src/api/*.rs` 中 54 个"拆参数"方法统一改造为接受 `common::api::*` 协议结构体作为入参，URL 拼接逻辑由方法内部手工处理，废弃 `StatsOptions` / `build_url_with_stats`。

**Architecture:** 入参出参统一为 common 协议结构体（path/query/body 字段已通过 `#[param(source)]` 标注，宏已存在），方法内部手工分离 path/query/body（不引入新 macro）；body-only 和单字段方法保持不动；54 个方法按文件分批改造，每批改完编译验证后提交。调用方按需更新。

**Tech Stack:** Rust + Dioxus 0.7 + reqwest + serde + common::api 协议结构体（已存在，含 `#[param(source = "path"/"query"/"body")]` 标注）

**改造原则（务必遵守）:**
1. **拆参数方法**：签名改为接受 common 协议结构体；URL 拼接逻辑收敛到方法内部
2. **body-only 方法**：不动（已经是协议结构体）
3. **单字段方法**：保持原始类型（如 `delete_agent(id: &str)`），不包一层 `DeleteXxxRequest`
4. **不引入新 macro**：path/query/body 分配由方法内部手工代码完成
5. **StatsOptions 废弃**：统计参数纳入对应 `GetXxxRequest`（common 已含 `with_stats` 等字段）
6. **重复实现合并**：`hr.rs::list_tools` 改为重导出 `finance::list_tools`
7. **验证方式**：`cd frontend && cargo check --target wasm32-unknown-unknown` 编译 + `cargo test` 运行 46 个前端测试 + 手动 review（Dioxus 0.8 测试生态不成熟，不强制 TDD）

**改造范围（54 个方法）:**

| 文件 | 拆参数方法数 | 单字段（不动） | body-only（不动） |
|------|------|------|------|
| `auth.rs` | 0 | 0 | 4 |
| `finance.rs` | 14 | 10 | 14 |
| `hr.rs` | 18 | 8 | 6 |
| `project.rs` | 10 | 4 | 5 |
| `system.rs` | 6 | 7 | 8 |
| `log_stats.rs` | 2 | 0 | 0 |
| `message.rs` | 4 | 0 | 1 |
| `organization.rs` | 0 | 1 | 8 |

---

## File Structure

- `frontend/src/api/mod.rs` — 添加 `build_query_string` helper；改造完成后清理 `StatsOptions` / `build_url_with_stats`
- `frontend/src/api/finance.rs` — 改造 14 个拆参数方法
- `frontend/src/api/hr.rs` — 改造 18 个拆参数方法 + 合并 `list_tools` 重复实现
- `frontend/src/api/project.rs` — 改造 10 个拆参数方法
- `frontend/src/api/system.rs` — 改造 6 个拆参数方法
- `frontend/src/api/log_stats.rs` — 改造 2 个拆参数方法
- `frontend/src/api/message.rs` — 改造 4 个拆参数方法
- `frontend/src/pages/**/*.rs` — 更新调用方（按需）
- `frontend/src/hooks/*.rs` — 更新调用方（按需）
- `docs/frontend_architecture.md` — 更新 API 客户端章节说明

---

## 通用改造模式（参考）

### 模式 A：path + body（如 `update_agent`）

**改造前：**
```rust
pub async fn update_agent(id: &str, req: UpdateAgentRequest) -> Result<UpdateAgentResponse, ApiError> {
    api_put(&format!("/api/v1/hr/agents/{}", id), &req).await
}
```

**改造后：**
```rust
pub async fn update_agent(req: UpdateAgentRequest) -> Result<UpdateAgentResponse, ApiError> {
    api_put(&format!("/api/v1/hr/agents/{}", req.id), &req).await
}
```

**调用方改造：**
```rust
// 改造前
update_agent(&id, UpdateAgentRequest { name: ..., .. }).await
// 改造后
UpdateAgentRequest { id: id.clone(), name: ..., .. }.pass_to(update_agent).await
// 或更直接
update_agent(UpdateAgentRequest { id, name: ..., .. }).await
```

### 模式 B：path + query（如 `get_agent` 含 stats）

**改造前：**
```rust
pub async fn get_agent(id: &str, stats_options: Option<&super::StatsOptions>) -> Result<GetAgentResponse, ApiError> {
    let url = super::build_url_with_stats(&format!("/api/v1/hr/agents/{}", id), stats_options);
    api_get(&url).await
}
```

**改造后：**
```rust
pub async fn get_agent(req: GetAgentRequest) -> Result<GetAgentResponse, ApiError> {
    let mut query: Vec<String> = Vec::new();
    if let Some(v) = req.with_stats { query.push(format!("with_stats={}", v)); }
    if let Some(v) = req.with_model_call_stats { query.push(format!("with_model_call_stats={}", v)); }
    if let Some(v) = req.stats_time_start { query.push(format!("stats_time_start={}", v)); }
    if let Some(v) = req.stats_time_end { query.push(format!("stats_time_end={}", v)); }
    if let Some(v) = &req.stats_interval { query.push(format!("stats_interval={}", v)); }
    let qs = if query.is_empty() { String::new() } else { format!("?{}", query.join("&")) };
    api_get(&format!("/api/v1/hr/agents/{}{}", req.id, qs)).await
}
```

### 模式 C：path only + 空 body（如 `install_tool_pack`）

**改造前：**
```rust
pub async fn install_tool_pack(agent_id: &str, tag: &str) -> Result<(), ApiError> {
    let body = serde_json::json!({});
    api_post_empty(&format!("/api/v1/hr/agents/{}/tool-packs/{}", agent_id, tag), &body).await
}
```

**改造后：**
```rust
pub async fn install_tool_pack(req: InstallToolPackRequest) -> Result<(), ApiError> {
    let body = serde_json::json!({});
    api_post_empty(&format!("/api/v1/hr/agents/{}/tool-packs/{}", req.agent_id, req.tag), &body).await
}
```

### 模式 D：query 分页（如 `list_agents`）

**改造前：**
```rust
pub async fn list_agents(limit: Option<usize>, offset: Option<usize>) -> Result<PagedResult<AgentListItem>, ApiError> {
    let mut params: Vec<String> = Vec::new();
    if let Some(l) = limit { params.push(format!("limit={}", l)); }
    if let Some(o) = offset { params.push(format!("offset={}", o)); }
    let url = if params.is_empty() { "/api/v1/hr/agents".to_string() } else { format!("/api/v1/hr/agents?{}", params.join("&")) };
    api_get(&url).await
}
```

**改造后：**
```rust
pub async fn list_agents(req: ListAgentsRequest) -> Result<PagedResult<AgentListItem>, ApiError> {
    let url = super::build_pagination_url("/api/v1/hr/agents", &req.pagination);
    api_get(&url).await
}
```

---

## Task 1: mod.rs 添加分页 query helper

**Files:**
- Modify: `frontend/src/api/mod.rs`

- [ ] **Step 1: 添加 `build_pagination_url` helper**

在 `frontend/src/api/mod.rs` 末尾（`build_url_with_stats` 之后）添加：

```rust
/// 构造分页 URL：把 `PaginationParams` 序列化为 query string 附加到 base_url
pub fn build_pagination_url(base_url: &str, pagination: &common::api::PaginationParams) -> String {
    let mut params: Vec<String> = Vec::new();
    if let Some(l) = pagination.limit {
        params.push(format!("limit={}", l));
    }
    if let Some(o) = pagination.offset {
        params.push(format!("offset={}", o));
    }
    if params.is_empty() {
        base_url.to_string()
    } else {
        format!("{}?{}", base_url, params.join("&"))
    }
}

/// 构造 query string：从 `&[(&str, Option<String>)]` 过滤 None 后拼接
pub fn build_query_string(params: &[(&str, Option<String>)]) -> String {
    let pairs: Vec<String> = params
        .iter()
        .filter_map(|(k, v)| v.as_ref().map(|val| format!("{}={}", k, val)))
        .collect();
    if pairs.is_empty() {
        String::new()
    } else {
        format!("?{}", pairs.join("&"))
    }
}
```

- [ ] **Step 2: 编译验证**

Run: `cd frontend && cargo check --target wasm32-unknown-unknown 2>&1 | tail -20`
Expected: 编译通过（仅添加新函数，无破坏性变更）

- [ ] **Step 3: 提交**

```bash
git add frontend/src/api/mod.rs
git commit -m "refactor(frontend-api): 添加 build_pagination_url/build_query_string helper"
```

---

## Task 2: 改造 finance.rs（14 个拆参数方法）

**Files:**
- Modify: `frontend/src/api/finance.rs`
- Modify: 调用方（按编译错误定位）

### 2.1 模型提供商（5 个方法）

- [ ] **Step 1: 改造 `get_model_provider`**

把：

```rust
pub async fn get_model_provider(id: &str, stats_options: Option<&super::StatsOptions>) -> Result<GetModelProviderResponse, ApiError> {
    let url = super::build_url_with_stats(&format!("/api/v1/finance/model-providers/{}", id), stats_options);
    api_get(&url).await
}
```

改为：

```rust
pub async fn get_model_provider(req: GetModelProviderRequest) -> Result<GetModelProviderResponse, ApiError> {
    let qs = super::build_query_string(&[
        ("with_stats", req.with_stats.map(|v| v.to_string())),
        ("with_model_call_stats", req.with_model_call_stats.map(|v| v.to_string())),
        ("stats_time_start", req.stats_time_start.map(|v| v.to_string())),
        ("stats_time_end", req.stats_time_end.map(|v| v.to_string())),
        ("stats_interval", req.stats_interval.clone()),
    ]);
    api_get(&format!("/api/v1/finance/model-providers/{}{}", req.id, qs)).await
}
```

- [ ] **Step 2: 改造 `update_model_provider`**

把 `pub async fn update_model_provider(id: &str, req: UpdateModelProviderRequest)` 改为 `pub async fn update_model_provider(req: UpdateModelProviderRequest)`，URL 改为 `format!("/api/v1/finance/model-providers/{}", req.id)`。

- [ ] **Step 3: 改造 `toggle_model_provider`**

把：

```rust
pub async fn toggle_model_provider(id: &str, status: i32) -> Result<(), ApiError> {
    let body = serde_json::json!({ "status": status });
    api_put_empty(&format!("/api/v1/finance/model-providers/{}/status", id), &body).await
}
```

改为：

```rust
pub async fn toggle_model_provider(req: UpdateModelProviderStatusRequest) -> Result<(), ApiError> {
    let body = serde_json::json!({ "status": req.status });
    api_put_empty(&format!("/api/v1/finance/model-providers/{}/status", req.id), &body).await
}
```

注：`UpdateModelProviderStatusRequest` 在 common 已存在，含 `id`（path）+ `status`（body）字段。如不存在，先在 `common/src/api/model_provider.rs` 新增。

- [ ] **Step 4: 改造 `call_model_provider`**

把 `pub async fn call_model_provider(id: &str, prompt: &str)` 改为 `pub async fn call_model_provider(req: CallModelRequest)`，URL 改为 `format!("/api/v1/finance/model-providers/{}/call", req.id)`，body 改为 `serde_json::json!({ "prompt": req.prompt })` 或直接 `&req`（取决于结构体定义）。

- [ ] **Step 5: 改造 `switch_embedding_provider`**

把 `pub async fn switch_embedding_provider(id: &str)` 改为 `pub async fn switch_embedding_provider(req: SwitchEmbeddingProviderRequest)`，URL 改为 `format!("/api/v1/finance/model-providers/{}/switch", req.id)`，body 用 `&req`。

- [ ] **Step 6: 编译并修复调用方**

Run: `cd frontend && cargo check --target wasm32-unknown-unknown 2>&1 | grep "error\[" | head -30`

逐个修复调用方（主要是 `pages/finance/model_provider_detail.rs`、`pages/finance/model_providers.rs`），把 `get_model_provider(&id, Some(&stats_options))` 改为 `get_model_provider(GetModelProviderRequest { id, with_stats: Some(true), with_model_call_stats: Some(true), stats_interval: Some("daily".to_string()), ..Default::default() })`。

- [ ] **Step 7: 提交**

```bash
git add frontend/src/api/finance.rs frontend/src/pages/finance/
git commit -m "refactor(frontend-api): finance 模型提供商 5 个方法统一协议结构体入参"
```

### 2.2 工具管理（5 个方法）

- [ ] **Step 8: 改造 `list_tools`**

把 `pub async fn list_tools(limit: Option<usize>, offset: Option<usize>)` 改为 `pub async fn list_tools(req: ListToolsRequest)`，URL 用 `super::build_pagination_url("/api/v1/finance/tools", &req.pagination)`。

- [ ] **Step 9: 改造 `get_tool`**

参照 Task 2 Step 1 的 `get_model_provider` 模式，改造为接受 `GetToolRequest`。

- [ ] **Step 10: 改造 `update_tool`**

把 `pub async fn update_tool(id: &str, req: UpdateToolRequest)` 改为 `pub async fn update_tool(req: UpdateToolRequest)`，URL 用 `req.id`。

- [ ] **Step 11: 改造 `update_tool_status`**

参照 `toggle_model_provider` 模式，改造为接受 `UpdateToolStatusRequest`。

- [ ] **Step 12: 改造 `debug_call_tool`**

把 `pub async fn debug_call_tool(id: &str, args: &serde_json::Value)` 改为 `pub async fn debug_call_tool(req: DebugCallToolRequest)`，URL 用 `req.id`，body 用 `serde_json::json!({ "args": req.args })` 或 `&req`。

- [ ] **Step 13: 编译并修复调用方**

Run: `cd frontend && cargo check --target wasm32-unknown-unknown 2>&1 | grep "error\[" | head -30`

修复 `pages/finance/tool_detail.rs`、`pages/finance/tools.rs` 中的调用方。

- [ ] **Step 14: 提交**

```bash
git add frontend/src/api/finance.rs frontend/src/pages/finance/
git commit -m "refactor(frontend-api): finance 工具管理 5 个方法统一协议结构体入参"
```

### 2.3 其他（4 个方法）

- [ ] **Step 15: 改造 `update_message_channel_status`**

参照 `toggle_model_provider` 模式，改造为接受 `UpdateMessageChannelStatusRequest`。

- [ ] **Step 16: 改造 `update_mcp_server_status`**

参照 `toggle_model_provider` 模式，改造为接受 `UpdateMcpServerStatusRequest`。

- [ ] **Step 17: 改造 `update_attachment_content`**

把 `pub async fn update_attachment_content(id: &str, content: String)` 改为 `pub async fn update_attachment_content(req: UpdateAttachmentContentRequest)`，URL 用 `req.id`（或 `req.attachment_id`，按结构体定义），body 用 `&req`。

- [ ] **Step 18: 改造 `query_tool_call_entries`**

把 `pub async fn query_tool_call_entries(params: &QueryToolCallEntriesRequest)` 保持入参类型不变，但内部改为用 `build_query_string` 拼接 9 个 query 字段：

```rust
pub async fn query_tool_call_entries(params: &QueryToolCallEntriesRequest) -> Result<QueryToolCallEntriesResponse, ApiError> {
    let qs = super::build_query_string(&[
        ("call_id", params.call_id.clone()),
        ("agent_id", params.agent_id.clone()),
        ("project_id", params.project_id.clone()),
        ("task_id", params.task_id.clone()),
        ("tool_id", params.tool_id.clone()),
        ("status", params.status.clone()),
        ("started_after", params.started_after.map(|v| v.to_string())),
        ("started_before", params.started_before.map(|v| v.to_string())),
        ("limit", params.limit.map(|v| v.to_string())),
    ]);
    api_get(&format!("/api/v1/finance/tool-call-entries{}", qs)).await
}
```

注：实际字段名以 `QueryToolCallEntriesRequest` 定义为准，改造前先 `grep -A 30 "struct QueryToolCallEntriesRequest" common/src/api/`。

- [ ] **Step 19: 编译并修复调用方**

Run: `cd frontend && cargo check --target wasm32-unknown-unknown 2>&1 | grep "error\[" | head -30`

- [ ] **Step 20: 提交**

```bash
git add frontend/src/api/finance.rs frontend/src/pages/finance/
git commit -m "refactor(frontend-api): finance 其他 4 个方法统一协议结构体入参"
```

---

## Task 3: 改造 hr.rs（18 个拆参数方法）

**Files:**
- Modify: `frontend/src/api/hr.rs`
- Modify: 调用方（按编译错误定位）

### 3.1 Agent 管理（5 个方法）

- [ ] **Step 1: 改造 `list_agents`**

把 `pub async fn list_agents(limit: Option<usize>, offset: Option<usize>)` 改为 `pub async fn list_agents(req: ListAgentsRequest)`，URL 用 `super::build_pagination_url("/api/v1/hr/agents", &req.pagination)`。

- [ ] **Step 2: 改造 `get_agent`**

参照 Task 2 Step 1 的 `get_model_provider` 模式（path + 5 个 stats query 字段），改造为接受 `GetAgentRequest`。

- [ ] **Step 3: 改造 `update_agent`**

把 `pub async fn update_agent(id: &str, req: UpdateAgentRequest)` 改为 `pub async fn update_agent(req: UpdateAgentRequest)`，URL 用 `req.id`。

- [ ] **Step 4: 改造 `update_agent_status`**

把 `pub async fn update_agent_status(id: &str, status: i32)` 改为 `pub async fn update_agent_status(req: UpdateAgentStatusRequest)`，URL 用 `req.id`，body 用 `serde_json::json!({ "status": req.status })`。注意 `req.status` 是 `AgentStatus` 枚举，序列化时可能需要 `serde_json::to_value(&req.status).unwrap()`。

- [ ] **Step 5: 删除 `hr.rs::list_tools` 重复实现，改为重导出**

在 `hr.rs` 末尾把：

```rust
pub async fn list_tools(limit: Option<usize>, offset: Option<usize>) -> Result<PagedResult<ToolListItem>, ApiError> {
    // ... 重复实现
}
```

改为：

```rust
pub use super::finance::list_tools;
```

修复所有调用方（如 `pages/hr/agents.rs` 中的 `list_agents` 调用方），改为 `crate::api::finance::list_tools` 或保持 `crate::api::hr::list_tools`（重导出后路径不变）。

- [ ] **Step 6: 编译并修复调用方**

Run: `cd frontend && cargo check --target wasm32-unknown-unknown 2>&1 | grep "error\[" | head -30`

主要调用方：
- `pages/hr/agent_detail.rs`：4 处 `get_agent(&id, Some(&stats_options))` → `get_agent(GetAgentRequest { id, with_stats: Some(true), with_model_call_stats: Some(true), stats_interval: Some("daily".to_string()), ..Default::default() })`
- `pages/hr/agents.rs`：3 处 `list_agents(None, None)` → `list_agents(ListAgentsRequest::default())`
- `hooks/use_workspace_data.rs`：1 处 `list_agents(None, None)`
- `pages/project/task_edit_modal.rs`：1 处 `list_agents(None, None)`

为了减少重复，可以在 `agent_detail.rs` 顶部定义一个 helper：

```rust
fn build_agent_stats_request(id: String) -> common::api::GetAgentRequest {
    common::api::GetAgentRequest {
        id,
        with_stats: Some(true),
        with_model_call_stats: Some(true),
        stats_interval: Some("daily".to_string()),
        ..Default::default()
    }
}
```

然后 4 处调用统一改为 `get_agent(build_agent_stats_request(aid.clone())).await`。

- [ ] **Step 7: 提交**

```bash
git add frontend/src/api/hr.rs frontend/src/pages/hr/ frontend/src/hooks/ frontend/src/pages/project/
git commit -m "refactor(frontend-api): hr Agent 管理 5 个方法统一协议结构体入参 + 合并 list_tools 重复实现"
```

### 3.2 包管理（6 个方法）

- [ ] **Step 8: 改造 `install_tool_pack`**

把 `pub async fn install_tool_pack(agent_id: &str, tag: &str)` 改为 `pub async fn install_tool_pack(req: InstallToolPackRequest)`，URL 用 `req.agent_id` + `req.tag`。

- [ ] **Step 9: 改造 `uninstall_tool_pack`**

把 `pub async fn uninstall_tool_pack(agent_id: &str, tag: &str)` 改为 `pub async fn uninstall_tool_pack(req: UninstallToolPackRequest)`，URL 用 `req.agent_id` + `req.tag`。

- [ ] **Step 10: 改造 `install_skill_pack`**

参照 Step 8 模式，改造为接受 `InstallSkillPackRequest`。

- [ ] **Step 11: 改造 `uninstall_skill_pack`**

参照 Step 9 模式，改造为接受 `UninstallSkillPackRequest`。

- [ ] **Step 12: 改造 `bind_tool_to_agent`**

把 `pub async fn bind_tool_to_agent(agent_id: &str, tool_id: &str)` 改为 `pub async fn bind_tool_to_agent(req: BindToolToAgentRequest)`，URL 用 `req.agent_id` + `req.tool_id`。

- [ ] **Step 13: 改造 `unbind_tool_from_agent`**

参照 Step 12 模式，改造为接受 `UnbindToolFromAgentRequest`。

- [ ] **Step 14: 编译并修复调用方**

Run: `cd frontend && cargo check --target wasm32-unknown-unknown 2>&1 | grep "error\[" | head -30`

主要调用方在 `pages/hr/agent_detail.rs`。

- [ ] **Step 15: 提交**

```bash
git add frontend/src/api/hr.rs frontend/src/pages/hr/
git commit -m "refactor(frontend-api): hr 包管理 6 个方法统一协议结构体入参"
```

### 3.3 技能与记忆（7 个方法）

- [ ] **Step 16: 改造 `list_skills`**

把 `pub async fn list_skills(limit: Option<usize>, offset: Option<usize>)` 改为接受 `ListSkillsRequest`（如 common 无此结构体，先在 `common/src/api/skill.rs` 新增，参照 `ListAgentsRequest` 模式：含 `#[serde(flatten)] #[param(source = "query")] pub pagination: PaginationParams`）。

- [ ] **Step 17: 改造 `update_skill`**

把 `pub async fn update_skill(id: &str, req: UpdateSkillRequest)` 改为 `pub async fn update_skill(req: UpdateSkillRequest)`，URL 用 `req.id`。如 `UpdateSkillRequest` 不含 `id` 字段，先在 common 中补上（`#[param(source = "path")] pub id: String`）。

- [ ] **Step 18: 改造 `get_skill_file_content`**

把 `pub async fn get_skill_file_content(skill_id: &str, filename: &str)` 改为 `pub async fn get_skill_file_content(req: GetSkillFileContentRequest)`，URL 用 `req.skill_id` + `req.filename`。

- [ ] **Step 19: 改造 `update_skill_file_content`**

把 `pub async fn update_skill_file_content(skill_id: &str, filename: &str, content: String)` 改为 `pub async fn update_skill_file_content(req: UpdateSkillFileContentRequest)`，URL 用 `req.skill_id` + `req.filename`，body 用 `&req`。

- [ ] **Step 20: 改造 `search_memory`**

把 `pub async fn search_memory(query: &str, memory_type: Option<&str>, tags: Option<&[String]>)` 改为 `pub async fn search_memory(req: SearchMemoryParams)`，body 用 `&req`。`max_results` 由调用方决定（或在结构体内默认 20）。

- [ ] **Step 21: 改造 `query_memory`**

把 `pub async fn query_memory(agent_id: Option<&str>, memory_type: Option<&str>, tags: Option<&[String]>)` 改为 `pub async fn query_memory(req: QueryMemoryParams)`，body 用 `&req`。

- [ ] **Step 22: 改造 `search_memory_with_traversal`**

把 `pub async fn search_memory_with_traversal(query: &str, seed_node_ids: &[String], depth: i32, tags: Option<&[String]>)` 改为 `pub async fn search_memory_with_traversal(req: SearchMemoryParams)`，body 用 `&req`。

注：`search_memory_with_traversal` 和 `search_memory` 共用同一个 `SearchMemoryParams` 结构体和同一个 URL `/api/v1/hr/agents/search_memory`。改造后两者区别仅在调用方填充的字段不同。可考虑合并为一个方法 + 不同的 default 填充 helper，但本计划保持两个方法独立以减少调用方改动。

- [ ] **Step 23: 编译并修复调用方**

Run: `cd frontend && cargo check --target wasm32-unknown-unknown 2>&1 | grep "error\[" | head -30`

主要调用方：
- `pages/hr/skills.rs`：`list_skills`、`search_skills`
- `pages/hr/skill_detail.rs`：`update_skill`、`get_skill_file_content`、`update_skill_file_content`
- `pages/hr/memory_search.rs`：`search_memory`、`query_memory`
- `pages/hr/knowledge_graph.rs`：`search_memory_with_traversal`

- [ ] **Step 24: 提交**

```bash
git add frontend/src/api/hr.rs frontend/src/pages/hr/ common/src/api/skill.rs
git commit -m "refactor(frontend-api): hr 技能与记忆 7 个方法统一协议结构体入参"
```

---

## Task 4: 改造 project.rs（10 个拆参数方法）

**Files:**
- Modify: `frontend/src/api/project.rs`
- Modify: 调用方

- [ ] **Step 1: 改造 `list_projects`**

参照 `list_agents` 模式，改造为接受 `ListProjectsRequest`。

- [ ] **Step 2: 改造 `get_project`**

参照 `get_agent` 模式（path + 5 个 stats query 字段），改造为接受 `GetProjectRequest`。

- [ ] **Step 3: 改造 `update_project`**

把 `pub async fn update_project(id: &str, req: UpdateProjectRequest)` 改为 `pub async fn update_project(req: UpdateProjectRequest)`，URL 用 `req.id`。

- [ ] **Step 4: 改造 `update_project_status`**

参照 `toggle_model_provider` 模式，改造为接受 `UpdateProjectStatusRequest`。

- [ ] **Step 5: 改造 `list_tasks`**

参照 `list_agents` 模式，改造为接受 `ListTasksRequest`。

- [ ] **Step 6: 改造 `get_task`**

参照 `get_agent` 模式，改造为接受 `GetTaskRequest`。

- [ ] **Step 7: 改造 `update_task`**

把 `pub async fn update_task(id: &str, req: UpdateTaskRequest)` 改为 `pub async fn update_task(req: UpdateTaskRequest)`，URL 用 `req.id`。

- [ ] **Step 8: 改造 `update_task_status`**

参照 `toggle_model_provider` 模式，改造为接受 `UpdateTaskStatusRequest`。

- [ ] **Step 9: 改造 `update_task_progress`**

把 `pub async fn update_task_progress(id: &str, progress: i32)` 改为 `pub async fn update_task_progress(req: UpdateTaskProgressRequest)`，URL 用 `req.id`，body 用 `&req`。

- [ ] **Step 10: 改造 `update_artifact_content`**

把 `pub async fn update_artifact_content(id: &str, content: String)` 改为 `pub async fn update_artifact_content(req: UpdateArtifactContentRequest)`，URL 用 `req.artifact_id`（按结构体定义），body 用 `&req`。

- [ ] **Step 11: 编译并修复调用方**

Run: `cd frontend && cargo check --target wasm32-unknown-unknown 2>&1 | grep "error\[" | head -40`

主要调用方：
- `pages/project/projects.rs`：`list_projects`
- `pages/project/project_detail.rs`：`get_project`（含 stats）、`update_project_status`
- `pages/project/tasks.rs`：`list_tasks`、`update_task_status`
- `pages/project/task_detail.rs`：`get_task`（含 stats）、`update_task_progress`
- `pages/project/task_edit_modal.rs`：`update_task`
- `pages/project/artifacts.rs`、`pages/project/artifact_detail.rs`：`update_artifact_content`
- `hooks/use_workspace_data.rs`：`list_projects`

- [ ] **Step 12: 提交**

```bash
git add frontend/src/api/project.rs frontend/src/pages/project/ frontend/src/hooks/
git commit -m "refactor(frontend-api): project 10 个方法统一协议结构体入参"
```

---

## Task 5: 改造 system.rs（6 个拆参数方法）

**Files:**
- Modify: `frontend/src/api/system.rs`
- Modify: 调用方

- [ ] **Step 1: 改造 `update_cron_trigger`**

把 `pub async fn update_cron_trigger(id: &str, req: UpdateCronTriggerRequest)` 改为 `pub async fn update_cron_trigger(req: UpdateCronTriggerRequest)`，URL 用 `req.id`。如 `UpdateCronTriggerRequest` 不含 `id`，先在 common 补上。

- [ ] **Step 2: 改造 `query_logs`**

把本地 `LogQueryParams` 替换为 common 的 `LogQueryRequest`。如 common 无对应结构体，先在 `common/src/api/log_stats.rs` 新增（参照本地 `LogQueryParams` 字段）。改造后：

```rust
pub async fn query_logs(req: &LogQueryRequest) -> Result<LogPageResult, ApiError> {
    let qs = super::build_query_string(&[
        ("keyword", req.keyword.clone()),
        ("log_id", req.log_id.clone()),
        ("level", req.level.clone()),
        ("start_time", req.start_time.map(|v| v.to_string())),
        ("end_time", req.end_time.map(|v| v.to_string())),
        ("page", req.page.map(|v| v.to_string())),
        ("page_size", req.page_size.map(|v| v.to_string())),
    ]);
    api_get(&format!("/api/v1/system/logs{}", qs)).await
}
```

注：本地 `LogPageResult` 暂保留（响应结构体），仅替换请求参数。实际字段名以 common 结构体为准。

- [ ] **Step 3: 改造 `list_events`**

把 `pub async fn list_events(consumer: &str, order_key: Option<&str>, status: Option<&str>, limit: usize, offset: usize)` 改为 `pub async fn list_events(req: ListEventsRequest)`，URL 用 `format!("/api/v1/system/aop/{}/events", req.consumer)`，query 用 `build_query_string` 拼 `order_key`/`status`/`limit`/`offset`。

- [ ] **Step 4: 改造 `get_event`**

把 `pub async fn get_event(consumer: &str, event_id: &str)` 改为 `pub async fn get_event(req: GetEventRequest)`，URL 用 `format!("/api/v1/system/aop/{}/events/{}", req.consumer, req.event_id)`。

- [ ] **Step 5: 改造 `get_aop_stats_time_series`**

把 `pub async fn get_aop_stats_time_series(event_kind: Option<&str>, consumer_name: Option<&str>, status: Option<&str>)` 改为 `pub async fn get_aop_stats_time_series(req: GetStatsTimeSeriesRequest)`，query 用 `build_query_string` 拼 3 个字段。

- [ ] **Step 6: 改造 `get_aop_stats_distribution`**

把 `pub async fn get_aop_stats_distribution(group_by: &str, status: Option<&str>)` 改为 `pub async fn get_aop_stats_distribution(req: GetStatsDistributionRequest)`，query 用 `build_query_string` 拼 `group_by` + `status`。

- [ ] **Step 7: 编译并修复调用方**

Run: `cd frontend && cargo check --target wasm32-unknown-unknown 2>&1 | grep "error\[" | head -30`

主要调用方：
- `pages/system/triggers.rs`：`update_cron_trigger`
- `pages/system/logs.rs`：`query_logs`
- `pages/system/aop.rs`：`list_events`、`get_event`、`get_aop_stats_time_series`、`get_aop_stats_distribution`

- [ ] **Step 8: 提交**

```bash
git add frontend/src/api/system.rs frontend/src/pages/system/ common/src/api/
git commit -m "refactor(frontend-api): system 6 个方法统一协议结构体入参"
```

---

## Task 6: 改造 log_stats.rs + message.rs（6 个方法）

**Files:**
- Modify: `frontend/src/api/log_stats.rs`
- Modify: `frontend/src/api/message.rs`
- Modify: 调用方

- [ ] **Step 1: 改造 `log_stats.rs::get_log_level_distribution`**

把 `pub async fn get_log_level_distribution(start_time: Option<i64>, end_time: Option<i64>)` 改为 `pub async fn get_log_level_distribution(req: &LogStatsQueryParams)`，query 用 `build_query_string` 拼 `start_time` + `end_time`。

- [ ] **Step 2: 改造 `log_stats.rs::get_log_time_series`**

同 Step 1 模式。

- [ ] **Step 3: 改造 `message.rs::load_latest_messages`**

把 `pub async fn load_latest_messages(project_id: Option<&str>, limit: Option<usize>)` 改为 `pub async fn load_latest_messages(req: ListMessagesRequest)`，query 用 `build_query_string` 拼 `project_id` + `limit`。注意 `project_id` 需要 URL 编码（保留 `url_encode` helper）。

- [ ] **Step 4: 改造 `message.rs::load_older_messages`**

把 `pub async fn load_older_messages(project_id: Option<&str>, before_timestamp: i64, limit: Option<usize>)` 改为接受 `ListMessagesRequest`（含 `before_timestamp` 字段，如 common 无此字段先补上）。

- [ ] **Step 5: 改造 `message.rs::poll_new_messages`**

同 Step 4 模式（含 `after_timestamp` 字段）。

- [ ] **Step 6: 改造 `message.rs::search_messages`**

把 `pub async fn search_messages(keyword: &str, project_id: Option<&str>)` 改为 `pub async fn search_messages(req: SearchMessagesRequest)`，body 用 `&req`。如 `SearchMessagesRequest` 不含 `limit` 字段，调用方需显式设置或后端给默认值。

- [ ] **Step 7: 编译并修复调用方**

Run: `cd frontend && cargo check --target wasm32-unknown-unknown 2>&1 | grep "error\[" | head -30`

主要调用方：
- `pages/system/logs.rs`：`get_log_level_distribution`、`get_log_time_series`
- `pages/message/chat.rs`、`pages/hr/agent_detail.rs`、`pages/workspace.rs`、`hooks/use_workspace_data.rs`：`load_latest_messages`、`load_older_messages`、`poll_new_messages`、`search_messages`

- [ ] **Step 8: 提交**

```bash
git add frontend/src/api/log_stats.rs frontend/src/api/message.rs frontend/src/pages/ frontend/src/hooks/
git commit -m "refactor(frontend-api): log_stats + message 6 个方法统一协议结构体入参"
```

---

## Task 7: 清理 StatsOptions + 编译全量验证

**Files:**
- Modify: `frontend/src/api/mod.rs`
- Modify: 残留调用方（如有）

- [ ] **Step 1: 全量编译验证**

Run: `cd frontend && cargo check --target wasm32-unknown-unknown 2>&1 | tail -10`
Expected: 编译通过，零 error

- [ ] **Step 2: 全量前端测试**

Run: `cd frontend && cargo test 2>&1 | tail -20`
Expected: 46 个测试全部通过

- [ ] **Step 3: 检查 StatsOptions 残留**

Run: `grep -rn "StatsOptions\|build_url_with_stats" frontend/src/`
Expected: 零匹配（或仅在 mod.rs 中定义但无调用）

- [ ] **Step 4: 从 mod.rs 移除 StatsOptions 和 build_url_with_stats**

删除 `frontend/src/api/mod.rs` 中的：
- `pub struct StatsOptions { ... }`
- `impl StatsOptions { pub fn to_query_string ... }`
- `pub fn build_url_with_stats(...)`

- [ ] **Step 5: 编译验证清理后无残留**

Run: `cd frontend && cargo check --target wasm32-unknown-unknown 2>&1 | tail -10`
Expected: 编译通过

- [ ] **Step 6: 提交**

```bash
git add frontend/src/api/mod.rs
git commit -m "refactor(frontend-api): 清理废弃的 StatsOptions 和 build_url_with_stats"
```

---

## Task 8: 文档更新

**Files:**
- Modify: `docs/frontend_architecture.md`

- [ ] **Step 1: 更新 `docs/frontend_architecture.md` 的 API 客户端章节**

找到 API 客户端章节，补充说明：

```markdown
### API 方法签名约定

- **拆参数方法**（path+query+body 混合）：统一接受 `common::api::*Request` 协议结构体作为入参，URL 拼接逻辑由方法内部手工处理（用 `build_pagination_url` / `build_query_string` helper）
- **body-only 方法**：直接接受协议结构体作为 body
- **单字段方法**（如 `delete_xxx(id)`）：保持原始类型，不包一层 `DeleteXxxRequest`

改造原则：前后端 API 签名对称，调用者无需关心 URL 拼接，协议结构体为 single source of truth。
```

- [ ] **Step 2: 提交**

```bash
git add docs/frontend_architecture.md
git commit -m "docs: 更新前端 API 客户端章节说明协议结构体入参约定"
```

- [ ] **Step 3: 推送所有改造到远程**

```bash
git push origin main
```

---

## Self-Review

### 1. Spec 覆盖

- ✅ 54 个拆参数方法全部覆盖：finance 14（Task 2）+ hr 18（Task 3）+ project 10（Task 4）+ system 6（Task 5）+ log_stats 2 + message 4（Task 6）
- ✅ StatsOptions 废弃（Task 7）
- ✅ 重复实现合并（Task 3 Step 5：hr::list_tools 重导出）
- ✅ body-only 和单字段方法保持不动（在改造原则中明确）
- ✅ 调用方更新（每个 Task 的 step 都包含编译并修复调用方）
- ✅ 文档更新（Task 8）

### 2. Placeholder 扫描

- 无 "TBD" / "TODO" / "implement later"
- 每个改造 step 都给出了改造前/改造后代码或明确的模式参照
- 特殊场景（如 `query_tool_call_entries` 9 字段 query）给出了完整代码
- 通用场景（path+body）给出了模式 A 代码 + 字段说明，工程师可机械套用

### 3. 类型一致性

- `build_pagination_url(base_url: &str, pagination: &common::api::PaginationParams) -> String`（Task 1 定义，Task 3/4/5 使用）
- `build_query_string(params: &[(&str, Option<String>)]) -> String`（Task 1 定义，Task 2/5/6 使用）
- 所有 Request 结构体名称使用 common 中已有的命名（`GetAgentRequest`、`UpdateAgentRequest` 等）
- `InstallToolPackRequest` / `UninstallToolPackRequest` 等结构体如 common 中不存在，需在对应 Task 中新增（已在 step 中注明）

---

## 执行风险与缓解

1. **common 协议结构体缺失**：部分 Request 可能不存在（如 `InstallToolPackRequest`）。改造前先 `grep -rn "struct InstallToolPackRequest" common/src/api/` 确认，缺失则先在 common 新增。
2. **结构体字段不含 path id**：如 `UpdateSkillRequest` 可能不含 `id` 字段。改造前 `grep -A 20 "struct UpdateSkillRequest" common/src/api/` 确认，缺失则补 `#[param(source = "path")] pub id: String`。
3. **调用方数量多**：54 个方法改造会触发大量调用方编译错误。每个 Task 的"编译并修复调用方"step 是关键，必须逐个修复直到零 error。
4. **Dioxus 0.8 测试生态不成熟**：不强制 TDD，靠 `cargo check` + `cargo test`（46 个现有测试）+ 手动 review 验证。
5. **`search_memory` 与 `search_memory_with_traversal` 共用 `SearchMemoryParams`**：改造后两者签名相同，区别仅在调用方填充的字段。如需更强类型区分，可考虑新增 `SearchMemoryWithTraversalRequest` wrapper，但本计划保持简单。
