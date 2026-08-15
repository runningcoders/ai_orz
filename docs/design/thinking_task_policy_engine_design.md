# Agent 思考运行时 + 策略引擎设计

> 🎯 **本文档定位**：Agent 思考运行时（AgentThinkRuntime）收敛 + 策略引擎组件（pkg/policy/）的整体设计大纲与关键决策思路；设计评审定稿快照，接口细节与宏展开以实际代码为准。
> 状态：v2.0（2026-08-14 已评审，2026-08-15 整理，2026-08-17 wiki/RAG 互引回填）
> 触发场景：需要理解思考循环可观察性设计动机、策略引擎解耦哲学、policy_set! 声明宏组合模式、前端轮询+取消链路边界时打开；字段级 trait 定义与策略实现体直接读代码。
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 项目整体分层架构与开发规范
> - [runtime_design.md](./runtime_design.md) — 两阶段唤醒 Runtime 设计（策略引擎的上游调用方）
> - [tool_design.md](./tool_design.md) — 工具调用架构（策略判断结果影响工具执行链路）
> - 【③ Wiki 百科长文 系统化上下文必读】
>   - [运行时领域.md](docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md) — §5 策略引擎集成与思考运行时挂载子章节
>   - [Runtime 领域编排.md](docs/wiki/zh/content/架构设计/分层架构设计/Domain%20层编排/Runtime%20领域编排.md) — §5 RuntimeDomain 3 个观测/编排接口小节
>   - [思考运行时面板观测接口.md](docs/wiki/zh/content/前端应用/组件系统/业务组件/思考运行时面板观测接口.md) — 前端 runtime 面板 + 3 Handler 对接
>   - [思考轮次统计消费者.md](docs/wiki/zh/content/基础设施/AOP%20事件系统/事件消费者/思考轮次统计消费者.md) — exit_reason + ThinkRoundEvent DuckDB 入库
> - 【④ RAG 原子知识卡（总结+索引）— 读 §4 硬约束 → §2 关键文件表 → §3 架构约定 → §1 概述】
>   - [策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法](docs/wiki/knowledge/zh/策略引擎：Policy%20trait%20+%20PolicyGroup%20嵌套组合%20+%20policy_set!%20宏声明式写法/策略引擎：Policy%20trait%20+%20PolicyGroup%20嵌套组合%20+%20policy_set!%20宏声明式写法.md)
>   - [Agent 思考运行时 AgentThinkRuntime：挂载清理取消与每轮快照上报](docs/wiki/knowledge/zh/Agent%20思考运行时%20AgentThinkRuntime：挂载清理取消与每轮快照上报/Agent%20思考运行时%20AgentThinkRuntime：挂载清理取消与每轮快照上报.md)
>   - [思考运行时前端观测：runtime-status cancel-thinking runtime-list 接口与 runtime_panel 组件](docs/wiki/knowledge/zh/思考运行时前端观测：runtime-status%20cancel-thinking%20runtime-list%20接口与%20runtime_panel%20组件/思考运行时前端观测：runtime-status%20cancel-thinking%20runtime-list%20接口与%20runtime_panel%20组件.md)
>   - [思考退出原因 exit_reason 统计与 ThinkRoundEvent AOP 事件链路](docs/wiki/knowledge/zh/思考退出原因%20exit_reason%20统计与%20ThinkRoundEvent%20AOP%20事件链路/思考退出原因%20exit_reason%20统计与%20ThinkRoundEvent%20AOP%20事件链路.md)
>   - [两阶段唤醒：IntentAnalyze Phase1 意图分析 + Awaken Phase2 正式执行串联（IntentAnalysis 7 字段 + 6 级 JSON 降级）](docs/wiki/knowledge/zh/两阶段唤醒：IntentAnalyze%20Phase1%20意图分析%20+%20Awaken%20Phase2%20正式执行串联（IntentAnalysis%207%20字段%20+%206%20级%20JSON%20降级）/两阶段唤醒：IntentAnalyze%20Phase1%20意图分析%20+%20Awaken%20Phase2%20正式执行串联（IntentAnalysis%207%20字段%20+%206%20级%20JSON%20降级）.md) — policy_set!(IntentAnalyze) 场景策略由本卡策略引擎驱动
>   - [AgentRuntimeInfo 三态状态机 + BusyGuard RAII：Idle Busy Resting 转换 + task_id project_id 业务上下文透视](docs/wiki/knowledge/zh/AgentRuntimeInfo%20三态状态机%20+%20BusyGuard%20RAII：Idle%20Busy%20Resting%20转换%20+%20task_id%20project_id%20业务上下文透视/AgentRuntimeInfo%20三态状态机%20+%20BusyGuard%20RAII：Idle%20Busy%20Resting%20转换%20+%20task_id%20project_id%20业务上下文透视.md) — 思考运行时挂在 Busy 态 runtime_info.think_runtime 上（§103 业务上下文字段决策）

---

## 一、问题背景

### 1.1 现状

当前 Agent 思考循环（`run_think_loop`）的控制逻辑**硬编码在业务代码中**：

| 控制点 | 实现位置 | 方式 |
|--------|---------|------|
| 轮次上限 | `run_think_loop` 的 `for offset in 0..available_rounds` | 循环耗尽 → `MaxRoundsExceeded` |
| 超时 | `run_think_loop` 的 `tokio::time::timeout` 包裹整个 think_future | 超时 → `Err` |
| 上下文溢出 | `run_think_loop` 的 `input_tokens >= threshold` 判断 | 溢出 → `ContextOverflow` |
| 配置解析 | `config_resolve` 模块 | Agent > system > 硬编码 |

这些控制逻辑分散在 `awakening.rs`（2200 行）中，与业务流程耦合，存在以下问题：

### 1.2 痛点

1. **不可扩展**：新增控制策略（如 token 预算、成本上限、工具调用次数限制）需要修改 `run_think_loop` 核心循环，影响面大
2. **不可监控**：`AgentRuntimeStateManager` 只记录三态（Idle/Busy/Resting）+ `current_message_id`，无法查看思考运行时信息（当前轮次、token 消耗、正在调用的工具）
3. **不可取消**：用户无法主动停止正在思考的 Agent，只能等待轮次耗尽或超时
4. **控制逻辑分散**：4 个场景（Awaken/Settle/Summary/IntentAnalyze）的控制逻辑各自实现，`ContextOverflow` 和 `MaxRoundsExceeded` 的处理方式不统一（有的返回 Err，有的返回空字符串兜底）

### 1.3 设计方向选择

经评审，**放弃将思考流程并入后台任务体系（BackgroundTask）**，原因：

1. **思考流程没有"进度"**：后台任务的核心特征是进度（0-100% 或 step 1/10），而思考流程只有"当前在哪个场景的第几轮"这种运行时状态，不是任务进度。强行套用 `TaskProgressSnapshot` 是语义错配。
2. **思考流程更像是 Agent 运行时的一部分**：前端展示时只在 Agent 相关页面上展示运行状态，跟着运行时信息一起走。状态流转跟着 Agent 状态走——Agent 到了休息状态就进入休息任务，到了某个状态就进入对应的任务中。
3. **闭环更简单**：直接在 Agent 运行时这里完成闭环，不感知后台任务体系。只需要在 Agent 的运行时上报里增加一些字段即可。

**最终方案**：思考运行时实体（`AgentThinkRuntime`）收敛到 `AgentRuntimeStateManager`，策略引擎作为独立组件提供控制逻辑判断能力。

---

## 二、设计目标

1. **策略化**：把控制逻辑抽象为 `Policy` trait，策略引擎统一调度，新增策略不改核心循环
2. **可监控**：思考循环每轮上报运行时信息，通过 Agent 运行时状态接口查看正在思考的 Agent + 丰富运行时信息
3. **可取消**：用户通过接口取消正在思考的 Agent，循环优雅停止
4. **运行时闭环**：思考运行时作为 `AgentRuntimeStateManager` 的扩展，跟着 Agent 状态走，完整流程结束后清理
5. **前端集成**：Agent 对话页面展示实时运行时，支持取消操作

---

## 三、核心架构

两层设计，职责清晰分离：

```
┌─────────────────────────────────────────────────────────────────┐
│  接口层（Handler + SSE）                                         │
│  ├── GET  /agents/{id}/runtime-status   实时运行时状态 + 思考信息 │
│  ├── POST /agents/{id}/cancel-thinking  取消正在思考             │
│  ├── GET  /agents/runtime-list          所有运行中 Agent 列表    │
│  └── SSE  /messages/sse                 增量推送思考运行时事件    │
└───────────────────────────┬─────────────────────────────────────┘
                            │ 查询 / 取消
                            ▼
┌─────────────────────────────────────────────────────────────────┐
│  Agent 运行时层（AgentRuntimeStateManager + AgentThinkRuntime） │
│  ├── AgentRuntimeInfo 扩展 think_runtime 字段                    │
│  │   ├── 持有 CancellationToken（cancel 信号）                   │
│  │   ├── 持有 Arc<RwLock<AgentThinkRuntime>>（运行时快照）       │
│  │   └── 跟着 Agent 状态走：Busy 时创建，Idle 时清理             │
│  └── cancel(agent_id) 直接操作 think_runtime.cancel_token       │
└───────────────────────────┬─────────────────────────────────────┘
                            │ 每轮上报运行时 / 检查 cancel
                            ▼
┌─────────────────────────────────────────────────────────────────┐
│  策略引擎层（pkg/policy/：独立组件）                             │
│  ├── Policy trait + 内置策略实现                                 │
│  ├── PolicyGroup：策略组（实现 Policy trait，支持 And/Or 组合）  │
│  ├── PolicyBuilder：按场景构造策略组（build=And / or=Or）        │
│  ├── 每个 think_loop 创建一个策略组实例（Box<dyn Policy>）        │
│  ├── 每轮结束后：think_runtime.report_round(metrics)             │
│  ├── 下一轮开始前：policy.evaluate(&metrics) → Vec<String>       │
│  │   ├── 空列表     → 继续下一轮                                 │
│  │   └── 非空列表   → map_triggered_to_result → ThinkLoopResult  │
│  └── 内置策略：MaxRounds / Timeout / ContextOverflow /           │
│                 TokenBudget（预留）/ UserCancel                  │
└─────────────────────────────────────────────────────────────────┘
```

**关键设计决策**：

| 决策 | 方案 | 理由 |
|------|------|------|
| 思考运行时归属 | `AgentRuntimeStateManager` 扩展，不并入 BackgroundTask | 思考流程没有"进度"概念，只有运行时状态；跟着 Agent 状态走，前端展示跟着运行时信息一起走；闭环更简单 |
| 策略引擎位置 | `pkg/policy/`（纯框架层，独立组件） | 策略引擎是通用框架，提供 trait 定义 + 实现 + builder + 策略组结构体 + 计算方法；符合 AGENTS.md 3.2.1 基础设施约定 |
| 策略引擎粒度 | 每个 think_loop 一个策略组实例（`Box<dyn Policy>`） | 不同场景（Awaken/Settle/Summary/IntentAnalyze）策略集不同，且 think_loop 是控制逻辑的实际作用域 |
| cancel 信号 | `tokio_util::sync::CancellationToken` | tokio 生态标准方案，支持 select! 协作式取消，零成本未触发时 |
| 运行时信息存储 | `AgentThinkRuntime` 持有 `Arc<RwLock<...>>`，think_loop 每轮写入 | 运行时快照原子读写，StateManager 直接读取，无需额外 IPC |
| **运行时清理** | **完整思考流程结束后清理 think_runtime** | 避免运行时信息泄漏；BusyGuard Drop 时同步清理 think_runtime，与 set_idle 一起完成 |
| **业务上下文字段归属** | `message_id` / `task_id` / `project_id` 放在 `AgentRuntimeInfo`（生命周期级），**不放 `AgentThinkRuntime`** | 整个 Busy 期间这些字段不变；一个 Busy 可能经历 awaken→settle→summary 多个 think_loop 实例，放 think_runtime 会重复设置且 settle/summary 场景易丢失；放 AgentRuntimeInfo 在 `set_busy` 时一次性设置，前端可按任务/项目视角过滤运行中 Agent |

### 3.1 Agent 运行时闭环：状态管理 + 思考运行时

**核心认知**：`AgentThinkRuntime` 是 `AgentRuntimeStateManager` 的运行时扩展，不是独立的后台任务。思考运行时跟着 Agent 状态走。

```
时间轴 ─────────────────────────────────────────────────────────►

AgentRuntimeStateManager（Agent 生命周期级）
  Idle ──────────────► Busy ──────────────────────────────► Idle ────►
                      │  current_message_id = msg-123       │
                      │  task_id = task-456                  │
                      │  project_id = proj-789               │
                      │  state_started_at = T1              │
                      │  think_runtime = Some(...)          │  think_runtime = None
                      │    ├── scene: Awaken                │  （清理）
                      │    ├── round: 5/365                 │
                      │    ├── tokens: 12,450               │
                      │    └── cancel_token                 │
                      └─ BusyGuard 创建 think_runtime       └─ BusyGuard Drop 清理 think_runtime
```

| 维度 | AgentRuntimeState | AgentThinkRuntime |
|------|-------------------|-------------------|
| **粒度** | Agent 生命周期级（粗） | 单次思考级（细） |
| **连续性** | 连续，覆盖整个生命周期 | 跟着 Busy 状态走，Busy 期间存在 |
| **互斥性** | 互斥（Idle/Busy/Resting 三选一） | 跟随状态，Busy 时有值，其他状态无值 |
| **所有权** | consumer（BusyGuard RAII） | StateManager 持有，BusyGuard 创建/清理 |
| **业务上下文** | `message_id` / `task_id` / `project_id`（set_busy 时一次性设置） | 无（业务上下文不随思考轮次变化） |
| **回答的问题** | "能接受新消息吗"+"在为哪个任务/项目工作" | "当前思考进展如何" |
| **取消语义** | 不存在（状态不是任务） | 明确（CancellationToken） |

**运行时信息生命周期**：

```
consumer.on_event(msg):
    try_set_busy(agent_id, message_id, task_id, project_id)      ← 业务上下文一次性设置
    BusyGuard::new()
    think_runtime = AgentThinkRuntime::new(agent_id, trace_id)   ← 创建
    state_manager.set_think_runtime(agent_id, think_runtime)     ← 挂载
    awaken(ctx, agent, message, think_runtime).await:            ← 传入
        ... think loop ...
        think_runtime.report_round(...)                          ← 每轮上报
        think_runtime.is_cancelled() 检查                        ← 每轮检查
    BusyGuard Drop:
        set_idle(agent_id)                                       ← 状态回归（含 task_id/project_id 清理）
        state_manager.clear_think_runtime(agent_id)              ← 清理运行时
```

**cancel 流程**：状态管理对 cancel 完全无感知——它只看到 awaken 返回了，BusyGuard 正常 Drop 并清理 think_runtime。

```
用户取消 → state_manager.cancel_thinking(agent_id)
    → think_runtime.cancel_token.cancel()
    ↓（异步）
think_loop 下一轮检测到 → 返回 Cancelled
    ↓
awaken() 返回（正常流程出口）
    ↓
BusyGuard::Drop → set_idle + clear_think_runtime  ← 状态管理和运行时清理一起完成
```

---

## 四、策略引擎设计

### 4.1 Policy trait

```rust
// src/pkg/policy/mod.rs

/// 策略 trait（通用判断引擎，不感知业务 action）
///
/// 设计要点：
/// - evaluate 返回命中的策略 id 列表（空 = 未命中，非空 = 命中）
/// - is_triggered 是 trait 级默认方法，基于 evaluate 判断
/// - 策略不响应 action，action 映射由业务侧处理
pub trait Policy: Send + Sync + 'static {
    /// 策略唯一 ID（如 "max_rounds" / "timeout" / "context_overflow"）
    fn id(&self) -> &str;

    /// 策略名称（人类可读，用于前端展示）
    fn name(&self) -> &str;

    /// 策略条件描述（如 "轮次 >= 365"）
    fn condition_desc(&self) -> &str;

    /// 声明关注的算子名称（文档化依赖，开发时可校验 Metrics 是否包含）
    fn required_metrics(&self) -> Vec<String>;

    /// 评估：返回命中的策略 id 列表
    /// 空列表 = 未命中，非空 = 命中（可能多个策略同时命中）
    fn evaluate(&self, metrics: &Metrics) -> Vec<String>;

    /// 默认方法：是否命中（列表非空）
    fn is_triggered(&self, metrics: &Metrics) -> bool {
        !self.evaluate(metrics).is_empty()
    }
}
```
> 当前实现参考：[policy_engine 模块](src/service/domain/runtime/policy_engine.rs)

### 4.2 Metrics

> 相关实现细节见：[runtime/think_loop.rs](src/service/domain/runtime/think_loop.rs)

**think_loop 构造 Metrics 示例**：

> 相关实现细节见：[runtime think_loop + policy](src/service/domain/runtime/)

### 4.3 PolicyGroup

策略组本身实现 Policy trait，支持 And/Or 组合关系，可嵌套。所有派生字段（id/name/condition_desc/required_metrics）从子策略自动拼接生成。

> 相关实现细节见：[policy engine 模块](src/service/domain/runtime/policy_engine.rs)

**自动生成效果**：

> 相关实现细节见：[pkg/agent_runtime_state.rs](src/pkg/agent_runtime_state.rs)

### 4.4 PolicyBuilder + policy_set! 宏

#### PolicyBuilder（底层 API）

> 相关实现细节见：[policy engine 模块](src/service/domain/runtime/policy_engine.rs)

#### policy_set! 宏（推荐用法）

声明宏，一步完成"策略初始化 + 组装 + 关系指定"，消除 `Box::new(XxxPolicy::new(...))` 样板。
约定内置策略通过 `::new` 构造，宏自动调用 `$Policy::new(args...)`。

**三种模式**：

> 相关实现细节见：[policy engine 模块](src/service/domain/runtime/policy_engine.rs)

**底层 API 使用场景**（PolicyBuilder 直接使用）：

> 相关实现细节见：[policy engine 模块](src/service/domain/runtime/policy_engine.rs)

> 💡 宏内部使用 TT munching 递归处理混合模式的条目，通过 `policy_set_mixed!` 辅助宏逐条匹配（平铺策略 / OR 子组 / AND 子组）。

### 4.5 内置策略

> 相关实现细节见：[policy engine 模块](src/service/domain/runtime/policy_engine.rs)

### 4.6 业务侧 action 映射

策略引擎不感知业务 action，业务侧维护策略 id → ThinkLoopResult 映射：

> 相关实现细节见：[runtime/think_loop.rs](src/service/domain/runtime/think_loop.rs)

### 4.7 按场景构造策略组

通过 `config_resolve` 从 Agent 配置 + 系统配置解析参数，`policy_set!` 宏构造策略组：

> 相关实现细节见：[runtime/think_loop.rs](src/service/domain/runtime/think_loop.rs)

> 💡 当前实现中所有场景共用同一套策略组（UserCancel OR MaxRounds OR Timeout），ContextOverflowPolicy 暂未启用（run_think_loop 已有独立的上下文溢出检测逻辑，后续可整合）。

**场景策略矩阵（设计目标）**：

| 场景 | UserCancel | ContextOverflow | MaxRounds | Timeout | 触发后处理 |
|------|:---:|:---:|:---:|:---:|------|
| `Awaken` | ✅ | ✅（待整合） | ✅ | ✅ | ContextOverflow → sleep_and_settle；MaxRounds/Timeout/TokenBudget → awaken_for_summary；Cancelled → 清理退出 |
| `Settle` | ✅ | ❌ | ✅ | ✅ | 所有触发 → 兜底返回空字符串（现有行为） |
| `Summary` | ✅ | ❌ | ✅ | ✅ | 所有触发 → 兜底返回空字符串（现有行为） |
| `IntentAnalyze` | ✅ | ❌ | ✅ | ✅ | 所有触发 → 返回 Err（现有行为，外层降级为 None） |

---

## 五、AgentThinkRuntime 设计

### 5.1 AgentThinkRuntime

> 相关实现细节见：[runtime/think_loop.rs](src/service/domain/runtime/think_loop.rs)

**设计要点**：
- 不实现 BackgroundTask trait，不注册到 Registry
- 持有 `Arc<RwLock<ThinkRuntimeSnapshot>>`，think_loop 每轮写入，StateManager 直接读取
- cancel_flag 由 think_loop 通过 `policy_set! { OR { UserCancelPolicy(cancel_flag), ... } }` 注入策略组
- 主体循环逻辑保持现状，只在每个 think_loop 调用点增加 `think_runtime.report_round()` 和 `think_runtime.is_cancelled()` 检查

### 5.2 AgentRuntimeStateManager 扩展

> 相关实现细节见：[pkg/agent_runtime_state.rs](src/pkg/agent_runtime_state.rs)

**BusyGuard 扩展**：Drop 时同步清理 think_runtime。

> 相关实现细节见：[runtime think_loop + policy](src/service/domain/runtime/)

### 5.3 trace_id 生成机制（决策基础）

**现状（基于代码查证）**：一次完整 awaken 流程包含 4 个子场景，各自生成独立的 trace_id：

| 场景 | 生成方式 | 格式 | 代码位置 |
|------|---------|------|---------|
| **IntentAnalyze**（Phase 1） | 字符串拼接，不建 MemoryTrace | `intent-analyze-{ctx.log_id}` | awakening.rs#L1145 |
| **Awaken**（主循环 Phase 2） | `MemoryTrace::new()` | `trace-{agent_id}-{timestamp_nanos}-{random_u16}` | awakening.rs#L546 |
| **Settle**（上下文压缩） | `MemoryTrace::new()` 新建 | `trace-{agent_id}-{timestamp_nanos}-{random_u16}` | awakening.rs#L954 |
| **Summary**（总结退出） | `MemoryTrace::new()` 新建，log_id 用 `summary-{parent}` | `trace-{agent_id}-{timestamp_nanos}-{random_u16}` | awakening.rs#L1329 |

**复用规则**：run_think_loop 内部所有轮次复用同一个 trace_id，但跨子流程会生成新的。

**MemoryTrace 维度（决策）**：MemoryTrace 记录的维度是「一次完整的 think_loop 流程」，而非单次 LLM 调用。一个 think_loop 可能包含多轮 LLM 调用（如多轮工具调用循环），这些轮次共享同一个 trace_id，最终合并为一条 MemoryTrace（`input` = 完整 Prompt，`output` = 最终模型返回，`completed_at` = 流程结束时间）。跨子流程（IntentAnalyze → Awaken → Settle → Summary）会生成新的 MemoryTrace，各自独立持久化。

| 维度 | 粒度 | 对应 trace_id | 说明 |
|------|------|---------------|------|
| **MemoryTrace** | 一次完整 think_loop 流程 | 一个 trace_id 对应一条 trace | 含多轮 LLM 调用，合并为一条记录 |
| **ThinkRoundEvent** | 单轮思考 | 关联当前 trace_id | 每轮发布一次，用于实时监控 |
| **LLM 调用** | 单次模型请求 | 不独立持久化 | 轮次内的 brain_dal.think() 调用，日志记录即可 |

```
一次完整 awaken 流程的 trace_id 演变：

IntentAnalyze:  intent-analyze-{log_id}        ← Phase 1，独立 trace_id
    ↓
Awaken 主循环:   trace-{agent}-{ts1}-{rand1}   ← Phase 2，所有轮次复用
    ↓ (ContextOverflow 触发)
Settle:          trace-{agent}-{ts2}-{rand2}   ← 新 trace_id，通过 pending_trace_ids 关联回 awaken
    ↓ (回到 Awaken 主循环，仍用 ts1)
    ↓ (Final 或 MaxRoundsExceeded 触发)
Summary:         trace-{agent}-{ts3}-{rand3}   ← 新 trace_id，log_id = summary-{awaken_trace_id}
```

**对设计的影响（决策依据）**：
1. trace_id 在一次 awaken 流程中不是单一的——IntentAnalyze、Awaken、Settle、Summary 各有自己的 trace_id
2. **这印证了思考运行时归属 AgentRuntimeStateManager 的正确性**：用 agent_id 作 key，trace_id 只是运行时快照中的一个动态字段
3. **AgentThinkRuntime 初始 trace_id**：用 awaken 主流程的 trace_id（Phase 2 的 trace_id）初始化
4. **ThinkRuntimeSnapshot.trace_id 字段**：动态更新，每轮上报时写入当前 think_loop 的 trace_id。前端看到的 trace_id 会随场景切换变化（`intent-analyze-xxx` → `trace-xxx` → `trace-yyy`），这样前端查日志时拿到的是当前阶段的正确 trace_id

### 5.4 ThinkRuntimeSnapshot 完整定义

> 相关实现细节见：[runtime/think_loop.rs](src/service/domain/runtime/think_loop.rs)

**注意**：`ThinkRuntimeSnapshot` **不实现** `TaskProgressSnapshot`，因为思考流程没有进度概念。它是独立的运行时快照结构，通过 `AgentRuntimeStatusResponse` 返回给前端。

---

## 六、控制信号流

### 6.1 正常思考流程

```
consumer/message.rs on_event(message)
    │
    ├── try_set_busy(agent_id, message_id)
    ├── BusyGuard::new()
    ├── 创建 AgentThinkRuntime（持有 cancel_token + 运行时快照）
    ├── state_manager.set_think_runtime(agent_id, think_runtime)  ← 挂载到运行时
    │
    └── awaken(ctx, agent, message, think_runtime).await:         ← 传入 think_runtime
        │
        ├── Phase 1: analyze_input_intent_inner
        │   ├── build_policy_for_scene(options, cancel_token)
        │   ├── run_think_loop 改造：
        │   │   loop {
        │   │       1. 构造 Metrics
        │   │       2. policy.evaluate(&metrics) → Vec<String>（命中策略 id 列表）
        │   │          ├── 空 → 执行本轮 think
        │   │          └── 非空 → map_triggered_to_result(&triggered) 返回对应 ThinkLoopResult
        │   │       3. brain_dal.think() → ThinkResult
        │   │       4. 处理 ThinkResult（Final/ToolCall）
        │   │       5. think_runtime.report_round(trace_id, scene, round, max, metrics)
        │   │       6. 发布 ThinkRoundEvent（现有逻辑保留）
        │   │   }
        │   └── 返回 IntentAnalysis 或 None（降级）
        │
        ├── Phase 2: awaken loop（同上改造）
        │   ├── ContextOverflow → sleep_and_settle（build 新 policy）
        │   ├── MaxRoundsExceeded → awaken_for_summary（build 新 policy）
        │   └── Final → awaken_for_summary（统一总结流程）
        │
        └── 完成 → BusyGuard Drop → set_idle + clear_think_runtime  ← 状态回归 + 运行时清理
```

### 6.2 用户取消流程

```
用户点击"取消思考"按钮
    │
    ▼
POST /api/v1/hr/agents/{agent_id}/cancel-thinking
    │
    ├── Handler 调用 domain.runtime().cancel_thinking(ctx, agent_id)
    │   └── domain 内部调用 state_manager.cancel_thinking(agent_id)
    │       └── think_runtime.cancel() → cancel_token.cancel()
    │
    └── 返回 { success: true, message: "取消信号已发送" }
        │
        ▼（异步，不阻塞 HTTP 响应）
    think_loop 下一轮开始前：
        policy.evaluate(&metrics)
            └── UserCancelPolicy.evaluate() 返回 vec!["user_cancel"]
        │
        ▼
        map_triggered_to_result 返回 ThinkLoopResult::Cancelled
        │
        ▼
    run_think_loop 返回 ThinkLoopResult::Cancelled
        │
        ▼
    awaken 主流程收到 Cancelled：
        ├── 不触发总结退出（用户主动放弃，无需总结）
        ├── 发布 AgentLoopEvent::finished(scene, status="cancelled")
        ├── 记录 AgentAwakeEvent（exit_reason="cancelled"）
        └── 返回 AwakeningResult（raw_output 可能为空）
        │
        ▼
    awaken() 返回 → BusyGuard::Drop → set_idle + clear_think_runtime  ← 状态回归 + 运行时清理
```

**cancel API 的 domain 归属（决策）**：取消接口归属于 **runtime domain**（`domain.runtime()`），而非 agent domain 或 message domain。

**理由**：
- `AgentRuntimeStateManager` 属于 runtime domain 管辖，`AgentThinkRuntime` 是其扩展
- cancel 操作的本质是操作运行时状态（触发 cancel_token），不涉及 Agent 实体或消息实体的业务变更
- 事件发布和统计记录由 awaken 主流程感知到 Cancelled 后自己处理（现有流程不变）

**调用链**：

```
Handler (POST /agents/{id}/cancel-thinking)
    └── domain.runtime().cancel_thinking(ctx, agent_id)
        └── state_manager.cancel_thinking(agent_id)
            └── think_runtime.cancel()  → cancel_token.cancel()
```

**domain 层接口定义**：

```rust
// src/service/domain/runtime/mod.rs
pub trait RuntimeDomain: Send + Sync {
    /// 取消正在思考的 Agent
    /// 返回 false 表示 Agent 当前非 Busy 或无 think_runtime（无需取消）
    fn cancel_thinking(&self, ctx: RequestContext, agent_id: &str) -> bool;

    /// 查询 Agent 实时运行时状态（含思考运行时快照）
    fn get_runtime_status(&self, ctx: RequestContext, agent_id: &str) -> AgentRuntimeInfo;

    /// 查询所有运行中 Agent
    fn list_busy_agents(&self, ctx: RequestContext) -> Vec<AgentRuntimeInfo>;
}
```
> 当前实现参考：[runtime policy + think_loop](src/service/domain/runtime/think_loop.rs)

**职责分工**：cancel 的"信号触发"在 runtime domain（操作 cancel_token），"信号响应"在 think_loop（下一轮检测到取消后返回 Cancelled）。Handler 只做参数校验和调用 domain，不感知 cancel 实现细节。

### 6.3 运行时信息上报与查询

```
think_loop 每轮结束
    │
    └── think_runtime.report_round(trace_id, scene, round, max_rounds, metrics)
        └── 写入 Arc<RwLock<ThinkRuntimeSnapshot>>
            ├── trace_id（动态切换到当前子流程）
            ├── scene / round_number / max_rounds
            ├── tokens_input / tokens_output / total_tokens
            ├── tool_call_count / elapsed_secs
            └── step_message = "Awaken round 5/365"

前端查询：
    GET /api/v1/hr/agents/{agent_id}/runtime-status
        └── state_manager.get_think_runtime_snapshot(agent_id)
            └── 返回 ThinkRuntimeSnapshot（与 runtime_state 一起返回）
```

**完整思考流程结束后清理**：

```
awaken() 返回（正常完成/失败/取消）
    ↓
BusyGuard::Drop:
    ├── set_idle(agent_id)                    ← 状态回归 Idle
    └── clear_think_runtime(agent_id)         ← 清理 think_runtime（避免泄漏）
    ↓
AgentRuntimeInfo.think_runtime = None         ← 后续查询返回 None
```

---

## 七、思考循环改造

### 7.1 run_think_loop 签名变化

> 相关实现细节见：[runtime/think_loop.rs](src/service/domain/runtime/think_loop.rs)

### 7.2 ThinkLoopResult 新增 Cancelled 变体

```rust
pub enum ThinkLoopResult {
    Final { content: String, messages: Vec<ChatMessage> },
    ContextOverflow { messages: Vec<ChatMessage>, input_tokens: u64, rounds_used: usize },
    MaxRoundsExceeded { messages: Vec<ChatMessage>, total_rounds: usize },
    Cancelled { messages: Vec<ChatMessage>, total_rounds: usize },  // 新增
}
```
> 当前实现：[runtime/types.rs](src/service/domain/runtime/types.rs)

### 7.3 循环体改造（伪代码）

> 相关实现细节见：[runtime/think_loop.rs](src/service/domain/runtime/think_loop.rs)

**改造要点**：
- `max_rounds` 和 `timeout_secs` 控制逻辑被 `policy: &dyn Policy` 替代，控制逻辑从硬编码变为策略驱动
- `max_rounds` 参数保留，仅用于运行时展示（`report_round` 的 `step_message`），不参与控制判断
- `timeout` 不再用 `tokio::time::timeout` 包裹整个 future，改为 `TimeoutPolicy` 基于 `elapsed_secs` 判断（更精确，可单轮判断）
- 每轮结束后上报运行时（`think_runtime` 为 None 时跳过，兼容内部测试）
- `ThinkRoundEvent` 发布逻辑保留不变，策略引擎是额外增加的控制层

---

## 八、接口设计

### 8.1 后端 API

| 方法 | 路径 | 功能 | 响应 |
|------|------|------|------|
| GET | `/api/v1/hr/agents/{id}/runtime-status` | 查询 Agent 实时运行时状态 + 思考运行时 | `AgentRuntimeStatusResponse` |
| POST | `/api/v1/hr/agents/{id}/cancel-thinking` | 取消正在思考的 Agent | `CancelThinkingResponse { success, message }` |
| GET | `/api/v1/hr/agents/runtime-list` | 所有运行中 Agent 列表（含运行时） | `Vec<AgentRuntimeStatusResponse>` |
| GET | `/api/v1/hr/agents/{id}/policies` | 查询 Agent 当前生效的策略集 | `Vec<PolicyInfo>` |

```rust
// common/src/api/agent.rs 新增

#[derive(Serialize, Deserialize)]
pub struct AgentRuntimeStatusResponse {
    pub agent_id: String,
    pub runtime_state: AgentRuntimeState,     // Idle/Busy/Resting
    pub current_message_id: Option<String>,
    pub task_id: Option<String>,              // 业务上下文（set_busy 时设置）
    pub project_id: Option<String>,           // 业务上下文（set_busy 时设置）
    pub state_started_at: i64,

    // 思考运行时（仅 Busy 时有值，通过 state_manager.get_think_runtime_snapshot 查询）
    pub think_runtime: Option<ThinkRuntimeInfo>,
}

#[derive(Serialize, Deserialize)]
pub struct ThinkRuntimeInfo {
    pub agent_id: String,
    pub trace_id: String,            // 动态，当前 think_loop 的 trace_id
    pub scene: String,               // "intent-analyze" / "awaken" / "settle" / "summary"
    pub status: String,              // "running" / "completed" / "failed" / "cancelled"
    pub round_number: usize,
    pub max_rounds: usize,
    pub tokens_input: u64,
    pub tokens_output: u64,
    pub total_tokens: u64,
    pub tool_call_count: usize,
    pub elapsed_secs: u64,
    pub active_policies: Vec<PolicyInfo>,
    pub started_at: i64,
    pub step_message: String,
}

#[derive(Serialize, Deserialize)]
pub struct PolicyInfo {
    pub policy_id: String,
    pub name: String,
    pub condition_desc: String,
    pub result_desc: String,
}

#[derive(Serialize, Deserialize)]
pub struct CancelThinkingResponse {
    pub success: bool,
    pub message: String,
}
```

**Handler 实现要点**：
- `runtime-status`：调用 `state_manager.get(agent_id)` 获取 runtime_state + current_message_id，调用 `state_manager.get_think_runtime_snapshot(agent_id)` 获取思考运行时，组装成 `AgentRuntimeStatusResponse` 返回
- `cancel-thinking`：调用 `state_manager.cancel_thinking(agent_id)`，返回 `CancelThinkingResponse`
- `runtime-list`：遍历 `state_manager.list_busy_agents()`，每个 Agent 都获取运行时快照
- 一次查询拿全部运行时信息，前端无需关联两个数据源

### 8.2 思考运行时事件（纯前端轮询，不扩展 SSE）

**决策：不扩展 SSE 推送思考运行时事件，由前端轮询 `GET /agents/{id}/runtime-status` 获取。**

理由：
1. **语义不同**：现有 SSE 通道面向 message（用户需要看到的内容），思考运行时是 Agent 内部状态（轮次/token/工具调用），属于查看时才需要的运维视角信息
2. **完成通知已有天然通道**：思考完成 → 产出消息 → message SSE 自然推送，用户收到 message 事件即知思考结束，无需额外的 `ThinkingFinished` 事件
3. **简化复杂度**：不扩展 `SseEventType` 枚举，不在 think_loop 埋推送点，不设计推送频率控制策略，SSE 通道保持纯净

**前端轮询策略**：
- 进入 Agent 详情页/对话页时启动 3-5 秒轮询 `runtime-status`
- Agent 状态为 Idle 时停止轮询（或降频到 30 秒一次）
- 离开页面时停止轮询

---

## 九、前端方案

### 9.1 Agent 对话页面实时运行时

在 Agent 对话页面（或 Agent 详情页的「状态图」Tab 改造为「运行时」Tab）新增**实时运行时面板**：

```
┌─────────────────────────────────────────────────┐
│ 🧠 Agent 运行时                    [取消思考]    │
├─────────────────────────────────────────────────┤
│ 状态：🔵 思考中 (Awaken)                         │
│ 上下文：msg-123 · task-456 · proj-789           │
│ 进度：第 5/365 轮  ████████░░░░░░░  1.4%        │
│ 耗时：00:42  |  Token：12,450  |  工具调用：8    │
│                                                 │
│ 生效策略：                                       │
│  • 轮次上限：单次唤醒轮次超过 365 → 总结退出     │
│  • 超时限制：累计耗时超过 0 秒 → 总结退出        │
│  • 上下文溢出：上下文 token 超过 60000 → 压缩   │
│  • 用户取消：用户主动取消 → 清理退出             │
│                                                 │
│ 最近工具调用：                                   │
│  1. search_memory (1.2s) ✅                      │
│  2. query_knowledge_graph (0.8s) ✅              │
│  3. save_short_term_memory (0.3s) ✅             │
└─────────────────────────────────────────────────┘
```

**实现方式**：
- 前端轮询 `GET /agents/{id}/runtime-status`（3-5 秒一次），实时更新面板
- 「取消思考」按钮调用 `POST /agents/{id}/cancel-thinking`
- Agent 空闲时面板收起，仅显示「状态：空闲」，并停止轮询
- 上下文行（msg/task/project）来自 `AgentRuntimeInfo`，支持点击跳转到对应详情页

### 9.2 全局运行中 Agent 列表

在系统监控页面（或工作台）新增「运行中 Agent」卡片，支持按任务/项目视角过滤：

```
┌──────────────────────────────────────────────────┐
│ 🤖 运行中 Agent (3)         [全部] [按任务] [按项目] │
├──────────────────────────────────────────────────┤
│ Agent-A  Awaken  5/365  12,450t  task-456/proj-789 │
│ Agent-B  Settle  2/365   3,200t  —                  │
│ Agent-C  Summary 1/365     800t  task-456/proj-789 │
└──────────────────────────────────────────────────┘
```

**任务/项目视角**：`AgentRuntimeInfo` 在 `set_busy` 时记录 `task_id` / `project_id`，前端调用 `runtime-list` 接口后按这两个字段分组，即可实现"任务视角下哪些 Agent 在运行""项目视角下运行状态汇总"的视图，无需额外查询。

---

## 十、与现有系统的关系

| 现有系统 | 关系 | 改动 |
|---------|------|------|
| `AgentRuntimeStateManager` | **扩展** | `AgentRuntimeInfo` 新增 `think_runtime: Option<Arc<AgentThinkRuntime>>` 字段 + `task_id` / `project_id` 业务上下文字段；新增 `set_think_runtime` / `clear_think_runtime` / `cancel_thinking` / `get_think_runtime_snapshot` 方法；`set_busy` / `try_set_busy` 签名扩展接收 task_id / project_id |
| `BusyGuard` | 扩展 | Drop 时同步清理 think_runtime（与 set_idle 一起完成，task_id/project_id 随状态回归一并清理） |
| `BackgroundTask` trait | **不改** | 思考运行时不并入后台任务体系 |
| `BackgroundTaskRegistry` | **不改** | 思考运行时不注册到 Registry |
| `run_think_loop` | 改造 | `max_rounds/timeout_secs` 参数 → `policy: &dyn Policy`（控制逻辑）+ `max_rounds`（仅展示）；新增 `think_runtime: Option<&AgentThinkRuntime>` 参数 |
| `config_resolve` | 保留 | 策略参数仍从 config_resolve 获取，策略引擎在 think_loop 入口处用解析的配置通过 `policy_set!` 宏构建策略组 |
| `ThinkRoundEvent` | 保留 | 每轮发布逻辑不变，策略引擎是额外的控制层 |
| `AgentLoopEvent` | 扩展 | `status` 字段新增 `"cancelled"` 值 |
| `AgentAwakeEvent` | 扩展 | 新增 `exit_reason` 字段（`"final"` / `"max_rounds"` / `"timeout"` / `"context_overflow"` / `"cancelled"`），用于 DuckDB 统计分析"哪些策略最常触发" |
| `ThinkLoopResult` | 扩展 | 新增 `Cancelled` 变体 |
| SSE | **不扩展** | 思考运行时事件由前端轮询 `runtime-status` 获取，SSE 通道保持只推 message |
| 前端 Agent 详情页 | 扩展 | 「状态图」Tab 改造为「运行时」Tab，或对话页新增运行时面板 |

**不影响的系统**：
- `AgentRuntimeStateManager` 三态状态机不变（Busy 仍由 consumer 设置/释放）
- `BusyGuard` RAII 守卫核心逻辑不变（Cancelled 仍走正常的 set_idle 清理，额外增加 clear_think_runtime）
- 5 个现有 `BackgroundTask` 实现不变

---

## 十一、分阶段实施建议

| 阶段 | 内容 | 价值 | 风险 |
|------|------|------|------|
| **P1** | 策略引擎框架（`pkg/policy/`：Policy trait + PolicyGroup + PolicyBuilder + 5 个内置策略）+ `run_think_loop` 改造接入策略引擎 | 控制逻辑解耦，可扩展 | 改造核心循环，需充分测试 |
| **P2** | `AgentThinkRuntime` + `AgentRuntimeStateManager` 扩展 + `BusyGuard` 扩展 + consumer 接入 | 思考运行时可监控、可取消 | 改动状态管理核心结构 |
| **P3** | 后端 API（runtime-status / cancel-thinking / runtime-list）+ AgentAwakeEvent 扩展 exit_reason | 前端可查询、可取消 | 接口权限与 DTO 设计 |
| **P4** | 前端运行时面板 + 取消按钮 + 全局运行中 Agent 列表 | 用户体验闭环 | 前端工作量较大 |

**建议**：P1 + P2 可合并实施（策略引擎 + 思考运行时一体化），P3 + P4 可合并实施（后端接口 + 前端面板）。

---

## 十二、已确认决策

> 以下问题已在设计评审中确认，记录于此供实现参考。

| # | 问题 | 决策 | 理由 |
|---|------|------|------|
| 1 | 思考流程是否并入后台任务体系 | **不并入，作为 AgentRuntimeStateManager 的运行时扩展** | 思考流程没有"进度"概念，只有运行时状态；跟着 Agent 状态走，前端展示跟着运行时信息一起走；闭环更简单，不感知后台任务体系 |
| 2 | 运行时信息存储与查询 | **AgentThinkRuntime 持有 `Arc<RwLock<ThinkRuntimeSnapshot>>`，StateManager 直接读取** | 跟着 Agent 状态走，Busy 时有值，Idle 时清理；一次查询拿全部运行时信息，前端无需关联两个数据源 |
| 3 | cancel 机制 | **AgentThinkRuntime 持有 CancellationToken，StateManager.cancel_thinking 直接操作** | cancel 链路最短（agent_id → state_manager → think_runtime.cancel_token）；状态清理由 BusyGuard RAII 自动完成 |
| 4 | 策略引擎位置 | **`pkg/policy/`（纯框架层，独立组件）** | 提供 trait 定义 + 实现 + builder + 策略组结构体 + 计算方法；AgentThinkRuntime 通过 builder 构造策略组存放，不感知策略实现细节 |
| 5 | 思考运行时事件推送方式 | **纯前端轮询，不扩展 SSE** | 现有 SSE 通道面向 message（用户需要看到的内容）；思考运行时是 Agent 内部状态，属于查看时才需要的运维视角信息；思考完成有 message 事件天然通知；不扩展 SSE 保持通道纯净，简化复杂度 |
| 6 | IntentAnalyze 阶段取消 | **立即生效** | run_think_loop 改造后天然检查 cancel_token，所有阶段逻辑统一；Phase 1 理论上可能卡住，不能假设一定快；用户取消是明确意图，立即响应体验最好 |
| 7 | 策略触发原因持久化 | **AgentAwakeEvent 新增 exit_reason 字段** | 已有统计事件，加一个字段成本最低；可 DuckDB 聚合分析"哪个 Agent 最常超时""哪种策略最常触发"，指导配置调优；不持久化完整策略集（日志记录即可） |
| 8 | trace_id 在 ThinkRuntimeSnapshot 中的处理 | **动态更新** | 一次 awaken 流程包含 4 个子场景（IntentAnalyze/Awaken/Settle/Summary），各自有独立的 trace_id；ThinkRuntimeSnapshot.trace_id 每轮上报时写入当前 think_loop 的 trace_id，前端查日志时拿到的是当前阶段的正确 trace_id |
| 9 | 运行时信息清理 | **完整思考流程结束后清理 think_runtime** | 避免运行时信息泄漏；BusyGuard Drop 时同步清理 think_runtime，与 set_idle 一起完成，保证状态一致性 |
| 10 | cancel API 的 domain 归属 | **归属于 runtime domain（`domain.runtime().cancel_thinking`）** | AgentRuntimeStateManager 属于 runtime domain 管辖，cancel 操作本质是操作运行时状态（触发 cancel_token），不涉及 Agent/Message 实体业务变更；信号触发在 domain，信号响应在 think_loop，分工清晰 |
| 11 | MemoryTrace 记录维度 | **一次完整的 think_loop 流程（含多轮 LLM 调用），非单次 LLM 调用** | 一个 think_loop 内所有轮次共享同一 trace_id，合并为一条 MemoryTrace（input=完整 Prompt，output=最终返回）；跨子流程生成新 MemoryTrace；与 ThinkRoundEvent（单轮粒度）形成两级追踪 |
| 12 | 策略集合构建方式 | **policy_set! 声明宏（推荐）+ PolicyBuilder 底层 API（兼容）** | 宏一步完成"策略初始化 + 组装 + 关系指定"，消除 `Box::new(XxxPolicy::new(...))` 样板；支持纯 OR/AND 和混合模式（平铺策略 + OR/AND 子组，外层 AND）；PolicyBuilder + with_policy 泛型方法保留用于嵌套子组等特殊场景 |

---

## 五、扩展模式

### 5.1 新增内置策略类型（如成本上限 / 工具调用次数限制 / 日配额）
现有 5 个内置策略：MaxRoundsPolicy / TimeoutPolicy / ContextOverflowPolicy / UserCancelPolicy / EmptyResponsePolicy。
1. 在 `src/pkg/policy/` 下新增策略实现文件，实现 `Policy` trait，参考现有：[policy 模块](src/pkg/policy)
2. 在 `policy_set!` 宏体中加入新策略的声明分支（如果扩展宏语法），或直接通过 PolicyBuilder::with_policy 手动注册，参考：[pkg/policy/mod.rs](src/pkg/policy/mod.rs)
3. 对应 ThinkRuntimeSnapshot 新增字段时，保持 BusyGuard Drop 时的清理逻辑一致，参考：[domain/runtime state_manager](src/service/domain/runtime)

### 5.2 新增运行时观察维度（如成本估算 / 当前执行的工具栈深度）
如果未来需要在前端运行时面板中展示更丰富的维度：
1. 扩展 ThinkRuntimeSnapshot 结构体字段，保持写入时机为每轮 think 上报节点，参考：[AgentThinkRuntime](src/service/domain/runtime)
2. runtime-status / runtime-list 两个 handler 无需变更 DTO 结构即可透出，DTO 定义见：[common/src/api/runtime.rs](common/src/api/runtime.rs)
3. 前端运行时面板扩展展示，入口参考现有页面：[frontend/src/pages/agent](frontend/src/pages/agent)
