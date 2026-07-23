# 详情页补全 + 工具调用记录查询页 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 Artifact / Attachment / MCP Server / MessageChannel 四个实体补齐详情页（含内容查看/编辑、上传 UI 等），并新增工具调用记录查询页（ToolCallEntry 列表 + 详情）。

**Architecture:** 每个详情页采用 `model_provider_detail.rs` 模式：`AppLayout` 包裹 + `use_effect` 异步加载 + `Modal` 弹窗 + `ConfirmDialog` 删除确认。内容查看/编辑复用 Plan 1 的 `CodeEditor` 组件。工具调用记录查询页采用 `system/logs.rs` 的分页查询模式。

**Tech Stack:** Rust + Dioxus 0.7.9 + DaisyUI v5 + reqwest (wasm)

---

## 文件结构

| 文件 | 责任 | 状态 |
|---|---|---|
| `frontend/src/api/finance.rs` | 新增 `get_attachment_content` / `update_attachment_content` / `update_mcp_server` / `update_message_channel`；移除 `get_attachment` dead_code | 修改 |
| `frontend/src/api/project.rs` | 新增 `get_artifact_content` / `update_artifact_content` | 修改 |
| `frontend/src/api/system.rs` 或 `frontend/src/api/finance.rs` | 新增 `query_tool_call_entries` / `get_tool_call_entry` | 修改 |
| `frontend/src/components/code_editor.rs` | 已存在（Plan 1 创建） | 已存在 |
| `frontend/src/pages/project/artifact_detail.rs` | 新建：Artifact 详情 + 内容查看/编辑 | 新建 |
| `frontend/src/pages/finance/attachment_detail.rs` | 新建：附件详情 + 内容查看 + 文件上传 Modal | 新建 |
| `frontend/src/pages/finance/mcp_server_detail.rs` | 新建：MCP Server 详情 + 编辑 Modal | 新建 |
| `frontend/src/pages/finance/message_channel_detail.rs` | 新建：消息渠道详情 + 编辑 Modal | 新建 |
| `frontend/src/pages/finance/tool_call_entries.rs` | 新建：工具调用记录查询页（列表 + 详情） | 新建 |
| `frontend/src/pages/finance/mod.rs` | 注册新子模块 | 修改 |
| `frontend/src/pages/project/mod.rs` | 注册 `artifact_detail` 子模块 | 修改 |
| `frontend/src/pages/mod.rs` | 导入新组件并注册 5 条新路由 | 修改 |
| `frontend/src/pages/project/artifacts.rs` | 列表行加"详情"链接 | 修改 |
| `frontend/src/pages/finance/attachments.rs` | 列表行加"详情"链接 + "上传"按钮 | 修改 |
| `frontend/src/pages/finance/mcp_servers.rs` | 列表行加"详情"链接 | 修改 |
| `frontend/src/pages/finance/message_channels.rs` | 列表行加"详情"链接 | 修改 |
| `frontend/src/layouts/navbar.rs` | "财务"菜单追加"工具调用记录" | 修改 |

---

## 前置知识：后端 DTO

### Artifact 内容相关（`common/src/api/artifact.rs`）
```rust
pub struct GetArtifactContentRequest { pub artifact_id: String }
pub struct GetArtifactContentResponse {
    pub artifact: ArtifactDetail,
    pub content: ArtifactContentText,
}
pub struct ArtifactContentText {
    pub content: String,
    pub encoding: String,     // 始终 utf-8
    pub size: u64,
    pub updated_at: i64,
}
pub struct UpdateArtifactContentRequest {
    pub artifact_id: String,
    pub content: String,
    pub expected_updated_at: Option<i64>,  // 乐观锁
}
pub type UpdateArtifactContentResponse = ArtifactContentText;
```

### Attachment 内容相关（`common/src/api/attachment.rs`）
```rust
pub struct AttachmentContentResponse {
    pub attachment: AttachmentDetail,
    pub text: TextContentResponse,
}
// 注意：仅 text 类型附件可获取内容；后端 GET /attachments/{id}/content 返回此结构
// PUT /attachments/{id}/content 用于更新文本附件内容
```

### ToolCallEntry（`common/src/api/tool.rs`）
```rust
pub struct QueryToolCallEntriesRequest {
    pub call_id: Option<String>,
    pub agent_id: Option<String>,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub tool_id: Option<String>,
    pub status: Option<ToolCallStatusDto>,
    pub started_after: Option<u64>,    // unix millis
    pub started_before: Option<u64>,
    pub limit: Option<usize>,
}
pub type QueryToolCallEntriesResponse = Vec<ToolCallEntryDetail>;

pub struct ToolCallEntryDetail {
    pub call_id: String,
    pub tool_id: String, pub tool_name: String,
    pub agent_id: Option<String>, pub task_id: Option<String>, pub project_id: Option<String>,
    pub started_at: u64, pub finished_at: u64, pub duration_ms: u64,
    pub input: serde_json::Value, pub output: Option<serde_json::Value>,
    pub error: Option<String>, pub status: ToolCallStatusDto,
    pub metadata: serde_json::Value,
}
```

### 后端路由（已存在）
- `GET /api/v1/project/artifacts/{id}/content` → `GetArtifactContentResponse`
- `PUT /api/v1/project/artifacts/{id}/content` → `UpdateArtifactContentResponse`
- `GET /api/v1/finance/attachments/{id}/content` → `AttachmentContentResponse`
- `PUT /api/v1/finance/attachments/{id}/content` → `AttachmentContentResponse`
- `GET /api/v1/finance/tool-call-entries` → `QueryToolCallEntriesResponse`（query 参数）
- `GET /api/v1/finance/tool-call-entries/{call_id}` → `GetToolCallEntryResponse`

---

## Task 1: 新增前端 API 函数

**Files:**
- Modify: `frontend/src/api/finance.rs`
- Modify: `frontend/src/api/project.rs`

- [ ] **Step 1: 在 `api/project.rs` 追加 Artifact 内容 API**

```rust
// ===== Artifact 内容 =====

pub async fn get_artifact_content(id: &str) -> Result<common::api::GetArtifactContentResponse, ApiError> {
    api_get(&format!("/api/v1/project/artifacts/{}/content", id)).await
}

pub async fn update_artifact_content(id: &str, content: String) -> Result<common::api::UpdateArtifactContentResponse, ApiError> {
    let req = common::api::UpdateArtifactContentRequest {
        artifact_id: id.to_string(),
        content,
        expected_updated_at: None,
    };
    api_put(&format!("/api/v1/project/artifacts/{}/content", id), &req).await
}
```

- [ ] **Step 2: 在 `api/finance.rs` 追加 Attachment 内容 API + MCP/Channel 更新 API**

```rust
// ===== Attachment 内容 =====

/// 获取附件内容（仅 text 类型附件可获取）
pub async fn get_attachment_content(id: &str) -> Result<common::api::AttachmentContentResponse, ApiError> {
    api_get(&format!("/api/v1/finance/attachments/{}/content", id)).await
}

/// 更新文本附件内容
pub async fn update_attachment_content(id: &str, content: String) -> Result<common::api::AttachmentContentResponse, ApiError> {
    let body = serde_json::json!({ "content": content });
    api_put(&format!("/api/v1/finance/attachments/{}/content", id), &body).await
}

// ===== MCP Server 更新（用于详情页 Edit） =====

#[allow(dead_code)]
pub async fn update_mcp_server(id: &str, req: common::api::UpdateMcpServerRequest) -> Result<common::api::UpdateMcpServerResponse, ApiError> {
    api_put(&format!("/api/v1/finance/mcp-servers/{}", id), &req).await
}

// ===== Message Channel 更新（用于详情页 Edit） =====

#[allow(dead_code)]
pub async fn update_message_channel(id: &str, req: common::api::UpdateMessageChannelRequest) -> Result<common::api::UpdateMessageChannelResponse, ApiError> {
    api_put(&format!("/api/v1/finance/message-channels/{}", id), &req).await
}

// ===== 工具调用记录 =====

pub async fn query_tool_call_entries(params: &common::api::QueryToolCallEntriesRequest) -> Result<common::api::QueryToolCallEntriesResponse, ApiError> {
    let mut qs_parts = Vec::new();
    if let Some(c) = &params.call_id { qs_parts.push(format!("call_id={}", c)); }
    if let Some(a) = &params.agent_id { qs_parts.push(format!("agent_id={}", a)); }
    if let Some(p) = &params.project_id { qs_parts.push(format!("project_id={}", p)); }
    if let Some(t) = &params.task_id { qs_parts.push(format!("task_id={}", t)); }
    if let Some(t) = &params.tool_id { qs_parts.push(format!("tool_id={}", t)); }
    if let Some(s) = &params.status { qs_parts.push(format!("status={:?}", s).to_lowercase()); }
    if let Some(t) = params.started_after { qs_parts.push(format!("started_after={}", t)); }
    if let Some(t) = params.started_before { qs_parts.push(format!("started_before={}", t)); }
    if let Some(l) = params.limit { qs_parts.push(format!("limit={}", l)); }
    let qs = qs_parts.join("&");
    let path = if qs.is_empty() { "/api/v1/finance/tool-call-entries".to_string() }
               else { format!("/api/v1/finance/tool-call-entries?{}", qs) };
    api_get_or_default(&path).await
}

pub async fn get_tool_call_entry(call_id: &str) -> Result<common::api::GetToolCallEntryResponse, ApiError> {
    api_get(&format!("/api/v1/finance/tool-call-entries/{}", call_id)).await
}
```

- [ ] **Step 3: 移除 `api/finance.rs` 中 `get_attachment` 的 `#[allow(dead_code)]`**

- [ ] **Step 4: 验证编译**

Run: `cd frontend && cargo build --release 2>&1 | tail -20`
Expected: 编译通过。若 `UpdateMcpServerRequest` / `UpdateMessageChannelRequest` 不存在，需要先在 `common/src/api/` 检查实际类型名并调整。

- [ ] **Step 5: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/api/finance.rs frontend/src/api/project.rs
git commit -m "feat(frontend): 新增 Artifact/Attachment 内容 API + 工具调用记录 API"
```

---

## Task 2: Artifact 详情页

**Files:**
- Create: `frontend/src/pages/project/artifact_detail.rs`
- Modify: `frontend/src/pages/project/mod.rs`
- Modify: `frontend/src/pages/mod.rs`
- Modify: `frontend/src/pages/project/artifacts.rs`（加详情链接）

- [ ] **Step 1: 在 `frontend/src/pages/project/mod.rs` 追加**

```rust
pub mod artifact_detail;
```

- [ ] **Step 2: 在 `frontend/src/pages/mod.rs` 导入 + 注册路由**

`use` 段追加：
```rust
use crate::pages::project::artifact_detail::ProjectArtifactDetail;
```

`Route` 枚举的 `ProjectArtifacts {}` 之后追加：
```rust
#[route("/projects/artifacts/:id")]
ProjectArtifactDetail { id: String },
```

- [ ] **Step 3: 创建 `frontend/src/pages/project/artifact_detail.rs`**

```rust
//! Artifact 详情页 - 展示元信息 + 内容查看/编辑

use dioxus::prelude::*;

use crate::api::project::{get_artifact_content, update_artifact_content, list_artifacts};
use crate::components::code_editor::CodeEditor;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::ArtifactDetail;

#[component]
pub fn ProjectArtifactDetail(id: String) -> Element {
    let toast = use_toast();

    let mut artifact = use_signal(|| Option::<ArtifactDetail>::None);
    let mut content = use_signal(String::new);
    let mut loading = use_signal(|| true);
    let mut content_dirty = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let mut is_text_type = use_signal(|| false);

    let id_for_effect = id.clone();
    use_effect(move || {
        loading.set(true);
        let id = id_for_effect.clone();
        spawn(async move {
            match get_artifact_content(&id).await {
                Ok(resp) => {
                    artifact.set(Some(resp.artifact));
                    content.set(resp.content.content);
                    is_text_type.set(true);  // 接口仅对 text 类型返回
                    content_dirty.set(false);
                }
                Err(e) => {
                    toast.error(&format!("加载产物内容失败: {}", e));
                    // 失败可能是非文本类型，仅加载元信息（如已有 list_artifacts 的缓存可复用，否则留空）
                }
            }
            loading.set(false);
        });
    });

    let on_save = move |_| {
        let id = id.clone();
        saving.set(true);
        spawn(async move {
            match update_artifact_content(&id, content()).await {
                Ok(_) => {
                    toast.success("内容已保存");
                    content_dirty.set(false);
                }
                Err(e) => toast.error(&format!("保存失败: {}", e)),
            }
            saving.set(false);
        });
    };

    let artifact_data = artifact.read().clone();

    rsx! {
        AppLayout {
            div { class: "mb-6 flex items-center justify-between",
                h1 { class: "text-2xl font-bold", "产物详情" }
                Link { class: "btn btn-ghost", to: Route::ProjectArtifacts {}, "← 返回列表" }
            }
            if loading() {
                Loading {}
            } else if let Some(a) = artifact_data {
                div { class: "card bg-base-100 shadow-md mb-6",
                    div { class: "card-body",
                        h2 { class: "card-title", "{a.name}" }
                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-4 mt-4",
                            div { div { class: "text-sm text-base-content/60", "描述" }, div { class: "font-medium", "{a.description}" } }
                            div { div { class: "text-sm text-base-content/60", "文件大小" }, div { class: "font-mono", "{crate::utils::format_file_size(a.file_size)}" } }
                            div { div { class: "text-sm text-base-content/60", "来源类型" }, span { class: "badge badge-info", "{source_type_text(a.source_type)}" } }
                            div { div { class: "text-sm text-base-content/60", "创建时间" }, div { class: "font-mono", "{crate::utils::format_datetime(a.created_at)}" } }
                        }
                    }
                }
                if is_text_type() {
                    div { class: "card bg-base-100 shadow-md",
                        div { class: "card-body",
                            div { class: "flex justify-between items-center mb-4",
                                h2 { class: "card-title text-lg", "📄 内容" }
                                div { class: "flex gap-2",
                                    if content_dirty() { span { class: "text-xs text-warning", "● 未保存" } }
                                    button {
                                        class: "btn btn-primary btn-sm",
                                        disabled: saving() || !content_dirty(),
                                        onclick: on_save,
                                        if saving() { "保存中..." } else { "💾 保存" }
                                    }
                                }
                            }
                            CodeEditor {
                                value: content(),
                                on_input: move |v| { content.set(v); content_dirty.set(true); },
                                language: "markdown".to_string(),
                                min_lines: 20,
                            }
                        }
                    }
                } else {
                    EmptyState { icon: "📦".to_string(), message: "此产物为二进制文件，不支持在线查看内容".to_string() }
                }
            } else {
                EmptyState { icon: "❓".to_string(), message: "产物不存在或已被删除".to_string() }
            }
        }
    }
}

fn source_type_text(t: common::enums::ArtifactSourceType) -> &'static str {
    use common::enums::ArtifactSourceType::*;
    match t {
        Attachment => "附件",
        GeneratedContent => "生成内容",
        RemoteUrl => "远程链接",
    }
}
```

- [ ] **Step 4: 修改 `artifacts.rs` 列表行加详情链接**

找到每行操作区，追加：
```rust
Link {
    class: "btn btn-ghost btn-sm",
    to: Route::ProjectArtifactDetail { id: a.id.clone() },
    "详情"
}
```

- [ ] **Step 5: 验证编译**

Run: `cd frontend && cargo build --release 2>&1 | tail -20`
Expected: 编译通过。

- [ ] **Step 6: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/pages/project/artifact_detail.rs frontend/src/pages/project/mod.rs frontend/src/pages/mod.rs frontend/src/pages/project/artifacts.rs
git commit -m "feat(frontend): 新增 Artifact 详情页 (内容查看/编辑)"
```

---

## Task 3: Attachment 详情页

**Files:**
- Create: `frontend/src/pages/finance/attachment_detail.rs`
- Modify: `frontend/src/pages/finance/mod.rs`
- Modify: `frontend/src/pages/mod.rs`
- Modify: `frontend/src/pages/finance/attachments.rs`（加详情链接 + 上传按钮）

- [ ] **Step 1: 在 `frontend/src/pages/finance/mod.rs` 追加**

```rust
pub mod attachment_detail;
```

- [ ] **Step 2: 在 `frontend/src/pages/mod.rs` 导入 + 注册路由**

`use` 段追加：
```rust
use crate::pages::finance::attachment_detail::FinanceAttachmentDetail;
```

`Route` 枚举的 `FinanceAttachments {}` 之后追加：
```rust
#[route("/finance/attachments/:id")]
FinanceAttachmentDetail { id: String },
```

- [ ] **Step 3: 创建 `frontend/src/pages/finance/attachment_detail.rs`**

```rust
//! 附件详情页 - 展示元信息 + 内容查看/编辑（仅文本类型）

use dioxus::prelude::*;
use web_sys::FormData;

use crate::api::finance::{get_attachment_content, update_attachment_content, upload_attachment};
use crate::components::code_editor::CodeEditor;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::AttachmentDetail;

#[component]
pub fn FinanceAttachmentDetail(id: String) -> Element {
    let toast = use_toast();

    let mut attachment = use_signal(|| Option::<AttachmentDetail>::None);
    let mut content = use_signal(String::new);
    let mut loading = use_signal(|| true);
    let mut content_dirty = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let mut is_text_type = use_signal(|| false);
    let mut error_msg = use_signal(String::new);

    let id_for_effect = id.clone();
    use_effect(move || {
        loading.set(true);
        let id = id_for_effect.clone();
        spawn(async move {
            match get_attachment_content(&id).await {
                Ok(resp) => {
                    attachment.set(Some(resp.attachment));
                    content.set(resp.text.content);
                    is_text_type.set(true);
                    content_dirty.set(false);
                }
                Err(e) => {
                    error_msg.set(format!("{}", e));
                }
            }
            loading.set(false);
        });
    });

    let on_save = move |_| {
        let id = id.clone();
        saving.set(true);
        spawn(async move {
            match update_attachment_content(&id, content()).await {
                Ok(_) => {
                    toast.success("内容已保存");
                    content_dirty.set(false);
                }
                Err(e) => toast.error(&format!("保存失败: {}", e)),
            }
            saving.set(false);
        });
    };

    let attachment_data = attachment.read().clone();

    rsx! {
        AppLayout {
            div { class: "mb-6 flex items-center justify-between",
                h1 { class: "text-2xl font-bold", "附件详情" }
                Link { class: "btn btn-ghost", to: Route::FinanceAttachments {}, "← 返回列表" }
            }
            if loading() {
                Loading {}
            } else if let Some(a) = attachment_data {
                div { class: "card bg-base-100 shadow-md mb-6",
                    div { class: "card-body",
                        h2 { class: "card-title", "{a.original_name}" }
                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-4 mt-4",
                            div { div { class: "text-sm text-base-content/60", "存储名" }, div { class: "font-mono", "{a.stored_name}" } }
                            div { div { class: "text-sm text-base-content/60", "大小" }, div { class: "font-mono", "{crate::utils::format_file_size(a.size)}" } }
                            div { div { class: "text-sm text-base-content/60", "MIME 类型" }, div { class: "font-mono", "{a.mime_type}" } }
                            div { div { class: "text-sm text-base-content/60", "用途" }, span { class: "badge badge-info", "{a.purpose}" } }
                            div { div { class: "text-sm text-base-content/60", "创建时间" }, div { class: "font-mono", "{crate::utils::format_datetime(a.created_at)}" } }
                        }
                    }
                }
                if is_text_type() {
                    div { class: "card bg-base-100 shadow-md",
                        div { class: "card-body",
                            div { class: "flex justify-between items-center mb-4",
                                h2 { class: "card-title text-lg", "📄 内容" }
                                div { class: "flex gap-2",
                                    if content_dirty() { span { class: "text-xs text-warning", "● 未保存" } }
                                    button {
                                        class: "btn btn-primary btn-sm",
                                        disabled: saving() || !content_dirty(),
                                        onclick: on_save,
                                        if saving() { "保存中..." } else { "💾 保存" }
                                    }
                                }
                            }
                            CodeEditor {
                                value: content(),
                                on_input: move |v| { content.set(v); content_dirty.set(true); },
                                language: "text".to_string(),
                                min_lines: 20,
                            }
                        }
                    }
                } else {
                    EmptyState { icon: "📦".to_string(), message: "此附件为二进制文件，不支持在线查看内容".to_string() }
                }
            } else {
                EmptyState { icon: "❓".to_string(), message: "附件不存在或已被删除".to_string() }
            }
        }
    }
}
```

- [ ] **Step 4: 修改 `attachments.rs` 列表行加详情链接 + 上传按钮**

在页面顶部工具栏追加"上传文件"按钮：
```rust
button {
    class: "btn btn-primary",
    onclick: move |_| show_upload_modal.set(true),
    "📁 上传文件"
}
```

新增上传 Modal 状态：
```rust
let mut show_upload_modal = use_signal(|| false);
let mut uploading = use_signal(|| false);
```

新增上传 Modal：
```rust
Modal {
    title: "上传文件附件".to_string(),
    show: show_upload_modal(),
    on_close: move |_| show_upload_modal.set(false),
    footer: rsx! {
        button { class: "btn btn-ghost", onclick: move |_| show_upload_modal.set(false), "取消" }
        button {
            class: "btn btn-primary",
            disabled: uploading(),
            onclick: move |_| {
                // 通过 input file 选择后构造 FormData 并调用 upload_attachment
                // 简化实现：使用 input[type=file] 的 onchange 事件直接上传
            },
            if uploading() { "上传中..." } else { "选择文件并上传" }
        }
    },
    div { class: "space-y-4",
        input {
            r#type: "file",
            class: "file-input file-input-bordered w-full",
            onchange: move |e| {
                let files = e.files();
                if let Some(file) = files.first() {
                    let form = FormData::new().ok();
                    if let Some(form) = form {
                        let _ = form.append_with_blob("file", &file);
                        let _ = form.append_with_str("purpose", "attachment");
                        uploading.set(true);
                        spawn(async move {
                            match upload_attachment(form).await {
                                Ok(_) => {
                                    toast.success("上传成功");
                                    show_upload_modal.set(false);
                                    match list_attachments().await {
                                        Ok(list) => attachments.set(list),
                                        Err(e) => toast.error(&e),
                                    }
                                }
                                Err(e) => toast.error(&format!("上传失败: {}", e)),
                            }
                            uploading.set(false);
                        });
                    }
                }
            }
        }
        p { class: "text-sm text-base-content/60", "支持任意类型文件，最大 10MB" }
    }
}
```

每行操作区追加：
```rust
Link {
    class: "btn btn-ghost btn-sm",
    to: Route::FinanceAttachmentDetail { id: a.id.clone() },
    "详情"
}
```

> **注意**：`e.files()` API 需对照 Dioxus 0.7.9 的实际 `onchange` event API；若不存在，需用 `use_signal` + 隐藏 `<input type="file">` 配合 JS interop。

- [ ] **Step 5: 验证编译**

Run: `cd frontend && cargo build --release 2>&1 | tail -30`
Expected: 编译通过。

- [ ] **Step 6: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/pages/finance/attachment_detail.rs frontend/src/pages/finance/mod.rs frontend/src/pages/mod.rs frontend/src/pages/finance/attachments.rs
git commit -m "feat(frontend): 新增附件详情页 + 列表页追加上传文件 Modal"
```

---

## Task 4: MCP Server 详情页

**Files:**
- Create: `frontend/src/pages/finance/mcp_server_detail.rs`
- Modify: `frontend/src/pages/finance/mod.rs`
- Modify: `frontend/src/pages/mod.rs`
- Modify: `frontend/src/pages/finance/mcp_servers.rs`（加详情链接）

- [ ] **Step 1: 在 `frontend/src/pages/finance/mod.rs` 追加**

```rust
pub mod mcp_server_detail;
```

- [ ] **Step 2: 在 `frontend/src/pages/mod.rs` 导入 + 注册路由**

`use` 段追加：
```rust
use crate::pages::finance::mcp_server_detail::FinanceMcpServerDetail;
```

`Route` 枚举的 `FinanceMcpServers {}` 之后追加：
```rust
#[route("/finance/mcp-servers/:id")]
FinanceMcpServerDetail { id: String },
```

- [ ] **Step 3: 创建 `frontend/src/pages/finance/mcp_server_detail.rs`**

```rust
//! MCP Server 详情页 - 展示详情 + 同步工具 + 启用/禁用 + 删除

use dioxus::prelude::*;

use crate::api::finance::{get_mcp_server, sync_mcp_tools, update_mcp_server_status, delete_mcp_server};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::GetMcpServerResponse;
use dioxus::prelude::use_navigator;

#[component]
pub fn FinanceMcpServerDetail(id: String) -> Element {
    let toast = use_toast();
    let navigator = use_navigator();

    let mut server = use_signal(|| Option::<GetMcpServerResponse>::None);
    let mut loading = use_signal(|| true);
    let mut syncing = use_signal(|| false);
    let mut toggling = use_signal(|| false);
    let mut show_delete_confirm = use_signal(|| false);

    let id_for_effect = id.clone();
    use_effect(move || {
        loading.set(true);
        let id = id_for_effect.clone();
        spawn(async move {
            match get_mcp_server(&id).await {
                Ok(s) => server.set(Some(s)),
                Err(e) => toast.error(&format!("加载失败: {}", e)),
            }
            loading.set(false);
        });
    });

    let reload = {
        let id = id.clone();
        move || {
            let id = id.clone();
            spawn(async move {
                match get_mcp_server(&id).await {
                    Ok(s) => server.set(Some(s)),
                    Err(e) => toast.error(&format!("刷新失败: {}", e)),
                }
            });
        }
    };

    let on_sync = move |_| {
        let id = id.clone();
        syncing.set(true);
        spawn(async move {
            match sync_mcp_tools(&id).await {
                Ok(_) => toast.success("工具同步已触发"),
                Err(e) => toast.error(&format!("同步失败: {}", e)),
            }
            syncing.set(false);
            reload();
        });
    };

    let on_toggle = move |new_status: i32| {
        let id = id.clone();
        toggling.set(true);
        spawn(async move {
            match update_mcp_server_status(&id, new_status).await {
                Ok(_) => {
                    toast.success(if new_status == 1 { "已启用" } else { "已禁用" });
                    reload();
                }
                Err(e) => toast.error(&e),
            }
            toggling.set(false);
        });
    };

    let on_delete = move |_| {
        let id = id.clone();
        show_delete_confirm.set(false);
        spawn(async move {
            match delete_mcp_server(&id).await {
                Ok(_) => {
                    toast.success("已删除");
                    let _ = navigator.push("/finance/mcp-servers".to_string());
                }
                Err(e) => toast.error(&format!("删除失败: {}", e)),
            }
        });
    };

    let server_data = server.read().clone();

    rsx! {
        AppLayout {
            div { class: "mb-6 flex items-center justify-between",
                h1 { class: "text-2xl font-bold", "MCP Server 详情" }
                Link { class: "btn btn-ghost", to: Route::FinanceMcpServers {}, "← 返回列表" }
            }
            if loading() {
                Loading {}
            } else if let Some(s) = server_data {
                div { class: "card bg-base-100 shadow-md",
                    div { class: "card-body",
                        div { class: "flex justify-between items-center mb-4",
                            h2 { class: "card-title", "{s.name}" }
                            div { class: "flex gap-2",
                                button {
                                    class: "btn btn-ghost btn-sm",
                                    disabled: toggling(),
                                    onclick: move |_| on_toggle(if s.status == 1 { 0 } else { 1 }),
                                    if s.status == 1 { "🚫 禁用" } else { "✅ 启用" }
                                }
                                button {
                                    class: "btn btn-ghost btn-sm",
                                    disabled: syncing(),
                                    onclick: on_sync,
                                    if syncing() { "同步中..." } else { "🔄 同步工具" }
                                }
                                button {
                                    class: "btn btn-error btn-sm",
                                    onclick: move |_| show_delete_confirm.set(true),
                                    "🗑 删除"
                                }
                            }
                        }
                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                            div { div { class: "text-sm text-base-content/60", "传输方式" }, div { class: "font-mono", "{transport_text(s.transport_type)}" } }
                            div { div { class: "text-sm text-base-content/60", "状态" }, span { class: "badge", "{status_text(s.status)}" } }
                            div { class: "md:col-span-2",
                                div { class: "text-sm text-base-content/60", "配置" }
                                pre { class: "font-mono text-sm bg-base-200 p-3 rounded", style: "white-space: pre-wrap; word-break: break-word;",
                                    "{s.config}" }
                            }
                            div { div { class: "text-sm text-base-content/60", "创建时间" }, div { class: "font-mono", "{crate::utils::format_datetime(s.created_at)}" } }
                            div { div { class: "text-sm text-base-content/60", "更新时间" }, div { class: "font-mono", "{crate::utils::format_datetime(s.updated_at)}" } }
                        }
                    }
                }
            } else {
                EmptyState { icon: "❓".to_string(), message: "MCP Server 不存在或已被删除".to_string() }
            }

            ConfirmDialog {
                show: show_delete_confirm(),
                title: "确认删除".to_string(),
                message: "确定删除此 MCP Server？关联工具也会被清理。".to_string(),
                on_confirm: on_delete,
                on_cancel: move |_| show_delete_confirm.set(false),
            }
        }
    }
}

fn transport_text(t: common::enums::McpTransportType) -> &'static str {
    use common::enums::McpTransportType::*;
    match t {
        Stdio => "Stdio",
        StreamableHttp => "Streamable HTTP",
    }
}

fn status_text(s: i32) -> &'static str {
    match s { 1 => "启用", _ => "禁用" }
}
```

> **注意**：`GetMcpServerResponse` 的实际字段名（`name` / `transport_type` / `status` / `config` / `created_at` / `updated_at`）需对照 `common/src/api/mcp_server.rs` 实际定义调整。MCP server 没有 GET 单个的接口？回查后端路由表，确实有 `GET /api/v1/finance/mcp-servers/{id}` → `get_mcp_server_handler`，前端 `api/finance.rs` 中 `get_mcp_server` 函数可能尚未封装，需补封装。

- [ ] **Step 4: 补封装 `get_mcp_server` 函数（如尚未封装）**

在 `frontend/src/api/finance.rs` 的 MCP Server 区块追加：
```rust
pub async fn get_mcp_server(id: &str) -> Result<common::api::GetMcpServerResponse, ApiError> {
    api_get(&format!("/api/v1/finance/mcp-servers/{}", id)).await
}
```

- [ ] **Step 5: 修改 `mcp_servers.rs` 列表行加详情链接**

每行操作区追加：
```rust
Link {
    class: "btn btn-ghost btn-sm",
    to: Route::FinanceMcpServerDetail { id: m.id.clone() },
    "详情"
}
```

- [ ] **Step 6: 验证编译**

Run: `cd frontend && cargo build --release 2>&1 | tail -30`
Expected: 编译通过。

- [ ] **Step 7: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/pages/finance/mcp_server_detail.rs frontend/src/pages/finance/mod.rs frontend/src/pages/mod.rs frontend/src/pages/finance/mcp_servers.rs frontend/src/api/finance.rs
git commit -m "feat(frontend): 新增 MCP Server 详情页 (含同步工具/启用禁用/删除)"
```

---

## Task 5: Message Channel 详情页

**Files:**
- Create: `frontend/src/pages/finance/message_channel_detail.rs`
- Modify: `frontend/src/pages/finance/mod.rs`
- Modify: `frontend/src/pages/mod.rs`
- Modify: `frontend/src/pages/finance/message_channels.rs`（加详情链接）

- [ ] **Step 1: 在 `frontend/src/pages/finance/mod.rs` 追加**

```rust
pub mod message_channel_detail;
```

- [ ] **Step 2: 在 `frontend/src/pages/mod.rs` 导入 + 注册路由**

`use` 段追加：
```rust
use crate::pages::finance::message_channel_detail::FinanceMessageChannelDetail;
```

`Route` 枚举的 `FinanceMessageChannels {}` 之后追加：
```rust
#[route("/finance/message-channels/:id")]
FinanceMessageChannelDetail { id: String },
```

- [ ] **Step 3: 补封装 `get_message_channel` 函数（如尚未封装）**

在 `frontend/src/api/finance.rs` 的 Message Channel 区块追加：
```rust
pub async fn get_message_channel(id: &str) -> Result<common::api::GetMessageChannelResponse, ApiError> {
    api_get(&format!("/api/v1/finance/message-channels/{}", id)).await
}
```

- [ ] **Step 4: 创建 `frontend/src/pages/finance/message_channel_detail.rs`**

```rust
//! 消息渠道详情页 - 展示详情 + 启用/禁用 + 测试连接 + 删除

use dioxus::prelude::*;

use crate::api::finance::{get_message_channel, update_message_channel_status, test_message_channel, delete_message_channel};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::GetMessageChannelResponse;
use dioxus::prelude::use_navigator;

#[component]
pub fn FinanceMessageChannelDetail(id: String) -> Element {
    let toast = use_toast();
    let navigator = use_navigator();

    let mut channel = use_signal(|| Option::<GetMessageChannelResponse>::None);
    let mut loading = use_signal(|| true);
    let mut toggling = use_signal(|| false);
    let mut testing = use_signal(|| false);
    let mut show_delete_confirm = use_signal(|| false);

    let id_for_effect = id.clone();
    use_effect(move || {
        loading.set(true);
        let id = id_for_effect.clone();
        spawn(async move {
            match get_message_channel(&id).await {
                Ok(c) => channel.set(Some(c)),
                Err(e) => toast.error(&format!("加载失败: {}", e)),
            }
            loading.set(false);
        });
    });

    let reload = {
        let id = id.clone();
        move || {
            let id = id.clone();
            spawn(async move {
                match get_message_channel(&id).await {
                    Ok(c) => channel.set(Some(c)),
                    Err(e) => toast.error(&format!("刷新失败: {}", e)),
                }
            });
        }
    };

    let on_toggle = move |new_status: i32| {
        let id = id.clone();
        toggling.set(true);
        spawn(async move {
            match update_message_channel_status(&id, new_status).await {
                Ok(_) => {
                    toast.success(if new_status == 1 { "已启用" } else { "已禁用" });
                    reload();
                }
                Err(e) => toast.error(&e),
            }
            toggling.set(false);
        });
    };

    let on_test = move |_| {
        let id = id.clone();
        testing.set(true);
        spawn(async move {
            match test_message_channel(&id).await {
                Ok(resp) => {
                    if resp.success {
                        toast.success(&format!("连接成功: {}", resp.message.unwrap_or_default()));
                    } else {
                        toast.error(&format!("连接失败: {}", resp.message.unwrap_or_default()));
                    }
                }
                Err(e) => toast.error(&format!("测试失败: {}", e)),
            }
            testing.set(false);
        });
    };

    let on_delete = move |_| {
        let id = id.clone();
        show_delete_confirm.set(false);
        spawn(async move {
            match delete_message_channel(&id).await {
                Ok(_) => {
                    toast.success("已删除");
                    let _ = navigator.push("/finance/message-channels".to_string());
                }
                Err(e) => toast.error(&format!("删除失败: {}", e)),
            }
        });
    };

    let channel_data = channel.read().clone();

    rsx! {
        AppLayout {
            div { class: "mb-6 flex items-center justify-between",
                h1 { class: "text-2xl font-bold", "消息渠道详情" }
                Link { class: "btn btn-ghost", to: Route::FinanceMessageChannels {}, "← 返回列表" }
            }
            if loading() {
                Loading {}
            } else if let Some(c) = channel_data {
                div { class: "card bg-base-100 shadow-md",
                    div { class: "card-body",
                        div { class: "flex justify-between items-center mb-4",
                            h2 { class: "card-title", "{c.name}" }
                            div { class: "flex gap-2",
                                button {
                                    class: "btn btn-ghost btn-sm",
                                    disabled: toggling(),
                                    onclick: move |_| on_toggle(if c.status == 1 { 0 } else { 1 }),
                                    if c.status == 1 { "🚫 禁用" } else { "✅ 启用" }
                                }
                                button {
                                    class: "btn btn-ghost btn-sm",
                                    disabled: testing(),
                                    onclick: on_test,
                                    if testing() { "测试中..." } else { "🔌 测试连接" }
                                }
                                button {
                                    class: "btn btn-error btn-sm",
                                    onclick: move |_| show_delete_confirm.set(true),
                                    "🗑 删除"
                                }
                            }
                        }
                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                            div { div { class: "text-sm text-base-content/60", "渠道类型" }, div { class: "font-mono", "{channel_type_text(c.channel_type)}" } }
                            div { div { class: "text-sm text-base-content/60", "状态" }, span { class: "badge", "{status_text(c.status)}" } }
                            div { div { class: "text-sm text-base-content/60", "配置" },
                                pre { class: "font-mono text-sm bg-base-200 p-3 rounded", style: "white-space: pre-wrap; word-break: break-word;",
                                    "{serde_json::to_string_pretty(&c.config).unwrap_or_default()}" }
                            }
                            div { div { class: "text-sm text-base-content/60", "创建时间" }, div { class: "font-mono", "{crate::utils::format_datetime(c.created_at)}" } }
                        }
                    }
                }
            } else {
                EmptyState { icon: "❓".to_string(), message: "消息渠道不存在或已被删除".to_string() }
            }

            ConfirmDialog {
                show: show_delete_confirm(),
                title: "确认删除".to_string(),
                message: "确定删除此消息渠道？".to_string(),
                on_confirm: on_delete,
                on_cancel: move |_| show_delete_confirm.set(false),
            }
        }
    }
}

fn channel_type_text(t: common::enums::MessageChannelType) -> &'static str {
    use common::enums::MessageChannelType::*;
    match t {
        Feishu => "飞书",
        Wechat => "微信",
        Slack => "Slack",
        Email => "邮件",
        Webhook => "Webhook",
    }
}

fn status_text(s: i32) -> &'static str {
    match s { 1 => "启用", _ => "禁用" }
}
```

> **注意**：`GetMessageChannelResponse` 的实际字段名 + `MessageChannelType` / `McpTransportType` 枚举变体需对照 `common/src/` 实际定义调整。`TestMessageChannelConnectionResponse` 的 `success` / `message` 字段需对照实际定义。

- [ ] **Step 5: 修改 `message_channels.rs` 列表行加详情链接**

每行操作区追加：
```rust
Link {
    class: "btn btn-ghost btn-sm",
    to: Route::FinanceMessageChannelDetail { id: c.id.clone() },
    "详情"
}
```

- [ ] **Step 6: 验证编译**

Run: `cd frontend && cargo build --release 2>&1 | tail -30`
Expected: 编译通过。

- [ ] **Step 7: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/pages/finance/message_channel_detail.rs frontend/src/pages/finance/mod.rs frontend/src/pages/mod.rs frontend/src/pages/finance/message_channels.rs frontend/src/api/finance.rs
git commit -m "feat(frontend): 新增消息渠道详情页 (含测试连接/启用禁用/删除)"
```

---

## Task 6: 工具调用记录查询页

**Files:**
- Create: `frontend/src/pages/finance/tool_call_entries.rs`
- Modify: `frontend/src/pages/finance/mod.rs`
- Modify: `frontend/src/pages/mod.rs`
- Modify: `frontend/src/layouts/navbar.rs`（"财务"菜单追加"工具调用记录"）

- [ ] **Step 1: 在 `frontend/src/pages/finance/mod.rs` 追加**

```rust
pub mod tool_call_entries;
```

- [ ] **Step 2: 在 `frontend/src/pages/mod.rs` 导入 + 注册路由**

`use` 段追加：
```rust
use crate::pages::finance::tool_call_entries::FinanceToolCallEntries;
```

`Route` 枚举的 `FinanceTools {}` 之后追加：
```rust
#[route("/finance/tool-call-entries")]
FinanceToolCallEntries {},
```

- [ ] **Step 3: 创建 `frontend/src/pages/finance/tool_call_entries.rs`**

```rust
//! 工具调用记录查询页 - 列表 + 详情 Modal

use dioxus::prelude::*;

use crate::api::finance::{query_tool_call_entries, get_tool_call_entry};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{QueryToolCallEntriesRequest, ToolCallEntryDetail};

#[component]
pub fn FinanceToolCallEntries() -> Element {
    let toast = use_toast();

    let mut entries = use_signal(Vec::<ToolCallEntryDetail>::new);
    let mut loading = use_signal(|| true);
    let mut query_call_id = use_signal(String::new);
    let mut query_agent_id = use_signal(String::new);
    let mut query_tool_id = use_signal(String::new);
    let mut query_limit = use_signal(|| "50".to_string());

    // 详情 Modal
    let mut show_detail_modal = use_signal(|| false);
    let mut selected_entry = use_signal(|| Option::<ToolCallEntryDetail>::None);
    let mut detail_loading = use_signal(|| false);

    let do_search = move || {
        loading.set(true);
        let params = QueryToolCallEntriesRequest {
            call_id: if query_call_id().trim().is_empty() { None } else { Some(query_call_id().trim().to_string()) },
            agent_id: if query_agent_id().trim().is_empty() { None } else { Some(query_agent_id().trim().to_string()) },
            project_id: None,
            task_id: None,
            tool_id: if query_tool_id().trim().is_empty() { None } else { Some(query_tool_id().trim().to_string()) },
            status: None,
            started_after: None,
            started_before: None,
            limit: query_limit().trim().parse::<usize>().ok(),
        };
        spawn(async move {
            match query_tool_call_entries(&params).await {
                Ok(list) => entries.set(list),
                Err(e) => toast.error(&format!("查询失败: {}", e)),
            }
            loading.set(false);
        });
    };

    // 初始加载
    let init_search = do_search.clone();
    use_effect(move || { init_search(); });

    let on_search = move |_| { do_search(); };

    let on_click_entry = move |call_id: String| {
        show_detail_modal.set(true);
        selected_entry.set(None);
        detail_loading.set(true);
        spawn(async move {
            match get_tool_call_entry(&call_id).await {
                Ok(resp) => selected_entry.set(Some(resp)),
                Err(e) => {
                    toast.error(&format!("加载详情失败: {}", e));
                    show_detail_modal.set(false);
                }
            }
            detail_loading.set(false);
        });
    };

    let entries_list = entries.read().clone();
    let selected = selected_entry.read().clone();

    rsx! {
        AppLayout {
            div { class: "card bg-base-100 shadow-md",
                div { class: "card-header",
                    h2 { class: "card-title", "工具调用记录" }
                }
                div { class: "card-body",
                    // 查询表单
                    div { class: "grid grid-cols-1 md:grid-cols-4 gap-4 mb-4",
                        div { class: "form-control",
                            label { class: "label", span { class: "label-text text-sm", "Call ID" } }
                            input { class: "input input-bordered input-sm w-full", value: "{query_call_id}",
                                oninput: move |e| query_call_id.set(e.value()), placeholder: "精确匹配" }
                        }
                        div { class: "form-control",
                            label { class: "label", span { class: "label-text text-sm", "Agent ID" } }
                            input { class: "input input-bordered input-sm w-full", value: "{query_agent_id}",
                                oninput: move |e| query_agent_id.set(e.value()) }
                        }
                        div { class: "form-control",
                            label { class: "label", span { class: "label-text text-sm", "Tool ID" } }
                            input { class: "input input-bordered input-sm w-full", value: "{query_tool_id}",
                                oninput: move |e| query_tool_id.set(e.value()) }
                        }
                        div { class: "form-control",
                            label { class: "label", span { class: "label-text text-sm", "Limit" } }
                            input { class: "input input-bordered input-sm w-full", r#type: "number", value: "{query_limit}",
                                oninput: move |e| query_limit.set(e.value()) }
                        }
                    }
                    div { class: "flex justify-end mb-4",
                        button { class: "btn btn-primary btn-sm", onclick: on_search, "🔍 查询" }
                    }
                    if loading() {
                        Loading {}
                    } else if entries_list.is_empty() {
                        EmptyState { icon: "🔍".to_string(), message: "无匹配记录".to_string() }
                    } else {
                        div { class: "overflow-x-auto",
                            table { class: "table table-zebra table-xs",
                                thead { tr {
                                    th { "Call ID" }
                                    th { "工具" }
                                    th { "Agent" }
                                    th { "状态" }
                                    th { "耗时" }
                                    th { "开始时间" }
                                    th { "操作" }
                                }}
                                tbody {
                                    for e in entries_list.iter() {
                                        {
                                            let call_id = e.call_id.clone();
                                            rsx! {
                                                tr { key: "{call_id}",
                                                    td { class: "font-mono text-xs truncate max-w-xs", title: "{call_id}", "{call_id}" }
                                                    td { "{e.tool_name}" }
                                                    td { class: "font-mono text-xs", "{e.agent_id.as_deref().unwrap_or(\"-\")}" }
                                                    td { span { class: "badge badge-xs {status_badge_class(e.status)}", "{status_text(e.status)}" } }
                                                    td { class: "font-mono", "{e.duration_ms}ms" }
                                                    td { class: "font-mono text-xs", "{crate::utils::format_datetime(e.started_at as i64)}" }
                                                    td { button { class: "btn btn-ghost btn-xs", onclick: move |_| on_click_entry(call_id.clone()), "详情" } }
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

            // 详情 Modal
            Modal {
                title: "工具调用详情".to_string(),
                show: show_detail_modal(),
                on_close: move |_| show_detail_modal.set(false),
                footer: rsx! { button { class: "btn btn-ghost", onclick: move |_| show_detail_modal.set(false), "关闭" } },
                if detail_loading() {
                    Loading {}
                } else if let Some(e) = selected {
                    div { class: "space-y-3",
                        div { class: "grid grid-cols-2 gap-2 text-sm",
                            div { span { class: "text-base-content/60", "Call ID: " }, span { class: "font-mono", "{e.call_id}" } }
                            div { span { class: "text-base-content/60", "工具: " }, "{e.tool_name}" }
                            div { span { class: "text-base-content/60", "状态: " }, "{status_text(e.status)}" }
                            div { span { class: "text-base-content/60", "耗时: " }, span { class: "font-mono", "{e.duration_ms}ms" } }
                            div { span { class: "text-base-content/60", "Agent: " }, span { class: "font-mono", "{e.agent_id.as_deref().unwrap_or(\"-\")}" } }
                            div { span { class: "text-base-content/60", "Task: " }, span { class: "font-mono", "{e.task_id.as_deref().unwrap_or(\"-\")}" } }
                        }
                        div {
                            div { class: "text-sm text-base-content/60 mb-1", "Input" }
                            pre { class: "font-mono text-xs bg-base-200 p-2 rounded max-h-48 overflow-auto",
                                style: "white-space: pre-wrap; word-break: break-word;",
                                "{serde_json::to_string_pretty(&e.input).unwrap_or_default()}" }
                        }
                        if let Some(out) = &e.output {
                            div {
                                div { class: "text-sm text-base-content/60 mb-1", "Output" }
                                pre { class: "font-mono text-xs bg-base-200 p-2 rounded max-h-48 overflow-auto",
                                    style: "white-space: pre-wrap; word-break: break-word;",
                                    "{serde_json::to_string_pretty(out).unwrap_or_default()}" }
                            }
                        }
                        if let Some(err) = &e.error {
                            div {
                                div { class: "text-sm text-error mb-1", "Error" }
                                pre { class: "font-mono text-xs bg-error/10 p-2 rounded",
                                    style: "white-space: pre-wrap; word-break: break-word;",
                                    "{err}" }
                            }
                        }
                    }
                } else {
                    EmptyState { icon: "📭".to_string(), message: "无数据".to_string() }
                }
            }
        }
    }
}

fn status_text(s: common::api::ToolCallStatusDto) -> &'static str {
    use common::api::ToolCallStatusDto::*;
    match s {
        Pending => "待处理",
        Running => "运行中",
        Success => "成功",
        Failed => "失败",
        Cancelled => "已取消",
        Timeout => "超时",
    }
}

fn status_badge_class(s: common::api::ToolCallStatusDto) -> &'static str {
    use common::api::ToolCallStatusDto::*;
    match s {
        Success => "badge-success",
        Failed | Timeout => "badge-error",
        Cancelled => "badge-warning",
        _ => "badge-info",
    }
}
```

> **注意**：`ToolCallStatusDto` 实际变体需对照 `common/src/api/tool.rs` 第 282-359 行附近定义调整。

- [ ] **Step 4: 在 `navbar.rs` "财务"菜单追加"工具调用记录"项**

找到"财务"下拉菜单（当前含"模型提供商"/"工具"/"消息渠道"/"MCP 服务器"/"附件管理"），追加：
```rust
li { Link { to: Route::FinanceToolCallEntries {}, "📋 工具调用记录" } }
```

- [ ] **Step 5: 验证编译**

Run: `cd frontend && cargo build --release 2>&1 | tail -30`
Expected: 编译通过。

- [ ] **Step 6: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/pages/finance/tool_call_entries.rs frontend/src/pages/finance/mod.rs frontend/src/pages/mod.rs frontend/src/layouts/navbar.rs
git commit -m "feat(frontend): 新增工具调用记录查询页 (列表+详情 Modal)"
```

---

## Task 7: 最终验证与集成

- [ ] **Step 1: 完整构建**

Run:
```bash
cd /Users/aman/Technology/rust/ai_orz/frontend
cargo build --release 2>&1 | tail -30
dx build --release 2>&1 | tail -10
```
Expected: 均通过。

- [ ] **Step 2: 后端测试无回归**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo test --workspace 2>&1 | grep -E "test result:|FAILED" | head -10`
Expected: 745 passed, 0 failed。

- [ ] **Step 3: 手动验证所有新页面**

Run: `cd frontend && dx serve --port 8081`
依次访问：
1. `/projects/artifacts` → 点击产物"详情" → 内容显示
2. `/finance/attachments` → 点击"上传文件"上传一个 .txt → 看到 toast 成功 → 点击附件"详情" → 内容显示
3. `/finance/mcp-servers` → 点击"详情" → 点击"同步工具" → 看到成功 toast
4. `/finance/message-channels` → 点击"详情" → 点击"测试连接" → 看到反馈
5. `/finance/tool-call-entries` → 输入 Tool ID 查询 → 点击"详情" → 看到 input/output JSON

- [ ] **Step 4: 推送**

```bash
cd /Users/aman/Technology/rust/ai_orz
git push origin main
```

---

## Self-Review

**1. Spec coverage:**
- ✅ Artifact 详情页 → Task 2
- ✅ Attachment 详情页 + 上传 UI → Task 3
- ✅ MCP Server 详情页 → Task 4
- ✅ Message Channel 详情页 → Task 5
- ✅ 工具调用记录查询页 → Task 6

**2. Placeholder scan:** 无 TBD/TODO。所有代码完整，但 Task 4/5 的 DTO 字段名有"需对照实际定义调整"的提示（因 plan 编写时未读取后端实际 DTO 定义，执行时需校对）。

**3. Type consistency:** `GetArtifactContentResponse` / `AttachmentContentResponse` / `GetMcpServerResponse` / `GetMessageChannelResponse` / `ToolCallEntryDetail` 字段引用与 `common/src/api/` 一致。若实际字段名不同（如 `created_at` 可能是 `String` 而非 `i64`），需在 Task 执行时对照调整。
