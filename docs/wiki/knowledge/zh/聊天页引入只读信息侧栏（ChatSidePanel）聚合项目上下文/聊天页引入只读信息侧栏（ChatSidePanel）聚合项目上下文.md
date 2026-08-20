---
kind: design
name: 聊天页引入只读信息侧栏（ChatSidePanel）聚合项目上下文
source: session
category: adr
scope:
    - 'frontend/src/**/ChatSidePanel*'
source_files:
    - docs/wiki/zh/content/前端应用/页面模块/消息与工作区页面/聊天侧面板/聊天侧面板.md
---

# 聊天页引入只读信息侧栏（ChatSidePanel）聚合项目上下文

_来源：eb09a60 → 46c56db 提交周期内记录的编码计划——内容为规划时意图，实现可能滞后或有出入。_

**状态：** accepted

## 背景
沟通页面缺失项目总览、进行中任务、执行计划/结果、产物等上下文；默认对话模式则完全没有侧栏信息。后端 API 已就绪（`with_progress_summary` / `with_task_graph` / `with_artifacts`）。

## 决策驱动
- 只读设计（不引入创建/编辑逻辑）
- 桌面端静态列 + 移动端抽屉复用左侧栏模式
- SSE 新消息触发防抖刷新（2s）
- localStorage 持久化面板开关状态

## 备选方案
- **右侧固定/抽屉式侧栏，按对话模式动态 Tab 组装** — 优点：复用 Agent 组件、懒加载任务详情、产物分组展示、N+1 查询规避；缺点：约 700 行新组件、需处理移动端布局差异
- **在主消息流内嵌上下文卡片** _（已否决）_ — 优点：实现简单；缺点：与消息流耦合、无法承载多层级信息（任务/产物/Agent）

## 决策
新增 `ChatSidePanel` 组件，项目对话模式提供「总览/任务/产物/Agent」Tab，默认对话模式提供「Agent/我」Tab；数据通过并行请求加载，任务详情懒加载并缓存，产物按 task_id 分组展示。

## 影响
聊天页获得完整的上下文感知能力；面板纯只读，编辑操作仍走各自详情页；移动端需处理抽屉遮罩与关闭交互；refresh_tick 机制避免 SSE 竞态导致的过期请求。