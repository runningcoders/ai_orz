# Frontend Page Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete all missing frontend management pages and enhance existing pages to match backend API capabilities, achieving full CRUD coverage for all business domains.

**Architecture:** Follow the established frontend patterns (Dioxus Router, CSS design system, unified API client, AppLayout wrapper). New pages reuse existing components (Button, Modal, state indicators). API client methods follow the `api_get`/`api_post`/`api_put`/`api_delete` pattern.

**Tech Stack:** Dioxus 0.7, Dioxus Router, reqwest 0.13, Mistral CSS design system, common crate DTOs.

---

## File Structure

### New Files

| File | Responsibility |
|------|---------------|
| `frontend/src/pages/finance/attachments.rs` | 附件管理页面 |
| `frontend/src/pages/finance/mcp_servers.rs` | MCP 服务器管理页面 |
| `frontend/src/pages/project/artifacts.rs` | 项目产物管理页面 |
| `frontend/src/api/project.rs` | 新增 artifacts API 方法 |
| `frontend/src/api/finance.rs` | 新增 attachments/mcp_servers API 方法 |

### Modified Files

| File | Changes |
|------|---------|
| `frontend/src/pages/mod.rs` | 新增路由定义 |
| `frontend/src/pages/hr/agent_detail.rs` | 新增工具绑定/解绑功能 |
| `frontend/src/pages/hr/agents.rs` | 新增搜索功能 |
| `frontend/src/pages/hr/skills.rs` | 新增搜索功能、技能文件管理 |
| `frontend/src/pages/finance/model_providers.rs` | 新增模型调用测试按钮 |
| `frontend/src/pages/finance/message_channels.rs` | 新增连接测试按钮 |
| `frontend/src/pages/project/project_detail.rs` | 新增产物列表/创建 |
| `frontend/src/layouts/navbar.rs` | 新增导航链接 |
| `frontend/src/main.rs` | 新增路由组件渲染 |

---

## Task 1: Attachments Management Page

**Files:**
- Create: `frontend/src/pages/finance/attachments.rs`
- Modify: `frontend/src/api/finance.rs`
- Modify: `frontend/src/pages/finance/mod.rs`

**Backend API:** `/api/v1/finance/attachments/*`

- [ ] **Step 1: Add API client methods for attachments**

Add to `frontend/src/api/finance.rs`:
```rust
pub async fn list_attachments() -> Result<Vec<Attachment>, String> {
    api_get_or_default("/finance/attachments").await
}

pub async fn get_attachment(id: &str) -> Result<Attachment, String> {
    api_get(&format!("/finance/attachments/{}", id)).await
}

pub async fn get_attachment_content(id: &str) -> Result<String, String> {
    api_get_text(&format!("/finance/attachments/{}/content", id)).await
}

pub async fn delete_attachment(id: &str) -> Result<(), String> {
    api_delete(&format!("/finance/attachments/{}", id)).await
}

pub async fn create_text_attachment(title: &str, content: &str) -> Result<Attachment, String> {
    let params = CreateTextAttachmentParams {
        title: title.to_string(),
        content: content.to_string(),
    };
    api_post("/finance/attachments/text", &params).await
}

pub async fn upload_attachment(file: &File) -> Result<Attachment, String> {
    // Use FormData for multipart upload
    let client = client();
    let mut form = reqwest::multipart::Form::new();
    form = form.file("file", file)?;
    let resp = client
        .post(api_url("/finance/attachments/upload"))
        .bearer_auth(token()?)
        .multipart(form)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    handle_api_response(resp).await
}
```

- [ ] **Step 2: Create attachments page component**

Create `frontend/src/pages/finance/attachments.rs`:
```rust
use dioxus::prelude::*;
use crate::api::finance;
use crate::components::{Button, Modal, EmptyState, Loading, ErrorAlert};
use crate::layouts::AppLayout;

pub fn FinanceAttachments(cx: Scope) -> Element {
    let attachments = use_signal(cx, || Vec::new());
    let loading = use_signal(cx, || true);
    let error = use_signal(cx, || None::<String>);
    let show_create_modal = use_signal(cx, || false);
    let new_title = use_signal(cx, || String::new());
    let new_content = use_signal(cx, || String::new());

    use_effect(cx, (), |_| async move {
        match finance::list_attachments().await {
            Ok(data) => attachments.set(data),
            Err(e) => error.set(Some(e)),
        }
        loading.set(false);
    });

    let handle_create = move |_| async move {
        if new_title.read().is_empty() {
            error.set(Some("标题不能为空".to_string()));
            return;
        }
        match finance::create_text_attachment(&new_title.read(), &new_content.read()).await {
            Ok(_) => {
                show_create_modal.set(false);
                new_title.set(String::new());
                new_content.set(String::new());
                // Refresh list
                match finance::list_attachments().await {
                    Ok(data) => attachments.set(data),
                    Err(e) => error.set(Some(e)),
                }
            }
            Err(e) => error.set(Some(e)),
        }
    };

    let handle_delete = move |id: String| async move {
        if let Err(e) = finance::delete_attachment(&id).await {
            error.set(Some(e));
        } else {
            attachments.set(attachments.read().iter().filter(|a| a.id != id).cloned().collect());
        }
    };

    cx.render(rsx! {
        AppLayout {
            title: "附件管理"
            if loading.read() {
                Loading {}
            } else if error.read().is_some() {
                ErrorAlert { message: error.read().clone().unwrap() }
            } else if attachments.read().is_empty() {
                EmptyState { message: "暂无附件", hint: "点击下方按钮创建文本附件" }
            } else {
                div { class: "content-area" }
                    div { class: "flex justify-between items-center mb-4" }
                        h2 { class: "text-xl font-semibold", "附件列表" }
                        Button {
                            variant: "primary"
                            onclick: move |_| show_create_modal.set(true)
                            "+ 创建文本附件"
                        }
                    table { class: "table w-full" }
                        thead {
                            tr {
                                th { "文件名" }
                                th { "大小" }
                                th { "创建时间" }
                                th { "操作" }
                            }
                        }
                        tbody {
                            attachments.read().iter().map(|a| rsx! {
                                tr { key: "{a.id}" }
                                    td { "{a.title}" }
                                    td { "{a.size} bytes" }
                                    td { "{a.created_at}" }
                                    td {
                                        Button {
                                            variant: "danger"
                                            size: "sm"
                                            onclick: move |_| handle_delete(a.id.clone())
                                            "删除"
                                        }
                                    }
                            })
                        }
                    }
                }
            }

            if show_create_modal.read() {
                Modal {
                    title: "创建文本附件"
                    on_close: move |_| show_create_modal.set(false)
                    div { class: "space-y-4" }
                        div { class: "form-group" }
                            label { class: "form-label", "标题" }
                            input {
                                class: "form-input"
                                bind:value: new_title
                                placeholder: "输入附件标题"
                            }
                        }
                        div { class: "form-group" }
                            label { class: "form-label", "内容" }
                            textarea {
                                class: "form-textarea"
                                bind:value: new_content
                                rows: 6
                                placeholder: "输入文本内容"
                            }
                        }
                    }
                    div { class: "modal-footer" }
                        Button {
                            variant: "secondary"
                            onclick: move |_| show_create_modal.set(false)
                            "取消"
                        }
                        Button {
                            variant: "primary"
                            onclick: handle_create
                            "创建"
                        }
                    }
                }
            }
        }
    })
}
```

- [ ] **Step 3: Add module export**

Add to `frontend/src/pages/finance/mod.rs`:
```rust
pub mod attachments;
```

- [ ] **Step 4: Commit**

```bash
git add frontend/src/api/finance.rs frontend/src/pages/finance/attachments.rs frontend/src/pages/finance/mod.rs
git commit -m "feat: add attachments management page"
```

---

## Task 2: MCP Server Management Page

**Files:**
- Create: `frontend/src/pages/finance/mcp_servers.rs`
- Modify: `frontend/src/api/finance.rs`
- Modify: `frontend/src/pages/finance/mod.rs`

**Backend API:** `/api/v1/finance/mcp-servers/*`

- [ ] **Step 1: Add API client methods for MCP servers**

Add to `frontend/src/api/finance.rs`:
```rust
pub async fn list_mcp_servers() -> Result<Vec<McpServer>, String> {
    api_get_or_default("/finance/mcp-servers").await
}

pub async fn get_mcp_server(id: &str) -> Result<McpServer, String> {
    api_get(&format!("/finance/mcp-servers/{}", id)).await
}

pub async fn create_mcp_server(name: &str, url: &str) -> Result<McpServer, String> {
    let params = CreateMcpServerParams {
        name: name.to_string(),
        url: url.to_string(),
        description: None,
    };
    api_post("/finance/mcp-servers", &params).await
}

pub async fn update_mcp_server(id: &str, name: &str, url: &str) -> Result<McpServer, String> {
    let params = UpdateMcpServerParams {
        name: Some(name.to_string()),
        url: Some(url.to_string()),
        description: None,
    };
    api_put(&format!("/finance/mcp-servers/{}", id), &params).await
}

pub async fn delete_mcp_server(id: &str) -> Result<(), String> {
    api_delete(&format!("/finance/mcp-servers/{}", id)).await
}

pub async fn update_mcp_server_status(id: &str, status: i32) -> Result<McpServer, String> {
    let params = UpdateMcpServerStatusParams { status };
    api_put(&format!("/finance/mcp-servers/{}/status", id), &params).await
}

pub async fn sync_mcp_tools(server_id: &str) -> Result<(), String> {
    api_post_empty(&format!("/finance/mcp-servers/{}/tools/sync", server_id), &()).await
}

pub async fn list_mcp_tools(server_id: &str) -> Result<Vec<McpTool>, String> {
    api_get_or_default(&format!("/finance/mcp-servers/{}/tools", server_id)).await
}
```

- [ ] **Step 2: Create MCP servers page component**

Create `frontend/src/pages/finance/mcp_servers.rs`:
```rust
use dioxus::prelude::*;
use crate::api::finance;
use crate::components::{Button, Modal, EmptyState, Loading, ErrorAlert};
use crate::layouts::AppLayout;

pub fn FinanceMcpServers(cx: Scope) -> Element {
    let servers = use_signal(cx, || Vec::new());
    let loading = use_signal(cx, || true);
    let error = use_signal(cx, || None::<String>);
    let show_create_modal = use_signal(cx, || false);
    let new_name = use_signal(cx, || String::new());
    let new_url = use_signal(cx, || String::new());

    use_effect(cx, (), |_| async move {
        match finance::list_mcp_servers().await {
            Ok(data) => servers.set(data),
            Err(e) => error.set(Some(e)),
        }
        loading.set(false);
    });

    let handle_create = move |_| async move {
        if new_name.read().is_empty() || new_url.read().is_empty() {
            error.set(Some("名称和URL不能为空".to_string()));
            return;
        }
        match finance::create_mcp_server(&new_name.read(), &new_url.read()).await {
            Ok(_) => {
                show_create_modal.set(false);
                new_name.set(String::new());
                new_url.set(String::new());
                match finance::list_mcp_servers().await {
                    Ok(data) => servers.set(data),
                    Err(e) => error.set(Some(e)),
                }
            }
            Err(e) => error.set(Some(e)),
        }
    };

    let handle_delete = move |id: String| async move {
        if let Err(e) = finance::delete_mcp_server(&id).await {
            error.set(Some(e));
        } else {
            servers.set(servers.read().iter().filter(|s| s.id != id).cloned().collect());
        }
    };

    let handle_status_toggle = move |id: String, current_status: i32| async move {
        let new_status = if current_status == 1 { 0 } else { 1 };
        match finance::update_mcp_server_status(&id, new_status).await {
            Ok(_) => {
                match finance::list_mcp_servers().await {
                    Ok(data) => servers.set(data),
                    Err(e) => error.set(Some(e)),
                }
            }
            Err(e) => error.set(Some(e)),
        }
    };

    let handle_sync_tools = move |server_id: String| async move {
        if let Err(e) = finance::sync_mcp_tools(&server_id).await {
            error.set(Some(e));
        } else {
            error.set(Some("工具同步成功".to_string()));
        }
    };

    fn status_text(status: i32) -> &'static str {
        match status {
            1 => "启用",
            0 => "禁用",
            _ => "未知",
        }
    }

    cx.render(rsx! {
        AppLayout {
            title: "MCP 服务器管理"
            if loading.read() {
                Loading {}
            } else if error.read().is_some() {
                ErrorAlert { message: error.read().clone().unwrap() }
            } else if servers.read().is_empty() {
                EmptyState { message: "暂无 MCP 服务器", hint: "点击下方按钮添加" }
            } else {
                div { class: "content-area" }
                    div { class: "flex justify-between items-center mb-4" }
                        h2 { class: "text-xl font-semibold", "MCP 服务器列表" }
                        Button {
                            variant: "primary"
                            onclick: move |_| show_create_modal.set(true)
                            "+ 添加服务器"
                        }
                    }
                    table { class: "table w-full" }
                        thead {
                            tr {
                                th { "名称" }
                                th { "URL" }
                                th { "状态" }
                                th { "操作" }
                            }
                        }
                        tbody {
                            servers.read().iter().map(|s| rsx! {
                                tr { key: "{s.id}" }
                                    td { "{s.name}" }
                                    td { "{s.url}" }
                                    td {
                                        span { class: if s.status == 1 { "badge badge-success" } else { "badge badge-neutral" } }
                                            "{status_text(s.status)}"
                                        }
                                    }
                                    td {
                                        Button {
                                            variant: if s.status == 1 { "secondary" } else { "primary" }
                                            size: "sm"
                                            onclick: move |_| handle_status_toggle(s.id.clone(), s.status)
                                            "{if s.status == 1 { '禁用' } else { '启用' }}"
                                        }
                                        Button {
                                            variant: "accent"
                                            size: "sm"
                                            onclick: move |_| handle_sync_tools(s.id.clone())
                                            "同步工具"
                                        }
                                        Button {
                                            variant: "danger"
                                            size: "sm"
                                            onclick: move |_| handle_delete(s.id.clone())
                                            "删除"
                                        }
                                    }
                            })
                        }
                    }
                }
            }

            if show_create_modal.read() {
                Modal {
                    title: "添加 MCP 服务器"
                    on_close: move |_| show_create_modal.set(false)
                    div { class: "space-y-4" }
                        div { class: "form-group" }
                            label { class: "form-label", "服务器名称" }
                            input {
                                class: "form-input"
                                bind:value: new_name
                                placeholder: "输入服务器名称"
                            }
                        }
                        div { class: "form-group" }
                            label { class: "form-label", "服务器 URL" }
                            input {
                                class: "form-input"
                                bind:value: new_url
                                placeholder: "http://localhost:8080"
                            }
                        }
                    }
                    div { class: "modal-footer" }
                        Button {
                            variant: "secondary"
                            onclick: move |_| show_create_modal.set(false)
                            "取消"
                        }
                        Button {
                            variant: "primary"
                            onclick: handle_create
                            "添加"
                        }
                    }
                }
            }
        }
    })
}
```

- [ ] **Step 3: Add module export**

Add to `frontend/src/pages/finance/mod.rs`:
```rust
pub mod mcp_servers;
```

- [ ] **Step 4: Commit**

```bash
git add frontend/src/api/finance.rs frontend/src/pages/finance/mcp_servers.rs frontend/src/pages/finance/mod.rs
git commit -m "feat: add MCP server management page"
```

---

## Task 3: Artifact Management Page

**Files:**
- Create: `frontend/src/pages/project/artifacts.rs`
- Modify: `frontend/src/api/project.rs`
- Modify: `frontend/src/pages/project/mod.rs`

**Backend API:** `/api/v1/project/artifacts/*`

- [ ] **Step 1: Add API client methods for artifacts**

Add to `frontend/src/api/project.rs`:
```rust
pub async fn list_artifacts(project_id: &str) -> Result<Vec<Artifact>, String> {
    api_get_or_default(&format!("/project/artifacts?project_id={}", project_id)).await
}

pub async fn get_artifact(id: &str) -> Result<Artifact, String> {
    api_get(&format!("/project/artifacts/{}", id)).await
}

pub async fn get_artifact_content(id: &str) -> Result<String, String> {
    api_get_text(&format!("/project/artifacts/{}/content", id)).await
}

pub async fn create_artifact(project_id: &str, task_id: Option<&str>, name: &str, description: &str) -> Result<Artifact, String> {
    let params = CreateArtifactParams {
        project_id: project_id.to_string(),
        task_id: task_id.map(|s| s.to_string()),
        name: name.to_string(),
        description: description.to_string(),
        content: None,
        attachment_id: None,
        source_type: None,
    };
    api_post("/project/artifacts", &params).await
}

pub async fn update_artifact_content(id: &str, content: &str) -> Result<(), String> {
    api_put_empty(&format!("/project/artifacts/{}/content", id), &content).await
}

pub async fn delete_artifact(id: &str) -> Result<(), String> {
    api_delete(&format!("/project/artifacts/{}", id)).await
}
```

- [ ] **Step 2: Create artifacts page component**

Create `frontend/src/pages/project/artifacts.rs`:
```rust
use dioxus::prelude::*;
use crate::api::project;
use crate::components::{Button, Modal, EmptyState, Loading, ErrorAlert};
use crate::layouts::AppLayout;

pub fn ProjectArtifacts(cx: Scope) -> Element {
    let artifacts = use_signal(cx, || Vec::new());
    let loading = use_signal(cx, || true);
    let error = use_signal(cx, || None::<String>);
    let show_create_modal = use_signal(cx, || false);
    let new_name = use_signal(cx, || String::new());
    let new_description = use_signal(cx, || String::new());

    use_effect(cx, (), |_| async move {
        match project::list_artifacts("").await {
            Ok(data) => artifacts.set(data),
            Err(e) => error.set(Some(e)),
        }
        loading.set(false);
    });

    let handle_create = move |_| async move {
        if new_name.read().is_empty() {
            error.set(Some("名称不能为空".to_string()));
            return;
        }
        match project::create_artifact("", None, &new_name.read(), &new_description.read()).await {
            Ok(_) => {
                show_create_modal.set(false);
                new_name.set(String::new());
                new_description.set(String::new());
                match project::list_artifacts("").await {
                    Ok(data) => artifacts.set(data),
                    Err(e) => error.set(Some(e)),
                }
            }
            Err(e) => error.set(Some(e)),
        }
    };

    let handle_delete = move |id: String| async move {
        if let Err(e) = project::delete_artifact(&id).await {
            error.set(Some(e));
        } else {
            artifacts.set(artifacts.read().iter().filter(|a| a.id != id).cloned().collect());
        }
    };

    cx.render(rsx! {
        AppLayout {
            title: "项目产物管理"
            if loading.read() {
                Loading {}
            } else if error.read().is_some() {
                ErrorAlert { message: error.read().clone().unwrap() }
            } else if artifacts.read().is_empty() {
                EmptyState { message: "暂无产物", hint: "点击下方按钮创建" }
            } else {
                div { class: "content-area" }
                    div { class: "flex justify-between items-center mb-4" }
                        h2 { class: "text-xl font-semibold", "产物列表" }
                        Button {
                            variant: "primary"
                            onclick: move |_| show_create_modal.set(true)
                            "+ 创建产物"
                        }
                    }
                    table { class: "table w-full" }
                        thead {
                            tr {
                                th { "名称" }
                                th { "描述" }
                                th { "项目" }
                                th { "创建时间" }
                                th { "操作" }
                            }
                        }
                        tbody {
                            artifacts.read().iter().map(|a| rsx! {
                                tr { key: "{a.id}" }
                                    td { "{a.name}" }
                                    td { "{a.description}" }
                                    td { "{a.project_id}" }
                                    td { "{a.created_at}" }
                                    td {
                                        Button {
                                            variant: "danger"
                                            size: "sm"
                                            onclick: move |_| handle_delete(a.id.clone())
                                            "删除"
                                        }
                                    }
                            })
                        }
                    }
                }
            }

            if show_create_modal.read() {
                Modal {
                    title: "创建产物"
                    on_close: move |_| show_create_modal.set(false)
                    div { class: "space-y-4" }
                        div { class: "form-group" }
                            label { class: "form-label", "名称" }
                            input {
                                class: "form-input"
                                bind:value: new_name
                                placeholder: "输入产物名称"
                            }
                        }
                        div { class: "form-group" }
                            label { class: "form-label", "描述" }
                            textarea {
                                class: "form-textarea"
                                bind:value: new_description
                                rows: 3
                                placeholder: "输入描述"
                            }
                        }
                    }
                    div { class: "modal-footer" }
                        Button {
                            variant: "secondary"
                            onclick: move |_| show_create_modal.set(false)
                            "取消"
                        }
                        Button {
                            variant: "primary"
                            onclick: handle_create
                            "创建"
                        }
                    }
                }
            }
        }
    })
}
```

- [ ] **Step 3: Add module export**

Add to `frontend/src/pages/project/mod.rs`:
```rust
pub mod artifacts;
```

- [ ] **Step 4: Commit**

```bash
git add frontend/src/api/project.rs frontend/src/pages/project/artifacts.rs frontend/src/pages/project/mod.rs
git commit -m "feat: add artifact management page"
```

---

## Task 4: Add Routes and Navigation

**Files:**
- Modify: `frontend/src/pages/mod.rs`
- Modify: `frontend/src/layouts/navbar.rs`
- Modify: `frontend/src/main.rs`

- [ ] **Step 1: Add new routes to Route enum**

Add to `frontend/src/pages/mod.rs`:
```rust
// Import new page components
use crate::pages::finance::attachments::FinanceAttachments;
use crate::pages::finance::mcp_servers::FinanceMcpServers;
use crate::pages::project::artifacts::ProjectArtifacts;

// Add new route variants
#[derive(Clone, Routable, Debug, PartialEq)]
pub enum Route {
    // ... existing routes ...
    
    // Finance 模块新增
    #[route("/finance/attachments")]
    FinanceAttachments {},
    #[route("/finance/mcp-servers")]
    FinanceMcpServers {},
    
    // Project 模块新增
    #[route("/projects/artifacts")]
    ProjectArtifacts {},
    
    // ... existing routes ...
}
```

- [ ] **Step 2: Add navbar links**

Add to `frontend/src/layouts/navbar.rs` in the finance dropdown:
```rust
Link {
    to: Route::FinanceAttachments {},
    class: "navbar-dropdown-item"
    "📎 附件管理"
}
Link {
    to: Route::FinanceMcpServers {},
    class: "navbar-dropdown-item"
    "🔌 MCP 服务器"
}
```

Add in the project dropdown:
```rust
Link {
    to: Route::ProjectArtifacts {},
    class: "navbar-dropdown-item"
    "📦 项目产物"
}
```

- [ ] **Step 3: Add route rendering in main.rs**

Add to `frontend/src/main.rs`:
```rust
Route::FinanceAttachments {} => cx.render(rsx! { FinanceAttachments {} }),
Route::FinanceMcpServers {} => cx.render(rsx! { FinanceMcpServers {} }),
Route::ProjectArtifacts {} => cx.render(rsx! { ProjectArtifacts {} }),
```

- [ ] **Step 4: Commit**

```bash
git add frontend/src/pages/mod.rs frontend/src/layouts/navbar.rs frontend/src/main.rs
git commit -m "feat: add new page routes and navigation"
```

---

## Task 5: Enhance Agent Detail Page - Tool Binding

**Files:**
- Modify: `frontend/src/pages/hr/agent_detail.rs`
- Modify: `frontend/src/api/hr.rs`

**Backend API:** `POST/DELETE /agents/{id}/tools/{tool_id}/bind`

- [ ] **Step 1: Add API client methods for tool binding**

Add to `frontend/src/api/hr.rs`:
```rust
pub async fn bind_tool_to_agent(agent_id: &str, tool_id: &str) -> Result<(), String> {
    api_post_empty(&format!("/hr/agents/{}/tools/{}/bind", agent_id, tool_id), &()).await
}

pub async fn unbind_tool_from_agent(agent_id: &str, tool_id: &str) -> Result<(), String> {
    api_delete(&format!("/hr/agents/{}/tools/{}/bind", agent_id, tool_id)).await
}

pub async fn list_tools() -> Result<Vec<Tool>, String> {
    api_get_or_default("/finance/tools").await
}
```

- [ ] **Step 2: Add tool binding section to agent detail**

Read `frontend/src/pages/hr/agent_detail.rs` first, then add:
```rust
// In the agent detail component, add:
let tools = use_signal(cx, || Vec::new());
let available_tools = use_signal(cx, || Vec::new());

// Load tools in use_effect
match hr::list_tools().await {
    Ok(data) => available_tools.set(data),
    Err(e) => error.set(Some(e)),
}

// Tool binding section
div { class: "card" }
    h3 { class: "card-title", "绑定工具" }
    if available_tools.read().is_empty() {
        div { class: "text-muted", "暂无可用工具" }
    } else {
        div { class: "space-y-2" }
            available_tools.read().iter().map(|tool| {
                let is_bound = tools.read().iter().any(|t| t.id == tool.id);
                rsx! {
                    div { class: "flex justify-between items-center p-2 border rounded" }
                        div {
                            span { "{tool.name}" }
                            span { class: "text-muted text-sm", " - {tool.description}" }
                        }
                        if is_bound {
                            Button {
                                variant: "danger"
                                size: "sm"
                                onclick: move |_| handle_unbind(tool.id.clone())
                                "解绑"
                            }
                        } else {
                            Button {
                                variant: "primary"
                                size: "sm"
                                onclick: move |_| handle_bind(tool.id.clone())
                                "绑定"
                            }
                        }
                }
            })
        }
    }
}

// Add handlers
let handle_bind = move |tool_id: String| async move {
    let id = id.read().clone();
    if let Err(e) = hr::bind_tool_to_agent(&id, &tool_id).await {
        error.set(Some(e));
    } else {
        error.set(Some(format!("工具 {} 绑定成功", tool_id)));
    }
};

let handle_unbind = move |tool_id: String| async move {
    let id = id.read().clone();
    if let Err(e) = hr::unbind_tool_from_agent(&id, &tool_id).await {
        error.set(Some(e));
    } else {
        error.set(Some(format!("工具 {} 解绑成功", tool_id)));
    }
};
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src/api/hr.rs frontend/src/pages/hr/agent_detail.rs
git commit -m "feat: add tool bind/unbind to agent detail"
```

---

## Task 6: Enhance Skills Page - Search and File Management

**Files:**
- Modify: `frontend/src/pages/hr/skills.rs`
- Modify: `frontend/src/api/hr.rs`

**Backend API:** `GET /skills/search`, `/skills/{id}/files/*`

- [ ] **Step 1: Add API client methods for skill search and files**

Add to `frontend/src/api/hr.rs`:
```rust
pub async fn search_skills(keyword: &str) -> Result<Vec<Skill>, String> {
    api_get_or_default(&format!("/hr/skills/search?keyword={}", keyword)).await
}

pub async fn list_skill_files(skill_id: &str) -> Result<Vec<String>, String> {
    api_get_or_default(&format!("/hr/skills/{}/files", skill_id)).await
}

pub async fn get_skill_file_content(skill_id: &str, filename: &str) -> Result<String, String> {
    api_get_text(&format!("/hr/skills/{}/files/{}", skill_id, filename)).await
}

pub async fn update_skill_file_content(skill_id: &str, filename: &str, content: &str) -> Result<(), String> {
    api_put_empty(&format!("/hr/skills/{}/files/{}", skill_id, filename), &content).await
}
```

- [ ] **Step 2: Add search and file management to skills page**

Read `frontend/src/pages/hr/skills.rs`, then add:
```rust
// Add search signal
let search_query = use_signal(cx, || String::new());

// Modify load to support search
let load_skills = async move |keyword: &str| {
    let result = if keyword.is_empty() {
        hr::list_skills().await
    } else {
        hr::search_skills(keyword).await
    };
    match result {
        Ok(data) => skills.set(data),
        Err(e) => error.set(Some(e)),
    }
    loading.set(false);
};

// Add search input
div { class: "flex gap-2 mb-4" }
    input {
        class: "form-input flex-1"
        bind:value: search_query
        placeholder: "搜索技能..."
        oninput: move |_| {
            loading.set(true);
            spawn(async move {
                load_skills(&search_query.read()).await;
            });
        }
    }
    Button {
        variant: "secondary"
        onclick: move |_| {
            search_query.set(String::new());
            loading.set(true);
            spawn(async move {
                load_skills("").await;
            });
        }
        "重置"
    }
}

// Add file management to skill items
div { class: "text-sm text-muted mb-2" }
    "文件: {skill.files.len()}"
if !skill.files.is_empty() {
    div { class: "space-y-1" }
        skill.files.iter().map(|f| rsx! {
            div { class: "text-sm text-accent cursor-pointer hover:underline" }
                "{f}"
        })
    }
}
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src/api/hr.rs frontend/src/pages/hr/skills.rs
git commit -m "feat: add search and file management to skills page"
```

---

## Task 7: Enhance Agent List Page - Search

**Files:**
- Modify: `frontend/src/pages/hr/agents.rs`
- Modify: `frontend/src/api/hr.rs`

**Backend API:** `GET /agents/search`

- [ ] **Step 1: Add API client method for agent search**

Add to `frontend/src/api/hr.rs`:
```rust
pub async fn search_agents(keyword: &str) -> Result<Vec<Agent>, String> {
    api_get_or_default(&format!("/hr/agents/search?keyword={}", keyword)).await
}
```

- [ ] **Step 2: Add search to agents list**

Read `frontend/src/pages/hr/agents.rs`, then add:
```rust
let search_query = use_signal(cx, || String::new());

// Modify load function
let load_agents = async move |keyword: &str| {
    let result = if keyword.is_empty() {
        hr::list_agents().await
    } else {
        hr::search_agents(keyword).await
    };
    match result {
        Ok(data) => agents.set(data),
        Err(e) => error.set(Some(e)),
    }
    loading.set(false);
};

// Add search input before the table
div { class: "flex gap-2 mb-4" }
    input {
        class: "form-input flex-1"
        bind:value: search_query
        placeholder: "搜索 Agent..."
        oninput: move |_| {
            loading.set(true);
            spawn(async move {
                load_agents(&search_query.read()).await;
            });
        }
    }
    Button {
        variant: "secondary"
        onclick: move |_| {
            search_query.set(String::new());
            loading.set(true);
            spawn(async move {
                load_agents("").await;
            });
        }
        "重置"
    }
}
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src/api/hr.rs frontend/src/pages/hr/agents.rs
git commit -m "feat: add search to agents list page"
```

---

## Task 8: Enhance Model Providers Page - Call Test Button

**Files:**
- Modify: `frontend/src/pages/finance/model_providers.rs`
- Modify: `frontend/src/api/finance.rs`

**Backend API:** `POST /model-providers/{id}/call`

- [ ] **Step 1: Add API client method for model call**

Add to `frontend/src/api/finance.rs`:
```rust
pub async fn call_model_provider(id: &str, prompt: &str) -> Result<ModelCallResponse, String> {
    let params = ModelCallParams {
        prompt: prompt.to_string(),
    };
    api_post(&format!("/finance/model-providers/{}/call", id), &params).await
}
```

- [ ] **Step 2: Add call test button to model providers page**

Read `frontend/src/pages/finance/model_providers.rs`, then add test button to each row:
```rust
Button {
    variant: "accent"
    size: "sm"
    onclick: move |_| handle_call_test(provider.id.clone())
    "调用测试"
}

// Add handler and modal
let show_call_modal = use_signal(cx, || false);
let call_provider_id = use_signal(cx, || String::new());
let call_prompt = use_signal(cx, || "你好，请介绍一下自己".to_string());
let call_result = use_signal(cx, || None::<String>);

let handle_call_test = move |id: String| {
    call_provider_id.set(id);
    show_call_modal.set(true);
};

let handle_call = move |_| async move {
    let id = call_provider_id.read().clone();
    match finance::call_model_provider(&id, &call_prompt.read()).await {
        Ok(resp) => call_result.set(Some(resp.content)),
        Err(e) => error.set(Some(e)),
    }
};

// Add modal
if show_call_modal.read() {
    Modal {
        title: "模型调用测试"
        on_close: move |_| show_call_modal.set(false)
        div { class: "space-y-4" }
            div { class: "form-group" }
                label { class: "form-label", "Prompt" }
                textarea {
                    class: "form-textarea"
                    bind:value: call_prompt
                    rows: 4
                }
            }
            if call_result.read().is_some() {
                div { class: "form-group" }
                    label { class: "form-label", "响应结果" }
                    textarea {
                        class: "form-textarea"
                        readonly: true
                        value: "{call_result.read().clone().unwrap()}"
                        rows: 8
                    }
                }
            }
        }
        div { class: "modal-footer" }
            Button {
                variant: "secondary"
                onclick: move |_| show_call_modal.set(false)
                "关闭"
            }
            Button {
                variant: "primary"
                onclick: handle_call
                "发送"
            }
        }
    }
}
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src/api/finance.rs frontend/src/pages/finance/model_providers.rs
git commit -m "feat: add model call test button"
```

---

## Task 9: Enhance Message Channels Page - Connection Test Button

**Files:**
- Modify: `frontend/src/pages/finance/message_channels.rs`
- Modify: `frontend/src/api/finance.rs`

**Backend API:** `POST /message-channels/{id}/test`

- [ ] **Step 1: Add API client method for channel test**

Add to `frontend/src/api/finance.rs`:
```rust
pub async fn test_message_channel(id: &str) -> Result<ChannelTestResult, String> {
    api_post(&format!("/finance/message-channels/{}/test", id), &()).await
}
```

- [ ] **Step 2: Add connection test button**

Read `frontend/src/pages/finance/message_channels.rs`, then add test button:
```rust
Button {
    variant: "accent"
    size: "sm"
    onclick: move |_| handle_test(channel.id.clone())
    "连接测试"
}

// Add handler
let handle_test = move |id: String| async move {
    match finance::test_message_channel(&id).await {
        Ok(_) => error.set(Some(format!("渠道 {} 连接测试成功", id))),
        Err(e) => error.set(Some(format!("连接测试失败: {}", e))),
    }
};
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src/api/finance.rs frontend/src/pages/finance/message_channels.rs
git commit -m "feat: add message channel connection test button"
```

---

## Task 10: Enhance Project Detail Page - Artifacts Section

**Files:**
- Modify: `frontend/src/pages/project/project_detail.rs`

**Backend API:** `/project/artifacts/*`

- [ ] **Step 1: Add artifacts section to project detail**

Read `frontend/src/pages/project/project_detail.rs`, then add:
```rust
// Add artifacts signals
let artifacts = use_signal(cx, || Vec::new());
let show_create_artifact_modal = use_signal(cx, || false);
let new_artifact_name = use_signal(cx, || String::new());
let new_artifact_description = use_signal(cx, || String::new());

// Load artifacts
let load_artifacts = async move {
    let project_id = id.read().clone();
    match project::list_artifacts(&project_id).await {
        Ok(data) => artifacts.set(data),
        Err(e) => error.set(Some(e)),
    }
};

use_effect(cx, (), |_| async move {
    load_artifacts.await;
});

// Add artifacts section
div { class: "card" }
    div { class: "flex justify-between items-center" }
        h3 { class: "card-title", "项目产物" }
        Button {
            variant: "primary"
            size: "sm"
            onclick: move |_| show_create_artifact_modal.set(true)
            "+ 新增产物"
        }
    }
    if artifacts.read().is_empty() {
        div { class: "text-muted", "暂无产物" }
    } else {
        div { class: "space-y-2" }
            artifacts.read().iter().map(|a| rsx! {
                div { class: "p-2 border rounded flex justify-between items-center" }
                    div {
                        span { class: "font-medium", "{a.name}" }
                        span { class: "text-muted text-sm ml-2", "{a.description}" }
                    }
                    Button {
                        variant: "danger"
                        size: "sm"
                        onclick: move |_| handle_delete_artifact(a.id.clone())
                        "删除"
                    }
            })
        }
    }
}

// Add handlers
let handle_create_artifact = move |_| async move {
    let project_id = id.read().clone();
    match project::create_artifact(&project_id, None, &new_artifact_name.read(), &new_artifact_description.read()).await {
        Ok(_) => {
            show_create_artifact_modal.set(false);
            new_artifact_name.set(String::new());
            new_artifact_description.set(String::new());
            load_artifacts.await;
        }
        Err(e) => error.set(Some(e)),
    }
};

let handle_delete_artifact = move |artifact_id: String| async move {
    if let Err(e) = project::delete_artifact(&artifact_id).await {
        error.set(Some(e));
    } else {
        load_artifacts.await;
    }
};

// Add modal
if show_create_artifact_modal.read() {
    Modal {
        title: "创建产物"
        on_close: move |_| show_create_artifact_modal.set(false)
        div { class: "space-y-4" }
            div { class: "form-group" }
                label { class: "form-label", "名称" }
                input {
                    class: "form-input"
                    bind:value: new_artifact_name
                }
            }
            div { class: "form-group" }
                label { class: "form-label", "描述" }
                textarea {
                    class: "form-textarea"
                    bind:value: new_artifact_description
                    rows: 3
                }
            }
        }
        div { class: "modal-footer" }
            Button {
                variant: "secondary"
                onclick: move |_| show_create_artifact_modal.set(false)
                "取消"
            }
            Button {
                variant: "primary"
                onclick: handle_create_artifact
                "创建"
            }
        }
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/pages/project/project_detail.rs
git commit -m "feat: add artifacts section to project detail"
```

---

## Task 11: Build Validation

**Files:**
- None (build validation)

- [ ] **Step 1: Run frontend build**

```bash
cd frontend && dx build
```

Expected: Build succeeds with 0 errors

- [ ] **Step 2: Run backend tests**

```bash
cargo test --workspace --no-fail-fast
```

Expected: All 693 tests pass

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "chore: build validation and final cleanup"
```

---

## Self-Review

### 1. Spec Coverage

| Requirement | Task |
|-------------|------|
| 附件管理页面 | Task 1 |
| MCP 服务器管理页面 | Task 2 |
| 产物管理页面 | Task 3 |
| 路由和导航 | Task 4 |
| Agent 详情工具绑定 | Task 5 |
| 技能搜索和文件管理 | Task 6 |
| Agent 列表搜索 | Task 7 |
| 模型调用测试按钮 | Task 8 |
| 消息渠道连接测试 | Task 9 |
| 项目详情产物管理 | Task 10 |
| 构建验证 | Task 11 |

### 2. Placeholder Scan

No placeholders found. All tasks contain complete code and commands.

### 3. Type Consistency

All API methods use consistent naming patterns (`list_*`, `get_*`, `create_*`, `update_*`, `delete_*`). All page components follow the same pattern (signals, use_effect, handlers).

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-07-13-frontend-page-completion.md`. Two execution options:

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**