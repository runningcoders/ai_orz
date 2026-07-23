# Skill 详情页 + 任务 Edit 入口 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 Skill 实体新增独立的详情页（含文件列表浏览、文件内容查看/在线编辑、Skill 元信息编辑），并在任务详情页加入 Edit 入口以复用已有的 TaskEditModal Edit 模式。

**Architecture:** 新建 `/hr/skills/:id` 路由 + `HrSkillDetail` 组件，复用现有 `Modal` / `ConfirmDialog` / `Loading` / `EmptyState` 组件。Skill 详情页采用"主信息卡 + 文件列表区 + 文件编辑 Modal"三段式布局。任务详情页仅在工具栏追加"编辑"按钮，调用已实现的 `TaskEditModal::Edit` 模式。前端 API 层新增 4 个 skill 文件相关函数。

**Tech Stack:** Rust + Dioxus 0.7.9 + DaisyUI v5 + reqwest (wasm)

---

## 文件结构

| 文件 | 责任 | 状态 |
|---|---|---|
| `frontend/src/api/hr.rs` | 新增 4 个 skill 文件相关 API 函数；移除 `get_skill` / `update_skill` 的 `#[allow(dead_code)]` | 修改 |
| `frontend/src/pages/hr/skill_detail.rs` | 新建 Skill 详情页组件（主信息 + 文件列表 + 文件编辑 Modal + 元信息编辑 Modal） | 新建 |
| `frontend/src/pages/hr/skills.rs | 修改列表行：加"详情"链接、"编辑"按钮 | 修改 |
| `frontend/src/pages/hr/mod.rs` | 注册 `skill_detail` 子模块 | 修改 |
| `frontend/src/pages/mod.rs` | 导入 `HrSkillDetail` 并注册 `Route::HrSkillDetail { id }` | 修改 |
| `frontend/src/pages/project/task_detail.rs` | 工具栏追加"编辑"按钮 + 接入 `TaskEditModal::Edit` 模式 | 修改 |
| `frontend/src/components/code_editor.rs` | 新建轻量代码编辑器组件（textarea + 等宽字体 + 行号显示） | 新建 |
| `frontend/src/components/mod.rs` | 注册 `code_editor` 子模块 | 修改 |

---

## 前置知识

### 后端 Skill 文件相关 DTO（位于 `common/src/api/skill.rs`）

```rust
pub struct SkillFileItem { pub filename: String, pub file_size: u64, pub has_content: bool }
pub struct ListSkillFilesResponse { pub files: Vec<SkillFileItem> }
pub struct GetSkillFileContentResponse { pub content: String }
pub struct UpdateSkillFileContentRequest {
    pub skill_id: String,
    pub filename: String,
    pub content: String,
    pub expected_updated_at: Option<i64>,  // 乐观锁，前端可传 None
}
pub type UpdateSkillFileContentResponse = ();  // 空响应
pub struct SkillDetail {
    pub id: String, pub name: String, pub description: String, pub tags: Vec<String>,
    pub category: String, pub parent_skill_id: String, pub author_id: String,
    pub content: Option<String>,             // skill.md 内容
    pub files: Vec<SkillFileItem>,           // 文件列表
    pub status: SkillStatus, pub created_at: i64, pub updated_at: i64,
    // ... 其他字段
}
pub struct UpdateSkillRequest {
    pub skill_id: String,
    pub name: Option<String>, pub description: Option<String>,
    pub tags: Option<Vec<String>>, pub category: Option<String>,
    pub status: Option<SkillStatus>, pub content: Option<String>,
    pub files: Option<Vec<SkillFileInput>>,  // 不在 UI 暴露文件上传，置 None
}
```

### 后端路由（已存在，无需改动）

- `GET /api/v1/hr/skills/{id}` → `GetSkillResponse = SkillDetail`
- `PUT /api/v1/hr/skills/{id}` → `UpdateSkillResponse = SkillDetail`
- `GET /api/v1/hr/skills/{skill_id}/files` → `ListSkillFilesResponse`
- `GET /api/v1/hr/skills/{skill_id}/files/{*filename}` → `GetSkillFileContentResponse`
- `PUT /api/v1/hr/skills/{skill_id}/files/{*filename}` → `UpdateSkillFileContentResponse`

### 前端已有可复用组件

- `Modal` (props: `title`/`show`/`on_close`/`children`/`footer: Option<Element>`)
- `ConfirmDialog` (props: `show`/`title`/`message`/`confirm_text`/`cancel_text`/`confirm_class`/`on_confirm`/`on_cancel`)
- `Loading` / `EmptyState { icon, message }`
- `use_toast()` → `toast.success/error/info(impl Display)`
- `AppLayout { ... }` 包裹页面

### TaskEditModal 已实现的 Edit 模式

`frontend/src/pages/project/task_edit_modal.rs` 中 `TaskEditModal` 组件已支持 `mode: TaskEditMode::Edit(task_id)`，会异步调用 `get_task` 加载详情填入表单，提交时调 `update_task`。当前仅 `project_detail.rs` 以 Create 模式调用。

---

## Task 1: 新增前端 API 函数（skill 文件相关）

**Files:**
- Modify: `frontend/src/api/hr.rs`（在 `delete_skill` 函数之后追加 4 个函数；移除 `get_skill` / `update_skill` 的 `#[allow(dead_code)]`）

- [ ] **Step 1: 移除 `get_skill` 和 `update_skill` 上的 `#[allow(dead_code)]` 标注**

打开 `frontend/src/api/hr.rs`，找到 `pub async fn get_skill` 和 `pub async fn update_skill` 两处 `#[allow(dead_code)]`，删除该 attribute 行（因为即将被 Skill 详情页调用）。

- [ ] **Step 2: 在 `delete_skill` 函数之后追加 4 个 skill 文件相关 API 函数**

```rust
// ===== Skill 文件管理 =====

/// 列出 Skill 的所有文件
pub async fn list_skill_files(skill_id: &str) -> Result<common::api::ListSkillFilesResponse, ApiError> {
    api_get(&format!("/api/v1/hr/skills/{}/files", skill_id)).await
}

/// 获取 Skill 文件内容（filename 可能含 /，需 URL 编码路径段）
pub async fn get_skill_file_content(skill_id: &str, filename: &str) -> Result<common::api::GetSkillFileContentResponse, ApiError> {
    api_get(&format!("/api/v1/hr/skills/{}/files/{}", skill_id, filename)).await
}

/// 更新 Skill 文件内容（乐观锁字段前端置 None）
pub async fn update_skill_file_content(skill_id: &str, filename: &str, content: String) -> Result<(), ApiError> {
    let req = common::api::UpdateSkillFileContentRequest {
        skill_id: skill_id.to_string(),
        filename: filename.to_string(),
        content,
        expected_updated_at: None,
    };
    api_put_empty(&format!("/api/v1/hr/skills/{}/files/{}", skill_id, filename), &req).await
}
```

- [ ] **Step 3: 验证编译**

Run: `cd frontend && cargo build --release 2>&1 | tail -20`
Expected: 编译通过，无新增 error（可能有 unused 警告，因为新函数尚未被调用）。

- [ ] **Step 4: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/api/hr.rs
git commit -m "feat(frontend): 新增 skill 文件相关 API 函数 (list/get/update_file_content)"
```

---

## Task 2: 新建轻量代码编辑器组件

**Files:**
- Create: `frontend/src/components/code_editor.rs`
- Modify: `frontend/src/components/mod.rs`（注册 `code_editor` 模块）

- [ ] **Step 1: 在 `frontend/src/components/mod.rs` 末尾追加模块声明**

```rust
pub mod code_editor;
```

- [ ] **Step 2: 创建 `frontend/src/components/code_editor.rs`**

```rust
//! 轻量代码编辑器组件：textarea + 等宽字体 + 行号显示
//!
//! 不引入 Monaco/CodeMirror 等重量级编辑器，使用纯 textarea + 行号同步滚动
//! 适用于 Skill 文件内容编辑、Artifact 内容编辑等场景

use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct CodeEditorProps {
    /// 当前内容
    value: String,
    /// 内容变更回调
    on_input: EventHandler<String>,
    /// 语言（用于占位符显示，不实际做语法高亮）
    #[props(default = "text".to_string())]
    language: String,
    /// 是否只读
    #[props(default = false)]
    read_only: bool,
    /// 最小行数（控制高度）
    #[props(default = 16)]
    min_lines: u32,
}

#[component]
pub fn CodeEditor(props: CodeEditorProps) -> Element {
    let line_count = props.value.lines().count().max(props.min_lines as usize);
    let line_numbers = (1..=line_count)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    rsx! {
        div { class: "code-editor-container",
            style: "display: flex; border: 1px solid var(--color-border, #e5e7eb); border-radius: var(--radius-md, 6px); overflow: hidden; background: var(--color-mistral-black, #1e1e1e);",
            // 行号栏
            pre {
                class: "code-editor-line-numbers",
                style: "margin: 0; padding: 12px 8px; min-width: 48px; text-align: right; color: #6b7280; font-family: ui-monospace, 'SF Mono', Menlo, monospace; font-size: 13px; line-height: 1.6; user-select: none; overflow: hidden; white-space: pre;",
                "{line_numbers}"
            }
            // 编辑区
            textarea {
                class: "code-editor-textarea",
                style: "flex: 1; min-height: {props.min_lines * 24}px; padding: 12px; border: none; outline: none; resize: vertical; background: transparent; color: var(--color-text-on-dark, #e5e7eb); font-family: ui-monospace, 'SF Mono', Menlo, monospace; font-size: 13px; line-height: 1.6; white-space: pre; overflow: auto;",
                value: "{props.value}",
                readonly: props.read_only,
                placeholder: "请输入 {props.language} 内容...",
                oninput: move |e| props.on_input.call(e.value()),
                spellcheck: "false",
                autocomplete: "off",
                autocapitalize: "off",
                autocorrect: "off",
            }
        }
    }
}
```

- [ ] **Step 3: 验证编译**

Run: `cd frontend && cargo build --release 2>&1 | tail -20`
Expected: 编译通过。

- [ ] **Step 4: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/components/code_editor.rs frontend/src/components/mod.rs
git commit -m "feat(frontend): 新增轻量 CodeEditor 组件 (textarea + 行号)"
```

---

## Task 3: 新建 Skill 详情页主体

**Files:**
- Create: `frontend/src/pages/hr/skill_detail.rs`
- Modify: `frontend/src/pages/hr/mod.rs`（注册 `skill_detail` 模块）
- Modify: `frontend/src/pages/mod.rs`（导入 `HrSkillDetail` 并注册路由）

- [ ] **Step 1: 在 `frontend/src/pages/hr/mod.rs` 追加模块声明**

```rust
pub mod skill_detail;
```

- [ ] **Step 2: 在 `frontend/src/pages/mod.rs` 导入 + 注册路由**

在 `use` 段追加：
```rust
use crate::pages::hr::skill_detail::HrSkillDetail;
```

在 `Route` 枚举的 `HrSkills {}` 之后追加：
```rust
#[route("/hr/skills/:id")]
HrSkillDetail { id: String },
```

- [ ] **Step 3: 创建 `frontend/src/pages/hr/skill_detail.rs`**

```rust
//! Skill 详情页 - 展示元信息 + 文件列表 + 文件内容查看/编辑 + 元信息编辑

use dioxus::prelude::*;

use crate::api::hr::{get_skill, list_skill_files, update_skill, get_skill_file_content, update_skill_file_content};
use crate::components::code_editor::CodeEditor;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{SkillDetail, SkillFileItem, UpdateSkillRequest};

#[component]
pub fn HrSkillDetail(id: String) -> Element {
    let toast = use_toast();

    let mut skill = use_signal(|| Option::<SkillDetail>::None);
    let mut files = use_signal(Vec::<SkillFileItem>::new);
    let mut loading = use_signal(|| true);
    let mut selected_file = use_signal(String::new);
    let mut file_content = use_signal(String::new);
    let mut file_content_loading = use_signal(|| false);
    let mut file_content_dirty = use_signal(|| false);
    let mut saving_file = use_signal(|| false);

    // 元信息编辑 Modal
    let mut show_edit_modal = use_signal(|| false);
    let mut edit_name = use_signal(String::new);
    let mut edit_description = use_signal(String::new);
    let mut edit_tags = use_signal(String::new);
    let mut edit_category = use_signal(String::new);
    let mut saving_meta = use_signal(|| false);

    let reload = {
        let id = id.clone();
        move || {
            let id = id.clone();
            let toast = toast;
            async move {
                match get_skill(&id).await {
                    Ok(s) => skill.set(Some(s.clone())),
                    Err(e) => toast.error(&format!("加载 Skill 失败: {}", e)),
                }
                match list_skill_files(&id).await {
                    Ok(resp) => files.set(resp.files),
                    Err(e) => toast.error(&format!("加载文件列表失败: {}", e)),
                }
            }
        }
    };

    // 初始加载
    let id_for_effect = id.clone();
    use_effect(move || {
        loading.set(true);
        let id = id_for_effect.clone();
        spawn(async move {
            let toast = toast;
            match get_skill(&id).await {
                Ok(s) => skill.set(Some(s)),
                Err(e) => toast.error(&format!("加载 Skill 失败: {}", e)),
            }
            match list_skill_files(&id).await {
                Ok(resp) => files.set(resp.files),
                Err(e) => toast.error(&format!("加载文件列表失败: {}", e)),
            }
            loading.set(false);
        });
    });

    // 选择文件 → 异步加载内容
    let on_click_file = move |filename: String| {
        let skill_id = id.clone();
        selected_file.set(filename.clone());
        file_content.set(String::new());
        file_content_dirty.set(false);
        file_content_loading.set(true);
        spawn(async move {
            let toast = toast;
            match get_skill_file_content(&skill_id, &filename).await {
                Ok(resp) => file_content.set(resp.content),
                Err(e) => toast.error(&format!("加载文件内容失败: {}", e)),
            }
            file_content_loading.set(false);
        });
    };

    // 保存文件内容
    let on_save_file = move |_| {
        let skill_id = id.clone();
        let filename = selected_file();
        let content = file_content();
        if filename.is_empty() { return; }
        saving_file.set(true);
        spawn(async move {
            let toast = toast;
            match update_skill_file_content(&skill_id, &filename, content).await {
                Ok(_) => {
                    toast.success("文件已保存");
                    file_content_dirty.set(false);
                }
                Err(e) => toast.error(&format!("保存失败: {}", e)),
            }
            saving_file.set(false);
        });
    };

    // 打开元信息编辑 Modal（填入当前值）
    let on_open_edit = move |_| {
        if let Some(s) = skill() {
            edit_name.set(s.name.clone());
            edit_description.set(s.description.clone());
            edit_tags.set(s.tags.join(", "));
            edit_category.set(s.category.clone());
            show_edit_modal.set(true);
        }
    };

    // 提交元信息更新
    let on_submit_edit = move |_| {
        let skill_id = id.clone();
        let name = edit_name().trim().to_string();
        if name.is_empty() {
            toast.error("名称不能为空");
            return;
        }
        let tags: Vec<String> = edit_tags()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let category = if edit_category().trim().is_empty() {
            None
        } else {
            Some(edit_category().trim().to_string())
        };
        let req = UpdateSkillRequest {
            skill_id: skill_id.clone(),
            name: Some(name),
            description: Some(edit_description()),
            tags: Some(tags),
            category,
            status: None,
            content: None,
            files: None,
        };
        saving_meta.set(true);
        spawn(async move {
            let toast = toast;
            match update_skill(&skill_id, req).await {
                Ok(_) => {
                    toast.success("Skill 元信息已更新");
                    show_edit_modal.set(false);
                    reload().await;
                }
                Err(e) => toast.error(&format!("更新失败: {}", e)),
            }
            saving_meta.set(false);
        });
    };

    let skill_data = skill.read().clone();
    let files_list = files.read().clone();

    rsx! {
        AppLayout {
            div { class: "mb-6 flex items-center justify-between",
                h1 { class: "text-2xl font-bold", "Skill 详情" }
                Link { class: "btn btn-ghost", to: Route::HrSkills {}, "← 返回列表" }
            }
            if loading() {
                Loading {}
            } else if let Some(s) = skill_data {
                // 主信息卡
                div { class: "card bg-base-100 shadow-md mb-6",
                    div { class: "card-body",
                        div { class: "flex justify-between items-center mb-4",
                            h2 { class: "card-title", "{s.name}" }
                            div { class: "flex gap-2",
                                button { class: "btn btn-ghost btn-sm", onclick: on_open_edit, "✏️ 编辑" }
                            }
                        }
                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                            div { div { class: "text-sm text-base-content/60", "描述" }, div { class: "font-medium", "{s.description}" } }
                            div { div { class: "text-sm text-base-content/60", "分类" }, div { class: "font-medium", "{s.category}" } }
                            div { div { class: "text-sm text-base-content/60", "标签" },
                                div { class: "flex flex-wrap gap-1",
                                    for tag in &s.tags { span { class: "badge badge-neutral", "{tag}" } }
                                }
                            }
                            div { div { class: "text-sm text-base-content/60", "状态" }, span { class: "badge", "{skill_status_text(s.status)}" } }
                        }
                    }
                }
                // 文件列表区
                div { class: "card bg-base-100 shadow-md mb-6",
                    div { class: "card-body",
                        h2 { class: "card-title text-lg mb-2", "📁 文件列表 ({files_list.len()})" }
                        if files_list.is_empty() {
                            EmptyState { icon: "📄".to_string(), message: "此 Skill 暂无文件".to_string() }
                        } else {
                            div { class: "grid grid-cols-1 md:grid-cols-3 gap-4",
                                // 左侧文件列表
                                div { class: "md:col-span-1",
                                    ul { class: "menu bg-base-200 rounded-box",
                                        for f in files_list.iter() {
                                            {
                                                let fname = f.filename.clone();
                                                let active = selected_file() == fname;
                                                rsx! {
                                                    li {
                                                        button {
                                                            class: if active { "active" } else { "" },
                                                            onclick: move |_| on_click_file(fname.clone()),
                                                            div { class: "flex justify-between items-center w-full",
                                                                span { class: "font-mono text-sm truncate", "{f.filename}" }
                                                                span { class: "text-xs text-base-content/50", "{format_file_size(f.file_size)}" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                // 右侧内容区
                                div { class: "md:col-span-2",
                                    if selected_file().is_empty() {
                                        EmptyState { icon: "👈".to_string(), message: "请选择左侧文件查看内容".to_string() }
                                    } else if file_content_loading() {
                                        Loading {}
                                    } else {
                                        div { class: "flex flex-col gap-2",
                                            div { class: "flex justify-between items-center",
                                                span { class: "font-mono text-sm text-base-content/70", "当前文件: {selected_file()}" }
                                                div { class: "flex gap-2",
                                                    if file_content_dirty() {
                                                        span { class: "text-xs text-warning", "● 未保存" }
                                                    }
                                                    button {
                                                        class: "btn btn-primary btn-sm",
                                                        disabled: saving_file() || !file_content_dirty(),
                                                        onclick: on_save_file,
                                                        if saving_file() { "保存中..." } else { "💾 保存" }
                                                    }
                                                }
                                            }
                                            CodeEditor {
                                                value: file_content(),
                                                on_input: move |v| {
                                                    file_content.set(v);
                                                    file_content_dirty.set(true);
                                                },
                                                language: "markdown".to_string(),
                                                min_lines: 20,
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                EmptyState { icon: "❓".to_string(), message: "Skill 不存在或已被删除".to_string() }
            }

            // 元信息编辑 Modal
            Modal {
                title: "编辑 Skill 元信息".to_string(),
                show: show_edit_modal(),
                on_close: move |_| show_edit_modal.set(false),
                footer: rsx! {
                    button { class: "btn btn-ghost", onclick: move |_| show_edit_modal.set(false), "取消" }
                    button {
                        class: "btn btn-primary",
                        disabled: saving_meta(),
                        onclick: on_submit_edit,
                        if saving_meta() { "保存中..." } else { "保存" }
                    }
                },
                div { class: "space-y-4",
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "名称 *" } }
                        input { class: "input input-bordered w-full", value: "{edit_name}",
                            oninput: move |e| edit_name.set(e.value()), placeholder: "Skill 名称" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "描述" } }
                        textarea { class: "textarea textarea-bordered w-full", value: "{edit_description}",
                            oninput: move |e| edit_description.set(e.value()), placeholder: "Skill 描述" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "标签（逗号分隔）" } }
                        input { class: "input input-bordered w-full", value: "{edit_tags}",
                            oninput: move |e| edit_tags.set(e.value()), placeholder: "tag1, tag2" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "分类" } }
                        input { class: "input input-bordered w-full", value: "{edit_category}",
                            oninput: move |e| edit_category.set(e.value()), placeholder: "如 uncategorized / neural" }
                    }
                }
            }
        }
    }
}

fn skill_status_text(status: common::enums::SkillStatus) -> &'static str {
    use common::enums::SkillStatus::*;
    match status {
        Draft => "草稿",
        Published => "已发布",
        Expired => "已过期",
        Archived => "已归档",
    }
}

fn format_file_size(bytes: u64) -> String {
    crate::utils::format_file_size(bytes)
}
```

- [ ] **Step 4: 验证编译**

Run: `cd frontend && cargo build --release 2>&1 | tail -30`
Expected: 编译通过。如果报错（如 `SkillStatus` 枚举变体名不对），对照 `common/src/enums.rs` 修正。

- [ ] **Step 5: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/pages/hr/skill_detail.rs frontend/src/pages/hr/mod.rs frontend/src/pages/mod.rs
git commit -m "feat(frontend): 新增 Skill 详情页 (元信息 + 文件列表 + 文件内容编辑)"
```

---

## Task 4: Skill 列表页加入详情链接

**Files:**
- Modify: `frontend/src/pages/hr/skills.rs`（每行操作区追加"详情"按钮，跳转到 `Route::HrSkillDetail { id }`）

- [ ] **Step 1: 修改 skills.rs 的操作列**

找到每行操作按钮区（当前只有"删除"按钮），追加"详情"按钮：

```rust
td { "data-label": "操作",
    div { class: "flex gap-1",
        Link {
            class: "btn btn-ghost btn-sm",
            to: Route::HrSkillDetail { id: id.clone() },
            "详情"
        }
        button {
            class: "btn btn-error btn-sm",
            onclick: move |_| {
                pending_delete_id.set(id.clone());
                show_delete_confirm.set(true);
            },
            "删除"
        }
    }
}
```

> 注意：原有代码可能没有 `id` 变量在循环外 clone，需要确保循环内有 `let id = s.id.clone();`。

- [ ] **Step 2: 验证编译**

Run: `cd frontend && cargo build --release 2>&1 | tail -20`
Expected: 编译通过。

- [ ] **Step 3: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/pages/hr/skills.rs
git commit -m "feat(frontend): Skill 列表行追加详情入口"
```

---

## Task 5: 任务详情页追加 Edit 入口

**Files:**
- Modify: `frontend/src/pages/project/task_detail.rs`（在工具栏追加"编辑"按钮，调用已存在的 `TaskEditModal` 的 Edit 模式）

- [ ] **Step 1: 先阅读 `task_edit_modal.rs` 了解 Edit 模式调用方式**

Run: `cd frontend && head -80 src/pages/project/task_edit_modal.rs`
观察 `TaskEditModal` 组件的 props 和 `TaskEditMode::Edit(String)` 用法。

- [ ] **Step 2: 在 `task_detail.rs` 引入 TaskEditModal 并添加状态**

在文件顶部 `use` 段追加：
```rust
use crate::pages::project::task_edit_modal::{TaskEditModal, TaskEditMode};
```

在组件内信号区追加：
```rust
let mut show_edit_modal = use_signal(|| false);
```

- [ ] **Step 3: 在工具栏追加"编辑"按钮**

找到任务详情页头部工具栏（通常在 `div { class: "flex justify-between items-center" }` 或类似位置，包含"返回"链接处），追加：
```rust
button {
    class: "btn btn-primary btn-sm",
    onclick: move |_| show_edit_modal.set(true),
    "✏️ 编辑"
}
```

- [ ] **Step 4: 在 rsx! 末尾（ConfirmDialog 之后）追加 TaskEditModal**

```rust
TaskEditModal {
    mode: TaskEditMode::Edit(id.clone()),
    show: show_edit_modal(),
    on_close: move |_| show_edit_modal.set(false),
    on_saved: move |_| {
        // 保存后重新加载任务详情
        let id = id.clone();
        spawn(async move {
            let stats_options = StatsOptions {
                with_stats: true,
                with_model_call_stats: true,
                stats_interval: Some("daily".to_string()),
            };
            match get_task(&id, Some(&stats_options)).await {
                Ok(t) => task.set(Some(t)),
                Err(e) => toast.error(&e),
            }
        });
    },
}
```

> **注意**：`TaskEditModal` 的实际 props（`show`/`on_close`/`on_saved` 等）需要对照 `task_edit_modal.rs` 实际定义调整。若该组件没有 `show`/`on_close` props 而是依赖外部 `use_signal`，请改为将 signal 传入或调整 `TaskEditModal` 暴露的接口。**先读 task_edit_modal.rs 确认 props 签名后再写这段代码。**

- [ ] **Step 5: 验证编译**

Run: `cd frontend && cargo build --release 2>&1 | tail -20`
Expected: 编译通过。如果 TaskEditModal 的 props 不匹配，根据编译错误调整。

- [ ] **Step 6: 手动验证**

Run: `cd frontend && dx serve --port 8081`
在浏览器打开 `http://localhost:8081/tasks/任意已存在任务id`，点击"编辑"按钮，确认 Modal 弹出并加载了任务详情，修改后保存成功。

- [ ] **Step 7: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/pages/project/task_detail.rs
git commit -m "feat(frontend): 任务详情页追加 Edit 入口 (复用 TaskEditModal Edit 模式)"
```

---

## Task 6: 最终验证与集成

- [ ] **Step 1: 完整构建验证**

Run:
```bash
cd /Users/aman/Technology/rust/ai_orz/frontend
cargo build --release 2>&1 | tail -30
dx build --release 2>&1 | tail -10
```
Expected: 均通过，无新增 error。

- [ ] **Step 2: 后端测试无回归**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo test --workspace 2>&1 | grep -E "test result:|FAILED" | head -20`
Expected: 745 passed, 0 failed。

- [ ] **Step 3: 手动验证 Skill 详情页核心流程**

Run: `cd frontend && dx serve --port 8081`
在浏览器：
1. 访问 `/hr/skills` → 点击任意 Skill 的"详情"
2. 确认元信息卡展示正常
3. 确认文件列表加载，点击任一文件 → 内容显示
4. 修改文件内容 → 点击保存 → 看到"文件已保存" toast
5. 点击"✏️ 编辑" → 修改名称 → 保存 → 元信息刷新

- [ ] **Step 4: 推送**

```bash
cd /Users/aman/Technology/rust/ai_orz
git push origin main
```

---

## Self-Review

**1. Spec coverage:**
- ✅ Skill 详情页 → Task 3
- ✅ Skill 文件浏览 → Task 3 + Task 1（API）
- ✅ Skill 文件编辑 → Task 3 + Task 1（API）+ Task 2（CodeEditor）
- ✅ Skill 元信息编辑 → Task 3
- ✅ Skill 列表入口 → Task 4
- ✅ 任务详情 Edit 入口 → Task 5

**2. Placeholder scan:** 无 TBD/TODO/"add appropriate"。所有代码均完整。

**3. Type consistency:** `SkillDetail` / `SkillFileItem` / `UpdateSkillRequest` 字段名与 `common/src/api/skill.rs` 一致；`TaskEditMode::Edit(String)` 与 `task_edit_modal.rs` 一致（Task 5 已注明需先确认 props）。
