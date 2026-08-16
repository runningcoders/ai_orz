---
kind: knowledge_card
name: 统计查询 API 与前端仪表盘：DuckDB 5 维表查询 + RuntimeStats 内存滑动聚合 + StatsHandler REST API + 前端 Line/Donut/Gauge 展示
category: 前端可视化
scope:
  - src/pkg/stats/**/*.rs
  - src/pkg/stats/runtime/**/*.rs
  - src/service/dal/stats.rs
  - src/service/dao/**/stats_duckdb.rs
  - src/handlers/system/**/*stats*.rs
  - frontend/src/components/**/*.{ts,tsx}
  - frontend/src/pages/**/*系统管理*.{ts,tsx}
source_files:
  - src/pkg/stats/mod.rs#L31-L171
  - src/pkg/stats/runtime/mod.rs#L33-L178
  - src/handlers/system/aop_stats.rs#L1-L72
  - src/service/dao/model_provider/stats_duckdb.rs
  - src/service/dal/stats.rs
  - frontend/src/components
  - docs/archive/design-archive/stats_module_design.md
  - docs/archive/design-archive/stats_query_design.md
  - docs/archive/plan-archive/统计图表Phase1基础设施与时序图展示重构.md
  - docs/wiki/zh/content/基础设施/AOP 事件系统/统计与监控.md
  - docs/wiki/zh/content/项目概述/核心功能特性/多维统计系统/多维统计系统.md
  - docs/wiki/zh/content/功能模块/系统管理/系统监控与健康检查.md
---

## §1 概述与定位

本知识卡描述 ai_orz 项目的统计查询与仪表盘全链路架构，覆盖 DuckDB 持久化多维查询（5 实体 Stats DAO 全实体覆盖）、RuntimeStatsCollector 内存滑动窗口聚合（AOP 旁路采集）、Stats REST API 三端点（overview/time-series/distribution）、前端图表组件（Line 折线/Donut 环形/Gauge 仪表盘）四大层级。触发读取场景：新增实体统计查询接口、排查时序聚合或 QPS 计算、新增前端统计图表组件、理解双层选型（持久化 vs 内存）决策时。

## §2 关键文件表

| 文件 | 角色 | 核心入口/约束 |
|------|------|---------------|
| [pkg/stats/mod.rs](src/pkg/stats/mod.rs) | Stats 顶层（DuckDB 持久化版） | `Stats::open()` + `batch_size` 批量写入；`record_event!` 宏三种模式（自动表/结构体/显式表）；global_stats() 无 ctx 单例访问；5 张专用表：default_events/model_call_events/tool_call_events/agent_awake_events/project_events/task_events |
| [pkg/stats/runtime/mod.rs](src/pkg/stats/runtime/mod.rs) | RuntimeStatsCollector 内存版 | WINDOW_MINUTES=60 滑动窗口；泛型 K 维度键（Clone+Eq+Hash+Send+Sync）；record(key, Option<duration>)：None 只计数 Some 计数+累计耗时；snapshot() 深拷贝返回 RuntimeStatsSnapshot + buckets 升序桶 + total_counts 全生命周期；evict_old_buckets 每分钟淘汰 |
| [handlers/system/aop_stats.rs](src/handlers/system/aop_stats.rs) | AOP 实时统计 HTTP 接口 | 3 端点：GET overview（5 指标 total_published/consumed/success/failed/avg_duration_ms）；GET time-series（event_kind/consumer_name/status 过滤 + bucket 聚合）；GET distribution（group_by=event_kind/consumer_name/status + status 过滤） |
| [stats_module_design.md](docs/archive/design-archive/stats_module_design.md) | Stats 双层互补框架设计 | StatEvent + StatTable 双 trait；专用表 is_dedicated_table() + column_sql()/metric_sql() 表自描述；query_aggregation 通用聚合；query_time_series 时序；双路径选型铁律表 |
| [stats_query_design.md](docs/archive/design-archive/stats_query_design.md) | 统计查询 Domain 层设计 | 5 Stats DAO 全实体覆盖：Agent/Project/Task/Tool/ModelProvider；StatsFetchOptions 按需动态注入模式（列表默认不加载，详情页按需开）；DAL 层统一 get_stats(id, options) + get_model_call_stats(id, options) |
| [统计图表Phase1基础设施与时序图展示重构.md](docs/archive/plan-archive/统计图表Phase1基础设施与时序图展示重构.md) | 统计 Phase1 Plan 快照 | DuckDB 建表 + record_event! 宏自动推断；前端图表组件封装（LineChart/DonutChart/GaugeChart 三组件） |
| [多维统计系统.md](docs/wiki/zh/content/项目概述/核心功能特性/多维统计系统/多维统计系统.md) | 五维度总览 Wiki | Agent/Project/Task/ModelProvider/Tool 五维度统计卡片入口 |

## §3 架构与约定

```
前端仪表盘 (4 种视图)
├─ 总览卡：5 指标卡片（Published/Consumed/Success/Failed/AvgDuration）
├─ 折线图：LineChart — 60 分钟时序 buckets（每分钟一个点）
├─ 环形图：DonutChart — 维度分布（按 event_kind/consumer_name/status 分组）
└─ 仪表盘：GaugeChart — 当前吞吐率 + 成功率
     ▲ REST API (JSON)
Stats Handlers (3 端点)
├─ GET  /system/aop/stats/overview      — 5 指标，AopStatsOverviewResponse
├─ GET  /system/aop/stats/time-series   — points[] interval_start + call_count
└─ GET  /system/aop/stats/distribution  — items[] label + value
     ▲ Domain → DAL → DAO
├─ DuckDB 持久化版（5 表）：default_events / model_call_events / tool_call_events / agent_awake_events / project_events / task_events
│   ├─ Stats.query_aggregation()   — 通用聚合 COUNT/SUM/AVG + group_by + 过滤
│   └─ Stats.query_time_series()   — 时序截断 timestamp/interval_ms + 聚合
└─ RuntimeStatsCollector 内存版（滑动 60min）
    ├─ AopStatsCollector wrap RuntimeStatsCollector<(event_kind, consumer_name, status)>
    ├─ record() → bucket 计数 + 可选累计耗时
    └─ snapshot() → overview() 在快照基础上业务聚合（按 status 分类）
```

**核心机制要点：**

1. **双层选型铁律表**：需跨重启保留历史、需要复杂 SQL 聚合过滤、数据量大且有长期价值 → DuckDB 持久化版。实时监控前端轮询、60 分钟内窗口足够、重启重置可接受、数据无持久化价值（AOP 事件、连接数、队列深度）→ RuntimeStatsCollector 内存版。禁止反过来：业务事件落内存版导致重启丢失。

2. **record_event! 宏三种调用模式**：最简模式 `record_event!(ctx, ModelCallEvent{...})` → 自动根据事件类型找注册的表 + 自动填充当前时间戳；结构体模式 `record_event!(ctx, event_expr)` → 直接传已构造事件；显式表模式 `record_event!(ctx, &MyTable, ModelCallEvent{...})` → 兼容自定义表。内部使用 ctx.stats_opt() 安全获取 Stats，未初始化时优雅跳过不 panic。

3. **5 Stats DAO 全实体覆盖 + 按需注入**：AgentStatsDao（agent_awake_events，call_summary）、ProjectStatsDao、TaskStatsDao、ToolStatsDao（tool_call_events，支持 agent_id 过滤）、ModelProviderStatsDao（model_call_events，全维度过滤 call_summary+token_summary+time_series）。每个实体详情页通过 FetchOptions.with_stats 控制是否加载统计，列表接口默认不加载，避免无谓 DuckDB 聚合。

4. **RuntimeStatsCollector 滑动窗口算法**：WINDOW_MINUTES=60 常量；VecDeque 按分钟桶升序存储；每次 record 先 evict_old_buckets（cutoff = 当前分钟 - 60*60000），淘汰旧桶只保最近 60 分钟；total_counts/total_duration_ms/total_completed 三总计数器不受滑动窗口限制，全生命周期进程级累计，重启才重置。

5. **前端四图表组件映射**：总览卡 = 5 指标映射 GET overview 的 5 字段；LineChart 折线 = GET time-series 的 points[]（X 轴 interval_start 格式化，Y 轴 call_count）；DonutChart 环形 = GET distribution 的 items[]（label+value 做饼图）；GaugeChart 仪表盘 = overview 的 success/total 算成功率 + instant_qps 从最近 1 秒桶计算。

## §4 硬约束与红线

1. **双层选型不违反**：禁止业务事件（ModelCall/ToolCall/Project/Task）落内存版导致重启丢失；禁止 AOP 吞吐/连接数落 DuckDB 持久化造成磁盘 IO 浪费。
2. **record_event! 必须走 stats_opt()**：禁止直接 ctx.stats() 遇到未初始化 panic，所有宏调用内部用 stats_opt() 未初始化时 Ok(()) 跳过。
3. **StatsFetchOptions 按需不默认**：列表接口默认关闭统计，禁止默认 with_stats=true 导致大列表触发 N×DuckDB 聚合。
4. **滑动窗口 WINDOW_MINUTES 不随意调大**：默认 60，超过会导致内存膨胀（60桶~38KB，扩大需评估内存占用）。
5. **5 实体 Stats DAO 全边界清晰**：ModelProviderStatsDao 是模型调用领域唯一 DAO（call+token+time_series 全能力），其他 4 个 DAO 只负责自身 call_summary，禁止交叉职责。
6. **StatTable 表自描述不硬编码**：新增专用表必须覆盖 is_dedicated_table()=true + 继承 column_sql/metric_sql 自动切直接字段引用，禁止查询构建层 if table_name 判断。
7. **.batch_size 批量写入不设 1**：每个表有独立缓冲队列，batch_size=1 会导致每条事件都刷 DuckDB，严重降低写入性能。
8. **snapshot() 必须返回深拷贝**：调用方释放读锁后安全聚合，禁止返回 &Inner 引用导致锁竞争。
9. **duration Option 语义固定**：None=只计数不计时（published/processing 状态），Some=计数+累计耗时（success/failed 等终止状态），禁止在框架层硬编码状态判断。
10. **前端图表组件禁止硬编码 API URL**：所有统计接口调用统一封装到前端 api/stats client，禁止组件内部 fetch 写死 path。
