# Tailwind CSS + DaisyUI 集成迁移

> 🎯 **本文档定位**：前端样式栈迁移规划 + 落地结果快照（概览级，不含代码细节；字段级实现以代码路径为准）
>
> 状态：完成（2026-07-22 验收通过）
> 查阅场景：新增 UI 组件或页面时回看「组件类映射表 + 主题系统」两处即可，无需通读全文
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 项目架构规范 §1.2 前端技术栈说明
> - [frontend_architecture.md](../design/frontend_architecture.md) — 前端架构设计 §CSS 方案章节
> - [ui_design_system.md](../design/ui_design_system.md) — UI 设计系统规范

---

## 一、目标（为什么做）

前端从手写 CSS 迁移到 Tailwind CSS v4 + DaisyUI v5 组件库，存在以下问题：

| 问题维度 | 解决方式 |
|---------|---------|
| (a) 手写 CSS 类名膨胀，无统一设计系统 | 引入 DaisyUI v5 组件库（btn/card/modal/table 等 50+ 组件），统一语义类名 |
| (b) 无可换肤主题机制，品牌色硬编码 | 自定义 `orz-light` 主题保留 Mistral 暖橙品牌色，内置 30+ DaisyUI 主题一键切换 |
| (c) 构建工具链缺失 CSS 预编译 | 用 Trunk pre-build hook 调用 Tailwind CLI 编译，与 Rust 构建链解耦 |
| (d) 组件包装层对外 API 与内部实现耦合 | 保留 Rust 组件包装层（Button/Modal 等），内部改用 DaisyUI 类名，对外 API 不变 |

**收敛后效果**：完成从手写 CSS 到 Tailwind CSS v4 + DaisyUI v5 的全量迁移，30+ 主题切换功能上线，组件对外 API 零变动，所有页面功能和视觉风格保持一致。

---

## 二、架构思路（怎么做的）

渐进式迁移，四层逐步替换：

```
构建工具链（第一层搭建）
  ├─ package.json：npm 依赖（tailwindcss、daisyui、@tailwindcss/cli）
  ├─ tailwind.config.js：内容路径 + DaisyUI 插件 + 自定义主题
  ├─ Trunk.toml：pre_build hook 运行 npm run build:css
  └─ index.html：引入 output.css，保留必要自定义样式
  │
  ▼
基础 UI 组件（第二层替换）
  ├─ Button：btn-primary/btn-secondary/btn-ghost/btn-error/btn-sm
  ├─ Modal：modal + modal-box + modal-action
  ├─ Loading/EmptyState：loading-spinner + Tailwind 布局
  ├─ Toast：toast-end + alert-error/success/warning/info
  ├─ Stats：stats shadow + stat 组件
  └─ Graph：Canvas 不变，布局用 Tailwind
  │
  ▼
布局 + Hooks + 主题（第三层）
  ├─ Navbar：navbar + dropdown + menu + avatar
  ├─ AppLayout：container mx-auto px-4 py-8
  ├─ use_theme hook：读取/保存 localStorage，设置 documentElement data-theme
  └─ 设置页面：主题选择器（join + btn join-item + data-theme 属性）
  │
  ▼
逐页面迁移（第四层，两批完成）
  ├─ 第一批简单页面：登录页、组织管理、系统健康/备份/日志、资源列表页、HR、项目列表、个人信息
  └─ 第二批复杂页面：详情页 + 看板视图 + 图表页 + 聊天页面（最复杂，DaisyUI chat 组件）
```

**关键边界（行为红线，回归必保）**：
1. 渐进式迁移策略：先搭建工具链与现有手写 CSS 共存，再逐模块迁移，最后清理旧 CSS
2. Rust 组件包装层（Button/Modal/Loading 等）对外 API 不变，内部改用 DaisyUI 类名，减少页面改动量
3. 自定义 orz-light 主题色值与现有品牌色严格对齐：primary #fa520f（Mistral 橙）、accent #ffb83e（金色）、neutral #1f1f1f（黑）、base-100 #fffaeb（暖象牙）
4. 保留少量暂无法用 Tailwind 替换的复杂样式：chat 复杂布局、typing 动画、reception 渐变背景等（逐步替换）
5. Chat 页面使用 DaisyUI chat 组件：chat-start/chat-end + chat-bubble + chat-bubble-primary，保证消息气泡语义准确

---

## 三、涉及文件清单（读代码直接跳）

按分层索引，每行带可点击路径链接：

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| **构建工具链（新增文件）** | | |
| [frontend/package.json](../../frontend/package.json) | npm 依赖声明 | 新增：tailwindcss v4.1、daisyui v5、@tailwindcss/cli；build:css / watch:css scripts |
| [frontend/tailwind.config.js](../../frontend/tailwind.config.js) | Tailwind 配置 | 内容路径（index.html + src/**/*.rs）；DaisyUI 插件；自定义 orz-light 主题 + 30+ 内置主题 |
| [frontend/styles/input.css](../../frontend/styles/input.css) | Tailwind 入口 | @import "tailwindcss" + @plugin "daisyui"；保留 chat 布局、typing 动画、reception 渐变等必要自定义样式 |
| [Trunk.toml](../../Trunk.toml) | Trunk 构建配置 | pre_build hook：cd frontend && npm run build:css |
| [frontend/.gitignore](../../frontend/.gitignore) | 忽略规则 | 新增 node_modules/、styles/output.css |
| **UI 组件迁移** | | |
| [frontend/src/components/button.rs](../../frontend/src/components/button.rs) | Button 组件 | ButtonVariant 类映射改为 DaisyUI btn 变体（primary/secondary/outline/error/ghost） |
| [frontend/src/components/modal.rs](../../frontend/src/components/modal.rs) | Modal 组件 | 改用 DaisyUI dialog modal + modal-box + modal-action 结构 |
| [frontend/src/components/state.rs](../../frontend/src/components/state.rs) | Loading/EmptyState | Loading 用 DaisyUI loading-spinner；EmptyState 用 Tailwind 布局类重写 |
| [frontend/src/components/toast.rs](../../frontend/src/components/toast.rs) | Toast 通知 | 容器用 toast-end；每条用 alert-error/success/warning/info |
| [frontend/src/components/stats.rs](../../frontend/src/components/stats.rs) | Stats 卡片 | 改用 DaisyUI stats shadow + stat 组件 |
| **布局 + Hooks + 主题** | | |
| [frontend/src/layouts/navbar.rs](../../frontend/src/layouts/navbar.rs) | Navbar 布局 | 迁移到 DaisyUI navbar + dropdown + menu + avatar；保留移动端汉堡菜单逻辑 |
| [frontend/src/layouts/app_layout.rs](../../frontend/src/layouts/app_layout.rs) | AppLayout 布局 | 容器和间距改用 Tailwind 通用类（container mx-auto px-4 py-8 等） |
| [frontend/src/hooks/mod.rs](../../frontend/src/hooks/mod.rs) | use_theme hook | 新增：读取 localStorage 默认 orz-light；切换时设置 data-theme；持久化保存 |
| [frontend/src/pages/settings.rs](../../frontend/src/pages/settings.rs) | 设置页面 | 新增主题选择器 UI（DaisyUI join 组 + 30+ 主题按钮） |
| [frontend/src/main.rs](../../frontend/src/main.rs) | 根组件 | 在根元素应用 data-theme 属性 |
| **页面迁移（pages/**）** | | |
| [frontend/src/pages/**](../../frontend/src/pages/) | 全部页面模块 | 分批迁移 reception/organization/system/finance/hr/project/message 所有页面 CSS 类名到 Tailwind/DaisyUI |
| **零改动面（验证架构稳定性）** | | |
| 后端所有代码 / 前端业务逻辑（api/、store/、utils/、config.rs） | 对外契约不变 | 纯样式迁移，零业务改动；后端和前端业务逻辑测试断言原样通过 |

---

## 四、组件类名映射速查表（新增 UI 时套用）

新增或修改组件时按以下映射表套用 DaisyUI 类名，入口在 `frontend/src/components/` 各组件文件：

### 4.1 常用组件类名对照表

| 现有组件 / 旧类名 | DaisyUI / Tailwind 替代 | 说明和参考入口 |
|------------------|----------------------|--------------|
| Button (Primary) | `btn btn-primary` | 品牌橙按钮，参考 [button.rs::ButtonVariant::Primary](../../frontend/src/components/button.rs) |
| Button (Accent) | `btn btn-secondary` | accent 映射为 secondary（火焰橙） |
| Button (Secondary) | `btn btn-outline` | 自定义背景→描边幽灵按钮 |
| Button (Danger) | `btn btn-error` | danger→error（红） |
| Button (Ghost) | `btn btn-ghost` | 直接使用 DaisyUI ghost |
| Button (small) | `btn btn-sm` | 尺寸变体 |
| Modal overlay + content | `<dialog class="modal modal-open"><div class="modal-box">` | DaisyUI modal，参考 [modal.rs](../../frontend/src/components/modal.rs) |
| Modal close button | `<form method="dialog"><button class="btn btn-sm btn-circle btn-ghost">✕</button></form>` | modal 右上角关闭 |
| Modal footer | `<div class="modal-action">` | 底部操作区 |
| Loading spinner | `<span class="loading loading-spinner loading-lg">` | DaisyUI loading |
| Alert/Toast | `<div class="alert alert-error/success/warning/info">` | Toast 容器加 `toast toast-end` |
| Badge | `<span class="badge badge-success/...">` | DaisyUI badge 变体 |
| Card 容器 | `<div class="card bg-base-100 shadow-md">` + `card-title` | 卡片 + 标题 |
| `.table` / `.table th` | `table table-zebra` 或 `table table-pin-rows` | 表格斑马纹 + 表头 |
| `.form-group` / `.form-label` / `.form-input` | `form-control w-full mb-4` + `label label` + `input input-bordered w-full` | 表单三件套 |
| Chat 气泡 | `chat chat-start/chat-end` + `chat-bubble` + `chat-bubble-primary` | 参考 [pages/message/chat.rs](../../frontend/src/pages/message/chat.rs) |

> 主题切换统一入口：[hooks/mod.rs::use_theme](../../frontend/src/hooks/mod.rs) — 用 `data-theme` 属性切换，localStorage 持久化。

---

## 五、验收清单（2026-07-22 全部达成 ✅）

- [x] 构建工具链搭建完成：Tailwind CSS v4 + DaisyUI v5 + Trunk pre-build hook
- [x] 自定义 orz-light 主题色值与品牌严格对齐（Mistral 橙 #fa520f + 金色 #ffb83e + 暖象牙背景 #fffaeb）
- [x] 基础组件全部迁移：Button/Modal/Loading/EmptyState/Toast/Stats/Graph
- [x] 布局组件迁移：Navbar + AppLayout（移动端汉堡菜单正常）
- [x] 主题切换功能上线：use_theme hook + 设置页面主题选择器 + localStorage 持久化 + 30+ 内置主题
- [x] 逐页面迁移完成：第一批简单页面 + 第二批复杂页面 + Chat 页面（DaisyUI chat 组件）
- [x] 旧 CSS 清理完成：index.html 删除已迁移样式，input.css 只保留真正必要的自定义样式
- [x] 全量编译和测试通过：trunk build 零错误，前端测试 100% 通过
- [x] 全页面视觉回归检查：所有页面加载、交互、主题切换正常，无样式丢失

---

## 六、执行结果摘要（2026-07-22，子代理驱动）

| 模块 | 验证结果 |
|------|---------|
| Trunk build 编译 | 零 error，CSS 输出正常（Tailwind + DaisyUI + 自定义 overrides） |
| 前端全量测试 | 全部通过（纯样式迁移未改动测试用例） |
| 主题切换验证 | 30+ 主题即时切换，刷新后保持（localStorage 持久化） |
| Chat 页面功能 | 桌面端双栏布局 / SSE 实时消息 / 工具调用卡片 / 移动端单栏布局全部正常 |
| 后端回归测试 | 零改动，所有测试维持 100% 通过 |
| 迁移范围统计 | 新增 5 个构建文件 + 修改全部组件 / 布局 / 页面模块（约 30+ 源文件） |

### 与计划的偏离（如有）
无重大偏离，9 个 Task 按计划顺序执行完成。Chat 页面工具调用卡片和任务卡片为业务组件，DaisyUI 无直接对应，保留自定义样式但用 Tailwind 类重写，符合设计预期。

---

## 七、后续扩展路径（新增 UI / 页面 4 步模板）

> **核心不变量**：构建工具链 / 主题系统 / Rust 组件包装层对外 API 不动。

1. **新增通用组件**：[frontend/src/components/](../../frontend/src/components/) — 优先用 §四 速查表中的 DaisyUI 类组合；如 DaisyUI 无直接对应，用 Tailwind 工具类 + 少量 input.css 自定义类（input.css 中按区域组织注释）
2. **新增页面样式**：参考 [pages/project/project_detail.rs](../../frontend/src/pages/project/project_detail.rs) 的卡片 + 表单 + 网格布局模式，优先用 DaisyUI 语义类（card/form-control/table/stats）+ Tailwind 布局类（flex/grid/gap/mb-4）
3. **新增自定义主题**：[tailwind.config.js](../../tailwind/tailwind.config.js) 的 daisyui.themes 数组中追加主题对象（按 orz-light 模板改 primary/secondary/accent/neutral/base-100 等色值）；或直接使用 DaisyUI 30+ 内置主题名
4. **新增页面级动画**：简单动画用 Tailwind animate-* / transition-*；复杂动画（typing 动画、进度条动画等）在 [styles/input.css](../../frontend/styles/input.css) 的 "Custom overrides" 区域追加 @keyframes + 自定义类，页面中直接引用类名

完成。
