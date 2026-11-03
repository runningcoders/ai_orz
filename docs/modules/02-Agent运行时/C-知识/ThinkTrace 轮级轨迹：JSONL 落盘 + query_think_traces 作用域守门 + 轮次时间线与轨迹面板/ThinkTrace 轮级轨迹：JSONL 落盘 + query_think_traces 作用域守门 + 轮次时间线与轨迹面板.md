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
  - src/pkg/think_trace.rs#L41-L378
  - src/service/domain/runtime/think_trace_query.rs#L48-L171
  - src/handlers/hr/agent/query_think_traces.rs#L27-L108
  - common/src/api/runtime.rs#L109-L246
  - frontend/src/components/chat/agent_trace_panel.rs#L34-L120
  - frontend/src/components/chat/think_rounds_timeline.rs#L11-L140
  - docs/modules/00-总览与架构/C-知识/Runtime 领域编排.md
---

## 1. 整体方案 / 使用的框架与工具

思考轨迹把一次思考 run 的**轮级细节**（模型可见文本、推理摘要、工具调用与状态、每轮 token 与耗时、裁决结果）连同图执行信息（节点 occurrence 序列、选路记录 candidates/chosen/via）落成 append-only JSONL：`agents/{agent_id}/think_trace/YYYYMMDD.jsonl`，时间戳为**毫秒**。写入由 `pkg::think_trace::append` 完成，读取由 `query` / `query_at` 完成，两者都不经过关系库。

落盘是**两段式**：每轮定型（think → dispatch → evaluate，`decision` 已写回）后由 `think_round::runner::append_partial_round` 追加一行 `partial=true`，只携带该轮 `rounds`；run 收尾的 `persist_trace` 再追加一行 `partial=false`，携带完整 `rounds` 与 occurrence / route / fingerprint。因此 **run 未结束时查询即可看到已完成轮次（半成品）**，异常中断也不会整条丢失。

对外 DTO 不暴露 `partial` 这个内部粒度，而是给一个 `finished: bool`（`to_list_item` 里取反）——前端据此在卡片上标「进行中」，不必再靠最后一条 `decision` 反推是否还在跑。`finished=false` 时 `rounds` 只到最新完成的一轮，`steps` / `occurrences` / `routes` 可能为空（这些只有收尾行才带）。

**成本与两条不可动的约束**：

- 一个 run 写 N+1 行（N 通常 < 20），但增量行只带单轮 `rounds`、收尾行才带全量，**查询解析的数据量约 2×**（不是行数那个 N+1 倍）。
- ⚠️ 增量写盘刻意**同步**，不要改成 `spawn_blocking` / 后台队列：JSONL 是 append-only，**行序即轮次序**，`merge_into` 的「增量行累积 → 收尾行替换」完全依赖它。并发写导致后一轮先落盘，合并出的轨迹轮次就错序了。
- ⚠️ 查询不做「读够一页就停」的早停：`total` 必须是合并后的真实总数，前端 `has_next = offset + len < total` 与「共 N 条」都依赖它；早停会让 total 变成「已扫描到的条数」，翻页当场失效。真要降成本，方向是调用方传 `started_after` 缩小时间窗口（由 `date_in_range` 在文件级粗筛掉整段历史分片）。

对外的唯一查询入口是 `GET /api/v1/hr/agents/think-traces`，它同时服务两种调用姿势：Agent 详情页带 `agent_id` 看单 Agent 历史；对话页「思考」Tab 带 `project_id` 聚合多 Agent。响应把 rounds / occurrences / routes **一次性内联返回**，前端展开条目时无需二次请求。

一次图 run 内思考循环可能被重新触发（新消息触发意图修订、`poll_exit` 回退执行），每次重跑都会追加一行 JSONL。查询层在分页前按 `(agent_id, run_id, scene)` 把这些行**合并成一条连续轨迹**（轮次 / 节点序号 / 图步号接续编号、步数与 advisor 次数求和、指纹重算），否则轨迹面板会把同一个动作显示成两个动作、各自统计轮次。

合并时增量行与收尾行的 `rounds` 是**同一批数据的两个粒度**：收尾行到达时先摘掉**本片段**逐轮增量累积的 rounds，再拼接收尾行的全量 rounds（`merge_into` 用 `partial_round_count` 精确记录本片段增量行数，该字段 `#[serde(skip)]` 不落盘），否则每轮会出现两次。⚠️ 只摘本片段、不能整体替换 `base.rounds`：一次唤醒内可能因意图修订多次重跑，每个片段各自「N 行增量 + 1 行收尾」，第二个片段的收尾行若整体替换会把第一个片段的轮次一并冲掉。片段之间仍走拼接 + 轮次偏移（意图重跑从 1 重新计数时整体后移）。

## 2. 关键文件与位置

| 文件 | 职责 |
|---|---|
| `src/pkg/think_trace.rs` | 轨迹基础设施：`init` 绑定基础数据目录、`append` 写入、`query` / `query_at` 读取；`MAX_THINK_TRACE_QUERY_LIMIT=100` 与默认 20；`ThinkTraceEntry` 及其 round / tool_call / occurrence / route 子结构 |
| `src/service/domain/runtime/think_trace_query.rs` | 作用域守门内核：`with_context_fields`（Agent/System 与上下文一致、User 普通成员必须显式作用域、limit 校验、冲突 400）+ `with_context_scope`；记忆轨迹复用同一内核 |
| `src/service/domain/runtime/mod.rs` | Runtime Domain 入口：先守门再直接调用 pkg 查询（pkg 属基础设施，允许 Domain 直达） |
| `src/handlers/hr/agent/query_think_traces.rs` | HTTP + 神经工具双暴露（`id=query_think_traces`，`tags=runtime,query`）；`to_list_item` 把内部结构逐字段映射为 DTO |
| `common/src/api/runtime.rs` | `QueryThinkTracesRequest` / `ThinkTraceListItem` / Round / ToolCall / Occurrence / Route DTO 单一事实源 |
| `frontend/src/components/chat/agent_trace_panel.rs` | Agent 详情「🧭 轨迹」主从面板：`PAGE_SIZE=20`、加载代际丢弃过期响应、按 `run_id` 回查记忆段；打开/手动刷新即读到进行中的半成品 |
| `frontend/src/components/chat/think_rounds_timeline.rs` | 轮次时间线：文本/推理预览截断 + 工具状态与裁决标签；`summarize_rounds` / `round_progress_label` / `RoundStatsLine` 供折叠态叠加轮次与 token / 耗时 / 强度；`TokenBadgeGrid` / `IntensityDurationColumn` 为折叠态与逐轮行共用的中右两栏组件 |
| [Runtime 领域编排](docs/modules/00-总览与架构/C-知识/Runtime 领域编排.md) | ⭐ 本卡对应的人类百科长文（轨迹查询在 Runtime 领域的作用域编排） |

## 3. 架构与设计约定

### 3.1 落盘与读取对称

写入走 `DailyJsonlWriter`（按日分区、无 position 回填）；读取按 `agent_id` 选目录（缺省遍历全部 Agent）、按日期文件名区间裁剪、逐行解析后过滤。读取后先按 `created_at` 正序把同一 `(agent_id, run_id, scene)` 的多行合并（同一 run 的重跑片段），再按 `(created_at desc, run_id desc)` 排序分页——合并必须发生在分页之前，否则 `total` 与页边界会按「行」而不是按「动作」计算。同一毫秒内必须有第二键，否则多条目翻页会重复或漏项。查询未初始化时返回**空页**而不是报错、也不产生文件副作用。

合并口径：`created_at` 取最早片段（run 起点，与 `MemoryTrace.id` 回查语义一致）；`steps` / `advisor_calls` 求和；`rounds` 若后一段从 1 重新计数则整体后移、若已带唤醒级累计 `start_round` 则原样保留；`occurrences` 的节点级 `seq` 跨片段续编号、`step` 与 `routes.step` 按前段 `steps` 后移；`fingerprint` 由合并后的 occurrence 序列重算。

### 3.2 作用域守门与会话一致

`with_context_fields` 的规则：Agent/System 调用若上下文带 agent/project/task 作用域，查询条件必须与之一致（显式冲突 → 400；上下文有、查询缺 → 用上下文补齐）；User 调用中 Admin/SuperAdmin 可全量，普通成员必须显式提供 `agent_id` / `project_id` / `task_id` 之一。口径与工具调用轨迹一致，避免「工具轨迹能查、思考轨迹能越权」的权限缝隙。

### 3.3 双入口共用一条链路

详情页与对话页不各写一套查询：都打到 `query_think_traces`，靠 `agent_id` / `project_id` 区分聚合范围。展开即用的设计让列表响应偏重（含全部 rounds），因此必须保留分页上限 100 与前端 20 条/页。

### 3.4 与记忆轨迹的关联

awaken / settle / compact / summary 场景下 `run_id` 等于 `MemoryTrace.id`；轨迹面板据此调用记忆轨迹详情接口渲染上段记忆条目。`intent-analyze` 没有对应 MemoryTrace，不渲染记忆段。判定统一走 common 的 `think_trace_has_memory_entry(scene)`，前后端不再各自维护场景白名单。

### 3.5 折叠态概览：轮次 / token / 耗时 / 强度由前端叠加

列表条目**折叠时**就要能回答「跑了几轮、花了多少 token、多久、什么强度」——答案全在已落盘的 `rounds` 里，前端叠加即可，**不为此新增后端字段、不二次请求**：`summarize_rounds` 求和四类 token 与 `duration_ms`、取最后一轮下发的 `thinking_intensity`（逐轮不同时置 `intensity_varies` 并把标签注明为「最后一轮」），`round_progress_label` 出轮次文案。

**折叠态与展开态同构**：`RoundStatsLine` 就是「一次 run 的汇总轮」，布局与 `ThinkRoundsTimeline` 每一轮的顶行完全一致 —— 左栏轮次进度 + 图执行信息（对应「第 N 轮 + 裁决」）、中栏 token 2×2、右栏强度 + 耗时，三栏各占两行高；展开前后不用重新认位置。条目因此是三段：头部徽章 / 统计三栏 / 元信息小字行（`created · 蓝图`）。折叠态显示**累计值**、时间线显示**单轮值**，只有数值口径不同。

为避免同一份数据两套视觉读法，中右两栏抽成共享组件（均在 `think_rounds_timeline.rs`）：`TokenBadgeGrid`（入 / 缓存 / 出 / 思考 四 badge，`ml-auto` 靠右）与 `IntensityDurationColumn`（强度 badge + 耗时，`scope` 决定悬停文案是「本轮」还是「累计」、`intensity_note` 补逐轮差异说明）。左栏刻意不抽：两处结构虽同为竖排，但内容是「第 N 轮 + 裁决」vs「已 N 轮 + 步数」，语义不同，抽了只会多一层无意义的参数透传。

⚠️ **不显示「第 N/M 轮」的分母**：运行时实际会跑几轮不可知（可能提前 `final` / `exit`，也可能因意图修订折返重跑），配置里的轮次预算是「上限」不是「实际」，拿它当分母等于把预算说成事实。故只给已记录轮数：进行中前缀「已」（数字还会涨），跨压缩导致本片段轮数 ≠ 累计轮次编号时两个都给（`已 2 轮（累计第 8 轮）`）。

同理，进行中的条目**不显示 `{steps} 步 · advisor {n}`**：图执行信息只有收尾行才写，增量行恒为 0，显示「0 步」会把「还没落盘」说成「一步没走」。

## 4. 约定与约束 / 最佳实践 / 常见坑

- ❌ **时间单位不可混用**：`started_after` / `started_before` 是**毫秒**（对齐 `ThinkTraceEntry.created_at`）；与记忆轨迹的秒口径刻意区分，照抄会得到空页。
- ❌ **静态路由必须先于参数路由**：`/agents/think-traces` 等必须注册在 `/agents/{id}` 之前，否则会被当作 Agent ID 吞掉。
- ❌ **limit 上限 100**：pkg 层 `bail` 超限，Domain 层同样校验；前端分页不可突破。
- ✅ **DTO 逐字段映射，禁止透传内部结构**：`to_list_item` 负责把 `ThinkTraceEntry` 转成 common DTO，保证前后端契约只在 `common/src/api/runtime.rs` 定义。
- ✅ **前端切换 Agent / 翻页要丢弃过期响应**：用加载代际计数（`load_gen`）比对后再写状态；选中 run 若不在新页，回落到首条，避免详情区显示上一页数据。
- ❌ **同一 run 的重跑不能按行展示**：意图修订 / 执行回退会让同一 `run_id` 追加多行；查询层必须按 `(agent_id, run_id, scene)` 合并后再分页，否则同一个动作会变成两个动作并各自统计轮次。
- ❌ **UI 展开 / 选中态不能用 `run_id` 单键**：Phase1 意图分析与 Phase2 唤醒**共享同一个 `run_id`**，列表 DOM key 与展开态必须用 `(agent_id, run_id, scene)`（common `ThinkTraceListItem::entry_key`），否则点一张卡片会同时展开 / 选中两张。
- ❌ **合并时编号不能重复偏移**：occurrence 的节点序号偏移要取合并前快照，片段内后续节点不能再吃一遍偏移；轮次只在「后段编号小于等于前段」时后移，已累计的 `start_round` 不能二次相加。
- ✅ **字段缺失靠 `serde(default)` 兼容**：历史行可能没有 `project_id` / `task_id`（2026-10-22 之后才写入），反序列化不得失败。
- ✅ **pkg 层允许 Domain 直达**：`think_trace` 是基础设施（类似日志），没有 DAL/DAO 参与；不要为它硬造一层 DAO 转发。
- ✅ **诊断顺序**：查不到轨迹 → 先确认 `init` 是否已绑定基础数据目录（未初始化返回空页）、再看 `agents/{agent_id}/think_trace/` 是否存在当日文件、最后核对毫秒区间与 scene 过滤。
