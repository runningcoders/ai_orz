# Tasks

按优先级 P0 → P3 顺序排列。每个 Task 完成后必须通过 `cargo check --lib --verbose`（后端）与 `cd frontend && cargo check --verbose`（前端）验证编译，并通过对应的桌面端回归检查（保证不破坏现状）。

---

## P0 - 核心阻塞修复（移动端可用性前提）

- [ ] **Task 1: 响应式基础设施（断点变量 + 全局移动端规则）**
  - [ ] SubTask 1.1: 修改 `frontend/index.html` 的 `:root`，新增 `--breakpoint-sm: 640px`、`--breakpoint-md: 768px`、`--breakpoint-lg: 1024px` 三个变量
  - [ ] SubTask 1.2: 在 `frontend/index.html` 的 `<style>` 末尾新增 `/* ===== Mobile Adaptation ===== */` 区块，包含全局触摸优化规则（`-webkit-tap-highlight-color: transparent`、`* { -webkit-tap-highlight-color: transparent; }`）
  - [ ] SubTask 1.3: 在该区块新增 `@media (max-width: 768px)` 全局规则：`body { font-size: 14px; }`、`.content-area { padding: var(--space-4) var(--space-3); }`、`.card { padding: var(--space-4); }`
  - [ ] SubTask 1.4: 在该区块新增 `@media (max-width: 768px)` 输入框字号规则：`.form-input, .form-textarea, .form-select, .chat-input { font-size: 16px; }`（避免 iOS 自动放大）
  - [ ] SubTask 1.5: 在该区块新增 `@media (max-width: 768px)` hover 降级规则：`.message-item .message-actions { opacity: 1; }`（不依赖 hover）
  - [ ] SubTask 1.6: 运行 `cd frontend && cargo check --verbose` 验证编译通过
  - [ ] SubTask 1.7: 桌面端回归：在 1024px 宽度下打开任意页面，验证视觉与交互无变化

- [ ] **Task 2: use_breakpoint Hook**
  - [ ] SubTask 2.1: 修改 `frontend/Cargo.toml`，在 `web-sys` features 数组中追加 `"MediaQueryList"`、`"MediaQueryListEvent"`
  - [ ] SubTask 2.2: 新增 `frontend/src/hooks/mod.rs`，导出 `use_breakpoint` 函数
  - [ ] SubTask 2.3: 在 `frontend/src/hooks/mod.rs` 实现 `use_breakpoint` Hook：
    ```rust
    //! 响应式 Hook
    use dioxus::prelude::*;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;
    use web_sys::MediaQueryList;

    /// 返回当前是否为移动端（≤ 768px）
    pub fn use_breakpoint() -> Signal<bool> {
        use_context(|| {
            let mut is_mobile = use_signal(|| false);
            use_effect(move || {
                if let Some(window) = web_sys::window() {
                    if let Ok(mql) = MediaQueryList::new_with_query(&window, "(max-width: 768px)") {
                        is_mobile.set(mql.matches());
                        let cb = Closure::new(move |e: MediaQueryListEvent| {
                            is_mobile.set(e.matches());
                        });
                        mql.add_listener_with_callback(cb.as_ref().unchecked_ref()).ok();
                        // 注意：Closure 在 effect 作用域内保活，组件卸载时随 effect 销毁
                        std::mem::forget(cb); // 简化：监听器随页面生命周期保活
                    }
                }
            });
            is_mobile
        })
    }
    ```
  - [ ] SubTask 2.4: 修改 `frontend/src/main.rs`，在文件顶部 `mod` 声明区添加 `mod hooks;`
  - [ ] SubTask 2.5: 运行 `cd frontend && cargo check --verbose` 验证编译通过
  - [ ] SubTask 2.6: 桌面端回归：打开页面，缩放窗口至 < 768px 再回到 1024px，验证无报错

- [ ] **Task 3: Navbar 移动端汉堡菜单**
  - [ ] SubTask 3.1: 在 `frontend/index.html` 的 Mobile Adaptation 区块新增 Navbar 移动端样式：
    ```css
    @media (max-width: 768px) {
      .navbar { padding: 0 var(--space-3); }
      .navbar-desktop-only { display: none !important; }
      .navbar-mobile-toggle {
        display: flex;
        background: transparent;
        border: none;
        color: #ecf0f1;
        font-size: 24px;
        cursor: pointer;
        padding: var(--space-2);
        align-items: center;
      }
      .navbar-drawer {
        position: fixed;
        top: 0; left: 0; bottom: 0;
        width: min(320px, 80vw);
        background: var(--color-pure-white);
        z-index: 1000;
        transform: translateX(-100%);
        transition: transform 0.25s ease;
        display: flex;
        flex-direction: column;
        padding: var(--space-4);
        overflow-y: auto;
      }
      .navbar-drawer.open { transform: translateX(0); }
      .navbar-overlay {
        position: fixed;
        top: 0; left: 0; right: 0; bottom: 0;
        background: rgba(31, 31, 31, 0.5);
        z-index: 999;
      }
      .navbar-drawer-item {
        display: block;
        padding: var(--space-3) var(--space-2);
        color: var(--color-text-primary);
        text-decoration: none;
        font-size: 15px;
        min-height: 44px;
        display: flex;
        align-items: center;
      }
      .navbar-drawer-section {
        font-size: 12px;
        color: var(--color-text-muted);
        text-transform: uppercase;
        letter-spacing: 0.5px;
        padding: var(--space-3) var(--space-2) var(--space-1);
        margin-top: var(--space-2);
      }
      .navbar-drawer-divider {
        border-top: 1px solid var(--color-border-light);
        margin: var(--space-3) 0;
      }
    }
    @media (min-width: 769px) {
      .navbar-mobile-toggle { display: none !important; }
    }
    ```
  - [ ] SubTask 3.2: 修改 `frontend/src/layouts/navbar.rs`，在 `Navbar` 组件顶部新增 `let is_mobile = use_breakpoint();` 与 `let mut drawer_open = use_signal(|| false);`
  - [ ] SubTask 3.3: 在 `navbar.rs` 顶部 `use` 区追加 `use crate::hooks::use_breakpoint;`
  - [ ] SubTask 3.4: 修改 `navbar.rs` 的 `rsx!` 结构，在 `<nav class="navbar">` 内：
    - 左侧：`Link { to: Route::MessageChat {}, class: "navbar-brand", "AI Orz" }`（保留）
    - 新增：`if is_mobile() { button { class: "navbar-mobile-toggle", onclick: move |_| drawer_open.set(true), "☰" } }`
    - 桌面菜单整体包裹：`if !is_mobile() { div { class: "navbar-desktop-only", /* 原有 navbar-section 内容 */ } }`
    - 移动端右侧用户头像：`if !is_mobile() { /* 原有用户菜单 */ }`，移动端简化为头像（点击打开抽屉）
  - [ ] SubTask 3.5: 在 `Navbar` 组件 `rsx!` 末尾追加移动端抽屉渲染：
    ```rust
    if is_mobile() && drawer_open() {
        rsx! {
            div { class: "navbar-overlay", onclick: move |_| drawer_open.set(false) }
            div { class: "navbar-drawer open",
                div { class: "navbar-drawer-section", "导航" }
                Link { to: Route::MessageChat {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "💬 对话" }
                Link { to: Route::MessageSearch {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "🔍 消息搜索" }

                div { class: "navbar-drawer-section", "人力资源" }
                Link { to: Route::HrAgents {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "Agent 管理" }
                Link { to: Route::HrSkills {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "技能库" }
                Link { to: Route::HrMemorySearch {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "记忆搜索" }
                Link { to: Route::HrKnowledgeGraph {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "知识图谱" }

                div { class: "navbar-drawer-section", "财务管理" }
                Link { to: Route::FinanceModelProviders {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "模型提供商" }
                Link { to: Route::FinanceTools {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "工具管理" }
                Link { to: Route::FinanceMessageChannels {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "消息渠道" }
                Link { to: Route::FinanceAttachments {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "附件管理" }
                Link { to: Route::FinanceMcpServers {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "MCP 服务器" }

                div { class: "navbar-drawer-section", "项目管理" }
                Link { to: Route::ProjectList {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "项目列表" }
                Link { to: Route::ProjectArtifacts {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "项目产物" }

                div { class: "navbar-drawer-section", "系统" }
                Link { to: Route::SystemTriggers {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "定时触发器" }
                Link { to: Route::SystemHealth {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "健康检查" }
                if is_admin {
                    Link { to: Route::SystemLogs {}, class: "navbar-drawer-item",
                        onclick: move |_| drawer_open.set(false), "日志查询" }
                    Link { to: Route::SystemBackup {}, class: "navbar-drawer-item",
                        onclick: move |_| drawer_open.set(false), "备份管理" }
                }

                div { class: "navbar-drawer-divider" }
                div { class: "navbar-drawer-section", "账户" }
                Link { to: Route::UserProfile {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "👤 个人信息" }
                if is_admin {
                    Link { to: Route::OrganizationInfo {}, class: "navbar-drawer-item",
                        onclick: move |_| drawer_open.set(false), "🏢 组织信息" }
                    Link { to: Route::OrganizationUsers {}, class: "navbar-drawer-item",
                        onclick: move |_| drawer_open.set(false), "👥 用户管理" }
                }
                Link { to: Route::Settings {}, class: "navbar-drawer-item",
                    onclick: move |_| drawer_open.set(false), "⚙️ 设置" }
                Link { to: Route::Reception {}, class: "navbar-drawer-item",
                    onclick: move |_| {
                        drawer_open.set(false);
                        crate::store::auth::clear_login_state();
                    }, "🚪 退出登录" }
            }
        }
    }
    ```
  - [ ] SubTask 3.6: 运行 `cd frontend && cargo check --verbose` 验证编译通过
  - [ ] SubTask 3.7: 桌面端回归：在 1024px 宽度下打开所有页面，验证 Navbar 显示与下拉菜单功能完全一致
  - [ ] SubTask 3.8: 移动端验证：在 375px 宽度下打开页面，点击汉堡按钮，验证抽屉滑出、点击导航项跳转后抽屉自动关闭

- [ ] **Task 4: Chat 页面移动端单栏**
  - [ ] SubTask 4.1: 在 `frontend/index.html` 的 Mobile Adaptation 区块新增 Chat 移动端样式：
    ```css
    @media (max-width: 768px) {
      .chat-container { flex-direction: column; position: relative; }
      .chat-sidebar {
        width: 100%;
        height: 100vh;
        position: absolute;
        top: 0; left: 0;
        z-index: 50;
        transform: translateX(-100%);
        transition: transform 0.25s ease;
      }
      .chat-sidebar.open { transform: translateX(0); }
      .chat-main { width: 100%; }
      .message-bubble { max-width: 85%; }
      .chat-messages { padding: 1rem; }
      .chat-input-area { padding: 0.75rem; }
      .chat-header { padding: 0.75rem 1rem; }
      .chat-mobile-back {
        background: transparent;
        border: none;
        color: var(--text-primary);
        cursor: pointer;
        font-size: 1.25rem;
        padding: var(--space-2);
        display: flex;
        align-items: center;
      }
    }
    @media (min-width: 769px) {
      .chat-mobile-back { display: none !important; }
    }
    ```
  - [ ] SubTask 4.2: 修改 `frontend/src/pages/message/chat.rs`，在 `MessageChat` 组件信号声明区追加 `let mut sidebar_open = use_signal(|| false);` 与 `let is_mobile = crate::hooks::use_breakpoint();`
  - [ ] SubTask 4.3: 修改 `handle_project_click` 闭包：在设置 `selected_project` 后追加 `if is_mobile() { sidebar_open.set(false); }`
  - [ ] SubTask 4.4: 修改 `rsx!` 中 `div { class: "chat-sidebar", ... }`，class 改为根据 `is_mobile() && sidebar_open()` 动态计算：`let sidebar_class = if is_mobile() && sidebar_open() { "chat-sidebar open" } else if is_mobile() { "chat-sidebar" } else { "chat-sidebar" };`
  - [ ] SubTask 4.5: 在 `chat-header` 内最左侧追加移动端返回按钮（仅 `is_mobile()` 且 `selected_project().is_some()` 时显示）：
    ```rust
    if is_mobile() && selected_project().is_some() {
        button {
            class: "chat-mobile-back",
            onclick: move |_| sidebar_open.set(true),
            "←"
        }
    }
    ```
  - [ ] SubTask 4.6: 调整 `chat-main` 显示逻辑：当 `is_mobile() && !sidebar_open() && selected_project().is_none()` 时，`chat-main` 不渲染（仅显示 sidebar）。在 `chat-main` 的 div 上添加条件渲染：`if !is_mobile() || selected_project().is_some() || sidebar_open() { ... }`
  - [ ] SubTask 4.7: 运行 `cd frontend && cargo check --verbose` 验证编译通过
  - [ ] SubTask 4.8: 桌面端回归：在 1024px 宽度下打开 Chat 页面，验证 sidebar 与 main 双栏并列显示，无返回按钮
  - [ ] SubTask 4.9: 移动端验证：在 375px 宽度下打开 Chat，点击项目后 sidebar 滑出，点击"←"返回 sidebar，已选项目状态保留

---

## P1 - 管理页可用性

- [ ] **Task 5: 数据表格移动端卡片化（CSS + 17 处页面改造）**
  - [ ] SubTask 5.1: 在 `frontend/index.html` 的 Mobile Adaptation 区块新增表格卡片化样式：
    ```css
    @media (max-width: 640px) {
      .table thead { display: none; }
      .table, .table tbody, .table tr, .table td { display: block; width: 100%; }
      .table tr {
        margin-bottom: var(--space-3);
        border: 1px solid var(--color-border-light);
        border-radius: var(--radius-md);
        padding: var(--space-2);
        background: var(--color-pure-white);
      }
      .table tbody tr:hover { background-color: var(--color-pure-white); }
      .table td {
        display: flex;
        justify-content: space-between;
        align-items: center;
        gap: var(--space-3);
        border-bottom: none;
        padding: var(--space-2) var(--space-3);
        text-align: right;
      }
      .table td::before {
        content: attr(data-label);
        font-weight: 500;
        color: var(--color-text-secondary);
        font-size: 13px;
        text-align: left;
        flex-shrink: 0;
      }
      .table td:empty { display: none; }
    }
    ```
  - [ ] SubTask 5.2: 修改 `frontend/src/pages/finance/message_channels.rs` 的表格 `<td>` 元素，每个 td 添加 `data-label: "字段名"` 属性（字段名与 th 一致，如 "名称"、"类型"、"状态"、"操作"）
  - [ ] SubTask 5.3: 同样修改 `frontend/src/pages/finance/attachments.rs` 表格 td 添加 data-label
  - [ ] SubTask 5.4: 同样修改 `frontend/src/pages/finance/model_providers.rs` 表格 td 添加 data-label
  - [ ] SubTask 5.5: 同样修改 `frontend/src/pages/finance/tools.rs` 表格 td 添加 data-label
  - [ ] SubTask 5.6: 同样修改 `frontend/src/pages/finance/mcp_servers.rs` 表格 td 添加 data-label
  - [ ] SubTask 5.7: 同样修改 `frontend/src/pages/system/logs.rs` 表格 td 添加 data-label
  - [ ] SubTask 5.8: 同样修改 `frontend/src/pages/system/backup.rs` 表格 td 添加 data-label
  - [ ] SubTask 5.9: 同样修改 `frontend/src/pages/system/triggers.rs` 表格 td 添加 data-label
  - [ ] SubTask 5.10: 同样修改 `frontend/src/pages/organization/users.rs` 表格 td 添加 data-label
  - [ ] SubTask 5.11: 同样修改 `frontend/src/pages/hr/agents.rs` 表格 td 添加 data-label
  - [ ] SubTask 5.12: 同样修改 `frontend/src/pages/hr/skills.rs` 表格 td 添加 data-label
  - [ ] SubTask 5.13: 同样修改 `frontend/src/pages/project/project_detail.rs` 两处表格 td 添加 data-label
  - [ ] SubTask 5.14: 同样修改 `frontend/src/pages/project/tasks.rs` 表格 td 添加 data-label（看板视图不受影响，仅 list 视图表格）
  - [ ] SubTask 5.15: 同样修改 `frontend/src/pages/project/projects.rs` 表格 td 添加 data-label
  - [ ] SubTask 5.16: 同样修改 `frontend/src/pages/project/artifacts.rs` 表格 td 添加 data-label
  - [ ] SubTask 5.17: 运行 `cd frontend && cargo check --verbose` 验证编译通过
  - [ ] SubTask 5.18: 桌面端回归：在 1024px 宽度下打开所有 17 个表格页面，验证表格渲染与操作完全一致（data-label 属性不影响桌面渲染）
  - [ ] SubTask 5.19: 移动端验证：在 375px 宽度下打开任一表格页面，验证 thead 隐藏、每行转为卡片、字段名显示在左侧、操作按钮可点击

- [ ] **Task 6: Modal 移动端全屏化 + Toast 适配**
  - [ ] SubTask 6.1: 在 `frontend/index.html` 的 Mobile Adaptation 区块新增 Modal 全屏化样式：
    ```css
    @media (max-width: 640px) {
      .modal-overlay { align-items: stretch; }
      .modal-content {
        width: 100vw;
        max-width: 100vw;
        height: 100vh;
        max-height: 100vh;
        border-radius: 0;
        padding: var(--space-4);
      }
      .modal-header { padding-top: var(--space-2); }
      .modal-footer { flex-direction: column-reverse; }
      .modal-footer .btn { width: 100%; }
    }
    ```
  - [ ] SubTask 6.2: 在 Mobile Adaptation 区块新增 Toast 移动端样式：
    ```css
    @media (max-width: 640px) {
      .toast-container {
        left: var(--space-3);
        right: var(--space-3);
        top: var(--space-3);
      }
      .toast {
        min-width: 0;
        max-width: 100%;
      }
    }
    ```
  - [ ] SubTask 6.3: 运行 `cd frontend && cargo check --verbose` 验证编译通过（Modal 组件本身无需修改，CSS 接管）
  - [ ] SubTask 6.4: 桌面端回归：在 1024px 宽度下打开任一带 Modal 的页面（如 Agent 创建），验证 Modal 500px 居中、Toast 右上角
  - [ ] SubTask 6.5: 移动端验证：在 375px 宽度下打开 Modal，验证全屏展示、内容可滚动；触发 Toast 验证横向占满（左右 12px 边距）

- [ ] **Task 7: 网格布局响应式**
  - [ ] SubTask 7.1: 在 `frontend/index.html` 的 Mobile Adaptation 区块新增网格响应式样式：
    ```css
    @media (max-width: 768px) {
      .overview-stats { grid-template-columns: repeat(2, 1fr); }
      .overview-grid { grid-template-columns: 1fr; }
      .detail-grid { grid-template-columns: 1fr; }
      .stats-grid { grid-template-columns: 1fr; }
    }
    @media (max-width: 480px) {
      .overview-stats { grid-template-columns: 1fr; }
    }
    ```
  - [ ] SubTask 7.2: 运行 `cd frontend && cargo check --verbose` 验证编译通过
  - [ ] SubTask 7.3: 桌面端回归：在 1024px 宽度下打开项目详情页、Agent 详情页，验证 4 列统计、auto-fit 网格与当前一致
  - [ ] SubTask 7.4: 移动端验证：在 375px 宽度下打开相同页面，验证网格降为 1-2 列，内容可读

---

## P2 - 完善体验

- [ ] **Task 8: 看板/筛选行/卡片头部适配**
  - [ ] SubTask 8.1: 在 `frontend/index.html` 的 Mobile Adaptation 区块新增样式：
    ```css
    @media (max-width: 768px) {
      .kanban-board { flex-direction: column; }
      .kanban-column { flex: 1; width: 100%; }
      .filter-row { flex-direction: column; }
      .filter-item { max-width: 100%; }
      .card-header { flex-direction: column; align-items: flex-start; gap: var(--space-3); }
      .page-header { flex-wrap: wrap; }
      .action-group { flex-wrap: wrap; }
    }
    ```
  - [ ] SubTask 8.2: 运行 `cd frontend && cargo check --verbose` 验证编译通过
  - [ ] SubTask 8.3: 桌面端回归：在 1024px 宽度下打开任务管理页（看板视图与列表视图）、各列表页筛选区，验证与当前一致
  - [ ] SubTask 8.4: 移动端验证：在 375px 宽度下打开相同页面，验证看板纵向堆叠、筛选条件纵向排列、卡片头部纵向

- [ ] **Task 9: 触摸交互优化**
  - [ ] SubTask 9.1: 在 `frontend/index.html` 的 Mobile Adaptation 区块新增触摸优化样式：
    ```css
    @media (max-width: 768px) {
      .btn { min-height: 40px; }
      .btn-sm { min-height: 36px; }
      .navbar-dropdown-item { min-height: 44px; }
      .navbar-drawer-item { min-height: 44px; }
      * { -webkit-tap-highlight-color: transparent; }
      .form-input, .form-textarea, .form-select { font-size: 16px; }
    }
    ```
  - [ ] SubTask 9.2: 运行 `cd frontend && cargo check --verbose` 验证编译通过
  - [ ] SubTask 9.3: 桌面端回归：在 1024px 宽度下验证按钮尺寸、输入框字号无变化
  - [ ] SubTask 9.4: 移动端验证：在 375px 宽度下点击按钮无蓝色高亮、输入框聚焦不放大、按钮易于点击

- [ ] **Task 10: Reception 页面 375px 验证**
  - [ ] SubTask 10.1: 在 `frontend/index.html` 的 `@media (max-width: 768px)` reception 区块补充 375px 极小屏规则：
    ```css
    @media (max-width: 375px) {
      .reception-brand-headline { font-size: 1.5rem; }
      .reception-form-side { padding: 1rem; }
      .reception-form-card { max-width: 100%; }
    }
    ```
  - [ ] SubTask 10.2: 运行 `cd frontend && cargo check --verbose` 验证编译通过
  - [ ] SubTask 10.3: 桌面端回归：在 1024px 宽度下打开 Reception 页面，验证双栏布局与当前一致
  - [ ] SubTask 10.4: 移动端验证：在 375px 宽度下打开 Reception 页面，验证表单可填写、按钮可点击、品牌区 headline 不溢出

---

## P3 - 质量保障

- [ ] **Task 11: 全页面回归验证**
  - [ ] SubTask 11.1: 桌面端 1440px 宽度下逐页打开并验证：MessageChat / MessageSearch / HrAgents / HrSkills / HrMemorySearch / HrKnowledgeGraph / FinanceModelProviders / FinanceTools / FinanceMessageChannels / FinanceAttachments / FinanceMcpServers / ProjectList / ProjectArtifacts / SystemTriggers / SystemHealth / SystemLogs / SystemBackup / OrganizationInfo / OrganizationUsers / UserProfile / Settings
  - [ ] SubTask 11.2: 桌面端 1024px 宽度下重复 11.1 验证
  - [ ] SubTask 11.3: 桌面端 768px 宽度下重复 11.1 验证（分界点）
  - [ ] SubTask 11.4: 移动端 375px 宽度下逐页打开并验证：所有页面可访问、Navbar 抽屉可用、表格转卡片、Modal 全屏、Toast 横向占满
  - [ ] SubTask 11.5: 移动端 390px 宽度下重复 11.4 验证
  - [ ] SubTask 11.6: iPad 768px 宽度下重复 11.4 验证（分界点，应显示桌面布局）
  - [ ] SubTask 11.7: 真机测试：iOS Safari（iPhone）打开应用，验证 Chat SSE 推送、文件上传、表单提交等核心交互
  - [ ] SubTask 11.8: 真机测试：Android Chrome 打开应用，重复 11.7 验证

- [ ] **Task 12: 编译与构建验证**
  - [ ] SubTask 12.1: 运行 `cd frontend && cargo check --verbose` 验证前端编译通过，无 warning
  - [ ] SubTask 12.2: 运行 `cargo check --lib --verbose` 验证后端编译通过，无 warning
  - [ ] SubTask 12.3: 运行 `cd frontend && cargo build --release --target wasm32-unknown-unknown` 验证 WASM 构建成功
  - [ ] SubTask 12.4: 运行 `cargo test --lib` 验证后端测试全部通过

---

# Task Dependencies

- Task 1 无依赖，可立即开始
- Task 2 无依赖，可与 Task 1 并行
- Task 3 依赖 Task 1（CSS 基础）与 Task 2（use_breakpoint Hook）
- Task 4 依赖 Task 1（CSS 基础）与 Task 2（use_breakpoint Hook）
- Task 5、6、7 依赖 Task 1（CSS 基础），彼此之间无依赖，可并行
- Task 8、9、10 依赖 Task 1，彼此之间无依赖，可并行
- Task 11、12 依赖所有任务完成

# Parallelizable Groups

- **Group A（P0 基础）**: Task 1 + Task 2 可并行
- **Group B（P0 应用）**: Task 3 + Task 4 可并行（依赖 Group A 完成）
- **Group C（P1）**: Task 5 + Task 6 + Task 7 可并行（依赖 Group A 完成）
- **Group D（P2）**: Task 8 + Task 9 + Task 10 可并行（依赖 Group A 完成）
- **Group E（P3）**: Task 11 + Task 12 可并行（依赖所有完成）

# Execution Strategy

按 P0 → P1 → P2 → P3 顺序推进，每个 Task 完成后立即执行桌面端回归验证（关键：保证不破坏现状）。每个 P 阶段完成后可做一次 commit，便于回滚。

- P0 完成：移动端核心可用（Navbar + Chat），可提交一次 commit
- P1 完成：管理页全部可用，可提交一次 commit
- P2 完成：体验完善，可提交一次 commit
- P3 完成：全量验证通过，可提交最终 commit
