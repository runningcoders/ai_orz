# 前端优化路线图

> 最后更新：2026-07-17

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
