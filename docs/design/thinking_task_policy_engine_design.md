# Agent 思考运行时 + 策略引擎设计

> 📌 **本文档定位**：设计决策文档（design/）。把 Agent 思考循环的运行时信息收敛到 `AgentThinkRuntime`，作为 `AgentRuntimeStateManager` 的运行时扩展；同时设计独立的策略引擎组件（`pkg/policy/`），统一管理控制逻辑（轮次/超时/上下文溢出/未来 token 预算/用户取消），实现可监控、可取消、可扩展的运行时控制。
>
> **状态**：已评审（2026-08-14），待实现
> **前置依赖**：[runtime_design.md](./runtime_design.md) v3.8（两阶段唤醒）

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

1. **策略化**：把控制逻辑抽象为 `TaskPolicy` trait，策略引擎统一调度，新增策略不改核心循环
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
│  ├── TaskPolicy trait + 内置策略实现                             │
│  ├── PolicyEngine：策略组，短路评估                              │
│  ├── PolicyGroupBuilder：按场景构造策略组                        │
│  ├── 每个 think_loop 创建一个 PolicyEngine 实例                  │
│  ├── 每轮结束后：think_runtime.report_round(metrics)             │
│  ├── 下一轮开始前：engine.evaluate_all(&metrics) → PolicyVerdict │
│  │   ├── Continue          → 继续下一轮                          │
│  │   └── Stop { action }  → 停止，返回对应 ThinkLoopResult       │
│  └── 内置策略：MaxRounds / Timeout / ContextOverflow /           │
│                 TokenBudget（预留）/ UserCancel                  │
└─────────────────────────────────────────────────────────────────┘
```

**关键设计决策**：

| 决策 | 方案 | 理由 |
|------|------|------|
| 思考运行时归属 | `AgentRuntimeStateManager` 扩展，不并入 BackgroundTask | 思考流程没有"进度"概念，只有运行时状态；跟着 Agent 状态走，前端展示跟着运行时信息一起走；闭环更简单 |
| 策略引擎位置 | `pkg/policy/`（纯框架层，独立组件） | 策略引擎是通用框架，提供 trait 定义 + 实现 + builder + 策略组结构体 + 计算方法；符合 AGENTS.md 3.2.1 基础设施约定 |
| 策略引擎粒度 | 每个 think_loop 一个 PolicyEngine 实例 | 不同场景（Awaken/Settle/Summary/IntentAnalyze）策略集不同，且 think_loop 是控制逻辑的实际作用域 |
| cancel 信号 | `tokio_util::sync::CancellationToken` | tokio 生态标准方案，支持 select! 协作式取消，零成本未触发时 |
| 运行时信息存储 | `AgentThinkRuntime` 持有 `Arc<RwLock<...>>`，think_loop 每轮写入 | 运行时快照原子读写，StateManager 直接读取，无需额外 IPC |
| **运行时清理** | **完整思考流程结束后清理 think_runtime** | 避免运行时信息泄漏；BusyGuard Drop 时同步清理 think_runtime，与 set_idle 一起完成 |

### 3.1 Agent 运行时闭环：状态管理 + 思考运行时

**核心认知**：`AgentThinkRuntime` 是 `AgentRuntimeStateManager` 的运行时扩展，不是独立的后台任务。思考运行时跟着 Agent 状态走。

```
时间轴 ─────────────────────────────────────────────────────────►

AgentRuntimeStateManager（Agent 生命周期级）
  Idle ──────────────► Busy ──────────────────────────────► Idle ────►
                      │  current_message_id = msg-123       │
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
| **回答的问题** | "能接受新消息吗" | "当前思考进展如何" |
| **取消语义** | 不存在（状态不是任务） | 明确（CancellationToken） |

**运行时信息生命周期**：

```
consumer.on_event(msg):
    try_set_busy(agent_id, message_id)
    BusyGuard::new()
    think_runtime = AgentThinkRuntime::new(agent_id, trace_id)   ← 创建
    state_manager.set_think_runtime(agent_id, think_runtime)     ← 挂载
    awaken(ctx, agent, message, think_runtime).await:            ← 传入
        ... think loop ...
        think_runtime.report_round(...)                          ← 每轮上报
        think_runtime.is_cancelled() 检查                        ← 每轮检查
    BusyGuard Drop:
        set_idle(agent_id)                                       ← 状态回归
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

### 4.1 TaskPolicy trait

```rust
// src/pkg/policy/mod.rs

/// 任务运行时算子信息（每轮结束后上报）
#[derive(Debug, Clone, Default)]
pub struct TaskMetrics {
    pub round_number: usize,        // 当前轮次（跨压缩累计）
    pub round_duration_ms: u64,     // 本轮耗时
    pub elapsed_secs: u64,          // 累计耗时
    pub tokens_input: u64,          // 本轮输入 token
    pub tokens_output: u64,         // 本轮输出 token
    pub total_tokens: u64,          // 累计 token
    pub context_tokens: u64,        // 当前上下文 token 数（用于 ContextOverflow 判断）
    pub tool_call_count: usize,     // 本轮工具调用次数
    pub total_tool_calls: usize,    // 累计工具调用次数
}

/// 策略判断结果
#[derive(Debug, Clone)]
pub enum PolicyVerdict {
    /// 继续循环
    Continue,
    /// 停止任务，返回当前结果（正常完成 / 轮次耗尽 / 超时）
    Stop { reason: String, action: StopAction },
}

/// 停止后的执行动作
#[derive(Debug, Clone)]
pub enum StopAction {
    /// 直接返回当前 ThinkResult（正常 Final 或轮次耗尽兜底）
    ReturnCurrent,
    /// 触发上下文压缩后重试
    CompressContext,
    /// 触发总结退出流程
    SummarizeAndExit,
    /// 用户取消，清理退出（不触发总结）
    Cancelled,
}

/// 策略 trait（可扩展）
pub trait TaskPolicy: Send + Sync + 'static {
    /// 策略唯一 ID（如 "max_rounds" / "timeout" / "context_overflow"）
    fn policy_id(&self) -> &str;

    /// 策略名称（人类可读，用于前端展示）
    fn name(&self) -> &str;

    /// 策略条件描述（如 "单次唤醒轮次超过 365"）
    fn condition_desc(&self) -> &str;

    /// 策略结果描述（如 "停止思考，触发总结退出"）
    fn result_desc(&self) -> &str;

    /// 策略判断：基于上报的算子信息，返回是否触发
    fn evaluate(&self, metrics: &TaskMetrics) -> PolicyVerdict;
}
```

**设计要点**：
- `evaluate` 是纯函数（无副作用），只做判断不执行动作，动作由调用方根据 `StopAction` 执行
- `condition_desc` / `result_desc` 用于前端展示当前生效的策略集，让用户理解为什么 Agent 停止了
- 策略不持有可变状态，所有状态从 `TaskMetrics` 读取，线程安全

### 4.2 PolicyEngine

```rust
// src/pkg/policy/engine.rs

/// 策略引擎：管理一组策略，统一调度
pub struct PolicyEngine {
    policies: Vec<Box<dyn TaskPolicy>>,
}

impl PolicyEngine {
    pub fn new() -> Self {
        Self { policies: Vec::new() }
    }

    /// 注册策略（按优先级顺序，先注册先判断）
    pub fn with_policy(mut self, policy: Box<dyn TaskPolicy>) -> Self {
        self.policies.push(policy);
        self
    }

    /// 评估所有策略，返回第一个触发的 verdict（短路）
    /// 如果所有策略都返回 Continue，则返回 Continue
    pub fn evaluate_all(&self, metrics: &TaskMetrics) -> PolicyVerdict {
        for policy in &self.policies {
            let verdict = policy.evaluate(metrics);
            if !matches!(verdict, PolicyVerdict::Continue) {
                return verdict;
            }
        }
        PolicyVerdict::Continue
    }

    /// 返回当前注册的策略信息（用于前端展示）
    pub fn policy_infos(&self) -> Vec<PolicyInfo> {
        self.policies.iter().map(|p| PolicyInfo {
            policy_id: p.policy_id().to_string(),
            name: p.name().to_string(),
            condition_desc: p.condition_desc().to_string(),
            result_desc: p.result_desc().to_string(),
        }).collect()
    }
}
```

### 4.3 PolicyGroupBuilder

策略引擎作为独立组件，提供 builder 类型的构造方法，按场景构造策略组。AgentThinkRuntime 通过 builder 构造策略组存放到运行时，不感知策略实现细节。

```rust
// src/pkg/policy/builder.rs

/// 策略组构造器：链式构造，按优先级顺序注册策略
pub struct PolicyGroupBuilder {
    engine: PolicyEngine,
}

impl PolicyGroupBuilder {
    pub fn new() -> Self {
        Self { engine: PolicyEngine::new() }
    }

    /// 用户取消策略（最高优先级，通常所有场景都注册）
    pub fn with_user_cancel(mut self, cancel_token: CancellationToken) -> Self {
        self.engine = self.engine.with_policy(Box::new(
            UserCancelPolicy::new(cancel_token)
        ));
        self
    }

    /// 上下文溢出策略
    pub fn with_context_overflow(mut self, threshold: Option<u64>) -> Self {
        if let Some(th) = threshold {
            self.engine = self.engine.with_policy(Box::new(
                ContextOverflowPolicy::new(th)
            ));
        }
        self
    }

    /// 轮次上限策略
    pub fn with_max_rounds(mut self, max_rounds: usize) -> Self {
        self.engine = self.engine.with_policy(Box::new(
            MaxRoundsPolicy::new(max_rounds)
        ));
        self
    }

    /// 超时策略（0 = 不限制，跳过注册）
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        if timeout_secs > 0 {
            self.engine = self.engine.with_policy(Box::new(
                TimeoutPolicy::new(timeout_secs)
            ));
        }
        self
    }

    /// Token 预算策略（预留）
    pub fn with_token_budget(mut self, budget: u64) -> Self {
        if budget > 0 {
            self.engine = self.engine.with_policy(Box::new(
                TokenBudgetPolicy::new(budget)
            ));
        }
        self
    }

    /// 自定义策略（扩展点）
    pub fn with_policy(mut self, policy: Box<dyn TaskPolicy>) -> Self {
        self.engine = self.engine.with_policy(policy);
        self
    }

    /// 构造完成，返回 PolicyEngine
    pub fn build(self) -> PolicyEngine {
        self.engine
    }
}
```

**按场景构造策略组**（业务层使用示例）：

```rust
// src/service/domain/runtime/awakening.rs

fn build_policy_engine_for_scene(
    scene: ThinkingScene,
    cancel_token: CancellationToken,
    config: &ResolvedConfig,
) -> PolicyEngine {
    let mut builder = PolicyGroupBuilder::new()
        .with_user_cancel(cancel_token);  // 所有场景都注册用户取消

    match scene {
        ThinkingScene::Awaken => builder
            .with_context_overflow(config.context_overflow_threshold)
            .with_max_rounds(config.max_thinking_rounds)
            .with_timeout(config.thinking_timeout_secs)
            .build(),
        ThinkingScene::Settle => builder
            .with_context_overflow(config.context_overflow_threshold)
            .with_max_rounds(config.max_settle_rounds)
            .with_timeout(config.settle_timeout_secs)
            .build(),
        ThinkingScene::Summary => builder
            .with_max_rounds(config.max_summary_rounds)
            .with_timeout(config.summary_timeout_secs)
            .build(),
        ThinkingScene::IntentAnalyze => builder
            .with_max_rounds(config.max_intent_rounds)
            .with_timeout(config.intent_timeout_secs)
            .build(),
    }
}
```

### 4.4 内置策略

```rust
// src/pkg/policy/builtin.rs

/// 轮次上限策略
pub struct MaxRoundsPolicy {
    max_rounds: usize,
}
// evaluate: metrics.round_number >= max_rounds → Stop { SummarizeAndExit }

/// 超时策略
pub struct TimeoutPolicy {
    timeout_secs: u64,  // 0 = 不限制
}
// evaluate: metrics.elapsed_secs >= timeout_secs → Stop { SummarizeAndExit }

/// 上下文溢出策略
pub struct ContextOverflowPolicy {
    threshold: Option<u64>,  // None = 不检查
}
// evaluate: metrics.context_tokens >= threshold → Stop { CompressContext }

/// 用户取消策略（检查 CancellationToken）
pub struct UserCancelPolicy {
    cancel_token: CancellationToken,
}
// evaluate: cancel_token.is_cancelled() → Stop { Cancelled }

/// Token 预算策略（预留，未来扩展）
pub struct TokenBudgetPolicy {
    budget: u64,
}
// evaluate: metrics.total_tokens >= budget → Stop { SummarizeAndExit }
```

**策略注册顺序**（短路优先级）：

```
UserCancel（最高优先级，用户取消立即生效）
  → ContextOverflow（上下文溢出优先压缩，避免下一轮 OOM）
  → MaxRounds（轮次耗尽触发总结）
  → Timeout（超时触发总结）
  → TokenBudget（预留）
```

### 4.5 策略集按场景配置

不同 `ThinkingScene` 注册不同策略集：

| 场景 | UserCancel | ContextOverflow | MaxRounds | Timeout | 触发后的 StopAction 处理 |
|------|:---:|:---:|:---:|:---:|------|
| `Awaken` | ✅ | ✅ | ✅ | ✅ | CompressContext → sleep_and_settle；SummarizeAndExit → awaken_for_summary；Cancelled → 清理退出 |
| `Settle` | ✅ | ✅ | ✅ | ✅ | 所有 Stop → 兜底返回空字符串（现有行为） |
| `Summary` | ✅ | ✅ | ✅ | ✅ | 所有 Stop → 兜底返回空字符串（现有行为） |
| `IntentAnalyze` | ✅ | ❌ | ✅ | ✅ | 所有 Stop → 返回 Err（现有行为，外层降级为 None） |

---

## 五、AgentThinkRuntime 设计

### 5.1 AgentThinkRuntime

```rust
// src/pkg/agent_runtime_state/think_runtime.rs

/// Agent 思考运行时：跟着 Agent 状态走，Busy 时存在，Idle 时清理
/// 持有 cancel 信号 + 运行时快照，由 think_loop 每轮上报
pub struct AgentThinkRuntime {
    agent_id: String,
    cancel_token: CancellationToken,
    // 运行时快照（原子读写，StateManager 直接读取）
    snapshot: Arc<RwLock<ThinkRuntimeSnapshot>>,
}

impl AgentThinkRuntime {
    pub fn new(agent_id: String, trace_id: String) -> Self {
        Self {
            agent_id: agent_id.clone(),
            cancel_token: CancellationToken::new(),
            snapshot: Arc::new(RwLock::new(ThinkRuntimeSnapshot::new(
                agent_id,
                trace_id,
            ))),
        }
    }

    /// 获取 cancel_token 引用（think_loop 用于构造 UserCancelPolicy）
    pub fn cancel_token(&self) -> &CancellationToken {
        &self.cancel_token
    }

    /// think_loop 每轮上报时调用
    pub fn report_round(
        &self,
        trace_id: &str,
        scene: ThinkingScene,
        round: usize,
        max_rounds: usize,
        metrics: &TaskMetrics,
    ) {
        if let Ok(mut snap) = self.snapshot.write() {
            snap.report_round(trace_id, scene, round, max_rounds, metrics);
        }
    }

    /// 用户取消（由 StateManager.cancel_thinking 调用）
    pub fn cancel(&self) -> bool {
        self.cancel_token.cancel();
        if let Ok(mut snap) = self.snapshot.write() {
            snap.status = ThinkStatus::Cancelled;
        }
        true
    }

    /// 是否已取消（think_loop 每轮检查）
    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    /// 获取运行时快照（前端查询用）
    pub fn snapshot(&self) -> ThinkRuntimeSnapshot {
        self.snapshot.read().map(|s| s.clone()).unwrap_or_default()
    }

    /// 获取快照的 Arc 句柄（用于直接传给 awaken 流程）
    pub fn snapshot_handle(&self) -> Arc<RwLock<ThinkRuntimeSnapshot>> {
        self.snapshot.clone()
    }
}
```

**设计要点**：
- 不实现 BackgroundTask trait，不注册到 Registry
- 持有 `Arc<RwLock<ThinkRuntimeSnapshot>>`，think_loop 每轮写入，StateManager 直接读取
- cancel_token 由 think_loop 通过 `PolicyGroupBuilder::with_user_cancel(cancel_token)` 注入策略引擎
- 主体循环逻辑保持现状，只在每个 think_loop 调用点增加 `think_runtime.report_round()` 和 `think_runtime.is_cancelled()` 检查

### 5.2 AgentRuntimeStateManager 扩展

```rust
// src/pkg/agent_runtime_state/manager.rs 扩展

pub struct AgentRuntimeStateManager {
    // 现有字段...
    agents: Arc<RwLock<HashMap<String, AgentRuntimeInfo>>>,
}

/// AgentRuntimeInfo 扩展 think_runtime 字段
#[derive(Clone, Default)]
pub struct AgentRuntimeInfo {
    pub state: AgentRuntimeState,          // 现有
    pub current_message_id: Option<String>, // 现有
    pub state_started_at: i64,             // 现有
    // 新增：思考运行时（仅 Busy 时有值）
    pub think_runtime: Option<Arc<AgentThinkRuntime>>,
}

impl AgentRuntimeStateManager {
    /// 挂载思考运行时（consumer 创建后调用）
    pub async fn set_think_runtime(
        &self,
        agent_id: &str,
        think_runtime: Arc<AgentThinkRuntime>,
    ) {
        let mut agents = self.agents.write().await;
        if let Some(info) = agents.get_mut(agent_id) {
            info.think_runtime = Some(think_runtime);
        }
    }

    /// 清理思考运行时（BusyGuard Drop 时调用）
    pub async fn clear_think_runtime(&self, agent_id: &str) {
        let mut agents = self.agents.write().await;
        if let Some(info) = agents.get_mut(agent_id) {
            info.think_runtime = None;
        }
    }

    /// 取消思考（cancel-thinking 接口调用）
    pub async fn cancel_thinking(&self, agent_id: &str) -> bool {
        let agents = self.agents.read().await;
        if let Some(info) = agents.get(agent_id) {
            if let Some(ref think_runtime) = info.think_runtime {
                return think_runtime.cancel();
            }
        }
        false
    }

    /// 查询思考运行时快照（runtime-status 接口调用）
    pub async fn get_think_runtime_snapshot(
        &self,
        agent_id: &str,
    ) -> Option<ThinkRuntimeSnapshot> {
        let agents = self.agents.read().await;
        if let Some(info) = agents.get(agent_id) {
            if let Some(ref think_runtime) = info.think_runtime {
                return Some(think_runtime.snapshot());
            }
        }
        None
    }
}
```

**BusyGuard 扩展**：Drop 时同步清理 think_runtime。

```rust
// src/pkg/agent_runtime_state/busy_guard.rs
impl Drop for BusyGuard {
    fn drop(&mut self) {
        // 现有逻辑：set_idle
        // 新增：清理 think_runtime
        // 两个操作一起完成，保证状态一致性
    }
}
```

### 5.3 trace_id 生成机制（决策基础）

**现状（基于代码查证）**：一次完整 awaken 流程包含 4 个子场景，各自生成独立的 trace_id：

| 场景 | 生成方式 | 格式 | 代码位置 |
|------|---------|------|---------|
| **IntentAnalyze**（Phase 1） | 字符串拼接，不建 MemoryTrace | `intent-analyze-{ctx.log_id}` | awakening.rs:1145 |
| **Awaken**（主循环 Phase 2） | `MemoryTrace::new()` | `trace-{agent_id}-{timestamp_nanos}-{random_u16}` | awakening.rs:546 |
| **Settle**（上下文压缩） | `MemoryTrace::new()` 新建 | `trace-{agent_id}-{timestamp_nanos}-{random_u16}` | awakening.rs:954 |
| **Summary**（总结退出） | `MemoryTrace::new()` 新建，log_id 用 `summary-{parent}` | `trace-{agent_id}-{timestamp_nanos}-{random_u16}` | awakening.rs:1329 |

**复用规则**：run_think_loop 内部所有轮次复用同一个 trace_id，但跨子流程会生成新的。

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

```rust
/// 思考运行时快照（前端查询用，不实现 TaskProgressSnapshot）
#[derive(Debug, Clone, Default)]
pub struct ThinkRuntimeSnapshot {
    // 标识字段
    pub agent_id: String,          // 固定 = StateManager key
    pub trace_id: String,          // 动态 = 当前 think_loop 的 trace_id（随场景切换更新）

    // 状态字段
    pub status: ThinkStatus,       // Running / Completed / Failed / Cancelled
    pub scene: ThinkingScene,      // IntentAnalyze / Awaken / Settle / Summary
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub step_message: String,      // "IntentAnalyze phase" / "Awaken round 5/365" 等

    // 运行时指标
    pub round_number: usize,       // 当前 think loop 的轮次
    pub max_rounds: usize,         // 当前 think loop 的轮次上限
    pub tokens_input: u64,         // 累计输入 token
    pub tokens_output: u64,        // 累计输出 token
    pub total_tokens: u64,         // 累计总 token
    pub tool_call_count: usize,    // 累计工具调用次数
    pub elapsed_secs: u64,         // 累计耗时

    // 策略信息
    pub active_policies: Vec<PolicyInfo>,  // 当前生效的策略集
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum ThinkStatus {
    #[default]
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl ThinkRuntimeSnapshot {
    pub fn new(agent_id: String, trace_id: String) -> Self {
        Self {
            agent_id,
            trace_id,
            status: ThinkStatus::Running,
            started_at: chrono::Utc::now().timestamp(),
            ..Default::default()
        }
    }

    /// think_loop 每轮上报时调用
    pub fn report_round(
        &mut self,
        trace_id: &str,
        scene: ThinkingScene,
        round: usize,
        max_rounds: usize,
        metrics: &TaskMetrics,
    ) {
        self.trace_id = trace_id.to_string();  // 切换到当前子流程的 trace_id
        self.scene = scene;
        self.round_number = round;
        self.max_rounds = max_rounds;
        self.tokens_input = metrics.tokens_input;
        self.tokens_output = metrics.tokens_output;
        self.total_tokens = metrics.total_tokens;
        self.tool_call_count = metrics.tool_call_count;
        self.elapsed_secs = metrics.elapsed_secs;
        self.step_message = format!("{:?} round {}/{}", scene, round, max_rounds);
    }
}
```

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
        │   ├── build_policy_engine_for_scene(IntentAnalyze, cancel_token, config)
        │   ├── run_think_loop 改造：
        │   │   loop {
        │   │       1. engine.evaluate_all(&last_metrics) → 判断是否继续
        │   │          ├── Continue → 执行本轮 think
        │   │          └── Stop { action } → 返回对应 ThinkLoopResult
        │   │       2. brain_dal.think() → ThinkResult
        │   │       3. 处理 ThinkResult（Final/ToolCall）
        │   │       4. 构造本轮 TaskMetrics
        │   │       5. think_runtime.report_round(trace_id, scene, round, max, metrics)
        │   │       6. 发布 ThinkRoundEvent（现有逻辑保留）
        │   │   }
        │   └── 返回 IntentAnalysis 或 None（降级）
        │
        ├── Phase 2: awaken loop（同上改造）
        │   ├── ContextOverflow → sleep_and_settle（build 新 PolicyEngine）
        │   ├── MaxRoundsExceeded → awaken_for_summary（build 新 PolicyEngine）
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
    ├── state_manager.cancel_thinking(agent_id)   ← 直接操作 think_runtime
    │   └── think_runtime.cancel() → cancel_token.cancel()
    │
    └── 返回 { success: true, message: "取消信号已发送" }
        │
        ▼（异步，不阻塞 HTTP 响应）
    think_loop 下一轮开始前：
        engine.evaluate_all(&metrics)
            └── UserCancelPolicy.evaluate() → Stop { Cancelled }
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

**关键**：cancel 全程通过 think_runtime 的 cancel_token 协作，状态清理由 BusyGuard RAII 自动完成。

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

```rust
// 改造前
async fn run_think_loop(
    &self,
    ctx: RequestContext,
    brain: &Brain,
    prompt: &str,
    tool_descriptors: &[ToolDescriptor],
    agent: &Agent,
    scene_str: &str,
    trace_id: &str,
    max_rounds: usize,
    start_round: usize,
    timeout_secs: u64,
) -> Result<ThinkLoopResult>

// 改造后
async fn run_think_loop(
    &self,
    ctx: RequestContext,
    brain: &Brain,
    prompt: &str,
    tool_descriptors: &[ToolDescriptor],
    agent: &Agent,
    scene_str: &str,
    trace_id: &str,
    engine: &PolicyEngine,           // 替代 max_rounds + timeout_secs
    start_round: usize,
    think_runtime: Option<&AgentThinkRuntime>,  // 运行时上报（None = 不上报，如内部测试）
) -> Result<ThinkLoopResult>
```

### 7.2 ThinkLoopResult 新增 Cancelled 变体

```rust
pub enum ThinkLoopResult {
    Final { content: String, messages: Vec<ChatMessage> },
    ContextOverflow { messages: Vec<ChatMessage>, input_tokens: u64, rounds_used: usize },
    MaxRoundsExceeded { messages: Vec<ChatMessage>, total_rounds: usize },
    Cancelled { messages: Vec<ChatMessage>, total_rounds: usize },  // 新增
}
```

### 7.3 循环体改造（伪代码）

```rust
// 改造后的循环体核心逻辑
let mut last_metrics = TaskMetrics::default();
let mut messages = vec![ChatMessage::user(prompt)];
let started_at = Instant::now();

loop {
    // 1. 策略判断（基于上一轮 metrics）
    match engine.evaluate_all(&last_metrics) {
        PolicyVerdict::Continue => {}
        PolicyVerdict::Stop { reason, action } => {
            log_info!(&ctx, "think_loop", "policy triggered: {} → {:?}", reason, action);
            return Ok(match action {
                StopAction::ReturnCurrent => ThinkLoopResult::MaxRoundsExceeded { 
                    messages, total_rounds: last_metrics.round_number 
                },
                StopAction::CompressContext => ThinkLoopResult::ContextOverflow { 
                    messages, input_tokens: last_metrics.context_tokens, rounds_used: last_metrics.round_number 
                },
                StopAction::SummarizeAndExit => ThinkLoopResult::MaxRoundsExceeded { 
                    messages, total_rounds: last_metrics.round_number 
                },
                StopAction::Cancelled => ThinkLoopResult::Cancelled { 
                    messages, total_rounds: last_metrics.round_number 
                },
            });
        }
    }

    // 2. 执行 think
    let think_result = brain_dal.think(&messages, tool_descriptors).await?;
    let round_duration = started_at.elapsed();  // 本轮耗时（近似）

    // 3. 处理 ThinkResult
    match think_result {
        ThinkResult::Final { content, usage } => {
            // 发布 ThinkRoundEvent（现有逻辑）
            // 上报运行时
            if let Some(rt) = think_runtime {
                rt.report_round(trace_id, scene, last_metrics.round_number + 1, max_rounds, &last_metrics);
            }
            return Ok(ThinkLoopResult::Final { content, messages });
        }
        ThinkResult::ToolCall { .. } => {
            // 执行工具调用（现有逻辑）
            // 发布 ThinkRoundEvent（现有逻辑）
        }
    }

    // 4. 构造本轮 metrics
    last_metrics = TaskMetrics {
        round_number: last_metrics.round_number + 1,
        round_duration_ms: round_duration.as_millis() as u64,
        elapsed_secs: started_at.elapsed().as_secs(),
        tokens_input: usage.input_tokens,
        tokens_output: usage.output_tokens,
        total_tokens: last_metrics.total_tokens + usage.total(),
        context_tokens: usage.input_tokens,
        tool_call_count: tc_count,
        total_tool_calls: last_metrics.total_tool_calls + tc_count,
    };

    // 5. 上报运行时
    if let Some(rt) = think_runtime {
        rt.report_round(trace_id, scene, last_metrics.round_number, max_rounds, &last_metrics);
    }
}
```

**改造要点**：
- `max_rounds` 和 `timeout_secs` 参数被 `engine: &PolicyEngine` 替代，控制逻辑从硬编码变为策略驱动
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

### 8.2 SSE 推送增强

现有 SSE 只推送消息事件，新增思考运行时事件推送。**推送策略选 C：关键节点推送**——只在场景切换、每 10 轮里程碑、工具调用完成、思考完成时推送，前端轮询（3-5 秒）补充细节。

```rust
// SSE 推送的事件类型扩展
pub enum SseEventType {
    Message(SsePushPayload),                // 现有：消息投递
    ThinkingProgress(ThinkingProgressSse),  // 新增：关键节点思考运行时
    ThinkingFinished(ThinkingFinishedSse),  // 新增：思考完成
}

#[derive(Serialize)]
pub struct ThinkingProgressSse {
    pub agent_id: String,
    pub trace_id: String,
    pub scene: String,               // 场景切换时推送
    pub round_number: usize,         // 每 10 轮里程碑推送
    pub max_rounds: usize,
    pub total_tokens: u64,
    pub tool_call_count: usize,
    pub elapsed_secs: u64,
    pub event_type: String,          // "scene_change" / "milestone" / "tool_call"
}

#[derive(Serialize)]
pub struct ThinkingFinishedSse {
    pub agent_id: String,
    pub trace_id: String,
    pub status: String,              // "success" / "failed" / "cancelled"
    pub exit_reason: String,         // "final" / "max_rounds" / "timeout" / "context_overflow" / "cancelled"
    pub duration_ms: u64,
}
```

**推送触发条件**：
- `scene_change`：IntentAnalyze→Awaken、Awaken→Settle、Awaken→Summary 等场景切换
- `milestone`：每 10 轮（round_number % 10 == 0）
- `tool_call`：工具调用完成（可选，视实际频率决定是否开启）
- `finished`：awaken 流程结束（success/failed/cancelled）

---

## 九、前端方案

### 9.1 Agent 对话页面实时运行时

在 Agent 对话页面（或 Agent 详情页的「状态图」Tab 改造为「运行时」Tab）新增**实时运行时面板**：

```
┌─────────────────────────────────────────────────┐
│ 🧠 Agent 运行时                    [取消思考]    │
├─────────────────────────────────────────────────┤
│ 状态：🔵 思考中 (Awaken)                         │
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
- SSE 订阅 `ThinkingProgress` 事件，实时更新面板
- 「取消思考」按钮调用 `POST /agents/{id}/cancel-thinking`
- Agent 空闲时面板收起，仅显示「状态：空闲」

### 9.2 全局运行中 Agent 列表

在系统监控页面（或工作台）新增「运行中 Agent」卡片：

```
┌──────────────────────────────────┐
│ 🤖 运行中 Agent (3)              │
├──────────────────────────────────┤
│ Agent-A  Awaken  5/365  12,450t  │
│ Agent-B  Settle  2/365   3,200t  │
│ Agent-C  Summary 1/365     800t  │
└──────────────────────────────────┘
```

---

## 十、与现有系统的关系

| 现有系统 | 关系 | 改动 |
|---------|------|------|
| `AgentRuntimeStateManager` | **扩展** | `AgentRuntimeInfo` 新增 `think_runtime: Option<Arc<AgentThinkRuntime>>` 字段；新增 `set_think_runtime` / `clear_think_runtime` / `cancel_thinking` / `get_think_runtime_snapshot` 方法 |
| `BusyGuard` | 扩展 | Drop 时同步清理 think_runtime（与 set_idle 一起完成） |
| `BackgroundTask` trait | **不改** | 思考运行时不并入后台任务体系 |
| `BackgroundTaskRegistry` | **不改** | 思考运行时不注册到 Registry |
| `run_think_loop` | 改造 | `max_rounds/timeout_secs` 参数 → `engine: &PolicyEngine`；新增 `think_runtime: Option<&AgentThinkRuntime>` 参数 |
| `config_resolve` | 保留 | 策略参数仍从 config_resolve 获取，策略引擎在 think_loop 入口处用 config_resolve 值通过 `PolicyGroupBuilder` 构建策略组 |
| `ThinkRoundEvent` | 保留 | 每轮发布逻辑不变，策略引擎是额外的控制层 |
| `AgentLoopEvent` | 扩展 | `status` 字段新增 `"cancelled"` 值 |
| `AgentAwakeEvent` | 扩展 | 新增 `exit_reason` 字段（`"final"` / `"max_rounds"` / `"timeout"` / `"context_overflow"` / `"cancelled"`），用于 DuckDB 统计分析"哪些策略最常触发" |
| `ThinkLoopResult` | 扩展 | 新增 `Cancelled` 变体 |
| SSE | 扩展 | 新增 `ThinkingProgress`（关键节点）/ `ThinkingFinished` 事件类型 |
| 前端 Agent 详情页 | 扩展 | 「状态图」Tab 改造为「运行时」Tab，或对话页新增运行时面板 |

**不影响的系统**：
- `AgentRuntimeStateManager` 三态状态机不变（Busy 仍由 consumer 设置/释放）
- `BusyGuard` RAII 守卫核心逻辑不变（Cancelled 仍走正常的 set_idle 清理，额外增加 clear_think_runtime）
- 5 个现有 `BackgroundTask` 实现不变

---

## 十一、分阶段实施建议

| 阶段 | 内容 | 价值 | 风险 |
|------|------|------|------|
| **P1** | 策略引擎框架（`pkg/policy/`：trait + engine + builder + 4 个内置策略）+ `run_think_loop` 改造接入策略引擎 | 控制逻辑解耦，可扩展 | 改造核心循环，需充分测试 |
| **P2** | `AgentThinkRuntime` + `AgentRuntimeStateManager` 扩展 + `BusyGuard` 扩展 + consumer 接入 | 思考运行时可监控、可取消 | 改动状态管理核心结构 |
| **P3** | 后端 API（runtime-status / cancel-thinking / runtime-list）+ SSE 推送 | 前端可查询、可取消 | SSE 推送频率控制 |
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
| 5 | SSE 推送频率 | **方案 C：关键节点推送 + 前端轮询补充** | 只在场景切换、每 10 轮里程碑、工具调用完成、思考完成时推送；前端 3-5 秒轮询补充细节；避免高频 think_loop 产生大量事件 |
| 6 | IntentAnalyze 阶段取消 | **立即生效** | run_think_loop 改造后天然检查 cancel_token，所有阶段逻辑统一；Phase 1 理论上可能卡住，不能假设一定快；用户取消是明确意图，立即响应体验最好 |
| 7 | 策略触发原因持久化 | **AgentAwakeEvent 新增 exit_reason 字段** | 已有统计事件，加一个字段成本最低；可 DuckDB 聚合分析"哪个 Agent 最常超时""哪种策略最常触发"，指导配置调优；不持久化完整策略集（日志记录即可） |
| 8 | trace_id 在 ThinkRuntimeSnapshot 中的处理 | **动态更新** | 一次 awaken 流程包含 4 个子场景（IntentAnalyze/Awaken/Settle/Summary），各自有独立的 trace_id；ThinkRuntimeSnapshot.trace_id 每轮上报时写入当前 think_loop 的 trace_id，前端查日志时拿到的是当前阶段的正确 trace_id |
| 9 | 运行时信息清理 | **完整思考流程结束后清理 think_runtime** | 避免运行时信息泄漏；BusyGuard Drop 时同步清理 think_runtime，与 set_idle 一起完成，保证状态一致性 |
