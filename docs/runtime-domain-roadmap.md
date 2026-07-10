# Runtime Domain 推进路线图

> 🎯 **目标**：按阶段推进 Runtime Domain 的完整实现，从"能唤醒"到"能做事"到"能协作"
>
> **当前版本**：v1.0（2026-07-10）
> **状态**：Phase 2 完成，神经工具集已上线
>
> **文档定位**：总体规划 + 各阶段入口，每个阶段开始前在 `docs/superpowers/plans/` 下细化具体执行方案

---

## 一、当前状态总览

### 1.1 已实现模块

| 模块 | 完成度 | 说明 |
|------|--------|------|
| **RuntimeMemory** | ✅ 100% | 完整 CRUD（search/query/create/update/delete）+ Trace 闭环 |
| **ContextAssembly** | ✅ 100% | Builder 模式 Prompt 拼装，PO 自格式化，神经工具自动注入 |
| **Awakening** | ✅ 100% | 9 步主流程完整，状态机管理，统计上报，神经工具注入 |
| **ToolExecution** | ✅ 100% | 协议路由（MCP/Builtin/HTTP）、Manual 授权、神经工具免绑定、Trace 查询 |
| **RuntimeState** | ✅ 100% | DashMap 内存状态管理，Idle/Resting/Busy 三态 |
| **神经工具集** | ✅ 100% | 8 个神经工具（记忆 5 个 + send_message + request_tool_call + mark_done + list_tools） |

### 1.2 当前能力边界

**能做的：**
- Agent 可以被唤醒并进行一次模型推理
- 推理过程会记录 Trace（输入/输出完整记录）
- 工具可以被调用（Manual 模式，经 Runtime Domain 路由）
- Agent 运行时状态可以被查询（空闲/忙碌/休息）
- Agent 拥有 8 个天生神经工具，无需绑定即可调用
- Agent 通过 `send_message` 神经工具主动发送消息（框架不再自动回复）
- Memory 完整 CRUD 能力通过 RuntimeMemory trait 统一暴露

**不能做的：**
- ✅ 消息消费者已实际调用 awaken()（Phase 1 完成）
- ✅ Agent 拥有神经工具集（Phase 2 完成）
- ❌ 工具调用结果不会触发下一次思考
- ❌ 没有唤醒轮次限制（会无限循环？）
- ❌ Resting 状态没有实际用途

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

- [ ] 单元测试：ToolStatsDao 各查询方法正确
- [ ] 单元测试：Agent 附带信息（with_stats）正确注入
- [ ] 单元测试：Agent 唤醒次数按 task_id 过滤正确
- [ ] 集成测试：Agent 调用工具 → 工具执行 → 结果触发再次思考
- [ ] 集成测试：达到最大轮次后停止，用户发新消息恢复
- [ ] 集成测试：调用 mark_done 后任务完成，后续消息不触发唤醒
- [ ] 所有现有测试通过

**执行方案**：待在 `docs/superpowers/plans/` 下创建具体实现方案

---

## Phase 4：技能与记忆增强

> **目标**：Agent 能利用技能库和长期记忆，变得更"聪明"
>
> **核心交付**：技能动态注入、记忆压缩、知识突触构建
>
> **预估工作量**：中-大

### 任务清单

| # | 任务 | 说明 | 优先级 |
|---|------|------|--------|
| 4.1 | 技能动态注入 | 根据 Agent 绑定的技能，自动注入到 Prompt | P0 |
| 4.2 | Resting 状态实现 | 连续工作 N 轮后自动休息，休息期间压缩上下文 | P1 |
| 4.3 | 短期记忆摘要 | 把多轮对话压缩成摘要，存入短期记忆 | P1 |
| 4.4 | 长期记忆沉淀 | 重要信息沉淀为长期记忆（知识突触） | P2 |
| 4.5 | 用户画像构建 | 客服类 Agent 构建用户画像，个性化回复 | P2 |

### 关键设计点

- **休息触发条件**：连续工作轮数？还是 token 消耗量？
- **记忆压缩时机**：休息期间做，还是每轮都做增量？
- **技能注入方式**：Prompt 里加技能说明，还是作为 tool 让 Agent 主动调用？

### 验收标准

- [ ] 单元测试：技能注入到 Prompt 正确
- [ ] 集成测试：Agent 休息后上下文被压缩
- [ ] 集成测试：Agent 能调用 read_skill 学习新技能
- [ ] 所有现有测试通过

**执行方案**：待在 `docs/superpowers/plans/` 下创建具体实现方案

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

## 三、开发原则

### 3.1 小步推进

每个大阶段拆成多个小任务，每个任务：
- 可独立编译通过
- 有对应的单元测试
- 不破坏现有功能

### 3.2 测试驱动

- 核心业务逻辑必须有单元测试
- 每个阶段完成后有集成测试验证
- 所有改动必须通过现有 544+ 测试

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

**当前阶段**：Phase 2 - 神经工具集（Agent 能做事）

**Phase 1 完成时间**：2026-07-10

**下一步**：在 `docs/superpowers/plans/` 下创建 Phase 2 的具体执行方案

---

## 五、变更记录

| 日期 | 版本 | 变更 |
|------|------|------|
| 2026-07-10 | v0.1 | 初始版本，定义 5 个阶段的总体路线图 |
| 2026-07-10 | v0.9 | Phase 1 完成，更新任务清单和验收标准，进入 Phase 2 |
