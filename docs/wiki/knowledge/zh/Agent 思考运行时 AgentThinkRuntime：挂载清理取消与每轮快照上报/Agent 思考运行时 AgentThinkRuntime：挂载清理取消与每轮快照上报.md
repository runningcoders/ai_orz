---
kind: wiki_knowledge_card
name: Agent 思考运行时 AgentThinkRuntime：挂载清理取消与每轮快照上报
category: Agent状态运行时
scope:
  - "src/pkg/agent_runtime_state.rs"
  - "src/service/domain/runtime/busy_guard.rs"
  - "src/service/domain/runtime/awakening.rs"
  - "src/service/domain/runtime/think_loop.rs"
  - "src/handlers/hr/agent/cancel_thinking.rs"
source_files:
  - src/pkg/agent_runtime_state.rs:Ln-Lm
  - src/service/domain/runtime/busy_guard.rs:Ln-Lm
  - src/service/domain/runtime/awakening.rs:Ln-Lm
  - src/service/domain/runtime/think_loop.rs:Ln-Lm
  - src/handlers/hr/agent/cancel_thinking.rs:Ln-Lm
  - docs/design/thinking_task_policy_engine_design.md
  - docs/superpowers/plans/2026-08-14-policy-engine-and-think-runtime.md
  - docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md
  - docs/wiki/knowledge/zh/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法.md
---

# Agent 思考运行时 AgentThinkRuntime

## §1 整体方案

AgentThinkRuntime 解决三个问题：(a) 前端/API 无法观测「正在思考的 Agent 当前第几轮/用了多少 token/当前在执行什么工具」(b) 无法取消正在思考的 Agent，只能等超时或轮次耗尽 (c) 状态清理路径分散（panic/early return 漏清理）。设计上把 ThinkRuntime 作为 AgentRuntimeInfo 的扩展字段（Agent 状态为 Busy 时存在、Idle/Resting 时清理），与策略引擎配合每轮更新快照、暴露 cancel 原子信号。

生命周期总览：`MessageConsumer.awaken()` → try_set_busy 成功 → `StateManager.set_busy_with_think_runtime(agent_id)`（挂载 ThinkRuntime：Arc<RwLock<AgentThinkRuntime>> + cancel_token: Arc<AtomicBool>）→ BusyGuard RAII 包裹 → run_think_loop **每轮**：写快照（round/elapsed/tokens/current_tool/last_exit_reason）+ 检查 cancel_token（Policy UserCancelPolicy 读）→ 退出/沉淀 → BusyGuard drop → StateManager.clear_think_runtime(agent_id)（清理）。Handler POST /agents/{id}/cancel-thinking → 直接 `Arc<AtomicBool>.store(true)`（下一轮 UserCancelPolicy 命中）。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构/入口 |
|------|------|-------------|
| [src/pkg/agent_runtime_state.rs](src/pkg/agent_runtime_state.rs) | AgentRuntimeStateManager 扩展 | `AgentRuntimeInfo` 扩展字段 `think_runtime: Option<Arc<RwLock<AgentThinkRuntime>>>`；`ThinkRuntimeSnapshot`（round/elapsed_ms/total_tokens_input/total_tokens_output/current_tool_call/last_exit_reason/scene）；StateManager 方法：`set_busy_with_think_runtime` / `clear_think_runtime` / `get_think_runtime_snapshot`（Option） / `cancel_think(agent_id) -> bool`（原子置位 cancel_token，返回是否真的在思考中）|
| [src/service/domain/runtime/busy_guard.rs](src/service/domain/runtime/busy_guard.rs) | RAII 清理保护 | BusyGuard drop 路径：先 `state_manager.clear_think_runtime(agent_id)` 再 set_idle（无论 panic/early return，思考挂载一定清理）|
| [src/service/domain/runtime/awakening.rs](src/service/domain/runtime/awakening.rs) | 挂载/清理入口 + 退出 reason 映射 | awaken() 入口调 StateManager.set_busy_with_think_runtime（写入 scene + ThinkingOptions 配置的策略参数）；awaken 返回前统一清理；`ThinkLoopResult` 6 变体（Final / SummaryExit / UserCancel / MaxRoundsExceeded / Timeout / ContextOverflow）→ 写入 AgentAwakeEvent.exit_reason |
| [src/service/domain/runtime/think_loop.rs](src/service/domain/runtime/think_loop.rs) | 每轮快照上报 + cancel 检查 | 每轮 loop 尾部：`think_runtime.write().unwrap().update_snapshot(round, elapsed, tokens, current_tool, None)`；UserCancelPolicy 命中 → update_snapshot.last_exit_reason = "user_cancel" 后 break |
| [src/handlers/hr/agent/cancel_thinking.rs](src/handlers/hr/agent/cancel_thinking.rs) | HTTP 取消入口（POST /agents/{id}/cancel-thinking）| 参数鉴权 → 调 RuntimeDomain.awakening().cancel_think(ctx, agent_id) → StateManager.cancel_think() → ApiResponse { success: bool, was_thinking: bool }（前端用 was_thinking 判断 toast）|
| 【Wiki 长文】运行时领域.md | 系统化上下文 §5 详细分析 §8 故障排查 | [运行时领域](docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md) |
| 【平行卡】策略引擎框架（策略 UserCancelPolicy 读取 cancel_token）| 关联知识卡（不同切面）| [策略引擎框架卡](docs/wiki/knowledge/zh/策略引擎：Policy%20trait%20+%20PolicyGroup%20嵌套组合%20+%20policy_set!%20宏声明式写法/策略引擎：Policy%20trait%20+%20PolicyGroup%20嵌套组合%20+%20policy_set!%20宏声明式写法.md) |
| 【① Design】thinking_task_policy_engine_design.md | 决策动机（为什么不用 BackgroundTask 体系接管）| [docs/design/thinking_task_policy_engine_design.md](docs/design/thinking_task_policy_engine_design.md) |
| 【② Plan】执行蓝图 | 完整落地步骤 | [docs/superpowers/plans/2026-08-14-policy-engine-and-think-runtime.md](docs/superpowers/plans/2026-08-14-policy-engine-and-think-runtime.md)（占位：ai-orz-doc-maintainer 精简到 docs/plan/）|

## §3 架构约定

1. **AgentThinkRuntime 只存在于 Busy 态**：Idle/Resting 时 `AgentRuntimeInfo.think_runtime == None`；任何读取快照的方法（runtime-status 接口 / frontend panel）必须先 `if let Some(snapshot) = state.get_snapshot()`，否则返回「未在思考」。
2. **cancel_token 用 Arc<AtomicBool>（零新增依赖）**：设计文档原方案是 tokio_util::CancellationToken，但考虑依赖体积改为等价实现；禁止把 cancel 做成 Mutex 包裹的 bool（不必要的锁开销）。
3. **每轮写一次快照（think_loop 末尾）**：不在中间状态写，避免前端读到半写入；如果某轮发生 panic，BusyGuard.drop 会清理掉整个 ThinkRuntime，所以不会留下脏快照。
4. **Runtime 与策略引擎协作关系单向**：Runtime 暴露 cancel_token 和 update_snapshot() 给 Policy/think_loop 调用，Policy 只"读"Runtime 状态 + 通过 awakening 的 exit_reason 写"外部结果"，Policy 不持有 Runtime 引用（保持纯 pkg 框架定位）。
5. **清理路径优先级：BusyGuard.drop > awaken() 显式清理**：awakening 成功退出路径会显式 clear_think_runtime，但 BusyGuard.drop 里再次调用保证幂等（clear_think_runtime 本身是幂等的，二次调用 no-op）。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止在 think_loop 之外更新 ThinkRuntimeSnapshot**：只能在 run_think_loop 每轮末尾统一写一次；awakening() 入口初始化写 scene + thinking_options 配置快照（round=0），退出写 last_exit_reason 后不再更新。
2. ❌ **禁止 cancel_think 路径上做非原子操作 / 阻塞等待**：cancel_think() 只能做 `cancel_token.store(true, Ordering::SeqCst)` + 返回当前是否 think_runtime.is_some()；不能等待 think_loop 退出，不能调用 DAL/DAO 发事件（AOP 事件由 think_loop 命中 UserCancelPolicy 后统一 emit）。
3. ✅ **幂等性强约束**：以下操作全幂等（调 N 次效果相同），写 Handler/Domain 时必须保证：① set_busy_with_think_runtime（如果 think_runtime.is_some() 则跳过再次挂载，复用现有 cancel_token）② clear_think_runtime ③ cancel_think（已取消再调 = 直接返回 was_thinking = 当前已清理前状态或 false）。
4. ✅ **退出时 last_exit_reason 完整语义**：所有 ThinkLoopResult 变体都要对应写 snapshot.last_exit_reason 字符串（`user_cancel` / `max_rounds_exceeded` / `timeout` / `context_overflow` / `summary_exit` / `final`），**AgentAwakeEvent.exit_reason 字段与 snapshot.last_exit_reason 完全一致**（统计维度不允许两张来源）。
5. ✅ **RwLock 不持有跨 await**：读取/写入 ThinkRuntimeSnapshot 时 `{ let mut s = rt.write().unwrap(); update... }` 块内只做字段赋值，绝对不持有 RwLockWriteGuard 跨越 .await 点（否则死锁：Handler cancel 路径读锁 + think_loop 写锁交叉）。
6. ✅ **四类互引闭环**：本卡 source_files[] 含 wiki 绝对路径 1 条 + Design 1 + Plan 占位 + 平行策略引擎卡；Wiki 长文 cite 段回链本卡绝对路径。
