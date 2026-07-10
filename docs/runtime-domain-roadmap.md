# Runtime Domain 推进路线图

> 🎯 **目标**：按阶段推进 Runtime Domain 的完整实现，从"能唤醒"到"能做事"到"能协作"
>
> **当前版本**：v0.9（2026-07-10）
> **状态**：Phase 1 完成，端到端链路已打通
>
> **文档定位**：总体规划 + 各阶段入口，每个阶段开始前在 `docs/superpowers/plans/` 下细化具体执行方案

---

## 一、当前状态总览

### 1.1 已实现模块

| 模块 | 完成度 | 说明 |
|------|--------|------|
| **RuntimeMemory** | ✅ 100% | `get_recent_context` + `write_thinking_trace`，Trace 闭环 |
| **ContextAssembly** | ✅ 100% | Builder 模式 Prompt 拼装，PO 自格式化 |
| **Awakening** | ✅ 90% | 7 步主流程完整，状态机管理，统计上报 |
| **ToolExecution** | ✅ 90% | 协议路由（MCP/Builtin/HTTP）、Manual 授权、Trace 查询 |
| **RuntimeState** | ✅ 100% | DashMap 内存状态管理，Idle/Resting/Busy 三态 |

### 1.2 当前能力边界

**能做的：**
- Agent 可以被唤醒并进行一次模型推理
- 推理过程会记录 Trace（输入/输出完整记录）
- 工具可以被调用（Manual 模式，经 Runtime Domain 路由）
- Agent 运行时状态可以被查询（空闲/忙碌/休息）

**不能做的：**
- ✅ 消息消费者已实际调用 awaken()（Phase 1 完成）
- ✅ Agent 回复的消息自动发送给用户（Phase 1 完成）
- ❌ 工具调用结果不会触发下一次思考
- ❌ Agent 没有"神经工具"（search_memory、send_message 等）
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

### 任务清单

| # | 任务 | 说明 | 优先级 |
|---|------|------|--------|
| 2.1 | 神经工具框架 | 定义 NeuralTool trait，适配 rig Tool 接口 | P0 |
| 2.2 | search_memory 工具 | 搜索短期/长期记忆 | P0 |
| 2.3 | send_message 工具 | 给用户/其他 Agent 发消息 | P0 |
| 2.4 | request_tool_call 工具 | 请求调用外骨骼工具（Manual 模式） | P0 |
| 2.5 | mark_done 工具 | 标记任务完成 | P1 |
| 2.6 | list_tools 工具 | 列出可用的外骨骼工具 | P1 |
| 2.7 | read_skill 工具 | 读取技能详情 | P2 |
| 2.8 | search_skill 工具 | 搜索技能库 | P2 |
| 2.9 | 工具注入到 awaken | 唤醒时把神经工具注入模型的 tool list | P0 |

### 关键设计点

- **神经工具 vs 外骨骼工具**：
  - 神经工具：Agent 天生就会的（search_memory、send_message），走 Runtime Domain 内部
  - 外骨骼工具：需要授权的（写文件、调 API），走 Tool Domain + Manual 模式
- **工具调用追踪**：每次工具调用都要记录 Trace，关联到本次思考
- **权限控制**：神经工具是否需要权限？还是所有 Agent 都有？

### 验收标准

- [ ] 单元测试：每个神经工具单独可调用
- [ ] 集成测试：Agent 自主决定调用 search_memory 查历史
- [ ] 集成测试：Agent 自主决定调用 send_message 回复用户
- [ ] 所有现有测试通过

**执行方案**：待在 `docs/superpowers/plans/` 下创建具体实现方案

---

## Phase 3：多回合循环控制

> **目标**：工具调用结果自动触发下一次思考，形成完整的思考→行动→再思考循环
>
> **核心交付**：ToolCallResult 自动触发 awaken，有轮次限制和终止条件
>
> **预估工作量**：中

### 任务清单

| # | 任务 | 说明 | 优先级 |
|---|------|------|--------|
| 3.1 | ToolCallResult 触发唤醒 | 工具执行完成后，结果消息自动触发下一次思考 | P0 |
| 3.2 | 唤醒轮次限制 | 每次对话有最大轮次，超过则暂停 | P0 |
| 3.3 | 任务完成检测 | 检测到 mark_done 调用后，不再继续唤醒 | P0 |
| 3.4 | 统计模块联动 | 轮次预算、进度等从统计模块读取 | P1 |
| 3.5 | 错误重试机制 | 工具调用失败时的重试策略 | P2 |

### 关键设计点

- **触发方式**：ToolCallResult 消息走 to_role=Agent 路径，复用 handle_agent_message
- **轮次限制的位置**：在消费者层面判断，还是在 awaken 内部判断？
  - 倾向：消费者层面判断，Runtime Domain 只负责单次唤醒
- **暂停后的恢复**：用户发新消息时自动恢复

### 验收标准

- [ ] 集成测试：Agent 调用工具 → 工具执行 → 结果触发再次思考
- [ ] 集成测试：达到最大轮次后停止，用户发新消息恢复
- [ ] 集成测试：调用 mark_done 后停止
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
