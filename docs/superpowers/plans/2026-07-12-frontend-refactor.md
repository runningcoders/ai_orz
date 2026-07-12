# AI Orz 前端重构实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 重构 AI Orz 前端，建立 CSS 设计系统、统一 API 层、全局状态管理、基础组件库，并按功能模块补齐所有基础 CRUD 页面，产出可用版本。

**Architecture:** 保持 Dioxus 0.7 (Rust/WASM) 技术栈，引入全局 CSS 设计系统（Mistral 暖色调，CSS 变量 + 组件类）替代内联样式；引入 Dioxus Router 替代 signal 状态切换；建立统一 API 客户端（含 JWT 注入、错误处理）；按 organization/hr/finance/project/message/system 六大业务域组织页面。

**Tech Stack:** Rust + Dioxus 0.7 + reqwest + web-sys + common crate（与后端共享 DTO）

---

## 文件结构总览

重构后的目录结构（按功能模块组织，与后端 Handler 域对齐）：

```
frontend/
├── Cargo.toml                    # 新增 tokio、chrono 依赖
├── index.html                    # 引入全局 CSS 设计系统
├── build.rs                      # 保持不变
└── src/
    ├── main.rs                   # 入口：Router 配置
    ├── config.rs                 # 保持不变（配置管理）
    │
    ├── api/                      # API 客户端层
    │   ├── mod.rs                # 统一 HTTP 客户端 client()、错误处理
    │   ├── auth.rs               # 认证 API（login/logout/check_initialized）
    │   ├── organization.rs       # 组织管理 API
    │   ├── hr.rs                 # HR 域 API（agent/skill/tool-pack）
    │   ├── finance.rs            # Finance 域 API（model_provider/tool/attachment/mcp）
    │   ├── project.rs            # Project 域 API（project/task/artifact）
    │   ├── message.rs            # Message 域 API（message_channel）
    │   └── system.rs             # System 域 API（health/cron_trigger）
    │
    ├── store/                    # 全局状态管理
    │   ├── mod.rs
    │   └── auth.rs               # 认证状态（token/user_info/role）
    │
    ├── components/               # 基础 UI 组件库
    │   ├── mod.rs
    │   ├── button.rs             # Button 组件
    │   ├── card.rs               # Card 组件
    │   ├── modal.rs              # Modal 对话框
    │   ├── table.rs              # Table 表格
    │   ├── input.rs              # Input/Textarea/Select 组件
    │   ├── badge.rs              # Badge 徽章
    │   ├── toast.rs              # Toast 提示
    │   └── empty_state.rs        # 空状态/加载/错误状态
    │
    ├── layouts/                  # 布局组件
    │   ├── mod.rs
    │   ├── navbar.rs             # 顶部导航栏
    │   └── app_layout.rs         # 应用主布局（Navbar + Content）
    │
    └── pages/                    # 页面组件（按业务域分组）
        ├── mod.rs
        ├── reception.rs          # 前台接待（登录/初始化）
        │
        ├── organization/         # 组织模块
        │   ├── mod.rs
        │   ├── info.rs           # 组织信息
        │   └── users.rs          # 用户管理
        │
        ├── hr/                   # 人力资源模块
        │   ├── mod.rs
        │   ├── agents.rs         # Agent 管理列表
        │   ├── agent_detail.rs   # Agent 详情（工具包/技能包）
        │   └── skills.rs         # 技能库管理
        │
        ├── finance/              # 财务模块
        │   ├── mod.rs
        │   ├── model_providers.rs # 模型提供商管理
        │   ├── tools.rs          # 工具管理
        │   ├── attachments.rs    # 附件管理
        │   └── mcp_servers.rs    # MCP 服务器管理
        │
        ├── project/              # 项目模块
        │   ├── mod.rs
        │   ├── projects.rs       # 项目列表
        │   ├── project_detail.rs # 项目详情（含任务）
        │   └── tasks.rs          # 任务管理
        │
        ├── message/              # 消息模块
        │   ├── mod.rs
        │   ├── channels.rs       # 消息渠道管理
        │   └── chat.rs           # 对话界面
        │
        ├── system/               # 系统模块
        │   ├── mod.rs
        │   ├── triggers.rs       # 定时触发器管理
        │   └── health.rs         # 健康检查
        │
        ├── user/                 # 用户个人
        │   ├── mod.rs
        │   └── profile.rs        # 个人信息
        │
        └── settings.rs           # 系统设置
```

---

## Task 1: 建立全局 CSS 设计系统

**Files:**
- Modify: `frontend/index.html`

**目标：** 在 index.html 中建立 Mistral 暖色调 CSS 设计系统，包含 CSS 变量、基础 reset、通用工具类和组件类，替代所有内联样式。

- [ ] **Step 1: 重写 index.html，注入全局 CSS 设计系统**

将 `frontend/index.html` 完整替换为以下内容（包含 Mistral 色彩变量、reset、布局工具类、组件类）：

```html
<!DOCTYPE html>
<html lang="zh-CN">
  <head>
    <meta charset="utf-8" />
    <title>AI Orz - AI 代理执行框架</title>
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <style>
      /* ===== Mistral Design System CSS Variables ===== */
      :root {
        /* Brand Colors */
        --color-mistral-orange: #fa520f;
        --color-mistral-flame: #fb6424;
        --color-block-orange: #ff8105;
        --color-sunshine-900: #ff8a00;
        --color-sunshine-700: #ffa110;
        --color-sunshine-500: #ffb83e;
        --color-sunshine-300: #ffd06a;
        --color-block-gold: #ffe295;
        --color-bright-yellow: #ffd900;

        /* Surface & Background */
        --color-warm-ivory: #fffaeb;
        --color-cream: #fff0c2;
        --color-pure-white: #ffffff;
        --color-mistral-black: #1f1f1f;
        --color-black-tint: #3d3d3d;

        /* Semantic Colors */
        --color-success: #2ecc71;
        --color-success-bg: #e8f8f0;
        --color-warning: #f39c12;
        --color-warning-bg: #fef5e7;
        --color-error: #e74c3c;
        --color-error-bg: #fdeaea;
        --color-info: #3498db;
        --color-info-bg: #ebf5fb;

        /* Text Colors */
        --color-text-primary: #1f1f1f;
        --color-text-secondary: #666666;
        --color-text-muted: #999999;
        --color-text-on-dark: #ffffff;

        /* Border Colors */
        --color-border: #e0d6c0;
        --color-border-light: #f0e8d4;
        --color-border-focus: #fa520f;

        /* Typography */
        --font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
        --font-mono: 'SF Mono', Monaco, 'Cascadia Code', 'Roboto Mono', monospace;

        /* Spacing (8px base) */
        --space-1: 4px;
        --space-2: 8px;
        --space-3: 12px;
        --space-4: 16px;
        --space-5: 20px;
        --space-6: 24px;
        --space-8: 32px;
        --space-10: 40px;
        --space-12: 48px;

        /* Border Radius */
        --radius-sm: 2px;
        --radius-md: 4px;
        --radius-lg: 8px;
        --radius-xl: 12px;

        /* Shadows - Warm Golden Shadow System */
        --shadow-sm: 0 1px 3px rgba(127, 99, 21, 0.08);
        --shadow-md: 0 2px 8px rgba(127, 99, 21, 0.10), 0 1px 2px rgba(127, 99, 21, 0.06);
        --shadow-lg: 0 4px 16px rgba(127, 99, 21, 0.12), 0 2px 4px rgba(127, 99, 21, 0.08);
        --shadow-xl: -8px 16px 39px rgba(127, 99, 21, 0.12), -33px 64px 72px rgba(127, 99, 21, 0.10);

        /* Layout */
        --navbar-height: 56px;
        --max-width: 1200px;
      }

      /* ===== Reset ===== */
      * { margin: 0; padding: 0; box-sizing: border-box; }
      html, body { min-height: 100vh; }
      body {
        font-family: var(--font-family);
        background-color: var(--color-warm-ivory);
        color: var(--color-text-primary);
        font-size: 15px;
        line-height: 1.5;
      }

      /* ===== Layout Utilities ===== */
      .app-container { min-height: 100vh; background-color: var(--color-warm-ivory); }
      .content-area { max-width: var(--max-width); margin: 0 auto; padding: var(--space-8) var(--space-4); }
      .flex { display: flex; }
      .flex-col { flex-direction: column; }
      .items-center { align-items: center; }
      .justify-between { justify-content: space-between; }
      .justify-center { justify-content: center; }
      .gap-2 { gap: var(--space-2); }
      .gap-3 { gap: var(--space-3); }
      .gap-4 { gap: var(--space-4); }
      .gap-6 { gap: var(--space-6); }
      .w-full { width: 100%; }
      .text-center { text-align: center; }
      .text-right { text-align: right; }

      /* ===== Navbar ===== */
      .navbar {
        background-color: var(--color-mistral-black);
        height: var(--navbar-height);
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 0 var(--space-6);
        position: sticky;
        top: 0;
        z-index: 100;
        box-shadow: var(--shadow-md);
      }
      .navbar-brand {
        color: var(--color-bright-yellow);
        font-size: 18px;
        font-weight: 700;
        letter-spacing: -0.5px;
        cursor: pointer;
      }
      .navbar-section { display: flex; align-items: center; gap: var(--space-2); }
      .navbar-item {
        color: #ecf0f1;
        background: transparent;
        border: none;
        padding: var(--space-2) var(--space-3);
        border-radius: var(--radius-md);
        cursor: pointer;
        font-size: 14px;
        transition: background-color 0.2s;
        display: flex;
        align-items: center;
        gap: var(--space-1);
      }
      .navbar-item:hover { background-color: rgba(255,255,255,0.1); }
      .navbar-dropdown {
        position: absolute;
        top: 100%;
        left: 0;
        margin-top: var(--space-1);
        background: var(--color-pure-white);
        border-radius: var(--radius-md);
        box-shadow: var(--shadow-lg);
        min-width: 180px;
        overflow: hidden;
        z-index: 200;
      }
      .navbar-dropdown-item {
        display: block;
        padding: var(--space-3) var(--space-4);
        color: var(--color-text-primary);
        text-decoration: none;
        cursor: pointer;
        font-size: 14px;
        transition: background-color 0.2s;
      }
      .navbar-dropdown-item:hover { background-color: var(--color-warm-ivory); }

      /* ===== Button ===== */
      .btn {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        gap: var(--space-2);
        padding: var(--space-3) var(--space-4);
        border: none;
        border-radius: var(--radius-md);
        cursor: pointer;
        font-size: 14px;
        font-weight: 500;
        font-family: inherit;
        transition: all 0.2s;
        text-decoration: none;
        white-space: nowrap;
      }
      .btn:disabled { opacity: 0.5; cursor: not-allowed; }
      .btn-primary {
        background-color: var(--color-mistral-black);
        color: var(--color-text-on-dark);
      }
      .btn-primary:hover:not(:disabled) { background-color: var(--color-black-tint); }
      .btn-accent {
        background-color: var(--color-mistral-orange);
        color: var(--color-text-on-dark);
      }
      .btn-accent:hover:not(:disabled) { background-color: var(--color-mistral-flame); }
      .btn-secondary {
        background-color: var(--color-cream);
        color: var(--color-mistral-black);
      }
      .btn-secondary:hover:not(:disabled) { background-color: var(--color-sunshine-300); }
      .btn-danger {
        background-color: var(--color-error);
        color: var(--color-text-on-dark);
      }
      .btn-danger:hover:not(:disabled) { background-color: #c0392b; }
      .btn-ghost {
        background: transparent;
        color: var(--color-text-primary);
      }
      .btn-ghost:hover:not(:disabled) { background-color: var(--color-border-light); }
      .btn-sm { padding: var(--space-1) var(--space-2); font-size: 13px; }
      .btn-lg { padding: var(--space-4) var(--space-6); font-size: 16px; }

      /* ===== Card ===== */
      .card {
        background: var(--color-pure-white);
        border-radius: var(--radius-lg);
        padding: var(--space-6);
        box-shadow: var(--shadow-md);
      }
      .card-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: var(--space-6);
        padding-bottom: var(--space-4);
        border-bottom: 1px solid var(--color-border-light);
      }
      .card-title { font-size: 20px; font-weight: 600; color: var(--color-text-primary); }

      /* ===== Table ===== */
      .table { width: 100%; border-collapse: collapse; }
      .table th {
        text-align: left;
        padding: var(--space-3) var(--space-3);
        background-color: var(--color-warm-ivory);
        border-bottom: 2px solid var(--color-border);
        font-size: 13px;
        font-weight: 600;
        color: var(--color-text-secondary);
        text-transform: uppercase;
        letter-spacing: 0.5px;
      }
      .table td {
        padding: var(--space-3) var(--space-3);
        border-bottom: 1px solid var(--color-border-light);
        font-size: 14px;
        color: var(--color-text-primary);
      }
      .table tbody tr:hover { background-color: var(--color-warm-ivory); }

      /* ===== Form ===== */
      .form-group { margin-bottom: var(--space-4); }
      .form-label {
        display: block;
        margin-bottom: var(--space-2);
        font-size: 14px;
        font-weight: 500;
        color: var(--color-text-primary);
      }
      .form-input, .form-textarea, .form-select {
        width: 100%;
        padding: var(--space-3);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        font-size: 14px;
        font-family: inherit;
        background-color: var(--color-pure-white);
        transition: border-color 0.2s, box-shadow 0.2s;
        outline: none;
      }
      .form-input:focus, .form-textarea:focus, .form-select:focus {
        border-color: var(--color-border-focus);
        box-shadow: 0 0 0 3px rgba(250, 82, 15, 0.1);
      }
      .form-textarea { min-height: 80px; resize: vertical; }
      .form-hint { font-size: 12px; color: var(--color-text-muted); margin-top: var(--space-1); }

      /* ===== Badge ===== */
      .badge {
        display: inline-flex;
        align-items: center;
        padding: 2px var(--space-2);
        border-radius: var(--radius-md);
        font-size: 12px;
        font-weight: 500;
      }
      .badge-success { background-color: var(--color-success-bg); color: var(--color-success); }
      .badge-warning { background-color: var(--color-warning-bg); color: var(--color-warning); }
      .badge-error { background-color: var(--color-error-bg); color: var(--color-error); }
      .badge-info { background-color: var(--color-info-bg); color: var(--color-info); }
      .badge-neutral { background-color: var(--color-border-light); color: var(--color-text-secondary); }

      /* ===== Modal ===== */
      .modal-overlay {
        position: fixed;
        top: 0; left: 0; right: 0; bottom: 0;
        background-color: rgba(31, 31, 31, 0.5);
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 1000;
      }
      .modal-content {
        background: var(--color-pure-white);
        border-radius: var(--radius-xl);
        padding: var(--space-6);
        width: 500px;
        max-width: 90vw;
        max-height: 90vh;
        overflow-y: auto;
        box-shadow: var(--shadow-xl);
      }
      .modal-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: var(--space-6);
      }
      .modal-title { font-size: 18px; font-weight: 600; color: var(--color-text-primary); }
      .modal-close {
        background: none;
        border: none;
        font-size: 24px;
        cursor: pointer;
        color: var(--color-text-muted);
        padding: 0;
        line-height: 1;
      }
      .modal-footer {
        display: flex;
        gap: var(--space-3);
        justify-content: flex-end;
        margin-top: var(--space-6);
      }

      /* ===== Alert / Toast ===== */
      .alert {
        padding: var(--space-3) var(--space-4);
        border-radius: var(--radius-md);
        margin-bottom: var(--space-4);
        font-size: 14px;
      }
      .alert-error { background-color: var(--color-error-bg); border: 1px solid var(--color-error); color: var(--color-error); }
      .alert-success { background-color: var(--color-success-bg); border: 1px solid var(--color-success); color: var(--color-success); }
      .alert-warning { background-color: var(--color-warning-bg); border: 1px solid var(--color-warning); color: var(--color-warning); }
      .alert-info { background-color: var(--color-info-bg); border: 1px solid var(--color-info); color: var(--color-info); }

      /* ===== State Indicators ===== */
      .state-loading { text-align: center; padding: var(--space-12); color: var(--color-text-secondary); }
      .state-empty { text-align: center; padding: var(--space-12) var(--space-6); color: var(--color-text-secondary); }
      .state-empty-icon { font-size: 48px; margin-bottom: var(--space-4); }

      /* ===== Misc ===== */
      .text-mono { font-family: var(--font-mono); font-size: 13px; }
      .text-muted { color: var(--color-text-muted); }
      .text-secondary { color: var(--color-text-secondary); }
      .mt-2 { margin-top: var(--space-2); }
      .mt-4 { margin-top: var(--space-4); }
      .mt-6 { margin-top: var(--space-6); }
      .mb-2 { margin-bottom: var(--space-2); }
      .mb-4 { margin-bottom: var(--space-4); }
      .mb-6 { margin-bottom: var(--space-6); }
    </style>
  </head>
  <body>
    <div id="main"></div>
    <script type="module">
      import init from "./pkg/frontend.js";
      init();
    </script>
  </body>
</html>
```

- [ ] **Step 2: 验证 index.html 修改无误**

运行：`cd /Users/aman/Technology/rust/ai_orz && head -5 frontend/index.html`
预期：输出 `<!DOCTYPE html>` 开头

- [ ] **Step 3: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/index.html
git commit -m "feat(frontend): establish Mistral CSS design system with warm color variables and component classes"
```

---

## Task 2: 建立统一 API 客户端基础

**Files:**
- Modify: `frontend/src/api/mod.rs`

**目标：** 建立统一 HTTP 客户端，自动注入 JWT token、统一错误处理、复用连接池。

- [ ] **Step 1: 重写 api/mod.rs 建立统一 HTTP 客户端**

将 `frontend/src/api/mod.rs` 完整替换为：

```rust
//! API 客户端模块 - 统一 HTTP 客户端、JWT 注入、错误处理

pub mod auth;
pub mod finance;
pub mod hr;
pub mod message;
pub mod organization;
pub mod project;
pub mod system;

use common::api::ApiResponse;
use reqwest::{Client, Method, RequestBuilder};
use std::sync::OnceLock;

use crate::config::current_config;

/// 全局 HTTP 客户端单例（复用连接池）
static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

/// 获取全局 HTTP 客户端
pub fn client() -> &'static Client {
    HTTP_CLIENT.get_or_init(|| Client::new())
}

/// 从 localStorage 获取 JWT token
fn get_token() -> Option<String> {
    let window = web_sys::window()?;
    let storage = window.local_storage().ok()??;
    storage.get("ai_orz_token").ok()?
}

/// 构建带 JWT 的请求
fn build_request(method: Method, path: &str) -> RequestBuilder {
    let url = current_config().api_url(path);
    let req = client().request(method, &url);
    match get_token() {
        Some(token) if !token.is_empty() => req.bearer_auth(&token),
        _ => req,
    }
}

/// 发送 GET 请求并解析 ApiResponse<T>
pub async fn api_get<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, String> {
    let resp = build_request(Method::GET, path).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let api_resp: ApiResponse<T> = resp.json().await.map_err(|e| e.to_string())?;
    if !api_resp.is_success() {
        return Err(api_resp.message);
    }
    api_resp.data.ok_or_else(|| "响应数据为空".to_string())
}

/// 发送 GET 请求，返回可选数据（用于列表可能为空的场景）
pub async fn api_get_or_default<T: serde::de::DeserializeOwned + Default>(path: &str) -> Result<T, String> {
    let resp = build_request(Method::GET, path).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let api_resp: ApiResponse<T> = resp.json().await.map_err(|e| e.to_string())?;
    if !api_resp.is_success() {
        return Err(api_resp.message);
    }
    Ok(api_resp.data.unwrap_or_default())
}

/// 发送 POST 请求
pub async fn api_post<T: serde::de::DeserializeOwned, B: serde::Serialize>(path: &str, body: &B) -> Result<T, String> {
    let resp = build_request(Method::POST, path).json(body).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let api_resp: ApiResponse<T> = resp.json().await.map_err(|e| e.to_string())?;
    if !api_resp.is_success() {
        return Err(api_resp.message);
    }
    api_resp.data.ok_or_else(|| "响应数据为空".to_string())
}

/// 发送 POST 请求（无响应体）
pub async fn api_post_empty<B: serde::Serialize>(path: &str, body: &B) -> Result<(), String> {
    let resp = build_request(Method::POST, path).json(body).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let api_resp: ApiResponse<common::api::EmptyResponse> = resp.json().await.map_err(|e| e.to_string())?;
    if !api_resp.is_success() {
        return Err(api_resp.message);
    }
    Ok(())
}

/// 发送 PUT 请求
pub async fn api_put<T: serde::de::DeserializeOwned, B: serde::Serialize>(path: &str, body: &B) -> Result<T, String> {
    let resp = build_request(Method::PUT, path).json(body).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let api_resp: ApiResponse<T> = resp.json().await.map_err(|e| e.to_string())?;
    if !api_resp.is_success() {
        return Err(api_resp.message);
    }
    api_resp.data.ok_or_else(|| "响应数据为空".to_string())
}

/// 发送 PUT 请求（无响应体）
pub async fn api_put_empty<B: serde::Serialize>(path: &str, body: &B) -> Result<(), String> {
    let resp = build_request(Method::PUT, path).json(body).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let api_resp: ApiResponse<common::api::EmptyResponse> = resp.json().await.map_err(|e| e.to_string())?;
    if !api_resp.is_success() {
        return Err(api_resp.message);
    }
    Ok(())
}

/// 发送 DELETE 请求
pub async fn api_delete(path: &str) -> Result<(), String> {
    let resp = build_request(Method::DELETE, path).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let api_resp: ApiResponse<common::api::EmptyResponse> = resp.json().await.map_err(|e| e.to_string())?;
    if !api_resp.is_success() {
        return Err(api_resp.message);
    }
    Ok(())
}

/// 发送纯文本 GET 请求（用于 /health 等非标准 API）
pub async fn api_get_text(path: &str) -> Result<String, String> {
    let url = current_config().api_url(path);
    let resp = client().get(&url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.text().await.map_err(|e| e.to_string())
}
```

- [ ] **Step 2: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/api/mod.rs
git commit -m "feat(frontend): establish unified API client with JWT injection and typed helpers"
```

---

## Task 3: 建立认证状态管理

**Files:**
- Create: `frontend/src/store/mod.rs`
- Create: `frontend/src/store/auth.rs`

**目标：** 建立全局认证状态管理，包含 token 持久化、用户信息、登录态判断。

- [ ] **Step 1: 创建 store/mod.rs**

```rust
//! 全局状态管理模块

pub mod auth;
```

- [ ] **Step 2: 创建 store/auth.rs**

```rust
//! 认证状态管理 - token 持久化、用户信息全局共享

use dioxus::prelude::*;
use web_sys::Storage;

const TOKEN_KEY: &str = "ai_orz_token";

/// 获取 localStorage
fn get_storage() -> Option<Storage> {
    web_sys::window()?.local_storage().ok()?
}

/// 保存 token 到 localStorage
pub fn save_token(token: &str) {
    if let Some(storage) = get_storage() {
        let _ = storage.set(TOKEN_KEY, token);
    }
}

/// 从 localStorage 读取 token
pub fn load_token() -> Option<String> {
    get_storage()?.get(TOKEN_KEY).ok()?
}

/// 清除 token
pub fn clear_token() {
    if let Some(storage) = get_storage() {
        let _ = storage.remove(TOKEN_KEY);
    }
}

/// 判断是否已登录
pub fn is_logged_in() -> bool {
    load_token().is_some()
}

/// 全局认证状态 Signal
/// 在 App 根组件中通过 use_context_provider 初始化
#[derive(Clone, Debug, Default)]
pub struct AuthState {
    pub token: Option<String>,
    pub username: String,
    pub role: i32,
    pub org_id: String,
    pub org_name: String,
}

impl AuthState {
    /// 从 localStorage 恢复状态
    pub fn restore() -> Self {
        Self {
            token: load_token(),
            ..Default::default()
        }
    }

    pub fn is_logged_in(&self) -> bool {
        self.token.is_some()
    }

    pub fn is_admin(&self) -> bool {
        self.role >= 2
    }
}

/// 在根组件初始化全局认证状态
/// 使用方式：let auth = use_context_provider(|| Signal::new(AuthState::restore()));
pub fn use_auth_state() -> Signal<AuthState> {
    use_context()
}
```

- [ ] **Step 3: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/store/
git commit -m "feat(frontend): add global auth state management with token persistence"
```

---

## Task 4: 实现各业务域 API 客户端

**Files:**
- Create: `frontend/src/api/auth.rs`
- Create: `frontend/src/api/organization.rs` (覆盖)
- Create: `frontend/src/api/hr.rs`
- Create: `frontend/src/api/finance.rs`
- Create: `frontend/src/api/project.rs`
- Create: `frontend/src/api/message.rs`
- Create: `frontend/src/api/system.rs`

**目标：** 为所有业务域实现 API 客户端，使用 Task 2 建立的统一 helper 函数。

- [ ] **Step 1: 创建 api/auth.rs**

```rust
//! 认证相关 API

use common::api::{
    CheckInitializedResponse, InitializeSystemRequest, InitializeSystemResponse, LoginRequest,
    LoginResponse,
};

use super::{api_get, api_post};

pub async fn check_initialized() -> Result<CheckInitializedResponse, String> {
    api_get("/api/v1/organization/initialize/check").await
}

pub async fn initialize_system(req: InitializeSystemRequest) -> Result<InitializeSystemResponse, String> {
    api_post("/api/v1/organization/initialize", &req).await
}

pub async fn login(req: LoginRequest) -> Result<LoginResponse, String> {
    api_post("/api/v1/organization/auth/login", &req).await
}

pub async fn logout() -> Result<(), String> {
    // logout 不需要返回数据，但后端返回 ApiResponse<EmptyResponse>
    let req = serde_json::json!({});
    super::api_post_empty("/api/v1/organization/auth/logout", &req).await
}
```

- [ ] **Step 2: 创建 api/organization.rs（覆盖旧文件）**

```rust
//! 组织管理 API

use common::api::{
    CreateOrganizationUserRequest, CreateOrganizationUserResponse, GetOrganizationResponse,
    ListOrganizationsResponse, ListUsersResponse, UpdateOrganizationRequest,
    UpdateOrganizationResponse, UpdateUserRequest, UpdateUserResponse,
};

use super::{api_get, api_get_or_default, api_post, api_put};

/// 公开获取组织列表（无需登录，登录页用）
pub async fn list_organizations_public() -> Result<ListOrganizationsResponse, String> {
    api_get("/api/v1/organization/list").await
}

/// 获取当前组织信息
pub async fn get_current_organization() -> Result<GetOrganizationResponse, String> {
    api_get("/api/v1/organization/me").await
}

/// 更新当前组织信息
pub async fn update_current_organization(req: UpdateOrganizationRequest) -> Result<UpdateOrganizationResponse, String> {
    api_put("/api/v1/organization/me", &req).await
}

/// 获取当前组织用户列表
pub async fn list_users() -> Result<ListUsersResponse, String> {
    api_get_or_default("/api/v1/organization/user/me/list").await
}

/// 创建用户
pub async fn create_user(req: CreateOrganizationUserRequest) -> Result<CreateOrganizationUserResponse, String> {
    api_post("/api/v1/organization/user/", &req).await
}

/// 更新用户
pub async fn update_user(req: UpdateUserRequest) -> Result<UpdateUserResponse, String> {
    api_put("/api/v1/organization/user/update", &req).await
}

/// 删除用户
pub async fn delete_user(user_id: &str) -> Result<(), String> {
    super::api_delete(&format!("/api/v1/organization/user/id/{}", user_id)).await
}
```

- [ ] **Step 3: 创建 api/hr.rs**

```rust
//! HR 域 API - Agent 管理、技能管理、工具包/技能包管理

use common::api::{
    CreateAgentRequest, CreateAgentResponse, CreateSkillRequest, CreateSkillResponse,
    DeleteSkillResponse, GetAgentResponse, GetSkillResponse, ListAgentsResponse,
    ListInstalledSkillPacksResponse, ListInstalledToolPacksResponse, ListSkillsResponse,
    UpdateAgentRequest, UpdateAgentResponse, UpdateSkillRequest, UpdateSkillResponse,
};

use super::{api_delete, api_get, api_get_or_default, api_post, api_put};

// ===== Agent 管理 =====

pub async fn list_agents() -> Result<ListAgentsResponse, String> {
    api_get_or_default("/api/v1/hr/agents").await
}

pub async fn get_agent(id: &str) -> Result<GetAgentResponse, String> {
    api_get(&format!("/api/v1/hr/agents/{}", id)).await
}

pub async fn create_agent(req: CreateAgentRequest) -> Result<CreateAgentResponse, String> {
    api_post("/api/v1/hr/agents", &req).await
}

pub async fn update_agent(id: &str, req: UpdateAgentRequest) -> Result<UpdateAgentResponse, String> {
    api_put(&format!("/api/v1/hr/agents/{}", id), &req).await
}

pub async fn update_agent_status(id: &str, status: i32) -> Result<(), String> {
    let body = serde_json::json!({ "status": status });
    super::api_put_empty(&format!("/api/v1/hr/agents/{}/status", id), &body).await
}

pub async fn delete_agent(id: &str) -> Result<(), String> {
    api_delete(&format!("/api/v1/hr/agents/{}", id)).await
}

// ===== Agent 工具包管理 =====

pub async fn list_installed_tool_packs(agent_id: &str) -> Result<ListInstalledToolPacksResponse, String> {
    api_get_or_default(&format!("/api/v1/hr/agents/{}/tool-packs", agent_id)).await
}

pub async fn install_tool_pack(agent_id: &str, tag: &str) -> Result<(), String> {
    let body = serde_json::json!({});
    super::api_post_empty(&format!("/api/v1/hr/agents/{}/tool-packs/{}", agent_id, tag), &body).await
}

pub async fn uninstall_tool_pack(agent_id: &str, tag: &str) -> Result<(), String> {
    api_delete(&format!("/api/v1/hr/agents/{}/tool-packs/{}", agent_id, tag)).await
}

// ===== Agent 技能包管理 =====

pub async fn list_installed_skill_packs(agent_id: &str) -> Result<ListInstalledSkillPacksResponse, String> {
    api_get_or_default(&format!("/api/v1/hr/agents/{}/skill-packs", agent_id)).await
}

pub async fn install_skill_pack(agent_id: &str, tag: &str) -> Result<(), String> {
    let body = serde_json::json!({});
    super::api_post_empty(&format!("/api/v1/hr/agents/{}/skill-packs/{}", agent_id, tag), &body).await
}

pub async fn uninstall_skill_pack(agent_id: &str, tag: &str) -> Result<(), String> {
    api_delete(&format!("/api/v1/hr/agents/{}/skill-packs/{}", agent_id, tag)).await
}

// ===== 技能库管理 =====

pub async fn list_skills() -> Result<ListSkillsResponse, String> {
    api_get_or_default("/api/v1/hr/skills").await
}

pub async fn get_skill(id: &str) -> Result<GetSkillResponse, String> {
    api_get(&format!("/api/v1/hr/skills/{}", id)).await
}

pub async fn create_skill(req: CreateSkillRequest) -> Result<CreateSkillResponse, String> {
    api_post("/api/v1/hr/skills", &req).await
}

pub async fn update_skill(id: &str, req: UpdateSkillRequest) -> Result<UpdateSkillResponse, String> {
    api_put(&format!("/api/v1/hr/skills/{}", id), &req).await
}

pub async fn delete_skill(id: &str) -> Result<DeleteSkillResponse, String> {
    // DELETE 返回 ApiResponse<DeleteSkillResponse>，需要走 json 解析
    let resp = super::client()
        .delete(&crate::config::current_config().api_url(&format!("/api/v1/hr/skills/{}", id)));
    let resp = match get_token_bearer(resp).send().await {
        Ok(r) => r,
        Err(e) => return Err(e.to_string()),
    };
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let api_resp: common::api::ApiResponse<DeleteSkillResponse> = resp.json().await.map_err(|e| e.to_string())?;
    if !api_resp.is_success() {
        return Err(api_resp.message);
    }
    api_resp.data.ok_or_else(|| "响应数据为空".to_string())
}

fn get_token_bearer(req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    if let Some(token) = crate::store::auth::load_token() {
        req.bearer_auth(&token)
    } else {
        req
    }
}
```

- [ ] **Step 4: 创建 api/finance.rs**

```rust
//! Finance 域 API - 模型提供商、工具、附件、MCP 服务器、消息渠道

use common::api::{
    CreateModelProviderRequest, CreateModelProviderResponse, CreateToolRequest,
    CreateToolResponse, DeleteModelProviderResponse, DeleteToolResponse,
    GetModelProviderResponse, GetToolResponse, ListModelProvidersResponse, ListToolsResponse,
    TestConnectionResponse, UpdateModelProviderRequest, UpdateModelProviderResponse,
    UpdateToolRequest, UpdateToolResponse,
};

use super::{api_delete, api_get, api_get_or_default, api_post, api_post_empty, api_put, api_put_empty};

// ===== 模型提供商 =====

pub async fn list_model_providers() -> Result<ListModelProvidersResponse, String> {
    api_get_or_default("/api/v1/finance/model-providers").await
}

pub async fn get_model_provider(id: &str) -> Result<GetModelProviderResponse, String> {
    api_get(&format!("/api/v1/finance/model-providers/{}", id)).await
}

pub async fn create_model_provider(req: CreateModelProviderRequest) -> Result<CreateModelProviderResponse, String> {
    api_post("/api/v1/finance/model-providers", &req).await
}

pub async fn update_model_provider(id: &str, req: UpdateModelProviderRequest) -> Result<UpdateModelProviderResponse, String> {
    api_put(&format!("/api/v1/finance/model-providers/{}", id), &req).await
}

pub async fn delete_model_provider(id: &str) -> Result<(), String> {
    api_delete(&format!("/api/v1/finance/model-providers/{}", id)).await
}

pub async fn test_model_provider_connection(id: &str) -> Result<TestConnectionResponse, String> {
    let body = serde_json::json!({});
    api_post(&format!("/api/v1/finance/model-providers/{}/test", id), &body).await
}

// ===== 工具管理 =====

pub async fn list_tools() -> Result<ListToolsResponse, String> {
    api_get_or_default("/api/v1/finance/tools").await
}

pub async fn get_tool(id: &str) -> Result<GetToolResponse, String> {
    api_get(&format!("/api/v1/finance/tools/{}", id)).await
}

pub async fn create_tool(req: CreateToolRequest) -> Result<CreateToolResponse, String> {
    api_post("/api/v1/finance/tools", &req).await
}

pub async fn update_tool(id: &str, req: UpdateToolRequest) -> Result<UpdateToolResponse, String> {
    api_put(&format!("/api/v1/finance/tools/{}", id), &req).await
}

pub async fn update_tool_status(id: &str, status: i32) -> Result<(), String> {
    let body = serde_json::json!({ "status": status });
    api_put_empty(&format!("/api/v1/finance/tools/{}/status", id), &body).await
}

pub async fn delete_tool(id: &str) -> Result<(), String> {
    api_delete(&format!("/api/v1/finance/tools/{}", id)).await
}

// ===== 消息渠道 =====

pub async fn list_message_channels() -> Result<common::api::ListMessageChannelsResponse, String> {
    api_get_or_default("/api/v1/finance/message-channels").await
}

pub async fn create_message_channel(req: common::api::CreateMessageChannelRequest) -> Result<common::api::CreateMessageChannelResponse, String> {
    api_post("/api/v1/finance/message-channels", &req).await
}

pub async fn update_message_channel_status(id: &str, status: i32) -> Result<(), String> {
    let body = serde_json::json!({ "status": status });
    api_put_empty(&format!("/api/v1/finance/message-channels/{}/status", id), &body).await
}

pub async fn delete_message_channel(id: &str) -> Result<(), String> {
    api_delete(&format!("/api/v1/finance/message-channels/{}", id)).await
}
```

- [ ] **Step 5: 创建 api/project.rs**

```rust
//! Project 域 API - 项目管理、任务管理

use common::api::{
    CreateProjectRequest, CreateProjectResponse, CreateTaskRequest, CreateTaskResponse,
    GetProjectResponse, GetTaskResponse, ListProjectsResponse, ListTasksResponse,
    UpdateProjectRequest, UpdateProjectResponse, UpdateTaskRequest, UpdateTaskResponse,
};

use super::{api_get, api_get_or_default, api_post, api_put, api_put_empty};

// ===== 项目管理 =====

pub async fn list_projects() -> Result<ListProjectsResponse, String> {
    api_get_or_default("/api/v1/projects").await
}

pub async fn get_project(id: &str) -> Result<GetProjectResponse, String> {
    api_get(&format!("/api/v1/projects/{}", id)).await
}

pub async fn create_project(req: CreateProjectRequest) -> Result<CreateProjectResponse, String> {
    api_post("/api/v1/projects", &req).await
}

pub async fn update_project(id: &str, req: UpdateProjectRequest) -> Result<UpdateProjectResponse, String> {
    api_put(&format!("/api/v1/projects/{}", id), &req).await
}

pub async fn update_project_status(id: &str, status: i32) -> Result<(), String> {
    let body = serde_json::json!({ "status": status });
    api_put_empty(&format!("/api/v1/projects/{}/status", id), &body).await
}

// ===== 任务管理 =====

pub async fn list_project_tasks(project_id: &str) -> Result<ListTasksResponse, String> {
    api_get_or_default(&format!("/api/v1/projects/{}/tasks", project_id)).await
}

pub async fn get_task(id: &str) -> Result<GetTaskResponse, String> {
    api_get(&format!("/api/v1/tasks/{}", id)).await
}

pub async fn create_task(req: CreateTaskRequest) -> Result<CreateTaskResponse, String> {
    api_post("/api/v1/tasks", &req).await
}

pub async fn update_task(id: &str, req: UpdateTaskRequest) -> Result<UpdateTaskResponse, String> {
    api_put(&format!("/api/v1/tasks/{}", id), &req).await
}

pub async fn update_task_status(id: &str, status: i32) -> Result<(), String> {
    let body = serde_json::json!({ "status": status });
    api_put_empty(&format!("/api/v1/tasks/{}/status", id), &body).await
}

pub async fn update_task_progress(id: &str, progress: i32) -> Result<GetTaskResponse, String> {
    let body = serde_json::json!({ "id": id, "progress": progress });
    api_put(&format!("/api/v1/tasks/{}/progress", id), &body).await
}
```

- [ ] **Step 6: 创建 api/message.rs**

```rust
//! Message 域 API - 消息发送

use common::api::SendMessageToAgentParams;

use super::api_post;

/// 用户向 Agent 发送消息
pub async fn send_message_to_agent(params: SendMessageToAgentParams) -> Result<common::api::SendMessageToAgentResponse, String> {
    api_post("/api/v1/finance/messages/agents", &params).await
}
```

- [ ] **Step 7: 创建 api/system.rs**

```rust
//! System 域 API - 健康检查、定时触发器

use super::{api_delete, api_get, api_get_or_default, api_post, api_post_empty, api_put};

/// 健康检查（返回纯文本）
pub async fn check_health() -> Result<String, String> {
    super::api_get_text("/health").await
}

// ===== 定时触发器 =====

pub async fn list_cron_triggers() -> Result<common::api::ListCronTriggersResponse, String> {
    api_get_or_default("/api/v1/system/cron-triggers").await
}

pub async fn get_cron_trigger(id: &str) -> Result<common::api::GetCronTriggerResponse, String> {
    api_get(&format!("/api/v1/system/cron-triggers/{}", id)).await
}

pub async fn create_cron_trigger(req: common::api::CreateCronTriggerRequest) -> Result<common::api::CreateCronTriggerResponse, String> {
    api_post("/api/v1/system/cron-triggers", &req).await
}

pub async fn update_cron_trigger(id: &str, req: common::api::UpdateCronTriggerRequest) -> Result<common::api::UpdateCronTriggerResponse, String> {
    api_put(&format!("/api/v1/system/cron-triggers/{}", id), &req).await
}

pub async fn delete_cron_trigger(id: &str) -> Result<(), String> {
    api_delete(&format!("/api/v1/system/cron-triggers/{}", id)).await
}

pub async fn pause_cron_trigger(id: &str) -> Result<(), String> {
    let body = serde_json::json!({});
    api_post_empty(&format!("/api/v1/system/cron-triggers/{}/pause", id), &body).await
}

pub async fn resume_cron_trigger(id: &str) -> Result<(), String> {
    let body = serde_json::json!({});
    api_post_empty(&format!("/api/v1/system/cron-triggers/{}/resume", id), &body).await
}
```

- [ ] **Step 8: 删除旧的 api/agent.rs、api/health.rs、api/model_provider.rs**

删除以下旧文件（功能已合并到新模块）：
- `frontend/src/api/agent.rs`
- `frontend/src/api/health.rs`
- `frontend/src/api/model_provider.rs`

```bash
cd /Users/aman/Technology/rust/ai_orz
rm frontend/src/api/agent.rs frontend/src/api/health.rs frontend/src/api/model_provider.rs
```

- [ ] **Step 9: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/api/
git commit -m "feat(frontend): implement all business domain API clients (auth/org/hr/finance/project/message/system)"
```

---

## Task 5: 建立基础 UI 组件库

**Files:**
- Create: `frontend/src/components/mod.rs` (覆盖)
- Create: `frontend/src/components/button.rs`
- Create: `frontend/src/components/modal.rs`
- Create: `frontend/src/components/state.rs`

**目标：** 建立可复用的基础 UI 组件，使用 Task 1 定义的 CSS 类。

- [ ] **Step 1: 重写 components/mod.rs**

```rust
//! 基础 UI 组件库

pub mod button;
pub mod modal;
pub mod state;
```

- [ ] **Step 2: 创建 components/button.rs**

```rust
//! 按钮组件

use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    Primary,
    Accent,
    Secondary,
    Danger,
    Ghost,
}

impl ButtonVariant {
    fn class(self) -> &'static str {
        match self {
            ButtonVariant::Primary => "btn btn-primary",
            ButtonVariant::Accent => "btn btn-accent",
            ButtonVariant::Secondary => "btn btn-secondary",
            ButtonVariant::Danger => "btn btn-danger",
            ButtonVariant::Ghost => "btn btn-ghost",
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct ButtonProps {
    #[props(default = ButtonVariant::Primary)]
    variant: ButtonVariant,
    #[props(default = false)]
    disabled: bool,
    #[props(default = false)]
    small: bool,
    onclick: Option<EventHandler<MouseEvent>>,
    children: Element,
}

#[component]
pub fn Button(props: ButtonProps) -> Element {
    let mut class = props.variant.class().to_string();
    if props.small {
        class.push_str(" btn-sm");
    }
    rsx! {
        button {
            class: "{class}",
            disabled: props.disabled,
            onclick: move |e| {
                if let Some(handler) = &props.onclick {
                    handler.call(e);
                }
            },
            {props.children}
        }
    }
}
```

- [ ] **Step 3: 创建 components/modal.rs**

```rust
//! 模态对话框组件

use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ModalProps {
    title: String,
    show: bool,
    on_close: EventHandler<()>,
    children: Element,
    #[props(default = None)]
    footer: Option<Element>,
}

#[component]
pub fn Modal(props: ModalProps) -> Element {
    if !props.show {
        return rsx! {};
    }
    rsx! {
        div {
            class: "modal-overlay",
            onclick: move |_| props.on_close.call(()),
            div {
                class: "modal-content",
                onclick: |e| e.stop_propagation(),
                div {
                    class: "modal-header",
                    h3 { class: "modal-title", "{props.title}" }
                    button {
                        class: "modal-close",
                        onclick: move |_| props.on_close.call(()),
                        "×"
                    }
                }
                {props.children}
                if let Some(footer) = &props.footer {
                    div { class: "modal-footer", {footer.clone()} }
                }
            }
        }
    }
}
```

- [ ] **Step 4: 创建 components/state.rs**

```rust
//! 状态展示组件 - 加载中、空状态、错误提示

use dioxus::prelude::*;

#[component]
pub fn Loading() -> Element {
    rsx! { div { class: "state-loading", "加载中..." } }
}

#[component]
pub fn EmptyState(icon: Option<String>, message: String) -> Element {
    let icon = icon.unwrap_or_else(|| "📭".to_string());
    rsx! {
        div { class: "state-empty",
            div { class: "state-empty-icon", "{icon}" }
            p { "{message}" }
        }
    }
}

#[component]
pub fn ErrorAlert(message: String) -> Element {
    if message.is_empty() {
        return rsx! {};
    }
    rsx! { div { class: "alert alert-error", "{message}" } }
}

#[component]
pub fn SuccessAlert(message: String) -> Element {
    if message.is_empty() {
        return rsx! {};
    }
    rsx! { div { class: "alert alert-success", "{message}" } }
}
```

- [ ] **Step 5: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/components/
git commit -m "feat(frontend): add reusable UI components (Button/Modal/State alerts)"
```

---

## Task 6: 建立布局组件（Navbar + AppLayout）

**Files:**
- Create: `frontend/src/layouts/mod.rs`
- Create: `frontend/src/layouts/navbar.rs`
- Create: `frontend/src/layouts/app_layout.rs`

**目标：** 建立应用主布局，包含顶部导航栏（基于 CSS 类）和内容区域。

- [ ] **Step 1: 创建 layouts/mod.rs**

```rust
//! 布局组件模块

pub mod app_layout;
pub mod navbar;
```

- [ ] **Step 2: 创建 layouts/navbar.rs**

```rust
//! 顶部导航栏

use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::pages::Route;
use crate::store::auth::use_auth_state;

#[component]
pub fn Navbar() -> Element {
    let mut hr_menu_open = use_signal(|| false);
    let mut finance_menu_open = use_signal(|| false);
    let mut project_menu_open = use_signal(|| false);
    let mut system_menu_open = use_signal(|| false);
    let mut user_menu_open = use_signal(|| false);
    let auth = use_auth_state();

    let close_all = move || {
        hr_menu_open.set(false);
        finance_menu_open.set(false);
        project_menu_open.set(false);
        system_menu_open.set(false);
        user_menu_open.set(false);
    };

    let username = if auth().username.is_empty() {
        "用户".to_string()
    } else {
        auth().username.clone()
    };
    let is_admin = auth().is_admin();

    rsx! {
        nav { class: "navbar",
            // 左侧：品牌 + 导航
            div { class: "navbar-section",
                Link { to: Route::Reception, class: "navbar-brand", "AI Orz" }

                // 人力资源
                div { style: "position: relative;",
                    button {
                        class: "navbar-item",
                        onclick: move |_| { close_all(); hr_menu_open.set(!hr_menu_open()); },
                        "人力资源"
                        span { " ▾" }
                    }
                    if hr_menu_open() {
                        div { class: "navbar-dropdown",
                            Link { to: Route::HrAgents, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "Agent 管理"
                            }
                            Link { to: Route::HrSkills, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "技能库"
                            }
                        }
                    }
                }

                // 财务管理
                div { style: "position: relative;",
                    button {
                        class: "navbar-item",
                        onclick: move |_| { close_all(); finance_menu_open.set(!finance_menu_open()); },
                        "财务管理"
                        span { " ▾" }
                    }
                    if finance_menu_open() {
                        div { class: "navbar-dropdown",
                            Link { to: Route::FinanceModelProviders, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "模型提供商"
                            }
                            Link { to: Route::FinanceTools, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "工具管理"
                            }
                            Link { to: Route::FinanceMessageChannels, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "消息渠道"
                            }
                        }
                    }
                }

                // 项目管理
                div { style: "position: relative;",
                    button {
                        class: "navbar-item",
                        onclick: move |_| { close_all(); project_menu_open.set(!project_menu_open()); },
                        "项目管理"
                        span { " ▾" }
                    }
                    if project_menu_open() {
                        div { class: "navbar-dropdown",
                            Link { to: Route::ProjectList, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "项目列表"
                            }
                        }
                    }
                }

                // 系统管理
                div { style: "position: relative;",
                    button {
                        class: "navbar-item",
                        onclick: move |_| { close_all(); system_menu_open.set(!system_menu_open()); },
                        "系统"
                        span { " ▾" }
                    }
                    if system_menu_open() {
                        div { class: "navbar-dropdown",
                            Link { to: Route::SystemTriggers, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "定时触发器"
                            }
                            Link { to: Route::SystemHealth, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "健康检查"
                            }
                        }
                    }
                }
            }

            // 右侧：用户菜单
            div { class: "navbar-section",
                div { style: "position: relative;",
                    button {
                        class: "navbar-item",
                        onclick: move |_| { close_all(); user_menu_open.set(!user_menu_open()); },
                        span { style: "background: var(--color-mistral-orange); width: 28px; height: 28px; border-radius: 50%; display: flex; align-items: center; justify-content: center; color: white; font-weight: bold; font-size: 13px;",
                            "{username.chars().next().unwrap_or('U')}"
                        }
                        span { "{username}" }
                        span { " ▾" }
                    }
                    if user_menu_open() {
                        div { class: "navbar-dropdown", style: "right: 0; left: auto;",
                            Link { to: Route::UserProfile, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "👤 个人信息"
                            }
                            if is_admin {
                                Link { to: Route::OrganizationInfo, class: "navbar-dropdown-item",
                                    onclick: move |_| close_all(),
                                    "🏢 组织信息"
                                }
                                Link { to: Route::OrganizationUsers, class: "navbar-dropdown-item",
                                    onclick: move |_| close_all(),
                                    "👥 用户管理"
                                }
                            }
                            div { style: "border-top: 1px solid var(--color-border-light);" }
                            Link { to: Route::Settings, class: "navbar-dropdown-item",
                                onclick: move |_| close_all(),
                                "⚙️ 设置"
                            }
                            Link { to: Route::Reception, class: "navbar-dropdown-item",
                                onclick: move |_| {
                                    close_all();
                                    crate::store::auth::clear_token();
                                },
                                "🚪 退出登录"
                            }
                        }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 3: 创建 layouts/app_layout.rs**

```rust
//! 应用主布局 - Navbar + Content

use dioxus::prelude::*;

use super::navbar::Navbar;

#[component]
pub fn AppLayout(children: Element) -> Element {
    rsx! {
        div { class: "app-container",
            Navbar {}
            main { class: "content-area",
                {children}
            }
        }
    }
}
```

- [ ] **Step 4: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/layouts/
git commit -m "feat(frontend): add app layout with CSS-based navbar and router links"
```

---

## Task 7: 引入 Dioxus Router 并建立页面模块结构

**Files:**
- Modify: `frontend/Cargo.toml` (确认 dioxus router feature)
- Create: `frontend/src/pages/mod.rs`
- Modify: `frontend/src/main.rs` (覆盖)

**目标：** 引入 Dioxus Router，定义所有路由，建立页面模块声明。

- [ ] **Step 1: 确认 Cargo.toml 依赖**

检查 `frontend/Cargo.toml` 中 dioxus 的 features 是否包含 `router`。当前已有：
```toml
dioxus = { version = "0.7", features = ["web", "router"] }
```
无需修改。

- [ ] **Step 2: 创建 pages/mod.rs**

```rust
//! 页面模块 - 按业务域分组

pub mod finance;
pub mod hr;
pub mod message;
pub mod organization;
pub mod project;
pub mod reception;
pub mod settings;
pub mod system;
pub mod user;

use dioxus_router::prelude::*;

/// 全局路由枚举
#[derive(Clone, Routable, Debug, PartialEq)]
pub enum Route {
    // 前台接待（登录/初始化）
    #[route("/")]
    Reception {},

    // 组织模块
    #[route("/organization")]
    OrganizationInfo {},
    #[route("/organization/users")]
    OrganizationUsers {},

    // HR 模块
    #[route("/hr/agents")]
    HrAgents {},
    #[route("/hr/agents/:id")]
    HrAgentDetail { id: String },
    #[route("/hr/skills")]
    HrSkills {},

    // Finance 模块
    #[route("/finance/model-providers")]
    FinanceModelProviders {},
    #[route("/finance/tools")]
    FinanceTools {},
    #[route("/finance/message-channels")]
    FinanceMessageChannels {},

    // Project 模块
    #[route("/projects")]
    ProjectList {},
    #[route("/projects/:id")]
    ProjectDetail { id: String },

    // Message 模块
    #[route("/messages/chat")]
    MessageChat {},

    // System 模块
    #[route("/system/triggers")]
    SystemTriggers {},
    #[route("/system/health")]
    SystemHealth {},

    // 用户
    #[route("/user/profile")]
    UserProfile {},

    // 设置
    #[route("/settings")]
    Settings {},
}
```

- [ ] **Step 3: 创建各页面模块的 mod.rs 和占位组件**

创建以下文件，每个文件包含基本的页面组件占位：

`frontend/src/pages/reception.rs`:
```rust
use dioxus::prelude::*;

#[component]
pub fn Reception() -> Element {
    rsx! { div { class: "card", "前台接待 - 待实现" } }
}
```

`frontend/src/pages/organization/mod.rs`:
```rust
pub mod info;
pub mod users;
```

`frontend/src/pages/organization/info.rs`:
```rust
use dioxus::prelude::*;

#[component]
pub fn OrganizationInfo() -> Element {
    rsx! { div { class: "card", "组织信息 - 待实现" } }
}
```

`frontend/src/pages/organization/users.rs`:
```rust
use dioxus::prelude::*;

#[component]
pub fn OrganizationUsers() -> Element {
    rsx! { div { class: "card", "用户管理 - 待实现" } }
}
```

`frontend/src/pages/hr/mod.rs`:
```rust
pub mod agent_detail;
pub mod agents;
pub mod skills;
```

`frontend/src/pages/hr/agents.rs`:
```rust
use dioxus::prelude::*;

#[component]
pub fn HrAgents() -> Element {
    rsx! { div { class: "card", "Agent 管理 - 待实现" } }
}
```

`frontend/src/pages/hr/agent_detail.rs`:
```rust
use dioxus::prelude::*;

#[component]
pub fn HrAgentDetail(id: String) -> Element {
    rsx! { div { class: "card", "Agent 详情 {id} - 待实现" } }
}
```

`frontend/src/pages/hr/skills.rs`:
```rust
use dioxus::prelude::*;

#[component]
pub fn HrSkills() -> Element {
    rsx! { div { class: "card", "技能库 - 待实现" } }
}
```

`frontend/src/pages/finance/mod.rs`:
```rust
pub mod message_channels;
pub mod model_providers;
pub mod tools;
```

`frontend/src/pages/finance/model_providers.rs`:
```rust
use dioxus::prelude::*;

#[component]
pub fn FinanceModelProviders() -> Element {
    rsx! { div { class: "card", "模型提供商管理 - 待实现" } }
}
```

`frontend/src/pages/finance/tools.rs`:
```rust
use dioxus::prelude::*;

#[component]
pub fn FinanceTools() -> Element {
    rsx! { div { class: "card", "工具管理 - 待实现" } }
}
```

`frontend/src/pages/finance/message_channels.rs`:
```rust
use dioxus::prelude::*;

#[component]
pub fn FinanceMessageChannels() -> Element {
    rsx! { div { class: "card", "消息渠道管理 - 待实现" } }
}
```

`frontend/src/pages/project/mod.rs`:
```rust
pub mod project_detail;
pub mod projects;
```

`frontend/src/pages/project/projects.rs`:
```rust
use dioxus::prelude::*;

#[component]
pub fn ProjectList() -> Element {
    rsx! { div { class: "card", "项目列表 - 待实现" } }
}
```

`frontend/src/pages/project/project_detail.rs`:
```rust
use dioxus::prelude::*;

#[component]
pub fn ProjectDetail(id: String) -> Element {
    rsx! { div { class: "card", "项目详情 {id} - 待实现" } }
}
```

`frontend/src/pages/message/mod.rs`:
```rust
pub mod chat;
```

`frontend/src/pages/message/chat.rs`:
```rust
use dioxus::prelude::*;

#[component]
pub fn MessageChat() -> Element {
    rsx! { div { class: "card", "对话界面 - 待实现" } }
}
```

`frontend/src/pages/system/mod.rs`:
```rust
pub mod health;
pub mod triggers;
```

`frontend/src/pages/system/triggers.rs`:
```rust
use dioxus::prelude::*;

#[component]
pub fn SystemTriggers() -> Element {
    rsx! { div { class: "card", "定时触发器 - 待实现" } }
}
```

`frontend/src/pages/system/health.rs`:
```rust
use dioxus::prelude::*;

#[component]
pub fn SystemHealth() -> Element {
    rsx! { div { class: "card", "健康检查 - 待实现" } }
}
```

`frontend/src/pages/user/mod.rs`:
```rust
pub mod profile;
```

`frontend/src/pages/user/profile.rs`:
```rust
use dioxus::prelude::*;

#[component]
pub fn UserProfile() -> Element {
    rsx! { div { class: "card", "个人信息 - 待实现" } }
}
```

`frontend/src/pages/settings.rs`:
```rust
use dioxus::prelude::*;

#[component]
pub fn Settings() -> Element {
    rsx! { div { class: "card", "系统设置 - 待实现" } }
}
```

- [ ] **Step 4: 重写 main.rs**

将 `frontend/src/main.rs` 完整替换为：

```rust
mod api;
mod components;
mod config;
mod layouts;
mod pages;
mod store;

// Include compile-time generated configuration from build.rs
include!(concat!(env!("OUT_DIR"), "/compiled_config.rs"));

use dioxus::prelude::*;
use dioxus_router::prelude::*;
use store::auth::{save_token, AuthState};

use crate::pages::Route;

fn main() {
    launch(App);
}

#[component]
fn App() -> Element {
    // 初始化全局认证状态
    use_context_provider(|| Signal::new(AuthState::restore()));

    rsx! {
        document::Title { "AI Orz - AI 代理执行框架" }
        Router::<Route> {}
    }
}

// ===== 路由组件渲染入口 =====
// Dioxus Router 会根据 Route 枚举自动调用对应的组件函数

// 前台接待
#[component]
fn Reception() -> Element {
    crate::pages::reception::Reception()
}

// 组织模块
#[component]
fn OrganizationInfo() -> Element {
    crate::pages::organization::info::OrganizationInfo()
}

#[component]
fn OrganizationUsers() -> Element {
    crate::pages::organization::users::OrganizationUsers()
}

// HR 模块
#[component]
fn HrAgents() -> Element {
    crate::pages::hr::agents::HrAgents()
}

#[component]
fn HrAgentDetail(id: String) -> Element {
    crate::pages::hr::agent_detail::HrAgentDetail { id }
}

#[component]
fn HrSkills() -> Element {
    crate::pages::hr::skills::HrSkills()
}

// Finance 模块
#[component]
fn FinanceModelProviders() -> Element {
    crate::pages::finance::model_providers::FinanceModelProviders()
}

#[component]
fn FinanceTools() -> Element {
    crate::pages::finance::tools::FinanceTools()
}

#[component]
fn FinanceMessageChannels() -> Element {
    crate::pages::finance::message_channels::FinanceMessageChannels()
}

// Project 模块
#[component]
fn ProjectList() -> Element {
    crate::pages::project::projects::ProjectList()
}

#[component]
fn ProjectDetail(id: String) -> Element {
    crate::pages::project::project_detail::ProjectDetail { id }
}

// Message 模块
#[component]
fn MessageChat() -> Element {
    crate::pages::message::chat::MessageChat()
}

// System 模块
#[component]
fn SystemTriggers() -> Element {
    crate::pages::system::triggers::SystemTriggers()
}

#[component]
fn SystemHealth() -> Element {
    crate::pages::system::health::SystemHealth()
}

// 用户
#[component]
fn UserProfile() -> Element {
    crate::pages::user::profile::UserProfile()
}

// 设置
#[component]
fn Settings() -> Element {
    crate::pages::settings::Settings()
}
```

- [ ] **Step 5: 删除旧的 components 文件（已迁移到 pages 和 layouts）**

```bash
cd /Users/aman/Technology/rust/ai_orz
rm -f frontend/src/components/navbar.rs
rm -f frontend/src/components/reception.rs
rm -f frontend/src/components/health_check.rs
rm -f frontend/src/components/agent_management.rs
rm -f frontend/src/components/model_provider_management.rs
rm -f frontend/src/components/organization_info.rs
rm -f frontend/src/components/user_management.rs
rm -f frontend/src/components/user_profile.rs
rm -f frontend/src/components/settings_page.rs
```

- [ ] **Step 6: 尝试编译，修复错误**

```bash
cd /Users/aman/Technology/rust/ai_orz
cd frontend && cargo check 2>&1 | head -50
```

根据编译错误修复导入路径、类型不匹配等问题。常见问题：
- `save_token` 导入未使用 - 移除
- Route 枚举需要 `#[derive(Clone, Routable, Debug, PartialEq)]`
- 确保所有页面组件函数名与 Route 变体对应

- [ ] **Step 7: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/
git commit -m "feat(frontend): introduce Dioxus Router with full page module structure and route definitions"
```

---

## Task 8: 实现 Reception 页面（登录闭环）

**Files:**
- Modify: `frontend/src/pages/reception.rs` (覆盖)

**目标：** 实现完整的登录/初始化流程，登录成功后保存 token 到 localStorage 并跳转。

- [ ] **Step 1: 实现 Reception 页面**

将 `frontend/src/pages/reception.rs` 完整替换为：

```rust
//! 前台接待 - 系统初始化 + 登录

use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::api::auth::{check_initialized, initialize_system, login};
use crate::api::organization::list_organizations_public;
use crate::components::state::{ErrorAlert, Loading};
use crate::store::auth::{save_token, AuthState};
use common::api::{
    InitializeSystemRequest, LoginRequest, OrganizationListItem,
};

#[component]
pub fn Reception() -> Element {
    let mut loading = use_signal(|| true);
    let mut initialized = use_signal(|| false);
    let mut organizations = use_signal(Vec::<OrganizationListItem>::new);
    let mut error = use_signal(String::new);

    // 登录表单
    let mut selected_org_id = use_signal(String::new);
    let mut login_username = use_signal(String::new);
    let mut login_password = use_signal(String::new);
    let mut login_submitting = use_signal(|| false);

    // 初始化表单
    let mut org_name = use_signal(String::new);
    let mut org_description = use_signal(String::new);
    let mut init_username = use_signal(String::new);
    let mut init_password = use_signal(String::new);
    let mut display_name = use_signal(String::new);
    let mut email = use_signal(String::new);
    let mut init_submitting = use_signal(|| false);

    let auth = use_context::<Signal<AuthState>>();

    // 页面加载检查初始化状态
    use_effect(move || {
        spawn(async move {
            match check_initialized().await {
                Ok(resp) => {
                    if resp.initialized {
                        match list_organizations_public().await {
                            Ok(list) => {
                                organizations.set(list.organizations);
                                initialized.set(true);
                            }
                            Err(e) => error.set(e),
                        }
                    } else {
                        initialized.set(false);
                    }
                }
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    });

    // 登录提交
    let on_submit_login = move |_| {
        spawn(async move {
            if selected_org_id().is_empty() {
                error.set("请先选择一个组织".to_string());
                return;
            }
            if login_username().is_empty() || login_password().is_empty() {
                error.set("用户名和密码不能为空".to_string());
                return;
            }
            login_submitting.set(true);
            error.set(String::new());

            let req = LoginRequest {
                organization_id: selected_org_id(),
                username: login_username(),
                password_hash: login_password(),
            };

            match login(req).await {
                Ok(resp) => {
                    save_token(&resp.token);
                    // 更新全局认证状态
                    let mut state = auth.write();
                    state.token = Some(resp.token);
                    state.username = resp.username.unwrap_or_default();
                    state.role = resp.role.unwrap_or(1);
                    state.org_id = resp.organization_id.unwrap_or_default();
                    drop(state);
                    // 跳转 - 使用 window location 触发完整刷新
                    let _ = web_sys::window().unwrap().location().set_href("/");
                }
                Err(e) => {
                    error.set(e);
                    login_submitting.set(false);
                }
            }
        });
    };

    // 初始化提交
    let on_submit_init = move |_| {
        spawn(async move {
            if org_name().is_empty() || init_username().is_empty() || init_password().is_empty() {
                error.set("组织名称、用户名、密码不能为空".to_string());
                return;
            }
            init_submitting.set(true);
            error.set(String::new());

            let req = InitializeSystemRequest {
                organization_name: org_name(),
                description: if org_description().is_empty() { None } else { Some(org_description()) },
                admin_username: init_username(),
                admin_password_hash: init_password(),
                admin_display_name: if display_name().is_empty() { None } else { Some(display_name()) },
                admin_email: if email().is_empty() { None } else { Some(email()) },
            };

            match initialize_system(req).await {
                Ok(_) => {
                    let _ = web_sys::window().unwrap().location().reload();
                }
                Err(e) => {
                    error.set(e);
                    init_submitting.set(false);
                }
            }
        });
    };

    rsx! {
        div { style: "max-width: 600px; margin: 0 auto;",
            div { class: "card", style: "text-align: center;",
                div { style: "font-size: 56px; margin-bottom: 24px;", "👋" }
                h2 { style: "color: var(--color-mistral-black); margin-bottom: 16px; font-size: 28px;",
                    "欢迎来到 AI Orz"
                }
                p { class: "text-secondary", style: "margin-bottom: 32px; font-size: 16px;",
                    "AI Orz 是一个智能的 AI 代理执行框架，帮助您组织和管理各类 AI 智能体，让它们协同工作完成复杂任务。"
                }

                if loading() {
                    Loading {}
                } else {
                    ErrorAlert { message: error() }

                    if initialized() {
                        // 已初始化：登录表单
                        div { style: "text-align: left;",
                            h3 { class: "mb-4", "🔐 请选择组织并登录" }

                            // 组织列表
                            div { class: "mb-4",
                                for org in organizations() {
                                    {
                                        let is_selected = selected_org_id() == org.organization_id;
                                        let border = if is_selected { "var(--color-mistral-orange)" } else { "var(--color-border)" };
                                        let bg = if is_selected { "var(--color-cream)" } else { "var(--color-warm-ivory)" };
                                        rsx! {
                                            div {
                                                key: "{org.organization_id}",
                                                style: "padding: 12px; border-radius: 4px; margin-bottom: 8px; cursor: pointer; border: 2px solid {border}; background: {bg};",
                                                onclick: move |_| selected_org_id.set(org.organization_id.clone()),
                                                div { style: "font-weight: 600; color: var(--color-text-primary);",
                                                    "{org.name}"
                                                }
                                                if let Some(desc) = &org.description {
                                                    p { class: "text-secondary", style: "font-size: 13px; margin-top: 4px;", "{desc}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            form { onsubmit: move |e| { e.prevent_default(); on_submit_login.call(()); },
                                div { class: "form-group",
                                    label { class: "form-label", "用户名" }
                                    input {
                                        class: "form-input",
                                        r#type: "text",
                                        value: "{login_username}",
                                        oninput: move |e| login_username.set(e.value()),
                                        placeholder: "请输入用户名",
                                    }
                                }
                                div { class: "form-group",
                                    label { class: "form-label", "密码" }
                                    input {
                                        class: "form-input",
                                        r#type: "password",
                                        value: "{login_password}",
                                        oninput: move |e| login_password.set(e.value()),
                                        placeholder: "请输入密码",
                                    }
                                }
                                button {
                                    class: "btn btn-accent btn-lg w-full",
                                    r#type: "submit",
                                    disabled: login_submitting(),
                                    if login_submitting() { "登录中..." } else { "登录" }
                                }
                            }
                        }
                    } else {
                        // 未初始化：初始化表单
                        div { style: "text-align: left;",
                            h3 { class: "mb-4", "🚀 首次使用 - 初始化系统" }
                            p { class: "text-secondary mb-6",
                                "欢迎使用 AI Orz！请填写以下信息完成初始化，创建您的第一个组织和超级管理员用户。"
                            }
                            form { onsubmit: move |e| { e.prevent_default(); on_submit_init.call(()); },
                                div { class: "form-group",
                                    label { class: "form-label", "组织名称 *" }
                                    input { class: "form-input", r#type: "text", value: "{org_name}",
                                        oninput: move |e| org_name.set(e.value()), placeholder: "例如：我的组织" }
                                }
                                div { class: "form-group",
                                    label { class: "form-label", "组织描述" }
                                    textarea { class: "form-textarea", value: "{org_description}",
                                        oninput: move |e| org_description.set(e.value()), placeholder: "简单描述一下您的组织..." }
                                }
                                div { class: "form-group",
                                    label { class: "form-label", "管理员用户名 *" }
                                    input { class: "form-input", r#type: "text", value: "{init_username}",
                                        oninput: move |e| init_username.set(e.value()), placeholder: "例如：admin" }
                                }
                                div { class: "form-group",
                                    label { class: "form-label", "管理员密码 *" }
                                    input { class: "form-input", r#type: "password", value: "{init_password}",
                                        oninput: move |e| init_password.set(e.value()), placeholder: "请输入密码" }
                                }
                                div { class: "form-group",
                                    label { class: "form-label", "显示名称" }
                                    input { class: "form-input", r#type: "text", value: "{display_name}",
                                        oninput: move |e| display_name.set(e.value()), placeholder: "例如：超级管理员" }
                                }
                                div { class: "form-group",
                                    label { class: "form-label", "邮箱" }
                                    input { class: "form-input", r#type: "email", value: "{email}",
                                        oninput: move |e| email.set(e.value()), placeholder: "admin@example.com" }
                                }
                                button { class: "btn btn-accent btn-lg w-full", r#type: "submit", disabled: init_submitting(),
                                    if init_submitting() { "初始化中..." } else { "完成初始化" }
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

- [ ] **Step 2: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/pages/reception.rs
git commit -m "feat(frontend): implement Reception page with login/init flow and token persistence"
```

---

## Task 9: 实现 Agent 管理页面

**Files:**
- Modify: `frontend/src/pages/hr/agents.rs` (覆盖)

**目标：** 实现 Agent 列表、创建、删除功能，使用新组件库和 CSS 类。

- [ ] **Step 1: 实现 Agent 管理页面**

将 `frontend/src/pages/hr/agents.rs` 完整替换为：

```rust
//! Agent 管理列表

use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::api::hr::{create_agent, delete_agent, list_agents};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, ErrorAlert, Loading};
use common::api::{CreateAgentRequest, ListAgentsResponseItem};

#[component]
pub fn HrAgents() -> Element {
    let mut agents = use_signal(Vec::<ListAgentsResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut show_add_modal = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut new_roles = use_signal(String::new);
    let mut new_model_provider_id = use_signal(String::new);
    let mut new_description = use_signal(String::new);
    let mut creating = use_signal(|| false);

    let load_agents = move || {
        loading.set(true);
        error.set(String::new());
        spawn(async move {
            match list_agents().await {
                Ok(list) => agents.set(list.agents),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    };

    use_effect(move || { load_agents(); });

    let handle_create = move |_| {
        spawn(async move {
            if new_name().is_empty() || new_model_provider_id().is_empty() {
                error.set("名称和模型提供商 ID 不能为空".to_string());
                return;
            }
            creating.set(true);
            let req = CreateAgentRequest {
                name: new_name(),
                roles: if new_roles().is_empty() { None } else { Some(vec![new_roles()]) },
                description: if new_description().is_empty() { None } else { Some(new_description()) },
                capabilities: None,
                soul: None,
                model_provider_id: new_model_provider_id(),
            };
            match create_agent(req).await {
                Ok(_) => {
                    show_add_modal.set(false);
                    new_name.set(String::new());
                    new_roles.set(String::new());
                    new_model_provider_id.set(String::new());
                    new_description.set(String::new());
                    load_agents();
                }
                Err(e) => error.set(format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let agents_list = agents.read().clone();

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }

            div { class: "card-header",
                h2 { class: "card-title", "Agent 管理" }
                button { class: "btn btn-accent", onclick: move |_| show_add_modal.set(true), "+ 创建 Agent" }
            }

            if loading() {
                Loading {}
            } else if agents_list.is_empty() {
                EmptyState { icon: "🤖".to_string(), message: "暂无 Agent，点击上方按钮创建第一个".to_string() }
            } else {
                table { class: "table",
                    thead { tr {
                        th { "名称" }
                        th { "角色" }
                        th { "模型提供商" }
                        th { "操作" }
                    }}
                    tbody {
                        for agent in agents_list.iter() {
                            {
                                let id = agent.id.clone();
                                rsx! {
                                    tr { key: "{id}",
                                        td {
                                            Link { to: crate::pages::Route::HrAgentDetail { id: id.clone() },
                                                style: "color: var(--color-mistral-orange); text-decoration: none; font-weight: 500;",
                                                "{agent.name}"
                                            }
                                        }
                                        td { class: "text-secondary", "{agent.roles.join(", ")}" }
                                        td { class: "text-mono", "{agent.model_provider_id}" }
                                        td {
                                            button { class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let id = id.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_agent(&id).await {
                                                            error.set(format!("删除失败: {}", e));
                                                        } else {
                                                            load_agents();
                                                        }
                                                    });
                                                },
                                                "删除"
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

        // 创建 Agent 弹窗
        Modal {
            title: "创建新 Agent".to_string(),
            show: show_add_modal(),
            on_close: move |_| {
                show_add_modal.set(false);
                new_name.set(String::new());
                new_roles.set(String::new());
                new_model_provider_id.set(String::new());
                new_description.set(String::new());
            },
            div {
                div { class: "form-group",
                    label { class: "form-label", "Agent 名称 *" }
                    input { class: "form-input", value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()), placeholder: "请输入 Agent 名称" }
                }
                div { class: "form-group",
                    label { class: "form-label", "角色描述" }
                    input { class: "form-input", value: "{new_roles}",
                        oninput: move |e| new_roles.set(e.value()), placeholder: "如：代码助手" }
                }
                div { class: "form-group",
                    label { class: "form-label", "模型提供商 ID *" }
                    input { class: "form-input", value: "{new_model_provider_id}",
                        oninput: move |e| new_model_provider_id.set(e.value()), placeholder: "已配置的模型提供商 ID" }
                }
                div { class: "form-group",
                    label { class: "form-label", "描述" }
                    textarea { class: "form-textarea", value: "{new_description}",
                        oninput: move |e| new_description.set(e.value()), placeholder: "Agent 描述（可选）" }
                }
            },
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_add_modal.set(false), "取消" }
                button { class: "btn btn-accent", disabled: creating(), onclick: handle_create,
                    if creating() { "创建中..." } else { "创建" }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/pages/hr/agents.rs
git commit -m "feat(frontend): implement Agent management page with list/create/delete using new UI components"
```

---

## Task 10: 实现模型提供商管理页面

**Files:**
- Modify: `frontend/src/pages/finance/model_providers.rs` (覆盖)

**目标：** 实现模型提供商列表、创建、删除、测试连接功能。

- [ ] **Step 1: 实现模型提供商管理页面**

将 `frontend/src/pages/finance/model_providers.rs` 完整替换为：

```rust
//! 模型提供商管理

use dioxus::prelude::*;

use crate::api::finance::{create_model_provider, delete_model_provider, list_model_providers, test_model_provider_connection};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, ErrorAlert, Loading, SuccessAlert};
use common::api::{CreateModelProviderRequest, ListModelProvidersResponseItem};

#[component]
pub fn FinanceModelProviders() -> Element {
    let mut providers = use_signal(Vec::<ListModelProvidersResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut success = use_signal(String::new);
    let mut show_modal = use_signal(|| false);

    // 表单状态
    let mut name = use_signal(String::new);
    let mut provider_type = use_signal("openai".to_string());
    let mut model_name = use_signal(String::new);
    let mut api_key = use_signal(String::new);
    let mut base_url = use_signal(String::new());
    let mut description = use_signal(String::new);
    let mut creating = use_signal(|| false);

    let load = move || {
        loading.set(true);
        spawn(async move {
            match list_model_providers().await {
                Ok(list) => providers.set(list.providers),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    };

    use_effect(move || { load(); });

    let handle_create = move |_| {
        spawn(async move {
            if name().is_empty() || model_name().is_empty() {
                error.set("名称和模型名称不能为空".to_string());
                return;
            }
            creating.set(true);
            let req = CreateModelProviderRequest {
                name: name(),
                provider_type: provider_type(),
                model_name: model_name(),
                api_key: if api_key().is_empty() { None } else { Some(api_key()) },
                base_url: if base_url().is_empty() { None } else { Some(base_url()) },
                description: if description().is_empty() { None } else { Some(description()) },
            };
            match create_model_provider(req).await {
                Ok(resp) => {
                    show_modal.set(false);
                    name.set(String::new());
                    model_name.set(String::new());
                    api_key.set(String::new());
                    base_url.set(String::new());
                    description.set(String::new());
                    success.set("创建成功".to_string());
                    load();
                    // 自动测试连接
                    spawn(async move {
                        match test_model_provider_connection(&resp.id).await {
                            Ok(_) => success.set("创建成功，连接测试通过".to_string()),
                            Err(e) => error.set(format!("创建成功但测试失败: {}", e)),
                        }
                    });
                }
                Err(e) => error.set(format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let providers_list = providers.read().clone();

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }
            SuccessAlert { message: success() }

            div { class: "card-header",
                h2 { class: "card-title", "模型提供商管理" }
                button { class: "btn btn-accent", onclick: move |_| show_modal.set(true), "+ 添加提供商" }
            }

            if loading() {
                Loading {}
            } else if providers_list.is_empty() {
                EmptyState { icon: "🧠".to_string(), message: "暂无模型提供商".to_string() }
            } else {
                table { class: "table",
                    thead { tr {
                        th { "名称" }
                        th { "类型" }
                        th { "模型" }
                        th { "操作" }
                    }}
                    tbody {
                        for p in providers_list.iter() {
                            {
                                let id = p.id.clone();
                                rsx! {
                                    tr { key: "{id}",
                                        td { style: "font-weight: 500;", "{p.name}" }
                                        td { span { class: "badge badge-info", "{p.provider_type}" } }
                                        td { class: "text-mono", "{p.model_name}" }
                                        td {
                                            button { class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let id = id.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_model_provider(&id).await {
                                                            error.set(format!("删除失败: {}", e));
                                                        } else {
                                                            success.set("已删除".to_string());
                                                            load();
                                                        }
                                                    });
                                                },
                                                "删除"
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

        Modal {
            title: "添加模型提供商".to_string(),
            show: show_modal(),
            on_close: move |_| show_modal.set(false),
            div {
                div { class: "form-group",
                    label { class: "form-label", "名称 *" }
                    input { class: "form-input", value: "{name}",
                        oninput: move |e| name.set(e.value()), placeholder: "如：OpenAI 主账号" }
                }
                div { class: "form-group",
                    label { class: "form-label", "类型" }
                    select { class: "form-select", value: "{provider_type}",
                        onchange: move |e| provider_type.set(e.value()),
                        option { value: "openai", "OpenAI" }
                        option { value: "openai_compatible", "OpenAI 兼容" }
                        option { value: "deepseek", "DeepSeek" }
                        option { value: "doubao", "豆包" }
                        option { value: "qwen", "通义千问" }
                        option { value: "ollama", "Ollama" }
                    }
                }
                div { class: "form-group",
                    label { class: "form-label", "模型名称 *" }
                    input { class: "form-input", value: "{model_name}",
                        oninput: move |e| model_name.set(e.value()), placeholder: "如：gpt-4o" }
                }
                div { class: "form-group",
                    label { class: "form-label", "API Key" }
                    input { class: "form-input", r#type: "password", value: "{api_key}",
                        oninput: move |e| api_key.set(e.value()), placeholder: "sk-..." }
                }
                div { class: "form-group",
                    label { class: "form-label", "Base URL" }
                    input { class: "form-input", value: "{base_url}",
                        oninput: move |e| base_url.set(e.value()), placeholder: "https://api.openai.com/v1" }
                }
                div { class: "form-group",
                    label { class: "form-label", "描述" }
                    input { class: "form-input", value: "{description}",
                        oninput: move |e| description.set(e.value()), placeholder: "可选" }
                }
            },
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_modal.set(false), "取消" }
                button { class: "btn btn-accent", disabled: creating(), onclick: handle_create,
                    if creating() { "创建中..." } else { "创建" }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/pages/finance/model_providers.rs
git commit -m "feat(frontend): implement model provider management with create/delete/test connection"
```

---

## Task 11: 实现项目管理页面

**Files:**
- Modify: `frontend/src/pages/project/projects.rs` (覆盖)

**目标：** 实现项目列表和创建功能。

- [ ] **Step 1: 实现项目列表页面**

将 `frontend/src/pages/project/projects.rs` 完整替换为：

```rust
//! 项目列表

use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::api::project::{create_project, list_projects};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, ErrorAlert, Loading};
use common::api::{CreateProjectRequest, ListProjectsResponseItem};

fn status_badge(status: i32) -> &'static str {
    match status {
        0 => "badge badge-error",
        1 => "badge badge-info",
        2 => "badge badge-success",
        3 => "badge badge-neutral",
        _ => "badge badge-neutral",
    }
}

fn status_text(status: i32) -> &'static str {
    match status {
        0 => "已归档",
        1 => "进行中",
        2 => "已完成",
        3 => "已暂停",
        _ => "未知",
    }
}

#[component]
pub fn ProjectList() -> Element {
    let mut projects = use_signal(Vec::<ListProjectsResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut show_modal = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut new_description = use_signal(String::new);
    let mut creating = use_signal(|| false);

    let load = move || {
        loading.set(true);
        spawn(async move {
            match list_projects().await {
                Ok(list) => projects.set(list.projects),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    };

    use_effect(move || { load(); });

    let handle_create = move |_| {
        spawn(async move {
            if new_name().is_empty() {
                error.set("项目名称不能为空".to_string());
                return;
            }
            creating.set(true);
            let req = CreateProjectRequest {
                name: new_name(),
                description: if new_description().is_empty() { None } else { Some(new_description()) },
                assignee_id: None,
                assignee_type: None,
            };
            match create_project(req).await {
                Ok(_) => {
                    show_modal.set(false);
                    new_name.set(String::new());
                    new_description.set(String::new());
                    load();
                }
                Err(e) => error.set(format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let projects_list = projects.read().clone();

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }

            div { class: "card-header",
                h2 { class: "card-title", "项目管理" }
                button { class: "btn btn-accent", onclick: move |_| show_modal.set(true), "+ 创建项目" }
            }

            if loading() {
                Loading {}
            } else if projects_list.is_empty() {
                EmptyState { icon: "📁".to_string(), message: "暂无项目".to_string() }
            } else {
                table { class: "table",
                    thead { tr {
                        th { "项目名称" }
                        th { "状态" }
                        th { "任务数" }
                        th { "创建时间" }
                    }}
                    tbody {
                        for p in projects_list.iter() {
                            {
                                let id = p.id.clone();
                                rsx! {
                                    tr { key: "{id}",
                                        td {
                                            Link { to: crate::pages::Route::ProjectDetail { id: id.clone() },
                                                style: "color: var(--color-mistral-orange); text-decoration: none; font-weight: 500;",
                                                "{p.name}"
                                            }
                                        }
                                        td { span { class: "{status_badge(p.status)}", "{status_text(p.status)}" } }
                                        td { class: "text-secondary", "{p.task_count.unwrap_or(0)}" }
                                        td { class: "text-mono text-muted", "{p.created_at}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Modal {
            title: "创建项目".to_string(),
            show: show_modal(),
            on_close: move |_| {
                show_modal.set(false);
                new_name.set(String::new());
                new_description.set(String::new());
            },
            div {
                div { class: "form-group",
                    label { class: "form-label", "项目名称 *" }
                    input { class: "form-input", value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()), placeholder: "请输入项目名称" }
                }
                div { class: "form-group",
                    label { class: "form-label", "描述" }
                    textarea { class: "form-textarea", value: "{new_description}",
                        oninput: move |e| new_description.set(e.value()), placeholder: "项目描述（可选）" }
                }
            },
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_modal.set(false), "取消" }
                button { class: "btn btn-accent", disabled: creating(), onclick: handle_create,
                    if creating() { "创建中..." } else { "创建" }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/pages/project/projects.rs
git commit -m "feat(frontend): implement project list page with create and status badges"
```

---

## Task 12: 实现其余基础页面

**Files:**
- Modify: 以下所有页面文件（覆盖占位代码）

**目标：** 为剩余页面实现基础 CRUD 功能，使用统一的组件和 CSS 风格。

- [ ] **Step 1: 实现组织信息页面 `pages/organization/info.rs`**

```rust
//! 组织信息管理

use dioxus::prelude::*;

use crate::api::organization::{get_current_organization, update_current_organization};
use crate::components::state::{ErrorAlert, Loading, SuccessAlert};
use common::api::{UpdateOrganizationRequest};

#[component]
pub fn OrganizationInfo() -> Element {
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut success = use_signal(String::new);
    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut org_id = use_signal(String::new);
    let mut saving = use_signal(|| false);

    use_effect(move || {
        spawn(async move {
            match get_current_organization().await {
                Ok(org) => {
                    name.set(org.name);
                    description.set(org.description.unwrap_or_default());
                    org_id.set(org.organization_id);
                }
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    });

    let handle_save = move |_| {
        spawn(async move {
            saving.set(true);
            let req = UpdateOrganizationRequest {
                name: Some(name()),
                description: if description().is_empty() { None } else { Some(description()) },
            };
            match update_current_organization(req).await {
                Ok(_) => success.set("保存成功".to_string()),
                Err(e) => error.set(e),
            }
            saving.set(false);
        });
    };

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }
            SuccessAlert { message: success() }

            div { class: "card-header",
                h2 { class: "card-title", "组织信息" }
            }

            if loading() {
                Loading {}
            } else {
                div { class: "form-group",
                    label { class: "form-label", "组织 ID" }
                    input { class: "form-input", disabled: true, value: "{org_id}" }
                }
                div { class: "form-group",
                    label { class: "form-label", "组织名称" }
                    input { class: "form-input", value: "{name}",
                        oninput: move |e| name.set(e.value()) }
                }
                div { class: "form-group",
                    label { class: "form-label", "组织描述" }
                    textarea { class: "form-textarea", value: "{description}",
                        oninput: move |e| description.set(e.value()) }
                }
                button { class: "btn btn-accent", disabled: saving(), onclick: handle_save,
                    if saving() { "保存中..." } else { "保存" }
                }
            }
        }
    }
}
```

- [ ] **Step 2: 实现用户管理页面 `pages/organization/users.rs`**

```rust
//! 用户管理

use dioxus::prelude::*;

use crate::api::organization::{create_user, delete_user, list_users};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, ErrorAlert, Loading};
use common::api::{CreateOrganizationUserRequest, ListUsersResponseItem};

fn role_badge(role: i32) -> &'static str {
    match role {
        3 => "badge badge-info",
        2 => "badge badge-success",
        _ => "badge badge-neutral",
    }
}

fn role_text(role: i32) -> &'static str {
    match role {
        3 => "超级管理员",
        2 => "管理员",
        _ => "成员",
    }
}

#[component]
pub fn OrganizationUsers() -> Element {
    let mut users = use_signal(Vec::<ListUsersResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut show_modal = use_signal(|| false);
    let mut new_username = use_signal(String::new);
    let mut new_display_name = use_signal(String::new());
    let mut new_email = use_signal(String::new);
    let mut new_password = use_signal(String::new);
    let mut new_role = use_signal(1i32);
    let mut creating = use_signal(|| false);

    let load = move || {
        loading.set(true);
        spawn(async move {
            match list_users().await {
                Ok(list) => users.set(list.users),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    };

    use_effect(move || { load(); });

    let handle_create = move |_| {
        spawn(async move {
            if new_username().is_empty() || new_password().is_empty() {
                error.set("用户名和密码不能为空".to_string());
                return;
            }
            creating.set(true);
            let req = CreateOrganizationUserRequest {
                username: new_username(),
                password_hash: new_password(),
                display_name: if new_display_name().is_empty() { None } else { Some(new_display_name()) },
                email: if new_email().is_empty() { None } else { Some(new_email()) },
                role: new_role(),
            };
            match create_user(req).await {
                Ok(_) => {
                    show_modal.set(false);
                    new_username.set(String::new());
                    new_display_name.set(String::new());
                    new_email.set(String::new());
                    new_password.set(String::new());
                    new_role.set(1);
                    load();
                }
                Err(e) => error.set(format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let users_list = users.read().clone();

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }

            div { class: "card-header",
                h2 { class: "card-title", "用户管理" }
                button { class: "btn btn-accent", onclick: move |_| show_modal.set(true), "+ 添加用户" }
            }

            if loading() {
                Loading {}
            } else if users_list.is_empty() {
                EmptyState { icon: "👥".to_string(), message: "暂无用户".to_string() }
            } else {
                table { class: "table",
                    thead { tr {
                        th { "用户名" }
                        th { "显示名称" }
                        th { "邮箱" }
                        th { "角色" }
                        th { "操作" }
                    }}
                    tbody {
                        for u in users_list.iter() {
                            {
                                let uid = u.user_id.clone();
                                rsx! {
                                    tr { key: "{uid}",
                                        td { style: "font-weight: 500;", "{u.username}" }
                                        td { class: "text-secondary", "{u.display_name.unwrap_or_default()}" }
                                        td { class: "text-mono text-muted", "{u.email.unwrap_or_default()}" }
                                        td { span { class: "{role_badge(u.role)}", "{role_text(u.role)}" } }
                                        td {
                                            button { class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let uid = uid.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_user(&uid).await {
                                                            error.set(format!("删除失败: {}", e));
                                                        } else {
                                                            load();
                                                        }
                                                    });
                                                },
                                                "删除"
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

        Modal {
            title: "添加用户".to_string(),
            show: show_modal(),
            on_close: move |_| show_modal.set(false),
            div {
                div { class: "form-group",
                    label { class: "form-label", "用户名 *" }
                    input { class: "form-input", value: "{new_username}",
                        oninput: move |e| new_username.set(e.value()) }
                }
                div { class: "form-group",
                    label { class: "form-label", "密码 *" }
                    input { class: "form-input", r#type: "password", value: "{new_password}",
                        oninput: move |e| new_password.set(e.value()) }
                }
                div { class: "form-group",
                    label { class: "form-label", "显示名称" }
                    input { class: "form-input", value: "{new_display_name}",
                        oninput: move |e| new_display_name.set(e.value()) }
                }
                div { class: "form-group",
                    label { class: "form-label", "邮箱" }
                    input { class: "form-input", r#type: "email", value: "{new_email}",
                        oninput: move |e| new_email.set(e.value()) }
                }
                div { class: "form-group",
                    label { class: "form-label", "角色" }
                    select { class: "form-select", value: "{new_role}",
                        onchange: move |e| new_role.set(e.value().parse().unwrap_or(1)),
                        option { value: "1", "成员" }
                        option { value: "2", "管理员" }
                        option { value: "3", "超级管理员" }
                    }
                }
            },
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_modal.set(false), "取消" }
                button { class: "btn btn-accent", disabled: creating(), onclick: handle_create,
                    if creating() { "创建中..." } else { "创建" }
                }
            }
        }
    }
}
```

- [ ] **Step 3: 实现个人信息页面 `pages/user/profile.rs`**

```rust
//! 个人信息

use dioxus::prelude::*;

use crate::api::organization::get_current_user_info;
use crate::components::state::{ErrorAlert, Loading, SuccessAlert};

#[component]
pub fn UserProfile() -> Element {
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut success = use_signal(String::new);
    let mut username = use_signal(String::new);
    let mut display_name = use_signal(String::new);
    let mut email = use_signal(String::new());
    let mut role = use_signal(1i32);
    let mut saving = use_signal(|| false);

    use_effect(move || {
        spawn(async move {
            match get_current_user_info().await {
                Ok(user) => {
                    username.set(user.username);
                    display_name.set(user.display_name.unwrap_or_default());
                    email.set(user.email.unwrap_or_default());
                    role.set(user.role);
                }
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    });

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }
            SuccessAlert { message: success() }

            div { class: "card-header",
                h2 { class: "card-title", "个人信息" }
            }

            if loading() {
                Loading {}
            } else {
                div { class: "form-group",
                    label { class: "form-label", "用户名" }
                    input { class: "form-input", disabled: true, value: "{username}" }
                }
                div { class: "form-group",
                    label { class: "form-label", "角色" }
                    input { class: "form-input", disabled: true,
                        value: "{match role() { 3 => \"超级管理员\", 2 => \"管理员\", _ => \"成员\" }}" }
                }
                div { class: "form-group",
                    label { class: "form-label", "显示名称" }
                    input { class: "form-input", value: "{display_name}",
                        oninput: move |e| display_name.set(e.value()) }
                }
                div { class: "form-group",
                    label { class: "form-label", "邮箱" }
                    input { class: "form-input", r#type: "email", value: "{email}",
                        oninput: move |e| email.set(e.value()) }
                }
                // 注意：更新用户信息需要后端 UpdateUserRequest，此处简化
                button { class: "btn btn-accent", disabled: saving(),
                    onclick: move |_| success.set("功能开发中".to_string()),
                    if saving() { "保存中..." } else { "保存" }
                }
            }
        }
    }
}
```

- [ ] **Step 4: 实现工具管理页面 `pages/finance/tools.rs`**

```rust
//! 工具管理

use dioxus::prelude::*;

use crate::api::finance::{delete_tool, list_tools, update_tool_status};
use crate::components::state::{EmptyState, ErrorAlert, Loading};
use common::api::ListToolsResponseItem;

#[component]
pub fn FinanceTools() -> Element {
    let mut tools = use_signal(Vec::<ListToolsResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);

    let load = move || {
        loading.set(true);
        spawn(async move {
            match list_tools().await {
                Ok(list) => tools.set(list.tools),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    };

    use_effect(move || { load(); });

    let tools_list = tools.read().clone();

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }

            div { class: "card-header",
                h2 { class: "card-title", "工具管理" }
            }

            if loading() {
                Loading {}
            } else if tools_list.is_empty() {
                EmptyState { icon: "🔧".to_string(), message: "暂无工具".to_string() }
            } else {
                table { class: "table",
                    thead { tr {
                        th { "名称" }
                        th { "协议" }
                        th { "状态" }
                        th { "操作" }
                    }}
                    tbody {
                        for t in tools_list.iter() {
                            {
                                let id = t.id.clone();
                                let status = t.status;
                                rsx! {
                                    tr { key: "{id}",
                                        td { style: "font-weight: 500;", "{t.name}" }
                                        td { span { class: "badge badge-neutral", "{t.protocol}" } }
                                        td {
                                            if status == 1 {
                                                span { class: "badge badge-success", "启用" }
                                            } else {
                                                span { class: "badge badge-error", "禁用" }
                                            }
                                        }
                                        td {
                                            if status == 1 {
                                                button { class: "btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let id = id.clone();
                                                        spawn(async move {
                                                            if let Err(e) = update_tool_status(&id, 0).await {
                                                                error.set(e);
                                                            } else { load(); }
                                                        });
                                                    },
                                                    "禁用"
                                                }
                                            } else {
                                                button { class: "btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let id = id.clone();
                                                        spawn(async move {
                                                            if let Err(e) = update_tool_status(&id, 1).await {
                                                                error.set(e);
                                                            } else { load(); }
                                                        });
                                                    },
                                                    "启用"
                                                }
                                            }
                                            button { class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let id = id.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_tool(&id).await {
                                                            error.set(format!("删除失败: {}", e));
                                                        } else { load(); }
                                                    });
                                                },
                                                "删除"
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

- [ ] **Step 5: 实现消息渠道管理页面 `pages/finance/message_channels.rs`**

```rust
//! 消息渠道管理

use dioxus::prelude::*;

use crate::api::finance::{delete_message_channel, list_message_channels, update_message_channel_status};
use crate::components::state::{EmptyState, ErrorAlert, Loading};
use common::api::ListMessageChannelsResponseItem;

#[component]
pub fn FinanceMessageChannels() -> Element {
    let mut channels = use_signal(Vec::<ListMessageChannelsResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);

    let load = move || {
        loading.set(true);
        spawn(async move {
            match list_message_channels().await {
                Ok(list) => channels.set(list.channels),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    };

    use_effect(move || { load(); });

    let channels_list = channels.read().clone();

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }
            div { class: "card-header",
                h2 { class: "card-title", "消息渠道管理" }
            }
            if loading() {
                Loading {}
            } else if channels_list.is_empty() {
                EmptyState { icon: "📡".to_string(), message: "暂无消息渠道".to_string() }
            } else {
                table { class: "table",
                    thead { tr { th { "名称" }, th { "类型" }, th { "状态" }, th { "操作" } }}
                    tbody {
                        for c in channels_list.iter() {
                            {
                                let id = c.id.clone();
                                let status = c.status;
                                rsx! {
                                    tr { key: "{id}",
                                        td { style: "font-weight: 500;", "{c.name}" }
                                        td { span { class: "badge badge-info", "{c.channel_type}" } }
                                        td {
                                            if status == 1 { span { class: "badge badge-success", "启用" } }
                                            else { span { class: "badge badge-error", "禁用" } }
                                        }
                                        td {
                                            if status == 1 {
                                                button { class: "btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let id = id.clone();
                                                        spawn(async move {
                                                            if let Err(e) = update_message_channel_status(&id, 0).await { error.set(e); } else { load(); }
                                                        });
                                                    }, "禁用"
                                                }
                                            } else {
                                                button { class: "btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let id = id.clone();
                                                        spawn(async move {
                                                            if let Err(e) = update_message_channel_status(&id, 1).await { error.set(e); } else { load(); }
                                                        });
                                                    }, "启用"
                                                }
                                            }
                                            button { class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let id = id.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_message_channel(&id).await { error.set(format!("删除失败: {}", e)); } else { load(); }
                                                    });
                                                }, "删除"
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

- [ ] **Step 6: 实现技能库页面 `pages/hr/skills.rs`**

```rust
//! 技能库管理

use dioxus::prelude::*;

use crate::api::hr::{delete_skill, list_skills};
use crate::components::state::{EmptyState, ErrorAlert, Loading};
use common::api::ListSkillsResponseItem;

#[component]
pub fn HrSkills() -> Element {
    let mut skills = use_signal(Vec::<ListSkillsResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);

    let load = move || {
        loading.set(true);
        spawn(async move {
            match list_skills().await {
                Ok(list) => skills.set(list.skills),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    };

    use_effect(move || { load(); });

    let skills_list = skills.read().clone();

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }
            div { class: "card-header",
                h2 { class: "card-title", "技能库" }
            }
            if loading() {
                Loading {}
            } else if skills_list.is_empty() {
                EmptyState { icon: "📚".to_string(), message: "暂无技能".to_string() }
            } else {
                table { class: "table",
                    thead { tr { th { "名称" }, th { "描述" }, th { "标签" }, th { "操作" } }}
                    tbody {
                        for s in skills_list.iter() {
                            {
                                let id = s.skill_id.clone();
                                rsx! {
                                    tr { key: "{id}",
                                        td { style: "font-weight: 500;", "{s.name}" }
                                        td { class: "text-secondary", "{s.description}" }
                                        td {
                                            for tag in &s.tags {
                                                span { class: "badge badge-neutral", style: "margin-right: 4px;", "{tag}" }
                                            }
                                        }
                                        td {
                                            button { class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let id = id.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_skill(&id).await { error.set(format!("删除失败: {}", e)); } else { load(); }
                                                    });
                                                }, "删除"
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

- [ ] **Step 7: 实现定时触发器页面 `pages/system/triggers.rs`**

```rust
//! 定时触发器管理

use dioxus::prelude::*;

use crate::api::system::{delete_cron_trigger, list_cron_triggers, pause_cron_trigger, resume_cron_trigger};
use crate::components::state::{EmptyState, ErrorAlert, Loading};
use common::api::ListCronTriggersResponseItem;

#[component]
pub fn SystemTriggers() -> Element {
    let mut triggers = use_signal(Vec::<ListCronTriggersResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);

    let load = move || {
        loading.set(true);
        spawn(async move {
            match list_cron_triggers().await {
                Ok(list) => triggers.set(list.triggers),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    };

    use_effect(move || { load(); });

    let triggers_list = triggers.read().clone();

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }
            div { class: "card-header",
                h2 { class: "card-title", "定时触发器" }
            }
            if loading() {
                Loading {}
            } else if triggers_list.is_empty() {
                EmptyState { icon: "⏰".to_string(), message: "暂无触发器".to_string() }
            } else {
                table { class: "table",
                    thead { tr { th { "名称" }, th { "Cron" }, th { "状态" }, th { "操作" } }}
                    tbody {
                        for t in triggers_list.iter() {
                            {
                                let id = t.trigger_id.clone();
                                let status = t.status;
                                rsx! {
                                    tr { key: "{id}",
                                        td { style: "font-weight: 500;", "{t.name}" }
                                        td { class: "text-mono", "{t.cron_expression}" }
                                        td {
                                            if status == 1 { span { class: "badge badge-success", "运行中" } }
                                            else if status == 0 { span { class: "badge badge-neutral", "暂停" } }
                                            else { span { class: "badge badge-error", "已禁用" } }
                                        }
                                        td {
                                            if status == 1 {
                                                button { class: "btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let id = id.clone();
                                                        spawn(async move {
                                                            if let Err(e) = pause_cron_trigger(&id).await { error.set(e); } else { load(); }
                                                        });
                                                    }, "暂停"
                                                }
                                            } else {
                                                button { class: "btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let id = id.clone();
                                                        spawn(async move {
                                                            if let Err(e) = resume_cron_trigger(&id).await { error.set(e); } else { load(); }
                                                        });
                                                    }, "恢复"
                                                }
                                            }
                                            button { class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let id = id.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_cron_trigger(&id).await { error.set(format!("删除失败: {}", e)); } else { load(); }
                                                    });
                                                }, "删除"
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

- [ ] **Step 8: 实现健康检查页面 `pages/system/health.rs`**

```rust
//! 健康检查

use dioxus::prelude::*;

use crate::api::system::check_health;
use crate::components::state::{ErrorAlert, Loading, SuccessAlert};

#[component]
pub fn SystemHealth() -> Element {
    let mut loading = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut success = use_signal(String::new);

    let check = move |_| {
        loading.set(true);
        error.set(String::new());
        success.set(String::new());
        spawn(async move {
            match check_health().await {
                Ok(msg) => success.set(format!("服务正常: {}", msg)),
                Err(e) => error.set(format!("健康检查失败: {}", e)),
            }
            loading.set(false);
        });
    };

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }
            SuccessAlert { message: success() }
            div { class: "card-header",
                h2 { class: "card-title", "健康检查" }
            }
            p { class: "text-secondary mb-6", "检查后端服务运行状态" }
            button { class: "btn btn-accent", disabled: loading(), onclick: check,
                if loading() { "检查中..." } else { "执行检查" }
            }
        }
    }
}
```

- [ ] **Step 9: 实现设置页面 `pages/settings.rs`**

```rust
//! 系统设置

use dioxus::prelude::*;

use crate::components::state::{ErrorAlert, SuccessAlert};
use crate::config::FrontendConfig;

#[component]
pub fn Settings() -> Element {
    let mut config = use_signal(FrontendConfig::load);
    let mut error = use_signal(String::new);
    let mut success = use_signal(String::new);

    let handle_save = move |_| {
        let cfg = config.read().clone();
        match cfg.save() {
            Ok(_) => success.set("配置已保存".to_string()),
            Err(e) => error.set(e),
        }
    };

    let handle_reset = move |_| {
        let mut cfg = config.write();
        cfg.reset_to_default();
        drop(cfg);
        success.set("已重置为默认配置".to_string());
    };

    let current = config.read().clone();

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }
            SuccessAlert { message: success() }
            div { class: "card-header",
                h2 { class: "card-title", "系统设置" }
            }
            div { class: "form-group",
                label { class: "form-label", "后端 API 地址" }
                input { class: "form-input", value: "{current.api_base_url}",
                    oninput: move |e| config.write().api_base_url = e.value(),
                    placeholder: "http://localhost:3000" }
                p { class: "form-hint", "配置保存在浏览器 localStorage 中" }
            }
            div { class: "flex gap-3",
                button { class: "btn btn-accent", onclick: handle_save, "保存配置" }
                button { class: "btn btn-ghost", onclick: handle_reset, "重置为默认" }
            }
        }
    }
}
```

- [ ] **Step 10: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/pages/
git commit -m "feat(frontend): implement all remaining CRUD pages (org/users/profile/tools/channels/skills/triggers/health/settings)"
```

---

## Task 13: 构建验证与修复

**Files:**
- Modify: 视编译错误而定

**目标：** 确保前端能正常编译通过。

- [ ] **Step 1: 检查 common crate 中缺失的 DTO 类型**

许多 API 客户端函数引用了 `common::api` 中的 DTO 类型（如 `CheckInitializedResponse`、`ListOrganizationsResponse` 等）。需要检查这些类型是否存在于 `common/src/api/` 中。

```bash
cd /Users/aman/Technology/rust/ai_orz
grep -r "pub struct CheckInitializedResponse" common/src/api/ || echo "MISSING: CheckInitializedResponse"
grep -r "pub struct ListOrganizationsResponse" common/src/api/ || echo "MISSING: ListOrganizationsResponse"
grep -r "pub struct LoginResponse" common/src/api/ || echo "MISSING: LoginResponse"
```

对于缺失的类型，在 `common/src/api/organization.rs` 中补充定义。例如：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckInitializedResponse {
    pub initialized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListOrganizationsResponse {
    pub organizations: Vec<OrganizationListItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub username: Option<String>,
    pub role: Option<i32>,
    pub organization_id: Option<String>,
}
```

**注意：** 先检查后端已有的 DTO 定义，确认字段名和类型是否匹配。如果后端返回的 JSON 结构不同，需要调整前端解析逻辑。

- [ ] **Step 2: 编译前端，逐个修复错误**

```bash
cd /Users/aman/Technology/rust/ai_orz/frontend
cargo check 2>&1 | tee /tmp/frontend-check.log
```

常见错误类型及修复策略：

1. **类型不存在**：在 `common/src/api/` 对应模块中添加 DTO 定义
2. **字段名不匹配**：检查后端 Handler 返回的 DTO，对齐字段名
3. **Route 枚举错误**：确保所有路由变体的 `#[route(...)]` 路径正确
4. **导入路径错误**：确保 `use crate::xxx` 路径正确
5. **组件 Props 不匹配**：检查 `#[component]` 宏的 Props 定义

每次修复后重新运行 `cargo check`，直到无错误。

- [ ] **Step 3: 修复所有编译错误后 Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/ common/
git commit -m "fix(frontend): resolve all compilation errors and align DTO types with backend"
```

- [ ] **Step 4: 构建验证**

```bash
cd /Users/aman/Technology/rust/ai_orz
cargo build -p frontend 2>&1 | tail -5
```

预期：`Finished` 无错误。

- [ ] **Step 5: 后端测试验证（确保 common crate 修改不影响后端）**

```bash
cd /Users/aman/Technology/rust/ai_orz
SQLX_OFFLINE=true cargo test --lib 2>&1 | tail -5
```

预期：所有测试通过。

- [ ] **Step 6: 最终 Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add .
git commit -m "feat(frontend): complete frontend refactor with CSS design system, router, and all CRUD pages"
```

---

## Self-Review Checklist

### Spec 覆盖检查

| 需求 | 对应 Task | 状态 |
|------|-----------|------|
| 引入 CSS 框架/设计系统 | Task 1 | ✅ Mistral 暖色调 CSS 变量 + 组件类 |
| 保持 Dioxus 技术栈 | 所有 Task | ✅ 纯 Rust + Dioxus 0.7 |
| 统一 API 客户端 | Task 2 | ✅ 统一 HTTP 客户端 + JWT 注入 |
| 全局状态管理 | Task 3 | ✅ AuthState + use_context |
| 按业务域补齐 API | Task 4 | ✅ 7 个域全部覆盖 |
| 基础 UI 组件库 | Task 5 | ✅ Button/Modal/State |
| 布局组件 | Task 6 | ✅ Navbar + AppLayout |
| Dioxus Router | Task 7 | ✅ 全部路由定义 |
| 登录闭环 | Task 8 | ✅ Token 持久化 + 跳转 |
| Agent 管理 | Task 9 | ✅ 列表/创建/删除 |
| 模型提供商管理 | Task 10 | ✅ 列表/创建/删除/测试 |
| 项目管理 | Task 11 | ✅ 列表/创建 |
| 其余基础页面 | Task 12 | ✅ 组织/用户/工具/渠道/技能/触发器/健康/设置 |
| 构建验证 | Task 13 | ✅ 编译 + 测试 |

### 类型一致性检查

- `AuthState` 在 store/auth.rs 定义，在 reception.rs 和 navbar.rs 中通过 `use_context::<Signal<AuthState>>()` 使用 ✅
- `api_get`/`api_post`/`api_put`/`api_delete` 在 api/mod.rs 定义，在各域 API 中调用 ✅
- `Route` 枚举在 pages/mod.rs 定义，在 navbar.rs 和 main.rs 中使用 ✅
- `Modal` 组件 Props 在 components/modal.rs 定义，在 agents.rs 和 model_providers.rs 中使用 ✅

---

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-07-12-frontend-refactor.md`. Two execution options:**

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
