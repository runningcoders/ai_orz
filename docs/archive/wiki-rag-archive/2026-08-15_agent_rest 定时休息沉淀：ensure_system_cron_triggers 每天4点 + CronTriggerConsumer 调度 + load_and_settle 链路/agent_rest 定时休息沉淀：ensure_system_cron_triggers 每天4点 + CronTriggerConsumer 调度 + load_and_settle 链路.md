> 📦 归档标记（2026-08-15）：被 [Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle 向量去重合并](docs/wiki/knowledge/zh/Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle 向量去重合并/Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle 向量去重合并.md) 取代。保留原因：历史参考，主卡已吸收本卡独有源码锚点与硬约束。生效方案：主卡真实路径作为唯一 RAG 召回目标。
---
kind: RAG 原子知识卡
name: agent_rest 定时休息沉淀：ensure_system_cron_triggers 每天4点 + CronTriggerConsumer 调度 + load_and_settle 链路
category: 记忆系统 / 定时调度
scope:
  - "src/service/domain/system/mod.rs"
  - "src/consumer/scheduler.rs"
  - "src/producer/cron_trigger.rs"
  - "src/handlers/hr/agent/settle_memory.rs"
  - "src/models/cron_trigger.rs"
  - "src/pkg/cron/*.rs"
source_files:
  - src/service/domain/system/mod.rs#L415-L472 (ensure_system_cron_triggers：幂等创建 2 条系统触发器：agent_rest 每天 04:00 Cron + project_followup 每 3600 秒间隔)
  - src/consumer/scheduler.rs#L39-L95 (CronTriggerConsumer：AOP 事件 cron.trigger → match action 路由；agent_rest → handle_agent_rest；project_followup → followup 分支)
  - src/consumer/scheduler.rs#L100-L127 (handle_agent_rest：解析 payload.agent_id + extra.settle_limit → 调 load_and_settle → 日志记 settled_count)
  - src/producer/cron_trigger.rs#L38-L87 (CronTriggerProducer：每分钟扫 CronTriggerDao.list_due，发布 cron.trigger 事件；max_events 限流防压垮)
  - src/handlers/hr/agent/settle_memory.rs#L1-L133 (load_and_settle 公共函数：无状态加载 Agent + 调 RuntimeAwakening::sleep_and_settle → 内部走 DAL settle)
  - src/models/cron_trigger.rs (CronTriggerPo：cron_expression/interval_seconds/payload/next_run_at/last_run_at/is_enabled 六字段核心)
  - src/pkg/cron/mod.rs#L1-L60 (next_run_at 计算 + system_timezone 获取；Cron 表达式解析基于 chrono-crate)
  - tests/integration/system_cron_triggers_test.rs (系统级集成测试：校验 init_base_data 后 agent_rest + project_followup 两条触发器确实存在且 is_enabled=1)
  - docs/archive/design-archive/memory_system_enhancement_design.md（§1 决策 3/4：SystemDomain + CronManager；休息双轨 = 上下文过载小憩 + 每日定时睡眠沉淀）
  - docs/design/runtime_design.md（AgentRuntimeInfo::Resting 状态定义与 BusyGuard 嵌套，防止沉淀期被外部唤醒并发冲突）
  - （占位：待 ai-orz-doc-maintainer 落地后回填真实 Plan 路径 → 预期 docs/archive/plan-archive/定时任务系统建设与agent_rest沉淀链路.md）
  - docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/记忆沉淀机制.md（沉淀工作队列架构图 §2 CronTriggerProducer→Consumer→DAL 链路 + §8 故障排查）
  - docs/wiki/zh/content/功能模块/系统管理功能/系统定时任务管理.md（后台 CronTrigger 面板：暂停/恢复/手工触发按钮与状态列）
  - docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md（§4 三态 FSM Idle/Busy/Resting Resting 态 RAII 保护）
  - 【平行卡 1】docs/wiki/knowledge/zh/四层记忆沉淀：save_short_term 与 save_long_term 工具拆分 + settle 向量去重合并策略/四层记忆沉淀：save_short_term 与 save_long_term 工具拆分 + settle 向量去重合并策略.md（沉淀真正实现 = settle_short_term_to_long_term；本卡是它的上游调度链路）
  - 【平行卡 2】docs/wiki/knowledge/zh/AgentRuntimeInfo 三态状态机 + BusyGuard RAII：Idle Busy Resting 转换 + task_id project_id 业务上下文透视/AgentRuntimeInfo 三态状态机 + BusyGuard RAII：Idle Busy Resting 转换 + task_id project_id 业务上下文透视.md（沉淀期间 Agent 进入 Resting，BusyGuard RAII 防外部唤醒冲突）
---

## §1 概述

**本卡角色**：记忆沉淀「每日定时触发」的上游调度链路卡。覆盖从 `system::init_base_data` 的两条系统级 Cron 触发器幂等注入 → 每分钟 Producer 扫描到期触发器发 AOP 事件 → Scheduler Consumer 按 action 路由 agent_rest → 复用 `load_and_settle` 公共函数与神经工具路径完全一致的整条闭环。**定位：排查沉淀未运行、手工调试沉淀触发器、新增 Cron 系统动作时读。**

- **双触发语义**：动作触发不代表沉淀必执行——进入 Resting 状态前需先过 AgentRuntimeInfo `try_set_resting()` CAS（与其他 Busy 唤醒互斥），若 Agent 当前正忙则跳过，日志记「跳过沉淀（Agent 正忙）」不会重试。
- **幂等注入**：`ensure_system_cron_triggers` 启动时先全量 `list_triggers(query=default)`，按 `payload.contains("\"agent_rest\"")` 字符串匹配判断已存在与否——不依赖 id/name 避免人工改名后重复注入。
- **Cron 表达式系统时区**：`0 0 4 * * *`（每天 04:00）按 `crate::pkg::cron::system_timezone()` 计算 `next_run_at`，与 UTC 的差由系统环境变量 `TZ` 控制，容器化部署必须挂 `/etc/timezone`。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 锚点 |
|------|------|---------|------|
| system/mod.rs (Domain) | 触发器启动注入 | `ensure_system_cron_triggers` = 先 list→再按 payload 字符串匹配去重→create 两条：agent_rest(Cron 0 0 4 * * *) + project_followup(3600s interval) | `:L415-L472` |
| cron_trigger.rs (Producer) | 到期扫描器 | 每分钟 tick：`CronTriggerDal.list_due(ctx, now, max_events=20)` → 逐个发布 AOP 事件 cron.trigger → 事件被消费后调 mark_trigger_executed 更新 next_run_at | `:L38-L87` |
| scheduler.rs (Consumer) | Cron 动作路由 | 同步 Consumer：解析 CronTriggerEvent.payload → JSON 反序列化为 action 枚举 → match "agent_rest" / "project_followup" | `:L39-L95` |
| scheduler.rs (Consumer) | agent_rest 动作 | payload.extra.settle_limit 默认 10 → 调 load_and_settle(ctx, agent_id, settle_limit).await → 返回 settled_count 记日志 | `:L100-L127` |
| settle_memory.rs (Handler) | load_and_settle 公共函数 | 加载 Agent（按 agent_id）→ 创建 RequestContext(uid=agent.owner_uid) → 调 Runtime::sleep_and_settle(ctx, agent, limit) | `:L1-L133` |
| cron_trigger.rs (Model) | PO 字段 | cron_expression / interval_seconds 二选一；payload JSON 字符串存 action + extra；next_run_at = 下次执行时间戳（ms）；last_run_at 记录上次 | 见 Model |
| system_cron_triggers_test.rs | 集成测试 | 启动 init_full_test_env → 查 cron_triggers → 断言两条触发器存在且 payload 正确（防启动注入逻辑被改）| 见 tests/integration |

**章节来源**
- [system/mod.rs:L415-L472](src/service/domain/system/mod.rs#L415-L472)
- [cron_trigger.rs:L38-L87](src/producer/cron_trigger.rs#L38-L87)
- [scheduler.rs:L39-L95](src/consumer/scheduler.rs#L39-L95)
- [scheduler.rs:L100-L127](src/consumer/scheduler.rs#L100-L127)

---

## §3 架构约定与扩展模式

### 3.1 完整调度链路

```
service::init_base_data() 启动阶段
  └── domain::system::ensure_system_cron_triggers(ctx)
        └── cron_manager.create_trigger (如果 payload 不存在)
              └── agent_rest: cron_expression="0 0 4 * * *"
                            payload={"action":"agent_rest","extra":{"settle_limit":10}}
                            
生产/消费阶段（运行时持续）
  CronTriggerProducer (每分钟轮询 tick)
    → list_due(now, max=20)
    → 发布 AOP 事件 cron.trigger (event_id 关联)
  CronTriggerConsumer (同步消费)
    → JSON 解析 payload.action = "agent_rest"
    → handle_agent_rest(event, &payload.extra)
          └── load_and_settle(ctx, agent_id, settle_limit=10)
                └── RuntimeAwakening::sleep_and_settle
                      ├── try_set_resting() CAS (Busy 则跳过)
                      ├── BusyGuard(Resting) RAII
                      ├── MemoryDAL::settle_short_term_to_long_term(limit)
                      └── BusyGuard Drop → Idle
    → cron_manager.mark_trigger_executed(id, next_run_at_recalc)
```

### 3.2 扩展模式：新增一条系统级 Cron 动作

1. **Payload 约定**：`{"action":"your_new_action","extra":{...}}` — action 用 snake_case，extra 内字段都是可选（Consumer 端用 `serde(default)` 防解析报错）。
2. **注入触发器**：在 `ensure_system_cron_triggers` 末尾追加 `let has_yours = existing.iter().any(|t| t.payload.contains("\"your_new_action\"")); if !has_yours { ... }`。
3. **Consumer 路由分支**：`scheduler.rs handle_event` match 块加一行 `"your_new_action" => self.handle_yours(&event, &payload.extra).await?`。
4. **集成测试**：在 `system_cron_triggers_test.rs` 追加对「触发器存在 + payload 包含 your_new_action」的断言（防未来有人误删注入代码）。

---

## §4 硬约束与故障排查

### 4.1 必守红线

1. **红线 1**：CronTriggerConsumer 的 handle_agent_rest **永不 panic**。即使 payload 缺字段也只打 warn 跳过——否则整条 Cron 消费者线程崩了，所有触发器都不再执行。
2. **红线 2**：`ensure_system_cron_triggers` 是「有则跳过、无则创建」的幂等语义。**绝不做 UPDATE**——用户手工改了 cron_expression（把 04:00 → 03:00）时，系统启动不能覆盖用户设置。
3. **红线 3**：Agent 沉淀期间如果收到唤醒请求，**唤醒必须排队等待沉淀结束（Resting→Idle）或返回 429「Agent 正休息」**，绝不允许 Runtime Awake 与 Sleep 同时进入—— BusyGuard 已从语义上保证，但是 Handler 层还要做 429 兜底。
4. **红线 4**：`next_run_at` 计算失败（Cron 表达式非法）时，**触发器必须置 is_enabled=0 + 打 sys_error**，不能永远排队失败但还一直标记 enabled。否则 max_events=20 永远被同一条非法触发器占满，其他正常触发器不再跑。

### 4.2 故障排查路径

| 症状 | 起点锚点 | 次级排查 |
|------|---------|---------|
| 每天早上看知识图谱，没有新节点沉淀（连续 >1 天） | [system/mod.rs:L415-L472](src/service/domain/system/mod.rs#L415-L472) 检查是否有 agent_rest 触发器 | 查 `SELECT cron_expression, is_enabled, next_run_at FROM cron_triggers WHERE payload LIKE '%agent_rest%'`；如果 next_run_at < now，说明 Producer 没跑，查 AOP 调度器启动日志 |
| 触发器存在、next_run_at 也每天推进，但就是看不到新节点 | [scheduler.rs:L100-L127](src/consumer/scheduler.rs#L100-L127) grep 日志关键词 `agent_rest action triggered` | 看后面是否有 `Agent 正忙，跳过沉淀`——说明每天 4 点 Agent 恰好醒着（被调了），可把 cron_expression 改成 0 0 5 或更晚时间 |
| 系统重启后每天重复新增 1 条 agent_rest 触发器（DB 里越来越多） | 检查 `has_agent_rest` 判断条件是否写对 | 应是 `t.payload.contains("\"agent_rest\"")`（带双引号），如果写成 `t.payload.contains("agent_rest")`，另一条 project_followup 触发器的 payload 如果包含 action=agent_rest 字符串就会误判为存在，导致每次启动都重复插入 |
| 容器化部署，发现沉淀每天 UTC 04:00 执行（比预期早 8 小时） | [pkg/cron/mod.rs](src/pkg/cron/mod.rs) 查 system_timezone 实现 | 检查容器内是否正确挂载宿主机 /etc/timezone 或 TZ 环境变量；K8s 用 PodSpec.volumes hostPath=/etc/localtime |
| 单条沉淀执行时间 > 60 秒，同一 Agent 下一分钟 Producer 又扫一次触发了第二次（并发冲突） | 查 mark_trigger_executed 调用是否在「业务逻辑开始前」先占了 | **正确顺序：① 先 mark next_run_at → ② 再跑 handle_agent_rest**，否则业务逻辑跑 > 60 秒，下一轮 Producer 的 list_due 还能扫到同一条。如果仍偶发，可把 max_events=20 → 10（缓解）|
