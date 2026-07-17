# 前端移动端适配 Spec

## Why

当前前端（Dioxus 0.7 WASM）针对桌面端宽屏设计，仅在 2 处做了基础 `@media` 适配（`graph-container` 与 `reception-page`）。在 375px~768px 的移动端典型宽度下存在多处可用性阻塞：

1. **Navbar 严重溢出**：顶部 5+ 一级菜单项 + 4 个下拉按钮 + 用户菜单，在 375px 宽度下横向溢出或挤压，无法正常点击
2. **Chat 双栏挤压**：`chat-sidebar` 固定 320px + `chat-main` 并列，移动端无法共存展示
3. **数据表格横向滚动**：17 处 `.table` 在小屏出现大量横向滚动，列数多时关键列不可见
4. **看板固定列宽**：`kanban-column` 固定 300px，移动端水平滑动体验差
5. **Modal/Toast 越界**：`.modal-content` 固定 500px 宽，超出 375px 屏宽；Toast 在小屏可能覆盖整屏
6. **网格列数过多**：`overview-stats` 4 列、`detail-grid` auto-fit 220px，在 375px 下挤压不可读
7. **触摸交互缺陷**：按钮点击区域偏小（部分 `< 32px`），`hover` 才显示的操作（如 `message-actions`）在触摸设备不可达
8. **iOS 输入框放大**：输入框 `font-size < 16px` 触发 iOS Safari 聚焦时自动放大

本次适配目标是**在完全不破坏现有桌面端功能的前提下**，使所有页面在 375px 及以上宽度可用，并保持 768px 以上桌面端体验与现状完全一致。

## What Changes

### P0 - 核心阻塞修复（移动端可用性前提）

- **新增** 响应式基础设施：CSS 断点变量、全局移动端规则（字号、padding、触摸优化）
- **新增** `use_breakpoint` 信号 Hook：基于 `window.matchMedia("(max-width: 768px)")` 监听屏幕尺寸
- **修改** `frontend/src/layouts/navbar.rs`：移动端切换为汉堡菜单 + 抽屉式垂直导航
- **修改** `frontend/index.html`：新增 `.navbar-mobile-toggle`、`.navbar-drawer`、`.navbar-overlay` 样式
- **修改** `frontend/src/pages/message/chat.rs`：移动端 sidebar 改为可滑入/滑出的覆盖层，新增 `sidebar_open` 信号
- **修改** `frontend/index.html`：新增 `.chat-sidebar.open`、`.chat-mobile-back` 等样式

### P1 - 管理页可用性

- **修改** `frontend/index.html`：新增 `.table-responsive` 卡片化样式（移动端 thead 隐藏，每行转卡片）
- **修改** 17 处表格页面的 `<td>` 元素：补充 `data-label` 属性标注字段名
- **修改** `frontend/src/components/modal.rs`：移动端 Modal 全屏化（width: 100vw, height: 100vh, radius: 0）
- **修改** `frontend/index.html`：新增 `.modal-content-mobile`、`.toast-mobile` 样式
- **修改** `frontend/index.html`：新增网格响应式（`overview-stats` / `detail-grid` / `stats-grid` / `overview-grid` 在移动端降列）

### P2 - 完善体验

- **修改** `frontend/index.html`：`.kanban-board` 移动端纵向堆叠，`.filter-row` 改为纵向，`.page-header` 允许换行
- **修改** `frontend/index.html`：`.card-header` 移动端纵向布局，`.action-group` 允许换行
- **修改** `frontend/src/pages/reception.rs`：移动端品牌区与表单区纵向堆叠（已有 CSS，需校验 Reception 在 375px 下表单宽度合适）
- **修改** `frontend/index.html`：触摸优化（按钮最小 44x44px、`-webkit-tap-highlight-color`、`touch-action`）

### P3 - 质量保障

- **新增** 桌面端回归验证清单（768px / 1024px / 1440px 三档）
- **新增** 移动端验证清单（375px iPhone SE / 390px iPhone 14 / 768px iPad）
- **新增** 真机测试要求（iOS Safari + Android Chrome）

## Impact

- **Affected specs**: 无（首个前端响应式 spec）
- **Affected code**:
  - `frontend/index.html` - 新增大量 `@media` 规则与移动端样式类（约 +300 行 CSS）
  - `frontend/src/layouts/navbar.rs` - 新增汉堡菜单逻辑与抽屉组件
  - `frontend/src/layouts/app_layout.rs` - 无需修改（children 透传）
  - `frontend/src/pages/message/chat.rs` - 新增 `sidebar_open` 信号与移动端切换按钮
  - `frontend/src/components/modal.rs` - 无需修改（CSS 接管全屏化）
  - `frontend/src/hooks/mod.rs`（新增）- `use_breakpoint` Hook
  - 17 个含 `.table` 的页面文件 - `<td>` 补 `data-label` 属性
  - `frontend/Cargo.toml` - web-sys features 需补充 `MediaQueryList`、`MediaQueryListEvent`
- **Backward compatibility**: 所有桌面端（≥768px）样式与行为保持不变；新增的 CSS 类与 `data-label` 属性不影响桌面渲染
- **Performance**: 移动端 WASM 首屏可能较慢，本期不优化包体（留待 P3 之后单独处理）

## Constraints

- **双端兼容**：桌面端（≥768px）所有现有功能与视觉保持完全一致，不允许回归
- **不破坏功能**：所有交互（点击、表单、SSE、文件上传等）在移动端必须可用
- **不重构组件结构**：仅在现有组件上添加响应式分支，不替换为全新组件库
- **不引入新依赖**：仅使用现有 `web-sys`、`wasm-bindgen`、`dioxus` 能力
- **断点统一**：所有 `@media` 使用 `--breakpoint-md: 768px` 作为移动/桌面分界点
- **CSS 优先**：能用 CSS 解决的不写 JS（如 Modal 全屏化、表格卡片化），仅在必要时使用 `use_breakpoint` Hook（如 Navbar 切换组件结构）

## ADDED Requirements

### Requirement: 响应式断点系统

系统 SHALL 在 `:root` 定义统一断点变量，所有 `@media` 查询使用相同分界点。

#### Scenario: 断点定义
- **WHEN** 查看 `frontend/index.html` 的 `:root`
- **THEN** 包含 `--breakpoint-sm: 640px`、`--breakpoint-md: 768px`、`--breakpoint-lg: 1024px`
- **AND** 所有 `@media (max-width: ...)` 查询使用这些值（不硬编码）

### Requirement: use_breakpoint Hook

系统 SHALL 提供 `use_breakpoint` Hook，返回当前是否为移动端（≤768px），并在窗口尺寸变化时自动更新。

#### Scenario: 初始加载
- **WHEN** 组件挂载时窗口宽度 ≤ 768px
- **THEN** `use_breakpoint()` 返回 `true`

#### Scenario: 窗口缩放
- **WHEN** 用户从 1024px 缩放窗口至 600px
- **THEN** `use_breakpoint()` 返回值从 `false` 变为 `true`
- **AND** 所有使用该 Hook 的组件自动重渲染

#### Scenario: 资源清理
- **WHEN** 组件卸载
- **THEN** 自动移除 `MediaQueryList` 监听器，避免内存泄漏

### Requirement: Navbar 移动端汉堡菜单

在移动端（≤768px），Navbar SHALL 切换为汉堡菜单 + 抽屉式垂直导航，桌面端保持现状不变。

#### Scenario: 移动端默认状态
- **WHEN** 窗口宽度 ≤ 768px
- **THEN** Navbar 仅显示品牌 Logo + 汉堡按钮（☰）
- **AND** 隐藏所有一级菜单项与下拉按钮
- **AND** 用户头像菜单也收起

#### Scenario: 打开抽屉
- **WHEN** 移动端点击汉堡按钮
- **THEN** 从左侧滑出全屏抽屉（宽度 min(320px, 80vw)）
- **AND** 抽屉内垂直排列所有导航项（对话 / 消息搜索 / 人力资源 / 财务管理 / 项目管理 / 系统）
- **AND** 二级菜单以可折叠分组形式展示（点击一级项展开/收起二级）
- **AND** 抽屉底部显示用户信息与退出登录按钮
- **AND** 抽屉外区域显示半透明遮罩，点击遮罩关闭抽屉

#### Scenario: 路由跳转后自动关闭
- **WHEN** 抽屉打开状态下点击任意导航项
- **THEN** 跳转目标路由
- **AND** 自动关闭抽屉

#### Scenario: 桌面端不受影响
- **WHEN** 窗口宽度 > 768px
- **THEN** Navbar 显示完整水平菜单（与当前完全一致）
- **AND** 汉堡按钮隐藏
- **AND** 抽屉组件不渲染

### Requirement: Chat 页面移动端单栏

在移动端，Chat 页面 SHALL 改为单栏覆盖式 sidebar，桌面端保持双栏不变。

#### Scenario: 移动端默认状态
- **WHEN** 窗口宽度 ≤ 768px 且未选择项目
- **THEN** 显示项目列表 sidebar 占满屏幕
- **AND** `chat-main` 不渲染或隐藏

#### Scenario: 移动端选择项目
- **WHEN** 在移动端点击项目列表中的项目
- **THEN** sidebar 滑出消失
- **AND** `chat-main` 占满屏幕显示对话内容
- **AND** 在 chat-header 左侧显示"返回项目列表"按钮

#### Scenario: 返回项目列表
- **WHEN** 在移动端已选择项目状态下点击"返回"按钮
- **THEN** 切回 sidebar 显示项目列表
- **AND** 不丢失已选项目状态

#### Scenario: 桌面端双栏不变
- **WHEN** 窗口宽度 > 768px
- **THEN** sidebar 与 chat-main 并列显示（与当前完全一致）
- **AND** 不显示"返回"按钮

#### Scenario: 消息气泡宽度适配
- **WHEN** 移动端渲染消息气泡
- **THEN** `.message-bubble` 最大宽度为视口宽度的 85%
- **AND** 长文本自动换行不溢出

### Requirement: 数据表格移动端卡片化

在移动端，所有 `.table` SHALL 转换为卡片列表形式，桌面端保持表格形式不变。

#### Scenario: 移动端表格转卡片
- **WHEN** 窗口宽度 ≤ 640px 且页面包含 `.table`
- **THEN** `thead` 隐藏
- **AND** 每个 `tr` 渲染为独立卡片（带边框、圆角、间距）
- **AND** 每个 `td` 渲染为 `flex` 行，左侧显示 `data-label` 属性值作为字段名，右侧显示值
- **AND** 操作列（按钮）保持可点击，整行排布

#### Scenario: data-label 属性
- **WHEN** 渲染表格 `td`
- **THEN** 每个 `td` 必须包含 `data-label="字段名"` 属性
- **AND** 字段名与 `th` 文本一致

#### Scenario: 桌面端表格不变
- **WHEN** 窗口宽度 > 640px
- **THEN** 表格保持原有 `<table>` 渲染形式（与当前完全一致）

### Requirement: Modal 移动端全屏化

在移动端，Modal SHALL 全屏展示，桌面端保持 500px 居中不变。

#### Scenario: 移动端全屏
- **WHEN** 窗口宽度 ≤ 640px 且 Modal 打开
- **THEN** `.modal-content` 宽度为 100vw，高度为 100vh
- **AND** 圆角为 0
- **AND** 内部内容可垂直滚动

#### Scenario: 桌面端不变
- **WHEN** 窗口宽度 > 640px
- **THEN** `.modal-content` 保持 500px 宽度居中（与当前完全一致）

### Requirement: Toast 移动端位置适配

在移动端，Toast 容器 SHALL 横向占满屏幕（左右留 12px 边距），桌面端保持右上角不变。

#### Scenario: 移动端 Toast
- **WHEN** 窗口宽度 ≤ 640px 且 Toast 显示
- **THEN** Toast 容器 `left: 12px; right: 12px; top: 12px`
- **AND** 单条 Toast 宽度为 100%（不再有 max-width: 400px 限制）

### Requirement: 网格布局响应式

系统 SHALL 为所有多列网格在移动端降列，桌面端保持不变。

#### Scenario: overview-stats 四列降两列
- **WHEN** 窗口宽度 ≤ 768px
- **THEN** `.overview-stats` 改为 `grid-template-columns: repeat(2, 1fr)`
- **AND** 窗口宽度 ≤ 480px 时改为 1 列

#### Scenario: detail-grid 单列
- **WHEN** 窗口宽度 ≤ 768px
- **THEN** `.detail-grid` 改为 `grid-template-columns: 1fr`

#### Scenario: overview-grid 单列
- **WHEN** 窗口宽度 ≤ 768px
- **THEN** `.overview-grid` 改为 `grid-template-columns: 1fr`

#### Scenario: stats-grid 单列
- **WHEN** 窗口宽度 ≤ 768px
- **THEN** `.stats-grid` 改为 `grid-template-columns: 1fr`

### Requirement: 看板视图移动端纵向堆叠

在移动端，`.kanban-board` SHALL 纵向堆叠各列，桌面端保持横向滑动不变。

#### Scenario: 移动端看板
- **WHEN** 窗口宽度 ≤ 768px
- **THEN** `.kanban-board` 改为 `flex-direction: column`
- **AND** `.kanban-column` 宽度为 100%

### Requirement: 筛选行与卡片头部移动端适配

在移动端，横向排列的筛选行与卡片头部 SHALL 改为纵向，桌面端保持不变。

#### Scenario: filter-row 纵向
- **WHEN** 窗口宽度 ≤ 768px
- **THEN** `.filter-row` 改为 `flex-direction: column`
- **AND** `.filter-item` 宽度为 100%

#### Scenario: card-header 纵向
- **WHEN** 窗口宽度 ≤ 768px
- **THEN** `.card-header` 改为 `flex-direction: column; align-items: flex-start`
- **AND** 标题与操作按钮之间留 12px 间距

### Requirement: 触摸交互优化

系统 SHALL 为移动端优化触摸交互，桌面端不受影响。

#### Scenario: 按钮最小点击区域
- **WHEN** 窗口宽度 ≤ 768px
- **THEN** `.btn` 最小高度为 40px
- **AND** `.btn-sm` 最小高度为 36px
- **AND** `.navbar-dropdown-item` 最小高度为 44px（符合 iOS HIG）

#### Scenario: 输入框避免 iOS 放大
- **WHEN** 移动端聚焦 `.form-input`、`.form-textarea`、`.form-select`、`.chat-input`
- **THEN** 字号不小于 16px
- **AND** 不触发 iOS Safari 自动放大

#### Scenario: 取消点击高亮
- **WHEN** 移动端点击任意元素
- **THEN** 不显示蓝色或灰色点击高亮（`-webkit-tap-highlight-color: transparent`）

#### Scenario: hover 交互移动端降级
- **WHEN** 窗口宽度 ≤ 768px
- **THEN** `.message-item:hover .message-actions` 始终可见（不再依赖 hover）
- **AND** `.table-row-clickable:hover` 高亮效果保留但不依赖 hover 触发

## MODIFIED Requirements

### Requirement: Reception 页面响应式

现有 `.reception-page` 在 `@media (max-width: 768px)` 已改为纵向堆叠，SHALL 进一步验证 375px 极小屏下表单可用性。

#### Scenario: 375px 极小屏
- **WHEN** 窗口宽度 = 375px
- **THEN** 登录表单宽度不超过 100vw - 32px
- **AND** 输入框与按钮可正常点击
- **AND** 品牌区 headline 字号降为 1.5rem

## REMOVED Requirements

无（本次为纯新增适配，不删除任何现有需求）
