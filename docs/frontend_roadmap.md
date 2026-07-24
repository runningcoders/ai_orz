# 前端优化路线图

> 最后更新：2026-07-24（新增方向六：前沿渲染技术调研）

---

## 总览

基于当前前端各模块完成度和用户体验，规划五个优化方向，按优先级排序。

| 方向 | 优先级 | 完成度 | 核心缺口 |
|------|--------|--------|----------|
| 一、知识图谱交互完善 | P0 | ✅ 100% | 已全部完成 |
| 二、对话功能补全 | P1 | ✅ 98% | 消息编辑/删除（可选） |
| 三、任务管理可视化 | P2 | ✅ 95% | 任务完成率饼图（可选） |
| 四、Agent 详情页增强 | P3 | ⏳ 80% | 统计面板 UI（后端 API 已就绪） |
| 五、移动端适配 | P0 | ✅ 100% | 已全部完成（2026-07-17） |
| 六、前沿渲染技术调研 | - | 🔬 调研中 | 长期方向储备，详见下文 |

---

## 方向一：知识图谱交互完善

**目标**：从"只能搜索初始节点"升级为"可交互探索的知识图谱"

### 1.1 节点点击展开关联
- 点击节点调用 `search_memory_with_traversal` API，传入 `seed_node_ids` + `traversal_depth=1`
- 将遍历结果中的新节点和关系添加到图谱
- 已展开的节点不重复展开（用 `expanded_nodes` HashSet 记录）

### 1.2 节点类型差异化
- 不同记忆类型用不同颜色和形状区分
  - knowledge_node：蓝色圆形（主要节点）
  - short_term：绿色圆形（短期记忆）
  - relation：灰色菱形（关系节点）
- 图例说明

### 1.3 详情侧边栏
- 点击节点后右侧弹出面板
- 显示：完整内容、类型、摘要、匹配分数
- 关闭按钮收起面板

### 1.4 布局优化
- 改进圆形布局算法：中心节点居中，关联节点围绕分布
- 中心节点放大
- 边标签优化（避免重叠）

---

## 方向二：对话功能补全

**目标**：从"基础文本对话"升级为"富媒体交互对话"

### 2.1 消息类型差异化渲染 ✅
- Text → 普通气泡
- ToolCallRequest/ToolCallResult → 折叠卡片，可展开看工具名、参数、结果
- TaskAssignment → 任务分配卡片，带标题和状态标签

### 2.2 附件系统接入 ✅
- 消息气泡中展示附件（图片预览、文件图标+文件名+大小）
- 输入框旁加附件上传按钮
- 后端附件上传/下载 API 已实现，前端对接

### 2.3 "正在输入"效果 ✅
- 收到用户消息后、Agent 回复前显示 typing 指示器
- 基于 SSE 消息状态推断（Processing → 显示 typing，Processed → 隐藏）

### 2.4 消息时间分组 ✅
- 按日期分组显示消息
- 日期分隔线样式

### 2.5 消息复制 ✅
- hover 消息气泡显示复制按钮
- 点击复制到剪贴板，toast 提示结果

### 2.6 快捷指令 ✅
- 输入 `/` 开头显示快捷指令菜单
- `/clear` 清空对话、`/help` 显示帮助
- 支持 ↑↓ 选择、Enter 执行、Esc 关闭

---

## 方向三：任务管理可视化

**目标**：从"只能查看和切换状态"升级为"完整任务管理"

### 3.1 独立任务管理页面 ✅
- 按项目/状态/负责人筛选
- 列表视图 + 看板视图切换
- 任务卡片：标题、进度条、优先级标签、负责人

### 3.2 任务创建/编辑弹窗 ✅
- 标题、描述、优先级、截止日期、负责人 Agent
- 关联项目选择

### 3.3 任务详情页 ✅
- 基本信息 + 状态流转
- 进度更新
- 标签和依赖展示

### 3.4 进度可视化 ✅
- 项目整体进度条
- 任务状态分布统计
- 任务概览统计卡片（总数/进行中/待处理/已完成）

---

## 方向四：Agent 详情页增强

**目标**：从"管理视角"升级为"交互+洞察视角"

### 4.1 Agent 对话入口 ✅
- 详情页加"发起对话"按钮
- 跳转到对话页并自动选择与该 Agent 的对话

### 4.2 记忆面板 ✅
- 新增 Tab：记忆浏览
- 按类型筛选（短期记忆/知识节点/关系）
- 搜索和列表展示
- 关系类型显示源节点和目标节点

### 4.3 统计面板（⏳ 后端 API 已就绪，待前端实现）
- 唤醒次数、任务完成数、工具调用次数
- 时间趋势图（最近 7 天/30 天）
- 常用工具排行
- **后端 API 状态**：✅ 已就绪，通过实体详情接口 query 参数 `with_stats`/`with_model_call_stats` 按需返回
  - `GET /api/v1/agents/{id}?with_stats=true&with_model_call_stats=true&stats_interval=daily`
  - `GET /api/v1/projects/{id}?with_stats=true&with_model_call_stats=true`
  - `GET /api/v1/tasks/{id}?with_stats=true&with_model_call_stats=true`
  - `GET /api/v1/tools/{id}?with_stats=true`
  - `GET /api/v1/model-providers/{id}?with_model_call_stats=true`

---

## 方向五：移动端适配 ✅（2026-07-17 完成）

**目标**：在不破坏桌面端现有功能的前提下，使所有页面在 375px 及以上宽度可用，并保持 768px 以上桌面端体验与现状完全一致。

### 5.1 响应式基础设施 ✅
- `:root` 新增 `--breakpoint-sm` (640px)、`--breakpoint-md` (768px)、`--breakpoint-lg` (1024px) 三个断点变量
- 新增 Mobile Adaptation CSS 区块：全局触摸优化（`-webkit-tap-highlight-color: transparent`）、字号 padding 调整、iOS 输入框 16px 防放大、hover 降级
- 新增 `hooks/mod.rs` 的 `use_breakpoint` Hook：基于 `window.matchMedia("(max-width: 768px)")` 监听，`use_context_provider` 全局共享

### 5.2 Navbar 移动端汉堡菜单 ✅
- 移动端隐藏桌面菜单，显示汉堡按钮（☰）
- 点击展开左侧抽屉（`.navbar-drawer`，宽度 min(320px, 80vw)）+ 半透明遮罩
- 按"导航 / 人力资源 / 财务管理 / 项目管理 / 系统 / 账户"分组垂直排列所有路由项
- 点击任意导航项后自动关闭抽屉，点击遮罩同样关闭

### 5.3 Chat 页面移动端单栏 ✅
- 移动端 sidebar 改为覆盖式（CSS transform 滑入滑出）
- 未选项目时仅显示 sidebar；已选项目时仅显示 chat-main
- chat-header 左侧新增"←"返回按钮（桌面端隐藏）
- 消息气泡最大宽度 85%

### 5.4 数据表格移动端卡片化 ✅
- CSS `@media (max-width: 640px)` thead 隐藏、tr 转卡片、td 转 flex 行
- `::before` 伪元素显示 `data-label` 属性作为字段名标签
- 13 处表格共 75 个 td 添加 `data-label` 属性（与 th 文本一致）

### 5.5 Modal/Toast/网格/看板适配 ✅
- Modal 移动端全屏化（100vw/100vh、圆角 0、底部按钮纵向）
- Toast 横向占满（左右 12px 边距）
- 网格降列（overview-stats 4→2→1、其他网格 1 列）
- 看板纵向堆叠、筛选行/卡片头部纵向

### 5.6 触摸交互优化 ✅
- 按钮最小点击区域（btn 40px、btn-sm 36px、navbar 44px）
- 输入框 font-size 16px 避免 iOS Safari 聚焦自动放大
- 全局 `-webkit-tap-highlight-color: transparent` 取消点击高亮

### 5.7 Reception 375px 极小屏 ✅
- headline 字号降为 1.5rem
- form-side padding 调整为 1rem
- form-card max-width 100%

### 验证结果
- 前端 `cargo check` 通过
- 后端 `cargo check --lib` 通过
- WASM release 构建成功
- 后端 732 个测试全部通过
- 桌面端（≥769px）所有页面视觉与交互零回归

### 后续优化方向（未实现）
- WASM 包体优化：移动端首屏加载较慢，可考虑代码分割或骨架屏
- 真机测试：需在 iOS Safari + Android Chrome 上验证核心交互（Chat SSE、文件上传、表单提交）

---

## 方向六：前沿渲染技术调研（2026-07-24）

> 本节为长期方向储备，记录可能影响前端架构演进的前沿技术调研结论，当前均不引入生产。

### 6.1 pretext — 纯 JS 文本测量与布局库

**项目**：[chenglou/pretext](https://github.com/chenglou/pretext)（react-motion 作者）
**核心能力**：绕开 DOM 测量（`getBoundingClientRect`/`offsetHeight` 触发昂贵 reflow），用浏览器字体引擎做一次性测量 + 纯算术布局。
- `prepare(text, font)` 一次性分析（归一化空白、分段、glue 规则、canvas 测量）
- `layout(prepared, width, lineHeight)` 纯算术返回 `{height, lineCount}`，无 DOM reflow
- `layoutWithLines()` 返回分行数据，供 Canvas/WebGL/SVG 手动渲染
- 支持 RTL/阿拉伯语/复杂脚本，有 rich-inline（chip/mention/code span）流式布局

**价值场景（不限于 Canvas 渲染）**：
| 场景 | 价值 |
|------|------|
| 虚拟化列表（按高度 occlusion） | 精确高度避免滚动跳变，原生测量会导致卡顿 |
| Canvas/WebGL 文本渲染 | Canvas 无原生文本排版，pretext 提供分行数据 |
| Masonry / JS-driven 布局 | shrink-wrap 紧凑容器宽度，原生 CSS 难做 |
| AI 开发期校验 | 浏览器无关验证 label 不溢出 |

**当前评估结论**：❌ 不引入
- 我们用 SVG + Dioxus，浏览器原生排版引擎处理换行，无 reflow 痛点
- pretext 是 JS 库，Dioxus 调用需走 JS interop，集成成本不低
- 当前规模未到瓶颈

**重新评估信号**：
- 消息列表/记忆列表上量到虚拟化刚需，原生高度测量导致滚动卡顿
- 决定将知识图谱从 SVG 迁移到 Canvas/WebGL（pretext 提供分行数据）
- 出现复杂 rich-text 场景（mention chip + code span + 复杂换行规则）

### 6.2 HTML-in-Canvas — WICG 提案

**项目**：[WICG/html-in-canvas](https://github.com/WICG/html-in-canvas)（孵化中）
**核心能力**：让 Canvas 直接渲染真正的 HTML/DOM 内容，终结 Canvas 二十年"渲染不了富文本"的困境。
- `layoutsubtree` 属性：标记 canvas 子元素，视觉渲染被截胡存为快照，但仍参与布局/可访问性/事件命中
- `drawElementImage()` / `texElementImage2D()` / `copyElementImageToTexture()`：把 DOM 元素绘制到 2D Canvas / WebGL / WebGPU，返回 DOMMatrix 同步 DOM 位置
- `paint` 事件：只在子元素视觉渲染真正变化时触发重绘，一帧只跑一次

**生态进展**：three.js / PlayCanvas / vfx-js 已适配，有 polyfill（three-html-render）。当前仅 Chrome Canary + Brave Stable（Chromium 147+）通过 flag 支持。

**当前评估结论**：❌ 不引入
- 我们用 SVG，没有"Canvas 渲染不了 HTML"的痛点
- 提案孵化阶段 + flag 才能用 + 16 个 open issues，不能进生产
- 事件模型与 Dioxus VDOM diff 有架构摩擦

**重新评估信号**：
- 提案进入 WICG 官方推荐且 Chromium 默认开启（去掉 flag）
- 知识图谱节点规模到 1000+，SVG 渲染出现明显卡顿
- 需要 WebGL/WebGPU 纹理化 HTML 面板（3D 场景内的交互式 HTML UI）

### 6.3 长期愿景：游戏化交互体验

两个技术是"游戏化交互"方向的关键基础设施：

**当前架构**：Dioxus → SVG → 浏览器原生排版
- 优势：原生事件、可访问性、富文本排版开箱即用
- 局限：传统管理页面布局范式，交互想象力受 DOM 约束

**目标架构（长期）**：Dioxus → Canvas/WebGL + pretext + HTML-in-Canvas
- Canvas/WebGL 提供像素级控制：粒子、动画、3D 场景、物理效果
- pretext 解决 Canvas 文本排版：精确分行、shrink-wrap、虚拟化高度
- HTML-in-Canvas 让 Canvas 内渲染交互式 HTML UI：按钮、表单、富文本，且保留可访问性
- 三者配合可实现：沉浸式工作空间、3D 知识图谱节点内嵌交互面板、节点粒子动效、空间布局打破栅格约束

**演进路径建议**：
1. 短期（当前）：持续完善 SVG + Dioxus 架构，积累交互设计经验
2. 中期：当 SVG 节点数到瓶颈时，先尝试节点虚拟化 + Canvas 2D 手绘 + DOM 覆盖层
3. 长期：HTML-in-Canvas 标准化后，评估全面迁移到 Canvas/WebGL 渲染管线

**关键前提**：任何迁移决策前，必须先证明 SVG 架构在目标场景下确实存在不可接受的性能或交互瓶颈，避免为技术而技术。

### 6.4 游戏化重构完整路径（2026-07-24）

**核心架构原则**：混合渲染，渐进迁移。Canvas/WebGL 管视觉场景，DOM 管交互表单，各司其职。

```
当前架构：Dioxus → DOM (SVG 图谱 + Tailwind 表单 + 全部交互)

目标架构：
  Dioxus → DOM        ← 主交互层：表单、文本输入、菜单、Modal（原生事件 + 可访问性）
         ↓
  Canvas/WebGL        ← 主视觉层：图谱、动画场景、粒子、物理效果
         ↓
  DOM 覆盖层          ← Canvas 上的 HTML 控件（HTML-in-Canvas 标准化后）
```

**五阶段渐进路径**：

| 阶段 | 目标 | 技术选型 | 交付价值 |
|------|------|----------|----------|
| 0 | 当前基线 | Dioxus + SVG + Tailwind | 已完成 |
| 1 | Canvas 渲染基础设施 | `web-sys` Canvas 2D + Dioxus 组件封装 | Canvas 能力就绪，不破坏现有页面 |
| 2 | 知识图谱迁移 Canvas | Canvas 2D + 力导向布局 + pretext 文本分行 | 千级节点支持，视觉表现力跃升 |
| 3 | WebGL + 粒子/物理 | `wgpu` + `rapier2d` + GPU 粒子 | 游戏级视觉冲击力 |
| 4 | 场景化工作空间 | 全屏 Canvas + 空间布局 + DOM 覆盖层 | 从"表格"到"空间"的交互革命 |
| 5 | HTML-in-Canvas 整合 | 等待 WICG 标准化 + Chromium 默认开启 | 单一渲染管线，事件/可访问性统一 |

**阶段 1 试点**：新建 `/workspace` 工作台页面，Canvas 渲染 Agent 状态面板 + 实时数据流，验证事件桥/渲染循环/性能，完全独立不影响现有页面。

**风险护栏**：
- Dioxus 与 Canvas 事件桥接复杂 → 阶段 1 先在独立页面验证
- Canvas 文本排版退化 → 阶段 2 引入 pretext
- WASM 包体膨胀 → 按需加载，工作空间页面懒加载
- 可访问性丢失 → DOM 覆盖层保留 ARIA
- 为技术而技术 → 每阶段必须有明确业务价值才推进
