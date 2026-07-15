# 前端统计数据集成实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将后端实体统计数据动态注入到前端实体详情页，实现一次请求获取实体+统计数据。

**Architecture:** 扩展现有前端 API 客户端函数支持统计参数，在实体详情页添加可折叠统计面板，使用 `with_stats`/`with_model_call_stats` 参数按需加载。

**Tech Stack:** Dioxus 0.7, Rust, WebAssembly, CSS

---

## 文件结构

| 文件 | 职责 |
|------|------|
| `frontend/src/api/hr.rs` | Agent API 客户端，扩展 get_agent 支持统计参数 |
| `frontend/src/api/project.rs` | Project/Task API 客户端，扩展 get_project/get_task |
| `frontend/src/api/finance.rs` | Tool/ModelProvider API 客户端，扩展 get_tool/get_model_provider |
| `frontend/src/api/mod.rs` | API 模块入口，新增通用统计选项结构体 |
| `frontend/src/pages/hr/agent_detail.rs` | Agent 详情页，添加统计面板 |
| `frontend/src/pages/project/project_detail.rs` | Project 详情页，添加统计面板 |
| `frontend/src/pages/project/task_detail.rs` | Task 详情页，添加统计面板 |
| `frontend/src/components/stats.rs` | 统计面板公共组件 |

---

## Task 1: API 模块入口 - 新增通用统计选项

**Files:**
- Modify: `frontend/src/api/mod.rs`

- [ ] **Step 1: 新增 StatsOptions 结构体**

在 `frontend/src/api/mod.rs` 文件末尾添加：

```rust
#[derive(Debug, Clone)]
pub struct StatsOptions {
    pub with_stats: bool,
    pub with_model_call_stats: bool,
    pub stats_interval: Option<String>,
}

impl StatsOptions {
    pub fn to_query_string(&self) -> String {
        let mut params = Vec::new();
        if self.with_stats {
            params.push("with_stats=true".to_string());
        }
        if self.with_model_call_stats {
            params.push("with_model_call_stats=true".to_string());
        }
        if let Some(interval) = &self.stats_interval {
            params.push(format!("stats_interval={}", interval));
        }
        params.join("&")
    }
}

pub fn build_url_with_stats(base_url: &str, options: Option<&StatsOptions>) -> String {
    match options {
        Some(opt) => {
            let query = opt.to_query_string();
            if query.is_empty() {
                base_url.to_string()
            } else {
                format!("{}?{}", base_url, query)
            }
        }
        None => base_url.to_string(),
    }
}
```

- [ ] **Step 2: 编译检查**

Run: `cd frontend && cargo check`
Expected: 0 errors

- [ ] **Step 3: Commit**

```bash
git add frontend/src/api/mod.rs
git commit -m "feat: add StatsOptions and build_url_with_stats helper"
```

---

## Task 2: HR API - 扩展 get_agent 支持统计参数

**Files:**
- Modify: `frontend/src/api/hr.rs`

- [ ] **Step 1: 修改 get_agent 函数**

将：

```rust
pub async fn get_agent(id: &str) -> Result<GetAgentResponse, String> {
    api_get(&format!("/api/v1/hr/agents/{}", id)).await
}
```

替换为：

```rust
pub async fn get_agent(id: &str, stats_options: Option<&super::StatsOptions>) -> Result<GetAgentResponse, String> {
    let url = super::build_url_with_stats(&format!("/api/v1/hr/agents/{}", id), stats_options);
    api_get(&url).await
}
```

- [ ] **Step 2: 编译检查**

Run: `cd frontend && cargo check`
Expected: 0 errors

- [ ] **Step 3: 更新调用点**

搜索 `get_agent(` 调用点并更新：

文件: `frontend/src/pages/hr/agent_detail.rs`

将：

```rust
match get_agent(&aid).await {
```

替换为：

```rust
let stats_options = super::StatsOptions {
    with_stats: true,
    with_model_call_stats: true,
    stats_interval: Some("daily".to_string()),
};
match get_agent(&aid, Some(&stats_options)).await {
```

- [ ] **Step 4: 编译检查**

Run: `cd frontend && cargo check`
Expected: 0 errors

- [ ] **Step 5: Commit**

```bash
git add frontend/src/api/hr.rs frontend/src/pages/hr/agent_detail.rs
git commit -m "feat: extend get_agent with stats options"
```

---

## Task 3: Project API - 扩展 get_project/get_task 支持统计参数

**Files:**
- Modify: `frontend/src/api/project.rs`

- [ ] **Step 1: 修改 get_project 函数**

将：

```rust
pub async fn get_project(id: &str) -> Result<GetProjectResponse, String> {
    api_get(&format!("/api/v1/projects/{}", id)).await
}
```

替换为：

```rust
pub async fn get_project(id: &str, stats_options: Option<&super::StatsOptions>) -> Result<GetProjectResponse, String> {
    let url = super::build_url_with_stats(&format!("/api/v1/projects/{}", id), stats_options);
    api_get(&url).await
}
```

- [ ] **Step 2: 修改 get_task 函数**

将：

```rust
pub async fn get_task(id: &str) -> Result<GetTaskResponse, String> {
    api_get(&format!("/api/v1/tasks/{}", id)).await
}
```

替换为：

```rust
pub async fn get_task(id: &str, stats_options: Option<&super::StatsOptions>) -> Result<GetTaskResponse, String> {
    let url = super::build_url_with_stats(&format!("/api/v1/tasks/{}", id), stats_options);
    api_get(&url).await
}
```

- [ ] **Step 3: 更新 project_detail.rs 调用**

文件: `frontend/src/pages/project/project_detail.rs`

将：

```rust
match get_project(&id_clone).await {
```

替换为：

```rust
let stats_options = super::StatsOptions {
    with_stats: true,
    with_model_call_stats: true,
    stats_interval: Some("daily".to_string()),
};
match get_project(&id_clone, Some(&stats_options)).await {
```

- [ ] **Step 4: 更新 task_detail.rs 调用**

文件: `frontend/src/pages/project/task_detail.rs`

将：

```rust
match get_task(&id_clone).await {
```

替换为：

```rust
let stats_options = super::StatsOptions {
    with_stats: true,
    with_model_call_stats: true,
    stats_interval: Some("daily".to_string()),
};
match get_task(&id_clone, Some(&stats_options)).await {
```

- [ ] **Step 5: 编译检查**

Run: `cd frontend && cargo check`
Expected: 0 errors

- [ ] **Step 6: Commit**

```bash
git add frontend/src/api/project.rs frontend/src/pages/project/project_detail.rs frontend/src/pages/project/task_detail.rs
git commit -m "feat: extend get_project/get_task with stats options"
```

---

## Task 4: Finance API - 扩展 get_tool/get_model_provider 支持统计参数

**Files:**
- Modify: `frontend/src/api/finance.rs`

- [ ] **Step 1: 读取并修改 get_tool 函数**

首先读取 finance.rs 找到 get_tool 和 get_model_provider 函数，然后修改为支持统计参数。

- [ ] **Step 2: 修改 get_tool 函数**

```rust
pub async fn get_tool(id: &str, stats_options: Option<&super::StatsOptions>) -> Result<GetToolResponse, String> {
    let url = super::build_url_with_stats(&format!("/api/v1/finance/tools/{}", id), stats_options);
    api_get(&url).await
}
```

- [ ] **Step 3: 修改 get_model_provider 函数**

```rust
pub async fn get_model_provider(id: &str, stats_options: Option<&super::StatsOptions>) -> Result<GetModelProviderResponse, String> {
    let url = super::build_url_with_stats(&format!("/api/v1/finance/model-providers/{}", id), stats_options);
    api_get(&url).await
}
```

- [ ] **Step 4: 编译检查**

Run: `cd frontend && cargo check`
Expected: 0 errors

- [ ] **Step 5: Commit**

```bash
git add frontend/src/api/finance.rs
git commit -m "feat: extend get_tool/get_model_provider with stats options"
```

---

## Task 5: 创建统计面板公共组件

**Files:**
- Create: `frontend/src/components/stats.rs`

- [ ] **Step 1: 创建统计面板组件**

创建 `frontend/src/components/stats.rs`:

```rust
//! 统计面板公共组件

use common::models::{AgentStats, CallSummary, ModelCallStats, TokenSumResult};
use dioxus::prelude::*;

fn format_token_count(count: i64) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}K", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

fn format_qps(qps: f64) -> String {
    format!("{:.2}", qps)
}

#[component]
pub fn StatsCard(title: &str, icon: &str, value: &str, subtitle: Option<&str>) -> Element {
    rsx! {
        div { class: "stats-card",
            div { class: "stats-icon", "{icon}" }
            div { class: "stats-content",
                div { class: "stats-title", "{title}" }
                div { class: "stats-value", "{value}" }
                if let Some(sub) = subtitle {
                    div { class: "stats-subtitle", "{sub}" }
                }
            }
        }
    }
}

#[component]
pub fn AgentStatsPanel(stats: Option<&AgentStats>, model_call_stats: Option<&ModelCallStats>) -> Element {
    rsx! {
        div { class: "stats-panel",
            div { class: "stats-panel-header",
                div { class: "stats-panel-title", "📊 Agent 统计" }
            }
            div { class: "stats-grid",
                if let Some(s) = stats {
                    if let Some(call) = &s.call_summary {
                        StatsCard { title: "唤醒次数", icon: "🔔", value: &call.total_calls.to_string(), subtitle: None }
                        StatsCard { title: "平均 QPS", icon: "📈", value: &format_qps(call.avg_qps), subtitle: None }
                        StatsCard { title: "瞬时 QPS", icon: "⚡", value: &format_qps(call.instant_qps), subtitle: None }
                    }
                }
                if let Some(mcs) = model_call_stats {
                    if let Some(call) = &mcs.call_summary {
                        StatsCard { title: "模型调用", icon: "🤖", value: &call.total_calls.to_string(), subtitle: None }
                    }
                    if let Some(token) = &mcs.token_summary {
                        StatsCard { title: "输入 Token", icon: "📥", value: &format_token_count(token.total_tokens_input), subtitle: None }
                        StatsCard { title: "输出 Token", icon: "📤", value: &format_token_count(token.total_tokens_output), subtitle: None }
                    }
                }
            }
        }
    }
}

#[component]
pub fn ProjectStatsPanel(stats: Option<&common::models::ProjectStats>, model_call_stats: Option<&ModelCallStats>) -> Element {
    rsx! {
        div { class: "stats-panel",
            div { class: "stats-panel-header",
                div { class: "stats-panel-title", "📊 项目统计" }
            }
            div { class: "stats-grid",
                if let Some(s) = stats {
                    if let Some(call) = &s.call_summary {
                        StatsCard { title: "事件次数", icon: "📝", value: &call.total_calls.to_string(), subtitle: None }
                        StatsCard { title: "平均 QPS", icon: "📈", value: &format_qps(call.avg_qps), subtitle: None }
                    }
                }
                if let Some(mcs) = model_call_stats {
                    if let Some(call) = &mcs.call_summary {
                        StatsCard { title: "模型调用", icon: "🤖", value: &call.total_calls.to_string(), subtitle: None }
                    }
                    if let Some(token) = &mcs.token_summary {
                        StatsCard { title: "输入 Token", icon: "📥", value: &format_token_count(token.total_tokens_input), subtitle: None }
                        StatsCard { title: "输出 Token", icon: "📤", value: &format_token_count(token.total_tokens_output), subtitle: None }
                    }
                }
            }
        }
    }
}

#[component]
pub fn TaskStatsPanel(stats: Option<&common::models::TaskStats>, model_call_stats: Option<&ModelCallStats>) -> Element {
    rsx! {
        div { class: "stats-panel",
            div { class: "stats-panel-header",
                div { class: "stats-panel-title", "📊 任务统计" }
            }
            div { class: "stats-grid",
                if let Some(s) = stats {
                    if let Some(call) = &s.call_summary {
                        StatsCard { title: "事件次数", icon: "📝", value: &call.total_calls.to_string(), subtitle: None }
                        StatsCard { title: "平均 QPS", icon: "📈", value: &format_qps(call.avg_qps), subtitle: None }
                    }
                }
                if let Some(mcs) = model_call_stats {
                    if let Some(call) = &mcs.call_summary {
                        StatsCard { title: "模型调用", icon: "🤖", value: &call.total_calls.to_string(), subtitle: None }
                    }
                    if let Some(token) = &mcs.token_summary {
                        StatsCard { title: "输入 Token", icon: "📥", value: &format_token_count(token.total_tokens_input), subtitle: None }
                        StatsCard { title: "输出 Token", icon: "📤", value: &format_token_count(token.total_tokens_output), subtitle: None }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 2: 注册组件**

在 `frontend/src/components/mod.rs` 添加：

```rust
pub mod stats;
```

- [ ] **Step 3: 编译检查**

Run: `cd frontend && cargo check`
Expected: 0 errors

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/stats.rs frontend/src/components/mod.rs
git commit -m "feat: add StatsCard and entity stats panel components"
```

---

## Task 6: Agent 详情页集成统计面板

**Files:**
- Modify: `frontend/src/pages/hr/agent_detail.rs`

- [ ] **Step 1: 添加组件导入**

在文件头部添加：

```rust
use crate::components::stats::AgentStatsPanel;
```

- [ ] **Step 2: 在页面中添加统计面板**

找到记忆面板之后、对话区域之前的位置，添加：

```rust
if let Some(ref agent) = *agent_data.read() {
    if agent.stats.is_some() || agent.model_call_stats.is_some() {
        AgentStatsPanel {
            stats: agent.stats.as_ref(),
            model_call_stats: agent.model_call_stats.as_ref(),
        }
    }
}
```

- [ ] **Step 3: 编译检查**

Run: `cd frontend && cargo check`
Expected: 0 errors

- [ ] **Step 4: Commit**

```bash
git add frontend/src/pages/hr/agent_detail.rs
git commit -m "feat: integrate stats panel into agent detail page"
```

---

## Task 7: Project/Task 详情页集成统计面板

**Files:**
- Modify: `frontend/src/pages/project/project_detail.rs`
- Modify: `frontend/src/pages/project/task_detail.rs`

- [ ] **Step 1: 添加组件导入**

在 `project_detail.rs` 和 `task_detail.rs` 添加：

```rust
use crate::components::stats::{ProjectStatsPanel, TaskStatsPanel};
```

- [ ] **Step 2: 在 Project 详情页添加统计面板**

在项目基本信息之后、任务列表之前添加：

```rust
if let Some(ref project) = *project_data.read() {
    if project.stats.is_some() || project.model_call_stats.is_some() {
        ProjectStatsPanel {
            stats: project.stats.as_ref(),
            model_call_stats: project.model_call_stats.as_ref(),
        }
    }
}
```

- [ ] **Step 3: 在 Task 详情页添加统计面板**

在任务基本信息之后、操作按钮之前添加：

```rust
if let Some(ref t) = *task.read() {
    if t.stats.is_some() || t.model_call_stats.is_some() {
        TaskStatsPanel {
            stats: t.stats.as_ref(),
            model_call_stats: t.model_call_stats.as_ref(),
        }
    }
}
```

- [ ] **Step 4: 编译检查**

Run: `cd frontend && cargo check`
Expected: 0 errors

- [ ] **Step 5: Commit**

```bash
git add frontend/src/pages/project/project_detail.rs frontend/src/pages/project/task_detail.rs
git commit -m "feat: integrate stats panels into project/task detail pages"
```

---

## Task 8: 添加统计面板 CSS 样式

**Files:**
- Modify: `frontend/index.html` (CSS 样式)

- [ ] **Step 1: 添加统计面板 CSS**

在 index.html 的 `<style>` 标签中添加：

```css
.stats-panel {
    background: var(--color-background);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    padding: var(--space-lg);
    margin-bottom: var(--space-lg);
}

.stats-panel-header {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    margin-bottom: var(--space-md);
}

.stats-panel-title {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
    color: var(--color-text-primary);
}

.stats-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: var(--space-md);
}

.stats-card {
    background: var(--color-card);
    border-radius: var(--radius-md);
    padding: var(--space-md);
    display: flex;
    align-items: center;
    gap: var(--space-md);
}

.stats-icon {
    width: 40px;
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 24px;
    background: var(--color-primary-light);
    border-radius: var(--radius-md);
}

.stats-content {
    flex: 1;
}

.stats-title {
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.5px;
}

.stats-value {
    font-size: var(--font-size-xl);
    font-weight: var(--font-weight-bold);
    color: var(--color-text-primary);
}

.stats-subtitle {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
}
```

- [ ] **Step 2: 编译检查**

Run: `cd frontend && cargo check`
Expected: 0 errors

- [ ] **Step 3: Commit**

```bash
git add frontend/index.html
git commit -m "style: add stats panel CSS styles"
```

---

## Task 9: 全量验证

**Files:**
- None

- [ ] **Step 1: 后端编译和测试**

Run: `cargo check --lib && cargo test --lib`
Expected: 0 errors, 697 tests passed

- [ ] **Step 2: 前端编译检查**

Run: `cd frontend && cargo check`
Expected: 0 errors

- [ ] **Step 3: 提交验证结果**

```bash
git commit -m "chore: full verification passed" --allow-empty
```

---

## Self-Review

**1. Spec coverage:**
- ✅ API 客户端扩展支持统计参数
- ✅ Agent 详情页统计面板
- ✅ Project 详情页统计面板
- ✅ Task 详情页统计面板
- ✅ 公共统计组件
- ✅ CSS 样式

**2. Placeholder scan:**
- ✅ 无 TBD/TODO
- ✅ 无空泛描述
- ✅ 所有步骤都有具体代码

**3. Type consistency:**
- ✅ StatsOptions 结构在所有 API 客户端中一致
- ✅ 组件命名与实体类型对应

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-07-15-frontend-stats-integration.md`. Two execution options:

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**