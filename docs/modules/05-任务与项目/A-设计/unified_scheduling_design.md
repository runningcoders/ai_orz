---
kind: 设计决策
module: 05-任务与项目
name: unified_scheduling_design
title: 统一定时调度器与日程设计
status: 草案
scope:
  - src/models/cron_trigger.rs
  - src/config.rs
  - src/service/dao/cron_trigger/mod.rs
  - src/service/dao/cron_trigger/sqlite.rs
  - src/service/dal/cron_trigger.rs
  - src/service/domain/system/mod.rs
  - src/producer/cron_trigger.rs
  - src/consumer/scheduler.rs
  - src/pkg/aop/core/registry.rs
  - src/pkg/aop/core/consumer.rs
  - common/src/enums/cron_trigger.rs
  - common/src/api/cron_trigger.rs
  - common/src/schedule/{mod,cron,occurrence}.rs（前后端共用的时间算法）
  - src/handlers/system/cron_trigger/query_schedule.rs
  - frontend/src/pages/system/triggers.rs
  - migrations/20260420000000_initial.sql
related:
  - 10-异步基建(AOP)/A-设计/aop_producer_consumer_contract_design.md
  - 02-Agent运行时/A-设计/runtime_design.md
---

# 统一定时调度器与日程设计

> 状态：草案（待评审拍板）。本文只定义意图与边界，不描述当前实现细节；现状以源码及 C-知识为准。
>
> 关联设计：
> - [AOP 生产者消费者契约设计](../10-异步基建(AOP)/A-设计/aop_producer_consumer_contract_design.md) — Sync/Async、ordered、RetryDecision 的硬约束来源
> - [Runtime Domain 设计](../02-Agent运行时/A-设计/runtime_design.md) — Agent 唤醒状态机与 busy 门闩
> - [定时任务系统设计文档（已废弃）](./task_scheduler_design.md) — 初版顶层字段设计，本文是它在「日程」场景下的重新论证

---

## 1. 设计目标

把现有的「定时触发器」从**系统维护工具**升格为**统一动态事件调度内核**：一条记录完整描述「什么时间 · 谁 · 做什么事 · 怎么做」，系统维护、用户提醒、Agent 自主日程都是这条记录的 different instances。

具体目标：

1. **一张表承载多形态**：`cron_triggers` 扩展顶层参数，新增 `Schedule` 触发类型；系统默认触发器与用户日程共用同一张表、同一条扫描链路，各自只填与自己相关的字段。
2. **行为层可配置**：把散落在 `payload.action` 字符串与 `CronTriggerConsumer` 分支里的消费语义，收敛为**有限的行为模板**（同步/异步、ordered、重试策略），表里只存模板名。
3. **Agent 自主性**：Agent 通过工具创建日程，到点后由统一链路唤醒 Agent 执行；创建语境（组织、根用户、创建者）在写入时由 `RequestContext` 固化，不信任模型传参。
4. **一次建模，逐步落地**：本次只冻结模型与边界，实现按阶段推进（数据层 → 行为模板 → 日程工具 → 前端）。

### 1.1 为什么不是新建 `schedules` 表

初版设计曾定义过 `org_id` / `user_id` / `start_at` / `end_at` / `max_runs` / `task_*` 等完整顶层字段，后来为「单一 payload JSON」精简。现在重新引入归属、状态、行为配置的理由不是「又需要一个字段」，而是**日程让「谁」和「怎么做」从可选变成必需**：

- 系统维护任务没有受众，归属可空；日程有明确的创建者、执行者、受众。
- 系统维护任务是同步轻量动作；日程唤醒 Agent 必须异步串行，不能堵住 60s 轮询。
- 两者共享「到期扫描 + 事件推送 + 成功才推进」的全部机制。拆表等于复制一整套调度器。

判据：**形态不同，调度机制相同** → 同表 + 类型区分；如果将来出现「调度机制不同」的形态（如毫秒级时间轮），再分表。

### 1.2 域归属：规则内核在 system，动作落点在各业务域

本项目的「域」用传统职能名**隐喻**承载 AI 原生实体：`hr` = Agent 管理域（生命周期/技能/记忆沉淀），`finance` = 外部能力治理域（模型/工具/渠道/凭证/附件，计费统一入口），`system` = 定时触发器等系统功能（`src/service/domain/mod.rs` 模块注释明示）。据此：

| 关注点 | 归属 | 理由 |
|---|---|---|
| **调度规则**（`cron_triggers` 记录、When/Who/What/How、`CronManager`、Producer、行为模板分发） | **system 域**，保持原位 | 与现有定时器同源；日程是它的业务形态，不构成新域。不迁到 hr / finance |
| **到点后的动作** | **各业务域**，按 behavior 模板派发 | `AsyncAgentWakeup` → runtime（唤醒执行）；沉淀类 → hr；巡检类 → project；用户提醒 → message |
| **创建入口** | 适配层（handler / 工具），编排 system 域写入 | 工具是「写调度规则」，不是「拥有调度规则」 |

判据：**谁拥有「调度规则」的语义 → system；谁拥有「到点后动作」的语义 → 各业务域**。这与「域之间只通过参数交互」的红线一致——system 域不反向引用 hr / runtime / message，只发事件；动作域通过订阅事件或 handler 编排接收参数。

> 明确否定：日程**不归 hr**（hr 是 Agent 档案与记忆域，只是 `agent_rest` 沉淀动作的落点）、**不归 finance**（finance 是外部能力配置与计费治理）。把日程塞进任一业务域都会制造 system ↔ 业务域的双向依赖。

---

## 2. 核心模型：四组顶层参数

一条调度记录 = `When + Who + What + How`，四组参数全部是表的顶层列；系统触发器的 `payload` 退化为「行为模板的扩展参数」，日程的 `payload` 则是无协议的自由信息（见 §2.3），两者都不承载身份与调度语义。

| 组 | 语义 | 字段 | 系统默认触发器 | 用户日程 | Agent 日程 |
|---|---|---|---|---|---|
| **When** | 什么时间 | `trigger_type` / `cron_expression` / `interval_seconds` / `run_at` / `start_at` / `end_at` / `next_run_at` | 必填 | 必填 | 必填 |
| **Who** | 谁 | `org_id` / `owner_user_id` / `creator_type` / `creator_id` / `agent_id` | 留空 | 必填 | 必填 |
| **What** | 做什么事 | `name` / `description` / `action` / `payload.extra` | 必填 | 必填 | 必填 |
| **How** | 怎么做 | `behavior` / `consume_mode` / `order_key` / `retry_policy` / `max_runs` / `total_runs` / `last_error` | `SyncMaintenance` | `AsyncNotify` | `AsyncAgentWakeup` |

> 「字段留空」是设计的一部分，不是妥协：系统级触发器用 `NULL` 表达「无归属约束」，用户级记录必须填全 Who 组中的**主体**（`owner_user_id` / `creator_*`），唤醒模板还必须有 `agent_id`，由 Domain 层按 `behavior` 做条件校验。
> `org_id` 例外——**允许为空**：本地部署只有一个组织，日程优先绑定用户 / Agent，系统日程缺省即本地组织；远程组织走独立实例，不在同一张表内做租户隔离。

### 2.1 `When`：触发类型扩展

`TriggerType` 从 3 值扩展为 4 值，追加 `Schedule = 3`。

- `Once = 0` / `Cron = 1` / `Interval = 2` 语义不变，兼容存量数据。
- `Schedule = 3` 表示「日程」：时间计算规则与 `Once`/`Cron`/`Interval` 复用，差别只在 Who/How 组必填、行为模板限定为异步族。
- `start_at` / `end_at` 为**调度生效窗口**，不是业务时间：窗口外的到期记录由 Producer 跳过并推进 `next_run_at`，不产生事件。
- `run_at` 沿用现有字段（原设计稿叫 `one_time_run_at`），统一命名，避免两套一次性时间字段。

> 决策：`Schedule` 是**形态标签**而非独立调度算法。它的时间语义由 `cron_expression` / `interval_seconds` / `run_at` 之一给出，由 `trigger_type` 与这三个字段的组合校验保证「恰好一种」。

### 2.2 `Who`：归属、创建者、负责 Agent 三者分离

现有表只有 `created_by` 审计字段，没有归属列。新增三个**互不兼任**的字段：

| 字段 | 语义 | 来源 |
|---|---|---|
| `org_id` | 组织归属，用于隔离 | 创建时 `ctx.organization_id()` 固化 |
| `owner_user_id` | 根用户归属，即**推送面/受众**，也是日程的可见范围 | 创建时由 `ctx.user_id` 经 `root_user_id` 还原后固化；上下文无归属时由 User 创建者兜底 |
| `creator_type` | `user` / `agent` / `system` | 创建时固化 |
| `creator_id` | 创建者 ID（用户 ID 或 Agent ID） | 创建时固化 |
| `agent_id` | **负责该日程的 Agent**：到点唤醒它执行，或由它发出提醒 | 显式参数（校验存在 + `Onboarded`）；上下文没有负责 Agent 时由创建者兜底（仅当创建者是 Agent） |

三者的区别是最容易混淆的地方，必须分清：

- **归属**回答「这条日程算谁的」——决定查询可见范围与提醒推给谁；用户经 HTTP 建日程时它是该用户。
- **创建者**回答「谁建的」——可以是用户，也可以是 Agent（自主日程），还可能 System（seed）。
- **负责 Agent** 回答「到点谁干活」——唤醒模板的目标，也是通知模板的发送者。

> 用户建的日程不会再被误当成「Agent 发的」：发送者取 `agent_id`（负责 Agent）。
> `AsyncNotify` 未指定负责 Agent 时（纯用户提醒）发送者用 `system` 哨兵；
> `send_to_user` **按发送者 ID 推导角色**（`system` → `System`、users 表命中 → `User`、
> agents 表命中 → `Agent`），不会落出 `from_role=Agent, from_id=system` 的错位记录。
>
> `creator_*` 是「谁安排的」，用途只有两处**创建时兜底**，不参与可见/可改权限：
>
> 1. 上下文没有负责 Agent 时，创建者是 Agent 就由它兜底为 `agent_id`；
> 2. 上下文没有归属用户时，创建者是 User 就由它兜底为 `owner_user_id`。
>
> 创建者是 Agent/System 且无归属时直接拒绝——没有归属的日程到点无法推送、也无法
> 还原工具凭据。可见/可改一律看 `owner_user_id`（归属），同一用户下的多个 Agent
> 共享管理权，符合「Agent 代表用户行事」的模型。
>
> 到点唤醒时，身份三字段会随正文一起交给 Agent（`安排者 / 归属用户 / 负责 Agent`），
> 否则「谁安排的」只落库不进执行上下文，等于没建。
>
> 不再保留 `target_type` / `target_id`：多态目标里只有 Agent 真正被消费，两个字段并存
> 会与 `agent_id` 形成两套「谁执行」的口径。收紧为单列后，非法组合从数据层就不存在。

**硬约束（来自身份红线）：**

- `ctx.user_id` 只从 `root_user_id` 还原；工具调用不得信任模型传入的 `owner_user_id`，必须在 handler 内用 `resolve_root_user_id` 校验并覆盖。
- 推送面是受众 `owner_user_id`。Agent 间消息只走 SSE `push_message_to_audience`，不复用 `deliver_message`。
- `org_id` / `owner_user_id` / `creator_*` 在创建后不可变；更新接口禁止修改这四组字段。
  `agent_id` 属于业务配置，**可以**经更新接口改派（同样要过「存在 + Onboarded」校验）。

> 为什么不用 `created_by` 兼任：它是审计字段（系统任务写 `"system"`），语义是「谁改的」而非「谁拥有」。归属与审计混用会让多租户隔离失效。

### 2.3 `What`：动作与扩展参数

⚠️ **两类 payload 的语义必须分开，不要互相套：**

| 形态 | payload 是什么 | 谁读 | 框架约束 |
|---|---|---|---|
| **内置系统触发器**（`SyncMaintenance`） | `{"action":"<id>","extra":{...}}` 分派外框；`action` 决定执行哪个维护动作 | 消费端（分派 + 动作参数） | 有协议：`action` 必填语义、`extra` 具名字段 |
| **日程**（用户 / Agent 自定，`AsyncNotify` / `AsyncAgentWakeup`） | Agent 写在日程上、留给到点的自己读的**自由信息 / 备注** | 只有 Agent 自己；到点原样附在唤醒正文回灌 | **无协议**：字段、结构、含义全由 Agent 自定义，服务端不解析、不解释、不赋语义 |

日程**不存在「动作」语义**——到点做什么由收到信息的 Agent 自己决定。日程的 payload 既不是 action 分派外框，也不与顶层 `action` 列挂钩；顶层 `action` 列在日程形态下只是展示 / 检索用的标签（缺省取标题），不参与执行分派。

- `action` 从 `payload` 提升为顶层列，成为行为的业务标签（`agent_rest` / `agent_schedule` / `user_reminder` / …）。**此条的 action 分派语义只适用于内置系统触发器**；日程形态下 `action` 退化为标签。
- 系统触发器的 `payload` 保留为 `extra` 容器：`{"extra": {...}}`。存量 `{"action": "...", "extra": {...}}` 由 DAL 读取时兼容解析（`action` 列优先，回落 payload 内 action）。
- 系统触发器 `payload.extra` 必须是具名字段，禁止 `#[serde(flatten)]`（现有 `CronTriggerPayload` 的踩坑注释已说明）。
- `description` 是给用户看的自然语言意图，同时作为 Agent 日程的指令正文来源。

### 2.4 `How`：行为模板（关键决策）

**行为模板是一段静态配置，不是运行时自由组合。** AOP 注册期有硬约束：`ConsumeMode::Sync` 的消费者禁止声明 `.ordered()`。如果允许数据自由组合 `consume_mode` + `order_key` + `retry_policy`，一定会出现「Sync + ordered」这种注册期就非法的记录。

因此引入枚举 `TriggerBehavior`，表中只存模板名；模板固定以下属性：

| 模板 | 值 | 消费模式 | ordered | 重试 | `order_key` | 适用形态 |
|---|---|---|---|---|---|---|
| `SyncMaintenance` | 0 | Sync | 否 | `Retry`（失败不标记，下轮重捞） | — | 系统默认 6 个触发器 |
| `AsyncNotify` | 1 | Async | 是 | nack 重投 | `agent_id` | 用户提醒（由负责 Agent 推送，不唤醒） |
| `AsyncAgentWakeup` | 2 | Async | 是 | nack 重投 | `agent_id` | Agent 日程，唤醒后执行 `Awaken` run |

- 模板到 Consumer 的映射在 `Registry::init_all` 时静态注册，运行时不动态改变消费者声明。
- `consume_mode` / `order_key` / `retry_policy` 作为**模板属性**存在于代码配置中，不落表；表里只留 `behavior` 一个开关。避免两处真相源。
- `max_runs` / `total_runs` / `last_error` 是执行控制与观测字段，归在 How 组但由 DAL 自动维护，不接受创建参数直接写入。

> 决策理由：行为配置化的价值在「新增业务无需改调度器」，不在「同一个业务能任意切换同步异步」。有限模板足够，且能守住框架不变量。

---

## 3. 数据模型

### 3.1 迁移策略

在 `cron_triggers` 上做增量迁移（新增迁移文件，不改初始迁移）：

```sql
ALTER TABLE cron_triggers ADD COLUMN trigger_type_v2 ...;
-- 具体列见 3.2；trigger_type 复用原 INTEGER 列，追加枚举值 3
```

**兼容性要求：**

- 所有新增列 `NULLable` 或带默认值，存量 6 条系统触发器零数据迁移即可读取。
- 存量 `payload` 中的 `action` 由 DAL 回落到顶层 `action`，保证老记录行为不变。
- `is_enabled` 语义不变（暂停/软删仍置 0），不引入第二套启用状态列。

### 3.2 字段清单（新增部分）

| 字段 | 类型 | 默认 | 组 | 说明 |
|---|---|---|---|---|
| `org_id` | TEXT | NULL | Who | 组织归属 |
| `owner_user_id` | TEXT | NULL | Who | 根用户归属/受众 |
| `creator_type` | INTEGER | NULL | Who | 0=user 1=agent 2=system |
| `creator_id` | TEXT | NULL | Who | 创建者 ID |
| `agent_id` | TEXT | NULL | Who | 负责 Agent（唤醒目标 / 通知发送者） |
| `description` | TEXT | `''` | What | 自然语言意图 |
| `action` | TEXT | NULL | What | 业务动作标签，回落 `payload.action` |
| `behavior` | INTEGER | 0 | How | `TriggerBehavior` 模板枚举 |
| `start_at` | INTEGER | NULL | When | 生效窗口起点 |
| `end_at` | INTEGER | NULL | When | 生效窗口终点 |
| `max_runs` | INTEGER | NULL | How | 最大执行次数，NULL 不限 |
| `total_runs` | INTEGER | 0 | How | 累计执行次数 |
| `last_error` | TEXT | NULL | How | 上次执行错误 |
| `lease_owner` | TEXT | NULL | How | 领取者实例 ID（并发控制） |
| `lease_until` | INTEGER | NULL | How | 租约到期时间 |

索引新增：`(is_enabled, next_run_at, behavior)`、`(org_id, owner_user_id)`、`(agent_id)`。

### 3.3 并发领取（必须同批落地）

现状 `list_due` 只有 `WHERE next_run_at <= ? AND is_enabled = 1`，无 CAS、无行锁。系统任务重复跑是浪费，日程重复唤醒 Agent 会产生真实副作用。

设计采用**短租约领取**而非乐观锁版本号：

1. Producer 扫描时对每个到期记录执行 `UPDATE ... SET lease_owner=?, lease_until=? WHERE id=? AND (lease_until IS NULL OR lease_until <= now)`。
2. 只有 `rows_affected = 1` 的记录才 emit 事件。
3. 租约在 `on_consumed` 时清除并推进 `next_run_at`；消费失败/超时则租约自然过期，下轮重新领取。
4. 租约时长 > 单轮最长消费时间；同步模板消费极短，异步模板只等「派发成功」不等 Agent 跑完。

> 为什么不复用 `mark_executed` 做 CAS：它发生在 `on_consumed`，而异步消费完成可能很久以后；领取必须发生在 emit 之前，否则多实例窗口仍然存在。

---

## 4. 运行链路

### 4.1 扫描与派发

沿用现有 `CronTriggerProducer` → AOP → `CronTriggerConsumer` 链路，改动点：

1. `tick()` 增加生效窗口判断与租约领取。
2. `CronTriggerEvent` 扩展字段：携带 `trigger_type` / `behavior` / Who 组 / `action` / `description`，让消费侧无需回查表。
3. `CronTriggerConsumer` 升级为**统一分发器**：
   - `SyncMaintenance` → 本地 `match action`（现有 6 个分支原样保留）。
   - `AsyncNotify` → 构造消息并 `send_to_user`，走 `message.created`。
   - `AsyncAgentWakeup` → 构造 `SendToAgentCommand` + 行动清单正文，`send_to_agent` 走 `message.created`。

### 4.2 为什么 Agent 唤醒走消息，不走事件直调

已确认的现成机制：

- `project_followup` / `pending_notification_redelivery` 都是「定时 → 发消息 → `message.created` → `agent.awakening` 消费者唤醒 Agent」，不是直接调 Agent。
- `agent_rest` 走的是 `AgentSettleEvent` → Settle 场景沉淀 run，**只能跑 Settle**，不适合「日程到点做任意事」。
- 消息范式自动获得 busy 抢占、唤醒门闩、`order_key=agent_id` 串行、幂等去重、滞留重投、Final 路由六套机制。

因此 `AsyncAgentWakeup` 的落点是 `send_to_agent`，指令正文由 `description` + `action` 合成（Agent 靠正文判断动作，`message_type` 只影响路由/话术/去重）。

> 事件载荷只到「消息已投递」为止；Agent 真正执行的成功与否通过消息/任务状态回流，不占用触发器租约。

### 4.3 收尾语义（不可破坏）

现有三条红线原样保留：

- `mark_trigger_executed` 在 `on_consumed` 回调中执行，失败不标记。
- `decide_retry` 对 cron topic 恒返回 `Retry`，避免永久性错误被 `Discard` 后仍回调导致丢触发。
- `CronTriggerConsumer` 是 Sync，任何重活必须二级派发，不得在本线程执行。

---

## 5. Agent 自主创建日程

### 5.1 工具定义

新增 handler 工具 `schedule_agent_task`（`#[register_handler_tool]` + `#[generate_http_handler]` 双宏）：

- 参数 DTO 放 `common::api`，字段：`title` / `description` / `when`（`trigger_type` + 时间参数）/ `agent_id`（负责 Agent，可选）/ `behavior`（可选，默认 `AsyncAgentWakeup`）/ `max_runs`（可选，必须为正数）。
- **不接受** `org_id` / `owner_user_id` / `creator_type` / `creator_id`（由 handler 从 ctx 固化）。
  `agent_id` 是业务字段而非身份，允许显式指定，但要过「存在 + `Onboarded`」校验。
- 不加 `neural` tag：Awaken 场景「绑定即权限」可注入，Settle/Compact 场景按 tag 过滤，避免沉淀循环中误建日程。
- 工具执行自动获得 `AgentToolStarted` / `ToolExecEvent` 审计链（JSONL + DuckDB）。

### 5.2 身份固化流程

```
Agent 工具调用
  → ctx.message_sender_id() / caller_type → creator_type + creator_id
  → owner_user_id = ctx.uid()（root_user_id 还原）
      为空且 creator 是 User → 用 creator 兜底；否则拒绝
  → ctx.organization_id() → org_id（可空：本地部署缺省即本地组织）
  → agent_id 显式参数优先；上下文没有负责 Agent 时，creator 是 Agent → 用 creator 兜底
  → 校验 agent_id 存在 + Onboarded（同组织门做不了，见 P2 §1.2）
  → 写入 cron_triggers（trigger_type = Schedule, behavior = AsyncAgentWakeup）
  → 返回 trigger_id 给模型
```

禁止「用 A 的凭据替 B 建日程」，参照 `create_task` 的归属校验模式。

### 5.3 到点执行

由 §4.2 的消息范式执行。正文至少包含：日程 ID、安排者（`creator_type/creator_id`）、归属用户、负责 Agent、用户原始意图、期望动作、完成后如何汇报。身份三字段必须随正文下发——只落库不传递，Agent 到点只知道「做什么」，不知道「谁安排的、代表谁」。

提醒模板（`AsyncNotify`）的发送者取 `agent_id`；无负责 Agent 时用 `system` 哨兵，由 `send_to_user` 按 ID 推导 `from_role`，禁止在调用入口写死角色。

---

## 6. 状态机

```
                  ┌──────────────────────────────────┐
                  │                                  │
                  ▼                                  │
Active ──到期+领取成功──▶ Dispatched ──消费成功──▶ Advanced
  │                        │                          │
  ├──pause──▶ Paused       └──消费失败──▶ Retryable    │
  │                                           │       │
  └──end_at/max_runs 到达──▶ Completed ◀──────┘       │
                                                      │
  cancel（软删 is_enabled=0）──▶ Cancelled             │
```

- `Active` / `Paused` 用现有 `is_enabled` 表达，不新增启用状态列。
- `Dispatched` / `Retryable` 是租约的运行态，由 `lease_owner` / `last_error` 表达，不是持久状态机节点。
- `Completed` 对 Once/Schedule 终态（消费成功后 `is_enabled=0`）；Cron/Interval 无此终态，除非 `end_at` / `max_runs` 到达。
- `Cancelled` 即软删，与 Paused 的区别是取消不可恢复（更新接口拒绝复活 `Cancelled` 记录）。

---

## 7. 校验规则（Domain 层）

按 `trigger_type` + `behavior` 做**条件必填**校验：

| 校验 | 规则 |
|---|---|
| 时间 | Once 必须有 `run_at`；Cron 必须有合法 `cron_expression`；Interval 必须有 `interval_seconds > 0`；Schedule 三选一。interval 统一走 `common::schedule::interval_next_run_at`（受检乘加，极端值报错而非 wrap 成过去时间） |
| 归属 | `behavior != SyncMaintenance` 时 `owner_user_id` / `creator_type` / `creator_id` 全必填；`org_id` 可空；`max_runs` 若给出必须为正数 |
| 行为一致性 | `Schedule` 只允许 `AsyncNotify` / `AsyncAgentWakeup`；`SyncMaintenance` 仅系统 seed 可创建，用户接口拒绝 |
| 负责 Agent | `AsyncAgentWakeup` 必须有 `agent_id`；`agent_id` 存在时必须指向存在且 `Onboarded` 的 Agent（`AgentState` 维度，组织维度不可校验） |
| 窗口 | `end_at > start_at`（若有两者） |
| 不可变 | 更新接口拒绝修改 `org_id` / `owner_user_id` / `creator_type` / `creator_id` |

`behavior` 由服务端决定或从白名单选择，不接受任意字符串。

---

## 8. 分层落位

遵守域间只通过参数交互的红线：

- **Models**：`CronTriggerPo` 扩展字段；`TriggerType` / `TriggerBehavior` / `CreatorType` / `TargetType` 枚举放 `common/src/enums/`（DB 映射型，`#[repr(i32)]` + `sqlx::Type` + `From<i64>`）。
- **DAO**：`cron_trigger` 增加租约领取 SQL、按 `behavior` 过滤；不引入跨域查询。
- **DAL**：`create` 做条件必填校验与归属固化；`mark_executed` 处理租约清除 + 三种时间推进 + `total_runs` / `last_error`；兼容老 payload 解析。
- **Domain**：`CronManager` 增加 `create_schedule` / `cancel` / `list_by_owner`；校验规则集中在此，不散落在 handler。查询能力（含 `query_schedules`）同样归属 domain。参数是否另立领域结构体，判据是「DAO Query 能否表达全部查询意图」：能表达的（如 `list_triggers`）直接复用 `CronTriggerQuery`，表达不了的（日期词/关键词/展开）才定义 `ScheduleQuery`，下传时再转成 DAO Query。核心是查询逻辑不写 handler，DAL/DAO 是 domain 的基建。
- **Handler**：`schedule_agent_task` 工具（Agent 自主创建）；`create_cron_trigger` 保持系统/管理员用途，增加 `behavior` 白名单；`query_schedule` 只做参数映射与 DTO 透传，不碰 DAO、不写查询逻辑。
- **Producer**：`CronTriggerProducer` 增加窗口判断与租约领取，事件载荷扩展。
- **Consumer**：`CronTriggerConsumer` 升级为按 behavior 分发；Sync 分支只派发，不复写业务逻辑。
- **编排**：`AsyncAgentWakeup` 的消息构造由上层 handler/consumer 编排（先取 trigger 记录，再构造 `SendToAgentCommand`），不让 `cron_trigger` 域直接引用 message 域。

---

## 9. 可观测性

- 每次触发写 `ThinkBus`/`TraceSink`？**不**：调度器不在图内，走已有 `log_info!` + `ToolExecEvent` + `cron_triggers.last_error`。
- `record_event!` 打点：`schedule.fired`（按 behavior / trigger_type 维度）、`schedule.dispatch_failed`、`schedule.lease_conflict`。
- 失败不静默：`last_error` 写入错误摘要，下轮重捞；连续失败 N 次可配置告警（后续阶段）。

---

## 10. 分阶段落地

| 阶段 | 内容 | 交付判据 |
|---|---|---|
| P0 | 迁移 + Models/枚举/DAO/DAL 扩展 + 兼容解析 | 存量 6 条触发器行为不变；新字段可读写 |
| P1 | 行为模板 + 租约领取 + Consumer 按 behavior 分发 | 多实例不重复触发；同步链路不堵塞 |
| P2 | `schedule_agent_task` 工具 + `AsyncAgentWakeup` 消息范式 | Agent 可自主建日程并按时被唤醒执行 |
| P3 | 前端展示升级：现有 `/system/triggers` 页面按新模型改造（不新建日程独立页） | 用户可查看/取消日程；系统触发器与日程同页分组展示 |
| P5 | `cancel_schedule` / `update_schedule` 工具 + 管理端更新校验与游标重算 | Agent 可撤销/修改自己的日程；管理端改时间规则立即生效 |

**前端展示策略（P3）：**

- **原位改造、组件复用**：沿用 `frontend/src/pages/system/triggers.rs` 现有页面与 `/system/triggers` 路由，不新建日程模块路由。表格、筛选、分页、创建/编辑弹窗、状态标签、Cron 预设等组件全部复用，只按四组模型调整字段分组与展示文案。
- **同页分形态**：系统维度触发器（Who 组为 NULL，`SyncMaintenance`）与用户/Agent 日程（Who 组完整，异步模板）在同一列表内用 `trigger_type` / `behavior` 标签区分与筛选；系统项保留 readonly 语义，不因改造放开编辑。
- **过滤与分页一律在数据端**：管理端列表的 `statuses` / `behaviors` 走列表参数（`enabled,disabled` 逗号分隔），`offset` / `limit` 下推 SQL；`statuses` 缺省只看启用，总数由同一 WHERE 的 `COUNT(*)` 给出。前端不做本地过滤、不拉全量，只有展示排序与日历展开可留在前端。
- **展示按改造后模型**：详情页字段按 When / Who / What / How 四组重排，`payload.action` 提升为顶层 `action` 展示；`behavior` 以可读标签（同步维护 / 异步通知 / Agent 唤醒）呈现，不暴露枚举裸值。
- **不引入日历组件**：P3 只做列表与表单的字段升级；日历/月视图属于后续独立需求，不在本次边界内（见 §11）。

本次设计只冻结 P0~P3 的边界与顺序，不承诺实现时间。

### 10.1 P4：时间语义与日程实例展开

**问题**：`cron_triggers` 只持久化「规则 + 一个游标 `next_run_at`」，不存实例表。因此
「查明天干什么」「查一周后的某个周一有没有重复日程」在 P0~P3 无从查起——所有 DAO
时间条件都是 `next_run_at <= now`（查到期），没有任何面向未来的范围查询。

**解法分三层**：

1. **时间语义层**：`ScheduleDateTerm` 枚举（`yesterday/today/tomorrow/three_days/this_week/next_week/custom`）
   让模型只选语义，**不自己算毫秒时间戳**；服务端按 `server.timezone`（默认
   `Asia/Shanghai`）把相对日词折算为本地日历日的半开区间 `[from, to)`。跨月/跨年/闰年
   由 chrono 处理，DST 跳变取最早分支保证单调。工具 description 显式写明这一点。
2. **展开层**：`common::schedule` 纯算法（从服务端 `pkg` 下沉而来），输入 `ScheduleRule`
   + 区间 + 时区，输出落在区间内的发生时刻。Cron 走 `Schedule::after` 逐点取；Interval
   以 `next_run_at` 为相位锚点前后按整数倍步长扩展（历史实例回答「昨天干了什么」，
   未来实例回答「明天干什么」）；Once 只有一个点。统一受 `created_at`/`start_at`/
   `end_at` 下界与 `max_runs - total_runs` 剩余次数约束，单条最多展开 400 点防止
   `*/1 * * * *` 撑爆。
   - **为何在 common**：时间展开是纯算法，前端将来若支持在日历上直接新建/拖拽日程，
     预览时需要在本地展开，不应先落库再往返服务端。放 common 使前后端同一份代码，
     cron 表达式规范化也只有一处（`common::schedule::parse_schedule`）。
   - `effective_type` 与 `mark_executed` 共用同一口径（`cron > interval > run_at`），
     下沉 common 作为单一事实源，避免「下一次计算」与「区间展开」算出不同实例；
     DAL 与 query handler 都调用它；时区值由 `config::timezone()` 从 `server.timezone`
     读出注入。没有 `pkg::cron` —— 纯算法在 common，配置读取在 config，职责不重叠。
3. **领域查询**：`domain::system::CronManager::query_schedules(ctx, ScheduleQuery)` 拥有
   全部查询语义（日期词解析、关键词过滤、规则展开、排序截断），并强制从 ctx 固化
   `owner_user_id`。参数是否另立领域结构体取决于 DAO Query 能否表达查询意图：
   `list_triggers` 的过滤条件可被 `CronTriggerQuery` 完整表达，故直接复用（同
   project/task 范式）；`ScheduleQuery` 带日期词/显式区间/关键词，由 domain 下传时
   转成 DAO Query。**判据是查询逻辑不写 handler**，而非一律禁止引用 DAO 值对象；
   consumer 可直接复用同一个领域查询方法。
4. **消费层**：`query_schedule` 双宏 handler 只是薄适配层（参数映射 + 身份固化 + DTO 透传），
   同时作为 Agent 工具和 HTTP 接口 `POST /api/v1/system/schedules/query`；前端同页三视图
   （列表 / 三天 / 周）直接消费展开实例，分桶与 HH:MM 格式化都调用 `common::schedule` 的
   时区工具，按响应携带的 `timezone`（即服务端展开口径）而非浏览器本地时区渲染。

**身份边界**：只查当前 `root_user_id` 名下的日程——规则创建时由 `schedule_agent_task`
从 ctx 固化 `owner_user_id`，查询时强制 `CronTriggerQuery.owner_user_id = ctx.uid()`。
系统默认触发器没有 owner，不进 Agent 视图（它们是调度器维护任务，不是用户日程）。

**为什么不建 occurrence 实例表**：展开是纯计算，规则改了结果立刻对；实例表要处理
规则变更后的历史回填、取消/暂停的级联失效，复杂度远超收益（YAGNI）。代价是
`max_runs` 的「已执行次数」只按 `total_runs` 近似，失败重试导致的次数漂移不影响展示。

### 10.2 P5：日程更新与取消（自主性闭环）

P2/P4 只覆盖「建 + 查」，Agent 建错了、要改时间或撤销时仍只能走人工管理接口。P5 补齐
修改闭环：`cancel_schedule`（软删、幂等）与 `update_schedule`（只开放标题/描述/时间/窗口/
`max_runs`）两个双宏工具，身份从 ctx 固化，consumer 可直接复用领域方法。

关键约定：

- **更新统一走 domain**：`CronManager::update_trigger` 与 `create_trigger` 共用
  `validate_trigger`，并在时间三要素变化时按 `cron > interval > run_at` 重算 `next_run_at`
  ——否则「改了规则但游标还是旧值」会表现为下一次仍按旧规则触发。
- **时间三选一整组替换**：更新时给了三者之一就覆盖全部三处（未给=清除），避免
  `cron > interval` 优先级吃掉新值。
- **软删不可经更新复活**：`is_enabled=0` 的记录只能 `resume` 显式恢复。
- **归属硬门**：取消/更新都只放行自己的异步日程，`SyncMaintenance` 与他人的日程
  统一 `NotFound`；身份/行为/统计字段一律不接受修改。

详见 `docs/modules/05-任务与项目/B-落地方案/统一定时调度器P5落地方案.md`。

---

## 10.3 P6：执行记录统计与管理端详情页

**口径：按 attempt 一条 attempt 留痕，与 `total_runs` 严格分工。** `cron_triggers.total_runs`
是配额游标（只在成功推进时 +1，控制 `max_runs`），不能反映失败/跳过；执行记录落
DuckDB 独立表 `trigger_exec_events`，每次 `CronTriggerConsumer::on_event` 收尾都记一条，
三态 `status = success / skipped / failed`：

- `success`：派发成功。`dispatched_count` 记**实际下游动作数**（沉淀请求数、跟进消息数、
  重投数、清理条目数……），不是常量 1；若部分子动作失败（发送失败、记账失败、
  collection 失败），status 仍为 success（不触发重捞），但摘要写进 `skip_reason` 以免
  把「部分失败」伪装成「全成功」。
- `skipped`：配置/条件不满足主动放弃且**不重试**（缺 `owner_user_id` / `agent_id`、
  未知 action、系统维护 payload 非法（只告警不重捞）、清理保留天数为 0），带 `skip_reason`。
- `failed`：抛错，交给下轮重捞（`decide_retry=Retry`，不 mark → 下个 tick 重捞；
  仅瞬时错误走这里，持久性配置错误归 `skipped`，避免每 60s 无限重试）。

**打点覆盖**：`execute_attempt` 把 payload 解析等前置错误也纳入 attempt，所有
`on_event` 之后的错误路径都有统计；只有事件本身反序列化失败（连 trigger_id 都拿不到）
例外。用户系统级事件不再重复写内存 AOP 收集器（`AopStatsCollector` 对 `cron.trigger`
跳过计数），attempt 级统计以本表为唯一出口，链路日志仍保留。

**落盘沿用 Stats 三重保障**：攒满 `batch_size=100` / 周期 30s / 优雅退出 `flush_all`。
低频日程长期攒不满也能在 30s 内可见；代价是查询有 ≤30s 延迟、非优雅退出丢最近窗口，
对「执行留痕/健康度」可接受（若将来做对账/计费才需改同步落盘）。打点失败不改写业务
结论（用 `let _ =` 忽略，避免把成功变失败触发无谓重捞）。

**查询与归属**：`GET /cron-triggers/{id}/exec-records` 只返回明细分页 + 总数；
`GET /cron-triggers/{id}/exec-stats` 独立返回概览（总数/三态/平均耗时）与时序桶分布
（桶宽 1~1440 分钟，默认 60 分钟）。两者都经 `handler → domain CronManager → dal →
dao stats`（DuckDB 独立 DAO，不混入 SQLite CRUD）。handler 先 `get_trigger` 确认 trigger
存在并解析归属，组织管理员只能读本组织 trigger 的记录（系统级 `org_id=None` 全局可见），
DAO 的 `WHERE` 同时带 `trigger_id + org_id + 时间窗`，防止跨租户越权。
handler 归一化时间窗（缺省最近 7 天）与分页（默认 50、上限 200）。前端新增
`/system/triggers/:id` 详情页：详情信息 + 编辑（日程开放名称/时间规则/说明/payload/
生效窗口/次数/负责 Agent；系统项开放名称/时间规则/payload）+ 执行统计（趋势折线 +
状态环形图）+ 执行明细分页；统计与明细两个接口独立拉取、独立刷新。列表操作列
「编辑」改为「详情」入口，暂停/恢复/删除仍在列表页。

---

## 11. 明确不做的边界

- 不做毫秒级精度：Producer 仍 60s 轮询，日程最小粒度是分钟。
- 不做日历同步/RSVP/参与者冲突检测：日程是「到点做事」，不是会议系统。
- 不做动态消费者注册：`behavior` 只能选预注册模板，运行时不改 AOP 声明。
- 不在 `cron_triggers` 上直接建跨表外键：域间隔离靠 ID + Domain 校验，不靠 DB cascade。
- 不复用 `agent_rest` / `AgentSettleEvent` 做日程唤醒：它只能跑 Settle run（已确认）。

---

## 12. 遗留风险与待拍板

1. **租约 vs CAS**：选短租约，代价是「领取成功但进程崩溃」要等租约到期才重跑。租约时长需按最坏消费时间设定，且 `on_consumed` 必须幂等。
2. **payload 兼容**：存量记录的 `action` 在 payload 内，顶层 `action` 为 NULL；DAL 回落解析必须覆盖所有 6 个系统 action，否则升级即失效。
3. **`Schedule` 与时间字段的组合校验**：三选一校验必须强制，否则会出现「Schedule 但没有时间」的僵尸记录。
4. **消息类型**：`AsyncAgentWakeup` 是复用 `TaskAssignment(9)` 还是新增 `ScheduleTrigger(15)`，需在 P2 前拍板；前者零改动但语义混用，后者需改路由白名单。
5. **文档回填**：本文落地后必须同步更新 C-知识（定时任务管理、定时触发生产者、定时任务消费者）并重跑 `make wiki-index`；现有知识卡对「2 条默认触发器」「CAS 乐观锁」「5 字段 cron」的描述与代码不符，属独立待修项。
