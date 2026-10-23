---
kind: memory_trace_query
name: 记忆轨迹查询链路：MemoryTrace JSONL 分页扫描 + scope 守门 + 单条详情展开
category: 记忆系统
scope:
  - 'src/service/dao/memory/**'
  - 'src/service/dal/memory/**'
  - 'src/service/domain/runtime/memory_trace_query.rs'
  - 'src/service/domain/runtime/mod.rs'
  - 'src/handlers/hr/agent/query_memory_traces.rs'
  - 'common/src/api/runtime.rs'
  - 'frontend/src/components/chat/agent_trace_panel.rs'
  - 'frontend/src/components/chat/memory_trace_detail.rs'
source_files:
  - src/service/dao/memory/sqlite/trace.rs#L17-L53
  - src/service/dal/memory/trace.rs#L11-L26
  - src/service/domain/runtime/memory_trace_query.rs#L15-L33
  - src/handlers/hr/agent/query_memory_traces.rs#L31-L108
  - common/src/api/runtime.rs#L255-L356
  - frontend/src/components/chat/memory_trace_detail.rs#L56-L161
  - docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/短期记忆 (Short-term Memory)/原始记忆追踪.md
---

## 1. 整体方案 / 使用的框架与工具

原记忆轨迹（MemoryTrace）原本「只写不读」：每次思考闭环的消息 input/output 追加到 `agents/{agent_id}/memory/YYYYMMDD.jsonl` 后再无查询入口。本卡覆盖补齐后的**读侧链路**：DAO 顺序扫描每日 JSONL 分页返回 → DAL 仅做透传门面 → Domain 统一做作用域守门 → Handler 暴露「列表（预览）」与「单条详情（全文）」两个端点 → 前端轨迹面板按 think trace 的 `run_id` 回查详情。

不使用数据库：轨迹是 append-only 文件，查询靠目录枚举 + 行级过滤 + 全量排序后内存分页。这条链路与思考轨迹（think trace）同族但刻意保留**不同时间口径**——记忆轨迹的 `created_at` 是**秒**，思考轨迹是**毫秒**；混用会静默返回空页。

## 2. 关键文件与位置

| 文件 | 职责 |
|---|---|
| `src/service/dao/memory/sqlite/trace.rs` | `query_traces_at` 包内可测内核：limit 默认 20 / 上限 100、目录与日期文件枚举、逐行过滤、`created_at desc, id desc` 稳定排序、offset/limit 内存分页；外层 `query_traces` / `get_trace` 读全局 `base_data_path` |
| `src/service/dal/memory/trace.rs` | DAL 查询门面：`query_traces` / `get_trace` 直接转发 DAO，不加工业务条件 |
| `src/service/domain/runtime/memory_trace_query.rs` | `with_context_scope`：复用 think trace 的校验内核，`allow_project=false`（MemoryTrace 无 project 维度）、limit 上限 100 |
| `src/handlers/hr/agent/query_memory_traces.rs` | 列表 + 详情两个入口；`PREVIEW_MAX_CHARS=200` 按字符边界截断并追加 `…`；详情缺失返回 not_found |
| `common/src/api/runtime.rs` | `QueryMemoryTracesRequest` / `MemoryTraceListItem` / `GetMemoryTraceRequest` / `MemoryTraceDetail` 单一事实源 |
| `frontend/src/components/chat/memory_trace_detail.rs` | 详情组件：`PREVIEW_CHARS=400` 可展开；props 变化需 `key` 重挂载 |
| [原始记忆追踪](docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/短期记忆%20%28Short-term%20Memory%29/原始记忆追踪.md) | ⭐ 本卡对应的人类百科长文（写入侧 + 查询链路 + 故障排查） |

## 3. 架构与设计约定

### 3.1 分层职责

Adapter（Handler）只做 DTO 转换与预览截断；Domain 负责作用域校验；DAL 是纯透传；DAO 持有全部文件系统细节。任何一层都不允许绕过 Domain 直接查文件。

### 3.2 扫描与分页语义

`query_traces_at` 先按 `agent_id` 选目录（缺省时遍历 `agents/` 下全部 Agent 目录），再按日期文件名区间裁剪待读文件，然后逐行解析并 `matches_trace_query` 过滤；**先收集全量命中再排序**，`total` 取分页前的命中数，最后 `skip(offset).take(limit)`。排序键必须是 `(created_at desc, id desc)` 双键，单键在同秒多条时会因文件行序不同导致翻页重复/丢项。

### 3.3 作用域守门

`with_context_scope` 把显式查询条件与 `RequestContext` 合并：Agent/System 调用必须与上下文作用域一致（显式值冲突 → 400；上下文有值而查询缺失 → 用上下文补齐）；User 调用中 Admin/SuperAdmin 可全量，普通成员必须显式给出 `agent_id` / `task_id` 之一。`allow_project=false` 是有意为之：若允许 project 参与「是否具备作用域」判定，只带 project 的请求会退化成跨 Agent 全量扫描。

### 3.4 预览与详情二分

列表只返回 `input_preview` / `output_preview`（各截断 200 字符），完整正文只能走详情接口。详情用 `agent_id + trace_id` 精确定位——`trace_id` 本身不含 Agent 信息，缺 `agent_id` 无法确定扫描目录。

## 4. 约定与约束 / 最佳实践 / 常见坑

- ❌ **时间单位不可混用**：`created_after` / `created_before` 是**秒**（对齐 `MemoryTrace.created_at`）；照抄 think trace 的毫秒口径会让区间过滤恒为真或恒为假，表现为「查不到数据」而不是报错。
- ❌ **limit 上限 100**：DAO 与 Domain 双重校验，超过直接 400；前端分页 `PAGE_SIZE` 应远小于该阈值。
- ❌ **普通成员禁止无作用域查询**：不传 `agent_id` 会被 Domain 拒绝（fail-closed），不要靠 DAO 的全量遍历兜底。
- ✅ **详情必须同时给 `agent_id` 与 `trace_id`**：`GET /api/v1/hr/agents/memory-traces/{trace_id}?agent_id=`；缺 agent 无法定位 JSONL 目录。
- ✅ **DAO 内部分页要复用 `query_traces_at`**：`get_trace` 就是 `agent_id + trace_id + limit=1` 的组合查询，不要另写一套单条读取逻辑。
- ✅ **坏行跳过是设计而非 bug**：JSONL 逐行 `serde_json` 失败即跳过该行，因此「某条轨迹查不到」优先核对文件日期分区与写入侧是否真的落盘，而不是怀疑过滤条件。
- ✅ **`run_id` 是跨轨迹主键**：awaken / settle / compact 场景下 think trace 的 `run_id` 等于 MemoryTrace 的 `id`，这是前端主从联动、以及「从思考轨迹跳到原始输入输出」的唯一关联键。
- ✅ **目录化后的落位**：写入与查询同在 `src/service/dao/memory/sqlite/trace.rs`，DAL 门面在 `src/service/dal/memory/trace.rs`；`dao/memory/mod.rs` 只保留共享类型与常量，不要再往其中堆文件读写实现。
