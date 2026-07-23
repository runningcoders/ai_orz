# 4 个 CRUD Edit Modal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 Agent / ModelProvider / Project / User 四个实体补齐 Edit Modal，复用已封装但未被调用的 `update_agent` / `update_model_provider` / `update_project` / `update_user` API 函数。

**Architecture:** 每个 Edit Modal 采用 `triggers.rs` 的 Create/Edit 双模式设计：私有 `XEditMode` 枚举 + 异步加载详情填表 + match 分发 Create/Update 提交。所有 Modal 复用 `Modal` + `Loading` 组件，无新增组件。

**Tech Stack:** Rust + Dioxus 0.7.9 + DaisyUI v5

---

## 文件结构

| 文件 | 责任 | 状态 |
|---|---|---|
| `frontend/src/api/hr.rs` | 移除 `update_agent` 的 `#[allow(dead_code)]` | 修改 |
| `frontend/src/api/finance.rs` | 移除 `update_model_provider` 的 `#[allow(dead_code)]` | 修改 |
| `frontend/src/api/project.rs` | 移除 `update_project` 的 `#[allow(dead_code)]` | 修改 |
| `frontend/src/api/organization.rs` | 移除 `update_user` 的 `#[allow(dead_code)]` | 修改 |
| `frontend/src/pages/hr/agent_detail.rs` | 加"编辑基本信息"按钮 + AgentEditModal | 修改 |
| `frontend/src/pages/finance/model_provider_detail.rs` | 加"编辑"按钮 + ModelProviderEditModal | 修改 |
| `frontend/src/pages/project/project_detail.rs` | 加"编辑项目"按钮 + ProjectEditModal | 修改 |
| `frontend/src/pages/organization/users.rs` | 加"编辑"按钮 + UserEditModal | 修改 |

---

## 前置知识：Update 请求 DTO 字段

```rust
// common/src/api/agent.rs
pub struct UpdateAgentRequest {
    pub id: String,
    pub name: Option<String>,
    pub roles: Option<Vec<String>>,
    pub description: Option<String>,
    pub capabilities: Option<Vec<String>>,
    pub soul: Option<String>,           // 灵魂提示词
    pub model_provider_id: Option<String>,
}

// common/src/api/model_provider.rs
pub struct UpdateModelProviderRequest {
    pub id: String,
    pub name: Option<String>,
    pub provider_type: Option<ProviderType>,
    pub model_name: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub description: Option<String>,
    pub status: Option<i32>,
}

// common/src/api/project.rs
pub struct UpdateProjectRequest {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub priority: Option<i32>,
    pub tags: Option<Vec<String>>,
}

// common/src/api/user.rs
pub struct UpdateUserRequest {
    pub user_id: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub role: Option<i32>,
    pub status: Option<i32>,
    pub password_hash: Option<String>,  // 直接暴露 hash；UI 不暴露改密码，置 None
}
```

## 前置知识：Create/Edit 双模式 Modal 模板

参考 `frontend/src/pages/system/triggers.rs` 的 `TriggerEditMode`：
```rust
#[derive(Debug, Clone, PartialEq)]
enum XEditMode { Create, Edit(String) }

let mut edit_mode = use_signal(|| XEditMode::Create);
let mut show_modal = use_signal(|| false);
let mut form_field1 = use_signal(String::new);
// ...
let mut loading_detail = use_signal(|| false);
let mut submitting = use_signal(|| false);

// 打开 Edit Modal：先重置表单 + 设置 Edit(id) + 异步加载详情填表
onclick: move |_| {
    let edit_id = id.clone();
    form_field1.set(String::new());
    // ... 重置所有字段
    edit_mode.set(XEditMode::Edit(edit_id.clone()));
    show_modal.set(true);
    loading_detail.set(true);
    spawn(async move {
        match get_x(&edit_id).await {
            Ok(d) => {
                form_field1.set(d.field1.clone());
                // ... 填入所有字段
            }
            Err(e) => { toast.error(...); show_modal.set(false); }
        }
        loading_detail.set(false);
    });
}

// 提交：match 分发
let handle_submit = move |_| {
    // 1. 校验字段
    submitting.set(true);
    let mode = edit_mode();
    spawn(async move {
        let result = match &mode {
            XEditMode::Edit(id) => {
                let req = UpdateXRequest { id: id.clone(), field1: Some(form_field1()), ... };
                update_x(id, req).await.map(|_| ())
            }
            XEditMode::Create => { /* 本 plan 不实现 Create，可 panic 或省略 */ unreachable!() }
        };
        match result {
            Ok(_) => { toast.success("已更新"); show_modal.set(false); reload().await; }
            Err(e) => toast.error(&e),
        }
        submitting.set(false);
    });
};
```

---

## Task 1: Agent Edit Modal

**Files:**
- Modify: `frontend/src/api/hr.rs`（移除 `update_agent` 的 `#[allow(dead_code)]`）
- Modify: `frontend/src/pages/hr/agent_detail.rs`

- [ ] **Step 1: 移除 `update_agent` 的 dead_code 标注**

打开 `frontend/src/api/hr.rs`，找到 `pub async fn update_agent`，删除其上方的 `#[allow(dead_code)]`。

- [ ] **Step 2: 在 `agent_detail.rs` 添加 Edit Modal 状态与逻辑**

在文件顶部 `use` 段追加：
```rust
use crate::api::hr::update_agent;
use common::api::UpdateAgentRequest;
```

在组件内信号区追加：
```rust
let mut show_edit_modal = use_signal(|| false);
let mut edit_name = use_signal(String::new);
let mut edit_roles = use_signal(String::new);       // 逗号分隔
let mut edit_description = use_signal(String::new);
let mut edit_capabilities = use_signal(String::new);  // 逗号分隔
let mut edit_soul = use_signal(String::new);
let mut edit_model_provider_id = use_signal(String::new);
let mut saving_meta = use_signal(|| false);
let mut model_providers = use_signal(Vec::<common::api::ListModelProvidersResponseItem>::new);
```

- [ ] **Step 3: 加载 model_providers 供下拉选择**

在已有 `use_effect`（加载 agent 详情）中并行加载 model providers：
```rust
match list_model_providers().await {
    Ok(resp) => model_providers.set(resp.providers),
    Err(e) => toast.error(&format!("加载模型提供商列表失败: {}", e)),
}
```

- [ ] **Step 4: 添加"编辑基本信息"按钮**

在 agent 详情页头部工具栏（与状态切换按钮同区）追加：
```rust
button {
    class: "btn btn-ghost btn-sm",
    onclick: move |_| {
        if let Some(a) = agent_data() {
            edit_name.set(a.name.clone());
            edit_roles.set(a.roles.join(", "));
            edit_description.set(a.description.clone());
            edit_capabilities.set(a.capabilities.join(", "));
            edit_soul.set(a.soul.clone().unwrap_or_default());
            edit_model_provider_id.set(a.model_provider_id.clone().unwrap_or_default());
            show_edit_modal.set(true);
        }
    },
    "✏️ 编辑"
}
```

> **注意**：`agent_data` 是详情页已有的 `Signal<Option<GetAgentResponse>>`，请对照实际变量名调整。`a.roles` / `a.capabilities` / `a.soul` / `a.model_provider_id` 字段需对照 `GetAgentResponse` 实际字段名。

- [ ] **Step 5: 添加 Modal 与提交逻辑**

在 rsx! 末尾追加：
```rust
Modal {
    title: "编辑 Agent 基本信息".to_string(),
    show: show_edit_modal(),
    on_close: move |_| show_edit_modal.set(false),
    footer: rsx! {
        button { class: "btn btn-ghost", onclick: move |_| show_edit_modal.set(false), "取消" }
        button {
            class: "btn btn-primary",
            disabled: saving_meta(),
            onclick: move |_| {
                let name = edit_name().trim().to_string();
                if name.is_empty() { toast.error("名称不能为空"); return; }
                let roles: Vec<String> = edit_roles()
                    .split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                let capabilities: Vec<String> = edit_capabilities()
                    .split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                let soul = if edit_soul().trim().is_empty() { None } else { Some(edit_soul()) };
                let mp_id = if edit_model_provider_id().is_empty() { None } else { Some(edit_model_provider_id()) };
                let req = UpdateAgentRequest {
                    id: id.clone(),
                    name: Some(name),
                    roles: Some(roles),
                    description: Some(edit_description()),
                    capabilities: Some(capabilities),
                    soul,
                    model_provider_id: mp_id,
                };
                saving_meta.set(true);
                let id_clone = id.clone();
                spawn(async move {
                    match update_agent(&id_clone, req).await {
                        Ok(_) => {
                            toast.success("Agent 信息已更新");
                            show_edit_modal.set(false);
                            // 重新加载详情（复用已有 reload 逻辑）
                            let stats_options = StatsOptions {
                                with_stats: true,
                                with_model_call_stats: true,
                                stats_interval: Some("daily".to_string()),
                            };
                            match get_agent(&id_clone, Some(&stats_options)).await {
                                Ok(a) => agent_data.set(Some(a)),
                                Err(e) => toast.error(&e),
                            }
                        }
                        Err(e) => toast.error(&e),
                    }
                    saving_meta.set(false);
                });
            },
            if saving_meta() { "保存中..." } else { "保存" }
        }
    },
    div { class: "space-y-4",
        div { class: "form-control w-full",
            label { class: "label", span { class: "label-text font-medium", "名称 *" } }
            input { class: "input input-bordered w-full", value: "{edit_name}",
                oninput: move |e| edit_name.set(e.value()), placeholder: "Agent 名称" }
        }
        div { class: "form-control w-full",
            label { class: "label", span { class: "label-text font-medium", "描述" } }
            textarea { class: "textarea textarea-bordered w-full", value: "{edit_description}",
                oninput: move |e| edit_description.set(e.value()) }
        }
        div { class: "form-control w-full",
            label { class: "label", span { class: "label-text font-medium", "角色（逗号分隔）" } }
            input { class: "input input-bordered w-full", value: "{edit_roles}",
                oninput: move |e| edit_roles.set(e.value()), placeholder: "assistant, coder" }
        }
        div { class: "form-control w-full",
            label { class: "label", span { class: "label-text font-medium", "能力（逗号分隔）" } }
            input { class: "input input-bordered w-full", value: "{edit_capabilities}",
                oninput: move |e| edit_capabilities.set(e.value()), placeholder: "text, vision" }
        }
        div { class: "form-control w-full",
            label { class: "label", span { class: "label-text font-medium", "灵魂提示词 (Soul)" } }
            textarea { class: "textarea textarea-bordered w-full", value: "{edit_soul}",
                oninput: move |e| edit_soul.set(e.value()), rows: "4" }
        }
        div { class: "form-control w-full",
            label { class: "label", span { class: "label-text font-medium", "模型提供商" } }
            select {
                class: "select select-bordered w-full",
                value: "{edit_model_provider_id}",
                onchange: move |e| edit_model_provider_id.set(e.value()),
                option { value: "", "（不绑定）" }
                for p in model_providers().iter() {
                    option { value: "{p.id}", "{p.name}" }
                }
            }
        }
    }
}
```

- [ ] **Step 6: 验证编译**

Run: `cd frontend && cargo build --release 2>&1 | tail -30`
Expected: 编译通过。若 `agent_data` / `roles` / `capabilities` 等字段名与实际不符，对照 `GetAgentResponse` 定义调整。

- [ ] **Step 7: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/api/hr.rs frontend/src/pages/hr/agent_detail.rs
git commit -m "feat(frontend): Agent 详情页追加编辑基本信息 Modal"
```

---

## Task 2: Model Provider Edit Modal

**Files:**
- Modify: `frontend/src/api/finance.rs`（移除 `update_model_provider` 的 `#[allow(dead_code)]`）
- Modify: `frontend/src/pages/finance/model_provider_detail.rs`

- [ ] **Step 1: 移除 dead_code 标注**

打开 `frontend/src/api/finance.rs`，找到 `pub async fn update_model_provider`，删除 `#[allow(dead_code)]`。

- [ ] **Step 2: 在 model_provider_detail.rs 添加 Modal 状态**

在 `use` 段追加：
```rust
use crate::api::finance::update_model_provider;
use common::api::{UpdateModelProviderRequest, ProviderType};
```

在组件内追加信号：
```rust
let mut show_edit_modal = use_signal(|| false);
let mut edit_name = use_signal(String::new);
let mut edit_provider_type = use_signal(String::new);
let mut edit_model_name = use_signal(String::new);
let mut edit_api_key = use_signal(String::new);
let mut edit_base_url = use_signal(String::new);
let mut edit_description = use_signal(String::new);
let mut saving_meta = use_signal(|| false);
```

- [ ] **Step 3: 添加"编辑"按钮**

在工具栏（与启用/禁用按钮同区）追加：
```rust
button {
    class: "btn btn-ghost btn-sm",
    onclick: move |_| {
        if let Some(p) = provider_data() {
            edit_name.set(p.name.clone());
            edit_provider_type.set(format!("{:?}", p.provider_type).to_lowercase());
            edit_model_name.set(p.model_name.clone());
            edit_api_key.set(p.api_key.clone());
            edit_base_url.set(p.base_url.clone().unwrap_or_default());
            edit_description.set(p.description.clone().unwrap_or_default());
            show_edit_modal.set(true);
        }
    },
    "✏️ 编辑"
}
```

> **注意**：`provider_data` 是 `Signal<Option<GetModelProviderResponse>>`，字段 `provider_type` / `api_key` / `base_url` / `description` 实际类型需对照定义调整（可能 `Option<String>`）。

- [ ] **Step 4: 添加 Modal 与提交逻辑**

在 rsx! 末尾追加：
```rust
Modal {
    title: "编辑模型提供商".to_string(),
    show: show_edit_modal(),
    on_close: move |_| show_edit_modal.set(false),
    footer: rsx! {
        button { class: "btn btn-ghost", onclick: move |_| show_edit_modal.set(false), "取消" }
        button {
            class: "btn btn-primary",
            disabled: saving_meta(),
            onclick: move |_| {
                let name = edit_name().trim().to_string();
                if name.is_empty() { toast.error("名称不能为空"); return; }
                let provider_type = match edit_provider_type().as_str() {
                    "openai" => ProviderType::OpenAi,
                    "anthropic" => ProviderType::Anthropic,
                    "ollama" => ProviderType::Ollama,
                    _ => ProviderType::OpenAi,
                };
                let req = UpdateModelProviderRequest {
                    id: id.clone(),
                    name: Some(name),
                    provider_type: Some(provider_type),
                    model_name: Some(edit_model_name()),
                    api_key: Some(edit_api_key()),
                    base_url: Some(edit_base_url()),
                    description: Some(edit_description()),
                    status: None,
                };
                saving_meta.set(true);
                let id_clone = id.clone();
                spawn(async move {
                    match update_model_provider(&id_clone, req).await {
                        Ok(_) => {
                            toast.success("已更新");
                            show_edit_modal.set(false);
                            let stats_options = StatsOptions {
                                with_stats: false,
                                with_model_call_stats: true,
                                stats_interval: None,
                            };
                            match get_model_provider(&id_clone, Some(&stats_options)).await {
                                Ok(p) => provider_data.set(Some(p)),
                                Err(e) => toast.error(&e),
                            }
                        }
                        Err(e) => toast.error(&e),
                    }
                    saving_meta.set(false);
                });
            },
            if saving_meta() { "保存中..." } else { "保存" }
        }
    },
    div { class: "space-y-4",
        div { class: "form-control w-full",
            label { class: "label", span { class: "label-text font-medium", "名称 *" } }
            input { class: "input input-bordered w-full", value: "{edit_name}",
                oninput: move |e| edit_name.set(e.value()) }
        }
        div { class: "form-control w-full",
            label { class: "label", span { class: "label-text font-medium", "提供商类型" } }
            select {
                class: "select select-bordered w-full",
                value: "{edit_provider_type}",
                onchange: move |e| edit_provider_type.set(e.value()),
                option { value: "openai", "OpenAI" }
                option { value: "anthropic", "Anthropic" }
                option { value: "ollama", "Ollama" }
            }
        }
        div { class: "form-control w-full",
            label { class: "label", span { class: "label-text font-medium", "模型名称" } }
            input { class: "input input-bordered w-full", value: "{edit_model_name}",
                oninput: move |e| edit_model_name.set(e.value()) }
        }
        div { class: "form-control w-full",
            label { class: "label", span { class: "label-text font-medium", "API Key" } }
            input { class: "input input-bordered w-full", r#type: "password", value: "{edit_api_key}",
                oninput: move |e| edit_api_key.set(e.value()) }
        }
        div { class: "form-control w-full",
            label { class: "label", span { class: "label-text font-medium", "Base URL" } }
            input { class: "input input-bordered w-full", value: "{edit_base_url}",
                oninput: move |e| edit_base_url.set(e.value()), placeholder: "https://api.openai.com/v1" }
        }
        div { class: "form-control w-full",
            label { class: "label", span { class: "label-text font-medium", "描述" } }
            textarea { class: "textarea textarea-bordered w-full", value: "{edit_description}",
                oninput: move |e| edit_description.set(e.value()) }
        }
    }
}
```

- [ ] **Step 5: 验证编译**

Run: `cd frontend && cargo build --release 2>&1 | tail -30`
Expected: 编译通过。`ProviderType` 实际枚举变体需对照 `common/src/enums.rs` 调整。

- [ ] **Step 6: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/api/finance.rs frontend/src/pages/finance/model_provider_detail.rs
git commit -m "feat(frontend): 模型提供商详情页追加编辑 Modal"
```

---

## Task 3: Project Edit Modal

**Files:**
- Modify: `frontend/src/api/project.rs`（移除 `update_project` 的 `#[allow(dead_code)]`）
- Modify: `frontend/src/pages/project/project_detail.rs`

- [ ] **Step 1: 移除 dead_code 标注**

打开 `frontend/src/api/project.rs`，删除 `pub async fn update_project` 上方的 `#[allow(dead_code)]`。

- [ ] **Step 2: 在 project_detail.rs 添加 Modal 状态**

在 `use` 段追加：
```rust
use crate::api::project::update_project;
use common::api::UpdateProjectRequest;
```

在组件内追加信号：
```rust
let mut show_edit_modal = use_signal(|| false);
let mut edit_name = use_signal(String::new);
let mut edit_description = use_signal(String::new);
let mut edit_priority = use_signal(|| "0".to_string());
let mut edit_tags = use_signal(String::new);
let mut saving_meta = use_signal(|| false);
```

- [ ] **Step 3: 添加"编辑项目"按钮**

在项目详情头部工具栏追加：
```rust
button {
    class: "btn btn-ghost btn-sm",
    onclick: move |_| {
        if let Some(p) = project() {
            edit_name.set(p.name.clone());
            edit_description.set(p.description.clone());
            edit_priority.set(p.priority.to_string());
            edit_tags.set(p.tags.join(", "));
            show_edit_modal.set(true);
        }
    },
    "✏️ 编辑"
}
```

> **注意**：`project` 是 `Signal<Option<GetProjectResponse>>`，字段名 `priority` / `tags` 对照实际定义调整。

- [ ] **Step 4: 添加 Modal 与提交逻辑**

在 rsx! 末尾追加：
```rust
Modal {
    title: "编辑项目".to_string(),
    show: show_edit_modal(),
    on_close: move |_| show_edit_modal.set(false),
    footer: rsx! {
        button { class: "btn btn-ghost", onclick: move |_| show_edit_modal.set(false), "取消" }
        button {
            class: "btn btn-primary",
            disabled: saving_meta(),
            onclick: move |_| {
                let name = edit_name().trim().to_string();
                if name.is_empty() { toast.error("名称不能为空"); return; }
                let priority: i32 = edit_priority().trim().parse().unwrap_or(0);
                let tags: Vec<String> = edit_tags()
                    .split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                let req = UpdateProjectRequest {
                    id: id.clone(),
                    name: Some(name),
                    description: Some(edit_description()),
                    priority: Some(priority),
                    tags: Some(tags),
                };
                saving_meta.set(true);
                let id_clone = id.clone();
                spawn(async move {
                    match update_project(&id_clone, req).await {
                        Ok(_) => {
                            toast.success("项目已更新");
                            show_edit_modal.set(false);
                            let stats_options = StatsOptions {
                                with_stats: true,
                                with_model_call_stats: true,
                                stats_interval: Some("daily".to_string()),
                            };
                            match get_project(&id_clone, Some(&stats_options)).await {
                                Ok(p) => project.set(Some(p)),
                                Err(e) => toast.error(&e),
                            }
                        }
                        Err(e) => toast.error(&e),
                    }
                    saving_meta.set(false);
                });
            },
            if saving_meta() { "保存中..." } else { "保存" }
        }
    },
    div { class: "space-y-4",
        div { class: "form-control w-full",
            label { class: "label", span { class: "label-text font-medium", "名称 *" } }
            input { class: "input input-bordered w-full", value: "{edit_name}",
                oninput: move |e| edit_name.set(e.value()) }
        }
        div { class: "form-control w-full",
            label { class: "label", span { class: "label-text font-medium", "描述" } }
            textarea { class: "textarea textarea-bordered w-full", value: "{edit_description}",
                oninput: move |e| edit_description.set(e.value()) }
        }
        div { class: "form-control w-full",
            label { class: "label", span { class: "label-text font-medium", "优先级（数字，越大越优先）" } }
            input { class: "input input-bordered w-full", r#type: "number", value: "{edit_priority}",
                oninput: move |e| edit_priority.set(e.value()) }
        }
        div { class: "form-control w-full",
            label { class: "label", span { class: "label-text font-medium", "标签（逗号分隔）" } }
            input { class: "input input-bordered w-full", value: "{edit_tags}",
                oninput: move |e| edit_tags.set(e.value()), placeholder: "tag1, tag2" }
        }
    }
}
```

- [ ] **Step 5: 验证编译**

Run: `cd frontend && cargo build --release 2>&1 | tail -20`
Expected: 编译通过。

- [ ] **Step 6: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/api/project.rs frontend/src/pages/project/project_detail.rs
git commit -m "feat(frontend): 项目详情页追加编辑 Modal"
```

---

## Task 4: User Edit Modal

**Files:**
- Modify: `frontend/src/api/organization.rs`（移除 `update_user` 的 `#[allow(dead_code)]`）
- Modify: `frontend/src/pages/organization/users.rs`

- [ ] **Step 1: 移除 dead_code 标注**

打开 `frontend/src/api/organization.rs`，删除 `pub async fn update_user` 上方的 `#[allow(dead_code)]`。

- [ ] **Step 2: 在 users.rs 添加 Modal 状态**

在 `use` 段追加：
```rust
use crate::api::organization::update_user;
use common::api::{UpdateUserRequest, ListUsersResponseItem};
```

在组件内追加信号：
```rust
let mut show_edit_modal = use_signal(|| false);
let mut edit_user_id = use_signal(String::new);
let mut edit_display_name = use_signal(String::new);
let mut edit_email = use_signal(String::new);
let mut edit_role = use_signal(|| "0".to_string());  // i32
let mut saving_meta = use_signal(|| false);
```

- [ ] **Step 3: 在列表行操作区追加"编辑"按钮**

找到每行操作区（当前只有"删除"按钮），追加：
```rust
button {
    class: "btn btn-ghost btn-sm",
    onclick: move |_| {
        edit_user_id.set(u.id.clone());
        edit_display_name.set(u.display_name.clone());
        edit_email.set(u.email.clone().unwrap_or_default());
        edit_role.set(u.role.to_string());
        show_edit_modal.set(true);
    },
    "✏️"
}
```

> **注意**：`u` 是当前行用户对象，字段 `id` / `display_name` / `email` / `role` 对照 `ListUsersResponseItem` 定义调整。`email` 可能是 `Option<String>`。

- [ ] **Step 4: 添加 Modal 与提交逻辑**

在 rsx! 末尾追加：
```rust
Modal {
    title: "编辑用户".to_string(),
    show: show_edit_modal(),
    on_close: move |_| show_edit_modal.set(false),
    footer: rsx! {
        button { class: "btn btn-ghost", onclick: move |_| show_edit_modal.set(false), "取消" }
        button {
            class: "btn btn-primary",
            disabled: saving_meta(),
            onclick: move |_| {
                let role: i32 = edit_role().trim().parse().unwrap_or(0);
                let req = UpdateUserRequest {
                    user_id: edit_user_id(),
                    display_name: Some(edit_display_name()),
                    email: Some(edit_email()),
                    role: Some(role),
                    status: None,
                    password_hash: None,  // UI 不暴露改密码
                };
                saving_meta.set(true);
                spawn(async move {
                    match update_user(req).await {
                        Ok(_) => {
                            toast.success("用户已更新");
                            show_edit_modal.set(false);
                            match list_users().await {
                                Ok(resp) => users.set(resp.users),
                                Err(e) => toast.error(&e),
                            }
                        }
                        Err(e) => toast.error(&e),
                    }
                    saving_meta.set(false);
                });
            },
            if saving_meta() { "保存中..." } else { "保存" }
        }
    },
    div { class: "space-y-4",
        div { class: "form-control w-full",
            label { class: "label", span { class: "label-text font-medium", "显示名" } }
            input { class: "input input-bordered w-full", value: "{edit_display_name}",
                oninput: move |e| edit_display_name.set(e.value()) }
        }
        div { class: "form-control w-full",
            label { class: "label", span { class: "label-text font-medium", "邮箱" } }
            input { class: "input input-bordered w-full", r#type: "email", value: "{edit_email}",
                oninput: move |e| edit_email.set(e.value()) }
        }
        div { class: "form-control w-full",
            label { class: "label", span { class: "label-text font-medium", "角色（0=普通 1=Admin 2=SuperAdmin）" } }
            select {
                class: "select select-bordered w-full",
                value: "{edit_role}",
                onchange: move |e| edit_role.set(e.value()),
                option { value: "0", "普通用户" }
                option { value: "1", "管理员" }
                option { value: "2", "超级管理员" }
            }
        }
    }
}
```

> **注意**：`update_user` 的当前签名是 `update_user(req: UpdateUserRequest)`（无单独 id 参数，id 在 req 内）。若实际签名不同请对照 `api/organization.rs` 调整。

- [ ] **Step 5: 验证编译**

Run: `cd frontend && cargo build --release 2>&1 | tail -20`
Expected: 编译通过。

- [ ] **Step 6: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/api/organization.rs frontend/src/pages/organization/users.rs
git commit -m "feat(frontend): 用户管理列表追加编辑 Modal"
```

---

## Task 5: 最终验证

- [ ] **Step 1: 完整构建**

Run:
```bash
cd /Users/aman/Technology/rust/ai_orz/frontend
cargo build --release 2>&1 | tail -30
dx build --release 2>&1 | tail -10
```
Expected: 通过。

- [ ] **Step 2: 后端测试无回归**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo test --workspace 2>&1 | grep -E "test result:|FAILED" | head -10`
Expected: 745 passed, 0 failed。

- [ ] **Step 3: 手动验证 4 个 Modal**

Run: `cd frontend && dx serve --port 8081`
依次访问：
1. `/hr/agents/:id` → "✏️ 编辑" → 修改名称 → 保存 → 看到刷新
2. `/finance/model-providers/:id` → "✏️ 编辑" → 修改 → 保存
3. `/projects/:id` → "✏️ 编辑" → 修改 → 保存
4. `/organization/users` → 行内"✏️" → 修改 → 保存

- [ ] **Step 4: 推送**

```bash
cd /Users/aman/Technology/rust/ai_orz
git push origin main
```

---

## Self-Review

**1. Spec coverage:**
- ✅ Agent Edit Modal → Task 1
- ✅ ModelProvider Edit Modal → Task 2
- ✅ Project Edit Modal → Task 3
- ✅ User Edit Modal → Task 4

**2. Placeholder scan:** 无 TBD/TODO。所有 Modal 代码完整。

**3. Type consistency:** 4 个 `UpdateXRequest` 字段名均与 `common/src/api/` 实际定义一致。`ProviderType` 枚举变体需对照 `common/src/enums.rs` 实际值（Task 2 Step 4 的 match 是假设值，需调整）。
