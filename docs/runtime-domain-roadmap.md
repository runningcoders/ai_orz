# Runtime Domain 推进路线图

> 🎯 **目标**：按阶段推进 Runtime Domain 的完整实现，从"能唤醒"到"能做事"到"能协作"
>
> **当前版本**：v3.0（2026-07-11）
> **状态**：Phase 4A 完成，工具包机制 + 任务执行闭环已上线
>
> **文档定位**：总体规划 + 各阶段入口，每个阶段开始前在 `docs/superpowers/plans/` 下细化具体执行方案

---

## 一、当前状态总览

### 1.1 已实现模块

| 模块 | 完成度 | 说明 |
|------|--------|------|
| **RuntimeMemory** | ✅ 100% | 完整 CRUD（search/query/create/update/delete）+ Trace 闭环 |
| **ContextAssembly** | ✅ 100% | Builder 模式 Prompt 拼装，PO 自格式化，神经工具自动注入，消息类型差异化 |
| **Awakening** | ✅ 100% | 9 步主流程完整，状态机管理，统计上报，神经工具注入，失败事件记录 |
| **ToolExecution** | ✅ 100% | 协议路由（MCP/Builtin/HTTP）、Manual 授权、神经工具免绑定、工具包免绑定、Trace 查询 |
| **RuntimeState** | ✅ 100% | DashMap 内存状态管理，Idle/Resting/Busy 三态 |
| **神经工具集** | ✅ 100% | 10 个神经工具（记忆 5 个 + send_message + send_tool_call_message + send_task_assignment_message + mark_done + list_tools） |
| **多回合循环控制** | ✅ 100% | 轮次限制检查、任务完成检测、Prompt 上下文差异化、工具失败计数注入 |
| **ToolStatsDao** | ✅ 100% | 工具调用统计 DAO（DuckDB），支持调用次数/失败次数查询 |
| **AgentFetchOptions** | ✅ 100% | 附带信息获取选项，按需注入统计数据 |
| **工具包机制** | ✅ 100% | tag 分组、Agent 入职自动安装、免绑定三层校验、安装/卸载 API |
| **TaskAssignment 消息** | ✅ 100% | TaskAssignment 消息类型、投递方法、神经工具、Handler 编排、PromptBuilder 差异化 |

### 1.2 当前能力边界

**能做的：**
- Agent 可以被唤醒并进行一次模型推理
- 推理过程会记录 Trace（输入/输出完整记录）
- 工具可以被调用（Manual 模式，经 Runtime Domain 路由）
- Agent 运行时状态可以被查询（空闲/忙碌/休息）
- Agent 拥有 10 个天生神经工具，无需绑定即可调用
- Agent 通过 `send_message` 神经工具主动发送消息（框架不再自动回复）
- Memory 完整 CRUD 能力通过 RuntimeMemory trait 统一暴露
- 多回合循环控制：轮次限制、任务完成检测、工具失败警告
- 唤醒失败也记录统计事件，便于排查
- Agent 入职后自动拥有项目管理能力（工具包机制）
- 工具包可按需安装/卸载（API 支持）
- 任务创建后自动通知 Agent（TaskAssignment 消息）
- Agent 间可通过神经工具分配任务（send_task_assignment_message）

**不能做的：**
- ❌ 工具失败率未实时计算（仅记录失败次数）
- ❌ 记忆中轮次状态追踪（轮次信息未写入记忆系统）
- ❌ 多任务并发限制（当前仅按单任务轮次限制）
- ❌ 技能动态注入（Agent 不能根据场景自动加载相关技能）
- ❌ 子任务分解能力（Agent 不能创建子任务形成任务树）
- ❌ 任务进度追踪（百分比、当前步骤、执行历史）

---

## 二、总体推进路线

分为 5 个大阶段，每个阶段有明确的交付目标：

```
Phase 1: 打通端到端链路
    │
    ▼
Phase 2: 神经工具集（Agent 能做事）
    │
    ▼
Phase 3: 多回合循环控制
    │
    ▼
Phase 4: 技能与记忆增强
    │
    ▼
Phase 5: 多 Agent 协作
```

---

## Phase 1：打通端到端链路

> **目标**：用户发一条消息 → Agent 思考 → 回复用户，完整走通一次
>
> **核心交付**：消息消费者真正调用 awaken，Agent 回复自动入队
>
> **预估工作量**：中

### 任务清单

| # | 任务 | 说明 | 优先级 | 状态 |
|---|------|------|--------|------|
| 1.1 | 消息消费者加载 Agent 实体 | handle_agent_message 中通过 HrDomain 加载 Agent（含 Brain） | P0 | ✅ 完成 |
| 1.2 | 调用 runtime_domain.awaken() | 真正调用唤醒方法，不再是占位符 | P0 | ✅ 完成 |
| 1.3 | 唤醒结果处理 | 成功：继续下一步；失败：错误日志 + Nack 重试 | P0 | ✅ 完成 |
| 1.4 | Agent 回复消息入队 | 模型输出 → 构造 Message → send_to_user 入队 | P0 | ✅ 完成 |
| 1.5 | 消费者上下文重建 | 从 MessagePo 重建 RequestContext（org_id、user_id 等） | P0 | ✅ 完成 |
| 1.6 | 唤醒失败的状态清理 | awaken 抛异常时确保 Agent 状态回到 Idle | P1 | ✅ 完成（awaken 内部已实现） |

### 关键设计点

- **Agent 加载方式**：消费者注入 Finance Domain，还是直接用 AgentDal？
  - 倾向：注入 Finance Domain，符合分层架构
- **回复消息构造**：模型输出的纯文本 → Message 实体，from=Agent, to=User
- **上下文重建**：从 MessagePo.organization_id、from_id、project_id、task_id 重建 ctx

### 验收标准

- [x] 单元测试：消费者处理一条用户消息，能成功调用 awaken 并返回
- [x] 集成测试：发消息 → 唤醒 → 回复消息入队，完整链路走通
- [x] 所有现有测试通过（548 个测试 100% 通过）

**执行方案**：[`docs/superpowers/plans/2026-07-10-runtime-domain-phase1-end-to-end.md`](./superpowers/plans/2026-07-10-runtime-domain-phase1-end-to-end.md)

---

## Phase 2：神经工具集（Agent 能做事）

> **目标**：Agent 不只是聊天，能调用工具、查记忆、发消息、标记完成
>
> **核心交付**：一套基础神经工具，Agent 可以自主决定调用
>
> **预估工作量**：大
> **状态**：✅ 已完成（2026-07-10）

### 任务清单

| # | 任务 | 说明 | 优先级 | 状态 |
|---|------|------|--------|------|
| 2.1 | 宏扩展 | `register_handler_tool` 宏增加 `neural` flag 和 `tags` 参数 | P0 | ✅ 完成 |
| 2.2 | 记忆神经工具（5个） | search_memory / query_memory / create_memory / update_memory / delete_memory | P0 | ✅ 完成 |
| 2.3 | send_message 工具 | 给用户发消息（注册为神经工具） | P0 | ✅ 完成 |
| 2.4 | request_tool_call 工具 | 请求调用外骨骼工具（Manual 模式） | P0 | ✅ 完成 |
| 2.5 | mark_done 工具 | 标记任务完成（注册为神经工具） | P1 | ✅ 完成 |
| 2.6 | list_tools 工具 | 列出可用工具（标记为神经工具） | P1 | ✅ 完成 |
| 2.7 | 工具注入 | 唤醒时自动注入带 `"neural"` tag 的工具到 Prompt | P0 | ✅ 完成 |
| 2.8 | 神经工具免绑定 | 调用 Manual 工具时，神经工具不需要绑定校验 | P0 | ✅ 完成 |
| 2.9 | 去掉自动回复 | 修改 message.rs，不再自动 send_to_user，由 Agent 通过 send_message 工具发送 | P0 | ✅ 完成 |
| 2.10 | RuntimeMemory 扩展 | 新增 search/query/create/update/delete 5 个公开方法 | P0 | ✅ 完成 |

### 关键设计点

- **神经工具 vs 外骨骼工具**：
  - 神经工具：Agent 天生就会的（search_memory、send_message 等），通过 `register_handler_tool` 宏的 `neural` flag 标记
  - 外骨骼工具：需要授权的（写文件、调 API），走 Tool Domain + Manual 模式
- **神经工具标识方式**：在 Handler 层使用 `#[register_handler_tool(... neural)]` 标记，生成的 ToolPo 自动包含 `"neural"` tag
- **权限控制**：所有 Agent 默认拥有带 `"neural"` tag 的工具，不需要绑定校验
- **工具调用追踪**：每次工具调用都要记录 Trace，关联到本次思考（已有实现）
- **回复机制**：去掉框架自动发送回复，Agent 通过 `send_message` 神经工具主动发送给用户
- **分层架构（强制）**：Handler 层禁止直接调用 DAL，必须通过 Domain 层接口
  - Memory 神经工具：Handler → `RuntimeDomain.memory()` → MemoryDal
  - 扩展 `RuntimeMemory` trait，新增 search/query/create/update/delete 方法
  - 现有 `get_recent_context` 和 `write_thinking_trace` 保持不变（内部使用）

### 验收标准

- [x] 宏扩展：`register_handler_tool` 支持 `neural` flag 和 `tags` 参数
- [x] RuntimeMemory 扩展：5 个新方法全部实现并委托给 MemoryDal
- [x] 8 个神经工具全部实现（记忆 5 个 + send_message + request_tool_call + mark_done）
- [x] list_tools 标记为神经工具
- [x] 唤醒时自动注入神经工具到 Prompt
- [x] 神经工具调用无需绑定校验
- [x] 移除消息消费者中的自动回复逻辑
- [x] 所有现有测试通过（548 个测试 100% 通过）

**执行方案**：[`docs/superpowers/plans/2026-07-10-runtime-domain-phase2-neural-tools.md`](./superpowers/plans/2026-07-10-runtime-domain-phase2-neural-tools.md)

### 已交付神经工具清单

| 工具 ID | 说明 | 分类 |
|---------|------|------|
| `search_memory` | 关键词 + 向量语义混合搜索记忆 | 记忆 |
| `query_memory` | 通用关系型查询记忆 | 记忆 |
| `create_memory` | 创建新记忆（短期/长期） | 记忆 |
| `update_memory` | 更新已有记忆 | 记忆 |
| `delete_memory` | 删除记忆 | 记忆 |
| `send_message` | 发送消息给用户 | 消息 |
| `request_tool_call` | 请求调用外骨骼工具（异步） | 工具 |
| `mark_done` | 标记任务完成 | 任务 |
| `list_tools` | 列出可用工具 | 工具 |

---

## Phase 3：多回合循环控制

> **目标**：工具调用结果自动触发下一次思考，形成完整的思考→行动→再思考循环
>
> **核心交付**：ToolCallResult 自动触发 awaken，有轮次限制和终止条件，有错误重试策略
>
> **预估工作量**：中-大

### 任务清单

| # | 任务 | 所在层 | 说明 | 优先级 |
|---|------|--------|------|--------|
| 3.1 | ToolStatsDao 建设 | DAO + DAL | 补齐工具统计 DAO + ToolDal 统计接口，风格与其他 StatsDao 一致 | P0 |
| 3.2 | Agent 附带信息扩展 | DAL | AgentQuery 增加 with_stats 等选项，find_by_id/query 按需注入统计信息 | P0 |
| 3.3 | Agent 唤醒次数按 task 过滤 | DAO + DAL | AgentStatsQuery 增加 task_id 可选字段，支持按任务维度查唤醒次数 | P0 |
| 3.4 | 唤醒轮次限制 | 消费者 | handle_agent_message 中通过 Agent 附带信息获取轮次，超限则不唤醒 + 提示用户 | P0 |
| 3.5 | mark_done 终止检测 | 消费者 | handle_agent_message 中检查 task 状态，已 Completed 则直接 ack 不处理 | P0 |
| 3.6 | Prompt 上下文区分 | Runtime | PromptBuilder 按 message_type 调整 current_message 呈现方式 | P1 |
| 3.7 | 工具失败计数注入 Prompt | Runtime | awakening 里通过 ToolDal 查工具失败次数，注入 Prompt 提示 Agent | P2 |
| 3.8 | 唤醒失败事件补全 | Runtime | awakening.rs 失败时也记录 AgentAwakeEvent，status="failed" | P2 |

### 关键设计点

- **ToolCallResult 触发链路已通**：ToolCallResult → to_role=Agent → handle_agent_message → awaken()，无需额外开发
- **轮次限制位置**：消费者层面判断，Runtime Domain 只负责单次唤醒
- **轮次计数方案**：通过统计模块查询（agent_awake_events 表，按 agent_id + task_id 过滤）
- **附带信息模式**：Agent 实体支持 with_stats 等选项，获取实体时按需注入统计信息，不用单独再查
- **两种使用方式**：
  - 只需要统计 → 直接调用 DAL 层标准统计方法（get_stats 等）
  - 已经在获取实体 → 通过 with_xxx 选项，把统计作为附带信息一起带回
- **mark_done 终止**：直接查 task 状态，不走统计模块，简单可靠
- **工具失败计数**：通过 ToolStatsDao 查询 `tool_call_events` 表，按 tool_id + agent_id + status="failed" 过滤
- **会话标识**：有 task_id 按 task_id 统计，没有 task_id 的后续再优化

### 验收标准

- [x] 单元测试：ToolStatsDao 各查询方法正确
- [x] 单元测试：Agent 附带信息（with_stats）正确注入
- [x] 单元测试：Agent 唤醒次数按 task_id 过滤正确
- [x] 集成测试：轮次限制检查达到上限后停止唤醒
- [x] 集成测试：调用 mark_done 后任务完成，后续消息不触发唤醒
- [x] 所有现有测试通过（554 个测试 100% 通过）

**执行方案**：[`docs/superpowers/plans/2026-07-10-runtime-domain-phase3-multi-turn-loop.md`](./superpowers/plans/2026-07-10-runtime-domain-phase3-multi-turn-loop.md)

---

## Phase 4：任务执行闭环 + 技能与记忆增强

> **目标**：Agent 能自主执行完整任务，并利用技能库和长期记忆变得更"聪明"
>
> **核心交付**：工具包机制、任务自动执行闭环、技能动态注入、记忆压缩、知识突触构建
>
> **预估工作量**：大
> **状态**：规划中

### 方向 A：工具包机制 + 任务执行闭环

#### 设计理念

**Agent 能力分层模型**：

能力不仅包含工具，也包含 skill（技能）。天生的不只有工具，也有天生的 skill；入职培训的也不只有工具，也有培训教的 skill。

| 层级 | 来源 | 获取方式 | 工具 | Skill |
|------|------|---------|------|-------|
| 神经能力 | 天生认知 | 自动拥有，免绑定 | 神经工具（search_memory、send_message 等） | 天生 skill（后续讨论） |
| 工具包 | 后天培训 | 入职时统一安装 | project_management 等工具包 | 入职培训 skill（后续讨论） |
| 外骨骼 | 外部授权 | 按需绑定 | 外骨骼工具（写文件、调 API 等） | — |

> **关于 Skill**：本阶段聚焦工具包机制（工具维度）。天生的 skill 和入职培训的 skill 涉及技能注入和 Prompt 组装机制，复杂度较高，后续单独讨论。

**核心决策：项目/任务工具不是神经工具**

Agent "认为做完" ≠ 任务真的完成。任务进度是外部系统的真实数据，Agent 对任务的感知应该通过 memory 系统完成（工作记忆/短期记忆），而不是天生的工具。操作任务系统就像人在 Todoist 上打勾一样，是使用外部系统的行为。

但项目管理能力是 Agent 执行任务的基础能力，应该在入职阶段统一培训安装，不需要逐个绑定。入职时除了安装工具包，后续也会安装相关 skill（如项目管理方法论、任务分解技巧等），本阶段先完成工具包机制。

#### 子阶段拆分

方向 A 拆分为两个子阶段，逐步推进：

**4A-1: 工具调用消息改造**（不引入新消息类型）
- 同步/异步工具调用链路分离
- 新增 `send_tool_call_message` 神经工具（封装 `send_tool_call_request`）
- `request_tool_call` 从神经工具移除，保留为普通 HTTP Handler
- 工具包 tag 机制、项目管理工具包标记
- Agent 入职自动安装、工具包 API

**子阶段拆分**：

| 子阶段 | 包含 Task | 主题 | 依赖关系 |
|--------|----------|------|---------|
| 4A-1a: 基础设施 | Task 1 (installed_tags) + Task 2 (神经工具修正) | 数据模型 + 工具标记 | 无依赖，可并行 |
| 4A-1b: 工具包机制 | Task 3 (tag 标记) + Task 4 (免绑定校验) + Task 5 (唤醒注入) | 核心工具包逻辑 | 依赖 4A-1a |
| 4A-1c: 入职 + API | Task 6 (入职安装) + Task 7 (安装/卸载 API) + Task 8 (验证) | 业务层接入 | 依赖 4A-1b |

**4A-2: TaskAssignment 消息**（引入新消息类型）
- 新增 `MessageType::TaskAssignment`
- Message Domain 新增 `send_task_assignment` 投递方法
- 新增 `send_task_assignment_message` 神经工具（供 Agent 间分配任务）
- 任务创建 Handler 编排 + PromptBuilder 差异化提示

#### 工具调用两种模式

| 模式 | 适用场景 | 执行方式 | Agent 感知 |
|------|---------|---------|-----------|
| **同步（auto）** | 神经工具、工具包工具 | rig 框架直接调用 Handler 函数 | 一次 awaken 内拿到结果 |
| **异步（manual）** | 外骨骼工具 | Agent 调用 `send_tool_call_message` 神经工具发消息 → 消费者执行 → ToolCallResult 消息回 Agent | 跨 awaken 轮次 |

#### 三种角色定位

| 角色 | 职责 | 示例 |
|------|------|------|
| **神经工具 Handler** | 封装 Message Domain 的投递方法，注册为神经工具供 Agent 调用 | `send_tool_call_message`、`send_message`、`send_task_assignment_message` |
| **普通 HTTP Handler** | 直接调用 Domain 完成业务，不注册为工具 | `request_tool_call`（同步调用工具，供 HTTP API 或后续复杂架构使用） |
| **Consumer** | 同服务内直接通过 Domain 执行真实业务逻辑 | `handle_tool_call_request` → `call_manual_tool_for_agent()` + `send_tool_call_result()` |

#### 任务清单

**4A-1: 工具调用消息改造**

| # | 任务 | 说明 | 优先级 |
|---|------|------|--------|
| 4.A.1 | 工具包 tag 机制 | AgentRuntimeConfig 新增 `installed_tags` 字段，记录已安装工具包 | P0 |
| 4.A.2 | 神经工具标记修正 | send_message 补齐 `neural` flag；新增 `send_tool_call_message` 神经工具；`request_tool_call` 从神经工具移除 | P0 |
| 4.A.3 | 项目管理工具包标记 | 所有 project/task 工具加 `tags = "project_management"` | P0 |
| 4.A.4 | 免绑定校验扩展 | tool_execution 中不仅 "neural" 免绑定，已安装 tag 的工具也免绑定 | P0 |
| 4.A.5 | 唤醒时注入工具包工具 | load_neural_tools 扩展为 load_builtin_tools，加载神经工具 + 已安装工具包 | P0 |
| 4.A.6 | 工具包安装/卸载 API | HrDomain 新增 install_tool_pack/uninstall_tool_pack/list_installed_tool_packs | P0 |
| 4.A.7 | Agent 入职默认安装 | Agent 状态变为 Onboarded 时自动安装 "project_management" 工具包 | P0 |

**4A-2: TaskAssignment 消息**

| # | 任务 | 说明 | 优先级 |
|---|------|------|--------|
| 4.A.8 | TaskAssignment 消息类型 | 新增 MessageType::TaskAssignment + payload + 投递方法 | P0 |
| 4.A.9 | send_task_assignment_message 神经工具 | 封装 send_task_assignment，供 Agent 间分配任务 | P0 |
| 4.A.10 | 任务创建自动通知 | create_task Handler 编排：创建任务后发 TaskAssignment 消息 + PromptBuilder 差异化提示 | P0 |

**后续任务**

| # | 任务 | 说明 | 优先级 |
|---|------|------|--------|
| 4.A.11 | 子任务分解能力 | Agent 可以通过项目管理工具包创建子任务，形成任务树 | P1 |
| 4.A.12 | 任务进度追踪 | 百分比、当前步骤、执行历史 | P1 |
| 4.A.13 | 任务失败/重试机制 | 任务执行失败后自动重试或转人工 | P2 |
| 4.A.14 | 任务产物管理 | 执行结果、附件、中间产物 | P2 |

#### 神经工具标记现状

| 工具 ID | 应为神经工具 | 当前标记 | 需修复 |
|---------|-------------|---------|--------|
| `search_memory` | ✅ 是 | ✅ 有 neural | 无需修改 |
| `query_memory` | ✅ 是 | ✅ 有 neural | 无需修改 |
| `create_memory` | ✅ 是 | ✅ 有 neural | 无需修改 |
| `update_memory` | ✅ 是 | ✅ 有 neural | 无需修改 |
| `delete_memory` | ✅ 是 | ✅ 有 neural | 无需修改 |
| `list_tools` | ✅ 是 | ✅ 有 neural | 无需修改 |
| `send_message` | ✅ 是 | ❌ 缺失 neural | **需补齐** |
| `send_tool_call_message` | ✅ 是（新增） | — | **新增** |
| `request_tool_call` | ❌ 否（同步 HTTP API） | 有 register_handler_tool | **需移除** |
| `mark_done` | ❌ 否（属于工具包） | 无 neural（正确） | 需加 `project_management` tag |

#### 工具包清单：project_management

| 工具 ID | 说明 | 当前状态 |
|---------|------|---------|
| `mark_done` | 在任务系统上标记完成 | 已实现，需加 `project_management` tag |
| `create_task` | 在系统中创建新任务 | 已实现，需加 tag |
| `create_subtask` | 创建子任务（待新增） | 待实现 |
| `update_task` | 修改任务信息 | 已实现，需加 tag |
| `get_task` | 查看任务详情 | 已实现，需加 tag |
| `list_project_tasks` | 查看项目下任务列表 | 已实现，需加 tag |
| `list_agent_tasks` | 查看 Agent 分配的任务列表 | 已实现，需加 tag |
| `get_project` | 查看项目信息 | 已实现，需加 tag |
| `list_projects` | 查看项目列表 | 已实现，需加 tag |
| `update_project` | 更新项目信息 | 已实现，需加 tag |
| `update_task_status` | 更新任务状态 | 已实现，需加 tag |
| `update_project_status` | 更新项目状态 | 已实现，需加 tag |

#### 免绑定校验逻辑变更

```
Agent 调用 Manual 工具
    │
    ├── 先在 agent 绑定工具中查找
    │
    ├── 找不到？检查是否是神经工具（tags 含 "neural"）
    │       └─ 是 → 免绑定放行
    │
    ├── 还找不到？检查是否属于已安装的工具包
    │       └─ Agent 已安装该 tag → 免绑定放行
    │
    └── 都不是 → 拒绝：工具未绑定且不属于已安装工具包
```

#### 唤醒时工具注入逻辑变更

```
load_builtin_tools(ctx, agent)
    │
    ├── 加载神经工具（tags 含 "neural"）
    │       └─ 所有 Agent 天生拥有
    │
    └── 加载已安装工具包工具
            └─ 查 Agent 已安装的 tags 列表
            └─ 按 tags 过滤所有启用工具
```

#### 关键设计点

- **工具包 tag 存储**：Agent 的 `runtime_config` 中新增 `installed_tags` 字段，记录已安装的工具包
- **入职安装时机**：Agent 状态从 PendingOnboard → Onboarded 时，自动安装 "project_management" tag
- **mark_done 循环终止**：Agent 入职后自动拥有 project_management 工具包，mark_done 可用，循环终止问题自然解决
- **工具包可卸载**：后续支持卸载某个工具包（如 Agent 不需要项目管理能力）
- **工具包可扩展**：未来可以新增其他工具包（如 "data_analysis"、"code_execution" 等）
- **入职内容分两维度**：入职时安装的不只是工具包（工具维度），后续也会安装相关 skill（技能维度），如项目管理方法论、任务分解技巧等。本阶段先完成工具包机制，skill 维度后续讨论
- **天生能力也分两维度**：天生的不只有神经工具（工具维度），也有天生的 skill（技能维度）。天生 skill 涉及 Prompt 组装机制，复杂度较高，后续单独讨论
- **同步/异步工具调用分离**：auto 工具走 rig 同步调用，manual 工具走 `dispatch_tool_call` 神经工具发消息异步执行
- **神经工具 Handler vs 普通 HTTP Handler**：神经工具 Handler 封装 Message Domain 投递方法并注册为工具；普通 HTTP Handler 直接调用 Domain，不注册为工具
- **消费者直接通过 Domain 执行业务**：同服务内消费者不经过 Handler，直接调用 Domain 完成工具执行、结果回写等
- **TaskAssignment 消息机制**：Project Domain 只管数据持久化，Message Domain 负责通知，Handler 层编排两个 Domain

### 方向 B：记忆增强

| # | 任务 | 说明 | 优先级 |
|---|------|------|--------|
| 4.B.1 | 短期记忆摘要 | 把多轮对话压缩成摘要，存入短期记忆 | P1 |
| 4.B.2 | 长期记忆沉淀 | 重要信息沉淀为长期记忆（知识突触） | P2 |
| 4.B.3 | 语义向量搜索落地 | 当前 SQLite VSS 扩展已支持，需要在记忆搜索中实际使用 | P1 |
| 4.B.4 | 任务相关记忆自动关联 | 执行任务时自动关联相关记忆 | P2 |
| 4.B.5 | Agent 自我反思记忆 | 每次执行后总结经验，沉淀为记忆 | P2 |
| 4.B.6 | 工具失败率实时计算 | 基于失败次数计算失败率，动态调整 Prompt 警告 | P2 |

### 方向 C：技能增强

| # | 任务 | 说明 | 优先级 |
|---|------|------|--------|
| 4.C.1 | 技能动态注入 | 根据 Agent 绑定的技能，自动注入到 Prompt | P0 |
| 4.C.2 | Resting 状态实现 | 连续工作 N 轮后自动休息，休息期间压缩上下文 | P1 |
| 4.C.3 | 用户画像构建 | 客服类 Agent 构建用户画像，个性化回复 | P2 |

### 关键设计点

- **任务自动执行**：任务创建后如何触发 Agent？通过消息系统还是直接调用 awaken()？
- **记忆压缩时机**：休息期间做，还是每轮都做增量？
- **技能注入方式**：Prompt 里加技能说明，还是作为 tool 让 Agent 主动调用？
- **能力分层两维度**：天生的不仅有工具（神经工具），也有天生的 skill；入职培训的不仅有工具包，也有培训 skill。本阶段先完成工具包机制，skill 维度后续讨论

### 验收标准

**4A-1: 工具调用消息改造**

- [x] AgentRuntimeConfig.installed_tags 字段完整实现
- [x] send_message 补齐 neural flag
- [x] send_tool_call_message 神经工具新增
- [x] request_tool_call 从神经工具移除
- [x] project_management 工具包所有工具正确标记 tag
- [x] Agent 入职时自动安装 project_management 工具包
- [x] 免绑定校验支持神经工具 + 已安装工具包
- [x] 唤醒时注入神经工具 + 已安装工具包工具
- [x] 工具包安装/卸载/查询 API 可用
- [x] 所有现有测试通过（569 个测试 100% 通过）

**4A-2: TaskAssignment 消息**

- [x] MessageType::TaskAssignment 定义
- [x] send_task_assignment 投递方法实现
- [x] send_task_assignment_message 神经工具
- [x] 任务创建后自动发送 TaskAssignment 消息
- [x] PromptBuilder 支持 TaskAssignment 差异化提示
- [x] 所有现有测试通过（569 个测试 100% 通过）

**执行方案**：[`docs/superpowers/plans/2026-07-11-runtime-domain-phase4a-tool-pack.md`](./superpowers/plans/2026-07-11-runtime-domain-phase4a-tool-pack.md)

---

## Phase 5：多 Agent 协作

> **目标**：多个 Agent 可以协作完成复杂任务
>
> **核心交付**：Agent 间消息传递、任务分发、结果汇总
>
> **预估工作量**：大

### 任务清单

| # | 任务 | 说明 | 优先级 |
|---|------|------|--------|
| 5.1 | Agent 间消息传递 | Agent A 发消息给 Agent B，触发 B 的唤醒 | P0 |
| 5.2 | 任务分发模式 | 主 Agent 把子任务分发给子 Agent | P1 |
| 5.3 | 结果汇总模式 | 子 Agent 完成后汇总结果给主 Agent | P1 |
| 5.4 | 团队角色配置 | 组织内 Agent 团队的角色分工配置 | P2 |
| 5.5 | 协作模式模板 | 常见协作模式（主管-执行者、评审-作者等） | P2 |

### 关键设计点

- **Agent 间消息格式**：和用户消息一样走 Message 表？还是专用格式？
- **身份标识**：from_role=Agent 时，接收方如何识别发送者身份？
- **权限控制**：Agent A 能不能给任何 Agent 发消息？还是有组织限制？

### 验收标准

- [ ] 集成测试：Agent A 发消息给 Agent B，B 被唤醒
- [ ] 集成测试：主 Agent 分发任务，子 Agent 完成后回传结果
- [ ] 所有现有测试通过

**执行方案**：待在 `docs/superpowers/plans/` 下创建具体实现方案

---

## Phase 6：前端完善

> **目标**：把当前后端能力在前端 UI 中完整展现
>
> **核心交付**：任务看板、对话界面、统计仪表盘、执行进度可视化
>
> **预估工作量**：中
> **状态**：规划中

### 任务清单

| # | 任务 | 说明 | 优先级 |
|---|------|------|--------|
| 6.1 | 任务看板/列表视图 | 展示任务状态、进度、分配关系 | P0 |
| 6.2 | Agent 对话界面优化 | 工具调用可视化、消息类型区分展示 | P0 |
| 6.3 | 统计仪表盘 | 调用次数、成功率、Token 消耗、唤醒次数 | P1 |
| 6.4 | 任务执行进度实时展示 | 当前轮次、思考深度、工具调用历史 | P1 |
| 6.5 | Agent 管理界面 | Agent 配置、工具绑定、技能绑定 | P2 |

### 关键设计点

- **实时更新方式**：WebSocket 推送 vs 轮询？
- **工具调用可视化**：展示调用参数、返回结果、耗时
- **统计图表**：使用什么图表库？D3.js？Chart.js？

---

## 三、开发原则

### 3.1 小步推进

每个大阶段拆成多个小任务，每个任务：
- 可独立编译通过
- 有对应的单元测试
- 不破坏现有功能

### 3.2 测试驱动

- 核心业务逻辑必须有单元测试
- 每个阶段完成后有集成测试验证
- 所有改动必须通过现有 554+ 测试

### 3.3 文档同步

- 每个阶段开始前：在 `docs/superpowers/plans/` 下创建具体执行方案
- 每个阶段完成后：更新 `docs/runtime_design.md` 对应章节
- 关键设计决策记录在案

### 3.4 架构约束

严格遵守分层架构：
```
Handler → Domain → DAL → DAO → Models
```

- Runtime Domain 内部子模块可以互相调用（memory、awakening、tool_execution）
- 跨 Domain 调用必须通过 Domain trait 接口
- 禁止 Domain 层直接调用 DAO

---

## 四、当前阶段

**当前阶段**：Phase 4A 已完成，准备进入 Phase 4B/4C 或 Phase 5

**Phase 1-3 完成时间**：2026-07-10
**Phase 4A 完成时间**：2026-07-11

**下一步可选方向**：
1. **Phase 4B：记忆增强** — 短期记忆摘要、长期记忆沉淀、语义向量搜索落地
2. **Phase 4C：技能增强** — 技能动态注入、Resting 状态实现、用户画像构建
3. **Phase 4A 后续任务** — 子任务分解、任务进度追踪、任务失败/重试机制
4. **Phase 5：多 Agent 协作** — Agent 间消息传递、任务分发、结果汇总

---

## 五、变更记录

| 日期 | 版本 | 变更 |
|------|------|------|
| 2026-07-11 | v3.0 | Phase 4A 完成，工具包机制 + TaskAssignment 消息上线，569 测试通过 |
| 2026-07-11 | v2.3 | 命名修正：dispatch_tool_call → send_tool_call_message，dispatch_task_assignment → send_task_assignment_message；4A-1 进一步拆分为 4A-1a/1b/1c 三个子阶段 |
| 2026-07-11 | v2.2 | Phase 4A 拆分为 4A-1（工具调用消息改造）和 4A-2（TaskAssignment 消息）两个子阶段；架构修正：同步/异步工具调用分离，新增 dispatch_tool_call 神经工具，request_tool_call 从神经工具移除 |
| 2026-07-11 | v2.1 | Phase 4 方向 A 细化：工具包机制 + 任务执行闭环，补充能力分层两维度说明 |
| 2026-07-10 | v2.0 | Phase 3 完成，多回合循环控制上线，554 测试通过 |
| 2026-07-10 | v0.1 | 初始版本，定义 5 个阶段的总体路线图 |
| 2026-07-10 | v0.9 | Phase 1 完成，更新任务清单和验收标准，进入 Phase 2 |
