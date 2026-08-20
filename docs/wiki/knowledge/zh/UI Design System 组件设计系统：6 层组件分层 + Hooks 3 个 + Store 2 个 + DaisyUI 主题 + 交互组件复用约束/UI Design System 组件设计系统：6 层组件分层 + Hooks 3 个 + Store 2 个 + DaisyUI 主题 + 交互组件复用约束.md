---
kind: knowledge_card
name: UI Design System 组件设计系统：6 层组件分层 + Hooks 3 个 + Store 2 个 + DaisyUI 主题 + 交互组件复用约束
category: 前端可视化
scope:
- frontend/src/components/**/*.rs
- frontend/src/hooks/**/*.rs
- frontend/src/store/**/*.rs
- frontend/src/layouts/**/*.rs
- frontend/styles/**/*.css
- frontend/src/pages/**/*.rs
source_files:
- frontend/src/components/button.rs#L1-L49
- frontend/src/components/modal.rs#L1-L43
- frontend/src/components/toast.rs#L1-L103
- frontend/src/components/state.rs
- frontend/src/components/stats.rs
- frontend/src/components/graph.rs
- frontend/src/components/graph_canvas.rs
- frontend/src/hooks/use_resource.rs#L1-L40
- frontend/src/hooks/use_workspace_data.rs
- frontend/src/store/auth.rs#L1-L100
- frontend/src/store/toast.rs#L1-L100
- frontend/src/layouts/app_layout.rs#L1-L27
- frontend/src/layouts/navbar.rs#L1-L80
- frontend/styles/input.css:L1-L120
- docs/design/ui_design_system.md
- docs/archive/plan-archive/统计图表Phase1基础设施与时序图展示重构.md
- docs/archive/plan-archive/知识图谱推荐起点与组件复用重构.md
- docs/wiki/zh/content/前端应用/组件系统/组件系统.md
- docs/wiki/zh/content/前端应用/UI 样式与主题.md
- docs/wiki/zh/content/前端应用/前端架构设计.md
- docs/wiki/zh/content/前端应用/前端应用.md

---

# §1 概述与定位

本知识卡沉淀 AI Orz 前端（Dioxus 0.7 WebAssembly）的 **Design System 组件设计系统**规范，覆盖 6 层组件分层架构、3 个自定义 Hooks、2 个全局 Store、DaisyUI 5 主题体系、以及交互组件的复用约束。项目已从 Mistral 设计系统内联样式迁移到 **Tailwind CSS v4 + DaisyUI v5** 组件库实现（2026-07-25 里程碑），自定义 `orz-light` 主题承袭 Mistral 暖色基因（品牌橙 + 暖象牙底色），同时开放 30+ 内置主题供用户切换。

# §2 关键文件表

| 角色 | 路径 | 关键锚点 |
|------|------|----------|
| Button 组件（5 variants） | frontend/src/components/button.rs | L1-L49 `ButtonVariant` 枚举（Primary/Accent/Secondary/Danger/Ghost）映射到 DaisyUI 类；`btn-sm` 尺寸控制；统一事件透传 |
| Modal 模态框 | frontend/src/components/modal.rs | L1-L43 DaisyUI `modal modal-open` 类；点击遮罩关闭 + `modal-action` footer 插槽；右上角 `✕` 关闭按钮 |
| Toast 通知系统 | frontend/src/components/toast.rs | L1-L103 全局容器 `toast toast-top toast-end z-[9999]`；4 种类型（Success/Error/Warning/Info）映射 alert 类；滑入+离开动画；底部进度条；自动 dismiss 生命周期 |
| State 状态指示器 | frontend/src/components/state.rs | Loading/Success/Error/Empty 四态组件，配合 use_resource 使用 |
| Stats 统计指标卡 | frontend/src/components/stats.rs | Agent/Tool/Skill 等实体详情页的指标数字展示 + 环比趋势箭头 |
| Graph SVG 图 | frontend/src/components/graph.rs | 关系图 SVG 原生渲染（兜底 Canvas 方案）；节点/边/标签绘制 |
| GraphCanvas 知识图谱 Canvas | frontend/src/components/graph_canvas.rs | HUD 风格 Canvas 渲染栈；节点选中态扫描环、未选中呼吸光晕、边流光、四角 HUD 装饰 |
| use_resource Hook | frontend/src/hooks/use_resource.rs | L1-L40 资源三态（Loading/Ready/Failed）封装；自动首次触发 + reload 手动刷新；fetcher 闭包异步加载 |
| use_workspace_data Hook | frontend/src/hooks/use_workspace_data.rs | Workspace 侧边栏频道/会话聚合数据加载 |
| use_require_auth Hook（布局内引用） | frontend/src/layouts/app_layout.rs | AppLayout 中调用 use_require_auth 守卫；未登录显示 loading 或重定向登录 |
| use_breakpoint Hook（Navbar 内引用） | frontend/src/layouts/navbar.rs | 响应式断点判断：桌面显示完整导航，移动端汉堡抽屉 |
| Auth Store | frontend/src/store/auth.rs | L1-L100 localStorage 持久化登录态标志位 + role；HttpOnly Cookie JWT 持有；mark_logged_in / save_role / clear_login_state / logout / is_logged_in；`AuthState {logged_in, username, role, org_id}` |
| Toast Store | frontend/src/store/toast.rs | L1-L100 `ToastState` Copy 结构体：Signal<Vec<ToastItem>> + next_id；success/error/warning/info 四接口；dismiss 移除；默认时长 3000-5000ms |
| AppLayout 主布局 | frontend/src/layouts/app_layout.rs | L1-L27 权限守卫 use_require_auth → Navbar + main（container mx-auto px-4 py-6 max-w-7xl）结构 |
| Navbar 顶部导航 | frontend/src/layouts/navbar.rs | L1-L80 品牌 + 桌面导航（对话/消息搜索/工作台/HR/Finance/Project/System）+ 移动端汉堡抽屉；登出流程：先调后端 logout API 清 cookie，再清前端 store |
| Tailwind + DaisyUI 主题配置 | frontend/styles/input.css | L1-L120 `@import "tailwindcss"` + `@plugin "daisyui"` 声明 31 主题；`[data-theme="orz-light"]` 自定义 oklch 色值；HUD 流光条 keyframes `.hud-streak`；知识图谱 `.kg-bg` 背景网格 |
| Design 规范文档 | docs/design/ui_design_system.md | Mistral 暖色系设计原则 + DaisyUI 5 迁移落地章节；HUD 驾驶舱效果说明；组件清单参考 |
| Plan 统计图表基础设施 | docs/archive/plan-archive/统计图表Phase1基础设施与时序图展示重构.md | charts/ 子目录组件（donut_chart/line_chart）落地计划与复用约束 |
| Plan 知识图谱组件复用 | docs/archive/plan-archive/知识图谱推荐起点与组件复用重构.md | Graph/GraphCanvas/KanbanCanvas/WorkspaceGraph/CanvasScene 复用层级划分 |

# §3 架构与约定

## 3.1 6 层组件分层架构

```
Layer 6 - Page Level（页面级）
  frontend/src/pages/**/*         业务页面模块，组合 Layer 1-5 组件

Layer 5 - Layout Level（布局级）
  frontend/src/layouts/
    ├─ app_layout.rs              AppLayout：权限守卫 + Navbar + Main 容器
    └─ navbar.rs                  Navbar：品牌/导航/用户菜单（响应式断点）

Layer 4 - Composite Business（复合业务组件）
  frontend/src/components/
    ├─ chat/                      Chat 域：ChatSidePanel/MessageBubble/TypingIndicator/ToolCallsTab
    ├─ charts/                    Chart 域：DonutChart/LineChart
    ├─ canvas_scene.rs / chart_scene.rs  Canvas 场景管理
    ├─ graph.rs / graph_canvas.rs / kanban_canvas.rs  图渲染：SVG/Canvas/Force 布局
    ├─ workspace_graph.rs / relation_graph.rs  关系图变体
    └─ runtime_panel.rs / process_detail.rs / artifact_meta_modal.rs / task_progress.rs  业务复合组件

Layer 3 - Interactive Basic（基础交互组件）
  frontend/src/components/
    ├─ button.rs                  Button（5 variants + size）
    ├─ modal.rs                   Modal（遮罩/关闭按钮/footer）
    ├─ confirm_dialog.rs          ConfirmDialog（确认对话框，基于 Modal）
    ├─ toast.rs                   Toast（全局通知容器 + 单条）
    ├─ code_editor.rs             CodeEditor 代码编辑
    ├─ markdown.rs                MarkdownRenderer 渲染
    ├─ searchable_select.rs       可搜索下拉
    └─ create_http_tool.rs        HTTP Tool 创建表单子组件

Layer 2 - Display Basic（基础展示组件）
  frontend/src/components/
    ├─ state.rs                   State 指示器（Loading/Success/Error/Empty）
    ├─ stats.rs                   Stats 指标卡（数字+趋势）
    ├─ gauge.rs / aop_gauge.rs    仪表盘
    └─ hud_palette.rs             HUD 调色板

Layer 1 - Foundation（基础层，非 Rust 组件）
  frontend/styles/input.css       Tailwind + DaisyUI 主题 + 自定义动画+HUD 类
  frontend/src/store/*.rs         全局状态（Auth/Toast）
  frontend/src/hooks/*.rs         自定义 Hooks（use_resource/use_workspace_data 等）
```

**新增组件必须先判断所属层级：** Layer 3 及以下必须通用可复用（不出现具体业务字段名）；Layer 4 可组合 Layer 1-3 但应保持页面无关。

## 3.2 DaisyUI 5 主题体系

- **CSS 框架**: Tailwind CSS v4.1（`@import "tailwindcss"` + `@theme` 配置）
- **组件库**: DaisyUI v5（`@plugin "daisyui"` 引入）
- **默认主题**: `orz-light`（Mistral 暖色系基因），CSS 变量值：
  - Primary: `oklch(0.63 0.24 50)` ≈ Mistral Orange `#fa520f`
  - Base 100: `oklch(0.98 0.02 85)` ≈ Warm Ivory `#fffaeb`
  - Secondary: 暖橙色；Accent: 金色；Radius 统一阶（0.375rem/0.5rem）
- **30+ 内置主题切换**: 设置 `<html data-theme="xxx">` 即可生效，切换入口在 Navbar 用户菜单
- **HUD 高级视觉特效**: 自定义 `.hud-streak` 流光条 + `.kg-bg` 知识图谱背景，配合动画 keyframes

## 3.3 Hooks（3 个自定义）+ Store（2 个全局）

**Hooks 3 个：**
1. `use_resource<T>` → `(Signal<ResourceState<T>>, impl FnMut())`：统一数据加载三态（Loading/Ready/Failed），自动首次加载，返回 reload 句柄
2. `use_require_auth()` → `bool`：AppLayout 权限守卫，检查登录态并回填用户信息
3. `use_breakpoint()` → `bool`：响应式判断（true = 移动端），控制 Navbar 抽屉模式

**Store 2 个：**
1. `AuthState`（auth.rs）：`Signal` 驱动的 Copy 结构体；localStorage 持久化 logged_in 标志位+role；HttpOnly Cookie JWT 持有（前端不直接持有 token）；`logout()` 函数级登出流程
2. `ToastState`（toast.rs）：双 Signal 设计（toasts: Signal<Vec<ToastItem>> + next_id: Signal<u64>），因此 ToastState 整体是 Copy 类型可安全 move 到任意闭包；`success()/error()/warning()/info()` 四方法

# §4 硬约束与红线

1. **禁止自定义 CSS 组件类红线**：自 DaisyUI 迁移完成后，**禁止**新增自定义 CSS 类实现通用组件（如自定义 `.my-button`）；通用交互组件统一使用 DaisyUI 类名（`btn`/`modal`/`toast`/`alert`/`card`/`table` 等）。特殊 HUD 视觉效果除外（`.hud-streak`/`.kg-bg` 在 input.css 中已有定义）
2. **Button Variant 映射一致性红线**：所有 Button 组件必须走 `button.rs:ButtonVariant` 枚举映射到 DaisyUI 类：Primary→btn-primary、Accent→btn-secondary、Secondary→btn-outline、Danger→btn-error、Ghost→btn-ghost。**禁止**在页面组件内直接手写 `btn btn-primary` 类跳过 Button 组件
3. **Toast 全局唯一容器红线**：`ToastContainer` 组件**必须**且只能在根组件（App）中实例化一次；任何页面需要 Toast 时调用 `use_toast().success(...)` 接口，**禁止**在页面内部手动渲染 Toast
4. **Auth HttpOnly Cookie 红线**：前端 JWT 基于 HttpOnly Cookie 持有，**禁止**在 localStorage、sessionStorage、内存 Signal 中存储真实 JWT token；AuthState 中仅保存登录状态标志位和角色信息。登出必须同时执行：(a) 调用后端 logout API 使 cookie 失效；(b) 清 localStorage 标志；(c) 重置内存 AuthState Signal
5. **Graph/GraphCanvas 选择红线**：关系图渲染优先使用 `GraphCanvas`（Canvas HUD 风格高性能），`graph.rs` 仅作为 SVG 兜底方案。两组件接受相同的节点/边数据结构输入，保证切换不重写调用方
6. **use_resource 非重写红线**：任何需要异步加载数据的场景**必须**使用 `use_resource` Hook，禁止在页面组件中手写 `use_signal + spawn + use_effect` 裸实现三态管理；use_resource 覆盖 Loading/Ready/Failed 三态 + 竞态防护模式
7. **DaisyUI 类名非简写红线**：在 `frontend/src/**/*.rs` 中使用 DaisyUI 类名必须使用全称（如 `btn-primary` 而非自定义缩写）；禁止通过 `@apply` 批量封装为新类——直接内联在 class 字符串中，便于工具类 grep
8. **响应式断点单一来源红线**：所有桌面/移动端布局差异判断必须走 `use_breakpoint()` Hook，**禁止**在各处手写 `window.inner_width < 768` 独立判断；断点阈值统一在 use_breakpoint 实现中维护
