---
kind: think_trace_rounds_panel
name: ThinkTrace 轮级轨迹：JSONL 落盘 + query_think_traces 作用域守门 + 轮次时间线与轨迹面板
category: Agent 运行时
scope:
  - 'src/pkg/think_trace.rs'
  - 'src/pkg/daily_jsonl.rs'
  - 'src/service/domain/runtime/think_trace_query.rs'
  - 'src/service/domain/runtime/mod.rs'
  - 'src/handlers/hr/agent/query_think_traces.rs'
  - 'common/src/api/runtime.rs'
  - 'frontend/src/components/chat/agent_trace_panel.rs'
  - 'frontend/src/components/chat/think_traces_tab.rs'
  - 'frontend/src/components/chat/think_rounds_timeline.rs'
source_files:
  - src/pkg/think_trace.rs#L41-L266
  - src/service/domain/runtime/think_trace_query.rs#L48-L171
  - src/handlers/hr/agent/query_think_traces.rs#L27-L108
  - common/src/api/runtime.rs#L109-L246
  - frontend/src/components/chat/agent_trace_panel.rs#L34-L120
  - frontend/src/components/chat/think_rounds_timeline.rs#L11-L140
  - docs/wiki/zh/content/架构设计/分层架构设计/Domain 层编排/Runtime 领域编排.md
---

## 1. 整体方案 / 使用的框架与工具

思考轨迹把一次思考 run 的**轮级细节**（模型可见文本、推理摘要、工具调用与状态、每轮 token 与耗时、裁决结果）连同图执行信息（节点 occurrence 序列、选路记录 candidates/chosen/via）落成 append-only JSONL：`agents/{agent_id}/think_trace/YYYYMMDD.jsonl`，时间戳为**毫秒**。写入由 `pkg::think_trace::append` 完成，读取由 `query` / `query_at` 完成，两者都不经过关系库。

对外的唯一查询入口是 `GET /api/v1/hr/agents/think-traces`，它同时服务两种调用姿势：Agent 详情页带 `agent_id` 看单 Agent 历史；对话页「思考」Tab 带 `project_id` 聚合多 Agent。响应把 rounds / occurrences / routes **一次性内联返回**，前端展开条目时无需二次请求。

## 2. 关键文件与位置

| 文件 | 职责 |
|---|---|
| `src/pkg/think_trace.rs` | 轨迹基础设施：`init` 绑定基础数据目录、`append` 写入、`query` / `query_at` 读取；`MAX_THINK_TRACE_QUERY_LIMIT=100` 与默认 20；`ThinkTraceEntry` 及其 round / tool_call / occurrence / route 子结构 |
| `src/service/domain/runtime/think_trace_query.rs` | 作用域守门内核：`with_context_fields`（Agent/System 与上下文一致、User 普通成员必须显式作用域、limit 校验、冲突 400）+ `with_context_scope`；记忆轨迹复用同一内核 |
| `src/service/domain/runtime/mod.rs` | Runtime Domain 入口：先守门再直接调用 pkg 查询（pkg 属基础设施，允许 Domain 直达） |
| `src/handlers/hr/agent/query_think_traces.rs` | HTTP + 神经工具双暴露（`id=query_think_traces`，`tags=runtime,query`）；`to_list_item` 把内部结构逐字段映射为 DTO |
| `common/src/api/runtime.rs` | `QueryThinkTracesRequest` / `ThinkTraceListItem` / Round / ToolCall / Occurrence / Route DTO 单一事实源 |
| `frontend/src/components/chat/agent_trace_panel.rs` | Agent 详情「🧭 轨迹」主从面板：`PAGE_SIZE=20`、加载代际丢弃过期响应、按 `run_id` 回查记忆段 |
| `frontend/src/components/chat/think_rounds_timeline.rs` | 轮次时间线：文本/推理预览截断 + 工具状态与裁决标签 |
| [Runtime 领域编排](docs/wiki/zh/content/架构设计/分层架构设计/Domain 层编排/Runtime 领域编排.md) | ⭐ 本卡对应的人类百科长文（轨迹查询在 Runtime 领域的作用域编排） |

## 3. 架构与设计约定

### 3.1 落盘与读取对称

写入走 `DailyJsonlWriter`（按日分区、无 position 回填）；读取按 `agent_id` 选目录（缺省遍历全部 Agent）、按日期文件名区间裁剪、逐行解析后过滤。排序键是 `(created_at desc, run_id desc)`——同一毫秒内必须有第二键，否则多条目翻页会重复或漏项。查询未初始化时返回**空页**而不是报错、也不产生文件副作用。

### 3.2 作用域守门与会话一致

`with_context_fields` 的规则：Agent/System 调用若上下文带 agent/project/task 作用域，查询条件必须与之一致（显式冲突 → 400；上下文有、查询缺 → 用上下文补齐）；User 调用中 Admin/SuperAdmin 可全量，普通成员必须显式提供 `agent_id` / `project_id` / `task_id` 之一。口径与工具调用轨迹一致，避免「工具轨迹能查、思考轨迹能越权」的权限缝隙。

### 3.3 双入口共用一条链路

详情页与对话页不各写一套查询：都打到 `query_think_traces`，靠 `agent_id` / `project_id` 区分聚合范围。展开即用的设计让列表响应偏重（含全部 rounds），因此必须保留分页上限 100 与前端 20 条/页。

### 3.4 与记忆轨迹的关联

awaken / settle / compact 场景下 `run_id` 等于 `MemoryTrace.id`；轨迹面板据此调用记忆轨迹详情接口渲染上段记忆条目。`summary` / `intent-analyze` 没有对应 MemoryTrace，`has_memory_entry` 判false，不渲染记忆段。

## 4. 约定与约束 / 最佳实践 / 常见坑

- ❌ **时间单位不可混用**：`started_after` / `started_before` 是**毫秒**（对齐 `ThinkTraceEntry.created_at`）；与记忆轨迹的秒口径刻意区分，照抄会得到空页。
- ❌ **静态路由必须先于参数路由**：`/agents/think-traces` 等必须注册在 `/agents/{id}` 之前，否则会被当作 Agent ID 吞掉。
- ❌ **limit 上限 100**：pkg 层 `bail` 超限，Domain 层同样校验；前端分页不可突破。
- ✅ **DTO 逐字段映射，禁止透传内部结构**：`to_list_item` 负责把 `ThinkTraceEntry` 转成 common DTO，保证前后端契约只在 `common/src/api/runtime.rs` 定义。
- ✅ **前端切换 Agent / 翻页要丢弃过期响应**：用加载代际计数（`load_gen`）比对后再写状态；选中 run 若不在新页，回落到首条，避免详情区显示上一页数据。
- ✅ **字段缺失靠 `serde(default)` 兼容**：历史行可能没有 `project_id` / `task_id`（2026-10-22 之后才写入），反序列化不得失败。
- ✅ **pkg 层允许 Domain 直达**：`think_trace` 是基础设施（类似日志），没有 DAL/DAO 参与；不要为它硬造一层 DAO 转发。
- ✅ **诊断顺序**：查不到轨迹 → 先确认 `init` 是否已绑定基础数据目录（未初始化返回空页）、再看 `agents/{agent_id}/think_trace/` 是否存在当日文件、最后核对毫秒区间与 scene 过滤。
