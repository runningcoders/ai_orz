# Artifact 编辑功能实施 Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让前端能完整编辑 artifact 的元信息（name/description/tags）和内容，Task/Project 详情页用 `with_artifacts` 合并返回展示产物列表并提供查看/编辑入口。

**Architecture:** 将 `update_artifact_content` 重命名为 `update_artifact`，content 改 `Option<String>`，新增 name/description/tags 字段（部分更新），路由从 `PUT /artifacts/{id}/content` 改为 `PUT /artifacts/{id}`。前端补 `get_artifact`，`get_project`/`get_task` 加 `with_artifacts`，新增 ArtifactMetaModal 组件。

**Tech Stack:** Rust (axum, sqlx), Dioxus (Rust Web 框架), Tailwind CSS + DaisyUI

---

## File Structure

### 新建文件
- `frontend/src/components/artifact_meta_modal.rs` — 元信息编辑 Modal 组件

### 修改文件
- `common/src/api/artifact.rs` — `UpdateArtifactContentRequest` 重命名为 `UpdateArtifactRequest`，扩展字段
- `src/service/domain/project/mod.rs` — ArtifactManage trait `update_artifact_content` 重命名为 `update_artifact`
- `src/service/domain/project/artifact.rs` — `update_artifact_content` 实现重命名为 `update_artifact`
- `src/handlers/project/artifact/update_artifact_content.rs` → 重命名为 `update_artifact.rs`
- `src/handlers/project/artifact/mod.rs` — 更新模块声明
- `src/router.rs` — 路由从 `/artifacts/{id}/content` 改为 `/artifacts/{id}`
- `frontend/src/api/project.rs` — `update_artifact_content` 重命名为 `update_artifact` + 路径更新 + 补 `get_artifact`；`get_project`/`get_task` 加 `with_artifacts`
- `frontend/src/components/mod.rs` — 注册 `artifact_meta_modal` 模块
- `frontend/src/pages/project/artifact_detail.rs` — 加元信息编辑 Modal
- `frontend/src/pages/project/task_detail.rs` — 加产物 Tab（用合并返回数据）
- `frontend/src/pages/project/project_detail.rs` — 改用合并返回 + 加查看入口

---

## Task 1: 后端 - 重命名 update_artifact_content 为 update_artifact 并扩展

**Files:**
- Modify: `common/src/api/artifact.rs` (第 195-206 行)
- Modify: `src/service/domain/project/mod.rs` (第 417-425 行)
- Modify: `src/service/domain/project/artifact.rs` (第 273-320 行)
- Rename: `src/handlers/project/artifact/update_artifact_content.rs` → `update_artifact.rs`
- Modify: `src/handlers/project/artifact/mod.rs`
- Modify: `src/router.rs` (第 218-227 行)

- [ ] **Step 1: 重命名 DTO 并扩展字段**

将 `common/src/api/artifact.rs` 第 195-206 行的 `UpdateArtifactContentRequest`：

```rust
/// Update artifact content request (full replace).
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateArtifactContentRequest {
    /// Artifact ID.
    #[param(source = "path")]
    pub artifact_id: String,
    /// Full new content for replacement.
    pub content: String,
    /// Optional optimistic locking: expect current updated_at matches this value.
    /// If mismatch, returns 409 Conflict.
    pub expected_updated_at: Option<i64>,
}
```

替换为（重命名为 `UpdateArtifactRequest`，content 改 Option，加 name/description/tags）：

```rust
/// Update artifact request (partial update).
///
/// Supports updating content and/or metadata in a single call.
/// Only fields that are `Some` will be updated. `None` fields are left unchanged.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateArtifactRequest {
    /// Artifact ID.
    #[param(source = "path")]
    pub artifact_id: String,
    /// New content for replacement. `None` to keep current content unchanged.
    /// Only applicable to GeneratedContent artifacts.
    pub content: Option<String>,
    /// New name. `None` to keep current.
    pub name: Option<String>,
    /// New description. `None` to keep current.
    pub description: Option<String>,
    /// New tags. `None` to keep current tags.
    pub tags: Option<Vec<String>>,
    /// Optional optimistic locking: expect current updated_at matches this value.
    /// If mismatch, returns 409 Conflict.
    pub expected_updated_at: Option<i64>,
}
```

- [ ] **Step 2: 重命名 ArtifactManage trait 方法并扩展签名**

将 `src/service/domain/project/mod.rs` 第 417-425 行：

```rust
    /// Update artifact content (full replace, only for generated-content artifacts).
    /// Returns the updated artifact.
    async fn update_artifact_content(
        &self,
        ctx: RequestContext,
        id: &str,
        content: Vec<u8>,
        expected_updated_at: Option<i64>,
    ) -> Result<Artifact>;
```

替换为（重命名为 `update_artifact`，扩展参数）：

```rust
    /// Update artifact content and/or metadata (partial update).
    ///
    /// Only fields that are `Some` will be updated. Content update only applies
    /// to GeneratedContent artifacts. Metadata (name/description/tags) applies to all.
    async fn update_artifact(
        &self,
        ctx: RequestContext,
        id: &str,
        content: Option<Vec<u8>>,
        name: Option<String>,
        description: Option<String>,
        tags: Option<Vec<String>>,
        expected_updated_at: Option<i64>,
    ) -> Result<Artifact>;
```

- [ ] **Step 3: 重命名 Domain 实现并扩展**

将 `src/service/domain/project/artifact.rs` 第 273-320 行的 `update_artifact_content` 方法替换为 `update_artifact`：

```rust
    /// Update artifact content and/or metadata (partial update).
    async fn update_artifact(
        &self,
        ctx: RequestContext,
        id: &str,
        content: Option<Vec<u8>>,
        name: Option<String>,
        description: Option<String>,
        tags: Option<Vec<String>>,
        expected_updated_at: Option<i64>,
    ) -> Result<Artifact> {
        let Some(mut artifact) = self.artifact_dal.find_by_id(ctx.clone(), id).await? else {
            bail_err!(NotFound, "Artifact not found: {}", id);
        };
        let ctx = enrich_ctx!(&ctx, &artifact);
        // Validate user has access to this artifact via project ownership
        self.validate_project_access(ctx.clone(), &artifact.po.project_id)
            .await?;

        // Optimistic locking check (applies to all updates)
        if let Some(expected) = expected_updated_at
            && artifact.po.updated_at != expected
        {
            bail_err!(
                Conflict,
                "Conflict: expected updated_at = {}, current updated_at = {}. Please reload and try again.",
                expected,
                artifact.po.updated_at
            );
        }

        // Update content if provided (only for GeneratedContent artifacts)
        if let Some(content_bytes) = content {
            if artifact.po.source_type != common::enums::ArtifactSourceType::GeneratedContent {
                bail_err!(
                    InvalidRequest,
                    "Cannot update content directly for artifact source type {:?}, only GeneratedContent artifacts support direct content update.",
                    artifact.po.source_type
                );
            }
            self.artifact_dal
                .write_content(ctx.clone(), &artifact, &content_bytes)
                .await?;
            artifact.po.file_meta.0.file_size = content_bytes.len() as u64;
        }

        // Update metadata if provided
        if let Some(new_name) = name {
            if new_name.trim().is_empty() {
                bail_err!(InvalidRequest, "name不能为空");
            }
            artifact.po.name = new_name;
        }
        if let Some(new_desc) = description {
            artifact.po.description = new_desc;
        }
        if let Some(new_tags) = tags {
            artifact.po.set_tags(new_tags, &ctx.uid());
        }

        // Update timestamp and modifier
        let now = common::constants::utils::current_timestamp_ms();
        artifact.po.updated_at = now;
        artifact.po.modified_by = ctx.uid();

        self.artifact_dal.update(ctx, &artifact).await?;
        Ok(artifact)
    }
```

- [ ] **Step 4: 重命名 handler 文件并扩展逻辑**

将 `src/handlers/project/artifact/update_artifact_content.rs` 重命名为 `update_artifact.rs`，内容替换为：

```rust
//! Handler: PUT /api/v1/project/artifacts/{id} - Update artifact content and/or metadata

use crate::handlers::project::artifact::response;
use crate::pkg::RequestContext;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::artifact::{ArtifactDetail, UpdateArtifactRequest};
use common::error::{Result, bail_err};

/// Update artifact content and/or metadata (partial update).
///
/// Only fields that are `Some` will be updated. Content update only applies
/// to GeneratedContent artifacts. Metadata (name/description/tags) applies to all.
#[register_handler_tool(
    id = "update_artifact",
    name = "update_artifact",
    description = "Update artifact content and/or metadata (name, description, tags). Only provided fields are updated. Supports optimistic locking.",
    params = "common::api::UpdateArtifactRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn update_artifact(
    ctx: RequestContext,
    params: UpdateArtifactRequest,
) -> Result<ArtifactDetail> {
    let domain = crate::service::domain::project::domain();

    // Convert content to Option<Vec<u8>> with size validation
    let content_bytes = if let Some(content) = params.content {
        let bytes = content.into_bytes();
        if bytes.len() > 1024 * 1024 {
            bail_err!(InvalidRequest, "Text content exceeds maximum size of 1MB");
        }
        Some(bytes)
    } else {
        None
    };

    let updated_artifact = domain
        .artifact_manage()
        .update_artifact(
            ctx.clone(),
            &params.artifact_id,
            content_bytes,
            params.name,
            params.description,
            params.tags,
            params.expected_updated_at,
        )
        .await?;

    let detail = response::to_detail(&updated_artifact);
    Ok(detail)
}
```

- [ ] **Step 5: 更新 mod.rs 模块声明**

修改 `src/handlers/project/artifact/mod.rs`：
- 将 `mod update_artifact_content;` 改为 `mod update_artifact;`
- 将 `pub use update_artifact_content::update_artifact_content_handler;` 改为 `pub use update_artifact::update_artifact_handler;`

- [ ] **Step 6: 修改路由**

将 `src/router.rs` 第 218-227 行：

```rust
        .route(
            "/artifacts/{id}",
            get(handlers::project::artifact::get_artifact_handler)
                .delete(handlers::project::artifact::delete_artifact_handler),
        )
        .route(
            "/artifacts/{id}/content",
            get(handlers::project::artifact::get_artifact_content_handler)
                .put(handlers::project::artifact::update_artifact_content_handler),
        )
```

替换为（`update_artifact_content_handler` 改为 `update_artifact_handler`，PUT 从 `/content` 移到根路径）：

```rust
        .route(
            "/artifacts/{id}",
            get(handlers::project::artifact::get_artifact_handler)
                .delete(handlers::project::artifact::delete_artifact_handler)
                .put(handlers::project::artifact::update_artifact_handler),
        )
        .route(
            "/artifacts/{id}/content",
            get(handlers::project::artifact::get_artifact_content_handler),
        )
```

- [ ] **Step 7: 验证编译**

Run: `cargo check -p ai_orz 2>&1 | tail -20`
Expected: 编译通过

- [ ] **Step 8: 提交**

```bash
git add common/src/api/artifact.rs src/service/domain/project/mod.rs src/service/domain/project/artifact.rs src/handlers/project/artifact/update_artifact.rs src/handlers/project/artifact/mod.rs src/router.rs
git commit -m "feat(artifact): rename update_artifact_content to update_artifact, support metadata update

Rename update_artifact_content to update_artifact across DTO, Domain trait,
impl, and handler. Extend to support partial update of content (Option<String>),
name, description, and tags in a single call. Route moved from
PUT /artifacts/{id}/content to PUT /artifacts/{id}. Content update only applies
to GeneratedContent artifacts; metadata update applies to all."
```

**注意**：如果 git 不允许直接 rename，可以先 `git mv update_artifact_content.rs update_artifact.rs`，再修改内容。

---

## Task 2: 前端 - 重命名 API 方法 + 补 get_artifact + with_artifacts 参数

**Files:**
- Modify: `frontend/src/api/project.rs`

- [ ] **Step 1: 重命名 update_artifact_content 为 update_artifact + 更新路径 + 补 get_artifact**

在 `frontend/src/api/project.rs` 第 136-144 行：

```rust
pub async fn update_artifact_content(
    req: common::api::UpdateArtifactContentRequest,
) -> Result<ArtifactDetail, ApiError> {
    api_put(
        &format!("/api/v1/project/artifacts/{}/content", req.artifact_id),
        &req,
    )
    .await
}
```

替换为（重命名为 `update_artifact`，DTO 改为 `UpdateArtifactRequest`，路径从 `/content` 改为根路径，补 `get_artifact`）：

```rust
pub async fn update_artifact(
    req: common::api::UpdateArtifactRequest,
) -> Result<ArtifactDetail, ApiError> {
    api_put(
        &format!("/api/v1/project/artifacts/{}", req.artifact_id),
        &req,
    )
    .await
}

pub async fn get_artifact(id: &str) -> Result<ArtifactDetail, ApiError> {
    api_get(&format!("/api/v1/project/artifacts/{}", id)).await
}
```

- [ ] **Step 2: get_project 加 with_artifacts 参数**

修改 `frontend/src/api/project.rs` 第 29-44 行的 `get_project` 函数，在 `build_query_string` 数组末尾添加 `with_artifacts`：

```rust
pub async fn get_project(req: GetProjectRequest) -> Result<GetProjectResponse, ApiError> {
    let qs = super::build_query_string(&[
        ("with_stats", req.with_stats.map(|v| v.to_string())),
        (
            "with_model_call_stats",
            req.with_model_call_stats.map(|v| v.to_string()),
        ),
        (
            "stats_time_start",
            req.stats_time_start.map(|v| v.to_string()),
        ),
        ("stats_time_end", req.stats_time_end.map(|v| v.to_string())),
        ("stats_interval", req.stats_interval.clone()),
        ("with_artifacts", req.with_artifacts.map(|v| v.to_string())),
    ]);
    api_get(&format!("/api/v1/projects/{}{}", req.id, qs)).await
}
```

- [ ] **Step 3: get_task 加 with_artifacts 参数**

修改 `frontend/src/api/project.rs` 第 74-89 行的 `get_task` 函数，同样在 query string 数组末尾添加 `with_artifacts`：

```rust
pub async fn get_task(req: GetTaskRequest) -> Result<GetTaskResponse, ApiError> {
    let qs = super::build_query_string(&[
        ("with_stats", req.with_stats.map(|v| v.to_string())),
        (
            "with_model_call_stats",
            req.with_model_call_stats.map(|v| v.to_string()),
        ),
        (
            "stats_time_start",
            req.stats_time_start.map(|v| v.to_string()),
        ),
        ("stats_time_end", req.stats_time_end.map(|v| v.to_string())),
        ("stats_interval", req.stats_interval.clone()),
        ("with_artifacts", req.with_artifacts.map(|v| v.to_string())),
    ]);
    api_get(&format!("/api/v1/tasks/{}{}", req.id, qs)).await
}
```

- [ ] **Step 4: 验证编译**

Run: `cargo check -p frontend 2>&1 | tail -20`
Expected: 编译通过（会有 artifact_detail.rs 调用旧名 `update_artifact_content` 的编译错误，这是预期的，Task 3 会修复）

- [ ] **Step 5: 提交**

```bash
git add frontend/src/api/project.rs
git commit -m "feat(frontend): rename update_artifact_content to update_artifact; add get_artifact; with_artifacts param"
```

---

## Task 3: 前端 - ArtifactMetaModal 组件 + artifact_detail.rs 编辑入口

**Files:**
- Create: `frontend/src/components/artifact_meta_modal.rs`
- Modify: `frontend/src/components/mod.rs`
- Modify: `frontend/src/pages/project/artifact_detail.rs`

- [ ] **Step 1: 创建 ArtifactMetaModal 组件**

创建 `frontend/src/components/artifact_meta_modal.rs`：

```rust
//! Artifact metadata editing modal

use dioxus::prelude::*;
use common::api::ArtifactDetail;

#[derive(PartialEq, Props)]
pub struct ArtifactMetaModalProps {
    pub artifact: ArtifactDetail,
    pub show: bool,
    pub on_save: EventHandler<(Option<String>, Option<String>, Option<Vec<String>>)>,
    pub on_close: EventHandler<()>,
}

#[component]
pub fn ArtifactMetaModal(props: ArtifactMetaModalProps) -> Element {
    let mut name = use_signal(|| props.artifact.name.clone());
    let mut description = use_signal(|| props.artifact.description.clone());
    let mut tags_text = use_signal(|| props.artifact.tags.join(", "));

    use_effect(move || {
        name.set(props.artifact.name.clone());
        description.set(props.artifact.description.clone());
        tags_text.set(props.artifact.tags.join(", "));
    });

    if !props.show {
        return rsx! {};
    }

    rsx! {
        div {
            class: "modal modal-open",
            onclick: move |_| props.on_close.call(()),
            div {
                class: "modal-box",
                onclick: move |e| e.stop_propagation(),
                h3 { class: "font-bold text-lg mb-4", "编辑产物信息" }
                div { class: "form-control mb-3",
                    label { class: "label", span { class: "label-text", "名称" } }
                    input {
                        class: "input input-bordered w-full",
                        value: name(),
                        oninput: move |e| name.set(e.value()),
                    }
                }
                div { class: "form-control mb-3",
                    label { class: "label", span { class: "label-text", "描述" } }
                    textarea {
                        class: "textarea textarea-bordered w-full",
                        rows: 3,
                        value: description(),
                        oninput: move |e| description.set(e.value()),
                    }
                }
                div { class: "form-control mb-4",
                    label { class: "label", span { class: "label-text", "标签（逗号分隔）" } }
                    input {
                        class: "input input-bordered w-full",
                        value: tags_text(),
                        oninput: move |e| tags_text.set(e.value()),
                    }
                }
                div { class: "modal-action",
                    button {
                        class: "btn btn-ghost",
                        onclick: move |_| props.on_close.call(()),
                        "取消"
                    }
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            let n = if name() != props.artifact.name { Some(name()) } else { None };
                            let d = if description() != props.artifact.description { Some(description()) } else { None };
                            let tags: Vec<String> = tags_text()
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                            let t = if tags != props.artifact.tags { Some(tags) } else { None };
                            props.on_save.call((n, d, t));
                        },
                        "保存"
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 2: 在 components/mod.rs 注册模块**

修改 `frontend/src/components/mod.rs`，添加：

```rust
pub mod artifact_meta_modal;
```

- [ ] **Step 3: 修改 artifact_detail.rs 加编辑入口**

在 `frontend/src/pages/project/artifact_detail.rs` 中：

1. 顶部 import 添加（在第 6 行 use 之后）：

```rust
use crate::components::artifact_meta_modal::ArtifactMetaModal;
```

**注意**：`update_artifact` 已在现有 import 中（因为内容编辑器已在使用，Task 2 已重命名），不需要额外 import。

2. 在信号声明区（第 23 行 `is_text_type` 之后）添加：

```rust
let mut show_meta_modal = use_signal(|| false);
```

3. 在元信息卡片的 `card-body` 内（第 82 行 h2 之后），添加编辑按钮：

```rust
div { class: "card-actions justify-end",
    button {
        class: "btn btn-ghost btn-sm",
        onclick: move |_| show_meta_modal.set(true),
        "✏️ 编辑信息"
    }
}
```

4. 在 `if is_text_type() { ... }` 块之后（第 117 行 `} else {` 之前），添加 Modal：

```rust
ArtifactMetaModal {
    artifact: a.clone(),
    show: show_meta_modal(),
    on_save: move |(name, description, tags)| {
        let id = id.clone();
        spawn(async move {
            match update_artifact(common::api::UpdateArtifactRequest {
                artifact_id: id.clone(),
                content: None,  // 只更新元信息，不更新内容
                name,
                description,
                tags,
                expected_updated_at: None,
            }).await {
                Ok(updated) => {
                    artifact.set(Some(updated));
                    toast.success("产物信息已更新");
                    show_meta_modal.set(false);
                }
                Err(e) => toast.error(&format!("更新失败: {}", e)),
            }
        });
    },
    on_close: move |_| show_meta_modal.set(false),
}
```

**注意**：
- Modal 必须在 `else if let Some(a) = artifact_data { ... }` 块内，确保 `a` 在作用域中
- `id` 变量来自组件参数，在闭包中需要 clone
- `content: None` 表示只更新元信息，不更新内容
- `update_artifact` 已在现有 import 中（Task 2 已重命名）

- [ ] **Step 4: 验证编译**

Run: `cargo check -p frontend 2>&1 | tail -20`
Expected: 编译通过

- [ ] **Step 5: 提交**

```bash
git add frontend/src/components/artifact_meta_modal.rs frontend/src/components/mod.rs frontend/src/pages/project/artifact_detail.rs
git commit -m "feat(frontend): add metadata editing modal to artifact detail page

ArtifactMetaModal supports editing name, description, and tags.
Calls update_artifact_content with content=None for metadata-only update."
```

---

## Task 4: 前端 - Task 详情页加产物 Tab（用合并返回数据）

**Files:**
- Modify: `frontend/src/pages/project/task_detail.rs`

**现有代码结构说明**：
- Tab 用 `match active_tab() { 0 => rsx!{...}, 1 => rsx!{...}, 2 => rsx!{...} }` 模式渲染内容
- Tab class 用 `tab0_class` / `tab1_class` / `tab2_class` 变量
- Tab 按钮在 `div { class: "tabs tabs-boxed mb-6" }` 内
- `GetTaskRequest` 在第 54-59 行构造

- [ ] **Step 1: 修改 GetTaskRequest 加 with_artifacts**

将第 54-59 行：

```rust
let req = GetTaskRequest {
    id: id_clone.clone(),
    with_stats: Some(true),
    with_model_call_stats: Some(true),
    stats_interval: Some("daily".to_string()),
    ..Default::default()
};
```

改为：

```rust
let req = GetTaskRequest {
    id: id_clone.clone(),
    with_stats: Some(true),
    with_model_call_stats: Some(true),
    stats_interval: Some("daily".to_string()),
    with_artifacts: Some(true),
    ..Default::default()
};
```

- [ ] **Step 2: 添加 tab3_class**

在第 347-351 行 `tab2_class` 声明之后，添加：

```rust
    let tab3_class = if active_tab() == 3 {
        "tab tab-lg tab-active"
    } else {
        "tab tab-lg"
    };
```

- [ ] **Step 3: 添加产物 Tab 按钮**

在第 374 行 tab2 按钮之后（`"🕸️ 关系图"` 按钮之后），添加 tab3 按钮：

```rust
                button { class: "{tab3_class}", onclick: move |_| active_tab.set(3), "📦 产物" }
```

- [ ] **Step 4: 在 match active_tab() 中添加产物 Tab 内容**

在 `match active_tab()` 的 `2 => rsx!{...}` 分支之后（关系图内容之后），添加 `3 => rsx!{...}` 分支。

产物数据从 `task` signal 的 `artifacts` 字段获取（合并返回，无需单独 API 调用）：

```rust
                3 => rsx! {
                    // === 产物 ===
                    {
                        let arts: Vec<_> = task.read().as_ref()
                            .and_then(|t| t.artifacts.clone())
                            .unwrap_or_default();
                        if arts.is_empty() {
                            EmptyState { icon: "📦".to_string(), message: "暂无产物".to_string() }
                        } else {
                            div { class: "space-y-3",
                                for art in arts.iter() {
                                    div { class: "card bg-base-100 shadow-sm",
                                        div { class: "card-body p-4",
                                            div { class: "flex justify-between items-start",
                                                div {
                                                    h3 { class: "font-semibold", "{art.name}" }
                                                    if !art.description.is_empty() {
                                                        p { class: "text-sm text-base-content/60 mt-1", "{art.description}" }
                                                    }
                                                }
                                                Link {
                                                    class: "btn btn-ghost btn-sm",
                                                    to: crate::pages::Route::ProjectArtifactDetail { id: art.id.clone() },
                                                    "查看详情 →"
                                                }
                                            }
                                            div { class: "flex gap-2 mt-2 flex-wrap",
                                                span { class: "badge badge-sm", "{format_file_type(art.file_type)}" }
                                                span { class: "badge badge-sm badge-info", "{art.mime_type}" }
                                                span { class: "badge badge-sm", "{crate::utils::format_file_size(art.file_size)}" }
                                                for tag in art.tags.iter() {
                                                    span { class: "badge badge-sm badge-outline", "#{tag}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
```

- [ ] **Step 5: 确认 Link import**

确认 `frontend/src/pages/project/task_detail.rs` 顶部是否有 `use dioxus_router::Link;`。如果没有，在现有 use 语句之后添加。

- [ ] **Step 6: 添加 format_file_type 辅助函数**

在 `frontend/src/pages/project/task_detail.rs` 文件末尾添加：

```rust
fn format_file_type(t: common::enums::FileType) -> &'static str {
    match t {
        common::enums::FileType::Document => "文档",
        common::enums::FileType::Image => "图片",
        common::enums::FileType::Audio => "音频",
        common::enums::FileType::Video => "视频",
        common::enums::FileType::Binary => "二进制",
    }
}
```

- [ ] **Step 7: 验证编译**

Run: `cargo check -p frontend 2>&1 | tail -20`
Expected: 编译通过

- [ ] **Step 8: 提交**

```bash
git add frontend/src/pages/project/task_detail.rs
git commit -m "feat(frontend): add artifacts tab to task detail page

Uses with_artifacts=true merged response (no extra API call).
4th tab shows artifacts with name, description, tags, and detail link."
```

---

## Task 5: 前端 - Project 详情页改用合并返回 + 加查看入口

**Files:**
- Modify: `frontend/src/pages/project/project_detail.rs`

**现有代码结构说明**：
- Tab 编号：0=概览 1=任务列表 2=产物 3=关系图
- `GetProjectRequest` 在第 74-80 行构造
- 第 119-122 行单独调 `list_artifacts` 加载产物
- 产物 Tab（tab=2）在第 584-660 行，用表格渲染
- 产物行用 `for a in artifacts_list.iter()`，变量名是 `a`
- 操作列在第 630-649 行，只有删除按钮

- [ ] **Step 1: 修改 GetProjectRequest 加 with_artifacts**

将第 74-80 行：

```rust
let req = GetProjectRequest {
    id: id_clone.clone(),
    with_stats: Some(true),
    with_model_call_stats: Some(true),
    stats_interval: Some("daily".to_string()),
    ..Default::default()
};
```

改为：

```rust
let req = GetProjectRequest {
    id: id_clone.clone(),
    with_stats: Some(true),
    with_model_call_stats: Some(true),
    stats_interval: Some("daily".to_string()),
    with_artifacts: Some(true),
    ..Default::default()
};
```

- [ ] **Step 2: 从合并返回取 artifacts，移除单独 list_artifacts 调用**

将第 81-84 行的 `get_project` 成功分支：

```rust
match get_project(req).await {
    Ok(p) => project.set(Some(p)),
    Err(e) => toast.error(&e),
}
```

改为：

```rust
match get_project(req).await {
    Ok(p) => {
        // 从合并返回的 with_artifacts 字段取产物列表
        if let Some(ref arts) = p.artifacts {
            artifacts.set(arts.clone());
        }
        project.set(Some(p));
    }
    Err(e) => toast.error(&e),
}
```

然后移除第 119-122 行的单独 `list_artifacts` 调用：

```rust
// 移除以下代码：
// match list_artifacts(&id_clone).await {
//     Ok(list) => artifacts.set(list),
//     Err(e) => toast.error(&e),
// }
```

- [ ] **Step 3: 修改删除后刷新逻辑**

在产物 Tab 的删除按钮 onclick 中（第 636-646 行），删除成功后当前调 `list_artifacts` 刷新。改为从 project signal 重新取 artifacts（或保持 `list_artifacts` 调用作为刷新手段）。

**推荐方案**：保持 `list_artifacts` 调用作为删除后刷新手段。因为删除后 project signal 中的 artifacts 字段不会自动更新，单独调 `list_artifacts` 刷新更简单可靠。

所以**删除按钮的逻辑不需要改动**，保持现有的 `list_artifacts` 刷新方式。

- [ ] **Step 4: 在产物行操作列加查看详情链接**

确认 `frontend/src/pages/project/project_detail.rs` 顶部是否有 `use dioxus_router::Link;`。如果没有，在现有 use 语句之后添加。

在第 630-649 行的操作列 `td` 中，在删除按钮之前添加查看详情链接：

将第 630-649 行：

```rust
td { "data-label": "操作",
    button { class: "btn btn-error btn-sm",
        onclick: move |_| {
            // ...删除逻辑...
        },
        "删除"
    }
}
```

改为：

```rust
td { "data-label": "操作",
    div { class: "flex gap-1",
        Link {
            class: "btn btn-ghost btn-sm",
            to: crate::pages::Route::ProjectArtifactDetail { id: artifact_id.clone() },
            "查看"
        }
        button { class: "btn btn-error btn-sm",
            onclick: move |_| {
                // ...删除逻辑保持不变...
            },
            "删除"
        }
    }
}
```

**注意**：`artifact_id` 变量已在第 609 行 clone，可以直接使用。`Link` 的 `to` 属性需要 `crate::pages::Route::ProjectArtifactDetail { id: String }`。

- [ ] **Step 5: 验证编译**

Run: `cargo check -p frontend 2>&1 | tail -20`
Expected: 编译通过

- [ ] **Step 6: 提交**

```bash
git add frontend/src/pages/project/project_detail.rs
git commit -m "feat(frontend): use with_artifacts merged response in project detail

Replaces separate list_artifacts call with merged response from get_project.
Adds detail link to each artifact row."
```

---

## Task 6: 集成验证

- [ ] **Step 1: 后端编译 + 测试**

Run: `cargo check -p ai_orz 2>&1 | tail -10`
Expected: 编译通过

Run: `cargo test -p ai_orz --lib 2>&1 | tail -10`
Expected: 所有测试通过

- [ ] **Step 2: 前端编译**

Run: `cargo check -p frontend 2>&1 | tail -10`
Expected: 编译通过

- [ ] **Step 3: 最终提交（如有剩余改动）**

```bash
git add -A
git commit -m "chore: integration verification for artifact editing feature"
```

---

## Self-Review Checklist

### Spec coverage

| 需求 | 对应 Task |
|------|----------|
| 重命名 update_artifact_content 为 update_artifact + 扩展元信息更新 | Task 1 |
| 前端 update_artifact 重命名 + 路径更新 | Task 2 Step 1 |
| 前端 get_artifact API | Task 2 Step 1 |
| get_project 加 with_artifacts | Task 2 Step 2 |
| get_task 加 with_artifacts | Task 2 Step 3 |
| artifact_detail 元信息编辑 Modal | Task 3 |
| Task 详情页产物 Tab（合并返回） | Task 4 |
| Project 详情页合并返回 + 查看入口 | Task 5 |

### Placeholder scan

- 无 "TBD"、"TODO"、"implement later"
- 每个 Step 都有具体代码
- 行号精确标注

### Type consistency

- `UpdateArtifactRequest` 在 Task 1 定义（content 改 Option，加 name/description/tags），Task 2/3 使用 ✓
- `update_artifact` Domain 方法在 Task 1 定义，Task 3 前端调 `update_artifact` 时传 `content: None` ✓
- `ArtifactMetaModal` 在 Task 3 定义并使用 ✓
- `with_artifacts` 参数在 Task 2 API 层添加，Task 4/5 前端页面使用 ✓
- Task 4 用 `t.artifacts`（GetTaskResponse 的字段），Task 5 用 `p.artifacts`（GetProjectResponse 的字段）✓

### 向后兼容性说明

- `update_artifact_content` 工具 id 改为 `update_artifact`：Agent 调用时工具名变化，但当前无 Agent 在使用此工具（刚加 tag，未注册为 neural），影响可控
- `UpdateArtifactContentRequest` 改名为 `UpdateArtifactRequest`：DTO 重命名，前端已同步更新
- `content` 从 `String` 改为 `Option<String>`：部分更新语义，前端内容编辑器传 `content: Some(...)`，元信息编辑传 `content: None`
- 路由从 `PUT /artifacts/{id}/content` 改为 `PUT /artifacts/{id}`：前端已同步更新路径
