🎯定位：在策略引擎+AgentThinkRuntime 已交付基础上，暴露 3 个 HTTP 接口（runtime-status/cancel-thinking/runtime-list）+ 在 DuckDB AgentAwakeEvent 统计事件中持久化 exit_reason 字段
状态：v1.0（2026-08-16，落地快照）
触发场景：前端运行时面板需要 HTTP 入口获取数据；取消思考需要后端接口；统计分析需要按退出原因（success/cancelled/error/overflow/max_rounds）维度聚合分析
关联文档段：
- 对应 design 文档：[docs/design/runtime_design.md](docs/design/runtime_design.md) 第 25.13 节（v3.8 增量：运行时 API + exit_reason）
- Wiki 长文真实路径：
  - [docs/wiki/zh/content/功能模块/工具生态系统/运行时诊断工具组.md](docs/wiki/zh/content/功能模块/工具生态系统/运行时诊断工具组.md)（运行时 3 接口组化说明）
  - [docs/wiki/zh/content/基础设施/AOP 事件系统/事件消费者/思考轮次统计消费者.md](docs/wiki/zh/content/基础设施/AOP%20事件系统/事件消费者/思考轮次统计消费者.md)（exit_reason 统计说明）
- RAG 卡真实路径：
  - [docs/wiki/knowledge/zh/思考退出原因 exit_reason 统计与 ThinkRoundEvent AOP 事件链路/思考退出原因 exit_reason 统计与 ThinkRoundEvent AOP 事件链路.md](docs/wiki/knowledge/zh/思考退出原因%20exit_reason%20统计与%20ThinkRoundEvent%20AOP%20事件链路/思考退出原因%20exit_reason%20统计与%20ThinkRoundEvent%20AOP%20事件链路.md)
  - [docs/wiki/knowledge/zh/运行时诊断工具注册为 Agent 可调用工具：runtime-status cancel-thinking runtime-list 三接口工具化/运行时诊断工具注册为 Agent 可调用工具：runtime-status cancel-thinking runtime-list 三接口工具化.md](docs/wiki/knowledge/zh/运行时诊断工具注册为%20Agent%20可调用工具：runtime-status%20cancel-thinking%20runtime-list%20三接口工具化/运行时诊断工具注册为%20Agent%20可调用工具：runtime-status%20cancel-thinking%20runtime-list%20三接口工具化.md)

---

## 一、目标

| 问题 | 方式 |
|------|------|
| StateManager 层已有 think_runtime 方法，但未暴露 HTTP 接口 | 新建 3 个 Handler + DTO，对称封装 3 个 RuntimeDomain 方法 |
| 前端 Agent 详情页/工作台无法实时查询运行时状态与快照 | GET /agents/{id}/runtime-status：返回 state + 业务上下文 + ThinkRuntimeInfo 快照 |
| 用户无法主动取消浪费的思考（无后端入口） | POST /agents/{id}/cancel-thinking：翻转 cancel_flag AtomicBool，返回 success/message |
| 工作台无全局运行中 Agent 列表视角 | GET /agents/runtime-list：支持 state/task_id/project_id 三参数过滤 |
| AgentAwakeEvent 统计事件缺少退出原因维度，DuckDB 无法聚合分析 | AgentAwakeEvent 结构新增 exit_reason String 字段；create_table/bulk_insert 扩展列；success/cancelled/error 三条路径分别写入 |
| Cancelled 分支无统计事件上报（之前直接 return 跳过） | awaken Cancelled 分支补齐 record_event(AgentAwakeEvent) + publish(AgentLoopEvent::finished) 双发布 |

收敛一句话：common DTO 新增 runtime 模块 + 3 个 Handler + 3 条路由 + exit_reason 字段贯穿统计事件三条路径，前端可观测、取消有入口、统计有维度。

---

## 二、架构思路

```
前端运行时面板 ──────────────────────────────────────────────────────────────
    │                    │                       │
    │ GET runtime-status │ POST cancel-thinking  │ GET runtime-list?state=&task_id=&project_id=
    ▼                    ▼                       ▼
3 个 HTTP Handler（hr/agent 模块，RequestContext 鉴权）
    │                    │                       │
    ├─ runtime_status_handler ── RuntimeDomain::get_runtime_status() ──┐
    ├─ cancel_thinking_handler ── RuntimeDomain::cancel_thinking() ────┤── 对称封装
    └─ runtime_list_handler ──── RuntimeDomain::list_runtime_agents() ──┘
                                      │
                                      ▼
                           StateManager 层（前一阶段已交付）
                                      │
           ┌──────────────────────────┼──────────────────────────┐
           ▼                          ▼                          ▼
  get_think_runtime_snapshot   cancel_thinking          list_runtime_agents
  (RwLock 读快照克隆)           (AtomicBool 翻转)         (DashMap 迭代 + 三参数过滤)


统计侧 exit_reason 贯穿：
awaken() 三条退出路径
    │
    ├─ 成功路径 (Final 正常返回)
    │     → record_event! AgentAwakeEvent { status: "success", exit_reason: exit_reason.to_lowercase() }
    │       （已有 exit_reason 局部变量，之前未写入事件字段，新增补上）
    │
    ├─ 取消路径 (ThinkLoopResult::Cancelled)
    │     → 新增：record_event! AgentAwakeEvent { status: "cancelled", exit_reason: "cancelled" }
    │     → 新增：AgentLoopEvent::finished("awaken", "cancelled", duration_ms) AOP 发布
    │     → 直接返回（不触发总结）
    │
    └─ 失败路径 (? 错误上抛 before 任何分支)
          → record_event! AgentAwakeEvent { status: "failed: ...", exit_reason: "error" }

DuckDB agent_awake_events 表新增 exit_reason VARCHAR 列
→ 未来可按退出原因维度做时间序列聚合（成功率/取消率/错误率）
```

---

## 三、涉及文件清单

| 层次 | 文件路径 | 职责 |
|------|----------|------|
| common DTO（新建） | [common/src/api/runtime.rs](common/src/api/runtime.rs) | RuntimeStatusResponse + ThinkRuntimeInfo + CancelThinkingResponse + RuntimeListRequest + RuntimeListResponse |
| common DTO 注册 | [common/src/api/mod.rs](common/src/api/mod.rs) | pub mod runtime; pub use runtime::*; |
| pkg stats 扩展 | [src/pkg/stats/agent_awake.rs](src/pkg/stats/agent_awake.rs) | AgentAwakeEvent 新增 exit_reason: String 字段（#[metric]）+ with_exit_reason builder + create_table/insert/bulk_insert SQL 扩展列 |
| pkg 状态扩展 | [src/pkg/agent_runtime_state.rs](src/pkg/agent_runtime_state.rs) | StateManager 新增 list_runtime_agents(state_filter, task_id_filter, project_id_filter)：DashMap 迭代+三参数过滤 |
| domain trait 扩展 | [src/service/domain/runtime/mod.rs](src/service/domain/runtime/mod.rs) | RuntimeDomain trait 新增 3 方法：cancel_thinking / get_runtime_status / list_runtime_agents；RuntimeDomainImpl 对称实现（委托 StateManager） |
| domain 事件发布 | [src/service/domain/runtime/awakening.rs](src/service/domain/runtime/awakening.rs) | Cancelled 分支补齐 AgentAwakeEvent(exit_reason=cancelled) + AgentLoopEvent::finished 双发布；成功路径补 exit_reason.to_lowercase()；失败路径补 exit_reason="error" |
| handler（新建） | [src/handlers/hr/agent/runtime_status.rs](src/handlers/hr/agent/runtime_status.rs) | GET /agents/{id}/runtime-status：state 枚举映射字符串 + ThinkRuntimeSnapshot → ThinkRuntimeInfo DTO |
| handler（新建） | [src/handlers/hr/agent/cancel_thinking.rs](src/handlers/hr/agent/cancel_thinking.rs) | POST /agents/{id}/cancel-thinking：success 判断 + toast 描述消息 + log_info |
| handler（新建） | [src/handlers/hr/agent/runtime_list.rs](src/handlers/hr/agent/runtime_list.rs) | GET /agents/runtime-list：Query 提取三参数 → list_runtime_agents → items.map 构造 DTO + total 计数 |
| handler 模块注册 | [src/handlers/hr/agent/mod.rs](src/handlers/hr/agent/mod.rs) | pub mod runtime_status/cancel_thinking/runtime_list + pub use handler 函数 |
| 路由注册 | [src/router.rs](src/router.rs) | hr_routes 注册三条路由：GET runtime-list（在 agent/{id} 前避免路由冲突） + GET runtime-status + POST cancel-thinking |
| pkg 测试扩展 | [src/pkg/agent_runtime_state.rs](src/pkg/agent_runtime_state.rs) 测试模块 | 新增 5 个 list_runtime_agents UT：无过滤 / by_state / by_task_id / by_project_id / 组合过滤 |

⭐ **落地索引（四类互引）**
- Wiki 长文 1（运行时诊断工具）：[docs/wiki/zh/content/功能模块/工具生态系统/运行时诊断工具组.md](docs/wiki/zh/content/功能模块/工具生态系统/运行时诊断工具组.md)
- Wiki 长文 2（思考轮次统计）：[docs/wiki/zh/content/基础设施/AOP 事件系统/事件消费者/思考轮次统计消费者.md](docs/wiki/zh/content/基础设施/AOP%20事件系统/事件消费者/思考轮次统计消费者.md)
- RAG 卡 1（exit_reason 事件链路）：[docs/wiki/knowledge/zh/思考退出原因 exit_reason 统计与 ThinkRoundEvent AOP 事件链路/思考退出原因 exit_reason 统计与 ThinkRoundEvent AOP 事件链路.md](docs/wiki/knowledge/zh/思考退出原因%20exit_reason%20统计与%20ThinkRoundEvent%20AOP%20事件链路/思考退出原因%20exit_reason%20统计与%20ThinkRoundEvent%20AOP%20事件链路.md)
- RAG 卡 2（三接口工具化）：[docs/wiki/knowledge/zh/运行时诊断工具注册为 Agent 可调用工具：runtime-status cancel-thinking runtime-list 三接口工具化/运行时诊断工具注册为 Agent 可调用工具：runtime-status cancel-thinking runtime-list 三接口工具化.md](docs/wiki/knowledge/zh/运行时诊断工具注册为%20Agent%20可调用工具：runtime-status%20cancel-thinking%20runtime-list%20三接口工具化/运行时诊断工具注册为%20Agent%20可调用工具：runtime-status%20cancel-thinking%20runtime-list%20三接口工具化.md)

---

## 四、分发点速查表

| HTTP 路由 | 方法 | Handler | 委托 domain 方法 | 过滤参数 |
|-----------|------|---------|-----------------|----------|
| /api/v1/hr/agents/runtime-list | GET | runtime_list_handler | RuntimeDomain::list_runtime_agents | state?task_id?project_id（Option） |
| /api/v1/hr/agents/{id}/runtime-status | GET | runtime_status_handler | RuntimeDomain::get_runtime_status | agent_id path |
| /api/v1/hr/agents/{id}/cancel-thinking | POST | cancel_thinking_handler | RuntimeDomain::cancel_thinking | agent_id path |

| 退出路径 | exit_reason 值 | status 值 | AgentLoopEvent 发布 |
|----------|---------------|-----------|-------------------|
| Final 成功正常返回 | 已有 exit_reason 变量小写化（如 finished/summary） | "success" | ✓ 已存在 |
| 用户取消 Cancelled | "cancelled" | "cancelled" | ✓ 本阶段新增（status=cancelled） |
| 中途错误 ? 上抛 | "error" | "failed: {e}" | ✓ 已存在（补字段） |

---

## 五、验收清单

| 验收项 | 结果 |
|--------|------|
| common/runtime.rs DTO 5 结构全部 Serialize/Deserialize，与 pkg 层枚举/结构字段对齐 | ✓ 已落地 |
| common/src/api/mod.rs 正确注册 runtime 模块与 re-export | ✓ 已落地 |
| AgentAwakeEvent 新增 exit_reason 字段 + with_exit_reason；CREATE TABLE/INSERT/BULK INSERT SQL 全量扩展 exit_reason VARCHAR 列 | ✓ 已落地 |
| StateManager::list_runtime_agents 三参数过滤（None 不过滤，Some 精确匹配）逻辑正确 | ✓ 已落地 |
| RuntimeDomain trait 新增 3 方法签名；RuntimeDomainImpl 正确委托 StateManager（get_runtime_status 需处理 Agent 不存在场景返回 Idle 元组） | ✓ 已落地 |
| awaken Cancelled 分支补齐 record_event!（exit_reason=cancelled, status=cancelled）+ AgentLoopEvent::finished("awaken", "cancelled", duration_ms）双发布；成功/失败路径对称补 exit_reason 字段写入 | ✓ 已落地 |
| 3 个 Handler 正确注入 RequestContext（鉴权）+ API Response 包装：runtime_status 正确映射 state 字符串 + snapshot→ThinkRuntimeInfo；cancel_thinking success=false 有友好描述；runtime_list 构造 items+total | ✓ 已落地 |
| handler/mod.rs 3 模块注册 + pub use 导出 | ✓ 已落地 |
| router.rs 路由顺序：runtime-list 在 agent/{id} 之前避免 {id}=runtime-list 误匹配；runtime-status 与 cancel-thinking 在 status 路由之后对称挂载 | ✓ 已落地 |
| 5 个 list_runtime_agents UT：无过滤/by_state/by_task_id/by_project_id/组合过滤 全部 PASS | ✓ 已落地 |
| cargo clippy --all-targets -- -D warnings 零警告通过 | ✓ 已落地 |
| 四类互引占位路径已写入 | ✓ |

---

## 六、执行结果摘要

| 指标 | 值 |
|------|----|
| 新建文件数 | 4 个（common/runtime.rs DTO + 3 个 handler 文件） |
| 修改文件数 | 7 个（common/mod.rs + stats/agent_awake.rs + pkg/agent_runtime_state.rs + runtime/mod.rs + awakening.rs + agent/mod.rs + router.rs） |
| 新增代码行（约） | 850 行（DTO 约 130 行 / stats 扩展约 80 行 / StateManager+domain 扩展约 180 行 / awaken 事件补约 60 行 / 3 Handler 约 320 行 / 路由+注册约 30 行 / UT 约 50 行） |
| HTTP 接口新增数量 | 3 个（GET list / GET status / POST cancel） |
| exit_reason 统计维度值 | success / cancelled / error 三类（覆盖三条路径） |
| DuckDB 表新增列 | 1 列 agent_awake_events.exit_reason VARCHAR |
| list_runtime_agents 过滤维度 | 3 维独立 + 组合（state × task_id × project_id） |
| 四类互引覆盖率 | design 1/1 + wiki 2/2 + RAG 2/2 = 100% |

---

## 七、后续扩展路径

1. **common 层**：RuntimeListRequest 新增分页参数（page/page_size），当前全量返回适合中小规模 Agent 部署，大规模需分页。
2. **domain 层**：ThinkRoundEvent（思考每轮 AOP 事件）补充 policy_triggered_ids 字段，与 exit_reason 联动，可在 DuckDB 中做单轮退出原因漏斗分析。
3. **handler 层**：3 个运行时接口注册为 CoreTool 内置工具（runtime 诊断 tag），Agent 可自查/自查同伴运行时状态（需严格作用域过滤）。
4. **前端**：Workspace 运行中 Agent 卡片按 exit_reason 历史概率做预测（如最近 7 天取消率高的 Agent 显示橙色预警），提示用户检查配置合理性。
