---
kind: RAG 原子知识卡
name: 工作台拓扑图 WorkspaceGraph：四视图节点渐进增强信息卡
category: 前端应用 / HUD 可视化 / 工作区拓扑
scope:
  - "frontend/src/components/workspace_graph.rs"
  - "frontend/src/pages/workspace.rs"
  - "frontend/src/pages/project/project_detail.rs"
  - "frontend/src/pages/project/task_detail.rs"
  - "frontend/src/pages/hr/agent_detail.rs"
source_files:
  - '【父卡】docs/wiki/knowledge/zh/Canvas HUD 可视化：GraphCanvas 知识图谱 + 图表场景LineDonut + 仪表盘Gauge双版 + HudPalette橙光光晕/Canvas HUD 可视化：GraphCanvas 知识图谱 + 图表场景LineDonut + 仪表盘Gauge双版 + HudPalette橙光光晕.md（CanvasScene 统一渲染管线 + CanvasNode::is_card 渐进增强判定 + 力导向碰撞避让（父卡 §4-22/23）的上位卡）'
  - 'frontend/src/components/workspace_graph.rs#L22-L48（workspace_node 统一构造：description trim 后原样下发 + tags 直传；is_card() 为真时按 node_card 几何算外接圆半径 max(w,h)/2 回填 radius —— 渐进增强与渲染器判定共用一条路径）'
  - 'frontend/src/components/workspace_graph.rs#L126-L132（task_tags：业务 tags 基础上 progress > 0 追加「进度 N%」胶囊，0% 不占位）'
  - 'frontend/src/components/workspace_graph.rs#L137-L196（build_global_view：Project(28px)/Agent(25px) 节点 + assignee_type==1 推断 Project↔Agent 边 + HashSet 去重）'
  - 'frontend/src/components/workspace_graph.rs#L202-L339（build_project_detail_view：LayeredLayout Task DAG（layer_height=80，layer+1 让位中心 Project）+ Project→Task 边 + 依赖边）'
  - 'frontend/src/components/workspace_graph.rs#L344-L420（build_agent_detail_view：中心 Agent(35px) + 该 Agent 的 Task + 关联 Project 去重）'
  - 'frontend/src/components/workspace_graph.rs#L429-L555（build_task_detail_view：中心 Task + 关联 Project/Agent + 前置/后继 Task）'
  - 'frontend/src/components/workspace_graph.rs#L558-L682（WorkspaceGraph 组件：click_map 节点 id → 真实 ID/类型，中心节点切视图、非中心跳详情路由；auto_size 适配父容器尺寸）'
  - 'frontend/src/components/workspace_graph.rs#L684-L756（3 条单测：有正文→卡片+外接圆半径；无正文→保持圆形；task_tags 进度胶囊守卫）'
  - 'frontend/src/components/canvas_scene.rs（CanvasNode::is_card 渐进增强判定 + DefaultRenderer 双形态绘制：圆形 / 矩形信息卡，详见父卡 §4-22）'
  - 'frontend/src/components/node_card.rs（卡片几何 SSOT：NODE_BOX_W=168 + box_width/box_height/wrap_text/hover_lines —— 构造点只引用不自算，详见父卡 §4-20）'
  - 'frontend/src/components/layered_layout.rs（Task DAG 分层布局：compute_layered_layout + y 自适应）'
  - 'docs/wiki/zh/content/前端应用/组件系统/业务组件.md（§工作区画布 WorkspaceGraph：四视图职责与交互长文）'
---

## §1 概述

**本卡角色**：Canvas HUD 父卡的子卡，专管工作台拓扑图 WorkspaceGraph 单文件——四视图（Global/ProjectDetail/AgentDetail/TaskDetail）的节点/边构建与点击导航。**定位：改 WorkspaceGraph 节点信息量、给四视图补展示字段、排查卡片互压或节点缩成一坨时读；CanvasScene 渲染管线与力导向算法本身去看父卡。**

核心机制一句话：四视图所有节点统一经 `workspace_node` 构造——DTO 的 description/tags（Agent 为 roles、Task 经 task_tags 追加进度胶囊）原样下发，`CanvasNode::is_card()` 判有正文/标签就升级成矩形信息卡（radius 回填外接圆半径供力导向碰撞避让），没有就保持原圆形；渲染器零分支新增，2026-09-18（commit 7f9b9075）起 WorkspaceGraph 与知识图谱共享同一套渐进增强。

## §2 关键文件与职责表

| 函数/结构 | 角色 | 关键点 | 源码锚点 |
|------|------|---------|---------|
| workspace_node | 四视图节点统一构造 | 有正文/标签 → 信息卡（radius=外接圆半径）；否则圆形（radius=原 circle_radius） | `:L22-L48` |
| task_tags | Task 标签组装 | 业务 tags + progress>0 追加「进度 N%」 | `:L126-L132` |
| build_global_view | Global 视图 | Project↔Agent 边由 Task 推断（assignee_type==1）+ 去重 | `:L137-L196` |
| build_project_detail_view | ProjectDetail 视图 | Task DAG 走 LayeredLayout（layer+1 让位中心 Project） | `:L202-L339` |
| build_agent_detail_view | AgentDetail 视图 | 中心 Agent + 其 Task + 关联 Project | `:L344-L420` |
| build_task_detail_view | TaskDetail 视图 | 中心 Task + Project/Agent + 前置/后继 Task | `:L429-L555` |
| WorkspaceGraph | 组件入口 | click_map 路由跳转；auto_size 覆盖 width/height | `:L558-L682` |

**章节来源**
- [workspace_graph.rs:22-682](frontend/src/components/workspace_graph.rs#L22-L682)

---

## §3 架构约定（数据流与四视图矩阵）

本卡是【Canvas HUD 可视化】父卡的子卡：父卡管 CanvasScene 渲染管线、is_card 判定、力导向碰撞避让的通用机制，本卡管 WorkspaceGraph 适配层怎么喂数据——§4 只列工作台特有红线，通用红线引用父卡 §4。

数据流：`pages/*（4 个消费方）→ WorkspaceGraphProps{view, projects/agents/tasks} → build_*_view 过滤 + 构节点边 → workspace_node 渐进增强 → CanvasScene(DefaultRenderer) 绘制`。

| 视图 | 中心节点 | 卫星节点 | 边 | 布局 |
|---|---|---|---|---|
| Global | — | Project(28) / Agent(25) | Task(assignee_type==1) 推断 Project↔Agent，HashSet 去重 | 力导向 |
| ProjectDetail | Project(28) | Task(20) / Agent | Project→Task、Task 依赖边 | Task DAG 分层 |
| AgentDetail | Agent(35) | Task(20) / Project(28) | Agent→Task、Task→Project | 力导向 |
| TaskDetail | Task(20) | Project / Agent / 前置后继 Task | 中心→相关节点 | 力导向 |

各视图节点下发字段：Project=description+tags；Agent=description+roles；Task=description+task_tags（含进度胶囊）。空字段不硬撑——description trim 后为空且 tags 为空 → 保持圆形（circle_radius 原样）。

---

## §4 硬约束与回归红线（7 条）

1. **四视图节点一律走 workspace_node 构造，禁止回到手写 CanvasNode 字面量**（2026-09-18 收敛 12 处）：字面量会漏掉外接圆半径回填（父卡 §4-23）→ 卡片互压；漏掉 node_type → click_map 路由失配。新增节点类型/视图同样只准调 workspace_node。
2. **渐进增强判定不许在适配层复制**：有没有正文/标签只认 `CanvasNode::is_card()`，workspace_graph 不另写 `if !description.is_empty()` 分支——两套判定必然漂移（构造点判成卡片、渲染器画圆形 = 半径语义错乱）。
3. **卡片 radius 必须是外接圆半径 `max(w,h)/2`**：workspace_node 已在构造点统一修（父卡 §4-23 在本文件的落点）；`box_width/box_height` 只准从 node_card.rs 引用（父卡 §4-20 的 SSOT 红线）。守卫单测 `workspace_node_with_body_becomes_card_with_circumscribed_radius` 断言半径 ≥ 半宽。
4. **description 原样下发，禁止构造点截断/改写**：trim 只去首尾空白；折行与 hover 读数是 node_card（wrap_text/hover_lines）的职责——构造点截断会制造「卡面省略号 vs hover 全文」之外的第三种不一致。
5. **Task 展示维度统一进 task_tags**：进度胶囊（progress>0 才追加，0% 无信息量不占位）之后任何新增维度（截止时间/优先级徽标）都进 task_tags，禁止散落回各视图手写 push。
6. **ProjectDetail 的 Task DAG 位置只认 LayeredLayout**：workspace_node 返回后仅允许改 x/y/layer 三个字段；layer_height=80 仍容纳最高信息卡（~71px），若卡片高度上限被突破需同步评估 layer_height 与 node_card 几何。
7. **WorkspaceGraphProps 的 PartialEq 排除 EventHandler，新增字段必须同步补进 eq**（`:L85-L95`）：手写 eq 不含 on_view_change（无法比较）；漏补新字段 = 该字段变化不触发重绘，症状是「改了数据图不刷新」，无编译错误、纯静默。
