# Checklist

## P0 - 核心阻塞修复

### Task 1: 响应式基础设施
- [ ] `:root` 新增 `--breakpoint-sm: 640px`、`--breakpoint-md: 768px`、`--breakpoint-lg: 1024px`
- [ ] 新增 `/* ===== Mobile Adaptation ===== */` 区块
- [ ] 全局触摸优化：`-webkit-tap-highlight-color: transparent`
- [ ] `@media (max-width: 768px)` 全局字号 padding 优化
- [ ] 输入框 `font-size: 16px`（避免 iOS 放大）
- [ ] hover 降级（message-actions 始终可见）
- [ ] `cd frontend && cargo check --verbose` 通过
- [ ] 桌面端 1024px 回归无变化

### Task 2: use_breakpoint Hook
- [ ] `frontend/Cargo.toml` web-sys features 追加 `MediaQueryList`、`MediaQueryListEvent`
- [ ] 新增 `frontend/src/hooks/mod.rs`
- [ ] 实现 `use_breakpoint` 函数（基于 `MediaQueryList::new_with_query`）
- [ ] `frontend/src/main.rs` 添加 `mod hooks;`
- [ ] `cd frontend && cargo check --verbose` 通过
- [ ] 桌面端缩放窗口验证无报错

### Task 3: Navbar 移动端汉堡菜单
- [ ] CSS：`.navbar-mobile-toggle`、`.navbar-drawer`、`.navbar-overlay`、`.navbar-drawer-item`、`.navbar-drawer-section`、`.navbar-drawer-divider`
- [ ] CSS：`@media (min-width: 769px)` 隐藏 `.navbar-mobile-toggle`
- [ ] `navbar.rs` 引入 `use_breakpoint`
- [ ] `navbar.rs` 新增 `drawer_open` 信号
- [ ] 桌面菜单包裹 `.navbar-desktop-only` 容器（仅 `!is_mobile()` 渲染）
- [ ] 移动端渲染汉堡按钮（仅 `is_mobile()` 渲染）
- [ ] 移动端抽屉包含所有导航项（对话 / 消息搜索 / 人力资源 / 财务管理 / 项目管理 / 系统）
- [ ] 抽屉二级菜单扁平化展示（不需点击展开，直接列在 section 下）
- [ ] 抽屉底部用户菜单（个人信息 / 组织信息 / 用户管理 / 设置 / 退出登录）
- [ ] Admin 角色判断：日志查询与备份管理仅 is_admin 显示
- [ ] 点击任意导航项后自动关闭抽屉
- [ ] 点击遮罩关闭抽屉
- [ ] `cd frontend && cargo check --verbose` 通过
- [ ] 桌面端 1024px 回归：Navbar 水平菜单与下拉功能完全一致
- [ ] 移动端 375px 验证：汉堡按钮显示、抽屉滑出、点击跳转后关闭

### Task 4: Chat 页面移动端单栏
- [ ] CSS：`.chat-sidebar.open`、`.chat-mobile-back` 移动端样式
- [ ] CSS：`@media (min-width: 769px)` 隐藏 `.chat-mobile-back`
- [ ] CSS：移动端 `.message-bubble { max-width: 85%; }`
- [ ] `chat.rs` 新增 `sidebar_open` 信号
- [ ] `chat.rs` 引入 `use_breakpoint`
- [ ] `handle_project_click` 中移动端关闭 sidebar
- [ ] sidebar class 根据状态动态计算
- [ ] chat-header 左侧移动端返回按钮（仅 `is_mobile() && selected_project().is_some()` 显示）
- [ ] `cd frontend && cargo check --verbose` 通过
- [ ] 桌面端 1024px 回归：双栏并列、无返回按钮
- [ ] 移动端 375px 验证：项目列表全屏、点击项目后切到对话、返回按钮可切回列表、已选项目状态保留

## P1 - 管理页可用性

### Task 5: 数据表格移动端卡片化
- [ ] CSS：`@media (max-width: 640px)` thead 隐藏、tr 转卡片、td 转 flex 行、`::before` 显示 data-label
- [ ] `finance/message_channels.rs` td 添加 data-label
- [ ] `finance/attachments.rs` td 添加 data-label
- [ ] `finance/model_providers.rs` td 添加 data-label
- [ ] `finance/tools.rs` td 添加 data-label
- [ ] `finance/mcp_servers.rs` td 添加 data-label
- [ ] `system/logs.rs` td 添加 data-label
- [ ] `system/backup.rs` td 添加 data-label
- [ ] `system/triggers.rs` td 添加 data-label
- [ ] `organization/users.rs` td 添加 data-label
- [ ] `hr/agents.rs` td 添加 data-label
- [ ] `hr/skills.rs` td 添加 data-label
- [ ] `project/project_detail.rs` 两处表格 td 添加 data-label
- [ ] `project/tasks.rs` 列表视图表格 td 添加 data-label
- [ ] `project/projects.rs` td 添加 data-label
- [ ] `project/artifacts.rs` td 添加 data-label
- [ ] `cd frontend && cargo check --verbose` 通过
- [ ] 桌面端 1024px 回归：17 个表格页面渲染与操作完全一致
- [ ] 移动端 375px 验证：thead 隐藏、行转卡片、字段名显示、操作按钮可点击

### Task 6: Modal 全屏化 + Toast 适配
- [ ] CSS：`@media (max-width: 640px)` `.modal-content` 全屏（100vw/100vh/无圆角）
- [ ] CSS：移动端 `.modal-footer` 纵向、按钮 100% 宽
- [ ] CSS：`@media (max-width: 640px)` `.toast-container` 横向占满（左右 12px）
- [ ] CSS：移动端 `.toast` 宽度 100%、无 max-width 限制
- [ ] `cd frontend && cargo check --verbose` 通过
- [ ] 桌面端 1024px 回归：Modal 500px 居中、Toast 右上角
- [ ] 移动端 375px 验证：Modal 全屏可滚动、Toast 横向占满

### Task 7: 网格布局响应式
- [ ] CSS：`@media (max-width: 768px)` `.overview-stats` 2 列
- [ ] CSS：`@media (max-width: 480px)` `.overview-stats` 1 列
- [ ] CSS：`@media (max-width: 768px)` `.overview-grid`、`.detail-grid`、`.stats-grid` 1 列
- [ ] `cd frontend && cargo check --verbose` 通过
- [ ] 桌面端 1024px 回归：4 列统计、auto-fit 网格不变
- [ ] 移动端 375px 验证：网格降为 1-2 列、内容可读

## P2 - 完善体验

### Task 8: 看板/筛选行/卡片头部适配
- [ ] CSS：移动端 `.kanban-board` 纵向、`.kanban-column` 100% 宽
- [ ] CSS：移动端 `.filter-row` 纵向、`.filter-item` 100% 宽
- [ ] CSS：移动端 `.card-header` 纵向、gap 12px
- [ ] CSS：移动端 `.page-header` 允许换行
- [ ] CSS：移动端 `.action-group` 允许换行
- [ ] `cd frontend && cargo check --verbose` 通过
- [ ] 桌面端 1024px 回归：看板横向滑动、筛选行横向、卡片头部横向
- [ ] 移动端 375px 验证：看板纵向堆叠、筛选条件纵向、卡片头部纵向

### Task 9: 触摸交互优化
- [ ] CSS：移动端 `.btn` 最小高度 40px、`.btn-sm` 36px
- [ ] CSS：移动端 `.navbar-dropdown-item`、`.navbar-drawer-item` 最小高度 44px
- [ ] CSS：移动端 `-webkit-tap-highlight-color: transparent` 全局
- [ ] CSS：移动端输入框 `font-size: 16px`
- [ ] `cd frontend && cargo check --verbose` 通过
- [ ] 桌面端 1024px 回归：按钮尺寸、输入框字号无变化
- [ ] 移动端 375px 验证：点击无高亮、输入框不放大、按钮易点击

### Task 10: Reception 375px 验证
- [ ] CSS：`@media (max-width: 375px)` `.reception-brand-headline` 1.5rem
- [ ] CSS：`@media (max-width: 375px)` `.reception-form-side` padding 1rem
- [ ] CSS：`@media (max-width: 375px)` `.reception-form-card` max-width 100%
- [ ] `cd frontend && cargo check --verbose` 通过
- [ ] 桌面端 1024px 回归：Reception 双栏布局不变
- [ ] 移动端 375px 验证：表单可填写、按钮可点击、headline 不溢出

## P3 - 质量保障

### Task 11: 全页面回归验证
- [ ] 桌面端 1440px 21 个页面逐一验证
- [ ] 桌面端 1024px 21 个页面逐一验证
- [ ] 桌面端 768px 21 个页面逐一验证（分界点）
- [ ] 移动端 375px 21 个页面逐一验证
- [ ] 移动端 390px 21 个页面逐一验证
- [ ] iPad 768px 验证（分界点，应显示桌面布局）
- [ ] iOS Safari 真机测试：Chat SSE 推送、文件上传、表单提交
- [ ] Android Chrome 真机测试：重复 iOS 验证项

### Task 12: 编译与构建验证
- [ ] `cd frontend && cargo check --verbose` 通过、无 warning
- [ ] `cargo check --lib --verbose` 通过、无 warning
- [ ] `cd frontend && cargo build --release --target wasm32-unknown-unknown` 成功
- [ ] `cargo test --lib` 全部通过

## 21 个验证页面清单

| # | 页面 | 路由 | 重点验证项 |
|---|------|------|-----------|
| 1 | 对话 | MessageChat | sidebar/main 切换、SSE、文件上传、消息气泡 |
| 2 | 消息搜索 | MessageSearch | 搜索表单、结果列表 |
| 3 | Agent 管理 | HrAgents | 表格卡片化、创建 Modal |
| 4 | 技能库 | HrSkills | 表格卡片化、创建 Modal |
| 5 | 记忆搜索 | HrMemorySearch | 搜索表单、结果列表 |
| 6 | 知识图谱 | HrKnowledgeGraph | 图谱渲染、详情面板 |
| 7 | 模型提供商 | FinanceModelProviders | 表格、详情页、切换 Modal |
| 8 | 工具管理 | FinanceTools | 表格卡片化 |
| 9 | 消息渠道 | FinanceMessageChannels | 表格卡片化、测试连接 |
| 10 | 附件管理 | FinanceAttachments | 表格卡片化、上传 |
| 11 | MCP 服务器 | FinanceMcpServers | 表格卡片化、同步 |
| 12 | 项目列表 | ProjectList | 表格卡片化、创建 Modal |
| 13 | 项目详情 | ProjectDetail | 概览统计、任务表格、Agent 表格 |
| 14 | 项目产物 | ProjectArtifacts | 表格卡片化 |
| 15 | 任务管理 | Tasks | 列表表格、看板视图、统计网格 |
| 16 | 定时触发器 | SystemTriggers | 表格卡片化、Cron 预设 |
| 17 | 健康检查 | SystemHealth | 状态展示 |
| 18 | 日志查询 | SystemLogs | 日志表格、筛选、详情展开 |
| 19 | 备份管理 | SystemBackup | 备份列表、创建/恢复 Modal |
| 20 | 组织信息 | OrganizationInfo | 表单 |
| 21 | 用户管理 | OrganizationUsers | 表格卡片化、创建 Modal |

## 双端兼容红线

以下情况视为破坏双端兼容，必须立即回滚或修复：

1. 桌面端（≥768px）任意页面视觉与适配前不一致
2. 桌面端任意交互（点击、表单、Modal、SSE、上传）失效
3. 移动端核心交互不可用（Navbar 抽屉、Chat 单栏、表格卡片）
4. `cargo check` 出现 warning 或错误
5. 现有测试用例失败

## 验收标准

- [ ] P0 全部完成：移动端可访问所有页面、可进行对话
- [ ] P1 全部完成：移动端可查看所有列表、可创建/编辑实体（Modal 全屏）
- [ ] P2 全部完成：移动端体验流畅（触摸友好、看板可用、网格可读）
- [ ] P3 全部完成：桌面端零回归、移动端真机验证通过、编译测试全绿
