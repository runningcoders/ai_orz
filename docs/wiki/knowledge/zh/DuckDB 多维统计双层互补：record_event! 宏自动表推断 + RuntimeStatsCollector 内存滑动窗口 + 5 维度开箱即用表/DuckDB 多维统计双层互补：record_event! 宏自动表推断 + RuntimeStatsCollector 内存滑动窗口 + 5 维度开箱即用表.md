---
kind: RAG 原子知识卡
name: DuckDB 多维统计双层互补：record_event! 宏自动表推断 + RuntimeStatsCollector 内存滑动窗口 + 5 维度开箱即用表
category: 基础设施 / 统计监控
scope:
  - "src/pkg/stats/**"
  - "src/service/domain/system/aop_stats.rs"
  - "src/service/domain/system/aop_monitor.rs"
  - "ai-orz-macros/src/**"
  - "src/handlers/sys/**/stats*.rs"
source_files:
  - src/pkg/stats/mod.rs#L1-L60 (模块总入口：持久化顶层 + runtime 子模块双层互补；record_event! 宏的三种调用模式说明)
  - src/pkg/stats/traits.rs (StatTable / StatEvent 两个 trait：注册表契约 + 事件序列化契约；自定义事件只需 impl 这两个)
  - src/pkg/stats/collector.rs#L1-L100 (Stats 核心收集器：DuckDB open + register_table + record 批量写入 + StatParam/StatFilter 五维度过滤聚合)
  - src/pkg/stats/runtime/mod.rs#L1-L80 (RuntimeStatsCollector 泛型内存收集器：WINDOW_MINUTES=60 滑动窗口 + TimeBucketSnapshot 分钟桶快照)
  - src/pkg/stats/tool_call.rs (ToolCallStatTable：工具调用维度表，字段 success/failure/tokens/duration，对应 DAO record_event! 打点)
  - src/pkg/stats/model_call.rs (ModelCallStatTable：模型调用维度表字段 tokens_input/tokens_output + ModelProvider 维度统计)
  - src/consumer/aop_stats_collector.rs (AopStatsCollector：基于 RuntimeStatsCollector<(EventKind, &str)> 的 AOP 中心统计面板数据)
  - src/service/domain/system/aop_stats.rs#L1-L80 (system domain aop_stats：handler 层查询接口 — 总览 / 分布 / 时序三段返回)
  - docs/archive/design-archive/stats_module_design.md（§定位与目标：持久化 vs 内存版双层互补；§默认表 5 类 AgentAwake/ModelCall/ToolCall/TaskEvent/ProjectEvent）
  - docs/archive/design-archive/stats_query_design.md（§Domain 层封装：StatFilter + StatAggregation 五维度查询封装）
  - docs/archive/plan-archive/统计图表Phase1基础设施与时序图展示重构.md（Phase1 落地：DuckDB 表结构 + record_event! 宏自动推断）
  - docs/archive/plan-archive/统计图表Phase2.md（Phase2 落地：五维度统计面板 + TokenSumResult 接口）
  - docs/archive/plan-archive/统计图表第三期.md（Phase3 落地：AOP 事件统计面板集成 RuntimeStatsCollector 内存版）
  - docs/wiki/zh/content/项目概述/核心功能特性/多维统计系统/多维统计系统.md（五维度总览：Agent/Project/Task/ModelProvider/Tool 五张卡片 + 入口说明）
  - docs/wiki/zh/content/项目概述/核心功能特性/多维统计系统/Agent 维度统计.md（Agent 维度时序折线：日调用次数、Token 消耗分布饼图）
  - docs/wiki/zh/content/项目概述/核心功能特性/多维统计系统/Tool 维度统计.md（Tool 成功/失败率柱状图 + 平均耗时箱线图）
  - docs/wiki/zh/content/基础设施/AOP 事件系统/统计与监控.md（AopStatsCollector 内存版数据：事件吞吐/积压/平均耗时）
  - docs/wiki/zh/content/前端应用/页面模块/系统管理页面/AOP 监控面板.md（前端面板：总览卡片 + 事件类型分布饼图 + 最近 60 分钟时序折线）
  - 【平行卡 1】docs/wiki/knowledge/zh/思考退出原因 exit_reason 统计与 ThinkRoundEvent AOP 事件链路/思考退出原因 exit_reason 统计与 ThinkRoundEvent AOP 事件链路.md（ThinkRoundStatsConsumer → 调 ToolCallStatTable record_event!）
  - 【平行卡 2】docs/wiki/knowledge/zh/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例.md（AOP 事件发布后走 AopStatsHook，实时打点到 RuntimeStatsCollector）
---

## §1 概述

**本卡角色**：统计模块的基础设施知识卡。覆盖 `pkg/stats/` 持久化版（DuckDB 嵌入式）+ `pkg/stats/runtime/` 内存版（泛型滑动窗口）双层互补架构、`record_event!` 宏的三种调用模式、5 个开箱即用的维度表（Agent/ModelProvider/Tool/Task/Project），以及 system domain `aop_stats.rs` 为 AOP 监控面板提供的三段查询结构（总览/分布/时序）。**定位：新增业务打点、排查面板无数据、写自定义统计表时读。**

- **双层互补设计**：`pkg/stats/` 顶层 DuckDB 版跨重启保留，适合「业务事件需要事后钻取」的 5 维度正式统计；`runtime/` 子模块的 `RuntimeStatsCollector<K>` 基于 Tokio RwLock<HashMap>，重启重置，适合「需要实时看 AOP/SSE/Channel 运行时能力」的场景。两者不互斥，同一个事件可以同时打两层。
- **record_event! 宏三种模式**：① 自动推断表（ctx, 结构体字面量）—— 默认从结构体名去掉 Event 后缀找 StatTable（如 `ModelCallEvent` → `ModelCallStatTable`），无需手写表名；② 自动推断 + 自定义 timestamp；③ 显式指定表（ctx, Table, event）。前两种模式需要结构体有 `#[derive(StatsEvent)]` 过程宏（ai-orz-macros crate 中定义）。
- **5 个开箱即用维度表**：`AgentAwakeStatTable`（Agent 唤醒统计，含 exit_reason）、`ModelCallStatTable`（模型调用：tokens 输入输出/耗时）、`ToolCallStatTable`（工具调用：成功失败/耗时/错误分类）、`TaskStatTable`（任务状态流转：创建/开始/完成/取消）、`ProjectStatTable`（项目跟进：巡检触发/进度变更）。对应 5 张独立 DuckDB 表，所有表字段完全一致：timestamp、tags(JSON)、metrics(JSON)，统一查询 API。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| mod.rs (pkg/stats) | 模块总入口 | pub use 导出 Trait + 5 张表 + record_event! 宏；文档头部明确「持久化 vs 内存版」二分语义 | `:L1-L60` |
| traits.rs | 核心 Trait 对 | StatTable::table_name/schema_sql；StatEvent::to_row()；所有表/事件均需实现 | 见 trait 定义 |
| collector.rs | DuckDB 核心收集器 | Stats::open(path, batch_size) → register_table(impl StatTable) → record(ctx, table, event) 批量落盘；StatFilter 五维度 + StatAggregation GROUP BY | `:L1-L100` |
| runtime/mod.rs | 内存泛型收集器 | RuntimeStatsCollector<K: Hash+Eq+Clone>：record(key, duration)；snapshot() 返回 RuntimeStatsSnapshot { total_counts, buckets[60 分钟桶] } | `:L1-L80` |
| tool_call.rs | Tool 维度表 | ToolCallEvent 字段：agent_id、tool_name、success(bool)、error_kind、duration_ms、tokens；对应 StatTable schema_sql 声明字段类型 | 见独立文件 |
| aop_stats_collector.rs (consumer) | AOP 内存统计对象 | 基于 RuntimeStatsCollector<(EventKind, &'static str)>，按「事件类别 + 动作」维度，AopStatsHook 每 publish 一次调用 collector.record | 见 consumer 目录 |
| aop_stats.rs (system domain) | 三段查询封装 | get_overview（总览：总事件数/消费速率/平均耗时/积压）→ get_distribution（按事件类型分布饼图）→ get_time_series（最近 60 分钟时序）对应前端 AOP 监控面板 3 区块 | `:L1-L80` |

**章节来源**
- [mod.rs:L1-L60](src/pkg/stats/mod.rs#L1-L60)
- [runtime/mod.rs:L1-L80](src/pkg/stats/runtime/mod.rs#L1-L80)
- [collector.rs:L1-L100](src/pkg/stats/collector.rs#L1-L100)
- [aop_stats.rs:L1-L80](src/service/domain/system/aop_stats.rs#L1-L80)

---

## §3 架构约定与扩展模式

### 3.1 新增业务统计事件（4 步最小模板）

1. **定义事件结构**：`src/pkg/stats/my_event.rs` → `pub struct MyEvent { pub agent_id: Option<String>, pub count: i32, ... }` + `#[derive(StatsEvent, Debug, Clone, Serialize)]`
2. **配套 StatTable**：同文件 `pub struct MyStatTable;` → `impl StatTable for MyStatTable`（table_name="my_events"、schema_sql="CREATE TABLE IF NOT EXISTS my_events (timestamp BIGINT, tags JSON, metrics JSON)"——通常用默认表结构不用改）
3. **注册进 module**：`pkg/stats/mod.rs` 顶部 `mod my_event;` + 底部 `pub use self::my_event::{MyEvent, MyStatTable};`
4. **consumer/producer 里打点**：`record_event!(&ctx, MyEvent { agent_id: Some(id), count: 42 });`（模式 1 自动推断表）

### 3.2 全局 Stats 初始化点

- `pkg::init_all()`（`lib.rs` 最底层）→ 内部调 `stats::init()` 一次性创建全局单例，使用 `OnceLock<Arc<Stats>>`
- 业务代码调用 `record_event!` 时宏内部调 `crate::pkg::stats::global().record(...)`，无需手动持有 Stats 引用

### 3.3 内存 vs 持久化选型铁律

| 选择持久化版 | 选择内存版 |
|-------------|-----------|
| 需要事后查询、跨重启保留 | 只看实时快照，重启重置可以接受 |
| 数据用于报表/对账/对比 | 数据用于监控面板/健康检查/实时告警 |
| 事件量 < 日 100w 条（DuckDB 单机上限 ~ 1000w/日） | 事件量无上限（仅保留 60 分钟滑动窗口）|
| 需要复杂 SQL 聚合（GROUP BY 多维度） | 只需要 count/sum/avg 简单聚合，调用方 snapshot 后自己算 |

---

## §4 硬约束与回归红线

1. **record_event! 永不返回业务错误**：统计失败不应该影响业务主流程。宏内部 `if let Err(e) = stats.record(...).await`，只打 `log_debug!`。严禁把 `?` 传播到调用方。
2. **内存版 WINDOW_MINUTES 禁止 < 10 或 > 240**：`pkg/stats/runtime/mod.rs` 顶部常量，修改后需同步更新前端监控面板「最近 X 分钟」下拉框默认值（当前 60）。
3. **所有 DuckDB 表必须是列式存储**：`schema_sql` 不能加 `WITHOUT ROWID`（DuckDB 不支持），但也不能加 SQLite 特有语法。跨存储兼容性由 DuckDB 兼容层保证。
4. **tags JSON 禁止超过 15 个键**：`tags` 用于 GROUP BY 维度过滤（`tags->>'$.agent_id'`），键过多会把 DuckDB 的 JSON 解析成本拉高。超过 15 个维度的业务 → 改用 `metrics` 字段（仅 sum/avg，不用于分组）。
5. **批量写入 batch_size 禁止 < 10 或 > 5000**：默认 `Stats::open(path, 100)` → 100 条一批批量写，太小则 flush 频繁 IO 爆炸，太大则进程崩溃丢失事件窗口过大。
6. **前端面板 3 段查询顺序不能乱**：总览 → 分布 → 时序。后端 `system/aop_stats.rs` 的返回结构体必须保持这 3 字段顺序；前端 AopStatsPanel 组件按这 3 段顺序渲染卡片，变更任一位置即破 UI。
