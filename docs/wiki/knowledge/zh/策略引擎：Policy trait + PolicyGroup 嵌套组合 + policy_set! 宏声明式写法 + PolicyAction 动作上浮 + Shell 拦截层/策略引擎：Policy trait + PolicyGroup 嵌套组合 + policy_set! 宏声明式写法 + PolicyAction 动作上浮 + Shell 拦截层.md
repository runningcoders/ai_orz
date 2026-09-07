---
kind: rag_card
name: 策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层 + PolicyAction 动作上浮 + Shell 拦截层
category: pkg层基础设施
scope:
- src/pkg/policy/**
- src/pkg/tool_registry/shell_policy.rs
- src/service/domain/runtime/think_loop.rs
- src/service/domain/runtime/types.rs
source_files:
- src/pkg/policy/mod.rs#L17-L442
- src/pkg/policy/builtin.rs#L1-L361
- src/pkg/policy/tests.rs#L1-L618
- src/pkg/tool_registry/shell_policy.rs#L1-L359
- src/service/domain/runtime/think_loop.rs#L16-L230
- src/service/domain/runtime/think_loop.rs#L255-L673
- src/service/domain/runtime/think_loop.rs#L82-L127
- docs/design/thinking_task_policy_engine_design.md
- docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md
- docs/wiki/zh/content/核心模块/服务层/领域层/策略引擎与 Shell 拦截层.md
- docs/wiki/zh/content/基础设施/工具注册表/内置工具系统/Shell执行工具.md
- docs/wiki/knowledge/zh/Shell 工具全链路：shell_tool 注册 + shell_policy 拦截 + shell_exec 执行/Shell 工具全链路：shell_tool 注册 + shell_policy 拦截 + shell_exec 执行.md
---

# 策略引擎框架（通用判断引擎 + Shell 拦截层落地）

## §1 概述

策略引擎是 pkg 层纯框架（不感知业务语义），核心三件套：**Policy trait**（基础判断单元，`evaluate` 返回命中策略 id 列表）、**Metrics**（HashMap 封装的运行时算子集，think_loop / shell_policy 各自构造注入）、**policy_set! 声明宏**（消除 `Box::new` 样板，支持纯 OR / 纯 AND / 混合模式）。引擎级新增 **PolicyAction** 三值枚举（Deny/Confirm/Audit）——策略可覆写 `action()` 方法携带处置动作，PolicyGroup 自动上浮首个命中策略的动作（声明顺序 = 优先级）；业务侧可选择忽略 action 自行按命中 id 映射（向后兼容），也可直接消费 action 短路执行（Shell 拦截层即走此路径）。

两轮集成闭环：
1. **think_loop 全策略化**：`build_policy_for_scene` 组装统一 OR 组（UserCancel → FinalAnswer → ContextOverflow → MaxRounds → Timeout → NoProgress → TokenBudget → ConsecutiveLlmErrors，声明顺序即优先级），Final 与工具调用**统一经 `policy_exit` 裁决**——正常完成、取消、溢出、预算耗尽、LLM 重试预算全部走 evaluate → map_triggered_to_result 分派；错误路径（brain_dal think 失败）也过同一策略，预算内立即重试，预算耗尽传播错误。
2. **Shell 拦截层落地**：`ShellRulePolicy`（声明式结构，实现 Policy trait，5 条静态规则）+ `shell_policy::evaluate`（适配层构造 Metrics → Or 组评估 → 汇总阻断动作 + 放行审计）。Shell 拦截**与 shell_exec 同一套策略管线**，也被 shell_tool（声明式 Shell 工具）复用——scope 结构规则 + 命令类规则统一裁决。

## §2 关键文件路径表格

| 文件 | 角色 | 关键结构/宏/入口 |
|------|------|----------------|
| [src/pkg/policy/mod.rs](src/pkg/policy/mod.rs) | 策略引擎核心 | `Policy` trait（id/name/condition_desc/required_metrics/evaluate/is_triggered/action）；`PolicyAction`（Deny/Confirm/Audit + is_blocking/reason 辅助）；`Metrics`（HashMap 封装 with/get_u64/get_bool/get_f64/get_str/get_str_list）；`PolicyGroup`（自身实现 Policy，And/Or 嵌套 + 自动聚合 action）；`PolicyBuilder` with_policy + build/or；`policy_set!` 宏（纯 OR / 纯 AND / 混合模式三层 DSL） |
| [src/pkg/policy/builtin.rs](src/pkg/policy/builtin.rs) | 7 个内置策略 | `MaxRoundsPolicy`（轮次上限，id=max_rounds）、`TimeoutPolicy`（超时，id=timeout）、`ContextOverflowPolicy`（上下文溢出，id=context_overflow，threshold=0 恒不命中）、`UserCancelPolicy`（Arc<AtomicBool>，id=user_cancel）、`TokenBudgetPolicy`（token 预算，id=token_budget，budget=0 恒不命中）、`FinalAnswerPolicy`（正常退出裁决，id=final_answer，output_kind=final 即命中）、`ConsecutiveLlmErrorsPolicy`（LLM 重试预算，id=llm_error_budget）、`NoProgressPolicy`（按工具差异化限制累计调用次数，id=no_progress） |
| [src/pkg/policy/tests.rs](src/pkg/policy/tests.rs) | 策略引擎单元测试 | 三档覆盖：PolicyAction 动作测试（默认 None 向后兼容、Or 组首个 Some 上浮、And 组全部命中才上浮、is_blocking/reason 辅助）；policy_set! 宏 DSL 测试（纯 OR / 纯 AND / 混合模式平铺+子组 / 多子组 / AND 子组嵌套）；7 个内置策略各自的命中/未命中/禁用条件测试 + PolicyGroup/PolicyBuilder 组合测试 |
| [src/pkg/tool_registry/shell_policy.rs](src/pkg/tool_registry/shell_policy.rs) | Shell 拦截层（策略引擎的 shell 领域落地） | `ShellRulePolicy`（声明式结构，id/name/condition_desc/matcher/action/reason，实现 Policy trait 并覆写 action()）；`CommandMatcher`（ScopeOutsideAllowedPaths / ScopeIdentityBoundary / Regex / Subcommand 四 matcher）；`RULE_DEFS`（静态规则表 5 条：工作目录越界 Confirm / 身份边界越界 Confirm / 破坏性 fs 命令 Confirm / git push-reset-clean Confirm / git commit Audit）；`ShellPolicyInput` / `ShellPolicyVerdict` / `evaluate` 入口 |
| [src/service/domain/runtime/think_loop.rs](src/service/domain/runtime/think_loop.rs) | think_loop 全策略化驱动循环 | `round_metrics`（统一构造器：round_number/max_rounds/elapsed_secs/total_tokens/context_tokens/tool_calls.{name}/llm_consecutive_errors）；`policy_exit`（统一退出裁决点，Final 和工具调用都经此）；`map_triggered_to_result`（声明顺序即优先级，映射为 ThinkLoopResult 的 Final/Cancelled/ContextOverflow/MaxRoundsExceeded）；`build_policy_for_scene`（统一 OR 组装配 8 策略，_scene 参数当前未使用）；错误路径（brain_dal think 失败 → 计入 llm_consecutive_errors → evaluate → 预算内重试，预算耗尽传播错误）；疲劳提示（连续 8 轮工具调用注入 System 提醒，只注入一次） |
| 【Design】thinking_task_policy_engine_design.md | 决策背景 | 引擎不感知业务语义的设计哲学、PolicyAction 引入时向后兼容的决策 |
| 【Wiki 长文】运行时领域.md | 系统化上下文 | [运行时领域](docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md) §5 详细组件分析 |
| 【Wiki 长文】策略引擎与 Shell 拦截层.md | 架构图 + Shell 拦截完整链路 | [策略引擎与 Shell 拦截层](docs/wiki/zh/content/核心模块/服务层/领域层/策略引擎与%20Shell%20拦截层.md) |
| 【兄弟卡】Shell 工具全链路 | Shell 工具注册 + 拦截 + 执行完整链路 | [Shell 工具全链路](docs/wiki/knowledge/zh/Shell%20工具全链路：shell_tool%20注册%20+%20shell_policy%20拦截%20+%20shell_exec%20执行/Shell%20工具全链路：shell_tool%20注册%20+%20shell_policy%20拦截%20+%20shell_exec%20执行.md) |

## §3 架构约定

1. **Policy 是纯判断框架，不感知业务 action**：禁止在 Policy 实现里直接调用 DAO/DAL/Domain；所有外部输入通过 Metrics.with(...) 注入，Policy 只读 Metrics。
2. **PolicyGroup 自身是 Policy（组合模式）**：And 要求全部子策略命中才命中（且合并 hit ids），Or 只要任一命中就收集全部子策略命中；嵌套深度不限。PolicyGroup.action 自动上浮：And 全部命中时取首个 Some，Or 按声明顺序取「首个命中且携带动作」的策略。
3. **action() 默认 None，向后兼容**：Policy trait 级 action() 返回 None = 不携带动作，业务侧按命中 id 自行映射（原 think_loop 行为）；声明式策略（如 ShellRulePolicy）覆写 action() 返回 PolicyAction。PolicyAction.is_blocking() 判断是否需要短路执行。
4. **policy_set! 宏支持三层 DSL**：① 纯 OR `policy_set! { OR { A(), B() } }` ② 纯 AND `policy_set! { AND { A(), B() } }` ③ 混合模式——平铺策略参与外层 AND，OR/AND {} 子组内部按指定关系。消除手工 `Box::new` 样板。
5. **think_loop 统一评估点**：FinalAnswerPolicy 是正常完成裁决的一部分（id=final_answer 命中 → ThinkLoopResult::Final），不再是独立旁路；工具调用轮 + 错误路径 + Final 轮，三条路径**全部经 `policy_exit` → `evaluate`**。这是架构级收敛，杜绝「某条退出路径忘了过策略」的遗漏。
6. **Shell 拦截层是通用 Policy 的 shell 领域实现**：ShellRulePolicy 覆写 action()，策略管线 Or 组按 RULE_DEFS 声明顺序组装；`shell_policy::evaluate` 是适配层（构造 Metrics → ruleset.action → PolicyAction.is_blocking 短路 + 放行场景收集 Audit 命中）。shell_exec 和 shell_tool 两条执行链路复用同一个 evaluate。
7. **Metrics 与 required_metrics 配对**：Policy.required_metrics() 返回的 key，调用方构造 Metrics 时必须全部 with 注入（即使 0 / false）。
8. **NoProgressPolicy 数据源严格走 ToolPo.config**：从 agent.tools() 遍历读取每个工具的 po.config_no_progress_max_calls()，只收集配置了该键的工具进入限制表；未配置的工具不参与限制（代码执行类高频工具天然免疫）。
9. **think_loop 疲劳提示只注入一次**：连续 8 轮全是工具调用时注入 System 提醒（TOOL_NUDGE_AFTER_CONSECUTIVE_ROUNDS=8），使用 nudge_injected 标志防重注入。

## §4 硬约束（红线，违反即打回）

1. ❌ **禁止 Policy 实现持有 &mut self**：Policy 必须是无状态纯判断（&self），所有可变状态放到 Metrics / 调用方上下文（如 AgentThinkRuntime.cancel_flag）。Policy 可跨线程/跨轮次复用，不允许内部 mut。
2. ❌ **禁止在 policy_set! 宏外手工组装策略**：所有 think_loop 的策略装配必须收敛到 policy_set! 宏内部；shell_policy 的 RULE_DEFS 是声明式规则表 + OnceLock 初始化，禁止散落在 evaluate 内部手工 PolicyBuilder。
3. ✅ **新增策略三步流程**：① builtin.rs 加 struct + impl Policy（必填 required_metrics）→ ② policy_set! 对应场景追加本策略 → ③ think_loop map_triggered_to_result / Shell 拦截 evaluate 加命中分支。
4. ✅ **Policy.evaluate 未命中 = Vec::new()**：命中 = 所有参与命中的子策略 id 列表（And 全部子策略 id / Or 所有命中子策略 id / 单策略只含自己的 id）。这是下游 map_triggered_to_result 的分派基础。
5. ✅ **测试强约束**：每个 Policy 至少 3 个测试（命中/未命中/禁用条件）；PolicyAction 覆写至少 2 个测试（默认 None 向后兼容 / 携带动作时的短路）；policy_set! 宏 DSL 每种模式至少 1 个测试。
6. ✅ **Shell 拦截层与 shell_exec 同策略管线**：shell_exec 和 shell_tool 两条执行链路必须调用同一个 shell_policy::evaluate；禁止 shell_tool 绕开 shell_policy 自己做拦截检查。env 注入（AI_ORZ_TASK_ID / AI_ORZ_AGENT_ID）也必须两条链路都走。
7. ❌ **禁止在 policy_set! 里写业务自定义逻辑**：policy_set! 只允许组合内置策略；业务自定义判断必须先在 Metrics 构造阶段注入算子（with("custom_flag", true)），再包装成独立 Policy 结构体。
8. ✅ **声明顺序即优先级**：Or 组 evaluate 按声明顺序返回命中列表；think_loop map_triggered_to_result 按列表顺序取首个可分派的。如果「A 与 B 都命中时 A 胜出」，把 A 放在 B 前面。

---

## §5 历史演进（变更摘要）

- **原始设计（thinking_task_policy_engine_design.md）**：Policy trait + PolicyGroup + PolicyBuilder 基础框架，think_loop 每轮 evaluate → 映射 ThinkLoopResult。
- **action() 可选动作（本轮 51d944f8）**：PolicyAction 三值枚举 + trait action() 默认 None + PolicyGroup 自动上浮首个命中策略的动作，实现「默认兼容 + 可选动作」的双模式；Shell 拦截层即走此路径（ShellRulePolicy 覆写 action() 返回 PolicyAction::Deny/Confirm/Audit）。
- **think_loop 全策略化（本轮 51d944f8）**：build_policy_for_scene 统一 8 策略 OR 组（含 FinalAnswerPolicy 正常完成裁决 + ConsecutiveLlmErrorsPolicy 错误路径裁决），Final 和工具调用统一经 policy_exit；错误路径（brain_dal think 失败）也过策略，预算内立即重试。
- **Shell 拦截层（本轮 51d944f8）**：shell_policy.rs 新建，ShellRulePolicy 实现 Policy trait + 覆写 action()；shell_exec 与 shell_tool 两条执行链路复用同一 evaluate；env 注入 AI_ORZ_TASK_ID/AI_ORZ_AGENT_ID 供 git commit-msg hook 消费。
- **【已移除 2026-09-07】PolicyMixed 硬软分层**：旧版策略引擎有 PolicyMixed { hard: PolicyGroup, soft: PolicyGroup } 双层 evaluate，hard_hit 强制退出 / soft_hit 只写日志。本轮迭代简化为单一 Or 组 + 统一 policy_exit，不再分层。**现存代码中 mixed.rs 已不存在**，原 Plan 快照（docs/archive/plan-archive/policy_set_macro_simplification_and_mixed_mode.md）仍保留作为历史参考，正文本卡不再引用。
- **【已移除 2026-09-07】policy_set! 场景 DSL**：旧版有 `policy_set!(Scene::Awaken => [...])` DSL 按 ThinkingScene 变体分 arm。本轮 build_policy_for_scene 不再依赖 scene，统一装配一套 OR 组（FinalAnswerPolicy 已覆盖正常完成裁决），_scene 参数保留但未使用。
- **【已移除 2026-09-07】TokenCostPolicy**：替换为 TokenBudgetPolicy（budget=0 语义：未启用），id=token_budget（而非 token_cost）。
