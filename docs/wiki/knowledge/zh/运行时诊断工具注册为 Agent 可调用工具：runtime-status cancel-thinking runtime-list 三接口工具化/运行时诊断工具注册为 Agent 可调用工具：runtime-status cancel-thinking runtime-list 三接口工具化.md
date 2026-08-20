---
kind: wiki_knowledge_card
name: 运行时诊断工具注册为 Agent 可调用工具：runtime-status cancel-thinking runtime-list 三接口工具化
category: 工具系统
scope:
- src/pkg/tool_registry/**
- src/handlers/runtime/**
- src/service/domain/runtime/*.rs
- src/pkg/aop/think/**
source_files:
- src/pkg/tool_registry/runtime_tools.rs#L1-L150
- src/handlers/runtime/runtime_status.rs#L1-L120
- src/handlers/runtime/cancel_thinking.rs#L1-L100
- src/handlers/runtime/runtime_list.rs#L1-L120
- src/service/domain/runtime/management.rs#L1-L200
- src/service/domain/runtime/awakening.rs#L1-L150
- src/pkg/aop/think/registry.rs#L1-L100
- docs/design/runtime_design.md
- docs/design/thinking_task_policy_engine_design.md
- docs/archive/design-archive/tool_and_skill_system.md
- docs/archive/plan-archive/runtime_tool_registration_as_agent_callable_tools.md
- docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md
- docs/wiki/zh/content/架构设计/分层架构设计/Domain 层编排/Runtime 领域编排.md
- docs/wiki/zh/content/前端应用/组件系统/业务组件/思考运行时面板观测接口.md
- docs/wiki/zh/content/功能模块/工具生态系统/运行时诊断工具组.md
- docs/wiki/knowledge/zh/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法
  + 混合模式支持/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + 混合模式支持.md
- docs/wiki/knowledge/zh/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool
  宏 + 神经工具免绑定三层校验/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool
  宏 + 神经工具免绑定三层校验.md

---

# 运行时诊断工具注册为 Agent 可调用工具

## §1 整体方案

d1d1013f 变更把原先「只供前端轮询面板」的 3 个 runtime 诊断 HTTP 接口（`runtime-status` / `cancel-thinking` / `runtime-list`）注册为 **Agent 可调用工具**：Agent 在思考/对话过程中可以自我诊断运行时状态、主动取消自己的 long-running thinking 任务、或列出当前 org 下所有活跃 runtime。

注册链路：`register_handler_tool!` 宏（工具系统统一注册入口）→ `AgentCallContext` 构造时 `tool_registry().register(Arc<dyn CoreTool>)` 注入 → Agent 唤醒时 binding 到可用工具列表 → 三层调用分发（Builtin 协议路由 → `invoke()` → 底层调 Domain 层 RuntimeManagement）。

3 个工具：
1. **tool://builtin/runtime_status**：查询指定 agent_id + trace_id 对应的 think runtime 状态（rounds/elapsed_ms/cancel_token 原子读/累计 tokens/hard_hit 策略命中列表/soft_hit 预警列表）
2. **tool://builtin/cancel_thinking**：向指定 trace_id 的 think runtime 发 cancel 信号（写 cancel_token Arc<AtomicBool> = true，PolicyMixed.hard 层下一论 evaluate 命中 → 强制退出，exit_reason="cancel_by_agent_self"）
3. **tool://builtin/runtime_list**：列出 org 下所有活跃（未退出）think runtime 列表（agent_id/trace_id/scene/rounds/start_time），支持分页

典型场景：Agent 陷入长时间循环时，自己调用 `runtime_status` 诊断 → 发现 rounds 已过 soft_limit → 调用 `cancel_thinking` 主动退出 + 沉淀结果；或 Agent B 想配合 Agent A 的任务，先 `runtime_list` 找 A 的活跃 runtime → 再 `runtime_status` 查 A 当前进度。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构/宏/入口 |
|------|------|----------------|
| [src/pkg/tool_registry/runtime_tools.rs](src/pkg/tool_registry/runtime_tools.rs) | d1d1013f 新增：3 个工具 struct 实现 CoreTool trait | `RuntimeStatusTool`（schema: agent_id string, trace_id string）、`CancelThinkingTool`（schema: trace_id string, reason? string）、`RuntimeListTool`（schema: org_id string, page? i32, page_size? i32）；`register_runtime_tools(registry: &ToolRegistry)` 三实例批量注册 |
| [src/handlers/runtime/runtime_status.rs](src/handlers/runtime/runtime_status.rs) | 前端 HTTP handler（保留原接口） | Handler 层参数校验 → 调 RuntimeDomain.get_runtime_status() → 返回 ApiResponse；工具层 `RuntimeStatusTool.invoke()` **复用同一个 Domain 方法**（Handler ↔ Tool 双入口同一 Domain 实现，DRY） |
| [src/handlers/runtime/cancel_thinking.rs](src/handlers/runtime/cancel_thinking.rs) | 前端取消 HTTP handler（保留原接口） | 写 cancel_token 原子 bool；工具层 `CancelThinkingTool.invoke()` 同样复用 RuntimeDomain.cancel_thinking() |
| [src/handlers/runtime/runtime_list.rs](src/handlers/runtime/runtime_list.rs) | 前端列表 HTTP handler（保留原接口） | 查询活跃 think runtime；工具层复用同一 Query 流程 |
| [src/service/domain/runtime/management.rs](src/service/domain/runtime/management.rs) | RuntimeDomain Management trait 实现（单一事实源） | `get_runtime_status(ctx, agent_id, trace_id) -> RuntimeStatusView`、`cancel_thinking(ctx, trace_id, reason) -> Result<()>`、`list_active_runtimes(ctx, org_id, pagination) -> PagedResult<RuntimeListView>`；**Handler 与 Tool 必须都调这里**，禁止各写一套查询逻辑（保证状态读取一致性） |
| [src/service/domain/runtime/awakening.rs](src/service/domain/runtime/awakening.rs) | 策略 evaluate 读取 cancel_token | UserCancelPolicy / 混合模式 hard 层每轮读 cancel_token，命中后 exit_reason 分「用户从前端取消」vs「Agent 自己调 cancel_thinking 工具」两种（reason 字段区分，日志前缀 `reason=user_cancel_frontend` vs `reason=cancel_by_agent_self`） |
| [src/pkg/aop/think/registry.rs](src/pkg/aop/think/registry.rs) | 活跃 runtime 注册中心（内存 HashMap） | 所有 AgentAwakeEvent 启动时 register、结束时 unregister；runtime_list 工具 & handler 都从这里扫活跃键 + management.rs 补业务字段 |
| 【兄弟卡】策略引擎卡（Level3 兄弟）| PolicyMixed 与 cancel_token UserCancelPolicy | [策略引擎卡](docs/wiki/knowledge/zh/策略引擎：Policy%20trait%20+%20PolicyGroup%20嵌套组合%20+%20policy_set!%20宏声明式写法%20+%20混合模式支持/策略引擎：Policy%20trait%20+%20PolicyGroup%20嵌套组合%20+%20policy_set!%20宏声明式写法%20+%20混合模式支持.md) |
| 【兄弟卡】工具系统三层调用架构卡（Level3 兄弟）| register_handler_tool! 宏 + 三协议路由 | [工具系统三层调用卡](docs/wiki/knowledge/zh/工具系统三层调用架构：CoreTool%20trait%20+%20Builtin%20HTTP%20MCP%20三协议路由%20+%20register_handler_tool%20宏%20+%20神经工具免绑定三层校验/工具系统三层调用架构：CoreTool%20trait%20+%20Builtin%20HTTP%20MCP%20三协议路由%20+%20register_handler_tool%20宏%20+%20神经工具免绑定三层校验.md) |
| 【Wiki 长文】运行时领域.md | 系统化上下文 | [运行时领域](docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md) |
| 【Wiki 长文】思考运行时面板观测接口.md | 面板侧长文 | [思考运行时面板](docs/wiki/zh/content/功能模块/Agent管理/思考运行时面板观测接口.md) |
| 【① Design】runtime_design.md | 运行时设计 | [docs/design/runtime_design.md](docs/design/runtime_design.md) |
| 【① Design】tool_and_skill_system.md | 工具系统设计 | [docs/archive/design-archive/tool_and_skill_system.md](docs/archive/design-archive/tool_and_skill_system.md) |

## §3 架构约定

本卡与 [策略引擎卡](docs/wiki/knowledge/zh/策略引擎：Policy%20trait%20+%20PolicyGroup%20嵌套组合%20+%20policy_set!%20宏声明式写法%20+%20混合模式支持/策略引擎：Policy%20trait%20+%20PolicyGroup%20嵌套组合%20+%20policy_set!%20宏声明式写法%20+%20混合模式支持.md) + [工具系统三层调用卡](docs/wiki/knowledge/zh/工具系统三层调用架构：CoreTool%20trait%20+%20Builtin%20HTTP%20MCP%20三协议路由%20+%20register_handler_tool%20宏%20+%20神经工具免绑定三层校验/工具系统三层调用架构：CoreTool%20trait%20+%20Builtin%20HTTP%20MCP%20三协议路由%20+%20register_handler_tool%20宏%20+%20神经工具免绑定三层校验.md) 构成 **运行时 + 策略 + 工具** 体系的 诊断执行 / 策略判断 / 工具注册 互补视角；按 AGENTS §2.1.3 Level 3 保留平行卡。

1. **【核心约定】Handler 与工具 MUST 复用同一 Domain 方法**：`runtime_status / cancel_thinking / runtime_list` 3 个能力，每个能力**只有一个 Domain 方法实现**；HTTP Handler 调它、CoreTool.invoke() 也调它。禁止「Handler 查 AOP registry 手工拼字段、工具查 DAO 直接拼」——两套代码拼同一份视图一定会字段漂移（比如 Handler 返回了 soft_warns 列表但工具漏掉了）。
2. **cancel_thinking 幂等 + 原子**：`cancel_thinking(ctx, trace_id, reason)` 内部是 `cancel_token.store(true, Ordering::SeqCst)`（原子写），无论调 N 次结果一致；工具层、Handler 层连续调用同一 trace_id 不得报「重复取消 = error」，应返回 `success: true, already_cancelled: true/false` 区分。
3. **权限三层隔离（工具层 & Handler 层统一）**：
   - runtime_status：Agent 自己调时只能查**自己 agent_id 下的** trace_id（防止 Agent A 偷查 Agent B 的思考过程）；Admin 角色跨 agent 查询放开；
   - cancel_thinking：Agent 只能取消**自己发起的** trace_id；Admin 可取消 org 下任意；SuperAdmin 可跨 org；
   - runtime_list：Agent 只能看到自己 org 下自己的 + 共享协作标记 runtime（详见 management.rs 过滤逻辑）；
   权限判定放在 management.rs（Domain 层），Handler 与工具都走同一权限 gate。
4. **工具 schema（JSON Schema）严格 = Handler DTO 字段**：`RuntimeStatusTool.schema().input` 与 `RuntimeStatusRequest DTO`（common/src/api/runtime.rs）字段完全一致——Handler DTO 改字段必须同步改工具 schema（测试：`runtime_tool_schema_eq_handler_dto` 单测保证）。
5. **cancel_thinking reason 不允许空字符串语义漂移**：reason 字段写进 exit_reason 字符串 + 日志；Agent 主动取消时 reason 传「cancel_by_agent_self」或业务场景描述，禁止传空（空 = 前端用户取消，语义完全不同）。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止 CoreTool.invoke 内部直接查 DAO/DAL**：必须全走 RuntimeDomain 方法（management.rs）；`register_runtime_tools()` 构造时注入 `Arc<dyn RuntimeDomain>` 而非 DAO 实例——违反分层架构（Domain 下才是 DAL/DAO）。
2. ❌ **禁止 runtime_status 把 Agent 的思考 prompt / 中间输出完整返回给工具**：状态视图只返回 rounds/tokens/策略命中这类**诊断元数据**，思考内容属于 WorkingMemory 敏感数据，工具侧无权访问；如果 Agent 需要读自己的思考过程，走 WorkingMemory 专用记忆工具（不在本次 d1d1013f 范围内）。
3. ✅ **强制幂等测试**：`cancel_thinking` 单测必须覆盖 ① 取消未启动 runtime → 返回 success, not_started=true；② 同一 trace_id 连续取消 5 次 → 每次 success，第 2~5 次 already_cancelled=true；③ 取消已退出 runtime → success, already_exited=true（3 条全过）。
4. ✅ **强制权限测试**：3 个工具每个至少 1 条「权限拒绝」单测 + 1 条「权限允许」：Agent A 调 runtime_status(trace_id = Agent_B) → 403，Agent A 调自己 → 200。
5. ✅ **schema = DTO 对齐测试**：`runtime_tool_schema_eq_handler_dto` 对比 3 个工具的 JSON Schema input 字段集合 vs 对应 common/src/api/runtime.rs 里的 Request DTO 字段集合（字段名 + 类型完全一致，允许工具 schema 缺 path/query 上的 org_id 这种 context 注入字段）。
6. ❌ **禁止 runtime_list 返回已退出 runtime**：必须从 aop think registry（仅活跃注册）取键集合，再去 Domain 侧查；已退出 runtime 在 registry 中 unregister 了所以不会出现；禁止走 SQL status=running 查（status=running 但进程已经 panic 挂掉的僵尸 runtime 会误报，与前端面板的 runtime 列表语义不一致）。
7. ✅ **四类互引闭环**：本卡 source_files[] 含 2 张 Wiki 长文（运行时领域 + 思考面板）+ 2 张兄弟 RAG 卡 + 2 个 Design + 1 个 Plan 占位；运行时领域长文 & 思考面板长文 cite 区必须回链本卡绝对路径 + 兄弟卡路径。
8. ✅ **exit_reason 前缀强制约定**：cancel_thinking reason 传 "cancel_by_agent_self" 时，exit_reason 日志前缀 = `cancel_by_agent_self:{用户传入reason}`；前端取消 = `user_cancel_frontend`。前缀不一致 = Agent 诊断时无法正确区分「主动退出 vs 用户取消 vs 超时」，策略引擎的沉淀日志分析会错判。
