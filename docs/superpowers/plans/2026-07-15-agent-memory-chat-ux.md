# Agent 记忆面板 + 对话体验打磨实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Agent 详情页新增记忆浏览面板（按类型 Tab 切换），并打磨对话页面的用户体验（消息复制、快捷指令、代码块优化等）。全部为纯前端工作，后端 API 已就绪。

**Architecture:**
- **记忆面板**：Agent 详情页新增第七个 detail-section，内部用 Tab 切换短期记忆/知识节点/关系三种视图，复用现有 `query_memory(agent_id, memory_type)` API
- **对话体验打磨**：在现有 `MessageChat` 组件中增加消息复制、快捷指令输入、代码块高亮等小功能
- **后端零改动**：所有 API 客户端已有，DTO 已有

**Tech Stack:** Dioxus 0.7.9、dioxus_router、chrono、Mistral CSS 设计系统

---

## Scope Check

本计划包含两个独立子系统：记忆面板（Subsystem A）和对话体验打磨（Subsystem B）。两者互不依赖，可并行开发。

---

## 调研结论

### 记忆面板（后端就绪，纯前端）

| 资源 | 状态 | 路径 |
|------|------|------|
| `query_memory` API 客户端 | ✅ 已有 | `frontend/src/api/hr.rs:145-152` |
| `search_memory` API 客户端 | ✅ 已有 | `frontend/src/api/hr.rs:132-143` |
| `QueryMemoryResponse` DTO | ✅ 已有 | `common/src/api/neural_tools.rs`（字段：`results: Vec<MemoryResult>`） |
| `MemoryResult` 结构 | ✅ 已有 | id, content, memory_type, score, summary, source_node_id, target_node_id, relation_type |
| Agent 详情页 | ✅ 已有 | `frontend/src/pages/hr/agent_detail.rs`（6 个 section） |

### 对话体验打磨（纯前端）

| 功能 | 当前状态 |
|------|----------|
| 消息气泡 | ✅ 已有（user/agent/system 三种角色 |
| 附件展示 | ✅ 已有（图片内联、文件下载） |
| 工具调用卡片 | ✅ 已有（可折叠展开） |
| 消息复制 | ❌ 缺失 |
| 快捷指令（slash commands） | ❌ 缺失 |
| 代码块高亮/复制 | ❌ 缺失 |
| SSE 实时推送 | ✅ 已有 |

---

## 文件结构总览

| 文件 | 子系统 | 操作 |
|------|--------|------|
| `frontend/src/pages/hr/agent_memory_panel.rs` | A | 创建 | Agent 记忆面板组件（Tab切换+列表展示） |
| `frontend/src/pages/hr/agent_detail.rs` | A | 修改 | 集成记忆面板为第七个 section |
| `frontend/src/pages/hr/mod.rs` | A | 修改 | 导出新模块 |
| `frontend/src/pages/message/chat.rs` | B | 修改 | 对话体验打磨（消息复制、快捷指令） |
| `frontend/index.html` | A+B | 修改 | 补充 CSS 样式 |

---

# Subsystem A: Agent 记忆面板

## Task A1: 记忆面板组件

**Files:**
- Create: `frontend/src/pages/hr/agent_memory_panel.rs`

### Step 1.1: 创建记忆面板组件

创建 `frontend/src/pages/hr/agent_memory_panel.rs`：

```rust
//! Agent 记忆面板 - 按类型 Tab 切换展示

use crate::api::hr::{query_memory, search_memory};
use crate::components::state::{EmptyState, Loading};
use crate::store::toast::use_toast;
use common::api::MemoryResult;
use dioxus::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MemoryTab {
    ShortTerm,
    Knowledge,
    Relations,
}

fn tab_label(tab: MemoryTab) -> &'static str {
    match tab {
        MemoryTab::ShortTerm => "短期记忆",
        MemoryTab::Knowledge => "知识节点",
        MemoryTab::Relations => "关系",
    }
}

fn tab_memory_type(tab: MemoryTab) -> &'static str {
    match tab {
        MemoryTab::ShortTerm => "short_term",
        MemoryTab::Knowledge => "knowledge_node",
        MemoryTab::Relations => "relation",
    }
}

fn format_time(timestamp: i64) -> String {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_opt(timestamp / 1000, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| timestamp.to_string())
}

fn truncate_text(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        text.to_string()
    } else {
        let mut s: String = text.chars().take(max_len).collect();
        format!("{}...", s)
    }
}

#[component]
pub fn AgentMemoryPanel(agent_id: String) -> Element {
    let mut active_tab = use_signal(|| MemoryTab::ShortTerm);
    let mut memories = use_signal(Vec::<MemoryResult>::new);
    let mut loading = use_signal(|| true);
    let mut search_keyword = use_signal(String::new);
    let toast = use_toast();

    let agent_id_for_load = agent_id.clone();
    let mut load_memories = move || {
        loading.set(true);
        let aid = agent_id_for_load.clone();
        let tab = active_tab();
        let keyword = search_keyword();
        spawn(async move {
            let mtype = tab_memory_type(tab);
            let result = if keyword.trim().is_empty() {
                query_memory(Some(&aid), Some(mtype)).await
            } else {
                search_memory(&keyword, Some(mtype)).await
            };
            match result {
                Ok(resp) => memories.set(resp.results),
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    };

    use_effect(move || {
        load_memories();
    });

    let memories_list = memories.read().clone();
    let count = memories_list.len();

    rsx! {
        div { class: "memory-panel",
            // Tab 切换
            div { class: "memory-tabs",
                for tab in [MemoryTab::ShortTerm, MemoryTab::Knowledge, MemoryTab::Relations] {
                    {
                        let is_active = active_tab() == tab;
                        let label = tab_label(tab);
                        rsx! {
                            button {
                                class: if is_active { "memory-tab active" } else { "memory-tab" },
                                onclick: move |_| {
                                    active_tab.set(tab);
                                    load_memories();
                                },
                                "{label}"
                            }
                        }
                    }
                }
            }

            // 搜索框
            div { class: "memory-search",
                input {
                    class: "form-input",
                    r#type: "text",
                    placeholder: "搜索记忆...",
                    value: search_keyword,
                    oninput: move |e| search_keyword.set(e.value().clone()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter {
                            e.prevent_default();
                            load_memories();
                        }
                    },
                }
                button {
                    class: "btn btn-secondary btn-sm",
                    onclick: move |_| load_memories(),
                    "搜索"
                }
            }

            // 计数
            div { class: "memory-count",
                span { class: "text-muted text-sm", "共 {count} 条" }
            }

            // 记忆列表
            if loading() {
                div { class: "memory-list", Loading {} }
            } else if memories_list.is_empty() {
                div { class: "memory-list",
                    EmptyState { icon: "🧠".to_string(), message: "暂无记忆记录".to_string() }
                }
            } else {
                div { class: "memory-list",
                    for mem in memories_list.iter() {
                        {
                            let mem_clone = mem.clone();
                            let mem_id = mem.id.clone();
                            let content_preview = truncate_text(&mem.content, 150);
                            let score_text = mem.summary.clone().unwrap_or_default();
                            rsx! {
                                div {
                                    class: "memory-item",
                                    key: "{mem_id}",
                                    div { class: "memory-item-header",
                                        span { class: "memory-type-badge", "{mem.memory_type}" }
                                        if let Some(score_val) = mem.score {
                                            span { class: "memory-score", "相似度: {:.2}", score_val }
                                        }
                                    }
                                    div { class: "memory-item-content", "{content_preview}" }
                                    if !score_text.is_empty() {
                                        div { class: "memory-item-summary",
                                            span { class: "text-sm text-muted", "摘要: {score_text}" }
                                        }
                                    }
                                    if mem.memory_type == "relation" {
                                        div { class: "memory-item-meta",
                                            span { class: "text-xs text-muted",
                                                "源: {mem.source_node_id.clone().unwrap_or_default()}"
                                            }
                                            span { class: "text-xs text-muted",
                                                "→ {mem.target_node_id.clone().unwrap_or_default()}"
                                            }
                                            if let Some(rel) = &mem.relation_type {
                                                span { class: "badge badge-info", "{rel}" }
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
}
```

### Step 1.2: 验证编译

```bash
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check 2>&1 | grep -E "^error|Finished" | tail -5
```

**预期**：0 错误。

---

## Task A2: 集成到 Agent 详情页

**Files:**
- Modify: `frontend/src/pages/hr/agent_detail.rs`
- Modify: `frontend/src/pages/hr/mod.rs`

### Step 2.1: 在 mod.rs 中导出模块

修改 `frontend/src/pages/hr/mod.rs`，添加：

```rust
pub mod agent_memory_panel;
```

### Step 2.2: 在 agent_detail.rs 中集成

在 `agent_detail.rs` 中：

1. 顶部 import 添加：
```rust
use crate::pages::hr::agent_memory_panel::AgentMemoryPanel;
```

2. 在"对话" section 之后（第 601 行 `</div>` 之前），添加第七个 section：

```rust
                        div { class: "detail-section",
                            h3 { class: "detail-section-title", "记忆" }
                            AgentMemoryPanel { agent_id: id.clone() }
                        }
```

注意：`id` 在 match 分支内部可用，直接 `agent_id: id.clone()`。

### Step 2.3: 验证编译

```bash
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check 2>&1 | grep -E "^error|Finished" | tail -5
```

**预期**：0 错误。

---

## Task A3: 记忆面板 CSS 样式

**Files:**
- Modify: `frontend/index.html`

### Step 3.1: 添加记忆面板 CSS

在 `</style>` 之前添加：

```css
      /* ===== 记忆面板 ===== */
      .memory-panel {
        display: flex;
        flex-direction: column;
        gap: var(--space-3);
      }

      .memory-tabs {
        display: flex;
        gap: var(--space-2);
        border-bottom: 1px solid var(--color-border);
        padding-bottom: var(--space-2);
      }

      .memory-tab {
        padding: var(--space-2) var(--space-3);
        border: none;
        background: none;
        color: var(--color-text-secondary);
        cursor: pointer;
        border-radius: var(--radius-md);
        font-size: var(--font-sm);
        transition: all 0.15s;
      }

      .memory-tab:hover {
        background-color: var(--color-warm-ivory);
      }

      .memory-tab.active {
        background-color: var(--color-mistral-orange);
        color: white;
        font-weight: 500;
      }

      .memory-search {
        display: flex;
        gap: var(--space-2);
      }

      .memory-search .form-input {
        flex: 1;
      }

      .memory-count {
        padding: 0 var(--space-1);
      }

      .memory-list {
        display: flex;
        flex-direction: column;
        gap: var(--space-2);
        max-height: 400px;
        overflow-y: auto;
      }

      .memory-item {
        padding: var(--space-3);
        background-color: var(--color-pure-white);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        transition: box-shadow 0.15s;
      }

      .memory-item:hover {
        box-shadow: var(--shadow-sm);
      }

      .memory-item-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: var(--space-2);
      }

      .memory-type-badge {
        font-size: 11px;
        font-weight: 500;
        padding: 2px 8px;
        border-radius: var(--radius-full);
        background-color: var(--color-warm-ivory);
        color: var(--color-mistral-orange);
      }

      .memory-score {
        font-size: 12px;
        color: var(--color-text-secondary);
      }

      .memory-item-content {
        font-size: var(--font-sm);
        color: var(--color-text-primary);
        line-height: 1.6;
        margin-bottom: var(--space-2);
        white-space: pre-wrap;
        word-break: break-word;
      }

      .memory-item-summary {
        margin-bottom: var(--space-1);
      }

      .memory-item-meta {
        display: flex;
        gap: var(--space-3);
        align-items: center;
        flex-wrap: wrap;
      }
```

### Step 3.2: 验证编译

```bash
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check 2>&1 | grep -E "^error|Finished" | tail -5
```

**预期**：0 错误。

---

# Subsystem B: 对话体验打磨

## Task B1: 消息复制功能

**Files:**
- Modify: `frontend/src/pages/message/chat.rs`

### Step 1.1: 添加消息复制按钮

在 `MessageChat` 组件中，为每条消息添加悬浮时显示的复制按钮。

**实现思路**：
- 为消息气泡添加 `message-row` 容器，hover 时显示复制图标
- 点击复制按钮调用 `web_sys::window().unwrap().navigator().clipboard().unwrap().write_text()`
- 复制成功后用 toast 提示

需要注意：`web_sys` 需要 Clipboard API，需在 `Cargo.toml` 的 `web-sys` features 中添加 `Clipboard`、`ClipboardItem`（如果还没有的话）

先检查 `Cargo.toml`：

```bash
grep -n "web-sys" frontend/Cargo.toml
```

如果没有 `Clipboard` feature，需要添加。

### Step 1.2: 验证编译

```bash
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check 2>&1 | grep -E "^error|Finished" | tail -5
```

---

## Task B2: 快捷指令输入（Slash Commands）

**Files:**
- Modify: `frontend/src/pages/message/chat.rs`

### Step 2.1: 添加快捷指令

在输入框中输入 `/` 时显示快捷指令菜单：

| 指令 | 功能 |
|------|------|
| `/clear` | 清空当前对话（本地清空，不删除服务器消息 |
| `/help` | 显示帮助信息 |
| `/reset` | 重置对话上下文 |

实现方式：
- 在 `input_text` 的 `oninput` 中检测是否以 `/` 开头
- 如是，显示快捷指令下拉面板
- 点击指令或按 Tab/Enter 执行

MVP 版本简化方案：先做一个简单的 `/clear` 和 `/help` 两个指令，其他后续可扩展。

### Step 2.2: 验证编译

```bash
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check 2>&1 | grep -E "^error|Finished" | tail -5
```

---

## Task B3: 对话页面 CSS 补充

**Files:**
- Modify: `frontend/index.html`

### Step 3.1: 添加对话相关新样式

在 `</style>` 之前添加：

```css
      /* ===== 消息操作按钮 ===== */
      .message-actions {
        display: flex;
        gap: var(--space-1);
        opacity: 0;
        transition: opacity 0.15s;
      }

      .message-item:hover .message-actions {
        opacity: 1;
      }

      .message-action-btn {
        padding: 2px 6px;
        font-size: 11px;
        border: none;
        background: none;
        color: var(--color-text-secondary);
        cursor: pointer;
        border-radius: var(--radius-sm);
      }

      .message-action-btn:hover {
        background-color: var(--color-warm-ivory);
        color: var(--color-text-primary);
      }

      /* ===== 快捷指令菜单 ===== */
      .slash-menu {
        position: absolute;
        bottom: 100%;
        left: 0;
        right: 0;
        margin-bottom: var(--space-2);
        background-color: var(--color-pure-white);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        box-shadow: var(--shadow-md);
        max-height: 200px;
        overflow-y: auto;
        z-index: 100;
      }

      .slash-menu-item {
        padding: var(--space-2) var(--space-3);
        cursor: pointer;
        display: flex;
        justify-content: space-between;
        align-items: center;
        transition: background-color 0.1s;
      }

      .slash-menu-item:hover,
      .slash-menu-item.active {
        background-color: var(--color-warm-ivory);
      }

      .slash-menu-item .slash-cmd {
        font-weight: 500;
        color: var(--color-mistral-orange);
      }

      .slash-menu-item .slash-desc {
        font-size: var(--font-xs);
        color: var(--color-text-secondary);
      }

      /* ===== 代码块 ===== */
      .message-code-block {
        background-color: #1e1e1e;
        color: #d4d4d4;
        padding: var(--space-3);
        border-radius: var(--radius-md);
        font-family: 'Monaco', 'Menlo', 'Consolas', monospace;
        font-size: 13px;
        line-height: 1.6;
        overflow-x: auto;
        margin: var(--space-2) 0;
        position: relative;
      }

      .code {
        background-color: var(--color-warm-ivory);
        padding: 2px 6px;
        border-radius: var(--radius-sm);
        font-family: 'Monaco', 'Menlo', 'Consolas', monospace;
        font-size: 0.9em;
      }

      .message-code-copy-btn {
        position: absolute;
        top: var(--space-2);
        right: var(--space-2);
        padding: 2px 8px;
        font-size: 11px;
        background: rgba(255,255,255,0.1);
        color: #999;
        border: none;
        border-radius: var(--radius-sm);
        cursor: pointer;
      }

      .message-code-copy-btn:hover {
        background: rgba(255,255,255,0.2);
        color: #fff;
      }
```

### Step 3.2: 验证编译

```bash
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check 2>&1 | grep -E "^error|Finished" | tail -5
```

---

## Task 6: 端到端验证

### Step 6.1: 前端编译检查

```bash
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check 2>&1 | grep -E "^error|Finished"
```

**预期**：0 错误。

### Step 6.2: 后端测试（确保无回归）

```bash
cd /Users/aman/Technology/rust/ai_orz && cargo test --lib --no-fail-fast 2>&1 | tail -5
```

**预期**：697 个测试 100% 通过（纯前端改动不影响后端）。

### Step 6.3: 手动验证清单

**记忆面板**：
1. 进入 Agent 详情页 → 滚动到"记忆" section
2. Tab 切换：短期记忆 / 知识节点 / 关系
3. 搜索框输入关键词 → 点击搜索或回车 → 结果更新
4. 记忆卡片展示：类型徽章、内容预览、摘要（如有）、关系元信息
5. 空状态展示正确

**对话体验**：
1. Hover 消息气泡 → 显示复制按钮 → 点击复制 → toast 提示成功
2. 输入框输入 `/` → 显示快捷指令菜单
3. 点击 `/clear` → 输入框清空/本地消息清空
4. 点击 `/help` → 显示帮助信息

---

## 风险与注意事项

| 风险 | 应对 |
|------|------|
| Dioxus 0.7 RSX 格式字符串限制 | 提取局部变量后再嵌入 rsx! |
| Clipboard API 不可用 | fallback 到 toast.error 提示 |
| 记忆数据为空时 UI 抖动 | 统一使用 Loading + EmptyState |
| 快捷指令与正常输入冲突 | 仅在输入框内容以 `/` 开头且无空格时显示 |

---

## 完成标准

- [ ] Task A1: 记忆面板组件创建成功，编译通过
- [ ] Task A2: 集成到 Agent 详情页，编译通过
- [ ] Task A3: 记忆面板 CSS 样式美观
- [ ] Task B1: 消息复制功能实现
- [ ] Task B2: 快捷指令输入实现
- [ ] Task B3: 对话相关 CSS 样式补充
- [ ] Task 6: 端到端验证通过，前端 0 编译错误，后端测试 100% 通过
