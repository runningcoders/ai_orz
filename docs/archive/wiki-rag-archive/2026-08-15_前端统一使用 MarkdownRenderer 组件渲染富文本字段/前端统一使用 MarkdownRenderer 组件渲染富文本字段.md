> 📦 归档标记（2026-08-15）：被 [统一使用 pulldown-cmark + MarkdownRenderer 组件渲染 Markdown 字段](docs/wiki/knowledge/zh/统一使用 pulldown-cmark + MarkdownRenderer 组件渲染 Markdown 字段/统一使用 pulldown-cmark + MarkdownRenderer 组件渲染 Markdown 字段.md) 取代。保留原因：历史参考，主卡已吸收本卡独有源码锚点与硬约束。生效方案：主卡真实路径作为唯一 RAG 召回目标。
---
kind: design
name: 前端统一使用 MarkdownRenderer 组件渲染富文本字段
source: session
category: adr
---

# 前端统一使用 MarkdownRenderer 组件渲染富文本字段

_来源：b156529 → eb09a60 提交周期内记录的编码计划——内容为规划时意图，实现可能滞后或有出入。_

**状态：** accepted

## 背景
Project/Task 的 execution_plan、execution_result，以及 Agent/Tool/Skill/Project/Task/Artifact 的 description、Agent soul、Skill.md、workflow/guidance、知识图谱 summary、聊天消息、记忆内容等大量 Markdown 性质字段此前以纯文本插值展示，且 docs.rs 中存在重复的 render_markdown() 实现。

## 决策驱动
- 消除重复渲染逻辑
- 复用 pulldown-cmark 0.13（WASM 友好）与现有 .markdown-body 样式
- 通过 use_memo 缓存 HTML 避免聊天多消息场景重复解析
- 仅详情/展开视图渲染 Markdown，列表态保持纯文本截断

## 备选方案
- **抽取为共享 MarkdownRenderer 组件** — 优点：单一来源、支持 compact 变体、use_memo 缓存、可被项目/任务/聊天/记忆等多处复用
- **在各页面内联 render_markdown()** _（已否决）_ — 优点：改动最小；缺点：重复代码、无法统一缓存策略、样式不一致

## 决策
新建 frontend/src/components/markdown.rs，提供 MarkdownRenderer 组件并注册到 components/mod.rs；将 docs.rs 中的 render_markdown() 抽取为公共函数，并在 project_detail、task_detail、agent_detail、skill_detail、tool_detail、model_provider_detail、artifact、knowledge_graph、chat、memory_search、agent_memory_panel 等处替换为 MarkdownRenderer。

## 影响
所有 Markdown 字段获得一致的表格/删除线/任务清单渲染与主题自适应；聊天消息因 use_memo 缓存提升性能；列表/表格仍走纯文本路径，避免不必要的 DOM 开销。