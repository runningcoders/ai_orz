# Runtime Domain 设计

> 🎯 **本文档定位**：Runtime Domain（运行时领域）的整体设计大纲与逻辑思路
>
> 范围：只覆盖**总纲与核心理念**，不下沉到具体工具实现与代码细节
> 状态：v3.5（2026-07-24）
>
> 关联文档：
> - [ARCHITECTURE.md](./ARCHITECTURE.md) - 项目整体架构
> - [memory_design.md](./memory_design.md) - 记忆系统
> - [skill_design.md](./skill_design.md) - 技能系统
> - [tool_design.md](./tool_design.md) - 工具系统
> - [message_interaction_design.md](./message_interaction_design.md) - 消息交互

---

## 一、设计哲学：少即是多

Runtime Domain 是 Agent 的"运行时"，即"把 Agent 唤醒、让它做一次思考"的入口。

### 1.1 三条核心原则

#### 原则一：Runtime 只做"唤醒"，不做"调度"

我们**不实现** ReAct / Plan-Execute / MultiStep 这类固化的循环结构。

Runtime 只负责：
- 给 Agent 一次推理机会
- 给它一组"神经级工具"
- 让它自己决定想什么、调什么、什么时候停

**边界补充（2026-06-24）**：
- 一次 `awaken()` 是一次**外部唤醒轮次**；轮次之间是否继续，由消息事件与统计模块共同驱动，不在 Runtime 内部写 `while` 循环。
- `ToolCallResult` 只是下一次唤醒的触发消息之一；Consumer / Runtime 不代替 Agent 生成最终用户回复。
- 最终用户回复必须由 Agent 在 Awakening 思考循环中主动调用 `send_message` / handler-backed 神经工具完成。
- 思考轮次限制、任务执行进度、Agent 工作状态等运行面判断应来自统一的**统计模块**，而不是零散的消息计数或局部 depth 字段。

**对比传统 Agent 框架**：

| 维度 | 传统框架（LangChain / AutoGPT） | ai_orz Runtime |
|------|-------------------------------|----------------|
| 行为循环 | 框架内置 ReAct / Plan-Execute | 不内置，由 Agent 自主决定 |
| 工具调用 | 框架解析模型输出、调度执行 | Agent 通过神经工具自己调 |
| 终止条件 | 框架判定 `final answer` | Agent 显式调用 `mark_done` |
| 上下文 | 框架预加载所有可用能力 | 极薄，按需"想起来" |

#### 原则二：工具二分——神经 vs 外骨骼

借用生物学的类比，把 Agent 能调用的工具分为两类：

| 类型 | 生物学类比 | 接入方式 | 调用时延 | 典型例子 |
|------|-----------|----------|---------|---------|
| **神经级（Native / Auto）** | 大脑 → 手眼口 | 启动时直接挂到模型 tool list | 同步、回合内 | 查工具列表、查技能列表、读写记忆 |
| **外骨骼级（Manual / Message）** | 借助外部系统（命令行、网络） | 模型产出"调用消息"，走消息总线异步执行 | 异步、跨回合 | 给其他 Agent 发消息、调外部 API、跑长任务 |

**判定原则**：

- 是否需要**借助外部系统**才能完成？→ 外骨骼
- 是否属于 Agent 的**天生认知能力**？→ 神经
- 是否需要**长时间执行 / 跨进程 / 跨网络**？→ 外骨骼

#### 原则三：上下文极薄，能力靠"想"出来

Runtime 拼装的 system prompt 只包含最基础的内容：

- Agent 身份（who am I）
- 一段"你可以这样思考"的元提示
- 神经级工具列表（名字 + 一句话描述，不含 schema）
- 几条基础元技能说明（如何查记忆、如何求助）

**绝对不预加载**：

- ❌ 所有技能的完整内容（让 Agent 用 `search_skill` 主动查）
- ❌ 所有长期记忆（让 Agent 用 `search_memory` 主动查）
- ❌ 所有工具的完整 schema（让 Agent 用 `read_tool_spec` 按需展开）

**类比人类**：人不会一上来把所有知识都摆在意识里——是被当前情境触发，主动"想起"相关知识。Agent 也一样。

### 1.2 这样设计的好处

1. **提示词不膨胀**：上下文极薄，token 成本可控
2. **Agent 行为可塑**：行为模式由模型自己决定，不被框架绑死
3. **复杂能力组合涌现**：基础工具 + 基础技能 → 通过 Agent 自主组合产生复杂行为
4. **可观测性强**：每一次"想起"都是一次显式的工具调用，可追踪
5. **职责清晰**：Runtime 不掺杂业务逻辑，只提供"唤醒 + 基础能力注入"

---

## 二、Runtime Domain 目录结构

当前 `runtime/` 下已有 `tool_execution`（外骨骼通道）和 `memory`（记忆读写）两个子模块。本设计**保持现有子模块定位不变**，新增 `awakening` 作为唤醒入口、`context_assembly` 作为上下文拼装器、`neural_tools` 作为神经工具注册表。

```
src/service/domain/runtime/
├── mod.rs                     // RuntimeDomain trait：聚合所有子模块
├── awakening.rs               // 【新增】唤醒入口：组装上下文 + 跑一次推理 + 落 trace
├── awakening_test.rs
├── context_assembly.rs        // 【新增】上下文拼装器（纯函数，无副作用）
├── context_assembly_test.rs
├── neural_tools.rs            // 【新增】神经级工具注册表（内置工具集合）
├── neural_tools_test.rs
├── memory.rs                  // 【已有】运行时记忆读写
└── tool_execution.rs          // 【已有】外骨骼工具执行通道
```

### 2.1 子模块职责划分

| 子模块 | 职责 | 不做的事 |
|--------|------|---------|
| `awakening` | 唤醒流程编排：取触发上下文 → 调 ContextAssembly → 调 Cortex → 落 trace | 不做工具实现、不做记忆 CRUD |
| `context_assembly` | 纯函数式拼装 `AssembledContext`（system prompt + neural tools + recent traces） | 不调外部 IO、不修改任何状态 |
| `neural_tools` | 注册并暴露内置工具（CoreTool trait 适配） | 不实现业务逻辑（业务委托给 DAL） |
| `memory` | 运行时记忆读写的薄封装 | 不做记忆策略选择 |
| `tool_execution` | 外骨骼工具的异步执行通道 | 不做神经工具调用 |

### 2.2 与其他 Domain 的边界

| 边界 | 谁负责 |
|------|--------|
| Agent 配置（model / tools / skills 绑定关系） | **Finance Domain**（管理面） |
| Agent 运行时唤醒、上下文拼装、记忆读写 | **Runtime Domain**（运行面） |
| 消息投递、消息持久化 | **Message Domain** |
| 技能库 CRUD | **Skill Domain**（如已拆分）/ Finance Domain |

Runtime 只**消费**这些 Domain 提供的能力，不**管理**它们的配置。

---

## 三、核心接口：Awakening

### 3.1 接口定义草案

```rust
#[async_trait]
pub trait Awakening: Send + Sync + Debug {
    /// 唤醒 Agent 处理一次触发。
    ///
    /// 一次调用 = 一次模型推理 + 该回合内的多次工具调用（ChatMessage 多轮对话循环）。
    /// 不在内部做 while-loop；下一回合由外部（消息消费者）再次唤醒。
    async fn awaken(
        &self,
        ctx: RequestContext,
        cmd: AwakenCommand,
    ) -> Result<AwakenOutcome, AppError>;
}

pub struct AwakenCommand {
    pub agent_id: String,
    pub trigger: Trigger,                  // 谁/什么触发的
    pub conversation_id: Option<String>,
}

pub enum Trigger {
    UserMessage { message_id: String },
    AgentMessage { from_agent: String, message_id: String },
    ToolResult { tool_call_id: String },
    Scheduled { schedule_id: String },
    Manual { reason: String },
}

pub struct AwakenOutcome {
    pub messages_emitted: Vec<String>,     // 通过消息工具发出去的消息 ID
    pub tool_calls: Vec<ToolCallEntry>,    // 本回合调用过的神经工具
    pub memory_writes: Vec<String>,        // 本回合写入的记忆 ID
    pub finished: bool,                    // 模型是否给出"完成"信号
}
```

### 3.2 关键设计点

**单回合、无内置循环**：
- Runtime 不实现 `while !done { ... }`
- 如果模型这一回合最后发了一条"问别的 Agent"的消息，本回合就结束
- 等对方回信后，消息消费者再次调 `awaken()` 触发下一回合

**ChatMessage 多轮对话循环（一次唤醒内多次工具调用）**：
- `BrainDal.think()` 接收 `&[ChatMessage]`（User/Assistant/Tool 三类消息），不再是单条扁平 prompt
- Awakening 在一次 `awaken()` 内显式驱动 think → ToolCall? → execute → 追加 Tool 消息 → think 的循环，最多 `MAX_TOOL_ITERATIONS`（当前 10）次
- 模型每轮看到完整对话历史（自己的 `tool_calls` 与对应 Tool 结果），从而能正确判定"是否已拿到所需信息、可以输出 Final 终止"
- 工具调用按 `control_mode` 分发：`Auto` 走 `execute_auto`（直接执行含装饰器），`Manual` 走 `execute_manual`（通过 internal 工具转发）
- 这是 Awakening 层的显式循环，循环内每次 `think()` 都是独立 HTTP 调用 OpenAI 兼容 API

**外部唤醒轮次由统计模块约束**：
- ToolCallResult、AgentMessage、Scheduled 等消息都可以触发新的 `awaken()`；
- 触发前由调度侧查询统计模块，判断当前 task / agent / conversation 是否还有轮次预算；
- 轮次预算耗尽时，系统只停止继续唤醒或要求人工反馈，不伪造 Agent 的最终答复；
- 页面上的任务执行情况、Agent 工作情况、工具调用耗时、Token / 轮次消耗，应和这里的预算判断使用同一套统计数据。

**Trigger 显式化**：
- 每种 trigger 决定 system_prompt 里附加什么开场（"你被用户消息触发"/"你被工具结果触发"）
- 让模型对"为什么我醒了"有显式感知，行为更可控

---

## 四、上下文拼装与神经工具集

### 4.1 AssembledContext 结构

`ContextAssembly` 是**纯函数式**的拼装器，输入 agent_id + trigger，输出最薄上下文：

```rust
pub struct AssembledContext {
    pub system_prompt: String,           // 身份 + 元思考提示 + 基础技能说明
    pub neural_tools: Vec<ToolSpec>,     // 传给模型 function calling 的工具列表
    pub recent_traces: Vec<MemoryTrace>, // 最近 N 条会话 trace（短期工作记忆）
}
```

**只放最薄的内容**：
- ✅ 身份信息（你是谁、归属哪个组织）
- ✅ 元思考提示（"遇到不会的，先想想有没有相关技能/记忆/工具"）
- ✅ 神经工具的名字 + 一句话描述
- ✅ 最近 N 条短期记忆（工作记忆窗口）

**不预加载**：
- ❌ 技能完整内容
- ❌ 长期记忆全集
- ❌ 工具完整 schema

### 4.2 神经工具最小集

每项都是 `CoreTool`，模型推理回合内可直接同步调用。神经工具通过 `register_handler_tool` 宏的 `neural` flag 标记，所有 Agent 默认拥有，不需要权限校验。

| 工具名 | 作用 | 委托给 | 优先级 |
|--------|------|--------|--------|
| `search_memory` | 混合搜索长期/短期记忆 | `RuntimeMemory.search` | P0 |
| `send_message` | 给 user / agent / channel 发消息 | `MessageDomain.delivery` | P0 |
| `mark_done` | 显式标记本任务完成（project_management 工具包） | Runtime 内部 | P1 |
| `list_tools` | "想起"有哪些外骨骼工具可用（仅返回名字+一句话） | `ToolDal` | P1 |
| `search_skill` | "想起"有哪些相关技能 | `SkillDal` | P2 |
| `read_skill` | 取出某个技能的具体内容 | `SkillDal` | P2 |
| `read_tool_spec` | 展开某个工具的完整 schema | `ToolDal` | P2 |
| `write_memory` | 沉淀新记忆 | `RuntimeMemory.write` | P2 |

> **注意（v3.7 架构变更）**：`request_tool_call`（同步）和 `send_tool_call_message`（异步）不再是神经工具，改为 **internal 工具**——带有 `internal` tag，由 `ToolDal::execute_manual` 在内部通过 `registry.create_tool` 创建实例并转发调用。Agent 不能直接调用这两个工具，它们仅作为 Manual 工具调用的内部转发器：
> - 同步路径：`execute_manual` 调用 `request_tool_call` → `call_manual_tool_for_agent` → `call_tool`（含装饰器），同轮返回结果
> - 异步路径：`execute_manual` 调用 `send_tool_call_message` → `MessageDomain.delivery.send_tool_call_request` 消息入队，结果在下一轮 awaken 送达

**设计要点**：
- `list_*` 系列只返回**摘要**（名字+一句话），不含完整 schema，避免 prompt 膨胀
- 真正要用某项能力时，模型主动调 `read_*` 系列展开
- 这正是"想起来"的过程——模型先看到目录，再决定翻哪一页
- 神经工具通过 `#[register_handler_tool(... neural)]` 标记，生成的 ToolPo 自动包含 `"neural"` tag
- internal 工具通过 `tags = "...,internal"` 标记，加载时过滤，不可绑定给 Agent
- 唤醒时只注入带 `"neural"` tag 的工具给模型

**三种角色定位**：

| 角色 | 职责 | 示例 |
|------|------|------|
| **神经工具 Handler** | 封装 Domain 方法，注册为神经工具供 Agent 调用 | `send_message`、`search_memory` |
| **internal 工具 Handler** | 带 `internal` tag，不可绑定给 Agent；由 `ToolDal::execute_manual` 通过 registry 创建实例转发调用 | `request_tool_call`（同步）、`send_tool_call_message`（异步） |
| **普通 HTTP Handler** | 直接调用 Domain 完成业务，不注册为工具 | （供 HTTP API 或前端使用） |
| **Consumer** | 同服务内直接通过 Domain 执行真实业务逻辑 | `handle_tool_call_request` → `call_manual_tool_for_agent()` + `send_tool_call_result()` |

**注册示例**：
```rust
// 神经工具：所有 Agent 默认拥有
#[register_handler_tool(
    id = "send_message",
    name = "send_message",
    description = "Send a message to user",
    params = "common::api::SendMessageParams",
    neural,  // ← 标记为神经工具
)]
async fn send_message(ctx: RequestContext, params: SendMessageParams) -> Result<Value> {
    // ...
}

// internal 工具：不可绑定给 Agent，由 execute_manual 内部转发
#[register_handler_tool(
    id = "request_tool_call",
    name = "request_tool_call",
    description = "Call a manual tool synchronously and get the result immediately",
    params = "common::api::RequestToolCallParams",
    tags = "tool_management,internal"  // ← internal tag 标记
)]
async fn request_tool_call(ctx: RequestContext, params: RequestToolCallParams) -> Result<RequestToolCallResponse> {
    // 内部转发到 call_manual_tool_for_agent → call_tool
}
```

### 4.3 神经 vs 外骨骼的关系图

```
Awakening 显式循环 (一次 awaken 内最多 MAX_TOOL_ITERATIONS 次)
    │
    │  messages: Vec<ChatMessage>  ← 多轮对话历史（User/Assistant/Tool）
    │
    ▼
BrainDal.think(ctx, brain, &messages, &tool_descriptors)
    │   └── brain.model_provider (ModelProviderPo) → CortexDaoRegistry.get(provider_type)
    │       → OpenAiCompatibleCortexDao.think() → POST /chat/completions
    │
    ▼
ThinkResult::Final?  → 退出循环，返回最终回答
ThinkResult::ToolCall?
    │
    │  1. 追加 Assistant 消息（含 tool_calls）到 messages
    │  2. 对每个 tool_call 按 control_mode 分发执行：
    │
    ├── ControlMode::Auto  ──► ToolDal.execute_auto(tool, args)
    │                            └── call_tool (直接执行层，含装饰器)
    │                                  └── ToolCallDao.execute → decorate() → CoreTool::call
    │
    ├── ControlMode::Manual (sync)  ──► ToolDal.execute_manual(tool, args)
    │                                    └── registry.create_tool("request_tool_call")
    │                                        → special_tool.call()
    │                                            └── call_manual_tool_for_agent()
    │                                                → call_tool (含装饰器，同轮返回)
    │
    └── ControlMode::Manual (async) ──► ToolDal.execute_manual(tool, args)
                                         └── registry.create_tool("send_tool_call_message")
                                             → special_tool.call()
                                                 └── MessageDomain.delivery.send_tool_call_request()
                                                     → 消息入队，立即返回"已提交"
                                                                                │
                                                                                ▼
                                                     Consumer 收到 ToolCallRequest 消息（to_role=System）
                                                         │
                                                         ├── tool_execution.call_manual_tool_for_agent()
                                                         │   └── 三层免绑定校验：绑定 → 神经 → 已安装 tag
                                                         │
                                                         └── MessageDomain.delivery.send_tool_call_result() ──► 结果消息
                                                                                │
                                                                                ▼
                                                     Consumer 收到 ToolCallResult 消息（to_role=Agent）
                                                         │
                                                         └── 触发下一次 awaken()

    │  3. 把工具结果作为 ChatMessage::Tool 追加到 messages
    │  4. 回到 BrainDal.think() 继续下一轮
```

> **三层工具调用架构**：
> - **上层**：`execute_auto` / `execute_manual`——Awakening 按 `control_mode` 分发
> - **中层**：`call_tool`——直接执行层，转发到底层 DAO
> - **底层**：`ToolCallDao.execute` + `decorate`——装饰器收敛，记录真实 ToolCallEntry trace
>
> **命名清理（2026-08-05）**：原 `ToolCallDao::call_manual` 重命名为 `ToolCallDao::execute`，消除"manual"语义重载（旧名误导：实际所有工具都走这里，不分 Auto/Manual）。同时删除 `ToolDal::call_manual` / `McpToolDal::call_manual` 纯转发方法，`call_tool` 直接调 DAO。详见 `docs/tool_design.md` "工具调用架构演进"。
>
> **同步 vs 异步 Manual**：`execute_manual` 根据 `tool.po.config.dispatch_mode` 选择 internal 工具——`sync`（默认）走 `request_tool_call` 在当前轮内通过 `call_tool` 同步执行并返回结果；`async` 走 `send_tool_call_message` 派发消息，结果在下一轮 awaken 送达。Agent 不直接调用这两个 internal 工具。

---

## 五、已确认的设计决策 ✅

以下设计决策已在实现中落地并验证。

### Q1：单回合 vs 多回合循环（已确认）

- **决策**：Runtime 内部只跑**单回合**，多回合靠"消息触发再次唤醒"
- **理由**：贴近"无复杂循环"原则；一次 awaken 内的 ChatMessage 多轮对话循环已支持多次工具调用，足够覆盖大多数场景
- **落地状态**：✅ 已实现（见第十八、十九章）

### Q2：一次 awaken 内的多步 tool calling 算不算"循环"（已确认）

- **决策**：算 Awakening 层的显式循环，**允许**，最多 `MAX_TOOL_ITERATIONS`（当前 10）次
- **理由**：自建 `OpenAiCompatibleCortexDao` 不再依赖 rig 内置循环，改为在 Awakening 层用 `Vec<ChatMessage>` 显式驱动 think → ToolCall? → execute → 追加 Tool 消息 → think 的循环；模型每轮看到完整对话历史（自己的 tool_calls 与 Tool 结果），自主判定是否输出 Final 终止
- **落地状态**：✅ 已实现（`BrainDal.think()` 接收 `&[ChatMessage]`，Awakening 显式循环）

### Q3：基础元能力放神经工具还是种子技能（已确认）

- **决策**：作为**神经工具**硬编码，每个 Agent 默认都有
- **理由**：神经工具是 Agent 的**天生能力**（就像人天生会说话、会回忆），不该用"技能"这层抽象套
- **落地状态**：⚠️ 部分实现（见 tool_design.md）

### Q4：ContextAssembly 放 Runtime 还是单独抽 pkg（已确认）

- **决策**：放 `runtime/context_assembly.rs`
- **理由**：上下文拼装本身就是 Runtime 的职责；如果未来别的 Domain 也要拼，再下沉到 pkg
- **落地状态**：✅ 已实现

### Q5：Trigger 类型枚举（已确认）

- **决策**：`UserMessage / AgentMessage / ToolResult / Scheduled / Manual` 五类
- **理由**：覆盖目前可预见的所有唤醒场景，每种 trigger 都能在 system_prompt 里加一段定制开场
- **落地状态**：✅ 已定义（见 awakening.rs）

### Q6：finished 信号怎么定（已确认）

- **决策**：约定一个特殊工具 `mark_done()`，模型显式调即结束
- **理由**：显式 > 隐式，便于追踪和审计；用户能清楚看到 Agent 自己判定"任务完成"的时点
- **落地状态**：⚠️ 部分实现（神经工具预留）

---

## 六、实现路线图 ✅（已完成）

按"小步推进、每步可编译可测"的原则，原定六步落地计划已全部执行完毕：

| 步骤 | 内容 | 产出 | 验收 | 状态 |
|------|------|------|------|------|
| **1** | 定义 `Awakening` trait + `AwakenCommand/Outcome` 数据结构 | `awakening.rs` 骨架、mod.rs 单例注册 | `cargo check` 通过 | ✅ 已完成 |
| **2** | 实现 `ContextAssembly` 纯函数：拼 system_prompt + recent_traces | `context_assembly.rs` + 单测 | 纯函数单测通过 | ✅ 已完成 |
| **3** | 实现 NeuralTools 最小集 | 通过 `#[register_handler_tool(neural)]` 宏注册，神经工具自动注入 Prompt | 编译通过 + 唤醒可见 | ✅ 已完成 |
| **4** | 接入 Cortex（模型推理）到 `Awakening` | 端到端最小可用 | 集成测试通过 | ✅ 已完成 |
| **5** | 加入消息通道和外骨骼通道 | 完整双通道 | Agent 可对话 + 调外骨骼工具 | ✅ 已完成 |
| **6** | 补齐展开式工具 + internal 工具机制 | 完整神经工具集 + `execute_auto`/`execute_manual` 三层调用 + internal 工具转发 | Agent 行为完整 | ✅ 已完成 |

**实际落地情况**：
- ✅ 核心架构全部落地：Runtime Memory + Context Assembly + Awakening
- ✅ Trace 闭环架构完成（第十八章）
- ✅ Agent 运行时状态管理完成（第十九章）
- ✅ 神经工具集 + internal 工具机制完成（神经工具通过 `neural` tag 自动注入；internal 工具不可绑定，由 `execute_manual` 转发）
- ✅ 自建 CortexDao（OpenAiCompatibleCortexDao）替代 rig，ChatMessage 多轮对话循环已落地
- ✅ 上下文压缩触发阈值机制落地（基于 `ModelProviderConfig` 的 `recommended_context_length` / `max_context_length`，详见 21.2.1b）
- ✅ 两层轮次限制落地：consumer 层 `max_thinking_depth` + awakening 层 `max_thinking_rounds`（详见 21.2.1）
- ✅ 总结退出流程落地：思考轮次耗尽后触发 `awaken_for_summary` 总结进展并通知消息源（详见 21.2.1a）
- ✅ 统一总结流程 + 强制记忆写入：正常 Final 完成也触发总结；pending_trace_ids 跟踪自上次压缩以来的 trace 列表；build_sleep_prompt/build_summary_prompt 强制要求 Agent 调用 save_short_term_memory 并填入 trace_ids（详见 25.12）
- ⚠️ 多 Agent 协作的高级编排仍在规划阶段

**不在本期范围**（留待后续设计）：
- 多 Agent 协作的高级编排（如团队、角色分工）
- Cortex 内部的模型选择策略、降级策略
- 神经工具的权限粒度控制（哪些 Agent 能用 `mark_done` 等）

---

## 七、附：与现有架构的对齐

### 7.1 复用现有能力

| 现有模块 | Runtime 如何复用 |
|----------|-----------------|
| `RuntimeMemory`（已实现） | 神经工具 `search_memory` / `write_memory` 直接委托 |
| `MessageDomain` | 神经工具 `send_message` 委托其投递能力 |
| `SkillDal` | 神经工具 `search_skill` / `read_skill` 委托 |
| `ToolDal` | 神经工具 `list_tools` / `read_tool_spec` 委托 |
| `Cortex`（模型推理） | `Awakening` 内部调用 |
| `tool_execution`（已有） | 神经工具 `request_tool_call` 委托 |

### 7.2 严格遵守的架构约束

- ✅ 单向调用：`handler → domain → dal → dao`，禁止跨层和同层互调
- ✅ DAO 单一职责：神经工具调 DAL，不直接调 DAO
- ✅ Context 传递：所有公共方法第一个参数是 `ctx: RequestContext`
- ✅ 测试内联：所有 `*_test.rs` 与对应源文件同目录
- ✅ 命名规范：trait 不带 `Trait` 后缀，实现类带 `Impl` 后缀

---

## 九、（已归档）唤醒流程早期设计参考

> 早期 12 步流程设计（草案 v0.2，2026-05-25）已被第十八章 18.7 的 7 步流程替代。原章节包含完整的 12 步执行流程、神经工具执行机制、ContextAssembly 拼装逻辑、AwakenOutcome 设计哲学等详细内容，已删除以精简文档。如需查阅历史代码示例请回溯 git 历史（`docs/runtime_design.md` v0.2 版本）。

> 📌 **章节编号缺口说明**：本文档保留原章节编号以兼容历史引用，存在以下缺口：
> - **第八章**：原计划内容已合并至第二、三、四章（核心接口与上下文拼装）
> - **第十章**：原计划内容已合并至第十一章（可执行落地方案）
> - **第十五章**：原变更记录已合并至第二十章（统一变更记录），避免重复维护

---

## 十一、可执行落地方案：第一阶段最小可用唤醒（已完成） ✅

> 📌 **目标**：基于现有能力，用最小改动实现第一版可跑通的 Agent 唤醒流程，先跑通纯文本对话，后续再叠加工具/技能能力。
>
> **完成状态**：第一阶段已于 2026-05-28 完成（见第十八章），所有核心功能均已实现。

### 11.1 当前能力盘点（✅ 已存在 / ❌ 需补充）

| 序号 | 能力 | 状态 | 对应 DAL/模块 | 现有方法 |
|------|------|------|-------------|---------|
| 1 | 获取用户信息 | ✅ | UserDal | `find_by_id` |
| 2 | 获取 Agent 实体 | ✅ | AgentDal | `find_by_id` |
| 3 | 读取最近短期记忆 | ⚠️ 部分 | MemoryDal | 有 `search`/`query`，需要封装 `get_recent_traces` 便捷方法 |
| 4 | 获取 Agent 绑定的工具 | ✅ | ToolDal | `list_tools_for_agent_full` |
| 5 | 获取 Agent 绑定的技能 | ✅ | SkillDal | `list_for_agent` |
| 6 | 调用模型推理 | ✅ | BrainDal | `prompt_existing_cortex` |
| 7 | 写入记忆 Trace | ⚠️ 部分 | RuntimeMemory | 有基础 `write`，需要 `write_thinking_trace` 专用方法 |
| 8 | 上下文拼装（Prompt 模板） | ❌ 新增 | ContextAssembly | 需要新建纯函数模块 |
| 9 | 唤醒入口方法 | ❌ 新增 | Awakening | 需要新建 `awaken_for_user_message` |

---

### 11.2 第一阶段唤醒流程（简化版 6 步）

**先跑通纯文本对话，工具/技能留到第二阶段。**

```
输入：ctx + agent_id + user_message_id

  1. 加载基础数据
     ├─ 加载 Agent 实体（AgentDal.find_by_id）
     ├─ 加载用户消息（MessageDal.find_by_id）
     └─ 如果有 user_id，加载用户信息（UserDal.find_by_id）
     ↓
  2. 读取最近短期记忆
     └─ RuntimeMemory.get_recent_traces(agent_id, limit=20)
     ↓
  3. 拼装 Prompt
     └─ ContextAssembly.build_conversation(agent, user, message, traces)
     ↓
  4. 记录输入 Trace
     └─ RuntimeMemory.write_thinking_trace(INPUT)
     ↓
  5. 调用模型推理
     └─ BrainDal.prompt_existing_cortex(agent.brain, prompt)
     ↓
  6. 记录输出 Trace + 返回结果
     └─ RuntimeMemory.write_thinking_trace(OUTPUT)
     └─ 返回 AwakeningResult
```

---

### 11.3 第一阶段需要新增/修改的代码清单

代码清单详见 `src/service/domain/runtime/awakening.rs`、`context_assembly.rs`、`memory.rs`、`mod.rs`。`awakening.rs` 现已包含 ChatMessage 多轮对话循环、按 `control_mode` 分发 `execute_auto`/`execute_manual` 的逻辑，以及 5 分钟 think 超时保护。原 v0.3 代码骨架（4 个模块的 trait/struct 定义示例）已删除以精简文档。

---

### 11.4 第一阶段唤醒的完整伪代码实现

伪代码实现详见 18.7 章节的 7 步流程（最终版）。原 v0.3 伪代码（7 步 `awaken_for_user_message` 完整实现）已被第十八章重构覆盖，此处删除以精简文档。

---

### 11.5 第一阶段开发顺序（4 步）

| 步骤 | 内容 | 预估工作量 | 验收标准 |
|------|------|-----------|---------|
| **1** | 给 `RuntimeMemory` 补充 `get_recent_traces` 和 `write_thinking_trace` 方法 | 30 min | 单测通过 + `cargo check` |
| **2** | 新增 `context_assembly.rs` 纯函数模块 | 20 min | 单测通过 + `cargo check` |
| **3** | 新增 `awakening.rs` 骨架 + 空实现 | 20 min | `cargo check` 通过 |
| **4** | 实现 `awaken_for_user_message` 完整逻辑 | 60 min | 集成测试跑通 |

**合计：约 2 小时**

---

### 11.6 第二阶段规划（叠加工具/技能能力）

第一阶段跑通后，再叠加：

1. **神经工具注入**：在 Step 3 拼装 Prompt 时，把 Agent 绑定的工具列表注入到 Prompt 中
2. **技能注入**：在 Step 3 拼装 Prompt 时，把 Agent 绑定的技能列表注入到 Prompt 中
3. **rig 多轮工具调用**：把 Step 5 的 `cortex.prompt()` 替换为 `cortex.prompt_with_tools()`，支持回合内工具调用
4. **收口信号处理**：检测模型是否调用了 `mark_done` 等收口工具

---

### 11.7 关键设计决策（已对齐）

| 决策点 | 方案 | 理由 |
|--------|------|------|
| **唤醒方法粒度** | 按触发场景拆方法（`awaken_for_user_message` / `awaken_for_agent_message`） | 不同触发场景的参数和逻辑差异大，分开更清晰 |
| **Prompt 模板位置** | 纯函数 `ContextAssembly` 模块 | 可单独测试、无副作用、未来容易扩展模板版本 |
| **Trace 存储** | 复用现有 Memory 表，用 `memory_type` 区分 | 不需要建新表，直接复用已有的四层记忆架构 |
| **第一阶段范围** | 先跑通纯文本，不做工具/技能 | 最小可用原则，先验证主链路通了再叠加复杂度 |
| **DAL 依赖方式** | Awakening 内部直接调用各 DAL 单例 | 符合项目现有风格（Domain 层可以直接依赖 DAL） |

---

## 十二、Runtime Memory 子模块行为对齐

> 📌 **核心原则**：Runtime Memory 是 MemoryDal 的最薄封装，所有核心能力直接复用 DAL，Domain 层只加便捷语法糖。

### 12.1 当前 Runtime Memory 能力确认

| 方法 | 参数 | 返回值 | 底层实现 | 说明 |
|------|------|--------|---------|------|
| `write` | `MemoryCreateParams` | `Result<Vec<Memory>>` | `MemoryDal.create()` | 写入记忆，按变体自动分发（Trace/ShortTerm/KnowledgeNode/Relation） |
| `search` | `MemorySearch` | `Result<Vec<Memory>>` | `MemoryDal.search()` | 混合搜索（关键词 + 向量 + 过滤） |
| `query` | `MemoryQuery` | `Result<Vec<Memory>>` | `MemoryDal.query()` | 普通查询（仅过滤，无向量） |

**✅ 设计正确，不需要改核心接口。** 我们只需要在这三个核心方法之上，补充便捷语法糖。

---

### 12.2 需要补充的便捷方法（语法糖）

这两个方法本质上是 `query` 和 `write` 的预填充参数版本，不涉及新的业务逻辑。

#### 12.2.1 `get_recent_traces` - 获取 Agent 最近的短期记忆

```rust
async fn get_recent_traces(
    &self,
    ctx: RequestContext,
    agent_id: &str,
    limit: i64,
) -> Result<Vec<Memory>, AppError> {
    // 内部直接调用 query，预填充参数
    self.query(ctx, MemoryQuery {
        agent_id: Some(agent_id.to_string()),
        memory_type: Some(MemoryType::ShortTerm),
        limit: Some(limit as usize),
        ..Default::default()
    }).await
}
```

**为什么是语法糖？**
- 不改变底层行为，只是把"查最近 N 条"这个常用模式封装起来
- 调用方不需要知道具体参数名，减少出错概率
- 后续如果"最近记忆"的定义变了（比如要过滤某种状态），只改这一处

#### 12.2.2 `write_thinking_trace` - 写入思考 Trace

```rust
async fn write_thinking_trace(
    &self,
    ctx: RequestContext,
    agent_id: &str,
    trace_type: ThinkingTraceType,
    content: &str,
) -> Result<Memory, AppError> {
    // 内部直接调用 write，预填充参数
    let trace = MemoryTrace {
        id: generate_trace_id(),
        agent_id: agent_id.to_string(),
        task_id: ctx.task_id(),
        log_id: ctx.log_id(),
        user_id: ctx.uid().unwrap_or_default(),
        organization_id: ctx.organization_id().unwrap_or_default(),
        role: match trace_type {
            ThinkingTraceType::Input => MemoryRole::System,
            ThinkingTraceType::Output => MemoryRole::Assistant,
            ThinkingTraceType::ToolCall => MemoryRole::Tool,
            ThinkingTraceType::ToolResult => MemoryRole::Tool,
        },
        content: content.to_string(),
        created_at: chrono::Utc::now().timestamp(),
        metadata: HashMap::new(),
        position: None,
    };

    let mut results = self.write(ctx, MemoryCreateParams::AppendTraces(vec![trace])).await?;
    results.pop().ok_or_else(|| AppError::Internal("Write trace failed".to_string()))
}
```

**为什么是语法糖？**
- 把"构造 MemoryTrace + 选择正确的 MemoryCreateParams 变体"这个细节封装起来
- 调用方只需要传业务参数，不需要关心 Memory 四层架构的内部细节
- trace_type 自动映射到 MemoryRole，统一标准

---

### 12.3 Runtime Memory 的分层定位

```
┌─────────────────────────────────────────────┐
│   Domain 层 RuntimeMemory                    │
│  ┌─────────────────────────────────────────┐  │
│  │ 便捷语法糖：                            │  │
│  │   get_recent_traces                     │  │
│  │   write_thinking_trace                  │  │
│  └──────────────┬──────────────────────────┘  │
│                 │ 直接调用                      │
└─────────────────▼─────────────────────────────┘
┌─────────────────────────────────────────────┐
│    DAL 层 MemoryDal                          │
│  ┌─────────────────────────────────────────┐  │
│  │ 核心业务逻辑：                          │  │
│  │   search() - 混合搜索                   │  │
│  │   query()  - 普通查询                   │  │
│  │   create() - 按变体分发写入              │  │
│  │   update() - 更新 + 重向量化            │  │
│  │   delete() - 级联删除                   │  │
│  └──────────────┬──────────────────────────┘  │
│                 │ 直接调用                      │
└─────────────────▼─────────────────────────────┘
┌─────────────────────────────────────────────┐
│    DAO 层 MemoryDao / MemoryVectorDao        │
│  ┌─────────────────────────────────────────┐  │
│  │ 纯 SQL：                                │  │
│  │   CRUD + 向量索引 + 全文搜索            │  │
│  └─────────────────────────────────────────┘  │
└─────────────────────────────────────────────┘
```

**✅ 定位正确，符合项目分层原则：**
- DAO 层：纯 SQL，无业务逻辑
- DAL 层：跨 DAO 编排、向量生成、结果聚合
- Domain 层：便捷语法糖、统一入口、业务语义封装

---

### 12.4 实现要点（不破坏现有架构）

1. **只加方法，不改现有方法签名**
   - 现有三个核心方法保持不变
   - 新增两个便捷方法

2. **`ThinkingTraceType` 枚举放在 Runtime Memory 模块**
   - 这是 Runtime 层的概念，不是 DAL 层的通用概念
   - 枚举值：`Input` / `Output` / `ToolCall` / `ToolResult`

3. **不引入新的参数结构体**
   - 继续 100% 复用 DAL 层的 `MemoryQuery` / `MemoryCreateParams`
   - 保持"零重复定义"的设计原则

---

## 十三、统一分页设计（2026-07-24 已落地实现）

> 📌 **当前状态**：已全面实现。query 是核心查询能力，list 是语法糖；两者统一返回 `PagedResult<T>`。详见 [AGENTS.md 4.9 查询接口分页规范](../AGENTS.md#49-查询接口分页规范强制执行2026-07-24-新增)。

### 13.1 最终方案

| 接口类型 | 职责 | HTTP 方法 | 参数位置 | 返回 |
|---------|------|----------|---------|------|
| **query（核心）** | 完整查询条件 + 分页 | POST body | `XxxQueryRequest { ...查询条件..., pagination }` | `PagedResult<T>` |
| **list（语法糖）** | 只接受分页，内部固定默认过滤和排序 | GET query param | `?limit=10&offset=0` | `PagedResult<T>` |

**核心设计**：
- `query` 接口承担所有复杂查询需求（ids 批量查询、status 过滤、keyword 搜索等），接受 `pagination: PaginationParams`
- `list` 接口是语法糖，只接受 `pagination`，不接受任何查询字段，内部固定默认过滤（如排除 Deleted）和默认排序
- 两者统一返回 `PagedResult<T> { items, total }`，前端处理结构一致

### 13.2 基础设施

```rust
// common/src/api/mod.rs
pub struct PaginationParams {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

pub struct PagedResult<T> {
    pub items: Vec<T>,
    pub total: usize,
}

impl<T> PagedResult<T> {
    pub fn map<U>(self, f: impl FnMut(T) -> U) -> PagedResult<U> { ... }
}
```

### 13.3 全链路实现

```
Handler ──► Domain ──► DAL ──► DAO
   XxxQueryRequest    XxxQuery    XxxQuery    SQL
   (含 pagination)    (含 pagination) (含 pagination)
   PagedResult<ListItem> ◄─ map(to_list_item) ◄─ PagedResult<Entity> ◄─ map(from_po) ◄─ PagedResult<Po> ◄─ COUNT + LIMIT/OFFSET
```

**已改造的 10 个实体**：Agent / Project / Task / Tool / Skill（DAO + Domain + Handler 全链路）+ McpServer / MessageChannel / Artifact / ModelProvider / User（仅 DAO query 通用化，Handler 按需适配）

**DAO 层实现模式**：抽取 `push_query_filters` 函数，COUNT 和 LIST 查询复用同一套 WHERE 条件，避免过滤条件不一致。参考实现：`src/service/dao/mcp_server/sqlite.rs`。

### 13.4 关键约束

- DAO 层 `query` 方法签名统一返回 `Result<PagedResult<Po>>`
- pagination 字段随 Query 结构体一起传递，不需要单独的方法参数
- 每层用 `PagedResult::map()` 转换内部类型，保留 total
- list handler 内部固定默认过滤（如 Agent 排除 Deleted，Skill 排除 Expired）
- SQLite 的 offset 单独使用时需 `LIMIT -1`

---

## 十四、（已归档）第一阶段唤醒最终确认

> v0.4 最终确认的 6 步流程（含 14.1 范围表、14.2 6 步执行流程、14.3 关键设计决策）已被第十八章 18.7 的 7 步流程替代。第一阶段已于 2026-05-28 完成（详见第十八章）。原内容已删除以精简文档。

---

## 十六、（已归档）简易版实现完成 v0.5

> 简易版实现于 2026-05-26 完成（v0.5），核心逻辑全部可编译：Runtime Memory + Context Assembly (Builder) + Awakening 主流程。详见 `src/service/domain/runtime/` 目录代码。后续被第十七、十八章重构覆盖，原模块清单表与设计决策清单已删除以精简文档。

---

## 十七、架构重构 + 角色分工完成 ✅

**日期：2026-05-26 | 版本：v0.6**

### 17.1 架构重构：正确的 Domain Trait 组织方式

**问题：** 之前的实现不符合项目其他 Domain 的组织方式，缺乏统一的总 trait。

**解决方案：** 对齐 Message Domain / Finance Domain 的标准模式：

```
mod.rs
├── RuntimeDomain: 总 trait（聚合所有子能力）
│   ├── fn memory(&self) -> &dyn RuntimeMemory
│   ├── fn awakening(&self) -> &dyn RuntimeAwakening
│   └── fn tool_execution(&self) -> &dyn RuntimeToolExecution
│
├── RuntimeMemory: 记忆管理子 trait
├── RuntimeAwakening: 唤醒能力子 trait
├── RuntimeToolExecution: 工具执行子 trait（Manual 工具授权、协议路由、强类型 trace_ref 结果引用）
│
├── RuntimeDomainImpl: 统一实现结构体
│   （所有子模块都为 RuntimeDomainImpl 实现对应的 trait）
│
└── domain() / init(): 单例访问函数
```

**各文件职责：**
| 文件 | 职责 |
|------|------|
| `mod.rs` | 总入口：所有 trait 定义 + 统一实现结构体 + 单例访问 |
| `memory.rs` | `impl RuntimeMemory for RuntimeDomainImpl` |
| `awakening.rs` | `impl RuntimeAwakening for RuntimeDomainImpl` |
| `tool_execution.rs` | `impl RuntimeToolExecution for RuntimeDomainImpl` |
| `context_assembly.rs` | 纯函数模块（无 trait，独立测试） |

### 17.2 关键设计决策

- **决策 1：消息链 ID 暴露给 Agent** — Runtime 不再自动读取消息链完整内容，只在 Prompt 中暴露 `reply_to_id`，Agent 自主决定是否回溯，降低 Runtime 复杂度。
- **决策 2：角色分工 - 只有客服 Agent 才拼接用户喜好** — 仅当 Agent 的 `roles` 字段包含 `customer_service` 或 `客服` 时才在 Prompt 中添加【用户画像】区块；用户画像数据由上层 Domain 传入（分层原则）。
- **决策 3：Prompt 结构调整** — 新增【用户画像】区块（仅客服 Agent 显示），位于【Agent 人设】与【历史对话】之间。

### 17.3 整体进度总览

| 模块 | 进度 | 核心能力 |
|------|------|---------|
| **架构组织** | ✅ 100% | 标准 Domain trait 分层，编译全过 |
| **Runtime Memory** | ✅ 100% | `get_recent_context()` + `write_thinking_trace()` |
| **Context Assembly** | ✅ 100% | Builder 模式，Trace IDs + Agent 人设 + **用户画像** + 历史 + 消息 + 技能/工具预留 |
| **Runtime Awakening** | ✅ 80% | 7 步主流程完整可跑：读记忆 → 拼 Prompt → 记输入 → 推理 → 记输出 → 返回；仅模型推理为模拟返回 |
| **Tool Execution** | ✅ 88% | Runtime Domain 已支持按协议路由 Builtin/HTTP/MCP；Manual 工具消息模式已完成 Agent 绑定授权、执行与 ToolCallResult 回调闭环；MCP synced tool stale/reconcile 已完成；ToolCallResult 已补不复制 request args、大结果 inline bound、强类型 `trace_ref = ToolCallTraceRef { tool_id, call_id }` 轻量引用、以及基于 call_id/tool_id 的 tool-specific call trace 查询 API；后续补 ToolCallResult 触发的外部唤醒调度、统计模块驱动轮次预算与更多 E2E 场景 |

**整体完成度：~86%**
**当前状态：纯文本对话流程完整可跑；工具执行已完成 MCP/Manual 最小闭环、MCP synced tool stale/reconcile 状态一致性，以及 ToolCallResult 第一层结果边界（不复制 request args、大结果安全 marker、强类型 trace_ref 关联完整 ToolCallEntry、基于 call_id/tool_id 查询完整 ToolCallEntry；执行前/策略失败不伪造 trace_ref）。后续重点转向 ToolCallResult 触发的外部唤醒轮次、统计模块驱动的轮次限制、产物化引用和更完整运行面策略。**

### 17.4 剩余待做（优先级排序）

| 优先级 | 任务 | 说明 |
|--------|------|------|
| **P1** | 统计模块驱动的外部唤醒轮次 | Agent 收到 ToolCallResult 后可被消息机制再次唤醒；是否继续唤醒、还能唤醒几轮、是否暂停等待用户反馈，统一由统计模块的 task / agent / conversation 运行数据决定；最终用户答复仍由 Agent 在 Awakening 显式循环中调用 `send_message` 等工具发出 |
| **P1** | Trace ID 关联逻辑 | 从 `message.reply_to_id` 追溯历史 Trace 链 |
| **P2** | 工具调用框架增强 | 在已完成 Manual ToolCallRequest → Runtime → ToolCallResult 最小闭环和 ToolCallEntry 查询基础上，补外部唤醒调度与更多 E2E 场景 |
| **P3** | 技能注入 | 根据 Agent 角色动态注入技能说明 |
| **P3** | 单元测试覆盖 | 各模块测试用例 |

---

## 十八、记忆 Trace 闭环架构 + PO 自格式化重构完成 ✅

**日期：2026-05-28 | 版本：v0.7**

### 18.1 本次重构概览（6 个核心变更）

本次重构围绕「Trace 闭环」和「职责单一」两大原则，完成了 6 个核心变更，整体进度从 75% 推进到 ~90%。

| Commit | 变更内容 | 核心理念 |
|--------|---------|---------|
| `01aeb2a` | **Prompt 格式化逻辑内聚到各 PO 实体** | 谁的数据谁负责格式化，符合单一职责 |
| `5d6c6ba` | **BrainDal 统一思考入口 + 打通唤醒链路** | 所有 LLM 调用必经 BrainDal，统一审计/限流/Token 统计 |
| `2ea2dec` | **记忆 Trace 闭环架构改造** | 输入/输出共用同一 Trace ID，形成完整闭环 |
| `76dd26e` | **简化记忆 Trace 写入流程，避免二次 IO** | 先拿 ID 注入 Prompt，模型思考完成后一次性写入完整记录 |
| `581eb6d` | **统一 Runtime 域内 Trace 写入路径** | 所有 Trace 写入收敛到一处，便于后续扩展 |
| `3fd39de` | **RuntimeMemory.write_thinking_trace 直接接收 MemoryTrace 结构体** | 类型安全，参数爆炸零容忍 |

---

### 18.2 核心架构变更 1：PO 实体自格式化

**❌ 旧问题：** Prompt 拼装逻辑散落在 ContextAssembly 中，每个 PO 的字段变化都要改拼装代码，容易遗漏。

**✅ 新方案：PO 实体自己负责自己的 Prompt 格式化**

```rust
// 每个 PO 都实现 to_xxx_prompt() 方法
impl AgentPo {
    pub fn to_identity_prompt(&self) -> String {
        format!("你是 {}，ID：{}\n{}",
            self.name,
            self.id,
            self.description.as_deref().unwrap_or_default()
        )
    }
}

impl MessagePo {
    pub fn to_conversation_line(&self) -> String {
        let role = match self.from_agent_id {
            Some(_) => "Agent",
            None => "用户",
        };
        format!("{}：{}", role, self.content)
    }
}

impl MemoryPo {
    pub fn to_history_line(&self) -> String {
        format!("[{}] {}", self.memory_type, self.content)
    }
}
```

**设计优势：**
1. **单一职责**：PO 最了解自己的字段含义，格式化逻辑内聚
2. **零遗漏**：新增字段时，修改 PO 的格式化方法即可，不会漏改拼装器
3. **可测试**：每个 `to_xxx_prompt()` 都是纯函数，可单独写单测
4. **可复用**：不同场景的 Prompt 拼装都可以复用这些方法

---

### 18.3 核心架构变更 2：BrainDal 统一思考入口

**❌ 旧问题：** LLM 调用散落在各处（Awakening 直接调 Cortex，工具执行也直接调），无法统一审计和限流。

**✅ 新方案：所有 LLM 调用必经 BrainDal**

```rust
// 语义化调用链：唤醒大脑 → 思考 → 返回结果
let result = brain_dal()
    .wake_brain(ctx, &agent.po.brain_id)  // 第一步：唤醒大脑
    .await?
    .think(ctx, prompt)                   // 第二步：思考（调用 LLM）
    .await?;
```

**设计优势：**
1. **统一入口**：所有 LLM 调用都经过 BrainDal，便于审计、限流、Token 统计
2. **语义化命名**：`wake_brain` → `think` 语义清晰，符合业务直觉
3. **可扩展**：未来加缓存、降级、重试，只需要改 BrainDal 一处
4. **分层清晰**：Runtime Domain 不直接依赖底层的 Cortex，通过 BrainDal 间接访问

---

### 18.4 核心架构变更 3：Trace 闭环架构

**❌ 旧方案：** 先写 Input Trace → 拿到 ID → 注入 Prompt → 模型思考 → 写 Output Trace（用新的 ID）。问题：输入输出是两条独立记录，关联关系弱。

**✅ 新方案：输入输出共用同一 Trace ID，形成完整闭环**

```
1. 生成 trace_id（还没写库）
   ↓
2. 把 trace_id 注入到 Prompt 中（Agent 可以看到并引用）
   ↓
3. 模型思考，产生输出
   ↓
4. 一次性写入完整的 Trace 记录：
   - id = 第 1 步生成的 trace_id
   - input = 第 2 步的完整 Prompt
   - output = 第 3 步的模型输出
   - tool_calls = 本次调用的工具列表
```

**设计优势：**
1. **真正的闭环**：输入输出同 ID，查询时一次拿到完整上下文
2. **减少一次 IO**：原来写两次，现在只写一次
3. **Agent 可见**：Trace ID 注入到 Prompt，Agent 可以在回复中引用这个 ID
4. **便于追溯**：任何问题都可以通过 trace_id 查到完整的输入输出

---

### 18.5 核心架构变更 4：避免二次 IO 的写入流程

**❌ 旧流程（两次 IO）：**
```
写 Input Trace → 拿 ID → 注入 Prompt → 模型思考 → 写 Output Trace
（2 次数据库写入）
```

**✅ 新流程（一次 IO）：**
```
生成 trace_id（内存操作，不写库）
    ↓
注入 Prompt
    ↓
模型思考
    ↓
一次性写入完整记录（包含 input + output + metadata）
（1 次数据库写入）
```

**代码实现：**
```rust
// Step 1: 先生成 trace_id（内存操作，不写库）
let trace_id = format!("trace-{}-{}", agent_id, Utc::now().timestamp_nanos());

// Step 2: 注入到 Prompt
let prompt = builder
    .trace_id(&trace_id)  // Agent 能看到这个 ID
    .build();

// Step 3: 模型思考
let output = brain.think(ctx, &prompt).await?;

// Step 4: 一次性写入完整记录
runtime_memory()
    .write_thinking_trace(ctx, MemoryTrace {
        id: trace_id,      // 复用第一步生成的 ID
        input: prompt,
        output: Some(output),
        // ... 其他字段
    })
    .await?;
```

**性能收益：** 减少 50% 的记忆写入 IO。

---

### 18.6 核心架构变更 5：统一 Trace 写入路径

**❌ 旧问题：** Trace 写入散落在 Awakening、工具执行、消息处理等多处，每处都有自己的写入逻辑。

**✅ 新方案：所有 Trace 写入收敛到 RuntimeMemory.write_thinking_trace()**

```rust
// 统一入口，所有场景都走这个方法
async fn write_thinking_trace(
    &self,
    ctx: RequestContext,
    trace: MemoryTrace,  // 直接接收完整结构体
) -> Result<(), AppError>;
```

**设计优势：**
1. **单点扩展**：未来加 Trace 上报、Trace 采样、Trace 清洗，只改这一处
2. **类型安全**：接收完整结构体，避免参数爆炸
3. **一致行为**：所有 Trace 写入都经过同样的校验和处理逻辑

---

### 18.7 重构后的完整唤醒流程（最终版 7 步）

```
输入: ctx + agent_id + user_message_id

  1. 加载基础数据
     ├─ AgentDal.find_by_id() → Agent 实体
     ├─ MessageDal.find_by_id() → 消息实体
     └─ if user_id 存在 → UserDal.find_by_id() → 用户信息
     ↓
  2. 读取最近记忆
     └─ RuntimeMemory.get_recent_traces(agent_id, 20) → Vec<Memory>
     ↓
  3. 生成 Trace ID（内存操作）
     └─ trace_id = generate()
     ↓
  4. 拼装 Prompt（PO 自格式化）
     ├─ AgentPo.to_identity_prompt()
     ├─ 每条 MemoryPo.to_history_line()
     └─ MessagePo.to_conversation_line()
     ↓
  5. 统一入口调用 LLM
     └─ BrainDal.wake_brain() → think() → 输出
     ↓
  6. 一次性写入完整 Trace（输入 + 输出同 ID）
     └─ RuntimeMemory.write_thinking_trace(MemoryTrace { id, input, output, ... })
     ↓
  7. 返回 AwakeningResult
```

---

### 18.8 本次重构后整体进度

| 模块 | 进度 | 核心能力 |
|------|------|---------|
| **PO 自格式化** | ✅ 100% | Agent/Message/Memory 都实现了各自的 to_xxx_prompt() 方法 |
| **BrainDal 统一入口** | ✅ 100% | wake_brain() → think() 语义化调用链，所有 LLM 必经此处 |
| **Runtime Memory** | ✅ 100% | write_thinking_trace() 直接接收结构体，一次 IO 写入完整 Trace |
| **Trace 闭环架构** | ✅ 100% | 输入输出同 ID，注入 Prompt 供 Agent 引用，完整可追溯 |
| **Context Assembly** | ✅ 100% | Builder 模式，复用 PO 的自格式化方法 |
| **Runtime Awakening** | ✅ 95% | 7 步主流程完整可跑，仅剩边缘场景处理 |
| **Tool Execution** | ✅ 88% | Runtime Domain 协议路由、MCP 调用、Manual 授权与 ToolCallResult 回调闭环已完成；MCP synced tool stale/reconcile 状态一致性已完成；ToolCallResult 已补不复制 request args、大结果 inline bound、基于 call_id/tool_id 的 tool-specific call trace 查询 API，并已强类型携带 `trace_ref = ToolCallTraceRef { tool_id, call_id }`；成功和已开始执行后失败可携带真实引用，执行前/策略失败不伪造；后续补统计模块驱动的外部唤醒轮次、产物化引用和完整 E2E |

**整体完成度：~93%**
**当前状态：核心架构全部落地，Trace 闭环打通；纯文本对话流程生产就绪，MCP/Manual 工具调用闭环已补 ToolCallResult 第一层结果边界与基于 call_id/tool_id 的 ToolCallEntry 查询能力，并完成 synced tool stale/reconcile 状态一致性。**

---

### 18.9 剩余待做（重构后更新）

| 优先级 | 任务 | 说明 |
|--------|------|------|
| ✅ 已完成 | ToolCallResult trace_ref 协议字段 | 已在结果协议中强类型写入 `trace_ref = ToolCallTraceRef { tool_id, call_id }`，wire JSON 保持 `{ tool_id, call_id }`；不暴露 JSONL date/line/path；成功和已开始执行后的失败可携带真实引用，执行前/策略失败不伪造 |
| **P1** | 统计模块驱动的外部唤醒轮次 | Agent 收到 ToolCallResult 后可被消息机制再次唤醒；轮次限制、暂停/继续、页面可见的执行进度统一来自统计模块；必要时通过 trace_ref/call_id 查询完整 ToolCallEntry；最终用户答复由 Agent 自己调用 `send_message` 等工具发出 |
| **P1** | ToolCallResult 产物化引用策略 | 仅当结果需要用户下载或成为 Project Artifact 时接入 attachment / artifact，不作为普通工具审计详情的默认存储 |
| **P1** | Trace ID 关联链 | 从 `message.reply_to_id` 追溯历史 Trace 链，构建完整对话树 |
| **P2** | 技能动态注入 | 根据 Agent 角色和当前场景，动态注入技能说明 |
| **P2** | 单元测试覆盖 | 各模块测试用例，重点覆盖 PO 格式化、Trace 写入和工具消息闭环 |
| **P3** | 神经工具集 | 实现 search_memory / send_message / mark_done 等内置工具 |

---

## 十九、Agent 运行时状态管理

> 📌 **设计状态**：已实现（2026-07-08）
>
> **核心思想**：用纯内存状态机管理 Agent 的实时运行状态，区分空闲/休息/忙碌三种状态，确保同一 Agent 不会并发处理多条消息。

### 19.1 状态定义

| 状态 | 值 | 含义 | 是否可接收消息 |
|------|-----|------|--------------|
| **Idle** | 0 | 空闲，可接受新消息 | ✅ 是 |
| **Resting** | 1 | 休息中（恢复精力、压缩上下文、构建知识突触） | ❌ 否 |
| **Busy** | 2 | 忙碌，正在处理消息 | ❌ 否 |

**枚举定义**（前后端共享，位于 `common/src/enums/agent.rs`）：

```rust
#[repr(i32)]
pub enum AgentRuntimeState {
    #[default]
    Idle = 0,
    Resting = 1,
    Busy = 2,
}
```

### 19.2 状态生命周期

```
消息入队 → 消费者取出消息 → 检查状态
    │                          │
    │                          ├── Idle → 设置 Busy → 执行 awaken() → 设置 Idle
    │                          │
    │                          └── Busy/Resting → Nack → 消息重新入队等待
    │
    ↓
消息持久化（无论 Agent 状态如何）
```

**关键规则：**
- **入队时不做拦截**：消息始终入队并持久化，保证业务可追溯
- **消费时校验**：消费者检查状态，不可用时返回错误触发 Nack，消息重新入队
- **生命周期自动管理**：`awaken()` 开始时设置 Busy，结束（成功/失败）时设置 Idle，RAII 风格

### 19.3 状态管理器架构

```
┌─────────────────────────────────────────────────────────┐
│                    AgentRuntimeStateManager             │
│  ┌─────────────────────────────────────────────────────┐ │
│  │  DashMap<String, AgentRuntimeInfo>                  │ │
│  │  ┌─────────────────────────────────────────────────┐│ │
│  │  │ agent_id: String                                ││ │
│  │  │ state: AgentRuntimeState (Idle/Resting/Busy)    ││ │
│  │  │ current_message_id: Option<String>              ││ │
│  │  │ last_active_at: u64                             ││ │
│  │  └─────────────────────────────────────────────────┘│ │
│  └─────────────────────────────────────────────────────┘ │
│                           │                              │
│         ┌─────────────────┼─────────────────┐            │
│         ▼                 ▼                 ▼            │
│    set_state()      is_unavailable()    get_info()       │
│         │                 │                 │            │
└─────────┼─────────────────┼─────────────────┼────────────┘
          ▼                 ▼                 ▼
┌──────────────────┐ ┌──────────────────┐ ┌──────────────────┐
│   Awakening      │ │  Message Consumer│ │    AgentDal      │
│  (设置 Busy/Idle)│ │ (校验状态 Nack)   │ │ (注入到实体)     │
└──────────────────┘ └──────────────────┘ └──────────────────┘
```

### 19.4 分层注入机制

遵循分层架构原则，状态信息通过 DAL 层注入到 Agent 实体：

| 层级 | 职责 | 实现方式 |
|------|------|---------|
| **状态管理器** | 维护纯内存状态 | `AgentRuntimeStateManager::global()` 单例 |
| **DAL 层** | 查询状态并注入实体 | `AgentDal.find_by_id()` / `query()` 中调用状态管理器 |
| **Domain 层** | 业务逻辑中使用状态 | 从 Agent 实体的 `runtime_info` 字段读取 |
| **Handler 层** | 构建响应 DTO | 从 Agent 实体的 `runtime_info` 字段读取 |

**核心代码位置：**
- `src/pkg/agent_runtime_state.rs` - 状态管理器实现
- `src/service/dal/agent.rs` - DAL 层注入逻辑
- `src/service/domain/runtime/awakening.rs` - 唤醒时状态切换

### 19.5 与消息消费的协作

**当前行为：**
1. 用户发消息 → `send_to_agent()` → 直接入队并持久化（不检查状态）
2. 消费者取出消息 → `handle_agent_message()` → 检查 Agent 状态
3. 如果 Agent 忙碌/休息 → 返回 `Conflict` 错误 → 触发 Nack → 消息重新入队
4. 如果 Agent 空闲 → 正常处理

**后续优化方向（已记录，暂不实现）：**

| 优化项 | 描述 | 优先级 |
|--------|------|--------|
| **特殊消息强制中断** | 高优先级消息可以打断当前执行 | 中 |
| **消息优先级** | 在 `MessagePo` 中增加 `priority` 字段 | 中 |
| **延迟入队** | 不可用时延迟 N 秒后重新入队，避免 busy loop | 低 |
| **重试上限** | 设置最大重试次数，超过后转入死信队列 | 低 |

### 19.6 代码清单

| 文件 | 类型 | 改动内容 |
|------|------|---------|
| `common/src/enums/agent.rs` | 新增 | `AgentRuntimeState` 枚举定义 |
| `common/src/api/agent.rs` | 修改 | DTO 新增 `runtime_state` / `current_message_id` 字段 |
| `src/pkg/agent_runtime_state.rs` | 新增 | `AgentRuntimeStateManager` + `AgentRuntimeInfo` |
| `src/models/agent.rs` | 修改 | `Agent` 实体新增 `runtime_info` 字段 |
| `src/service/dal/agent.rs` | 修改 | `find_by_id` / `query` 注入运行时状态 |
| `src/service/domain/runtime/awakening.rs` | 修改 | awaken 生命周期状态切换 |
| `src/service/domain/runtime/mod.rs` | 修改 | `RuntimeDomain` trait 新增状态查询方法 |
| `src/consumer/message.rs` | 修改 | 消费时检查 Agent 状态 |
| `src/handlers/hr/agent/*.rs` | 修改 | 从实体读取运行时状态 |

---

## 二十、变更记录

| 日期 | 版本 | 变更 |
|------|------|------|
| 2026-08-05 | v3.7 | 统一总结流程 + 强制记忆写入（详见 25.12）：正常 Final 完成也触发 awaken_for_summary；pending_trace_ids 跟踪自上次压缩以来的 trace 列表；build_sleep_prompt/build_summary_prompt 新增 trace_ids 参数 + 强制 save_short_term_memory 指令；SaveShortTermMemoryParams 新增 trace_ids 字段 |
| 2026-07-31 | v3.6 | 新增第二十五章：唤醒/沉睡场景化设计（ThinkingScene/ThinkingOptions + sleep_and_settle + 工具双层过滤 + 业务上下文注入），812 测试通过 |
| 2026-07-24 | v3.5 | 记忆 tags 全链路支持 + 知识图谱节点可视化增强，746 测试通过（详见第二十四章） |
| 2026-07-23 | v3.4 | Runtime 执行链路全面修复（16 项），覆盖正确性、用户体验、性能与安全，745 测试通过（详见第二十三章） |
| 2026-07-12 | v3.2 | Phase 4C 技能系统增强 + Phase 5 多 Agent 协作工具，693 测试通过 |
| 2026-07-11 | v3.0 | Phase 4A 工具包机制 + 任务执行闭环，tag 分组、免绑定三层校验、TaskAssignment 消息、569 测试通过（详见第二十二章） |
| 2026-07-23 | v2.2 | `request_tool_call` 重新注册为同步神经工具，与异步 `send_tool_call_message` 对齐；参数对齐（加 tool_name/project_id），响应加 result 字段；Manual 工具区块提示词更新说明两种调用方式 |
| 2026-07-11 | v2.1 | 架构修正：同步/异步工具调用分离；`request_tool_call` 从神经工具移除，改为普通 HTTP Handler；新增 `send_tool_call_message` 神经工具作为 manual 工具异步调用入口；新增三种角色定位说明 |
| 2026-07-10 | v2.0 | 新增第二十一章：多回合循环控制设计；轮次限制、任务完成检测、Prompt 上下文差异化、工具失败计数注入、唤醒失败事件记录 |
| 2026-07-10 | v1.0 | Phase 2 神经工具集完整落地：8 个神经工具全部实现，自动回复移除，548 测试通过 |
| 2026-07-08 | v0.8 | 新增第十九章：Agent 运行时状态管理完整设计；纯内存状态机、DashMap 并发安全、消费时校验、分层注入机制 |
| 2026-05-28 | v0.7 | 新增第十八章：记忆 Trace 闭环架构 + PO 自格式化重构完成；BrainDal 统一思考入口；简化 Trace 写入避免二次 IO；统一 Runtime 域内 Trace 写入路径；整体进度 ~93% |
| 2026-05-26 | v0.6 | 新增第十七章：架构重构 + 角色分工完成；对齐标准 Domain trait 组织方式；新增用户画像功能（客服专用）；消息链 ID 暴露给 Agent 自主读取；整体进度 ~86% |
| 2026-05-25 | v0.5 | 新增第十六章：简易版实现完成；核心逻辑全部可编译：Runtime Memory + Context Assembly (Builder) + Awakening 主流程 |
| 2026-05-25 | v0.4 | 新增第十二章：Runtime Memory 子模块行为对齐 + 第十三章：统一分页设计（独立需求） + 第十四章：第一阶段最终确认 |
| 2026-05-25 | v0.3 | 新增第十一章：可执行落地方案（第一阶段最小可用唤醒），包含能力盘点、6 步流程、4 个模块修改清单、开发顺序 |
| 2026-05-25 | v0.2 | 新增第九章：唤醒 Agent 操作的具体执行流程（12 步）、神经工具执行机制、ContextAssembly 拼装逻辑、AwakenOutcome 设计哲学 |
| 2026-05-25 | v0.1 | 初版草案，覆盖设计总纲与待拍板点 |

---

## 二十一、多回合循环控制设计（Phase 3）

> 📌 **本章定位**：Runtime Domain Phase 3 的完整设计文档，涵盖多回合循环的控制机制、统计数据注入、Prompt 差异化等核心能力。
>
> **完成状态**：✅ 已全部实现（2026-07-10），554 个测试 100% 通过

### 21.1 设计目标

Phase 3 的核心目标是让 Agent 在多回合对话中能够：
1. **自主控制循环**：通过 `mark_done` 神经工具显式标记任务完成
2. **遵守轮次限制**：不陷入无限思考循环
3. **感知任务状态**：任务完成后自动停止唤醒
4. **差异化理解上下文**：根据消息类型调整行为
5. **规避失败工具**：对高失败率工具保持谨慎

### 21.2 核心机制

#### 21.2.1 轮次限制检查（两层机制）

系统采用**两层轮次限制**，分别在不同层级保护 Agent 不陷入无限循环：

**第一层：consumer 层跨唤醒累计工具调用数（`max_thinking_depth`）**

**设计原则**：在消费者层实现，利用统计模块查询当前任务的累计工具调用次数，与 Agent 配置的 `max_thinking_depth`（默认 10）对比。防止 Agent 在无限消息循环中空转。

**执行流程**：
```
消息消费者收到 Agent 消息
    │
    ├── 查询 Agent（含统计信息）
    │       └── AgentFetchOptions { with_stats: true, stats_task_id: Some(task_id) }
    │
    ├── 检查跨唤醒累计工具调用数
    │       └── 如果 call_summary.total_calls >= max_thinking_depth
    │               └── 发送提示消息，终止唤醒
    │
    └── 正常唤醒 Agent
```

**关键代码位置**：`src/consumer/message.rs` → `handle_agent_message()`

**设计要点**：
- 统计数据通过 `AgentFetchOptions` 参数按需加载，避免每次查询都获取统计信息
- 使用 `agent.po.get_runtime_config()` 解析运行时配置（JSON 格式）
- 超限后发送系统提示消息，告知用户"思考深度已达上限"

**第二层：awakening 层单次唤醒 think loop 轮次上限（`max_thinking_rounds`）**

**设计原则**：在 awakening 层 think loop 内部实现，统计单次唤醒内的思考轮次（跨多次上下文压缩累计），与 Agent 配置的 `max_thinking_rounds`（默认 90）对比。防止长任务在单次唤醒中无限思考。

**与第一层的区别**：
- 第一层统计**跨消息**累计工具调用数，保护消息循环层
- 第二层统计**单次唤醒内**思考轮次（包含上下文压缩后的跨压缩累计），保护 think loop 层
- 长任务可能需要多次上下文压缩，压缩次数本身不限制，只限制总思考轮次

**执行流程**：
```
awaken() 调用 run_think_loop()
    │
    ├── 累计轮次 total_rounds（跨压缩累计）
    │
    ├── 每轮 think 后检查
    │       └── 如果 total_rounds >= max_thinking_rounds
    │               └── 返回 ThinkLoopResult::MaxRoundsExceeded
    │
    └── 调用方（awaken）收到 MaxRoundsExceeded
            └── 进入总结退出流程（awaken_for_summary）
```

**关键代码位置**：`src/service/domain/runtime/awakening.rs` → `run_think_loop()` + `awaken_for_summary()`

**设计要点**：
- 轮次计数跨上下文压缩累计：每次 `ContextOverflow` 后压缩并重试，`total_rounds += rounds_used`
- 默认值 90 轮可通过 `AgentRuntimeConfig.max_thinking_rounds` 配置
- 未来任务可在分配时预估写入，覆盖默认值

#### 21.2.1a 总结退出流程（MaxRoundsExceeded + 正常 Final 完成的统一总结）

**设计原则**：思考轮次耗尽后不是简单报错，而是通过再一次调用 Agent 思考完成总结，让 Agent 总结当前工作进展、记录问题，并通知消息源或记录到 task 信息中作为进度总结。

**统一总结流程（方案 B）**：正常 Final 完成（用户拿到回答）和 MaxRoundsExceeded（轮次耗尽）共用 `awaken_for_summary`。两者都需要把"自上次压缩以来"的工作对话总结为短期记忆，避免 Agent 忘记自发写入。区别仅在 MaxRoundsExceeded 多了"发送通知给消息源"的能力。

**执行流程**：
```
awaken() 收到 ThinkLoopResult::MaxRoundsExceeded { messages, total_rounds }
    │   或 ThinkLoopResult::Final { content, messages }（正常完成时也触发）
    │
    ├── 构造 summary_trace_ids = pending_trace_ids（+ awaken trace_id 兜底）
    │   └── pending_trace_ids 跟踪自上次压缩以来产生的 trace 列表
    │
    └── 调用 awaken_for_summary(trace_ids=&summary_trace_ids)
        │
        ├── 构造总结 Prompt（build_summary_prompt(work_summary, total_rounds, trace_ids)）
        │   ├── 包含工作对话摘要 + 总结任务说明
        │   └── 强制写入短期记忆指令：要求 Agent 调用 save_short_term_memory
        │       └── trace_ids 字段必须填入本次总结依赖的 trace 列表
        │
        ├── 过滤工具：允许 neural / memory / messaging / project_management tag
        │   └── 让 Agent 能调用 send_message / update_task_progress / save_short_term_memory 等工具
        └── 调用 run_think_loop() 完成总结（最多 10 轮）
```

**关键代码位置**：
- `src/service/domain/runtime/awakening.rs` → `awaken_for_summary()` + awaken 循环中的 `pending_trace_ids` 维护
- `src/models/prompt_builder.rs` → `build_summary_prompt(work_summary, total_rounds, trace_ids)` trait 方法
- `src/service/dal/agent.rs` → `DefaultPromptBuilder::build_summary_prompt()` 实现

**设计要点**：
- 总结场景使用 `ThinkingScene::Summary`，允许的消息和任务管理工具 tag：`neural` / `memory` / `messaging` / `project_management`
- 总结 Prompt 包含工作对话摘要（`messages_to_summary` 提取关键信息），避免完整历史 token 膨胀
- Agent 在总结中可调用 `send_message` 通知用户/Project Owner，或调用 `update_task_progress` 更新任务进度
- **强制写入短期记忆**：Prompt 中明确要求 Agent **必须**调用 `save_short_term_memory` 将本次工作总结写入短期记忆，并填入 `trace_ids` 字段（来自 prompt 模板）保证记忆可追溯
- 正常 Final 完成时的总结失败不阻塞业务返回（awaken 已成功），仅记录警告
- 总结完成后正常写入 MemoryTrace，保留思考闭环记录

**pending_trace_ids 维护规则**：
- 初始化：`[awaken_trace_id]`（awaken 流程预生成的 trace_id）
- 每次 `sleep_and_settle` 完成后重置：`[settle_trace_id]`（下次总结范围 = 自上次压缩以来）
- MaxRoundsExceeded 触发总结时：`pending_trace_ids + [awaken_trace_id]`（兜底去重）
- 正常 Final 完成触发总结时：直接使用 `pending_trace_ids`（已含 awaken_trace_id）

#### 21.2.1b 上下文压缩触发阈值

**设计原则**：基于 ModelProvider 配置的上下文长度，自动检测 think loop 中的上下文溢出并触发压缩（sleep_and_settle 沉淀后重试）。

**阈值优先级**：
```
recommended_context_length（推荐上下文长度，用户配置）
    ↓ 未设置时
max_context_length * 60%（自动计算）
    ↓ 未设置时
不检测（无上下文压缩）
```

**执行流程**：
```
run_think_loop() 每轮 think 后
    │
    ├── 检查 input_tokens 是否超过阈值
    │       └── 超过 → 返回 ThinkLoopResult::ContextOverflow { messages, input_tokens, rounds_used }
    │
    └── 调用方（awaken）收到 ContextOverflow
            ├── total_rounds += rounds_used
            ├── 调用 sleep_and_settle(pending_trace_ids) 沉淀记忆
            │   └── sleep_and_settle 内部强制写入沉淀摘要到短期记忆（含 trace_ids）
            ├── 压缩完成后重置 pending_trace_ids = [settle_trace_id]
            │   └── 下次总结范围 = 自上次压缩以来
            └── 重新构造 prompt 调用 run_think_loop（携带累计轮次）
```

**关键代码位置**：`src/service/domain/runtime/awakening.rs` → `run_think_loop()` 中的 `overflow_threshold` 计算

**配置来源**：
- `ModelProviderConfig.max_context_length` — 模型支持的最大 token 数
- `ModelProviderConfig.recommended_context_length` — 推荐的工作上下文上限（优先作为压缩触发阈值）
- 两个字段均可通过前端表单配置（创建/编辑模型提供商时填写）

#### 21.2.2 任务完成状态检测

**设计原则**：唤醒前检查关联任务的状态，已完成/已取消/已归档的任务不再唤醒 Agent。

**执行流程**：
```
消息消费者收到 Agent 消息
    │
    ├── 检查任务状态（如果消息关联了 task_id）
    │       └── ProjectDomain.task_manage().get(ctx, task_id)
    │               └── 如果任务状态 == Completed/Cancelled/Archived
    │                       └── 记录日志，跳过唤醒
    │
    └── 正常唤醒 Agent
```

**关键代码位置**：`src/consumer/message.rs` → `handle_agent_message()`

**设计要点**：
- 通过 `ProjectDomain` 查询任务状态，保持分层架构
- 三种终止状态：`Completed`（已完成）、`Cancelled`（已取消）、`Archived`（已归档）

#### 21.2.3 Prompt 上下文差异化

**设计原则**：不同类型的消息在 Prompt 中使用不同的标签，让 Agent 清楚理解当前触发的原因。

**消息类型与标签对应关系**：

| 消息类型 | Prompt 标签 | 说明 |
|----------|------------|------|
| `Text`（用户文本） | 【当前消息】 | 常规用户输入 |
| `ToolCallResult`（工具结果） | 【工具执行结果】 | 工具调用返回的结果 |
| `ToolCallRequest`（工具请求） | 【工具调用请求】 | Agent 发起的工具调用 |
| `ConfirmRequest`（确认请求） | 【确认请求】 | 需要用户确认的操作 |
| `ConfirmResponse`（确认回复） | 【确认回复】 | 用户的确认结果 |
| `Image/File/Audio/Video` | 【当前消息】 | 媒体类消息 |

**关键代码位置**：
- `src/models/message.rs` → `MessagePo::to_prompt()` - 消息内容格式化
- `src/service/domain/runtime/context_assembly.rs` → `PromptBuilder::current_message()` - 标签选择

**设计要点**：
- `to_prompt()` 方法根据消息类型调整内容标签（【消息内容】/【执行结果】/【调用详情】）
- `PromptBuilder` 根据消息类型选择外层标签

#### 21.2.4 工具失败计数注入

**设计原则**：在 Prompt 中注入工具失败统计，提醒 Agent 谨慎使用高失败率工具。

**执行流程**：
```
PromptBuilder.build()
    │
    ├── 工具说明（【可用 Manual 工具】）
    │
    ├── 工具失败警告（【工具失败警告】）← 有失败时才显示
    │       └── 列出失败次数 > 0 的工具
    │
    └── 当前消息
```

**关键代码位置**：`src/service/domain/runtime/context_assembly.rs` → `PromptBuilder`

**设计要点**：
- `tool_failures` 字段存储 `(工具名称, 失败次数)` 元组列表
- 只有存在失败工具时才显示警告区块
- 警告格式："以下工具近期失败次数较多，请谨慎使用或考虑替代方案"

#### 21.2.5 唤醒失败事件记录

**设计原则**：唤醒失败时也记录统计事件，便于后续分析和排查问题。

**执行流程**：
```
Awakening.awaken()
    │
    ├── 调用大脑思考
    │       └── BrainDal.think()
    │               ├── 成功 → 记录 status="success"
    │               └── 失败 → 记录 status="failed: {error}"
    │
    └── 返回结果
```

**关键代码位置**：`src/service/domain/runtime/awakening.rs` → `awaken()`

**设计要点**：
- 使用 `match think_result` 捕获成功/失败两种情况
- 失败时记录错误信息到 `status` 字段
- 无论成功失败，都记录耗时 `duration_ms`

### 21.3 附带信息模式（Fetch Options）

**设计原则**：通过 `XxxFetchOptions` 结构体控制实体查询时是否加载额外信息，避免接口膨胀。

**AgentFetchOptions 结构**：
```rust
pub struct AgentFetchOptions {
    pub with_runtime_state: Option<bool>,    // 是否加载运行时状态（默认 true）
    pub with_stats: Option<bool>,            // 是否加载统计信息
    pub stats_task_id: Option<String>,       // 统计过滤条件（with_stats=true 时生效）
}
```

**使用示例**：
```rust
// 获取 Agent，同时加载统计信息
let agent = hr_domain.agent_manage().get_agent(
    ctx,
    agent_id,
    AgentFetchOptions {
        with_stats: Some(true),
        stats_task_id: Some(task_id),
        ..Default::default()
    },
).await?;
```

**设计要点**：
- 参数全部可选，使用 `Option<bool>` 而非 `bool`，便于区分"未指定"和"明确指定"
- 默认 `with_runtime_state` 为 `None`（内部处理为 `true`）
- `stats_task_id` 仅在 `with_stats` 为 `Some(true)` 时生效

### 21.4 统计 DAO 扩展

**设计原则**：为工具调用统计新增专用 DAO，与现有的 Agent/Project/Task/ModelProvider 统计 DAO 保持一致的设计模式。

**ToolStatsDao 接口**：
```rust
pub trait ToolStatsDao: Send + Sync {
    async fn query_tool_calls(&self, ctx, query) -> Result<Vec<JsonValue>>;
    async fn sum_calls(&self, ctx, query) -> Result<u64>;
    async fn sum_failed_calls(&self, ctx, query) -> Result<u64>;
    async fn get_stats(&self, ctx, query, options) -> Result<ToolStats>;
}
```

**ToolStats 结构**：
```rust
pub struct ToolStats {
    pub call_summary: Option<CallSummary>,    // 调用次数汇总
    pub failed_count: Option<u64>,           // 失败次数
}
```

**关键代码位置**：`src/service/dao/tool/stats_duckdb.rs`

### 21.5 代码清单

| 文件 | 类型 | 改动内容 |
|------|------|---------|
| `common/src/models/stats.rs` | 修改 | 新增 `ToolStats` 结构体 |
| `src/service/dao/tool/mod.rs` | 修改 | 新增 `ToolStatsDao` trait |
| `src/service/dao/tool/stats_duckdb.rs` | 新增 | ToolStatsDao 的 DuckDB 实现 |
| `src/service/dal/tool.rs` | 修改 | 新增 `get_stats` 方法 |
| `src/service/dal/agent.rs` | 修改 | 新增 `AgentFetchOptions` + 统计注入 |
| `src/pkg/stats/mod.rs` | 修改 | `AgentAwakeEvent` 新增 `task_id` 字段 |
| `src/consumer/message.rs` | 修改 | 轮次限制检查 + 任务完成检测 |
| `src/models/message.rs` | 修改 | `to_prompt()` 按消息类型差异化 |
| `src/service/domain/runtime/context_assembly.rs` | 修改 | `PromptBuilder` 新增工具失败警告 |
| `src/service/domain/runtime/awakening.rs` | 修改 | 唤醒失败时记录事件 |
| `src/consumer/message_tests.rs` | 修改 | 新增 MockProjectDomain |

### 21.6 测试统计

| 指标 | 数值 | 说明 |
|------|------|------|
| 总测试数 | 554 | 比 Phase 2 增加 6 个 |
| 通过率 | 100% | ✅ 全部通过 |
| 新增测试文件 | 1 个 | `src/service/dao/tool/stats_duckdb_test.rs` |
| 修改测试文件 | 1 个 | `src/consumer/message_tests.rs`（新增 MockProjectDomain） |

### 21.7 不在本期范围

| 功能 | 说明 | 计划阶段 |
|------|------|---------|
| 工具失败率实时计算 | 目前仅记录失败次数，未计算失败率 | Phase 4 |
| 记忆中轮次状态追踪 | 轮次信息未写入记忆系统 | Phase 4 |
| 多任务并发限制 | 当前仅按单任务轮次限制 | Phase 5 |
| 动态调整思考深度 | 根据任务复杂度动态调整 | Phase 5 |

---

## 二十二、工具包机制 + 任务执行闭环（Phase 4A）

### 22.1 设计目标

Phase 4A 解决两个核心问题：
1. **工具能力批量授予**：Agent 入职后不应逐个绑定工具，应通过"工具包"批量授予能力
2. **任务执行闭环通知**：任务分配给 Agent 后，应通过消息自动通知 Agent 开始执行

### 22.2 能力分层两维度模型

```
Agent 能力
├── 工具（Tool）
│   ├── 神经工具（Neural）：天生具备，tags 含 "neural"
│   ├── 工具包（ToolPack）：按 tag 分组，入职培训获得
│   └── 外骨骼工具（Exoskeleton）：显式绑定获得
└── Skill（技能）
    ├── 天生技能：Agent 创建时配置
    └── 入职培训技能：后续沉淀
```

### 22.3 工具包 tag 机制

**核心字段**：
- `ToolPo.tags: Vec<String>` — 工具所属的标签列表
- `AgentRuntimeConfig.installed_tags: Vec<String>` — Agent 已安装的工具包 tags

**12 个项目管理工具统一打 "project_management" tag**：
- create_project / update_project / update_project_status / get_project / list_projects
- create_task / update_task / update_task_status / get_task / list_tasks / assign_task / list_task_artifacts

### 22.4 免绑定校验三层逻辑

```
Manual 工具调用校验：
1. 绑定工具？ → Agent.tools 中是否包含该 tool_id
2. 神经工具？ → tool.tags 是否包含 "neural"
3. 已安装工具包？ → tool.tags 与 agent.installed_tags 是否有交集
```

三层任一通过即允许调用，否则拒绝。

### 22.5 Agent 入职自动安装

当 Agent 状态流转到 `Onboarded` 时，自动安装 "project_management" 工具包：
```rust
if target_status == AgentStatus::Onboarded {
    agent.po.install_tag("project_management");
}
```

### 22.6 唤醒时工具加载与分流

工具加载遵循"domain 层统一加载，唤醒时按控制方式分流"原则：

**加载阶段**（`HrDomainImpl::get_agent(with_tools=true)`）：
- 绑定工具：通过 `agent_tools` 关联表查询（`enabled_only` 在 DB 层过滤）
- tag 匹配工具：tag_filter = `neural` + `agent.installed_tags`，SQL 层 `json_each` OR 匹配
- 合并去重后写入 `agent.tools`（`Vec<Tool>` 业务实体），供后续 wake/awaken 使用

**分流阶段**（`RuntimeDomainImpl::awaken`）：
- 全部工具保留在 `agent.tools`，Awakening 显式循环按 `tool.po.control_mode` 分发
- `ControlMode::Auto` 工具通过 `ToolDescriptor::from(&Tool)` 派生后传给 `BrainDal.think()` 作为 function calling 工具列表
- `ControlMode::Manual` 工具由 PromptBuilder 拼装"【常用工具】"区块告知模型存在；模型发起调用时走 `execute_manual`（通过 internal 工具转发）

**设计要点**：
- 不再有"built-in tools"概念，Auto/Manual 区分由 `control_mode` 决定，与工具定义位置无关
- 工具加载在 hr domain 完成（同业务域操作），无需跨 domain 组合
- ToolPo（Clone-able）供 PromptBuilder 使用；Tool 实体在 Awakening 循环中按 `control_mode` 分发到 `execute_auto` / `execute_manual`

### 22.6.1 唤醒时技能加载

技能加载与工具加载模式对齐，但策略略有不同：

**加载阶段**（`HrDomainImpl::get_agent(with_skills=true)`）：
- 仅在 Agent 已安装的技能副本范围内查询（`author_id = agent_id`，排除 Expired）
- 写入 `agent.skills`（`Vec<Skill>` 业务实体）

**注入阶段**（`RuntimeDomainImpl::awaken`）：
- 从 `agent.skills()` 提取 `SkillPo` 列表供 PromptBuilder 使用（与 ToolPo 提取方式一致）
- PromptBuilder 按 tag 分块拼装："【神经技能】"（tags 含 neural）+ "【必加载技能】"（tags 匹配 match_keys）

**技能与工具的关键差异**：
- 工具支持全局 tag 匹配（neural + installed_tags 全局查询），所有 Agent 天生拥有神经工具
- 技能讲究"安装且自进化"，即便是神经技能也必须安装到自身目录才能使用（只在 `author_id = agent_id` 范围内查）
- 不匹配 match_keys 的已安装技能不展示在 Prompt，由 Agent 通过 `search_skill` 神经工具按需渐进式加载

### 22.7 三种角色定位

| 角色 | 注册为工具 | 调用方式 | 示例 |
|------|----------|---------|------|
| 神经工具 Handler | ✅ `#[register_handler_tool(neural)]` | Agent 通过 function calling 调用 | send_message, search_memory |
| internal 工具 Handler | ✅ `#[register_handler_tool(tags="...,internal")]` | 由 `execute_manual` 内部转发，Agent 不可直接调用 | request_tool_call, send_tool_call_message |
| 普通 HTTP Handler | ❌ 不注册 | HTTP API 直接调用 | （供前端或外部 API 使用） |
| Consumer | — | 直接调 Domain | handle_tool_call_request |

### 22.8 TaskAssignment 消息机制

**消息类型**：`MessageType::TaskAssignment = 9`

**Payload 结构**：
```rust
pub struct TaskAssignmentMessage {
    pub task_id: String,
    pub task_title: String,
    pub task_description: Option<String>,
    pub project_id: Option<String>,
    pub from_id: String,
    pub to_agent_id: String,
}
```

**投递方法**：`MessageDelivery::send_task_assignment()`

**神经工具**：`send_task_assignment_message`（Agent 间任务分配）

**Handler 编排**：
- `create_task` 创建任务后自动发送 TaskAssignment 消息
- Project Domain 只管持久化，Message Domain 管通知，Handler 层编排

**PromptBuilder 差异化**：
- `【任务分配通知】` 标签，Agent 唤醒时明确感知任务分配

### 22.9 架构职责分离

```
Handler 层（编排）
├── create_task Handler
│   ├── 调用 Project Domain 创建任务（持久化）
│   └── 调用 Message Domain 发送 TaskAssignment 消息（通知）
└── send_task_assignment_message 神经工具
    └── 调用 Message Domain 投递方法（通知）

Domain 层
├── Project Domain：只管数据持久化
└── Message Domain：管通知能力（send_task_assignment）

Consumer 层
└── handle_agent_message：to_role=Agent 天然触发 awaken
```

### 22.10 测试统计

| 指标 | 数值 | 说明 |
|------|------|------|
| 总测试数 | 569 | 比 Phase 3 增加 15 个 |
| 通过率 | 100% | ✅ 全部通过 |
| 新增 API | 3 个 | install/uninstall/list installed tool packs |
| 新增消息类型 | 1 个 | TaskAssignment |
| 新增神经工具 | 1 个 | send_task_assignment_message |

---

## 二十三、Runtime 执行链路全面修复（v3.4）

> 📌 **本章定位**：Runtime 执行链路的系统性问题排查与修复，覆盖正确性、用户体验、性能与安全 4 个维度共 16 项修复。
>
> **完成状态**：✅ 已全部实现（2026-07-23），745 个测试 100% 通过

### 23.1 修复背景

通过对 Agent 唤醒流程、模型调用链路、ctx 传递机制的深度调查，发现 6 类问题。随后分 5 阶段 18 任务实施修复，完成后再次排查又发现 4 个补充问题，最终全部解决。

### 23.2 修复清单

#### 阶段 1：关键正确性

| # | 问题 | 修复 | 文件 |
|---|------|------|------|
| 2.1 | TOCTOU 竞态：`is_unavailable` + `set_busy` 之间存在窗口 | 原子 `try_set_busy` CAS + RAII `BusyGuard` | `awakening.rs`, `busy_guard.rs` |
| 2.2 | AOP ack/nack 未与 consumer ack/nack 配对 | registry.ack/nack 与 consumer.ack/nack 同时调用 | `consumer/message.rs` |
| 2.3 | 工具调用 trace 完整性：call_id 伪造 | `call_manual` 返回 `(Value, ToolCallEntry)` 传播真实 call_id | `tool_execution.rs` |
| 2.4 | `record_event!` 失败静默丢弃；任务状态检查晚于轮次检查 | 改为 `if let Err` + `log_warn!`；任务状态检查优先 | `awakening.rs`, `message.rs` |

#### 阶段 2：用户体验

| # | 问题 | 修复 | 文件 |
|---|------|------|------|
| 3.1 | 消息链断裂：root_id 始终为自身 ID | root_id 继承父消息（reply_to_id 查询父消息 root_id） | `delivery.rs` |
| 3.2 | SSE 内存泄漏：客户端断开未注销 | `CleanupStream` Drop guard 自动注销 | `subscribe_sse.rs` |
| 3.3 | 投递失败静默：所有渠道失败不触发重试 | 返回 Error 触发 nack | `message.rs` |
| 3.4 | order_key 按 project 分组导致跨 task 阻塞 | 分层策略：Agent→to_id，非 Agent→task_id→project_id | `events/message.rs` |

#### 阶段 3：中等问题

| # | 问题 | 修复 | 文件 |
|---|------|------|------|
| 4.1 | trace_id 并发碰撞 | 加 u16 随机后缀 | `models/memory.rs` |
| 4.2 | stats 查询失败阻塞 agent 加载 | 改为非致命，log_warn 后跳过 | `dal/agent.rs` |
| 4.3 | think() 无超时，Agent 可能永久卡住 | 5 分钟 `tokio::time::timeout` | `awakening.rs` |
| 4.4 | 死代码 + 无效参数 | 移除 user_profile 死代码 + task_id 参数 | `awakening.rs`, `memory.rs` |

#### 阶段 4：优化

| # | 问题 | 修复 | 文件 |
|---|------|------|------|
| 5.1 | Builtin/Http 工具错误暴露底层细节 | 脱敏为 `tool {id} execution failed` | `tool_execution.rs` |
| 5.2 | agent 不存在时静默退化为空 vec | 返回错误 `Agent not found` | `tool_execution.rs` |

#### 补充修复（深度排查后）

| # | 问题 | 严重度 | 修复 | 文件 |
|---|------|--------|------|------|
| S1 | wake_agent_brain 返回的 ctx 缺 model_provider 字段 | MEDIUM | 从 brain.cortex 提取 ModelProvider enrich ctx | `awakening.rs` |
| S2 | RigCortexDao `_ctx` 未使用（代码异味） | LOW | 文档化为 brain 缓存扩展点 | `cortex/mod.rs`, `rig.rs` |
| S3 | thinking_depth 通知失败被 `let _ =` 静默吞掉 | LOW | 改为 `if let Err` + log_warn | `message.rs` |
| S4 | root_id fallback 用当前消息 ID 导致父消息孤立 | LOW | 改用父消息 ID 作为链根 | `delivery.rs` |

### 23.3 关键设计决策

#### 23.3.1 order_key 接收者优先策略

**问题**：原 order_key 用 `task_id → project_id`，但 Agent busy 状态是全局的（不区分 task），导致同 agent 不同 task 的消息被不同 worker 并发取走后 `try_set_busy` 失败，进入 nack 重试循环。

**方案**：分层 order_key
- **接收者是 Agent** → 用 `to_id`（agent_id）：同 agent 消息在队列层串行，把串行点从"失败重试"提前到"队列层"
- **接收者不是 Agent** → 用 `task_id → project_id → ""`：无状态竞争，保持用户消息按 task 顺序

**收益**：减少无效 IO 和重试开销，与 busy 状态语义一致。

#### 23.3.2 wake_agent_brain ctx 补充

**问题**：doc 注释承诺返回含 `model_provider_id`/`model_name` 的 enriched ctx，但 `wake_brain` 返回 `Result<Brain>`，内部的 `enrich_ctx!(ctx, &provider)` 作用在局部变量上，返回后丢失。

**方案**：`wake_brain` 返回 brain 后，从 `brain.cortex` 提取 `ModelProvider` 重新 enrich ctx（仅 Local agent 有 cortex；外部 agent 无 cortex，ctx 保持原样）。

#### 23.3.3 think 超时保护

**问题**：LLM API hang 会导致 Agent 永久卡在 busy 状态。

**方案**：`tokio::time::timeout(300s, think())` 包装，超时返回 Internal error，触发 BusyGuard 释放 + nack 重试。

### 23.4 测试统计

| 指标 | 数值 | 说明 |
|------|------|------|
| 总测试数 | 745 | 修复前后保持一致 |
| 通过率 | 100% | ✅ 全部通过 |
| 修复项 | 16 项 | 12 项计划内 + 4 项补充排查 |
| 提交数 | 11 个 | 分阶段提交，每阶段独立验证 |

---

## 二十四、记忆 tags 全链路支持 + 知识图谱节点可视化增强（v3.5）

**日期：2026-07-24 | 版本：v3.5**

### 24.1 背景

知识图谱节点和短期记忆已具备 `tags` 字段（JSON 数组字符串），但搜索/查询接口未支持按 tags 过滤，前端也未展示 tags。本次补齐全链路能力，并将知识图谱节点升级为通用信息节点组件。

### 24.2 后端：tags 过滤全链路

**DTO 扩展**：
- `SearchMemoryParams` / `QueryMemoryParams` 新增 `tags: Option<Vec<String>>`
- `MemoryResult` 新增 `tags: Option<Vec<String>>`（仅 short_term / knowledge_node 有值）

**DAO 层**：
- `MemoryQuery` 新增 `tags: Option<Vec<String>>` 字段
- `query_short_term` / `query_knowledge_nodes`：QueryBuilder 追加 `EXISTS (SELECT 1 FROM json_each(tags) WHERE json_each.value IN (...))` 过滤
- `search_short_term` / `search_knowledge_nodes`：FTS5 + JOIN 场景改为动态 SQL 拼接，`json_each(m.tags)` 使用表别名
- OR 语义：传入多个 tag 时命中任一即返回（对齐 Tool/Skill 范式）

**Handler 层**：
- `search_memory` / `query_memory` 透传 `params.tags` 到 `MemorySearch.filters.tags` / `MemoryQuery.tags`
- `memory_to_result` 回填 `tags` 字段（通过 `parse_tags_json` 解析 JSON 数组字符串）

**Vectorizable trait 对齐**：
- `ShortTermMemoryIndexPo` / `LongTermKnowledgeNodePo` 实现 `Vectorizable` trait（`vectorize_text` + `vector_collection`）
- DAL 层统一使用 `embed_entity(ctx, cortex, po)` 替代手动拼接 `embed_text_for_search`
- 向量搜索场景下 tags 过滤在 query 层生效（向量命中的节点若不满足 tags 条件会被过滤掉）

### 24.3 前端：知识图谱节点可视化增强

**通用信息节点组件**：将 Graph 节点升级为承载多维度信息的组件，信息越多节点越大。

**GraphNode 结构扩展**：
- 新增 `tags: Vec<String>` — 标签列表
- 新增 `summary: Option<String>` — 摘要

**多色边框**：
- 每个 tag 对应一段 SVG arc path，等分圆周拼接成多色环
- tag 颜色基于字符串 hash 稳定取色（10 色预设色板），同一 tag 始终同色
- 无 tags 时保持原白色单色边框

**tags 文字标签**：
- 节点上方紧贴显示，每个 tag 是带颜色底色的小圆角标签（rect + 白色文字）
- 横向居中排列，宽度按字符估算（中文 9px，英文 5px）

**动态半径**：
- 基础半径按类型（knowledge_node 26, short_term 22, trace 18）
- 每个 tag +2（最多 +12）
- 有简介 +3，名称 >8 字符 +2

**节点内容**：
- 圆心显示名称（截断 10 字）
- 下方显示简介截断一行（小灰字）

### 24.4 测试统计

| 指标 | 数值 | 说明 |
|------|------|------|
| 总测试数 | 746 | +1 新增 tags 过滤测试 |
| 通过率 | 100% | ✅ 全部通过 |
| 提交数 | 3 个 | 后端 DTO/DAO/Handler + 前端 API/UI + 节点可视化增强 |

---

## 二十五、唤醒/沉睡场景化设计（v3.6）

> 📌 **本章定位**：将 `awaken` 与新增的 `sleep_and_settle` 抽象为对称的"唤醒/沉睡"双场景，通过 `ThinkingScene` / `ThinkingOptions` 统一签名扩展，实现工具双层过滤与业务上下文注入。
>
> **完成状态**：✅ 已全部实现（2026-07-31），812 个测试 100% 通过

### 25.1 设计动机

**v3.5 之前**：Runtime 只有 `awaken` 一个唤醒入口，沉淀记忆（settle_memory）通过 handler 直接调 LLM 完成，与消息层耦合，沉淀约束模板散落在 handler 中。

**v3.6 升级**：
1. **对称性**：`sleep_and_settle` 与 `awaken` 对称——awaken 是醒来响应外部消息，sleep_and_settle 是沉睡整理内部记忆
2. **解耦**：settle_memory handler 不再直接调 LLM，改为调用 `sleep_and_settle`，与消息层解耦
3. **场景化**：通过 `ThinkingScene` 区分场景，工具按场景过滤，避免沉淀模式下误调消息类工具触发异步唤醒自己
4. **上下文完整**：沉淀场景保留 user_profile + project/task 上下文，沉淀出的经验自带场景标签

### 25.2 核心数据结构

#### ThinkingScene 枚举

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThinkingScene {
    #[default]
    Awaken,  // 唤醒场景：响应外部消息，加载全部工具
    Settle,  // 沉睡场景：沉淀记忆，只加载记忆相关工具（neural/memory tag）
}

impl ThinkingScene {
    /// 判断工具是否在此场景可用
    pub fn is_tool_allowed(&self, tags: &[String]) -> bool {
        match self {
            ThinkingScene::Awaken => true,
            ThinkingScene::Settle => tags.iter().any(|t| t == "neural" || t == "memory"),
        }
    }
}
```

#### ThinkingOptions 结构体

```rust
#[derive(Debug, Clone, Default)]
pub struct ThinkingOptions {
    pub scene: ThinkingScene,
    pub project: Option<Project>,       // 消息关联的项目实体（awaken 场景使用）
    pub task: Option<Task>,             // 消息关联的任务实体（awaken 场景使用）
    pub user_profile: Option<UserPo>,   // 用户画像（预留扩展）
}
```

**设计要点**：
- 统一 options 字段避免频繁修改 awaken/sleep_and_settle 方法签名
- scene 字段控制工具过滤行为
- project/task 实体通过 builder 拼装到 prompt，沉淀出的经验自带场景标签

### 25.3 RuntimeAwakening trait 签名升级

```rust
#[async_trait]
pub trait RuntimeAwakening: Send + Sync {
    async fn wake_agent_brain(
        &self,
        ctx: RequestContext,
        agent: &mut Agent,
        scene: ThinkingScene,           // 新增：场景标识
    ) -> Result<RequestContext>;

    async fn awaken(
        &self,
        ctx: RequestContext,
        agent: &Agent,
        message: &Message,
        options: &ThinkingOptions,      // 新增：场景上下文
    ) -> Result<AwakeningResult>;

    async fn sleep_and_settle(
        &self,
        ctx: RequestContext,
        agent: &Agent,
        pending_memories_summary: &str,
        options: &ThinkingOptions,      // 新增：场景上下文
    ) -> Result<AwakeningResult>;
}
```

### 25.4 工具双层过滤机制

**问题**：唤醒 brain 在两种场景用同一个方法，传同一套 ToolDescriptor 给 `BrainDal.think()`。若 Settle 场景不过滤工具，模型可能通过 function calling 调用 `send_message` 等消息类工具，触发消息流程导致异步唤醒自己，破坏沉淀内循环。

**方案**：双层过滤，分别覆盖 Auto 工具和 Manual 工具/技能

| 层级 | 过滤位置 | 过滤对象 | 触发条件 |
|------|---------|---------|---------|
| 第一层 | `wake_agent_brain` | Auto 工具（传给 think() 的 ToolDescriptor） | Settle 场景：只保留 tags 含 `neural` 或 `memory` |
| 第二层 | `sleep_and_settle` | Manual 工具 + skill（Prompt 展示层） | Settle 场景：只保留 tags 含 `neural` 或 `memory` |

**过滤逻辑**（在 awakening.rs 中实现）：

```rust
// wake_agent_brain 中的 Auto 工具过滤
let auto = match scene {
    ThinkingScene::Awaken => auto,
    ThinkingScene::Settle => auto
        .into_iter()
        .filter(|t| scene.is_tool_allowed(&t.po.get_tags()))
        .collect(),
};

// sleep_and_settle 中的 skill + Manual 工具过滤
let scene = options.scene;
let skill_pos: Vec<SkillPo> = agent.skills().iter()
    .filter(|s| scene.is_tool_allowed(&s.po.parse_tags()))
    .map(|s| s.po.clone())
    .collect();
let all_tools: Vec<ToolPo> = agent.tools().iter()
    .filter(|t| scene.is_tool_allowed(&t.po.get_tags()))
    .map(|t| t.po.clone())
    .collect();
```

**设计要点**：
- Auto 工具过滤在 brain 装配阶段（派生 ToolDescriptor 前），过滤后模型无法通过 function calling 调用
- Manual 工具/skill 过滤在 Prompt 拼装阶段，过滤后模型在 Prompt 中看不到这些工具
- Awaken 场景不做过滤，全部工具可用

### 25.5 业务上下文注入

**问题**：唤醒流程中若消息携带 project_id / task_id，Agent 不知道当前所属项目和任务，沉淀出的经验缺少场景标签。

**方案**：通过 ThinkingOptions 传递实体，由 PromptBuilder 拼装到 prompt

**数据流**：

```
消息消费者（consumer/message.rs）
    │
    ├── 检查 task 状态时缓存 task 实体（避免重复查询）
    ├── 按需查询 project 实体（仅当 project_id 存在）
    │
    └── 构造 ThinkingOptions
        ├── with_project(project)  ← 按需查询
        └── with_task(task)        ← 复用缓存
            │
            ▼
        awaken(ctx, agent, message, &options)
            │
            ├── builder.project_context(project)
            ├── builder.task_context(task)
            │
            └── build() 拼装到 Prompt 的【项目上下文】【任务上下文】区块
```

**Context 补充原则**（遵循项目硬约束）：
- task 实体复用任务状态检查时的查询结果，不重复查询
- project 实体仅在下游 awaken 需要 project 上下文时才查询
- 不为补充上下文执行额外的查询操作，仅使用当前业务逻辑中已有的信息

### 25.6 sleep_and_settle 流程

```
settle_memory handler / awaken 上下文压缩 / awaken 正常完成
    │
    ├── build_pending_memories_summary: 查询未沉淀短期记忆，生成编号摘要
    │
    └── load_and_settle: 加载 Agent（含 tools+skills）
        │
        ├── wake_agent_brain(scene=Settle): 装配 Brain + 过滤 Auto 工具
        │
        └── sleep_and_settle(options=ThinkingOptions::for_scene(Settle), trace_ids)
            │
            ├── set_resting + RAII guard
            ├── 读取最近短期记忆作为 history
            ├── 过滤 skill + Manual 工具（只保留记忆相关）
            ├── 拼装 Prompt: builder.build_sleep_prompt(summary, trace_ids)
            │     ├── 沉淀约束模板内聚在 PromptBuilder
            │     └── 强制写入沉淀摘要指令：要求 Agent 调用 save_short_term_memory
            │         └── trace_ids 字段必须填入本次沉淀依赖的 trace 列表
            ├── think()（5 分钟超时）
            ├── 写 Trace
            ├── 记录统计事件（status: settle success/failed）
            └── set_idle（RAII guard）
```

**沉淀约束模板**（内聚在 `PromptBuilder.build_sleep_prompt`）：
- 不发送消息（睡觉是自身知识沉淀，不依赖外部信息）
- 不调用消息类工具（避免触发消息流程导致异步唤醒自己）
- 只使用记忆类工具（search_memory / save_long_term_memory / update_memory / query_memory / save_short_term_memory）
- 内循环：与自己的记忆对话，不是与外部世界交互
- **强制写入沉淀摘要**：沉淀完成后必须调用 `save_short_term_memory` 写入短期记忆，trace_ids 字段必须填入 prompt 提供的列表

### 25.7 PromptBuilder 扩展

新增方法（trait 定义在 `src/models/prompt_builder.rs`，实现在 `src/service/dal/agent.rs`）：

```rust
pub trait PromptBuilder: Send + Sync {
    // ... 现有方法
    fn project_context(&mut self, project: &Project);
    fn task_context(&mut self, task: &Task);
    fn build_sleep_prompt(&self, pending_memories_summary: &str, trace_ids: &[String]) -> String {
        let _ = (pending_memories_summary, trace_ids);
        self.build()
    }
    fn build_summary_prompt(
        &self,
        work_summary: &str,
        total_rounds: usize,
        trace_ids: &[String],
    ) -> String {
        let _ = (work_summary, total_rounds, trace_ids);
        self.build()
    }
}
```

**DefaultPromptBuilder 实现要点**：
- 提取 `build_tools_and_skills_sections` 复用工具/技能区块拼装逻辑
- 提取 `build_common_context_sections` 复用 user_profile + project + task 上下文拼装逻辑
- `build_sleep_prompt` 复用 system_prompt + tools + skills + common_context + history，跳过 tool_failures 和 current_message，附加沉淀约束 + 待沉淀记忆 + 任务步骤 + **强制写入沉淀摘要指令**
- `build_summary_prompt` 同样附加工作摘要 + 总结任务 + **强制写入短期记忆指令**
- 两个 prompt 模板都会将 `trace_ids` 渲染为 JSON 数组字符串，要求 Agent 调用 `save_short_term_memory` 时填入

**沉淀场景保留 user_profile 的理由**：认知是具身的，不知道自己是谁就不能形成有效认知。沉淀是对自身经验的整理，必须保留身份认知。

### 25.8 settle_memory handler 重构

**v3.5 之前**：handler 内部构造沉淀 prompt + 直接调 BrainDal.think()，与消息层耦合。

**v3.6 重构**：
- `build_pending_memories_summary`：只生成待沉淀记忆的编号摘要，约束模板由 builder 注入
- `load_and_settle`：调用 `wake_agent_brain(scene=Settle)` + `sleep_and_settle(options)`，与消息层解耦
- 复用性：`load_and_settle` 供 settle_memory handler 和 CronTrigger agent_rest 共用

### 25.9 代码清单

| 文件 | 类型 | 改动内容 |
|------|------|---------|
| `src/service/domain/runtime/awakening.rs` | 修改 | 新增 ThinkingScene/ThinkingOptions；wake_agent_brain 加 scene 参数 + Auto 工具过滤；awaken 加 options 参数 + 注入 project/task；新增 sleep_and_settle 实现；新增 awaken_for_summary + pending_trace_ids 跟踪 + 正常 Final 完成触发总结流程 |
| `src/service/domain/runtime/mod.rs` | 修改 | RuntimeAwakening trait 签名升级；awakening 模块改为 pub；sleep_and_settle 加 trace_ids 参数 |
| `src/models/prompt_builder.rs` | 修改 | PromptBuilder trait 新增 project_context/task_context/build_sleep_prompt/build_summary_prompt；两个 build 方法都接收 trace_ids 参数 |
| `src/service/dal/agent.rs` | 修改 | DefaultPromptBuilder 实现新方法；提取 build_tools_and_skills_sections/build_common_context_sections 复用方法；build_sleep_prompt/build_summary_prompt 强制写入短期记忆指令 + trace_ids 渲染 |
| `src/models/project.rs` | 修改 | 新增 to_prompt_summary 方法 |
| `src/models/task.rs` | 修改 | 新增 to_prompt_summary 方法 |
| `src/consumer/message.rs` | 修改 | 构造 ThinkingOptions 传入 awaken；task 复用缓存、project 按需查询 |
| `src/handlers/hr/agent/settle_memory.rs` | 修改 | 重构为 build_pending_memories_summary + load_and_settle，与消息层解耦；sleep_and_settle 调用传 trace_ids=&[]（独立沉淀场景） |
| `src/handlers/hr/agent/save_short_term_memory.rs` | 修改 | SaveShortTermMemoryParams 接收 trace_ids 字段并序列化到 ShortTermMemoryIndexPo |
| `common/src/api/neural_tools.rs` | 修改 | SaveShortTermMemoryParams 新增 trace_ids 字段 |

### 25.10 测试统计

| 指标 | 数值 | 说明 |
|------|------|------|
| 总测试数 | 812 | +66 新增（含沉淀流程、tags 过滤、is_published 字段等） |
| 通过率 | 100% | ✅ 全部通过 |
| 新增核心机制 | 3 项 | ThinkingScene/ThinkingOptions、工具双层过滤、build_sleep_prompt 沉淀约束 |
| 新增对称方法 | 1 个 | sleep_and_settle（与 awaken 对称） |

### 25.11 与历史设计的关系

| 历史设计 | v3.6 升级 |
|---------|----------|
| 第十七章 决策 2：用户画像仅客服 Agent 显示 | 升级：沉淀场景也保留 user_profile（认知是具身的） |
| 第十七章 决策 3：用户画像区块位置 | 保持：位于 Agent 人设与历史对话之间 |
| 第十九章 Agent 运行时状态管理 | 扩展：sleep_and_settle 使用 Resting 状态，复用 BusyGuard 的 set_idle 恢复语义 |
| memory_design.md 第十七章 休息与知识沉淀机制 | 升级：沉淀不再工程化创建节点，改为 Agent 自主沉淀（详见 memory_design.md 更新） |

### 25.12 统一总结流程与强制记忆写入（v3.7 增量）

**问题背景**：原 v3.6 中，上下文压缩沉淀（ContextOverflow → sleep_and_settle）、轮次耗尽总结退出（MaxRoundsExceeded → awaken_for_summary）是两条独立流程，且依赖 Agent "自发" 调用 `save_short_term_memory` 写入短期记忆。实际运行中 Agent 经常遗忘，导致总结沉淀后的经验没有形成短期记忆记录，也无法追溯到原始 trace。

**v3.7 设计要点**：
1. **统一总结流程**（方案 B）：正常 Final 完成（用户拿到回答）也触发 `awaken_for_summary`，与 MaxRoundsExceeded 共用同一总结流程，确保不漏掉短期记忆写入
2. **pending_trace_ids 跟踪**：awaken 循环维护 `pending_trace_ids`，跟踪自上次压缩以来产生的 trace 列表；压缩完成后重置为 `[settle_trace_id]`；总结时作为本次总结依赖的 trace 列表传入 prompt
3. **trace_ids 透传到 Prompt**：`build_sleep_prompt` 和 `build_summary_prompt` 都新增 `trace_ids: &[String]` 参数，在 prompt 模板中渲染为 JSON 数组字符串
4. **强制写入短期记忆指令**：两个 prompt 模板都明确要求 Agent **必须**调用 `save_short_term_memory`，并将 prompt 中提供的 `trace_ids` 填入 `trace_ids` 字段，保证记忆可追溯
5. **API 扩展**：`SaveShortTermMemoryParams` 新增 `trace_ids: Option<Vec<String>>` 字段，handler 序列化后存入 `ShortTermMemoryIndexPo.trace_ids`

**关键流程**（统一后）：
```
awaken 循环
    │
    ├── ContextOverflow → sleep_and_settle(trace_ids=pending_trace_ids)
    │   ├── 沉淀 + 强制写入沉淀摘要（含 trace_ids）
    │   └── 重置 pending_trace_ids = [settle_trace_id]
    │
    ├── MaxRoundsExceeded → awaken_for_summary(trace_ids=pending_trace_ids + [awaken_trace_id])
    │   └── 总结 + 强制写入短期记忆（含 trace_ids）
    │
    └── Final（正常完成）→ awaken_for_summary(trace_ids=pending_trace_ids)
        └── 总结 + 强制写入短期记忆（含 trace_ids，失败不阻塞业务返回）
```

**与 v3.6 的关系**：v3.7 是 v3.6 的增量升级，不改变 ThinkingScene/ThinkingOptions 框架，只在总结流程和 prompt 模板层面增强，确保短期记忆不遗漏、可追溯。

---

## 下一步讨论方向

1. **统计模块驱动的外部唤醒轮次**（推荐 P1：让 ToolCallResult 触发下一次 Agent 唤醒，但轮次预算、暂停/继续和页面状态统一来自统计模块；必要时通过强类型 trace_ref 查询完整 ToolCallEntry）
2. **ToolCallResult attachment / artifact 产物化引用策略**（P1，仅当结果需要用户下载或成为 Project Artifact 时接入）
3. **Trace ID 关联链实现**（P1，完善 `message.reply_to_id` 追溯能力）
4. **streamable HTTP MCP runtime**（P2，继承 HTTP Tool SSRF/header/redirect 安全策略后再做）
5. **技能动态注入策略**（P2，Agent 能力扩展）


