---
kind: rag_card
name: Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle
  向量去重合并
category: 领域建模
scope:
- src/service/domain/runtime/compaction.rs
- src/service/domain/runtime/memory.rs
- src/service/domain/memory/**
- src/service/dal/memory.rs
- src/service/dao/memory/**
- src/consumer/scheduler.rs
- src/producer/cron_trigger.rs
- src/handlers/hr/agent/settle_memory.rs
- src/models/cron_trigger.rs
- src/pkg/cron/*.rs
- src/service/domain/system/mod.rs
- src/models/memory*.rs
- common/src/enums/memory.rs
source_files:
- src/handlers/hr/agent/settle_memory.rs#L75-L600
- src/service/dal/memory.rs#L578-L652
- src/service/domain/runtime/compaction.rs#L1-L120
- src/service/domain/system/mod.rs#L420-L470
- src/consumer/scheduler.rs#L53-L131
- src/consumer/scheduler.rs（2026-09-12 增量：agent_rest 系统级全局执行，遍历所有活跃组织）
- src/consumer/scheduler.rs（2026-09-16 增量：agent_rest 改为只派发 AgentSettleEvent，不再同步沉淀）
- src/consumer/agent_settle.rs#L1-L150（2026-09-16 增量：沉淀队列消费者，抢占失败即 nack 重排）
- src/models/events/agent_settle.rs#L1-L85（2026-09-16 增量：AgentSettleEvent，order_key = agent_id）
- src/pkg/agent_runtime_state.rs#L353-L382（2026-09-16 增量：try_set_resting 原子抢占）
- tests/integration/agent_settle_queue_test.rs#L1-L210（2026-09-16 增量：Busy 时不丢、消费者冲突重排、跑完释放）
- src/handlers/hr/agent/settle_memory.rs#L75-L600（2026-09-12 增量：支持全局模式 organization_id=None）
- src/handlers/hr/agent/save_short_term_memory.rs#L19-L56
- src/handlers/hr/agent/save_long_term_memory.rs#L21-L108
- src/models/memory.rs#L158-L320
- docs/archive/design-archive/memory_system_enhancement_design.md
- docs/archive/plan-archive/唤醒上下文与睡眠约束.md
- docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/记忆沉淀机制.md
- docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/四层记忆系统.md
- docs/wiki/zh/content/项目概述/核心功能特性/Agent 全生命周期管理/Agent 记忆系统.md
- docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/工作记忆 (Working Memory).md

---

# §1 概述（一句话定位 + 解决什么问题）

**定位**：记忆系统三阶段增强闭环——① 写入接口拆分（save_short_term / save_long_term 两个专用神经工具替代宽泛的 create_memory）；② SystemDomain CronManager 定时框架建设（ensure_system_cron_triggers 注入 agent_rest 每天 04:00 cron="0 4 * * *"）；③ 休息沉淀完整链路（load_and_settle 查询 Active 短期记忆 → ThinkingScene=Settle 双层工具过滤 → LLM 总结归纳 → 向量搜索相似节点冲突检测 → 命中则合并关系/未命中则新建节点 → 标记短期记忆 Settled），全链路对齐人类认知（工作累了小憩 + 每晚睡觉整理记忆）。

**解决三类存量缺口**（对应 Design §1.1）：
1. **写入接口宽泛**：`create_memory` 单接口既写短期又写长期还带关系，Agent 用错参数概率高；拆分为两个极简参数专用工具
2. **缺自动沉淀**：短期记忆 Active 状态滚雪球，不自动消化为长期知识图谱，检索质量随时间指数下降
3. **沉淀并发冲突**：沉淀中被重复唤醒导致的状态错乱，通过 BusyGuard RAII + Resting 状态 + current_message_id 占用三重防护

**agent_rest 沉淀队列化修复（2026-09-16）**：修复「项目巡检跑了、睡眠沉淀没跑，且触发器显示最近执行时间 = 启动时刻」的静默丢一天问题。根因是链路不对称：消息链路忙时 `Err(conflict)` → 队列 nack 重投（能排队），而 `agent_rest` 在 Sync 消费者里直接同步调 `load_and_settle`，Agent 忙就 `Ok(0)` 跳过；紧接着 `CronTriggerProducer` 无条件 `mark_trigger_executed`，把日触发的 `next_run_at` 推到次日 04:00 —— 一次跳过等于丢一天的待沉淀量。触发条件很常见：启动补偿会把「每小时项目巡检」和「每日沉淀」排进同一轮 poll，巡检先给同一个 Owner Agent 发消息把它占成 Busy。修复后 `agent_rest` **只派发** `agent.settle.requested`（`consumer/agent_settle.rs` 异步消费、`concurrency = 2`），消费者用 `try_set_resting` 原子抢占，抢不到就抛冲突 → 队列退避重投，等 Agent 空闲自动补跑；同时沉淀不再占用 cron 轮询线程（旧实现实测一场沉淀阻塞轮询 7 分钟）。

**agent_rest 系统级全局执行修复（2026-09-12，Ref e9a93979）**：修复 agent_rest 定时沉淀「从未成功」的历史 bug——旧版 `CronTriggerConsumer` 的 `handle_agent_rest` 方法只对特定 `organization_id` 执行沉淀（trigger payload 里硬编码 org_id），导致多组织部署时大部分组织的沉淀永远不触发。重构后：① `consumer/scheduler.rs` 的 handle_agent_rest 新增**全局模式**（payload 无 org_id 或 org_id 为 None 时），遍历 DB 中所有活跃组织逐一执行沉淀；② `settle_memory.rs` 的 `load_and_settle` 支持 organization_id=None 的全局模式；③ 向后兼容——旧 trigger payload 带 org_id 时仍走组织级路径，新系统级 trigger 不带 org_id 时走全局遍历。这是一个 **fix 级别改动**：生产环境 agent_rest cron 一直在跑，但因为硬编码 org_id 导致 90% 的沉淀请求被跳过，用户感知不到但日志里会有大量 warn。
> ⚠️ **该「按组织遍历」实现已被 2026-09-16 取代**：`handle_agent_rest` 现在不再遍历组织（`load_and_settle` 也已无 `organization_id` 参数），改为直接扫描全库 `Active` 短期记忆、按 `agent_id` 去重出目标列表后逐个派发 —— 见上文「agent_rest 沉淀队列化修复」。本段仅保留为历史脉络，**红线以 §4 第 18/19/20 条为准**。

---

# §2 关键文件与核心锚点速查表

| 文件锚点（点击跳转） | 角色 | 核心契约 / 红线 |
|---------------------|------|-----------------|
| [Handler settle_agent_exclusive 定时沉淀入口](src/handlers/hr/agent/settle_memory.rs#L232-L262) | 定时触发链路入口（抢占式） | `try_set_resting` 抢占 → 失败回 `SettleAttempt::Busy`（调用方重排）→ 成功则挂 BusyGuard 兜底释放 → 汇入 `settle_body` |
| [Handler load_and_settle 神经工具入口](src/handlers/hr/agent/settle_memory.rs#L204-L231) | 神经工具 `settle_memory` 入口 | 保留「Agent 忙则 return 0」预检查（该路径上 Agent 天然 Busy，抢占式判定会永远失败）→ 汇入 `settle_body` |
| [AgentSettleConsumer 沉淀队列消费者](src/consumer/agent_settle.rs#L1-L150) | Async 消费 agent.settle.requested | `order_key=agent_id` 同 Agent 串行；忙 → `Err(conflict)` → 队列 nack 重投（30s 退避）；`resource_not_found` → ack 作废；`concurrency=2` |
| [AgentSettleEvent 事件定义](src/models/events/agent_settle.rs#L1-L85) | 沉淀排队单元 | kind=`agent.settle.requested`；**order_key 必须是 agent_id**（与 message.created 对 Agent 接收者同源，保证沉淀不与同 Agent 消息并发） |
| [try_set_resting 原子抢占](src/pkg/agent_runtime_state.rs#L353-L382) | 状态机保护 | Idle → Resting 返回 true；Busy/Resting → false 且不改状态；与 `try_set_busy` 同构，禁止拆成「先查询、后设状态」 |
| [DAL settle_short_term_to_long_term 核心沉淀](src/service/dal/memory.rs#L578-L652) | 短期 → 长期核心算法 | ① 向量搜索相似节点（冲突检测）→ ② 命中：更新已有节点 + 合并关系（去重）→ ③ 未命中：新建节点 + 关系 → ④ 更新短期记忆 status=Settled |
| [Runtime awaken_for_summary 总结退出](src/service/domain/runtime/summary.rs#L36-L80) | 轮次超限或任务完成的总结 | 与 Settle 场景工具白名单对齐（neural+memory+messaging+project_management）；结构化 Final 输出 + 标记退出原因 |
| [SystemDomain ensure_system_cron_triggers](src/service/domain/system/mod.rs#L415-L472) | 系统启动基础数据注入 | 幂等按 payload.contains("\"agent_rest\"") 检查；注入两条系统触发器：agent_rest Cron 每天 04:00 + project_followup 每 3600 秒间隔 |
| [CronTriggerProducer 每分钟扫描](src/producer/cron_trigger.rs#L38-L87) | 到期触发器扫描 | 每分钟 tick：list_due(now, max=20) → 发布 AOP 事件 cron.trigger；失败只打 warn 不 panic |
| [CronTriggerConsumer 调度分发](src/consumer/scheduler.rs#L39-L200) | Cron 事件消费路由 | 同步消费：payload JSON 解析 action → match agent_rest/project_followup；**agent_rest 只解析目标 Agent 并 publish AgentSettleEvent 后立即返回，不在本线程沉淀**（Sync 消费者跑在 cron poll 线程，长任务会堵住全部定时器） |
| [系统定时任务集成测试](tests/integration/system_cron_triggers_test.rs) | 启动注入回归防护 | init_full_test_env 后断言两条触发器存在；防止未来有人误删 ensure_system_cron_triggers 代码 |
| [Cron 表达式与系统时区](src/pkg/cron/mod.rs#L1-L60) | next_run_at 计算 | chrono-crate 解析 cron 表达式；system_timezone 获取；容器部署须挂载 /etc/timezone 或 TZ 环境变量 |
| [save_short_term_memory 神经工具](src/handlers/hr/agent/save_short_term_memory.rs#L19-L56) | 专用神经工具（参数极简） | 仅 summary/tags/task_id 三参数；注册 neural tag；Agent 唤醒工具列表含此项；create_memory 不注入 |
| [save_long_term_memory 神经工具](src/handlers/hr/agent/save_long_term_memory.rs#L21-L108) | 专用神经工具（参数极简） | node + relations 一并创建；不存在的 target 跳过 warn；relations 上限保护 |
| [四层记忆 PO 定义](src/models/memory.rs#L158-L320) | PO 实体 SSOT | ShortTermMemoryIndexPo（summary/tags/trace_ids/status）；LongTermKnowledgeNodePo（node_name/description/summary/tags/is_published）；MemoryStatus Forgotten(0)/Active(1)/Settled(2) |
| [记忆系统增强 Design 三阶段](docs/archive/design-archive/memory_system_enhancement_design.md) | 为什么 / 6 条决策 | §决策 1：写入拆分；§决策 3：SystemDomain Cron；§决策 4：休息双轨；§决策 5：冲突合并策略 |
| [唤醒上下文 Plan 落地快照](docs/archive/plan-archive/唤醒上下文与睡眠约束.md) | 怎么做 + 结果 | §双层工具过滤机制 §ThinkingOptions 统一参数 §build_sleep_prompt 内聚约束 |
| [记忆沉淀机制 Wiki 长文](docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/记忆沉淀机制.md) | 人类百科 | §5 沉淀调度完整链路 §8 故障排查 |

---

# §3 架构约定与数据流（业务语义层面，不贴实现代码）

**端到端沉淀全链路**：
```
【启动期】两阶段初始化 init_base_data 阶段
  → SystemDomain.ensure_system_cron_triggers(ctx).await
     幂等：SELECT * FROM cron_triggers WHERE payload LIKE '%agent_rest%'
     不存在 → INSERT cron_triggers
       cron="0 4 * * *"（每天凌晨 4 点）
       payload={ action:"agent_rest", extra:{settle_limit:10} }   ← 与外层 extra 是嵌套关系，勿扁平化/勿 flatten
       is_enabled=1
       next_run_at = 计算下次 04:00

【调度期】每分钟 CronSchedulerProducer 扫描
  → list_due(next_run_at<=now AND is_enabled=1)
  → 发布 cron.trigger 事件
  → CronTriggerConsumer 同步消费
     payload.action="agent_rest"
       → 解析目标 Agent（payload.agent_id，或扫描 Active 短期记忆去重出 Agent 列表）
       → 每个 Agent publish 一条 agent.settle.requested，立即返回
       → Producer 随后 mark_trigger_executed 推进 next_run_at
         （⚠️ 触发器的「已执行」与沉淀是否真的跑成无关：跳过不再可能，但也不要
           从这里读「沉淀成功了」——看 agent_settle 消费者的日志）

【排队期】AgentSettleConsumer（Async，order_key=agent_id，concurrency=2）
  → try_set_resting 抢占失败 → Err(conflict) → 队列 nack 重投（30s 退避）→ 空闲后自动补跑
  → 抢占成功 → BusyGuard 兜底 + settle_body
  → 进程重启会丢内存队列里的事件：**当天已派发但未跑完的请求不会自动重建**（触发器已
     `mark_trigger_executed`，日触发的下个周期是次日）。残留窗口 = 「派发后、沉淀跑完前」
     进程被杀；比旧实现（忙就跳过一次丢一天）窄得多，当前按 YAGNI 未加启动期补扫
     —— 若要严格保证，可在启动时对「仍有 Active 短期记忆的 Agent」补发一轮沉淀请求

【执行期】settle_body 主流程
  Step 1：短期记忆装载
     → MemoryDal.query_short_term(agent_id, status=Active, limit=settle_limit)
     → 为空 → return 0，正常空跑（不是失败，不重试）
     → 非空 → build_pending_memories_summary（仅编号摘要，不含任何约束模板）
  Step 2：状态机保护
     → 定时链路：settle_agent_exclusive 用 try_set_resting 抢占（忙 → 排队重试，绝不静默跳过）
     → 神经工具链路：load_and_settle 入参预检查（Agent 天然 Busy → return 0）
     → BusyGuard RAII 兜底释放
  Step 3：wake_agent_brain(ThinkingScene=Settle)
     → 【双层工具过滤第一层】Auto 工具只保留 neural/memory 标签
     → 装配 Cortex
  Step 4：sleep_and_settle(pending_summary, ThinkingOptions::for_scene(Settle))
     → 【双层工具过滤第二层】Manual 工具 + skill 也只保留 neural/memory
     → PromptBuilder.build_sleep_prompt(pending_summary)
        · 内聚沉淀约束：不发消息、只用记忆工具、内循环语义 3 条红线
        · 6 步任务步骤：归纳→查图谱→创建/更新节点→建关系→评估共享→标记完成
        · 5 条认知要点：图谱是活的、记抽象不记细节、迭代不覆盖、published 跨 Agent 桥接
     → run_think_loop（最多 N 轮 + 超时）
  Step 5：DAL.settle_short_term_to_long_term（LLM 产出后落库）
     → 向量搜索相似节点（memory:knowledge_node 集合，threshold=0.78）
        · 命中 → 更新已有节点的 summary/description/tags
                 合并已有关系（同方向+同类型关系去重不重复创建）
        · 未命中 → INSERT 新节点 + 关系
     → KnowledgeReferencePo 建立：知识节点 → 原始短期记忆 trace_id 的追溯引用
     → 批量 UPDATE short_term_memory_index SET status=Settled WHERE id IN (...)
  Step 6：正常结束 / panic / 任意返回路径
     → BusyGuard drop → Agent 状态恢复 Idle

【人工期】Agent 详情页手动沉淀按钮
  → settle_memory Handler（同 load_and_settle 流程）
```

**休息触发双轨机制**（对齐人类）：
| 触发模式 | 场景 | 状态 | 沉淀深度 | 触发条件 |
|---------|------|------|---------|---------|
| 短暂休息（小憩） | 上下文过载 | Resting(短暂) | 清 Working 内存，不做长期沉淀 | 连续 think loop > N 轮 / Prompt Token 超阈值 |
| 每日睡眠沉淀 | agent_rest cron 04:00 | Resting(睡眠) | 完整短期→长期沉淀 + 去重合并 | 每天定时 + 基础数据注入保证触发器存在 |

---

# §4 硬约束 / 必守红线 / 扩展入口

**§4.1 必守红线（10 条，违反 = FAIL）**

| # | 红线 | 验证方式 | 代码锚点 |
|---|------|---------|---------|
| 1 | **工具拆分红线**：Agent 唤醒时的工具列表**只注入 save_short_term_memory / save_long_term_memory**；`create_memory` 的 neural flag 必须移除（禁止误用，参数复杂易错） | 唤醒工具列表集成测试 grep create_memory 不存在；save_short/long_term 两个存在 | [awakening.rs 工具注册处](src/service/domain/runtime/awakening.rs) + save_short_term_memory.rs `register_handler_tool` |
| 2 | **search_memory 向后兼容**：`traversal_depth` 默认 0（不遍历），不传 traversal 参数的旧调用方行为 100% 等价改造前；禁止默认遍历导致性能回退 | 旧集成测试全量通过（无 traversal 参数） | [handlers/search_memory.rs 参数默认值](src/handlers/hr/agent/search_memory.rs) |
| 3 | **ensure_system_cron_triggers 幂等**：重复启动第二次不产生重复 cron_triggers 行；LIKE '%agent_rest%' 判定必须与 CronConsumer payload.action 解析一致 | 连续两次调用后 SELECT COUNT(*) = 1 | [system/mod.rs#L420-L440](src/service/domain/system/mod.rs#L420-L440) |
| 4 | **CronScheduler 后台线程不阻塞启动**：扫描失败仅 log_warn!，禁止 panic 影响主 HTTP 服务；并发触发器限制默认 4，配置可调 | scheduler 初始化 catch_unwind + 并发数限制常量 grep | [producer/cron_trigger.rs](src/producer/cron_trigger.rs) 主循环 |
| 5 | **沉淀状态机防护**：Resting 期间新消息**排队不丢失**（MessageConsumer try_set_busy 失败 → 延迟重试/重新入队），绝不拒绝消息 | Resting 状态并发消息集成测试：沉淀完成后消息全部被处理 0 丢失 | [consumer/message.rs try_set_busy 失败分支](src/consumer/message.rs) |
| 6 | **沉淀幂等性**：同一天重复触发「每日睡眠」第二次，向量冲突检测必须命中合并 → 不产生重复知识节点；短期记忆已是 Settled 跳过 | 连续两次 load_and_settle 断言第二次 settled_count=0 且 知识节点数不增长 | [dal/memory.rs settle 冲突检测 merge 分支](src/service/dal/memory.rs#L590-L620) |
| 7 | **双层工具过滤红线**：Settle 场景下 ① wake_agent_brain 的 Auto 工具 ② sleep_and_settle 的 Manual + skill，**两层都只保留 neural/memory**；禁止沉淀模式下 Agent 能调 send_message 导致循环唤醒自己 | grep is_tool_allowed Settle 场景仅 neural |memory：两层过滤代码 | [awakening.rs 双层过滤匹配](src/service/domain/runtime/awakening.rs) |
| 8 | **沉淀 Prompt 约束硬编码写入**：build_sleep_prompt 必须包含 3 条红线（不发消息 / 只用记忆工具 / 内循环）；禁止 handler 层 format! 传入完整 Prompt，必须由 builder 内聚生成 | grep settle_memory.rs format! 约束模板行数应为 0；builder.build_sleep_prompt 内 grep "不要发送消息" 命中 | [dal/agent.rs build_sleep_prompt](src/service/dal/agent/mod.rs) |
| 9 | **向量冲突阈值**：相似节点判定阈值硬编码 0.78（向量距离 ≤ 0.78 视为语义相同）；命中时 UPDATE 已有节点 + 合并关系，禁止直接 INSERT 造重复节点 | 两条语义相似的短期记忆两次 settle 断言最终 1 个知识节点 | [dal/memory.rs settle 冲突检测 vector_distance 阈值](src/service/dal/memory.rs#L590-L600) |
| 10 | **Sleep_and_settle 参数语义**：pending_memories_summary 参数**仅为编号摘要**字符串（编号列表格式）；完整约束模板由 builder.build_sleep_prompt() 内聚，handler 层禁止传入完整 Prompt 字符串 | settle_memory.rs build_pending_memories_summary 返回字符串中应无"不要" "禁止"等约束词汇 | [settle_memory.rs build_pending_memories_summary](src/handlers/hr/agent/settle_memory.rs#L50-L74) |
| 11 | **settle 永不硬删除短期记忆**：短期记忆消化后 status=Settled 软标记保留追溯链；物理 DELETE 破坏审计链，禁止 | 连续 settle 两次 grep DELETE FROM short_term_memory_index 次数应为 0 | [dal/memory.rs settle 完成后 UPDATE status 分支](src/service/dal/memory.rs#L630-L648) |
| 12 | **合并已有节点必须 MERGE 关系**：命中相似节点后更新 summary/description/tags，同时去重合并出入边；绝不能丢弃旧节点 relations 导致图谱断链 | 两次语义相似 settle，断言知识图谱关系最终是 MERGE 后的并集 | [dal/memory.rs settle 冲突检测 merge 分支](src/service/dal/memory.rs#L600-L630) |
| 13 | **CronTriggerConsumer handle_agent_rest 永不 panic**：payload 缺字段、Agent 不存在、LLM 异常，全部 catch_unwind 打 warn 跳过；否则整条 Cron 消费者线程崩了，所有触发器都不再执行 | 故意构造空 payload 启动 consumer → 应只打 warn，不影响后续 project_followup 触发 | [consumer/scheduler.rs#L100-L127](src/consumer/scheduler.rs#L100-L127) handle_agent_rest 异常处理 |
| 14 | **ensure_system_cron_triggers 绝不做 UPDATE**：系统重启只创建不存在的触发器；用户手工改 cron_expression（4点→5点）应保留，不能覆盖用户设置 | 用户改 cron_expression 为"0 0 5 * * *"→ 重启后 SELECT cron_expression 仍是 5 点 | [system/mod.rs ensure_system_cron_triggers](src/service/domain/system/mod.rs#L415-L472) 仅 INSERT 无 UPDATE |
| 15 | **next_run_at 计算失败必须置 is_enabled=0**：cron 表达式非法或时区异常 → 触发器禁用 + 打 sys_error；否则该触发器永远占 max_events=20 的坑，其他正常触发器跑不起来 | 故意把 cron_expression 改成"* * * *" → 启动后该触发器 is_enabled=0 | [pkg/cron/mod.rs](src/pkg/cron/mod.rs#L1-L60) next_run_at 异常处理分支 |
| 16 | **Producer 顺序是「先 publish、再 mark_trigger_executed」**（现状，勿颠倒）：`CronTriggerProducer::poll` 先 `publish(&ctx, event).await`（Sync 消费者在这里同步跑完）再推进 `next_run_at`。**含义**：业务是否真的执行成功，与 `next_run_at` 是否推进**无关** —— 消费者内部吞掉的失败（旧版 `Ok(0)` 跳过、或只打日志的 `agent_rest` 分支）都会被当成「已执行」，触发器界面照样显示「已执行」。所以「忙就跳过」必然静默丢一个周期，必须靠队列重投 / 消费者侧重试来补，不能指望触发器重来 | 读 `producer/cron_trigger.rs poll()` 的调用顺序；`tests/integration/agent_settle_queue_test.rs` 锁定「Agent 忙也必须入队」 | [cron_trigger.rs](src/producer/cron_trigger.rs) Producer 主循环 |
| 17 | **沉淀期间 Agent 收到唤醒请求需排队或 429**：Resting 态 Handler 层做 429 兜底，与 BusyGuard 语义双重保证，禁止 Awake 和 Sleep 同时进入 | Resting 态下 awaken 请求应返回 429 「Agent 正休息」，或 MessageConsumer 重新入队不丢 | [consumer/message.rs](src/consumer/message.rs) try_set_busy 失败分支 |
| 18 | **handle_agent_rest 的目标 Agent 解析**（2026-09-16 现状）：payload 带 `agent_id` → 只派发该 Agent；缺省 → 扫描全库 `MemoryStatus::Active` 短期记忆、按 `short_term_of(m).agent_id` 去重出 Agent 列表（上限 1000，防触达时下周期继续）。**禁止**在任何分支里「解析不出目标就静默 return 跳过」——那正是「沉淀从未成功 / 丢一天」的表现形式；解析不出应是**空列表**（真的无人待沉淀），而不是错误 | `tests/integration/agent_settle_queue_test.rs::test_agent_rest_dispatches_settle_request_when_agent_busy`（指定 agent_id 必须精确入队） | [consumer/scheduler.rs handle_agent_rest](src/consumer/scheduler.rs) |
| 19 | **`CronTriggerPayload.extra` 必须具名字段，禁止 `#[serde(flatten)]`**（2026-09-16 新增）：真实 payload 形态是**带 `extra` 键的嵌套对象** —— 系统默认 seed 与「定时任务 API」文档都是 `{"action":"agent_rest","extra":{"settle_limit":10}}`。`flatten` 会把 `extra` 这个键名本身一起收进 `Value`，得到 `{"extra":{"settle_limit":10}}` 再传给动作层；`AgentRestPayload` 无 `deny_unknown_fields`，未知键被丢弃 → `agent_id` / `settle_limit` **全部静默降级为 `None` 且不报错**（后果：指定单 Agent 的沉淀退化成全局扫描、`settle_limit` 永远被忽略）。必须用具名字段 + `#[serde(default)]`，并由 `action_params()` 把缺省/`null` 归一为空对象 | `cargo test --lib consumer::scheduler`：`test_cron_trigger_payload_passes_extra_through` / `_tolerates_missing_extra` / `_ignores_unknown_fields` 三条契约测试；对照 `select payload from cron_triggers` 与解析结果 | [consumer/scheduler.rs CronTriggerPayload](src/consumer/scheduler.rs) |
| 20 | **agent_rest 只派发，绝不在 cron 线程里沉淀**（2026-09-16 新增）：`handle_agent_rest` **禁止**改回同步调用 `load_and_settle`。`CronTriggerConsumer` 是 `ConsumeMode::Sync`，跑在 cron poll 线程里——一场沉淀是 LLM 往返（实测阻塞轮询 7 分钟），在此期间项目巡检 / 工具日志清理 / 目录对账全部干等；且 Agent 忙时同步路径只能「跳过」，而 `CronTriggerProducer` 随后无条件 `mark_trigger_executed` 把 `next_run_at` 推到下个 cron 点（日触发 = 次日）→ **一次跳过丢一天**。派发方也**不得**改动 Agent 运行状态：状态唯一写入方是消费者侧 `settle_agent_exclusive` 的抢占 | `tests/integration/agent_settle_queue_test.rs` 三条用例（Busy 时请求必须入队且派发不碰状态 / 消费者冲突上报 / 跑完回到 Idle） | [consumer/scheduler.rs](src/consumer/scheduler.rs) + [consumer/agent_settle.rs](src/consumer/agent_settle.rs) |

**§4.2 扩展入口速查**

| 扩展需求 | 改动位置（N 处同步） | 参考锚点 |
|---------|---------------------|---------|
| 新增第三类休息触发条件（连续失败>N 次复盘 / Token 超阈值小憩） | ① awakening.rs think_loop 每次迭代结束处追加触发条件检查 → ② 设置 Resting 状态 + 调用 rest_and_digest(ctx, RestReason::ConsecutiveFailures(n)) → ③ 沉淀逻辑复用现有 settle_short_term_to_long_term（零改动） | [runtime/awakening.rs think_loop 退出检查](src/service/domain/runtime/awakening.rs) |
| 沉淀任务步骤追加第 7 步（如「生成关联标签并推荐给 Agent 下次关注」） | DefaultPromptBuilder.build_sleep_prompt 的「你的任务」6 步编号段落末尾追加；同步更新 memory_design.md §认知要点章节保持文档对齐 | [dal/agent.rs build_sleep_prompt §你的任务](src/service/dal/agent/mod.rs) |
| 新增 Cron 触发器类型（如每周报表导出 / 月度数据归档） | ① consumer/scheduler.rs 追加 `match payload.action { "export_report" => ... }` 分支 → ② 对应 Domain（如 FinanceDomain）新增 export_weekly_report(ctx) 方法 → ③ ensure_system_cron_triggers 中追加 INSERT 语句（cron 表达式按需求） | [consumer/scheduler.rs 分发 match](src/consumer/scheduler.rs#L53-L131) |
| 沉淀策略可配置化（按 Agent 可配置冲突阈值 / 每天沉淀条数 / 是否发布为共享） | ① AgentRuntimeConfig 追加 memory_settle_config JSON 字段 → ② RuntimeAwakening 透传 options 到 sleep_and_settle → ③ DAL settle_short_term_to_long_term 读取 config 覆盖默认阈值 | [models/agent.rs AgentRuntimeConfig 定义](src/models/agent.rs) |
