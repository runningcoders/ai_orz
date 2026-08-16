---
kind: rag_card
name: Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle 向量去重合并
category: 领域建模
scope:
  - "src/service/domain/runtime/summary.rs"
  - "src/service/domain/runtime/memory.rs"
  - "src/service/domain/memory/**"
  - "src/service/dal/memory.rs"
  - "src/service/dao/memory/**"
  - "src/consumer/scheduler.rs"
  - "src/producer/cron_trigger.rs"
  - "src/handlers/hr/agent/settle_memory.rs"
  - "src/models/cron_trigger.rs"
  - "src/pkg/cron/*.rs"
  - "src/service/domain/system/mod.rs"
  - "src/models/memory*.rs"
  - "common/src/enums/memory.rs"
source_files:
  - "src/handlers/hr/agent/settle_memory.rs#L75-L154"
  - "src/service/dal/memory.rs#L578-L652"
  - "src/service/domain/runtime/summary.rs#L36-L80"
  - "src/service/domain/system/mod.rs#L420-L470"
  - "src/consumer/scheduler.rs#L53-L131"
  - "src/handlers/hr/agent/save_short_term_memory.rs#L19-L56"
  - "src/handlers/hr/agent/save_long_term_memory.rs#L21-L108"
  - "src/models/memory.rs#L158-L320"
  - "docs/archive/design-archive/memory_system_enhancement_design.md"
  - "docs/archive/plan-archive/唤醒上下文与睡眠约束.md"
  - "docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/记忆沉淀机制.md"
  - "docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/四层记忆系统.md"
  - "docs/wiki/zh/content/项目概述/核心功能特性/Agent 全生命周期管理/Agent 记忆系统.md"
  - "docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/工作记忆%20(Working%20Memory).md"
---

# §1 概述（一句话定位 + 解决什么问题）

**定位**：记忆系统三阶段增强闭环——① 写入接口拆分（save_short_term / save_long_term 两个专用神经工具替代宽泛的 create_memory）；② SystemDomain CronManager 定时框架建设（ensure_system_cron_triggers 注入 agent_rest 每天 04:00 cron="0 4 * * *"）；③ 休息沉淀完整链路（load_and_settle 查询 Active 短期记忆 → ThinkingScene=Settle 双层工具过滤 → LLM 总结归纳 → 向量搜索相似节点冲突检测 → 命中则合并关系/未命中则新建节点 → 标记短期记忆 Settled），全链路对齐人类认知（工作累了小憩 + 每晚睡觉整理记忆）。

**解决三类存量缺口**（对应 Design §1.1）：
1. **写入接口宽泛**：`create_memory` 单接口既写短期又写长期还带关系，Agent 用错参数概率高；拆分为两个极简参数专用工具
2. **缺自动沉淀**：短期记忆 Active 状态滚雪球，不自动消化为长期知识图谱，检索质量随时间指数下降
3. **沉淀并发冲突**：沉淀中被重复唤醒导致的状态错乱，通过 BusyGuard RAII + Resting 状态 + current_message_id 占用三重防护

---

# §2 关键文件与核心锚点速查表

| 文件锚点（点击跳转） | 角色 | 核心契约 / 红线 |
|---------------------|------|-----------------|
| [Handler load_and_settle 沉淀入口](src/handlers/hr/agent/settle_memory.rs#L75-L154) | HTTP + CronConsumer 双链路入口 | ① 查 Active 短期记忆 limit → ② wake_agent_brain(Settle) → ③ sleep_and_settle(pending_summary, options) → ④ 返回 settled_count；Resting 状态 BusyGuard 防并发 |
| [DAL settle_short_term_to_long_term 核心沉淀](src/service/dal/memory.rs#L578-L652) | 短期 → 长期核心算法 | ① 向量搜索相似节点（冲突检测）→ ② 命中：更新已有节点 + 合并关系（去重）→ ③ 未命中：新建节点 + 关系 → ④ 更新短期记忆 status=Settled |
| [Runtime awaken_for_summary 总结退出](src/service/domain/runtime/summary.rs#L36-L80) | 轮次超限或任务完成的总结 | 与 Settle 场景工具白名单对齐（neural+memory+messaging+project_management）；结构化 Final 输出 + 标记退出原因 |
| [SystemDomain ensure_system_cron_triggers](src/service/domain/system/mod.rs#L415-L472) | 系统启动基础数据注入 | 幂等按 payload.contains("\"agent_rest\"") 检查；注入两条系统触发器：agent_rest Cron 每天 04:00 + project_followup 每 3600 秒间隔 |
| [CronTriggerProducer 每分钟扫描](src/producer/cron_trigger.rs#L38-L87) | 到期触发器扫描 | 每分钟 tick：list_due(now, max=20) → 发布 AOP 事件 cron.trigger；失败只打 warn 不 panic |
| [CronTriggerConsumer 调度分发](src/consumer/scheduler.rs#L39-L127) | Cron 事件消费路由 | 同步消费：payload JSON 解析 action → match agent_rest/project_followup；agent_rest handle 解析 extra.settle_limit → 调 load_and_settle |
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
       payload={ action:"agent_rest", extra:{settle_limit:20} }
       is_enabled=1
       next_run_at = 计算下次 04:00

【调度期】每分钟 CronSchedulerProducer 扫描
  → list_due(next_run_at<=now AND is_enabled=1)
  → 发布 cron.trigger 事件
  → CronTriggerConsumer 同步消费
     payload.action="agent_rest" → load_and_settle(agent_id, settle_limit)

【执行期】load_and_settle 主流程
  Step 1：短期记忆装载
     → MemoryDal.query_short_term(agent_id, status=Active, limit=settle_limit)
     → 为空 → return 0，跳过沉淀
     → 非空 → build_pending_memories_summary（仅编号摘要，不含任何约束模板）
  Step 2：状态机保护
     → AgentRuntimeStateManager.try_set_busy(agent_id, trigger_id) 失败 → 跳过
     → BusyGuard<Resting> RAII 占用
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
| 8 | **沉淀 Prompt 约束硬编码写入**：build_sleep_prompt 必须包含 3 条红线（不发消息 / 只用记忆工具 / 内循环）；禁止 handler 层 format! 传入完整 Prompt，必须由 builder 内聚生成 | grep settle_memory.rs format! 约束模板行数应为 0；builder.build_sleep_prompt 内 grep "不要发送消息" 命中 | [dal/agent.rs build_sleep_prompt](src/service/dal/agent.rs) |
| 9 | **向量冲突阈值**：相似节点判定阈值硬编码 0.78（向量距离 ≤ 0.78 视为语义相同）；命中时 UPDATE 已有节点 + 合并关系，禁止直接 INSERT 造重复节点 | 两条语义相似的短期记忆两次 settle 断言最终 1 个知识节点 | [dal/memory.rs settle 冲突检测 vector_distance 阈值](src/service/dal/memory.rs#L590-L600) |
| 10 | **Sleep_and_settle 参数语义**：pending_memories_summary 参数**仅为编号摘要**字符串（编号列表格式）；完整约束模板由 builder.build_sleep_prompt() 内聚，handler 层禁止传入完整 Prompt 字符串 | settle_memory.rs build_pending_memories_summary 返回字符串中应无"不要" "禁止"等约束词汇 | [settle_memory.rs build_pending_memories_summary](src/handlers/hr/agent/settle_memory.rs#L50-L74) |
| 11 | **settle 永不硬删除短期记忆**：短期记忆消化后 status=Settled 软标记保留追溯链；物理 DELETE 破坏审计链，禁止 | 连续 settle 两次 grep DELETE FROM short_term_memory_index 次数应为 0 | [dal/memory.rs settle 完成后 UPDATE status 分支](src/service/dal/memory.rs#L630-L648) |
| 12 | **合并已有节点必须 MERGE 关系**：命中相似节点后更新 summary/description/tags，同时去重合并出入边；绝不能丢弃旧节点 relations 导致图谱断链 | 两次语义相似 settle，断言知识图谱关系最终是 MERGE 后的并集 | [dal/memory.rs settle 冲突检测 merge 分支](src/service/dal/memory.rs#L600-L630) |
| 13 | **CronTriggerConsumer handle_agent_rest 永不 panic**：payload 缺字段、Agent 不存在、LLM 异常，全部 catch_unwind 打 warn 跳过；否则整条 Cron 消费者线程崩了，所有触发器都不再执行 | 故意构造空 payload 启动 consumer → 应只打 warn，不影响后续 project_followup 触发 | [consumer/scheduler.rs#L100-L127](src/consumer/scheduler.rs#L100-L127) handle_agent_rest 异常处理 |
| 14 | **ensure_system_cron_triggers 绝不做 UPDATE**：系统重启只创建不存在的触发器；用户手工改 cron_expression（4点→5点）应保留，不能覆盖用户设置 | 用户改 cron_expression 为"0 0 5 * * *"→ 重启后 SELECT cron_expression 仍是 5 点 | [system/mod.rs ensure_system_cron_triggers](src/service/domain/system/mod.rs#L415-L472) 仅 INSERT 无 UPDATE |
| 15 | **next_run_at 计算失败必须置 is_enabled=0**：cron 表达式非法或时区异常 → 触发器禁用 + 打 sys_error；否则该触发器永远占 max_events=20 的坑，其他正常触发器跑不起来 | 故意把 cron_expression 改成"* * * *" → 启动后该触发器 is_enabled=0 | [pkg/cron/mod.rs](src/pkg/cron/mod.rs#L1-L60) next_run_at 异常处理分支 |
| 16 | **Cron 执行顺序：先 mark_trigger_executed 再跑业务**：否则 agent_rest 沉淀 > 60s 时，下一轮 Producer 的 list_due 还能扫到同一条 → 两次并发触发同一 Agent 休息 | grep 调用顺序：① mark_executed → ② handle_agent_rest | [cron_trigger.rs](src/producer/cron_trigger.rs#L38-L87) Producer 主循环 |
| 17 | **沉淀期间 Agent 收到唤醒请求需排队或 429**：Resting 态 Handler 层做 429 兜底，与 BusyGuard 语义双重保证，禁止 Awake 和 Sleep 同时进入 | Resting 态下 awaken 请求应返回 429 「Agent 正休息」，或 MessageConsumer 重新入队不丢 | [consumer/message.rs](src/consumer/message.rs) try_set_busy 失败分支 |

**§4.2 扩展入口速查**

| 扩展需求 | 改动位置（N 处同步） | 参考锚点 |
|---------|---------------------|---------|
| 新增第三类休息触发条件（连续失败>N 次复盘 / Token 超阈值小憩） | ① awakening.rs think_loop 每次迭代结束处追加触发条件检查 → ② 设置 Resting 状态 + 调用 rest_and_digest(ctx, RestReason::ConsecutiveFailures(n)) → ③ 沉淀逻辑复用现有 settle_short_term_to_long_term（零改动） | [runtime/awakening.rs think_loop 退出检查](src/service/domain/runtime/awakening.rs) |
| 沉淀任务步骤追加第 7 步（如「生成关联标签并推荐给 Agent 下次关注」） | DefaultPromptBuilder.build_sleep_prompt 的「你的任务」6 步编号段落末尾追加；同步更新 memory_design.md §认知要点章节保持文档对齐 | [dal/agent.rs build_sleep_prompt §你的任务](src/service/dal/agent.rs) |
| 新增 Cron 触发器类型（如每周报表导出 / 月度数据归档） | ① consumer/scheduler.rs 追加 `match payload.action { "export_report" => ... }` 分支 → ② 对应 Domain（如 FinanceDomain）新增 export_weekly_report(ctx) 方法 → ③ ensure_system_cron_triggers 中追加 INSERT 语句（cron 表达式按需求） | [consumer/scheduler.rs 分发 match](src/consumer/scheduler.rs#L53-L131) |
| 沉淀策略可配置化（按 Agent 可配置冲突阈值 / 每天沉淀条数 / 是否发布为共享） | ① AgentRuntimeConfig 追加 memory_settle_config JSON 字段 → ② RuntimeAwakening 透传 options 到 sleep_and_settle → ③ DAL settle_short_term_to_long_term 读取 config 覆盖默认阈值 | [models/agent.rs AgentRuntimeConfig 定义](src/models/agent.rs) |
