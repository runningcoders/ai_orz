---
kind: RAG 原子知识卡
name: 聊天侧栏项目内 Agent 列表 + 工具调用记录授权审批集成
category: 前端应用 / 组件系统
scope:
- frontend/src/components/chat/project_agents_tab.rs
- frontend/src/components/chat/tool_calls_tab.rs
- frontend/src/components/chat/chat_side_panel.rs
- frontend/src/pages/finance/tool_call_entries.rs
- frontend/src/api/finance.rs
- frontend/src/utils/status.rs
- frontend/styles/input.css
source_files:
- frontend/src/components/chat/project_agents_tab.rs#L1-L402（ProjectAgentsTab 独立 tab：项目内 Agent 列表 + 任务分配计数 + owner 优先排序 + 空态）
- frontend/src/components/chat/chat_side_panel.rs#L1-L1097（侧边栏 tab 注册 + 路由切换 + project_agents_tab/tool_calls_tab 子组件集成）
- frontend/src/components/chat/tool_calls_tab.rs（工具调用记录快速预览：status badge + call_id）
- frontend/src/pages/finance/tool_call_entries.rs#L1-L490（工具调用记录查询页 + 授权审批集成：待审批单顶部区块 + 调用记录行内快捷审批 + 详情 Modal 关联授权卡片 + 撤销二次确认）
- frontend/src/api/finance.rs（list_tool_authorizations / decide_tool_authorization / revoke_tool_authorization 三个 API）
- frontend/src/utils/status.rs（authorization_status_badge / authorization_revocable / short_id 授权状态辅助）
- docs/wiki/zh/content/前端应用/页面模块/消息与工作区页面/聊天侧面板/聊天侧面板.md
- docs/wiki/zh/content/前端应用/页面模块/Finance 管理页面/工具管理/工具管理.md
- docs/wiki/zh/content/功能模块/工具生态系统/工具生态系统.md
- docs/wiki/knowledge/zh/聊天页引入只读信息侧栏（ChatSidePanel）聚合项目上下文/聊天页引入只读信息侧栏（ChatSidePanel）聚合项目上下文.md
- docs/wiki/knowledge/zh/pkg/authorization 工具授权系统（拦截门 + 审批状态机 + 工具执行融合）/pkg/authorization 工具授权系统（拦截门 + 审批状态机 + 工具执行融合）.md
---

## §1 概述

**本卡角色**：前端聊天侧栏的「项目内 Agent 列表 + 工具调用记录授权审批集成」细知识卡。覆盖 ProjectAgentsTab（独立 tab，项目内 Agent 列表 + 任务分配计数 + owner 优先排序）、FinanceToolCallEntries 工具调用记录查询页扩展（顶部待审批授权单区块 + 调用记录行内快捷审批 + 详情 Modal 关联授权卡片 + 撤销二次确认）、chat_side_panel 侧边栏 tab 路由注册、api/finance.rs 三个新 API（list/decide/revoke 授权）、utils/status.rs 授权状态辅助函数。**定位：排查工具调用记录页审批按钮为什么不显示、侧边栏 ProjectAgents 为什么是空的、撤销授权后前端状态为什么不刷新时读。**

- **ProjectAgentsTab 侧栏新 tab**（2026-09-23 新增）：chat_side_panel.rs 注册第三个 tab——数据源是项目任务列表，按 `task.assignee_id` 聚合 agent_id → `assignee_task_counts` HashMap 得每个 Agent 分配的任务数 → `sort_project_agents` owner 优先 + name 升序。空态：无任务时显示"本项目尚无 Agent 参与"。
- **工具调用记录页授权审批集成**（2026-09-23 扩展，L490+）：原来纯查询 → 新增三重审批入口：① 顶部「待审批授权单」区块（list_tool_authorizations 拉全部 Pending 单）；② 调用记录表「审批」列（命中 Pending → 行内通过/拒绝按钮）；③ 详情 Modal（关联授权卡片，Pending 可批、Active 可撤销）。撤销走 ConfirmDialog 二次确认（影响面已生效）。
- **精确关联 call_id**：后端拦截建单时把 `call_id` 注入授权单（common/src/api/tool.rs 的 AuthorizationDetailDto.call_id），前端据此把授权单精确挂到对应调用记录——不靠 (agent, tool, 时间窗) 反推。

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| chat_side_panel.rs | 侧边栏 tab 路由 + 子组件集成 | 新增 PROJECT_AGENTS tab 注册 + tool_calls_tab 扩展；tab 切换 use_state 驱动 | 见 1097 行 |
| project_agents_tab.rs | 项目内 Agent 列表 tab | agent_ids_from_tasks → 去重 + assignee_task_counts HashMap → sort_project_agents（owner 优先 + name 升序）+ AgentListItem 列表渲染 | `:L41-L80` |
| tool_calls_tab.rs | 工具调用记录快速预览 | 侧边栏内嵌 Tab：status badge + call_id + 跳转完整页 | 见文件 |
| tool_call_entries.rs | 工具调用记录 + 授权审批三重入口 | 顶部待审批区块 + 调用记录审批列 + 详情 Modal 关联授权卡片；撤销 ConfirmDialog | 见 490+ 行 |
| api/finance.rs | 三个授权 API | list_tool_authorizations / decide_tool_authorization / revoke_tool_authorization | 见文件 |
| utils/status.rs | 授权状态辅助 | authorization_status_badge / authorization_status_text / authorization_revocable / short_id | 见文件 |

**章节来源**
- [chat_side_panel.rs](frontend/src/components/chat/chat_side_panel.rs)
- [project_agents_tab.rs](frontend/src/components/chat/project_agents_tab.rs)
- [tool_call_entries.rs](frontend/src/pages/finance/tool_call_entries.rs)

## §3 架构约定

本卡按 **Level 4（总卡-细卡）** 保留——总卡「聊天页引入只读信息侧栏（ChatSidePanel）」（scope 覆盖所有 ChatSidePanel 相关文件）定位侧栏整体，本卡定位其中「工具调用记录授权审批集成」这个具体细视角。双向声明：总卡 §2 末行列本卡路径，本卡 §3 首句声明总卡。

**数据流**：

```
[Agent 被拦截执行危险工具]
  → 后端拦截侧建单（带 call_id）
  → SSE 推送 ToolExecEvent
  → [聊天页] chat_side_panel tool_calls_tab 刷新
  → [Finance 工具调用记录页]
     → load_entries（调用记录）+ load_auths（授权单）双闭包并行
     → 以 call_id 为 key 交集合并
     → 顶部待审批区块（auths 过滤 Pending）
     → 调用记录审批列（auth.status==Pending 显示按钮）
     → 详情 Modal（auth 关联卡片，Pending 可批 / Active 可撤销）
  → 用户点决策 → decide_tool_authorization / revoke_tool_authorization
  → 成功后 auths.set([]) + 重新加载（单轮询刷新）
```

## §4 硬约束与回归红线（5 条）

1. **ProjectAgentsTab owner 必须优先排序**：sort_project_agents 必须把 owner_id 匹配的 Agent 放到第一行（owner 优先 + 其余按 name 升序）；禁止 owner 和其他 Agent 混排。单元测试 `sort_puts_owner_first_then_name_ascending` 覆盖。
2. **tool_call_entries 的 auths 与 entries 必须以 call_id 精确关联**：禁止用 (agent_id, tool_id, 时间窗口) 反推——call_id 是唯一桥，后端 DTO 层删 call_id 字段 = fail。
3. **撤销授权必须走 ConfirmDialog 二次确认**：影响面已生效（Active 授权已签发、工具已放行），必须弹确认框展示授权单 ID + 放行工具 + 撤销影响，禁止一键撤销。
4. **deciding 状态必须全局禁用所有审批按钮**：decide_tool_authorization 请求进行中（deciding=Some(id)）→ 所有 Pending 单的审批按钮禁用，防重复决策（同一 call_id 可能被同时点两次）。
5. **ProjectAgentsTab 空态不得返回空列表**：项目有任务但所有任务 assignee_type=User 时（没有 Agent 参与），ProjectAgentsTab 必须渲染空态"本项目尚无 Agent 参与"而非空白。单元测试 `agent_ids_empty_when_no_tasks` 覆盖。

## §5 历史演进（变更摘要）

- **侧边栏总卡扩展**（commit 2026-09-23）：chat_side_panel 新增 PROJECT_AGENTS tab 注册。
- **ProjectAgentsTab 新建**（commit f8b5114b）：402 行独立组件——任务分配计数 + owner 优先排序 + 空态 + 6 个单元测试。
- **工具调用记录页授权审批集成**（commit 0894a958 + 9bfd24f9）：前端 tool_call_entries 扩展三重审批入口 + 撤销 ConfirmDialog；后端 Finance 审批面路由挂载 + trace 关联 + 出口脱敏。
- **auths/entries 双闭包并行加载**：load_entries + load_auths 只捕获 Copy 的 Signal / ToastState → 闭包自身 Copy，可被多个事件处理器复用。
