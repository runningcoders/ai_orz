---
kind: wiki_knowledge_card
name: 思考退出原因 exit_reason 统计与 ThinkRoundEvent AOP 事件链路
category: 统计与AOP事件
scope:
  - "src/models/events/think_round.rs"
  - "src/consumer/think_round_stats_consumer.rs"
  - "src/service/domain/runtime/awakening.rs"
  - "src/service/domain/runtime/think_loop.rs"
  - "src/pkg/stats/**"
source_files:
  - src/models/events/think_round.rs:Ln-Lm
  - src/consumer/think_round_stats_consumer.rs:Ln-Lm
  - src/service/domain/runtime/awakening.rs:Ln-Lm
  - src/service/domain/runtime/think_loop.rs:Ln-Lm
  - docs/design/thinking_task_policy_engine_design.md
  - （2026-09-04 清理：superpowers 目录已归档，待 doc-maintainer 跟进）
  - docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md
  - docs/wiki/zh/content/基础设施/AOP 事件系统/事件消费者/思考轮次统计消费者.md
  - docs/wiki/knowledge/zh/Agent 思考运行时 AgentThinkRuntime：挂载清理取消与每轮快照上报/Agent 思考运行时 AgentThinkRuntime：挂载清理取消与每轮快照上报.md
---

# 思考退出原因 exit_reason 统计 + ThinkRoundEvent AOP 事件链路

## §1 整体方案

思考循环每轮 + 最终退出都通过 AOP 事件中心投递 2 类事件：(a) **ThinkRoundEvent**（每轮，记录 rounds/elapsed/tokens/tools/policy_hits）→ 由 ThinkRoundStatsConsumer 消费，落到 DuckDB 多维统计（5 维度 + exit_reason 分布）；(b) **AgentAwakeEvent.exit_reason**（最终退出，与 ThinkRuntimeSnapshot.last_exit_reason 同一来源字符串）→ 统计「Agent 维度 exit_reason 分布」供系统健康仪表盘展示。所有策略命中（UserCancel/MaxRounds/Timeout/ContextOverflow）都在 awakening 统一映射为 exit_reason 字符串，**统计口径单一，不允许第二来源**。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构/入口 |
|------|------|-------------|
| [src/models/events/think_round.rs](src/models/events/think_round.rs) | 事件模型定义 | `ThinkRoundEvent`（agent_id/organization_id/user_id/scene/round/elapsed_ms/tokens_in/tokens_out/tool_calls_count/policy_hit_ids: Vec<String>）；`AgentAwakeEvent` 扩展字段 `exit_reason: Option<String>`（final/user_cancel/max_rounds_exceeded/timeout/context_overflow/summary_exit 6 个枚举值对应字符串）|
| [src/consumer/think_round_stats_consumer.rs](src/consumer/think_round_stats_consumer.rs) | AOP 消费者（事件订阅→统计入库）| Registry 订阅 ThinkRoundEvent topic；record_event! 宏投递 DuckDB 多维统计；按 scene + exit_reason 聚合；exit_reason 分布按月/按 Agent 维度存 5 分钟聚合表 |
| [src/service/domain/runtime/awakening.rs](src/service/domain/runtime/awakening.rs) | 事件投递方 + exit_reason 归一映射 | `map_triggered_to_result(policy_hit_ids) -> ThinkLoopResult`：所有策略命中 → ThinkLoopResult → exit_reason 字符串（单一事实源）；awaken 返回前 publish(AgentAwakeEvent{ exit_reason: Some(...) })；think_loop 每轮 publish(ThinkRoundEvent) |
| [src/service/domain/runtime/think_loop.rs](src/service/domain/runtime/think_loop.rs) | 每轮 ThinkRoundEvent 构造 | 每轮结束：组装 ThinkRoundEvent（policy_hit_ids 来自 Policy.evaluate() 结果，即本轮命中的所有策略 id 列表：or 组全部 / and 组全部）|
| 【Wiki 长文 1】运行时领域.md §5 详细分析 | exit_reason 归一化位置 + 与策略引擎协作 | [运行时领域](docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md) |
| 【Wiki 长文 2】思考轮次统计消费者.md | 消费者链路详情（消费框架约定）| [思考轮次统计消费者](docs/wiki/zh/content/基础设施/AOP%20事件系统/事件消费者/思考轮次统计消费者.md) |
| 【平行卡】AgentThinkRuntime（last_exit_reason 与 exit_reason 同源约束）| Agent 状态侧 | [AgentThinkRuntime 卡](docs/wiki/knowledge/zh/Agent%20思考运行时%20AgentThinkRuntime：挂载清理取消与每轮快照上报/Agent%20思考运行时%20AgentThinkRuntime：挂载清理取消与每轮快照上报.md) |
| 【① Design】thinking_task_policy_engine_design.md §一 §二 exit_reason 决策 | 设计决策表：为什么统一字符串映射 | [docs/design/thinking_task_policy_engine_design.md](docs/design/thinking_task_policy_engine_design.md) |
| 【② Plan】执行蓝图 §Task 8 统计事件接入 | 事件接入步骤 | （2026-09-04 清理：superpowers 目录已归档，待 doc-maintainer 跟进）|

## §3 架构约定

1. **exit_reason 字符串是单一事实源**：ThinkRuntimeSnapshot.last_exit_reason / AgentAwakeEvent.exit_reason / DuckDB exit_reason 维度值 → 全部使用 awakening.rs map_triggered_to_result 生成的同一枚举字符串；禁止手工写入不同文案。
2. **ThinkRoundEvent policy_hit_ids 是完整命中策略 id 列表**：单策略 → 只含 self.id；And 组 → 所有子策略 id 合并；Or 组 → 所有实际命中的子策略 id 合并。统计上通过 policy_hit_ids 长度可判断"一次退出是由多策略同时触发"。
3. **消费者 ThinkRoundStatsConsumer 零业务逻辑**：只做 record_event! → 统计入库，不做业务判断；如果 exit_reason 聚合要做"取消占比超 30% 告警"，这类业务逻辑放到 System 域的健康检查 task（不在 consumer 里）。
4. **两个事件的组织维度完整**：ThinkRoundEvent 与 AgentAwakeEvent 都必须携带 organization_id / user_id / agent_id（5 维度齐全）；缺一个维度会导致 DuckDB 多维统计无法切片（按组织/按用户/按 Agent）。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止 exit_reason 硬编码 i32 枚举值**：exit_reason 必须是字符串（final/user_cancel/max_rounds_exceeded/timeout/context_overflow/summary_exit），禁止把 ThinkLoopResult 变体的 i32 数字直接入库（版本漂移无法统计）。
2. ❌ **禁止 think_loop 中直接 publish AgentAwakeEvent**：AgentAwakeEvent.exit_reason 必须由 awakening() 在返回前统一 publish（保证 exit_reason 与 ThinkLoopResult 返回值一致，避免 think_loop 内部提前返回漏写）。
3. ✅ **consumer 注册必须幂等**：ThinkRoundStatsConsumer 在 consumer::init() 中注册，AOP 调度器启动前不可收到事件；测试环境按真实启动顺序（init → init_base_data → aop.init_all）对齐。
4. ✅ **record_event! 宏必须带 ctx**：所有统计事件投递带完整 ctx（补 organization_id/user_id），禁止系统级别无 ctx 的 record_event!（导致维度缺失）。
5. ✅ **6 种 exit_reason 强覆盖**：任何新增的 Policy / 退出路径必须在 6 个 exit_reason 字符串内选择其一，或新增一个则同步更新：① awakening map_triggered_to_result ② ThinkRuntimeSnapshot last_exit_reason ③ AgentAwakeEvent doc 注释 ④ DuckDB 维度字典 ⑤ 前端健康仪表盘展示。
6. ✅ **四类互引闭环**：本卡含 2 条 wiki 长文绝对路径（运行时领域.md + 思考轮次统计消费者.md）+ Design + Plan；每篇 wiki 长文 cite 段回链本卡 + 对应 Design/Plan。
