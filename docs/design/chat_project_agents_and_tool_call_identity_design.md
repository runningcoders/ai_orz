# 聊天侧栏「项目内 Agent 列表」与「工具调用身份展示」设计说明

> **定位**：前端体验优化专项 T2 交付物，为 T3 前端实现与 T4b UI 验收提供唯一视觉与交互基准。
> **基线**：`ui_design_system.md` v3.1（HUD 设计系统）、`hud_design_prototype.html`（视觉基准）、T1 调研方案（任务 01a0cd34-8c36 execution_result，组件与数据口径已确认）。
> **约束**：零新增组件族——全部复用既有组件与工具类；仅新增一个列表容器组件 `ProjectAgentsTab`（前端结构，非新视觉体系）。

## 1. 范围

| 需求 | 改造对象 | 章节 |
|------|----------|------|
| ① 项目内 Agent 列表 | `chat_side_panel.rs` 项目模式 Agent Tab（tab=3） | §2 |
| ② 工具调用记录 Agent 身份展示 | `tool_calls_tab.rs` 行头 | §3 |

**明确不做**：默认对话模式 Agent Tab（保持现状）；`AgentInfoTab` 内部结构；后端接口；`AvatarBubble` / `IdentityChip` / `agent_summary` 组件本体。

## 2. 需求① 项目模式 Agent Tab → 项目内 Agent 列表

### 2.1 视图状态机

```
AgentTab(tab=3, 项目模式)
 ├─ Loading（项目详情未就绪 → 沿用现有 loading_placeholder）
 ├─ Ready(list) ──点击列表项──▶ Detail(agent_id)
 ▲                                │
 └──────点击「← 返回列表」─────────┘
```

- `list` 与 `detail` 是同一 Tab 内的两个视图态（面板内切换，不走路由）；`Detail` 原样复用现有 `AgentInfoTab { agent_id }`，不修改其内部。
- 默认进入 `list`；返回列表时保留上一次选中的 agent_id 用于高亮（§2.6）。

### 2.2 列表项布局（对齐 @提及列表 `.mention-menu-item`）

```
┌────────────────────────────────────────────────────────┐
│ ◉24px    名称（text-sm font-medium, truncate）   [徽标区] │
│ avatar   副标题（text-xs muted, truncate）                │
└────────────────────────────────────────────────────────┘
```

| 元素 | 规格 | 依据 |
|------|------|------|
| 行容器 | `flex items-center gap-2`、`padding: 0.375rem 0.5rem`、`border-radius: 0.375rem`、`cursor-pointer`、单行 nowrap | `.mention-menu-item`（input.css L458） |
| 头像 | 24px 圆形，Agent tone（`bg-secondary` + 首字母），复用 `AvatarSize::Sm` | avatar_bubble.rs |
| 名称 | `text-sm font-medium truncate min-w-0`；名称走全局目录解析，未命中回退短 ID（前 6 后 4） | IdentityChip 同口径 |
| 副标题 | `text-xs text-base-content/60 truncate`：优先 Agent 简介单行截断；简介为空回退「参与 N 个任务」（N=项目任务中该 Agent 作为 assignee 的数量，由面板已拉取的 tasks_list 统计，零额外请求） | T1 方案 |
| 徽标区 | 行尾 `margin-left:auto flex-shrink:0 flex-none nowrap`，最多 3 枚，超出截断（不换行） | `.mention-menu-flag` 语义（input.css L473） |

### 2.3 项目内身份徽标体系

| 徽标 | 视觉 | 语义依据 |
|------|------|---------|
| 「负责人」 | `badge hud-badge badge-primary badge-xs`（品牌橙语义色） | 项目负责人是关键身份，走状态徽章语义色突出；xs 尺寸防喧宾夺主 |
| 角色标签（Agent.roles） | `.mention-menu-flag` 中性胶囊（0.6875rem、base-300 边、opacity .55）或 `tag_chip()`（`badge orz-tag badge-xs`）——二选一由前端按实现成本定，两者均为体系内中性属性语言 | ui_design_system §4.4：属性走中性、状态走语义色 |

**规则**：徽标 = 负责人徽标（若 owner）+ 最多 2 个角色标签；roles 为空且非负责人时不显示徽标区。

### 2.4 排序与数据口径（沿用 T1，仅固化展示规则）

- 数据口径：项目任务 assignee（assignee_type==1）去重 → `query_agents(ids)` 批量取详情；**用户 assignee 不在本列表展示**（本需求聚焦 Agent；后续如需展示用户另行设计）。
- 排序：负责人置顶第一；其余按 Agent 名称字典序。

### 2.5 状态枚举

| 状态 | 视觉 |
|------|------|
| Loading | 沿用面板 `loading_placeholder()` |
| Empty | 居中 `py-12`：主文案「项目暂无参与的 Agent」`text-sm text-base-content/60`；副文案「给任务分配 Agent 后，会在这里显示」`text-xs text-base-content/40 mt-1`（样式对齐 tool_calls_tab 空态） |
| Ready(list) | §2.2 列表 |
| Detail(agent_id) | 返回行 + AgentInfoTab |

### 2.6 交互规格

| 交互 | 规格 |
|------|------|
| hover | 行背景 `color-mix(in oklab, var(--color-base-content) 6%, transparent)`（体系内 line-soft hover 语言） |
| 选中 | 背景 `color-mix(in oklab, var(--color-primary) 10%, transparent)` + `inset 2px 0 0 0 var(--color-primary)` 左数据条（对齐 `.hud-row` inset 语言） |
| 点击行 | 进入 Detail；详情视图滚动位置重置到面板顶部 |
| 返回 | Detail 顶部第一行渲染 `← 返回列表`（`btn hud-btn btn-ghost btn-xs`，置于 AgentInfoTab 之上的唯一新增元素）；点击回 list 并保留选中高亮 |
| 键盘 | 列表行 `role="button"` + `tabindex=0`，Enter/Space 激活；focus-visible outline 对齐 `.mention-menu-tab:focus-visible`（2px primary） |
| 保留入口 | AgentInfoTab 内「在详情页打开 →」Link 原样保留（跳 HrAgentDetail 路由） |

### 2.7 线框示意

```
Agent Tab（项目模式）                    Detail 视图
┌───────────────────────────┐          ┌───────────────────────────┐
│ [总览][任务][产物][Agent][工具]│          │ ← 返回列表                 │
│───────────────────────────│          │ ─────────────────────────  │
│ ◉UI   [负责人][UI设计师]    │          │ (AgentInfoTab 原样渲染)     │
│      参与任务 · HUD 设计    │          │  身份行 / 徽章行            │
│ ◉FE   [前端开发]           │          │  简介 / 能力               │
│      参与任务 · 前端实现    │          │  在详情页打开 →             │
│ ◉PMO  [负责人][PM]         │          └───────────────────────────┘
│      参与任务 · 项目管理    │
└───────────────────────────┘
```

## 3. 需求② 工具调用记录 Agent 身份展示

### 3.1 行头布局（位置定稿）

**定稿：IdentityChip 放在状态徽章之后、工具名之前。**

```
现状：  [状态徽章] [工具名·············flex-1] [⚙PID] [▼]
改后：  [状态徽章] [◉24px 名称] [工具名···flex-1] [⚙PID] [▼]
                   └ IdentityChip (flex-none)
```

理由：行头阅读顺序 =「谁 → 做了什么」；「负责人」样式的语义本就是操作主体，靠近行首符合认知；PID/展开指示器是附属性信息保持行尾。T1 给的两个候选位（状态徽章后/工具名前）实际是同一间隙的两端，取「紧贴状态徽章之后」即定稿，无歧义。

### 3.2 chip 规范

- 直接复用 `IdentityChip { id: e.agent_id, tone: Agent, align: Start }`，零改造；`agent_id` 为 None（系统级调用等）**不渲染**，行头退化为现状。
- chip 容器 `flex-none`；名称 `text-xs truncate`（组件自带）。窄侧栏下优先压缩工具名（`flex-1 truncate` 已有），chip 不换行。

### 3.3 点击小卡片

- `IdentityChip → AvatarBubble` 零成本复用：点击头像弹 `orz-popover w-64 p-3` 卡片（内容 = agent_summary 同源：身份行 + 徽章行 + 简介 line-clamp-3 + 能力 ≤6 + 「在详情页打开 →」）。
- 浮层水平方向定稿：`align: Start`（卡片向右展开）。工具行 chip 位于行首区域，右侧空间充足；垂直方向由 AvatarBubble 既有「向上优先 / 不够翻向下 / 再不够限高」逻辑处理（含侧栏 overflow 容器 scroll_ancestor_rect 兜底），无需新设计。
- 卡片内容不做任何新增/定制。

### 3.4 交互冲突与实现建议（给 T3）

| 风险 | 建议 |
|------|------|
| 点击 chip 会沿 DOM 冒泡触发行头「展开/收起」onclick | chip 触发层点击 `stop_propagation()`（先例：tool_calls_tab.rs PID 徽标） |
| 浮层在侧栏 overflow 容器内被裁 | AvatarBubble 已用 `position:fixed` + 滚动祖先边界兜底，T3 自测确认即可，无需改造 |

## 4. 设计体系对齐清单

| 使用项 | 出处 | 用途 |
|--------|------|------|
| `.mention-menu-item` 尺寸语言（gap/padding/radius/字号） | input.css L458 | 列表行 |
| `.mention-menu-flag` | input.css L473 | 角色/范围中性徽标 |
| `AvatarSize::Sm/Md`、`AvatarTone` | avatar_bubble.rs | 头像尺寸与配色 |
| `IdentityChip` | identity_chip.rs | 工具行身份展示 |
| `AgentInfoTab` | chat_side_panel.rs L799 | 详情子页（原样复用） |
| `agent_summary`（身份行/徽章行） | agent_summary.rs | 小卡片内容（复用不新增） |
| `badge hud-badge badge-primary badge-xs` | input.css | 负责人徽标 |
| `btn hud-btn btn-ghost btn-xs` | ui_design_system §4.3 | 返回按钮 |
| 空态文案样式 | tool_calls_tab.rs | 空态 |
| primary 10% 选中 + inset 2px 左条 | `.mention-menu-item.is-active` / `.hud-row` | 选中态 |

红线自查：未新增颜色/字号/圆角档位；未散写 DaisyUI 裸徽章类；状态/属性徽章语义符合 ui_design_system §4.4。

## 5. 验收清单（T4b 对照用）

**必修**
1. 项目模式 Agent Tab 显示项目内 Agent 列表（含负责人），不再只显示 PMO；负责人行第一且带「负责人」徽标。
2. 列表项含：24px 头像、名称、副标题（简介或「参与 N 个任务」）、项目内身份徽标。
3. 点击列表项进入单 Agent 详情子页（AgentInfoTab 内容完整），顶部有「← 返回列表」。
4. 返回后列表保留上一选中项高亮（primary 10% + 左条）。
5. 空态/加载态按 §2.5 呈现；默认对话模式 Agent Tab 行为不变。
6. 工具调用行头按 §3.1 顺序渲染 IdentityChip；agent_id 为空的记录不渲染 chip、行头不变。
7. 点击 chip 头像弹出 Agent 小卡片（内容 = agent_summary 同源），卡片不越出侧栏可视区；点击弹卡不触发行展开。

**建议**
8. 列表行键盘可达（Tab 聚焦、Enter/Space 进入）。
9. 徽标区超过 3 枚时优雅截断不换行。
10. 进入 Detail 后面板滚动位置重置顶部。

## 6. 待确认

- 无阻塞项。一处留观：若列表 Agent 数量较多（>10），是否需要折叠/搜索——本期不做（T1 口径：项目任务数有限，直接全量展示）。

---
*更新记录：2026-09-23 v1.0 初版（T2 UI 设计师，基于 T1 定稿方案与 ui_design_system v3.1）。*
