---
kind: RAG 原子知识卡
name: Cron 任务调度与系统启动顺序：5 字段 cron 解析 + ensure_system_cron_triggers 2 条注入 + CronTriggerProducer 60s 轮询 + 两阶段 init/init_base_data/aop 严格分离
category: 基础设施 / 调度与启动
scope:
  - "src/service/dao/cron_trigger/**"
  - "src/service/dal/cron_trigger.rs"
  - "src/service/domain/system/mod.rs"
  - "src/producer/cron_trigger.rs"
  - "src/consumer/scheduler.rs"
  - "src/handlers/system/cron_trigger/**"
  - "src/models/events/cron_trigger.rs"
  - "src/lib.rs (启动总顺序)"
source_files:
  - src/service/dao/cron_trigger/mod.rs#L1-L80 (CronTriggerDao trait：CRUD + list_due_triggers(now_ts, limit) WHERE next_fire_at<=now + enabled；5 字段 cron 表达式 minute hour day-of-month month day-of-week → 计算下一次 fire 时间戳)
  - src/service/dao/cron_trigger/sqlite.rs#L1-L120 (SQLite impl：cron_parse crate 解析 5 字段表达式 → `iterator.upcoming(Utc).next()` 算 next_fire_at；list_due_triggers 按 last_fired_at + next_fire_at 双字段锁乐观并发)
  - src/service/domain/system/mod.rs#L43-L90 (SystemDomain::init_base_data → ensure_system_cron_triggers：先查 cron_triggers WHERE kind IN("agent_rest", "stats_collect") COUNT → 0 则 INSERT 两条系统默认：① agent_rest cron="0 4 * * *" 每天 4 点 ② stats_collect cron="*/5 * * * *" 每 5 分钟收集统计)
  - src/producer/cron_trigger.rs#L38-L80 (CronTriggerProducer AOP Producer：poll_interval_secs=60 每分钟轮询；poll() 内 system().cron_manager().list_due(ctx, now, 100) → 每条 due → aop::publish(SchedulerTriggerFiredEvent{cron_trigger_id, kind, payload}) → UPDATE SET last_fired_at=now, next_fire_at=calc_next())
  - src/consumer/scheduler.rs#L1-L90 (SchedulerConsumer Sync 消费 SchedulerTriggerFiredEvent：match kind：agent_rest → HR::agent_rest_all(ctx) 遍历所有在线 Agent 做 settle 沉淀；stats_collect → DuckDB record_event 汇总 + RuntimeStatsCollector flush；backup 每日 → Finance::Backup.create)
  - src/lib.rs#L20-L70 (启动总顺序强制执行：pkg::init_all → service::init → producer::init → consumer::init → service::init_base_data().await【2 条系统 cron 注入于此】→ aop stats hook → aop::init_all()【AOP 调度器启动，cron producer 开始 poll】→ HTTP 启动；红线：**init_base_data 绝对不能放 consumer::init / producer::init 之前**，否则 poll 时系统 cron 还没注入就漏掉)
  - src/handlers/system/cron_trigger/list_cron_triggers.rs (Handler：管理员 list_cron_triggers，支持按 kind/status 过滤，分页；用户创建自定义 cron POST create_cron_trigger 需 cron 表达式格式校验 + 不允许创建 */1 * * * * < 1 分钟过于频繁的任务（防止队列爆）)
  - docs/design/task_scheduler_design.md（§5 字段 cron 解析约束 §轮询间隔 60s 与 fire 时间误差容忍 §乐观并发锁 last_fired_at CAS 更新）
  - docs/design/runtime_design.md（§Agent 唤醒链路与 agent_rest 关系 §Resting 状态不允许唤醒 §Resting 结束后 BusyGuard 自动回 Idle）
  - docs/design/event_design.md（§SchedulerTriggerFiredEvent 事件负载 §8 类消费者注册顺序 §Ack/Nack delivery attempt 记录）
  - docs/design/consumer_architecture.md（§SchedulerConsumer 与 AgentLoopConsumer 注册顺序 §AOP Registry register 顺序 = 消费顺序）
  - docs/plan/唤醒上下文与睡眠约束.md（§agent_rest 每天 4 点触发 §resting 期间 pending_message 不唤醒 §settle 写入知识图谱后 Agent 自动转入 Idle）
  - docs/plan/统计图表Phase1基础设施与时序图展示重构.md（§stats_collect 每 5 分钟 cron 任务 §DuckDB record_event! 汇总 RuntimeStatsCollector memory 数据）
  - docs/plan/通用后台任务模块与Seed异步化重构.md（§自定义 cron trigger 创建表单 §5 字段 cron 校验 §禁止 < 1 分钟间隔 §trigger 启用/禁用切换）
  - docs/wiki/zh/content/项目概述/核心功能特性/系统管理功能/定时任务管理.md（定时任务页面：SystemTriggers 页面，两条系统默认 readonly，用户自定义可 CRUD + 立即触发按钮）
  - docs/wiki/zh/content/功能模块/系统管理/定时任务调度.md（调度总览：5 字段 cron + 轮询 + 事件分发 + 消费者处理链路 + 监控指标）
  - docs/wiki/zh/content/核心模块/AOP 事件系统/消费者框架/定时任务消费者.md（SchedulerConsumer 职责：3 类系统任务 agent_rest/stats_collect/backup_daily + 用户自定义 kind 派发到对应 handler）
  - 【平行卡 1】docs/wiki/knowledge/zh/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册.md（AOP 框架：Producer/Consumer/Registry 三核心 + 8 类消费者 SchedulerConsumer 是其中之一）
  - 【平行卡 2】docs/wiki/knowledge/zh/Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle 向量去重合并/Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle 向量去重合并.md（agent_rest cron → HR::agent_rest_all → 每个 Active Agent 调 settle 沉淀 Working→Short→Long 记忆）
---

## §1 概述

**本卡角色**：Cron 5 字段调度 + 系统启动两阶段严格分离 + 2 条默认系统 cron 注入的知识卡。覆盖 CronTriggerDao（cron 解析 + list_due 乐观锁）、init_base_data 的 ensure_system_cron_triggers（每天4点 agent_rest + 每5分钟 stats_collect）、CronTriggerProducer 60s 轮询→ publish 事件→ SchedulerConsumer 消费链路、启动总顺序（init_base_data 必须在 aop::init_all 之前完成，否则首次 poll 漏掉系统默认）。**定位：新增自定义 cron kind、排查 agent_rest 没跑导致记忆不沉淀、系统启动后 stats_collect 立即触发因为顺序错、写定时任务忘记配置幂等时读。**

- **5 字段 cron 表达式 + 轮询模式（不用实时调度线程）**：cron 格式 `minute hour dom month dow`（标准 Unix cron），不支持秒级（粒度 1 分钟足够业务：agent_rest 天级、stats 5 分钟、备份日级）。实现方式不是 cron 库 spawn 一堆 timer（Agent 多了 OS 线程爆炸），而是 **CronTriggerProducer poll_interval_secs=60 每分钟扫一次**：① 取 now_ts 当前时间戳；② SELECT * FROM cron_triggers WHERE enabled=1 AND next_fire_at <= now_ts LIMIT 100（取到期触发器）；③ 每条 due trigger 计算 next_fire_at = 5字段cron.upcoming(now).next()；④ UPDATE cron_triggers SET last_fired_at=now_ts, next_fire_at=next_ts WHERE id=? AND last_fired_at=<old_value>（CAS 乐观并发，多实例部署时只有一个能 UPDATE 成功，避免重复触发）；⑤ 成功则 publish(SchedulerTriggerFiredEvent) 走 AOP。
- **两条系统默认 cron + init_base_data 幂等注入**（system/mod.rs ensure_system_cron_triggers）：① **agent_rest（每天 04:00）** cron="0 4 * * *"：低峰期遍历所有 status != Draft 的 Agent，逐个调用 HR::agent_rest → Working Memory 摘要写 ShortTerm + ShortTerm 7 天前 merge 入 LongTerm 知识图谱 → AgentRuntimeInfo 状态临时切 Resting（期间收到消息 AgentLoopConsumer 不唤醒，append pending_messages）。② **stats_collect（每 5 分钟）** cron="*/5 * * * *"：RuntimeStatsCollector（内存滑动窗口，例如 Agent 响应延迟 / AOP 队列积压数）flush 到 DuckDB 持久化表（record_event! 批量写）。注入幂等：先 `SELECT COUNT(*) FROM cron_triggers WHERE kind IN ("agent_rest","stats_collect")` → 结果为 2 就不 INSERT（重启不重复创建）。系统默认 kind 在 UI 上标记 readonly 不能被用户 DELETE。
- **启动总顺序 6 步严格分离（AGENTS.md §4.10 强制执行）**（lib.rs run()）：**① pkg::init_all()**（最底层日志/JWT/工具 OnceLock，一次性）→ **② service::init()**（DAO→DAL→Domain 单例注册，纯内存不碰 DB）→ **③ producer::init() / consumer::init()**（AOP 订阅者注册，还没真正开始 poll/consume）→ **④ service::init_base_data().await**（DB IO 异步，ensure_system_cron_triggers 在此执行 2 条默认 INSERT）→ **⑤ aop::init_all()**（AOP 调度器启动，CronTriggerProducer 首次注册后等 poll_interval_secs=60s 才开始 poll，确保 init_base_data 已经完成写 DB）→ **⑥ HTTP serve**。**红线（核心）**：把 init_base_data 放到 aop::init_all 之后 = 灾难：首次 aop poll 时 cron_triggers 表还空 → agent_rest/stats_collect 永不触发 → 用户反馈「记忆不沉淀 + 统计图表没数据」，排查成本极高。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| dao/cron_trigger/mod.rs CronTriggerDao Trait | 调度数据访问 | CRUD + list_due_triggers(ctx, now_ts, limit)：WHERE enabled AND next_fire_at <= now；update_trigger_fire(ctx, id, old_last_fired, new_next)：CAS 乐观并发 UPDATE 原子；支持 cron 表达式验证 fn validate_cron(expr) -> bool | `:L1-L80` |
| dao/cron_trigger/sqlite.rs SQLite Impl | SQLite 实现 | 用 `cron::Schedule` 解析 5 字段表达式；`schedule.upcoming(Utc).next()` 拿下次 DateTime；CAS UPDATE：UPDATE ... WHERE last_fired_at = ?1 RETURNING id；返回受影响行数=1 则成功 | `:L1-L120` |
| domain/system/mod.rs ensure_system_cron_triggers | 系统默认幂等注入 | init_base_data() 内 try/warn 包裹：COUNT kind IN (agent_rest, stats_collect) → 缺哪个 INSERT 哪个；失败只 sys_warn 不 panic（失败不影响主流程启动）；不重复创建 | `:L43-L90` |
| producer/cron_trigger.rs CronTriggerProducer | AOP 轮询生产者 | impl Producer；poll_interval_secs=60；poll()：list_due → CAS UPDATE 每条 → publish SchedulerTriggerFiredEvent{cron_trigger_id, kind, payload_json}；失败记录 log，不 throw | `:L38-L80` |
| consumer/scheduler.rs SchedulerConsumer | AOP 同步消费者 | impl Consumer Sync 模式；consume(ctx, SchedulerTriggerFiredEvent)：match kind；自定义 kind 走动态注册表（用户 cron handler）；Ack/Nack 自动写入 system_delivery_attempts 表 | `:L1-L90` |
| handlers/system/cron_trigger/* | 用户 cron CRUD Handler | list_cron_triggers：分页 + 过滤 status/kind；create_cron_trigger：校验 cron 表达式 + 拒绝 < 1 分钟间隔 + payload JSON schema；系统默认 kind 返回 403 「readonly」 | 见 list/create Handler |
| lib.rs run() 启动总顺序 | 6 步序列 | pkg::init → service::init → producer/consumer::init → service::init_base_data().await → aop::init_all → axum serve；每一步用 comment 标注顺序与 why（防止未来有人手贱调整） | `:L20-L70` |

**章节来源**
- [task_scheduler_design.md:L1-L60](docs/design/task_scheduler_design.md#L1-L60)
- [system/mod.rs:L43-L90](src/service/domain/system/mod.rs#L43-L90)
- [lib.rs run()](src/lib.rs)

---

## §3 Cron 事件触发 + 消费完整链路 + 立即触发兜底

```
【时间到达 04:00:00 UTC+8】

1. CronTriggerProducer poll 循环（每分钟一次）：
   04:00:30 时 poll() 执行
   → now_ts = 1789920030
   → CronTriggerDao::list_due_triggers(ctx, now=1789920030, 100)
      SELECT * FROM cron_triggers
      WHERE enabled = 1 AND next_fire_at <= 1789920030
      → 返回 1 行：id="trigger_agent_rest", kind="agent_rest",
                    old_last_fired_at=1789833600(昨天04点)
   → 对每条 due trigger：
      a) calc next_fire_at：cron="0 4 * * *".upcoming(now).next() = 1790006400(明天04点)
      b) CAS UPDATE：
         UPDATE cron_triggers
         SET last_fired_at = 1789920030, next_fire_at = 1790006400
         WHERE id = 'trigger_agent_rest' AND last_fired_at = 1789833600
         → RETURNING id;  -- 多实例部署时，只有第一个执行的实例拿到 1 行，其他 0 行
      c) UPDATE 成功（影响 1 行）→ aop::publish(SchedulerTriggerFiredEvent {
           trigger_id, kind: "agent_rest", payload: {}
         })
      d) UPDATE 失败（影响 0 行）→ 跳过（其他实例已触发），syslog 记录重复

2. AOP 广播事件 → SchedulerConsumer（注册顺序在 consumer::init）：
   sync consume(SchedulerTriggerFiredEvent) → match kind {
     "agent_rest" => HR::agent_rest_all(ctx).await：
       → SELECT id FROM agents WHERE status != Draft
       → 每个 agent 顺序：
           HR::agent_rest(ctx, agent_id).await：
             1) RuntimeInfo set_state(Resting)
             2) settle_work_memory(ctx, agent_id) → Working 摘要入 ShortTerm
             3) settle_short_term(ctx, agent_id, before=7d) → 合并入 LongTerm
             4) RuntimeInfo set_state(Idle)（Resting 期间收到的 pending_messages 下次唤醒时一起消费）
       → 完成 → ACK；任一 agent 失败 → 不中断其他，写日志 warn；整体 ack
     "stats_collect" => RuntimeStatsCollector::flush_all(ctx).await：
       → 内存各滑动窗口（AOP 队列延迟、Agent 响应 P95、ToolCall 成功率）
       → DuckDB record_event! 批量插入 5 维度表
       → ack
     "backup_daily" => Finance::Backup.create(ctx) → 全量 SQLite 备份到 data_dir/backups/
     custom_kind(kind) => dispatch_custom_handler(ctx, kind, payload)
   }

3. 【立即触发兜底按钮】：管理员在 System Triggers 页面点「立即触发」
   → POST /api/v1/system/cron/triggers/:id/fire
   → Handler 跳过检查 next_fire_at <= now，直接手动构造 SchedulerTriggerFiredEvent publish
   → 同样走 SchedulerConsumer 链路；页面 3 秒后自动刷新看 delivery_attempts 表状态
   （用于 debug agent_rest 不等到凌晨 4 点就能跑）
```

---

## §4 硬约束与回归红线（8 条）

1. **启动顺序必须严格保持：init_base_data → aop::init_all（cron producer 真正启动）**：lib.rs 代码写注释「DO NOT REORDER: init_base_data() 前 aop 启动会导致系统默认 cron 首次 poll 不到」；调整顺序 = fail；集成测试 system_cron_triggers_test 在 init_full_test_env 之后 COUNT 默认 cron=2 断言这条规则。
2. **CronTriggerProducer CAS UPDATE 必须用 RETURNING id 判断成功，不能相信 UPDATE 返回的 Ok()**：UPDATE 0 行返回 Ok(0 rows) 但 Ok(()) 还是成功；必须 match sqlx::query_scalar::<Option<i64>>...fetch_one() → Some(_) 才 publish，None/Err 不 publish；否则两实例同时跑时每条 due 会触发两次（Agent 重复 settle 两次记忆）。
3. **用户自定义 cron 最小间隔不得小于 1 分钟**：create_cron_trigger Handler 校验 `cron 表达式 upcoming 两次相邻 next_ts 差 < 60s → 400「Cron 过于频繁，最小间隔 1 分钟」`；拒绝 `*/1 * * * *` 每分钟、`* * * * *` 每秒（实际上 cron 不支持秒，但 parse 仍允许 `* * * * *` 语义每分钟也 OK，真正拒绝的是 Handler 算出来差 < 60）。
4. **agent_rest 期间 AgentRuntimeInfo.status = Resting，AgentLoopConsumer 不得唤醒**：AgentLoopConsumer 里 BusyGuard.try_acquire(state) 匹配 Resting → 不 publish(AgentWakeEvent)，而是把用户消息 append 到 pending_messages；agent_rest_all 的最后一步 set_state(Idle)，否则 Agent 永远沉睡无人唤醒；Resting 超过 6 小时 system cron 看门狗自动强制回 Idle 防异常卡死。
5. **stats_collect 必须 flush 成功才算 ack（内存数据不能丢）**：SchedulerConsumer consume stats_collect 时 RuntimeStatsCollector.flush(ctx) 失败（DuckDB 写入失败）→ 返回 Nack；AOP Registry 会 retry（指数退避），直到 3 次失败后入库死信队列；不能 Ack + 丢数据，否则 5 分钟窗口的监控数据永远缺失。
6. **系统默认 cron_triggers（agent_rest/stats_collect）在 Handler 中必须标记 readonly，禁止 DELETE**：DELETE /api/v1/system/cron/triggers/trigger_agent_rest → 403「系统默认触发器不可删除」；用户可临时 disabled=true（前端开关），不允许删除；disabled 时下次 poll 时 WHERE enabled=1 自动跳过。
7. **SchedulerConsumer 必须是 sync 消费模式（不并发），禁止并发消费同 kind**：多个 SchedulerTriggerFiredEvent 同时到达时，顺序串行消费；否则 agent_rest 并发导致两个任务同时 settle 同一个 Agent → LongTerm 去重失败重复节点；用 AOP 的 ConsumeMode::Sync（不是 Concurrent(4)），消费顺序即 register 顺序。
8. **立即触发接口不能修改 next_fire_at 和 last_fired_at**：管理员手动立即触发后不 UPDATE last_fired_at/next_fire_at，保持原定时节奏；例：04:00 正常 agent_rest，14:23 手动立即触发一次 → 明天 04:00 仍然正常触发，不会推迟到 14:23+24h；触发记录单独写到 system_manual_fire_logs 表审计，不影响 cron 主表。
