---
kind: RAG 原子知识卡
name: 唤醒前置门闩与依赖补发闭环：wake_gate_policy 策略化前置校验 + TaskEventConsumer 依赖就绪自动重发
category: 基础设施 / 异步事件
scope:
- src/consumer/wake_gate_policy.rs
- src/consumer/message.rs
- src/consumer/task_event_consumer.rs
- src/consumer/mod.rs
- src/pkg/policy/**
- src/models/task.rs
source_files:
- src/consumer/wake_gate_policy.rs#L26-L44
- src/consumer/wake_gate_policy.rs#L149-L268
- src/consumer/wake_gate_policy.rs#L269-L420
- src/consumer/message.rs#L491-L640
- src/consumer/message.rs#L1044-L1130
- src/consumer/task_event_consumer.rs#L100-L130
- src/consumer/task_event_consumer.rs#L205-L300
- src/models/task.rs#L304-L312
- docs/design/thinking_task_policy_engine_design.md
- docs/wiki/zh/content/核心模块/AOP 事件系统/消费者框架/任务事件消费者.md
- docs/wiki/zh/content/核心模块/AOP 事件系统/消费者框架/消息消费者.md
- docs/wiki/knowledge/zh/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层.md
- docs/wiki/knowledge/zh/任务状态机与项目聚合：TaskStatus 4 态 + progress 0-100 自动联动 + execution_plan_result JSON Patch + TaskGraph 依赖 DAG/任务状态机与项目聚合：TaskStatus 4 态 + progress 0-100 自动联动 + execution_plan_result JSON Patch + TaskGraph 依赖 DAG.md
- docs/wiki/knowledge/zh/消息交互与SSE推送：MessageDomain双能力 + AgentLoopConsumer循环 + 多渠道出站5类/消息交互与SSE推送：MessageDomain双能力 + AgentLoopConsumer循环 + 多渠道出站5类.md
---

# 唤醒前置门闩与依赖补发闭环

## §1 概述

本卡覆盖「Agent 该不该被这条消息唤醒」的判定，以及判定为「不该唤醒」之后**如何保证任务不丢**这两件事。

项目管理场景下项目负责人会把整张任务 DAG 一次性全部分派出去，后继任务的 `TaskAssignment` 消息会先于它的前置任务完成而到达。若无拦截，Agent 会被唤醒去执行一个还轮不到它的任务——白烧一次 LLM 往返与工具调用，且大概率产出错误的中间结果。

判定层复用 `pkg/policy` 策略引擎（`wake_gate_policy.rs`，消息消费领域落地），三条判据：任务已终结 / 前置依赖未就绪 / 单任务唤醒预算耗尽。**策略只回答「跳不跳 + 为什么」，绝不碰副作用**（释放 Busy、通知来源方都留在 consumer 侧）。

补发层在 `TaskEventConsumer`：任务 `Completed` 时反查同项目内依赖它的后继，全部前置真完成且后继仍 `Pending` 时自动重发 `TaskAssignment`。**门闩跳过 ≠ 丢弃**，跳过的那次唤醒被推迟到前置完成时补上。

## §2 关键文件路径表格

| 文件 | 角色 |
|---|---|
| [src/consumer/wake_gate_policy.rs](src/consumer/wake_gate_policy.rs) | 门闩策略本体：`keys` 模块（Metrics 键约定）+ `WakeGateInput`（查库事实的载体）+ `WAKE_GATE_DEFS` 声明式规则表 + `judge_wake_gate()` 出口 |
| [src/consumer/message.rs](src/consumer/message.rs) | 适配器：`build_wake_gate_input` 填 Metrics、`classify_dependencies` 逐个查前置分箱、`handle_agent_message` 统一出口释放 Busy |
| [src/consumer/task_event_consumer.rs](src/consumer/task_event_consumer.rs) | 补发闭环：`dispatch_ready_successors` 三道幂等闸 + 重发 `TaskAssignment` |
| [src/models/task.rs](src/models/task.rs) | 依赖数据源：`TaskPo::get_dependencies()` 解析 `dependencies` JSON 列 |
| 【Design】thinking_task_policy_engine_design.md | 策略引擎决策背景（不感知业务语义的设计哲学） |
| 【Wiki 长文】任务事件消费者.md | 补发链路系统化上下文 |
| 【Wiki 长文】消息消费者.md | 门闩在消息消费主链路的位置 |
| 【兄弟卡】策略引擎 | 引擎本体 + think_loop / Shell 两个既有落地 |
| 【兄弟卡】任务状态机与项目聚合 | `TaskStatus` 4 态语义 + TaskGraph DAG 依赖编排 |
| 【兄弟卡】消息交互与SSE推送 | 消息域双能力 + `message_route_policy` 路由策略落地 |

## §3 架构约定

### 3.1 本卡与策略引擎卡、任务状态机卡构成「策略引擎落地 + 任务依赖语义 + 消息消费主链路」的互补视角；按 AGENTS §2.1.3 Level 3 保留平行卡

策略引擎的领域落地已有三处，各自 scope 不重叠：Shell 拦截层（`tool_registry/shell_policy.rs`）、思考循环退出裁决（`runtime/think_loop.rs`）、**本卡的唤醒门闩**（`consumer/wake_gate_policy.rs`）。

### 3.2 声明顺序 = 优先级，首个命中即决策

`WAKE_GATE_DEFS` 是 `OnceLock` 静态规则表，按 Or 组组合，`evaluate().first()` 决定命中谁。所以规则表里 `dependency_unmet` 必须排在 `dependency_stale` 前面（两者可同时成立，前者信息量更大），`task_terminal` 排在最前（已终结的任务谈依赖无意义）。

### 3.3 动态阈值不能用 `ThresholdPolicy`

`ThresholdPolicy::new` 里有 `Box::leak`（构造期固定 threshold），**每条消息路径反复 new 会漏内存**。`wakeup_budget_exhausted` 需要读 Agent 级 `max_thinking_depth` 动态值，因此实现为领域专用策略，从 Metrics 双键取当前值与上限——与 builtin `MaxRoundsPolicy` 保留专用实现的理由一致。

### 3.4 查库只在适配器，策略侧零查询

`classify_dependencies` 逐个 `get` 前置任务并分箱成 `pending` / `stale` 两个列表，`build_wake_gate_input` 查完任务后构造 `WakeGateInput`。策略表只读 Metrics，不碰 DAO / Domain。

### 3.5 查不到的前置一律按「未完成」处理

`Ok(None)`（已删除 / 脏数据）与 `Err`（查询失败）都进 `pending` 并打 warn。宁可多等一轮，也不要因为一次查询异常把 DAG 误判为就绪。

### 3.6 依赖判据只对 `TaskAssignment` 生效

普通 `Text` 消息（用户对任务的追问、Agent 协作汇报）即便 `task_id` 非空也**不走依赖判据**——门闩拦的是「派发」不是「对话」。

### 3.7 补发幂等靠三道闸

后继仍 `Pending` / 全部前置真 `Completed` / 承接 Agent 无未投递的同类型指派（复用 `has_pending_message_for_agent`）。缺一道都可能重复派发。

### 3.8 指派给人的任务不参与补发

`assignee_type != Agent` 的后继走人的任务列表，不走消息通道。

### 3.9 身份计算提到两路投递之前共用

Owner 通知与后继补发共用同一份 `(from_id, from_role)`：项目有归属用户 → 以用户身份中继；无归属用户（A2A 项目）→ 落 System。两种身份都不会把 Final 回复路由回 Agent 自身，无自唤醒循环。

## §4 硬约束（红线，违反即打回）

1. ❌ **禁止在 `handle_agent_message` 里新增硬编码 `if` 判据**：唤醒前置校验只能加到 `WAKE_GATE_DEFS` 规则表 + `build_wake_gate_input` 取数，consumer 出口分支不动。
2. ❌ **禁止在策略实现里调 DAO / DAL / Domain 或做副作用**：释放 Busy、通知来源方、日志分级都在 consumer 侧。策略违反「纯判断框架」约定。
3. ❌ **禁止用 `ThresholdPolicy` 承载动态阈值**：其 `new` 含 `Box::leak`，消息路径每请求 new 一次就是一次内存泄漏。动态阈值走领域专用策略读 Metrics。
4. ❌ **禁止把「跳过」实现成 `return Err(...)`**：AOP 只有 `RetryDecision::Retry / Discard` 两档，**没有延迟重投**。`Err` = 立即重投 8 次后丢弃且堵住该 Agent 队列；`Ok` = ack 终结。都不等于「等会儿再来」，后继的推进责任只能由 `dispatch_ready_successors` 承担。
5. ❌ **禁止省掉三道幂等闸中的任何一道**：尤其 `has_pending_message_for_agent`——内存态看不到「已投递但堵在队列」的消息，去重必须落库。
6. ❌ **禁止把 `Cancelled / Archived` 的前置算作就绪**：那会让依赖被取消任务的下游在输入缺失时启动。前置非 `Completed` 即分箱，`stale` 分箱额外打 warn 暴露死锁。
7. ❌ **禁止用 `list_by_project` 之外的查询为 JSON 依赖列写反向 SQL**：项目内任务量可控，一次全量拉取 + 内存索引远快于逐条反查。
8. ✅ **前置状态查询失败必须打 `log_warn` 并按未完成处理**，不能静默 `continue`。
9. ✅ **新增门闩判据的流程**：`keys` 加 Metrics 键 → `WAKE_GATE_DEFS` 加一条规则（注意声明顺序即优先级）→ 需要副作用就在 `handle_agent_message` 出口按 `policy_id` 加分支 → 补单测（命中 / 未命中 / 优先级）。
10. ✅ **`WAKEUP_BUDGET_POLICY_ID` 与 `STALE_DEPENDENCY_POLICY_ID` 是 consumer 侧识别分支的契约**：改 id 必须同步改出口判断。
11. ✅ **日志分级**：预算耗尽走 `log_info`（合法停止，且已通知来源方）；依赖失效走 `log_warn`（死锁信号需人工介入）；普通「还没轮到」走 `log_info`。
12. ❌ **禁止在门闩层查「唤醒次数」时重新统计 AOP 事件条数**：口径是 `agent_awake_events` 中该 Agent 在该任务上的累计条数（单任务内唤醒次数上限），不是工具调用数、也不是思考轮次（后者由 `max_thinking_rounds` 管）。
