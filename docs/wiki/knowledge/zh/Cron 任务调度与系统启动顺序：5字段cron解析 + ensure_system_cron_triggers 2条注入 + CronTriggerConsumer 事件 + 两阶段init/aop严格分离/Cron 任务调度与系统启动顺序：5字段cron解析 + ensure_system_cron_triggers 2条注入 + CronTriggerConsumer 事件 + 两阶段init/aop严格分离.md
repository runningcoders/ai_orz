---
kind: RAG 原子知识卡
name: Cron 任务调度与系统启动顺序：5 字段 cron 解析 + ensure_system_cron_triggers 2 条注入 + CronTriggerProducer
  60s 轮询 + 两阶段 init/init_base_data/aop 严格分离
category: 基础设施 / 调度与启动
scope:
- src/service/dao/cron_trigger/**
- src/service/dal/cron_trigger.rs
- src/service/domain/system/mod.rs
- src/producer/cron_trigger.rs
- src/consumer/scheduler.rs
- src/handlers/system/cron_trigger/**
- src/models/events/cron_trigger.rs
- src/lib.rs (启动总顺序)
source_files:
- src/service/dao/cron_trigger/mod.rs#L1-L80 (CronTriggerDao trait：CRUD + list_due_triggers(now_ts,
  limit) WHERE next_fire_at<=now + enabled；5 字段 cron 表达式 minute hour day-of-month
  month day-of-week → 计算下一次 fire 时间戳)
- src/service/dao/cron_trigger/sqlite.rs#L1-L120 (SQLite impl：cron_parse crate 解析
  5 字段表达式 → `iterator.upcoming(Utc).next()` 算 next_fire_at；list_due_triggers 按 last_fired_at
  + next_fire_at 双字段锁乐观并发)
- src/service/domain/system/mod.rs#L43-L90 (SystemDomain::init_base_data → ensure_system_cron_triggers：先查
  cron_triggers WHERE kind IN("agent_rest", "stats_collect") COUNT → 0 则 INSERT 两条系统默认：①
  agent_rest cron="0 4 * * *" 每天 4 点 ② stats_collect cron="*/5 * * * *" 每 5 分钟收集统计)
- src/producer/cron_trigger.rs (CronTriggerProducer impl Producer：ProducerLoop 每 60s 自管循环
  （取代已删除的 poll_interval_secs + poll()）；tick(sink) 内 system().cron_manager().list_due(ctx, now, 100)
  → 每条 due → sink.emit(CronTriggerEvent{trigger_id, trigger_name, payload, created_at})；
  marker：mark_trigger_executed 已移入 on_consumed，executed_at 取事件 created_at)
- src/consumer/scheduler.rs#L1-L120 (CronTriggerConsumer Sync 消费 CronTriggerEvent：subscriptions() 声明 EventTopic::CronTrigger + notify_producer()；身份分层中继——查询项目归属 root_user_id，非空则 from_id=root_user_id/from_role=User，否则万不得已 from_role=System；match kind：agent_rest → HR::agent_rest_all(ctx) 遍历所有在线 Agent 做 settle 沉淀；stats_collect → DuckDB record_event 汇总 + RuntimeStatsCollector flush；backup 每日 → Finance::Backup.create；发送前调用 enrich_org_from_project_user 补齐组织上下文)
- src/consumer/mod.rs (enrich_org_from_project_user helper：当系统触发链路 ctx 无 organization_id 绑定时，根据 root_user_id 查 UserPo.organization_id 回退补齐；CronTriggerConsumer + TaskEventConsumer 在中继消息前必须调用)
- src/consumer/task_event_consumer.rs#L1-L100 (TaskEventConsumer：巡检/调度完成后向用户投递结论通知；同样按项目归属身份分层中继，非空 root_user_id 走 User 身份保证 Agent Final 能回到用户消息链)
- src/lib.rs#L20-L70 (启动总顺序强制执行：pkg::init_all → service::init → producer::init → consumer::init
  → service::init_base_data().await【2 条系统 cron 注入于此】→ aop stats hook → aop::init_all()【AOP
  调度器启动，cron producer 开始 poll】→ HTTP 启动；红线：**init_base_data 绝对不能放 consumer::init /
  producer::init 之前**，否则 poll 时系统 cron 还没注入就漏掉)
- src/handlers/system/cron_trigger/list_cron_triggers.rs (Handler：管理员 list_cron_triggers，支持按
  kind/status 过滤，分页；用户创建自定义 cron POST create_cron_trigger 需 cron 表达式格式校验 + 不允许创建 */1
  * * * * < 1 分钟过于频繁的任务（防止队列爆）)
- docs/archive/design-archive/task_scheduler_design.md
- docs/design/runtime_design.md
- docs/archive/design-archive/event_design.md
- docs/archive/design-archive/consumer_architecture.md
- docs/archive/plan-archive/唤醒上下文与睡眠约束.md
- docs/archive/plan-archive/统计图表Phase1基础设施与时序图展示重构.md
- docs/archive/plan-archive/通用后台任务模块与Seed异步化重构.md
- docs/wiki/zh/content/项目概述/核心功能特性/系统管理功能/定时任务管理.md
- docs/wiki/zh/content/功能模块/系统管理/定时任务调度.md
- docs/wiki/zh/content/核心模块/AOP 事件系统/消费者框架/定时任务消费者.md
- 【平行卡 1】docs/wiki/knowledge/zh/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry
  全局单例 + 8 类业务消费者注册/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8
  类业务消费者注册.md
- 【平行卡 2】docs/wiki/knowledge/zh/Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest
  每天 4 点 settle + load_and_settle 向量去重合并/Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+
  agent_rest 每天 4 点 settle + load_and_settle 向量去重合并.md

---

## §1 概述

**本卡角色**：Cron 5 字段调度 + 系统启动两阶段严格分离 + 2 条默认系统 cron 注入的知识卡。覆盖 CronTriggerDao（cron 解析 + list_due 乐观锁）、init_base_data 的 ensure_system_cron_triggers（每天4点 agent_rest + 每5分钟 stats_collect）、CronTriggerProducer 60s 自管循环→ emit 事件→ CronTriggerConsumer 消费链路、启动总顺序（init_base_data 必须在 aop::init_all 之前完成，否则首次 poll 漏掉系统默认）。**定位：新增自定义 cron kind、排查 agent_rest 没跑导致记忆不沉淀、系统启动后 stats_collect 立即触发因为顺序错、写定时任务忘记配置幂等时读。**

- **5 字段 cron 表达式 + 轮询模式（不用实时调度线程）**：cron 格式 `minute hour dom month dow`（标准 Unix cron），不支持秒级（粒度 1 分钟足够业务：agent_rest 天级、stats 5 分钟、备份日级）。实现方式不是 cron 库 spawn 一堆 timer（Agent 多了 OS 线程爆炸），而是 **`CronTriggerProducer` 以 `ProducerLoop` 每 60s 自管循环扫一次**（已删除 `poll_interval_secs` / `poll()`）：① 取 now_ts 当前时间戳；② SELECT * FROM cron_triggers WHERE enabled=1 AND next_fire_at <= now_ts LIMIT 100（取到期触发器）；③ 每条 due trigger 计算 next_fire_at = 5字段cron.upcoming(now).next()；④ UPDATE cron_triggers SET last_fired_at=now_ts, next_fire_at=next_ts WHERE id=? AND last_fired_at=<old_value>（CAS 乐观并发，多实例部署时只有一个能 UPDATE 成功，避免重复触发）；⑤ 成功则经 `EventSink` emit `CronTriggerEvent`（kind = `EventTopic::CronTrigger`）走 AOP。
- **两条系统默认 cron + init_base_data 幂等注入**（system/mod.rs ensure_system_cron_triggers）：① **agent_rest（每天 04:00）** cron="0 4 * * *"：低峰期遍历所有 status != Draft 的 Agent，逐个调用 HR::agent_rest → Working Memory 摘要写 ShortTerm + ShortTerm 7 天前 merge 入 LongTerm 知识图谱 → AgentRuntimeInfo 状态临时切 Resting（期间收到消息 AgentLoopConsumer 不唤醒，append pending_messages）。② **stats_collect（每 5 分钟）** cron="*/5 * * * *"：RuntimeStatsCollector（内存滑动窗口，例如 Agent 响应延迟 / AOP 队列积压数）flush 到 DuckDB 持久化表（record_event! 批量写）。注入幂等：先 `SELECT COUNT(*) FROM cron_triggers WHERE kind IN ("agent_rest","stats_collect")` → 结果为 2 就不 INSERT（重启不重复创建）。系统默认 kind 在 UI 上标记 readonly 不能被用户 DELETE。
- **启动总顺序 6 步严格分离（AGENTS.md §4.10 强制执行）**（lib.rs run()）：**① pkg::init_all()**（最底层日志/JWT/工具 OnceLock，一次性）→ **② service::init()**（DAO→DAL→Domain 单例注册，纯内存不碰 DB）→ **③ producer::init() / consumer::init()**（AOP 订阅者注册，还没真正开始 poll/consume）→ **④ service::init_base_data().await**（DB IO 异步，ensure_system_cron_triggers 在此执行 2 条默认 INSERT）→ **⑤ aop::init_all()**（AOP 启停编排：`start_all` 先校验再逐个 `producer.start(EventSink)`；`CronTriggerProducer` 用 `ProducerLoop` 自管循环、每 60s 跑一轮，确保 init_base_data 已完成写 DB）→ **⑥ HTTP serve**。**红线（核心）**：把 init_base_data 放到 aop::init_all 之后 = 灾难：首次 aop poll 时 cron_triggers 表还空 → agent_rest/stats_collect 永不触发 → 用户反馈「记忆不沉淀 + 统计图表没数据」，排查成本极高。

**2026-09-15 增量**：CronTriggerConsumer + TaskEventConsumer **触发器身份分层中继**——原来统一 from_role=System，现在改为「按被触达事项的归属选身份」：项目归属用户非空时以 User 身份中继（Agent Final 自然回到用户消息链，巡检/调度结论用户可感知），无归属用户（A2A 项目）万不得已落 System。新增 `consumer::enrich_org_from_project_user(ctx, root_user_id)` 补齐组织上下文（系统触发链路 ctx 无组织绑定时从 UserPo.organization_id 回退补齐），防止 Agent 后续工具调用报「缺少组织上下文」。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| dao/cron_trigger/mod.rs CronTriggerDao Trait | 调度数据访问 | CRUD + list_due_triggers(ctx, now_ts, limit)：WHERE enabled AND next_fire_at <= now；update_trigger_fire(ctx, id, old_last_fired, new_next)：CAS 乐观并发 UPDATE 原子；支持 cron 表达式验证 fn validate_cron(expr) -> bool | `:L1-L80` |
| dao/cron_trigger/sqlite.rs SQLite Impl | SQLite 实现 | 用 `cron::Schedule` 解析 5 字段表达式；`schedule.upcoming(Utc).next()` 拿下次 DateTime；CAS UPDATE：UPDATE ... WHERE last_fired_at = ?1 RETURNING id；返回受影响行数=1 则成功 | `:L1-L120` |
| domain/system/mod.rs ensure_system_cron_triggers | 系统默认幂等注入 | init_base_data() 内 try/warn 包裹：COUNT kind IN (agent_rest, stats_collect) → 缺哪个 INSERT 哪个；失败只 sys_warn 不 panic（失败不影响主流程启动）；不重复创建 | `:L43-L90` |
| producer/cron_trigger.rs CronTriggerProducer | AOP 生产者 | impl Producer（`name`/`topic`/`on_consumed`/`on_failed`/`start(sink)`/`stop`）；用 `ProducerLoop` 自管 60s 循环；`tick(sink)`：list_due → 逐条 `sink.emit(CronTriggerEvent{trigger_id, trigger_name, payload, created_at})`；`mark_trigger_executed` 已移入 `on_consumed`（`executed_at` 取事件 `created_at`），`decide_retry` 恒 `Retry` | 见 cron_trigger.rs |
| consumer/scheduler.rs CronTriggerConsumer | AOP 同步消费者 | impl Consumer，`ConsumeMode::Sync`；`subscriptions()` 声明 `EventTopic::CronTrigger` 并 `.notify_producer()`（**不可**声明 `.ordered()`：Sync 消费者声明会注册期 Err）；`on_event(ctx, CronTriggerEvent)`：① 身份分层中继（查项目 root_user_id，非空 from_role=User，否则 System）；② 发送前调用 enrich_org_from_project_user 补齐组织；③ match kind 分派；④ 自定义 kind 走动态注册表；收尾由 `finish_consumption` 按 topic 反查 `CronTriggerProducer` 回调（旧「Ack/Nack 自动写入 system_delivery_attempts 表」已随 `Consumer::ack/nack` 一并删除） | 见 scheduler.rs |
| consumer/mod.rs enrich_org_from_project_user | 组织上下文补齐 helper | 系统触发链路 ctx 无 organization_id 绑定时，根据 root_user_id 查 UserPo.organization_id 回退写入 ctx；CronTriggerConsumer / TaskEventConsumer 在中继消息前必须调用，否则 Agent 后续工具调用（如 list_messages 按组织过滤）缺 org 报错 | 见 mod.rs |
| consumer/task_event_consumer.rs TaskEventConsumer | 巡检结论投递消费者 | 项目巡检/任务调度完成后向用户投递通知；同样身份分层中继（root_user_id 非空走 User 身份），保证 Agent Final 能自然回到用户消息链 | `:L1-L100` |
| handlers/system/cron_trigger/* | 用户 cron CRUD Handler | list_cron_triggers：分页 + 过滤 status/kind；create_cron_trigger：校验 cron 表达式 + 拒绝 < 1 分钟间隔 + payload JSON schema；系统默认 kind 返回 403 「readonly」 | 见 list/create Handler |
| lib.rs run() 启动总顺序 | 6 步序列 | pkg::init → service::init → producer/consumer::init → service::init_base_data().await → aop::init_all → axum serve；每一步用 comment 标注顺序与 why（防止未来有人手贱调整） | `:L20-L70` |

**章节来源**
- [task_scheduler_design.md:L1-L60](docs/archive/design-archive/task_scheduler_design.md#L1-L60)
- [system/mod.rs:L43-L90](src/service/domain/system/mod.rs#L43-L90)
- [lib.rs run()](src/lib.rs)

---

## §3 Cron 事件触发 + 消费完整链路 + 立即触发兜底

```
【时间到达 04:00:00 UTC+8】

1. CronTriggerProducer 自管循环（ProducerLoop，每 60s 一轮）：
   04:00:30 时 tick(sink) 执行
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
      c) UPDATE 成功（影响 1 行）→ sink.emit(CronTriggerEvent {
           trigger_id, trigger_name, payload
         })（kind = EventTopic::CronTrigger）
      d) UPDATE 失败（影响 0 行）→ 跳过（其他实例已触发），syslog 记录重复

2. AOP 投递事件 → CronTriggerConsumer（注册顺序在 consumer::init）：
   Sync on_event(ctx, CronTriggerEvent) → match kind {
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
   → Handler 跳过检查 next_fire_at <= now，直接手动构造 CronTriggerEvent publish
   → 同样走 CronTriggerConsumer 链路；页面 3 秒后自动刷新看队列 / 事件状态
   （用于 debug agent_rest 不等到凌晨 4 点就能跑）
```

---

## §4 硬约束与回归红线（10 条）

1. **启动顺序必须严格保持：init_base_data → aop::init_all（cron producer 真正启动）**：lib.rs 代码写注释「DO NOT REORDER: init_base_data() 前 aop 启动会导致系统默认 cron 首次 poll 不到」；调整顺序 = fail；集成测试 system_cron_triggers_test 在 init_full_test_env 之后 COUNT 默认 cron=2 断言这条规则。
2. **CronTriggerProducer CAS UPDATE 必须用 RETURNING id 判断成功，不能相信 UPDATE 返回的 Ok()**：UPDATE 0 行返回 Ok(0 rows) 但 Ok(()) 还是成功；必须 match sqlx::query_scalar::<Option<i64>>...fetch_one() → Some(_) 才 publish，None/Err 不 publish；否则两实例同时跑时每条 due 会触发两次（Agent 重复 settle 两次记忆）。
3. **用户自定义 cron 最小间隔不得小于 1 分钟**：create_cron_trigger Handler 校验 `cron 表达式 upcoming 两次相邻 next_ts 差 < 60s → 400「Cron 过于频繁，最小间隔 1 分钟」`；拒绝 `*/1 * * * *` 每分钟、`* * * * *` 每秒（实际上 cron 不支持秒，但 parse 仍允许 `* * * * *` 语义每分钟也 OK，真正拒绝的是 Handler 算出来差 < 60）。
4. **agent_rest 期间 AgentRuntimeInfo.status = Resting，AgentLoopConsumer 不得唤醒**：AgentLoopConsumer 里 BusyGuard.try_acquire(state) 匹配 Resting → 不 publish(AgentWakeEvent)，而是把用户消息 append 到 pending_messages；agent_rest_all 的最后一步 set_state(Idle)，否则 Agent 永远沉睡无人唤醒；Resting 超过 6 小时 system cron 看门狗自动强制回 Idle 防异常卡死。
5. **stats_collect 必须 flush 成功才算 ack（内存数据不能丢）**：CronTriggerConsumer 处理 stats_collect 时 RuntimeStatsCollector.flush(ctx) 失败（DuckDB 写入失败）→ 上报 `Err`（不再自行 ack/nack）；框架**不设 `max_retry`、无死信队列**，重投（Nack）与否由消费者的 `decide_retry` 判定 —— `CronTriggerConsumer::decide_retry` 恒 `Retry`，故会持续重投；不能吞掉失败当成功，否则 5 分钟窗口的监控数据永远缺失。
6. **系统默认 cron_triggers（agent_rest/stats_collect）在 Handler 中必须标记 readonly，禁止 DELETE**：DELETE /api/v1/system/cron/triggers/trigger_agent_rest → 403「系统默认触发器不可删除」；用户可临时 disabled=true（前端开关），不允许删除；disabled 时下次 poll 时 WHERE enabled=1 自动跳过。
7. **CronTriggerConsumer 必须是 sync 消费模式（不并发），禁止并发消费同 kind**：多个 CronTriggerEvent 同时到达时顺序串行消费；否则 agent_rest 并发导致两个任务同时 settle 同一个 Agent → LongTerm 去重失败重复节点；用 AOP 的 `ConsumeMode::Sync`（不是并发 worker），消费顺序即 register 顺序。
8. **立即触发接口不能修改 next_fire_at 和 last_fired_at**：管理员手动立即触发后不 UPDATE last_fired_at/next_fire_at，保持原定时节奏；例：04:00 正常 agent_rest，14:23 手动立即触发一次 → 明天 04:00 仍然正常触发，不会推迟到 14:23+24h；触发记录单独写到 system_manual_fire_logs 表审计，不影响 cron 主表。
9. **Cron 触发器必须按归属中继身份：有 root_user_id 时 from_role=User，禁止统一 from_role=System 导致通知无人可投递**：原 CronTriggerConsumer + TaskEventConsumer 统一 from_id="system"/from_role=System → Agent Final 回复无处投递（System 角色消息无对等回复对象，Final 被丢弃）+ 用户侧收不到巡检/调度结论通知；必须查询项目归属用户，非空则以 User 身份中继，无归属用户（A2A 项目 root_user_id 为空）才万不得已落 System。
10. **enrich_org_from_project_user 必须在发送消息前调用（consumer 新增 helper），系统触发 ctx 无组织绑定的补全是 Agent 工具调用的前置条件**：cron 事件来自纯 AOP 触发链路，ctx 天然无 organization_id 绑定；不补齐 → Agent 后续 list_messages 按组织过滤等工具调用会报「缺少组织上下文」错误；CronTriggerConsumer / TaskEventConsumer 在中继消息给 Agent 前必须调用 consumer::enrich_org_from_project_user(ctx, root_user_id) 从 UserPo.organization_id 回退补齐。
