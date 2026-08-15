---
kind: wiki_knowledge_card
name: 策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法
category: pkg层基础设施
scope:
  - "src/pkg/policy/**"
  - "src/service/domain/runtime/*.rs"
  - "common/src/enums/thinking_scene.rs"
source_files:
  - src/pkg/policy/mod.rs:Ln-Lm
  - src/pkg/policy/builtin.rs:Ln-Lm
  - src/pkg/policy/tests.rs:Ln-Lm
  - src/service/domain/runtime/awakening.rs:Ln-Lm
  - src/service/domain/runtime/think_loop.rs:Ln-Lm
  - common/src/enums/thinking_scene.rs:Ln-Lm
  - docs/design/thinking_task_policy_engine_design.md
  - docs/superpowers/plans/2026-08-14-policy-engine-and-think-runtime.md
  - docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md
---

# 策略引擎框架

## §1 整体方案

策略引擎是 pkg 层纯框架（不感知业务），解决 `think_loop` 控制逻辑硬编码分散在 awakening.rs 里的问题：把「轮次上限/超时/上下文溢出/用户取消」等判断抽象为 Policy trait，可组合、可扩展；think_loop 每轮只需构造 Metrics 算子集 → 调用 PolicyGroup.evaluate() 拿到命中策略 id 列表 → 映射 ThinkLoopResult 决定下一步。

调用链路：`Policy trait`（基础判断单元，5 个内置策略实现）→ `PolicyGroup`（本身实现 Policy，And/Or 嵌套组合）→ `policy_set!` 声明宏（对 4 个 ThinkingScene 场景分别组装组合策略，内置 5 策略 OR 命中）→ `build_policy_for_scene(scene)` 在 awakening 里根据场景拿策略集 → `run_think_loop` 每轮传入 Metrics 评估。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构/宏/入口 |
|------|------|----------------|
| [src/pkg/policy/mod.rs](src/pkg/policy/mod.rs) | 策略引擎核心（trait/struct 定义） | `Policy` trait（id/name/condition_desc/required_metrics/evaluate/is_triggered）、`Metrics` HashMap 封装 with/get_u64/get_bool/get_f64、`PolicyRelation And/Or`、`PolicyGroup`（嵌套组合 + 自身实现 Policy）、`PolicyBuilder` with+build/or |
| [src/pkg/policy/builtin.rs](src/pkg/policy/builtin.rs) | 5 个内置策略实现 | `MaxRoundsPolicy`、`TimeoutPolicy`、`ContextOverflowPolicy`、`UserCancelPolicy`（读 AgentThinkRuntime.cancel_token 包装 Arc<AtomicBool>）、`TokenCostPolicy` |
| [src/pkg/policy/tests.rs](src/pkg/policy/tests.rs) | 策略引擎单元测试 | 单策略命中、And 组全部命中、Or 组任一命中、嵌套 PolicyGroup、空 Metrics、required_metrics 声明等覆盖 |
| [common/src/enums/thinking_scene.rs](common/src/enums/thinking_scene.rs) | 思考场景枚举（Awaken/IntentAnalyze/Summary/Settle） | 每个场景对应一套 policy_set! 声明 |
| [src/service/domain/runtime/awakening.rs](src/service/domain/runtime/awakening.rs) | Runtime 域使用策略的入口 | `build_policy_for_scene(scene) -> Box<dyn Policy>`：按场景调用 policy_set! 装配；`ThinkLoopResult` 把命中策略 id 映射成退出 reason；`map_triggered_to_result` |
| [src/service/domain/runtime/think_loop.rs](src/service/domain/runtime/think_loop.rs) | think_loop 驱动循环 | 每轮构造 `Metrics::new().with("rounds", n).with("elapsed_ms", ...).with("cancel", ...)`，调用 policy.evaluate(&metrics)，拿到命中列表进入 exit_reason 分支或继续 |
| 【对应 Wiki 长文】运行时领域.md | 系统化上下文（必读 §5）| /Users/aman/Technology/rust/ai_orz/docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md |
| 【① Design】thinking_task_policy_engine_design.md | 决策背景/关键决策表 | [docs/design/thinking_task_policy_engine_design.md](docs/design/thinking_task_policy_engine_design.md) |
| 【② Plan】执行蓝图（待转 plan 目录）| 完整 7 章落地 | [docs/superpowers/plans/2026-08-14-policy-engine-and-think-runtime.md](docs/superpowers/plans/2026-08-14-policy-engine-and-think-runtime.md)（占位：ai-orz-doc-maintainer 精简到 docs/plan/ 后回填真实 plan 路径）|

## §3 架构约定

1. **Policy 是纯判断框架，不感知业务 action**：禁止在 Policy 实现里直接调用 DAO/DAL/Domain；所有外部输入通过 Metrics.with(...) 注入，Policy 只读 Metrics。
2. **PolicyGroup 自身是 Policy（组合模式）**：And 要求全部子策略命中才命中（且合并 hit ids），Or 只要任一命中就收集全部子策略命中；嵌套深度不限（And(Or(...), A, B)）。
3. **policy_set! 宏按场景声明，不写死在 awakening 里**：每个 ThinkingScene 一套组合策略，新增场景只需新增一个 macro 分支；默认 5 个内置策略用 Or 关系组（任一命中 = 触发退出/沉淀），如需严格模式（轮次+超时同时满足）再包外层 And。
4. **5 个内置策略永不改动 API 形状**：新增策略 → 在 builtin.rs 加一个 struct 并在 policy_set! 内追加；不允许直接修改现 5 个策略的 id/required_metrics 名称（会影响下游 exit_reason 映射）。
5. **Metrics 与 required_metrics 配对**：Policy.required_metrics() 返回的 key，调用方在构造 Metrics 时**必须**全部 with 注入（即使是 0 / false）；测试里也必须覆盖全部 required_metrics（缺一个视为潜在运行时 miss）。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止 Policy 实现持有 &mut self**：Policy 必须是无状态纯判断（`&self`），所有可变状态放到 Metrics/AgentThinkRuntime.cancel_token/Domain 上下文里（策略框架可跨线程/跨轮次复用，不允许内部 mut）。
2. ❌ **禁止 policy_set! 直接写业务条件**：`policy_set!` 只允许组合内置策略 + PolicyGroup；业务自定义判断（如 "项目已完成则终止"）必须先实现在 awakening.rs 构造 Metrics 的阶段（with("project_done", true)），再包装成独立 Policy 结构体注册到 policy_set! 对应场景。
3. ✅ **新增策略的三步强制流程**：① builtin.rs 加 struct + impl Policy（必填 required_metrics 声明用到的所有 Metrics key）→ ② policy_set! 对应 ThinkingScene 场景的 Or/And 组追加本策略 → ③ awakening.rs map_triggered_to_result 命中分支加本策略 id → ThinkLoopResult 退出 reason（或沉淀 reason）对应枚举变体 + exit_reason 字符串。
4. ✅ **Policy.evaluate 禁止返回空 Vec 以外表示「未命中」的方式**：未命中 = Vec::new()；命中 = 所有参与命中的子策略 id 列表（And 组：全部子策略 id；Or 组：所有命中子策略的 id；单策略：只含自己的 id）。
5. ✅ **测试强约束**：每个 Policy 至少 3 个测试（命中/未命中/required_metrics 声明完整性）；PolicyGroup And/Or 组合各 1 个；嵌套组 1 个。参见 [src/pkg/policy/tests.rs](src/pkg/policy/tests.rs)。
6. ✅ **四类互引闭环**：本卡 `source_files[]` 含对应 Wiki 长文绝对路径 1 条（运行时领域.md）+ Design 文档 + Plan 执行蓝图占位；Wiki 长文 cite 区的「本文关联三类文档」段必须回链本卡绝对路径 + Design + Plan。
