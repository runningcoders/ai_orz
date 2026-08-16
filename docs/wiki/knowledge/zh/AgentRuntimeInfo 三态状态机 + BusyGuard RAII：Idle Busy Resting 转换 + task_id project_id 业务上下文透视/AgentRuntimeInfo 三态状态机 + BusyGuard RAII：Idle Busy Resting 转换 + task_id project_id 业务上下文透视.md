---
kind: wiki_knowledge_card
name: AgentRuntimeInfo 状态机 + BusyGuard RAII：Idle/Busy/Resting 三态转换 + task_id/project_id 业务上下文 + 前端 runtime-list 过滤
category: pkg agent_runtime_state（纯内存状态全局单例）
scope:
  - "src/pkg/agent_runtime_state.rs"
  - "src/service/domain/runtime/busy_guard.rs"
  - "src/handlers/hr/agent/runtime_status.rs"
  - "src/handlers/hr/agent/runtime_list.rs"
  - "src/service/domain/runtime/awakening.rs"
  - "src/consumer/message.rs"
  - "common/src/enums/agent_runtime_state.rs"
source_files:
  - src/pkg/agent_runtime_state.rs#L16-L28（AgentRuntimeInfo 结构体：state + current_message_id + state_started_at + task_id + project_id + think_runtime 六字段，全 Clone）
  - src/pkg/agent_runtime_state.rs#L266-L280（set_resting：state=Resting，current_message_id 清空，** task_id / project_id 保留不清空 **（沉淀仍在同一业务上下文））
  - src/pkg/agent_runtime_state.rs#L282-L306（set_busy：四个参数 agent_id + message_id + Option<task_id> + Option<project_id>；DashMap entry 原子写入 state_started_at；notify_state_change 事件通知）
  - src/pkg/agent_runtime_state.rs#L308-L338（try_set_busy：修复 TOCTOU 竞态。consumer 先检查 is_unavailable → 再 set_busy 之间被其他 worker 抢占插入。try_set_busy 将"检查+设置"放入同一个 entry 的 write lock 内，原子判定当前 Idle 才设置并返回 true，否则 false）
  - src/pkg/agent_runtime_state.rs#L340-L393（set_idle：清 task_id/project_id/think_runtime/current_message_id，state→Idle；list_runtime_agents：DashMap 全表扫描 + state/task_id/project_id 三重 Option 过滤，支持前端运行中面板按任务/项目视角透视）
  - src/pkg/agent_runtime_state.rs#L105-L115（AgentThinkRuntime 结构体：agent_id + cancel_flag Arc<AtomicBool> + snapshot RwLock<ThinkRuntimeSnapshot>。think_loop 每轮 set_snapshot；外部 cancel-thinking 接口 set cancel_flag = true）
  - src/service/domain/runtime/busy_guard.rs#L1-L34（BusyGuard RAII：Drop 时先 clear_think_runtime 再 set_idle。无论 awaken 返回成功/失败/? /panic，Busy 状态都会恢复 Idle。修复"set_busy 和 set_idle 之间 ? 或 panic 导致 Agent 永远卡死在 Busy"泄漏 bug。注意：Resting 也用 BusyGuard（两者 drop 行为都是 set_idle））
  - src/handlers/hr/agent/runtime_status.rs#L1-L50（GET /api/v1/hr/agents/{id}/runtime-status：查询单 Agent 运行时状态 + 思考快照。返回 RuntimeStatusResponse { state, current_message_id, task_id, project_id, state_started_at, think_runtime: Option<ThinkRuntimeInfo{trace_id,scene,round,max_rounds,tokens,tool_call_count,status}> }）
  - src/handlers/hr/agent/runtime_list.rs#L1-L50（GET /api/v1/hr/agents/runtime-list：全表按 state/task_id/project_id 过滤。参数 state = "busy"/"resting"/"idle"；task_id_filter / project_id_filter 精确匹配。对应前端思考运行时面板顶部三格过滤器）
  - src/service/domain/runtime/awakening.rs set_busy 调用 + BusyGuard::new（awaken 入口：set_busy(&agent_id, &msg_id, options.task.as_ref().map(|t| &t.po.id), options.project.as_ref().map(|p| &p.po.id)); let _guard = BusyGuard::new(agent_id.clone()); …后续 awaken 业务代码任何 ? / panic 都自动恢复 Idle；sleep_and_settle 用 set_resting + 同一个 BusyGuard drop set_idle）
  - src/consumer/message.rs:Ln-Lm（consumer 中 try_set_busy 用法：多实例并发下从同一个 Agent 队列取消息，try_set_busy 返回 false 表示其他 worker 已抢占 → 当前 worker 跳过该消息不唤醒）
  - common/src/enums/agent_runtime_state.rs:Ln-Lm（AgentRuntimeState enum { Idle, Busy, Resting } + #[repr(i32)] + #[derive(sqlx::Type)] + is_unavailable() → Busy || Resting，true 表示不可再接受新消息；默认 Frontend API 序列化用 "idle"/"busy"/"resting" 字符串）
  - docs/design/runtime_design.md
  - docs/design/thinking_task_policy_engine_design.md
  - docs/archive/plan-archive/运行时问题修复.md（核心修复项：BusyGuard RAII 防状态泄漏 + try_set_busy CAS 修复 TOCTOU）
  - docs/archive/plan-archive/唤醒上下文与睡眠约束.md（ThinkingOptions task_id/project_id 注入 = 业务上下文的源头）
  - docs/wiki/zh/content/前端应用/组件系统/业务组件/思考运行时面板观测接口.md
  - docs/wiki/zh/content/架构设计/分层架构设计/Domain 层编排/Runtime 领域编排.md
  - docs/wiki/zh/content/基础设施/AOP 事件系统/事件消费者/消息消费者.md
  - docs/wiki/zh/content/项目概述/核心功能特性/Agent 全生命周期管理/Agent 状态管理.md
  - docs/wiki/zh/content/API 参考/RESTful API/人力资源模块 API/Agent 管理 API.md
  - docs/wiki/zh/content/功能模块/AI Agent 管理/Agent 生命周期管理.md
  - 【平行卡1】Agent 思考运行时 AgentThinkRuntime：挂载清理取消与每轮快照上报 docs/wiki/knowledge/zh/Agent%20思考运行时%20AgentThinkRuntime：挂载清理取消与每轮快照上报/Agent%20思考运行时%20AgentThinkRuntime：挂载清理取消与每轮快照上报.md
  - 【平行卡2】思考运行时前端观测：runtime-status cancel-thinking runtime-list 接口与 runtime_panel 组件 docs/wiki/knowledge/zh/思考运行时前端观测：runtime-status%20cancel-thinking%20runtime-list%20接口与%20runtime_panel%20组件/思考运行时前端观测：runtime-status%20cancel-thinking%20runtime-list%20接口与%20runtime_panel%20组件.md
  - 【平行卡3】思考退出原因 exit_reason 统计与 ThinkRoundEvent AOP 事件链路 docs/wiki/knowledge/zh/思考退出原因%20exit_reason%20统计与%20ThinkRoundEvent%20AOP%20事件链路/思考退出原因%20exit_reason%20统计与%20ThinkRoundEvent%20AOP%20事件链路.md
---

# AgentRuntimeInfo 三态状态机 + BusyGuard RAII + 业务上下文透视

## §1 整体方案
Agent 运行时状态 = **纯内存全局单例**（`AgentRuntimeStateManager::global()`，DashMap 存 agent_id → AgentRuntimeInfo），不持久化。服务重启后状态自动重置（所有 Agent 相当于"休息完毕 → Idle"）。核心目的：
1. **前端观测**：运行中面板按「状态 / 任务 / 项目」三种视角过滤正在工作的 Agent。
2. **避免并发唤醒（TOCTOU）**：多实例 consumer 从同一个 Agent 的消息队列取消息时，必须保证只有一个 worker 能进入 Busy 并唤醒。
3. **状态泄漏零容忍**：无论 awaken 返回/抛错/panic，Busy 状态必须最终退回 Idle。RAII 守卫是关键。

**三态 FSM（有限状态机）与转换关系**：
```
   ┌──────────────────────────────────────────────────────┐
   │                                                      │
   ▼                                                      │
 Idle ──set_busy / try_set_busy──►  Busy（工作中）───┐
  ▲                                     │             │
  │                                     │ awaken 工作完 │ BusyGuard drop → set_idle
  │                                     ▼             │
  └────────set_idle (BusyGuard drop)─── Resting（沉淀中）
                              ▲
                              │
                              └─ set_resting（sleep_and_settle 入口）
```

- **Idle → Busy**：consumer 从消息队列取出 Agent 有待处理消息 → set_busy/try_set_busy 成功 → 触发 awaken；参数 task_id/project_id 作为业务上下文注入
- **Busy → Resting**：awaken 触发压缩循环（ContextOverflow）后调用 sleep_and_settle → 调用 set_resting；**task_id/project_id 保留不清空**（沉淀仍在同一任务/项目语境下完成）
- **任何状态 → Idle**：只有 set_idle。永远通过 BusyGuard 的 drop 自动调用，不要在业务代码里手动 set_idle（忘记某条分支就是状态泄漏）
- **Resting → Busy**：理论上禁止（Resting 也是 set_idle 才恢复 Idle）。但代码未强制阻止（因为 sleep_and_settle 失败时 BusyGuard drop 直接把 Resting 也会转到 Idle）。

**六字段信息：AgentRuntimeInfo（内存快照）**：
| 字段 | 语义 | 何时清空 |
|------|-----|--------|
| state: AgentRuntimeState | Idle/Busy/Resting | （转换时改）|
| current_message_id: Option\<String\> | 当前处理哪条消息，前端可溯源 | set_idle / set_resting |
| state_started_at: i64 | 进入当前状态毫秒时间戳，前端算"已忙碌 xx 分" | 每次状态写入重设 |
| **task_id: Option\<String\>** | 业务上下文：当前处理的任务（前端按任务过滤的关键字段）| **set_idle 清空；set_resting 保留** |
| **project_id: Option\<String\>** | 业务上下文：当前处理的项目（同上）| **set_idle 清空；set_resting 保留** |
| think_runtime: Option\<Arc\<AgentThinkRuntime\>\> | 当前思考运行时句柄（含 cancel_flag + 快照）| set_idle clear_think_runtime |

**TOCTOU 修复（try_set_busy 原子性）**：
- 旧代码 bug：consumer 先 if is_unavailable() → 若可用 → set_busy()。中间的空窗期其他 worker 也做同样判断 → 两个 worker 都认为自己可以设 Busy → 同一 Agent 被并发唤醒两次 → 同一消息被重复处理 → 状态机混乱。
- 修复：`try_set_busy(agent_id, message_id, task_id, project_id)` 把 **判定 Idle + 设置 Busy + 写业务上下文** 三件事放到同一个 DashMap entry 的可变借用 scope 内（{ let mut entry = ...; if is_unavailable return false; ... } 闭块）。DashMap 的 shard 级锁保证这一操作是原子的。成功返回 true，失败返回 false（其他 worker 已抢占），consumer 端跳过该消息。

**BusyGuard RAII Drop 语义（状态泄漏零容忍）**：
```rust
let _ = AgentRuntimeStateManager::global().set_busy(&agent_id, &msg_id, task_id?, project_id?);
let _guard = BusyGuard::new(agent_id.clone());
// 后续任何逻辑：awaken 正常完成 → _guard 出 scope drop set_idle
//            ? 提前返回 → 同上
//            某个 unwrap() panic → 同上（栈回溯时 drop 仍然执行）
//            sleep_and_settle 中间先 set_resting → 最后 _guard drop 还是 set_idle，一切 OK
```

**前端观测三接口（HTTP Handler + register_handler_tool 双暴露，既可前端调用也可作为 LLM 工具）**：
1. `runtime_status(id) → RuntimeStatusResponse`：单 Agent 详细状态 + 思考快照（round/max_rounds/tokens/tool_call_count/status）
2. `runtime_list(state?, task_id?, project_id?) → RuntimeListResponse { items: [...] }`：全表三过滤器；前端运行时面板默认 state=busy，按任务/项目切换透视
3. `cancel_thinking(id) → CancelThinkingResponse`：通过 AgentThinkRuntime.cancel_flag.store(true, Ordering::SeqCst) 发起取消（由策略引擎每轮检测；本卡不深述，见平行卡"思考运行时前端观测"）

**注入链路：业务上下文从 Command 一路透传到 set_busy**：
- 上游：consumer/message.rs 的 handle_agent_message() 从消息 MessagePo 读取 task_id 字段，再从 TaskDAL 查询 task 反查 project_id → 组装 ThinkingOptions.task / project → awaken 调用 set_busy(task_id, project_id)
- 前端 get_agent / update_agent_status：从 runtime_info.task_id / runtime_info.project_id 读到字段 → 构造 GetAgentResponse.current_task_id / current_project_id → 前端详情页展示"当前正在处理任务 X"徽章
- 测试用例：`test_set_idle_clears_context`（set_busy→set_idle→task/project 全 None）、`setup_list_runtime_agents`（4 个 Agent：busy/task1/proj1；busy/task2/proj1；resting/task1/proj2；idle/None/None）→ list_runtime_agents 按 task_id_filter=task1 应返回 busy agent1 + resting agent3（因为 Resting 也保留 task_id！前端"此任务相关的所有正在工作+沉淀的 Agent"都显示）

## §2 关键文件路径表格

| 文件 | 角色 | 关键结构/入口 |
|------|------|-------------|
| [pkg/agent_runtime_state.rs](/src/pkg/agent_runtime_state.rs) | 全局单例 + 三态 + 列表过滤 | AgentRuntimeInfo ~L16；set_busy/try_set_busy ~L282；set_resting ~L266；set_idle ~L340；list_runtime_agents 三过滤 ~L358；AgentThinkRuntime ~L105；单元测试 ~L473 |
| [runtime/busy_guard.rs](/src/service/domain/runtime/busy_guard.rs) | RAII | Drop：先 clear_think_runtime，再 set_idle。全局唯一！|
| [handlers/runtime_status.rs](/src/handlers/hr/agent/runtime_status.rs) | 单 Agent 状态接口 | domain().get_runtime_status → RuntimeStatusResponse |
| [handlers/runtime_list.rs](/src/handlers/hr/agent/runtime_list.rs) | 列表三过滤接口 | domain().list_runtime_agents(state, task_id?, project_id?) |
| [awakening.rs](/src/service/domain/runtime/awakening.rs) | awaken 内 set_busy + BusyGuard + sleep_and_settle 内 set_resting + BusyGuard | awaken 入口 set_busy + _guard ~L200；sleep_and_settle set_resting + _rest_guard ~L635 |
| [consumer/message.rs](/src/consumer/message.rs) | consumer 多实例用 try_set_busy 防并发 | handle_agent_message try_set_busy → false 跳过唤醒 |
| 【① Design 1】runtime_design.md §Agent 状态机 | Idle/Busy/Resting 三态设计 | docs/design/runtime_design.md |
| 【① Design 2】thinking_task_policy_engine_design.md §103 | 业务上下文字段归属决策（task_id/project_id 放 runtime_info vs 其他地方） | docs/design/thinking_task_policy_engine_design.md |
| 【③ Wiki 长文 1】思考运行时面板观测接口.md | 前端调用 runtime_status/runtime_list 的面板说明 | docs/wiki/zh/content/前端应用/组件系统/业务组件/思考运行时面板观测接口.md |
| 【③ Wiki 长文 2】Runtime 领域编排.md | awaken 全流程编排中状态转换位置 | docs/wiki/zh/content/架构设计/分层架构设计/Domain%20层编排/Runtime%20领域编排.md |
| 【③ Wiki 长文 3】Agent 管理 API.md | runtime_status / runtime_list 接口文档 | docs/wiki/zh/content/API%20参考/RESTful%20API/人力资源模块%20API/Agent%20管理%20API.md |
| 【③ Wiki 长文 4】消息消费者.md | consumer 的 try_set_busy 用法说明 | docs/wiki/zh/content/基础设施/AOP%20事件系统/事件消费者/消息消费者.md |
| 【③ Wiki 长文 5】Agent 状态管理.md | Idle/Busy/Resting 三态转换用户视角 | docs/wiki/zh/content/项目概述/核心功能特性/Agent%20全生命周期管理/Agent%20状态管理.md |
| 【平行卡 1~3】AgentThinkRuntime 挂载清理、前端观测、exit_reason 统计 | 思考运行时相关三张卡 | 本卡 source_files 尾绝对路径 3 条 |

## §3 架构约定

1. **三态写入必须通过 StateManager 的 4 个入口（set_busy/try_set_busy/set_resting/set_idle），禁止直接写 DashMap entry**：入口内部会调用 notify_state_change 发 AOP 事件给 stats；手动跳过程序化入口事件就丢了。
2. **task_id/project_id 的"保留/清空"规则 = 按语义不是按字面**：Resting = Busy 期间触发的子流程（沉淀阶段），仍然属于同一个任务/项目的工作，所以 set_resting 保留 task_id。set_idle 意味着"整个工作 100% 结束"，清空。
3. **Resting 状态下前端仍应显示**：前端按"任务视角过滤 Agent"时，Resting 也要算入（因为沉淀是任务的一部分，用户能看到"这个任务的 Agent 还在沉淀记忆中"）。测试用例 setup_list_runtime_agents 的断言已体现此约定。
4. **try_set_busy 是 consumer 默认调用，set_busy 仅用于单线程/非竞争路径（如 HTTP handler 立即触发的 awaken）**。多实例/多线程路径一律用 try_set_busy。
5. **BusyGuard 是唯一的 set_idle 入口**：不要在业务代码里手动 set_idle。哪怕你 100% 确信会执行到？未来有人在两者之间加一条 `return Err(...)` 或 `?` → 立即状态泄漏。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止业务代码手动调用 set_idle**（BusyGuard Drop 是唯一入口）。例外：测试代码 / runtime 内部恢复逻辑（这些地方调用前加注释说明为什么不经过 BusyGuard，以及为什么不会泄漏）。
2. ❌ **禁止 Resting 状态也清空 task_id/project_id**。清空后前端按"任务视角"过滤会丢失沉淀中的 Agent，用户误以为任务工作已结束。反模式：set_resting 内一行 entry.task_id = None; entry.project_id = None → 直接破坏过滤语义。
3. ❌ **禁止 is_unavailable + set_busy 分离（TOCTOU 反模式重现）**。consumer/多 worker 路径必须用 try_set_busy（内部封装原子判断+设置）。
4. ✅ **list_runtime_agents 三过滤器：state/task_id_filter/project_id_filter 必须支持 None 通配 + None 全表不扫字段**。None 即不启用该维度过滤。代码中用 `if let Some(tid) = task_id_filter { ... }`，不要 unwrap 或默认 "" 空串。
5. ✅ **AgentRuntimeInfo 六个字段 Clone 出来后独立不跟 DashMap 实时联动**：前端响应构造时读 info.task_id 是当时快照；若想实时追踪需改调用方持引用，但跨层返回引用需要生命周期标注复杂 → 约定返回 Clone 快照（用户可接受"有几百毫秒延迟"）。
6. ✅ **四类互引闭环**：本卡 source_files[] 含 6 篇 wiki 长文 + 2 Design + Plan 占位 + 3 张思考运行时平行卡；对应 Wiki 长文 cite 段回链本卡 + 2 份 Design + 思考运行时卡组。
