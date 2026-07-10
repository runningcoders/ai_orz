# Runtime Domain 设计

> 🎯 **本文档定位**：Runtime Domain（运行时领域）的整体设计大纲与逻辑思路
>
> 范围：只覆盖**总纲与核心理念**，不下沉到具体工具实现与代码细节
> 状态：v1.0（2026-07-10）
>
> **更新记录**：
> - v1.0 (2026-07-10): Phase 2 神经工具集落地，8 个神经工具全部实现，自动回复移除，548 测试通过
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
- 最终用户回复必须由 Agent 在 Rig 思考过程中主动调用 `send_message` / handler-backed 神经工具完成。
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
| **神经级（Native / Rig Auto）** | 大脑 → 手眼口 | 启动时直接挂到模型 tool list | 同步、回合内 | 查工具列表、查技能列表、读写记忆 |
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
| `neural_tools` | 注册并暴露内置工具（rig Tool trait 适配） | 不实现业务逻辑（业务委托给 DAL） |
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
    /// 一次调用 = 一次模型推理 + 该回合内的多次神经工具调用（rig auto）。
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

**rig 回合内多步工具调用是允许的**：
- rig 的 auto 模式下，模型可以在一次 `prompt()` 内连续调用多次神经工具然后给出最终回答
- 这是**模型自主行为**，不是我们写的循环——符合"模型自己决定"的原则

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
    pub neural_tools: Vec<ToolSpec>,     // 注册给 rig 的工具列表
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

每项都是 rig 原生 `Tool`，模型推理回合内可直接同步调用。神经工具通过 `register_handler_tool` 宏的 `neural` flag 标记，所有 Agent 默认拥有，不需要权限校验。

| 工具名 | 作用 | 委托给 | 优先级 |
|--------|------|--------|--------|
| `search_memory` | 混合搜索长期/短期记忆 | `RuntimeMemory.search` | P0 |
| `send_message` | 给 user / agent / channel 发消息 | `MessageDomain.delivery` | P0 |
| `request_tool_call` | 异步发起一次外骨骼工具调用 | `tool_execution` | P0 |
| `mark_done` | 显式标记本任务完成 | Runtime 内部 | P1 |
| `list_tools` | "想起"有哪些外骨骼工具可用（仅返回名字+一句话） | `ToolDal` | P1 |
| `search_skill` | "想起"有哪些相关技能 | `SkillDal` | P2 |
| `read_skill` | 取出某个技能的具体内容 | `SkillDal` | P2 |
| `read_tool_spec` | 展开某个工具的完整 schema | `ToolDal` | P2 |
| `write_memory` | 沉淀新记忆 | `RuntimeMemory.write` | P2 |

**设计要点**：
- `list_*` 系列只返回**摘要**（名字+一句话），不含完整 schema，避免 prompt 膨胀
- 真正要用某项能力时，模型主动调 `read_*` 系列展开
- 这正是"想起来"的过程——模型先看到目录，再决定翻哪一页
- 神经工具通过 `#[register_handler_tool(... neural)]` 标记，生成的 ToolPo 自动包含 `"neural"` tag
- 唤醒时只注入带 `"neural"` tag 的工具给模型

**注册示例**：
```rust
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
```

### 4.3 神经 vs 外骨骼的关系图

```
模型推理 (一回合内)
    │
    ├── 神经工具 (rig auto，同步)
    │      │
    │      ├── search_memory ───► RuntimeMemory ───► MemoryDal
    │      ├── search_skill  ───► SkillDal
    │      ├── list_tools    ───► ToolDal
    │      └── send_message  ───► MessageDomain
    │
    └── request_tool_call (神经工具，但触发异步)
           │
           ▼
        tool_execution ──► 落"待执行"消息 ──► 消息消费者异步执行
                                                      │
                                                      ▼
                                              结果消息触发下一次 awaken()
```

---

## 五、已确认的设计决策 ✅

以下设计决策已在实现中落地并验证。

### Q1：单回合 vs 多回合循环（已确认）

- **决策**：Runtime 内部只跑**单回合**，多回合靠"消息触发再次唤醒"
- **理由**：贴近"无复杂循环"原则；rig auto 已支持回合内多次工具调用，足够覆盖大多数场景
- **落地状态**：✅ 已实现（见第十八、十九章）

### Q2：rig 回合内多步 tool calling 算不算"循环"（已确认）

- **决策**：算"模型自主行为"，不算我们写的循环，**允许**
- **理由**：符合"模型自己决定"的原则，框架不应剥夺这个能力
- **落地状态**：✅ 已实现（rig auto 已集成）

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
| **3** | 实现 `NeuralTools` 最小集（预留） | `neural_tools.rs` 骨架 | 编译通过 | ⚠️ 部分实现 |
| **4** | 接入 Cortex（模型推理）到 `Awakening` | 端到端最小可用 | 集成测试通过 | ✅ 已完成 |
| **5** | 加入消息通道和外骨骼通道 | 完整双通道 | Agent 可对话 + 调外骨骼工具 | ✅ 已完成 |
| **6** | 补齐展开式工具（预留） | 完整神经工具集 | Agent 行为完整 | ⚠️ 部分实现 |

**实际落地情况**：
- ✅ 核心架构全部落地：Runtime Memory + Context Assembly + Awakening
- ✅ Trace 闭环架构完成（第十八章）
- ✅ Agent 运行时状态管理完成（第十九章）
- ⚠️ 神经工具集部分实现（见 tool_design.md）
- ⚠️ 多 Agent 协作、记忆压缩、长上下文管理等高级功能仍在规划阶段

**不在本期范围**（留待后续设计）：
- 多 Agent 协作的高级编排（如团队、角色分工）
- Cortex 内部的模型选择策略、降级策略
- 记忆压缩、长上下文窗口管理
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

## 九、唤醒 Agent 操作的具体执行流程（早期设计）

> 📌 **演进说明**：此章为早期设计思路（草案 v0.2，2026-05-25），包含完整的12步流程分析。
>
> **实际实现已简化为7步**，见第十八章"记忆 Trace 闭环架构 + PO 自格式化重构完成"（v0.7，2026-05-28）。
>
> 此章保留作为历史参考，帮助理解架构演进过程。

> 📌 **本章细化**：具体描述一次 `awaken()` 调用从开始到结束的完整执行路径，不涉及对外扩散。

### 9.1 唤醒调用的入口点

**调用入口**：消息消费者的 `handle_agent_message()` 方法。

```
消息总线 → Consumer → handle_agent_message() → Runtime::awaken()
```

这是唯一的唤醒入口。Runtime 不提供其他对外的唤醒接口，所有唤醒都由消息驱动。

---

### 9.2 完整的唤醒执行流程（12 步）

```
1. 参数校验 → 2. 加载 Agent 实体 → 3. 加载触发消息
   ↓
4. 拼装上下文（System Prompt + 神经工具）→ 5. 提取近期工作记忆
   ↓
6. 注入神经工具到 Cortex → 7. 调用模型推理（含回合内多步工具调用）
   ↓
8. 捕获模型输出 → 9. 落思考轨迹到记忆 → 10. 处理收口信号
   ↓
11. 生成 Outcome → 12. 返回（所有副作用已在神经工具内即时落地）
```

#### 第 1-3 步：准备阶段（纯数据加载，无副作用）

```rust
async fn awaken(
    &self,
    ctx: RequestContext,
    cmd: AwakenCommand,
) -> Result<AwakenOutcome, AppError> {

    // === Step 1: 参数校验 ===
    // 校验 agent_id 非空，trigger 类型合法
    cmd.validate()?;

    // === Step 2: 加载 Agent 实体 ===
    // 通过 FinanceDomain 获取 Agent 完整实体（含配置 + 绑定的工具/技能）
    let agent = finance_domain().get_agent(ctx, &cmd.agent_id).await?;

    // === Step 3: 加载触发消息（如果有）===
    // UserMessage / AgentMessage 等 trigger 需要加载消息内容
    let trigger_message = match &cmd.trigger {
        Trigger::UserMessage { message_id } => {
            Some(message_domain().get_message(ctx, message_id).await?)
        }
        Trigger::AgentMessage { message_id, .. } => {
            Some(message_domain().get_message(ctx, message_id).await?)
        }
        Trigger::ToolResult { tool_call_id } => {
            // 工具结果也需要加载对应的消息体
            Some(message_domain().get_message_by_tool_call_id(ctx, tool_call_id).await?)
        }
        _ => None,
    };

    // ...
}
```

#### 第 4-5 步：上下文拼装（纯函数，无副作用）

```rust
    // === Step 4: 拼装系统提示词 ===
    // 调用 ContextAssembly 纯函数：
    //  - Agent 身份（角色 + 人设）
    //  - 触发场景描述（"用户对你说" / "另一个 Agent 对你说"）
    //  - 元思考提示（"遇到不会的可以查技能/记忆"）
    //  - 神经工具列表（名字 + 一句话描述，不含完整 schema）
    let system_prompt = context_assembly().build_system_prompt(
        &agent,
        &cmd.trigger,
        trigger_message.as_ref(),
    );

    // === Step 5: 提取近期工作记忆（最近 N 条会话轨迹）===
    // 从 RuntimeMemory 拉取本 Agent 最近的会话痕迹，作为上下文的一部分
    let recent_traces = runtime_memory()
        .get_recent_traces(ctx, &agent.po.id, 20)  // 最近 20 条
        .await?;

    // 拼装成完整对话历史
    let conversation = context_assembly().build_conversation(
        system_prompt,
        &recent_traces,
        trigger_message.as_ref(),
    );
```

**设计要点**：
- 第 4-5 步都是**纯函数**，只输入数据，不产生副作用
- 上下文极薄原则：不预加载技能完整内容，只给"可以查技能"的提示和工具入口

#### 第 6-7 步：推理执行（有副作用，但副作用在神经工具内部）

```rust
    // === Step 6: 注入神经工具到 Cortex ===
    // 从 NeuralTools 注册表获取所有神经工具的 ToolDyn 实现
    // 注入到 Agent 的 Cortex 中（Cortex 创建时其实已经注入了，这里是动态补充）
    //
    // 【注意】神经工具是每个 Agent 默认都有的，与 Agent 绑定的自定义工具不同
    let neural_tools = neural_tools().get_all_tools(ctx, &agent);

    // === Step 7: 调用模型推理 ===
    // 调用 Cortex::prompt_with_tools() 进行推理
    // rig 会自动处理回合内的多步工具调用（直到模型不再调用工具）
    // 神经工具在调用时会即时落地副作用（发消息、写记忆等）
    let raw_response = agent
        .cortex()
        .ok_or(AppError::AgentNotInitialized)?
        .prompt_with_tools(conversation.to_string(), neural_tools)
        .await?;
```

**关键机制**：
- 神经工具调用是**同步、即时落地**的（写记忆直接落库，发消息直接进消息总线）
- rig 框架自动处理回合内的多步工具调用（这是允许的"模型自主行为"，不算我们写的循环）
- 所有副作用在神经工具调用时就已经发生，不需要等推理结束再统一处理

#### 第 8-10 步：收尾处理

```rust
    // === Step 8: 捕获模型输出（已经在 Step 7 拿到了）===
    // raw_response 是模型最终的自然语言输出

    // === Step 9: 落思考轨迹到记忆 ===
    // 把本次推理的完整痕迹（输入 + 输出 + 调用过的工具）写入 RuntimeMemory
    runtime_memory()
        .write_thinking_trace(
            ctx,
            &agent.po.id,
            ThinkingTrace {
                trigger: cmd.trigger.clone(),
                input: conversation.to_string(),
                output: raw_response.clone(),
                tool_calls: vec![],  // TODO: 从 Cortex 获取实际调用过的工具列表
                timestamp: chrono::Utc::now(),
            },
        )
        .await?;

    // === Step 10: 处理收口信号 ===
    // 检查神经工具中是否有 mark_done / stay_silent 等收口信号被调用
    // 从 NeuralTools 的调用上下文中提取信号（神经工具内部有状态记录）
    let done_signal = neural_tools().extract_done_signal();
```

#### 第 11-12 步：返回结果

```rust
    // === Step 11: 生成 Outcome ===
    let outcome = AwakenOutcome {
        done_signal,
        raw_response: Some(raw_response),
    };

    // === Step 12: 返回 ===
    // 注意：所有副作用（发消息、写记忆、工具执行）都已经在前面即时落地了
    // Outcome 只是用于日志和上层观测，不承载实际副作用
    Ok(outcome)
}
```

---

### 9.3 神经工具的执行机制

神经工具的执行是唤醒流程中最核心的部分，需要特别说明：

#### 9.3.1 神经工具的调用路径

```
模型决定调用 search_memory
    ↓
rig 框架解析 tool call 参数
    ↓
调用 ToolDyn::call() 方法
    ↓
Tool 实现内部调用对应的 DAL/Domain
    ↓
search_memory 直接调 RuntimeMemory.search() → 读库
send_message 直接调 MessageDomain.delivery.send() → 写消息总线
mark_done 在工具内部设置一个 done 标志 → 后面提取
    ↓
返回结果给模型 → 模型继续思考（可能继续调用其他工具）
```

#### 9.3.2 神经工具的两层设计

**第一层：Tool 接口适配层**（在 `neural_tools.rs` 中）
- 实现 rig 的 `ToolDyn` trait
- 参数解析：把模型传的 JSON 转为强类型结构体
- 错误处理：把业务错误转为工具调用的错误格式
- 记录调用痕迹：标记哪些工具被调用过

**第二层：业务实现层**（在各自的 Domain/DAL 中）
- 实际的业务逻辑：读记忆、发消息等
- 不感知 Tool 接口，只提供普通的 async fn

这样设计的好处：
- 业务逻辑不需要知道自己是被工具调用还是被 HTTP Handler 调用
- 同一套业务逻辑可以同时暴露为 HTTP API 和神经工具，符合你之前说的"handler 即工具"的方向

---

### 9.4 ContextAssembly 的具体拼装逻辑

#### 9.4.1 System Prompt 的组成结构

```
【角色定位】
你是 {agent.name}，ID: {agent.id}
你的角色是：{agent.role_description}

【当前场景】
你正在处理 {trigger_type} 触发的任务
{trigger_description}  // "用户对你说了一句话" / "收到了工具执行结果" 等

【可用能力】
你可以调用以下内置工具：
  • search_memory(kw: str) - 搜索你的长期记忆
  • write_memory(content: str) - 沉淀新的记忆
  • search_skill(kw: str) - 搜索可用的技能
  • read_skill(skill_id: str) - 读取某个技能的完整内容
  • send_message(to: str, content: str) - 发送消息
  • mark_done() - 标记任务完成

【思考提示】
- 遇到不懂的，先查记忆和技能，不要不懂装懂
- 每调用一次工具，你会得到结果，然后可以继续思考
- 不需要的内容不要预加载，用到时再查
- 任务完成了就调用 mark_done，不要继续啰嗦
```

#### 9.4.2 对话历史的拼装规则

```
1. 第一条永远是上面的 System Prompt
2. 接着是最近 N 条工作记忆（按时间倒序，最新的在后面）
3. 最后一条是本次的触发消息
4. 总 token 数控制在模型上限的 70% 以内，超出时裁剪旧的记忆
```

---

### 9.5 AwakenOutcome 的设计哲学

**极简设计**：`AwakenOutcome` 只包含两个字段：
- `done_signal: Option<DoneSignal>` - 是否以及如何收口
- `raw_response: Option<String>` - 模型的原始自然语言输出

**为什么不再多？**
- 所有消息已经通过 `send_message` 神经工具即时写入消息总线
- 所有记忆已经通过 `write_memory` 神经工具即时写入记忆库
- 所有工具调用的副作用已经在调用时即时落地
- Consumer 不需要再做"派发"，Outcome 只用于日志和观测

这就是"一切动作皆消息"的最终体现——Runtime 不攒结果，不做派发，调用即落地。

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

#### 📁 1. 新增：`src/service/domain/runtime/awakening.rs`

**核心接口设计：**

```rust
/// 唤醒参数
pub struct AwakenForUserMessage {
    pub agent_id: String,
    pub user_message_id: String,
}

/// 唤醒结果
pub struct AwakeningResult {
    pub agent_id: String,
    pub raw_input: String,
    pub raw_output: String,
    pub thinking_trace_ids: Vec<String>,
}

#[async_trait]
pub trait Awakening: Send + Sync {
    /// 唤醒 Agent 处理用户消息（第一阶段最小可用）
    async fn awaken_for_user_message(
        &self,
        ctx: RequestContext,
        cmd: AwakenForUserMessage,
    ) -> Result<AwakeningResult, AppError>;
}
```

**单例注册：** 对齐 Runtime 现有风格，使用 `OnceLock` 注册单例。

---

#### 📁 2. 新增：`src/service/domain/runtime/context_assembly.rs`

**纯函数模块，无副作用：**

```rust
/// 上下文拼装器（纯函数）
pub struct ContextAssembly;

impl ContextAssembly {
    /// 拼装对话历史
    pub fn build_conversation(
        agent: &Agent,
        user: Option<&UserPo>,
        current_message: &Message,
        recent_traces: &[Memory],
    ) -> String {
        let mut prompt = String::new();

        // 1. Agent 身份部分
        prompt.push_str(&format!("你是 {}，ID：{}\n", agent.po.name, agent.po.id));
        if let Some(desc) = &agent.po.description {
            prompt.push_str(&format!("角色描述：{}\n", desc));
        }
        prompt.push_str("\n");

        // 2. 历史对话（最近 N 条记忆）
        if !recent_traces.is_empty() {
            prompt.push_str("【历史对话】\n");
            for trace in recent_traces {
                // 简化：直接使用 memory.content
                prompt.push_str(&format!("{}\n", trace.po.content));
            }
            prompt.push_str("\n");
        }

        // 3. 当前用户消息
        prompt.push_str("【当前消息】\n");
        prompt.push_str(&format!("用户说：{}\n", current_message.po.content));
        prompt.push_str("\n请回复：");

        prompt
    }
}
```

**设计原则：** 纯函数、无 async、无副作用、可单独测试。

---

#### 📁 3. 修改：`src/service/domain/runtime/memory.rs`

**新增 2 个便捷方法：**

```rust
#[async_trait]
pub trait RuntimeMemory: Send + Sync {
    // --- 现有方法 ---
    async fn write(&self, ctx: RequestContext, params: &MemoryCreateParams) -> Result<Memory, AppError>;
    async fn search(&self, ctx: RequestContext, search: &MemorySearch) -> Result<Vec<Memory>, AppError>;
    async fn query(&self, ctx: RequestContext, query: &MemoryQuery) -> Result<Vec<Memory>, AppError>;

    // --- 新增方法 ---

    /// 获取 Agent 最近的短期记忆 Trace
    async fn get_recent_traces(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        limit: i64,
    ) -> Result<Vec<Memory>, AppError>;

    /// 写入思考 Trace
    async fn write_thinking_trace(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        trace_type: ThinkingTraceType,
        content: &str,
    ) -> Result<Memory, AppError>;
}

/// 思考 Trace 类型
pub enum ThinkingTraceType {
    Input,
    Output,
    ToolCall,
    ToolResult,
}
```

**实现要点：**
- `get_recent_traces`: 内部调用 `MemoryDal.query`，过滤 `memory_type = ShortTerm`，按时间倒序 limit
- `write_thinking_trace`: 内部调用 `MemoryDal.create`，预填充 `memory_type`、`agent_id` 等字段

---

#### 📁 4. 修改：`src/service/domain/runtime/mod.rs`

**注册新增模块：**

```rust
// 新增模块导入
mod awakening;
mod context_assembly;

// 新增导出
pub use awakening::{Awakening, AwakeningResult, AwakenForUserMessage};
pub use context_assembly::ContextAssembly;
pub use memory::{ThinkingTraceType, RuntimeMemory};

// RuntimeDomain trait 新增方法
#[async_trait]
pub trait RuntimeDomain: Send + Sync + Debug {
    // --- 现有方法 ---
    fn tool_execution(&self) -> &dyn ToolExecution;
    fn memory(&self) -> &dyn RuntimeMemory;

    // --- 新增方法 ---
    fn awakening(&self) -> &dyn Awakening;
}

// RuntimeDomainImpl 新增字段
pub struct RuntimeDomainImpl {
    tool_execution: tool_execution::ToolExecutionImpl,
    memory: memory::RuntimeMemoryImpl,
    awakening: awakening::AwakeningImpl,  // 新增
}

// new() 方法中初始化 awakening
impl RuntimeDomainImpl {
    pub fn new() -> Self {
        Self {
            tool_execution: tool_execution::ToolExecutionImpl::new(),
            memory: memory::RuntimeMemoryImpl::new(),
            awakening: awakening::AwakeningImpl::new(),  // 新增
        }
    }
}

// 新增便捷方法
pub fn awakening() -> &'static dyn Awakening {
    instance().awakening()
}
```

---

### 11.4 第一阶段唤醒的完整伪代码实现

```rust
// in awakening.rs

async fn awaken_for_user_message(
    &self,
    ctx: RequestContext,
    cmd: AwakenForUserMessage,
) -> Result<AwakeningResult, AppError> {

    // === Step 1: 加载基础数据 ===
    let agent = agent_dal()
        .find_by_id(ctx.clone(), &cmd.agent_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", cmd.agent_id)))?;

    let message = message_dal()
        .find_by_id(ctx.clone(), &cmd.user_message_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Message {} not found", cmd.user_message_id)))?;

    let user = if let Some(user_id) = &message.po.from_user_id {
        user_dal().find_by_id(ctx.clone(), user_id).await?
    } else {
        None
    };

    // === Step 2: 读取最近短期记忆 ===
    let recent_memories = runtime_domain()
        .memory()
        .get_recent_context(ctx.clone(), &cmd.agent_id, None, 20)
        .await?;

    // === Step 3: 收集关联的 Trace ID 列表 ===
    //  - 当前消息关联的 trace_id
    //  - 历史记忆中涉及的 trace_id
    //  - 工具调用关联的 trace_id
    let mut trace_ids = vec![];
    // （预留）从消息 metadata 提取关联 trace_ids
    // if let Some(ids) = message.po.metadata.get("trace_ids") { ... }

    // === Step 4: 拼装 Prompt（带上关联的 Trace IDs，方便 Agent 引用） ===
    let prompt = runtime_domain()
        .context_assembly()
        .build_conversation_prompt(
            &trace_ids,
            &agent,
            &recent_memories,
            &message,
        );

    // === Step 5: 生成本次输入的 Trace ID 并记录 ===
    let input_trace_id = format!("trace-{}-{}", &cmd.agent_id, chrono::Utc::now().timestamp_nanos());
    let input_trace = runtime_domain()
        .memory()
        .write_thinking_trace(
            ctx.clone(),
            &cmd.agent_id,
            ThinkingTraceType::Input,
            &prompt,
            Some(input_trace_id),
        )
        .await?;

    // === Step 6: 调用模型推理 ===
    // 检查 Agent 是否已唤醒 Brain
    let cortex = agent
        .brain()
        .ok_or_else(|| AppError::BadRequest("Agent brain not initialized".to_string()))?
        .cortex_trait();

    let raw_output = cortex.prompt(&prompt).await?;

    // === Step 7: 记录输出 Trace（复用输入的 trace_id，形成关联对） ===
    let output_trace = runtime_domain()
        .memory()
        .write_thinking_trace(
            ctx.clone(),
            &cmd.agent_id,
            ThinkingTraceType::Output,
            &raw_output,
            Some(input_trace.po.id.clone()),  // 复用输入的 trace_id
        )
        .await?;

    Ok(AwakeningResult {
        agent_id: cmd.agent_id,
        trace_ids: vec![input_trace.po.id, output_trace.po.id],  // 本次产生的 trace 列表
        raw_input: prompt,
        raw_output,
        thinking_trace_ids: vec![],  // （可废弃，统一用 trace_ids）
    })
}
```

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

## 十三、统一分页设计（独立需求，单独处理）

> 📌 **当前状态**：各 DAO 的 Query 结构体里都有零散的 `limit` 字段，但没有统一的分页设计。这个需求独立于唤醒逻辑，后续单独处理。

### 13.1 当前现状

| DAO | 分页相关字段 | 说明 |
|-----|-------------|------|
| MemoryQuery | `limit: Option<usize>` | 只有 limit，没有 offset |
| ToolQuery | `limit: Option<usize>` | 只有 limit |
| SkillQuery | `limit: Option<usize>` | 只有 limit |
| AgentQuery | ❌ 无 | 没有分页字段 |
| MessageQuery | ❌ 无 | 没有分页字段 |
| TaskQuery | ❌ 无 | 没有分页字段 |
| ProjectQuery | ❌ 无 | 没有分页字段 |

**问题：**
- 没有统一的分页结构体，各 DAO 自己加 `limit`
- 只有 limit 没有 offset，不支持翻页
- 没有总数统计（`total_count`）

---

### 13.2 统一分页设计方案（草稿）

**Option A：在 DAO 层加通用 Pagination 结构体**

```rust
// common/src/ 下定义通用分页参数
pub struct Pagination {
    pub offset: i64,
    pub limit: i64,
    pub order_by: Option<String>,
    pub order_desc: bool,
}

// 各 Query 结构体嵌入
pub struct MemoryQuery {
    // ... 原有字段
    pub pagination: Option<Pagination>,
}
```

**Option B：在 DAL 层加 `list_page` 方法**

```rust
// DAL 层统一返回分页结果
pub struct PageResult<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

#[async_trait]
pub trait MemoryDal: Send + Sync {
    // ... 原有方法
    async fn query_page(
        &self,
        ctx: RequestContext,
        query: MemoryQuery,
        pagination: Pagination,
    ) -> Result<PageResult<Memory>, AppError>;
}
```

**后续决策点：**
1. 选 Option A 还是 Option B？
2. 分页是 DAO 层概念还是 DAL 层概念？
3. `total_count` 怎么高效获取（COUNT 查询）？
4. 哪些场景需要真分页，哪些场景只要 limit 就够了？

---

## 十四、最终确认：第一阶段唤醒的完整设计

现在所有细节已对齐，第一阶段唤醒的完整设计已确定：

### 14.1 第一阶段范围（最小可用）

| 模块 | 修改内容 | 工作量 |
|------|---------|--------|
| `runtime/memory.rs` | 新增 `get_recent_traces` + `write_thinking_trace` 便捷方法 + `ThinkingTraceType` 枚举 | 30 min |
| `runtime/context_assembly.rs` | 新增纯函数模块，`build_conversation()` | 20 min |
| `runtime/awakening.rs` | 新增 trait + 实现 `awaken_for_user_message()` | 60 min |
| `runtime/mod.rs` | 注册新模块 + 导出 | 10 min |

**合计：约 2 小时**

### 14.2 6 步执行流程（最终版）

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
  3. 拼装 Prompt
     └─ ContextAssembly.build_conversation() → String
     ↓
  4. 记录输入 Trace
     └─ RuntimeMemory.write_thinking_trace(Input, prompt) → Memory
     ↓
  5. 模型推理
     └─ Agent.brain().cortex_trait().prompt() → String
     ↓
  6. 记录输出 Trace + 返回
     └─ RuntimeMemory.write_thinking_trace(Output, raw_output) → Memory
     └─ AwakeningResult
```

### 14.3 关键设计决策（不再变更）

✅ **Runtime Memory 只加便捷方法，不改核心接口**
✅ **ContextAssembly 纯函数，无副作用，可单独测试**
✅ **Awakening 按触发场景拆方法，不搞万能方法**
✅ **复用现有 Memory 表存 Trace，不建新表**
✅ **第一阶段只做纯文本，工具/技能第二阶段再加**

---

## 十五、变更记录

| 日期 | 版本 | 变更 |
|------|------|------|
| 2026-05-28 | v0.7 | 新增第十八章：记忆 Trace 闭环架构 + PO 自格式化重构完成；BrainDal 统一思考入口；简化 Trace 写入避免二次 IO；统一 Runtime 域内 Trace 写入路径；整体进度 ~90% |
| 2026-05-26 | v0.6 | 新增第十七章：架构重构 + 角色分工完成；对齐标准 Domain trait 组织方式；新增用户画像功能（客服专用）；消息链 ID 暴露给 Agent 自主读取；整体进度 ~75% |
| 2026-05-25 | v0.5 | 新增第十六章：简易版实现完成；核心逻辑全部可编译：Runtime Memory + Context Assembly (Builder) + Awakening 主流程 |
| 2026-05-25 | v0.4 | 新增第十二章：Runtime Memory 子模块行为对齐 + 第十三章：统一分页设计（独立需求） + 第十四章：第一阶段最终确认 |
| 2026-05-25 | v0.3 | 新增第十一章：可执行落地方案（第一阶段最小可用唤醒），包含能力盘点、6 步流程、4 个模块修改清单、开发顺序 |
| 2026-05-25 | v0.2 | 新增第九章：唤醒 Agent 操作的具体执行流程（12 步）、神经工具执行机制、ContextAssembly 拼装逻辑、AwakenOutcome 设计哲学 |
| 2026-05-25 | v0.1 | 初版草案，覆盖设计总纲与待拍板点 |


---

## 十六、简易版实现完成 ✅

**实现状态：** 全部编译通过

**已实现模块：**

| 模块 | 文件 | 核心功能 |
|------|------|---------|
| **Runtime Memory** | `src/service/domain/runtime/memory.rs` | 封装 Memory DAL，便捷读写思考 Trace；新增 `ThinkingTraceType` 枚举（Input/Output/ToolCall/ToolResult） |
| **Context Assembly** | `src/service/domain/runtime/context_assembly.rs` | Builder 模式 Prompt 拼装，`add_trace_id()` / `trace_ids()` 支持关联多个 Trace ID；便捷函数 `build_conversation_prompt()` 一键组装 |
| **Awakening Logic** | `src/service/domain/runtime/awakening.rs` | 唤醒主流程 9 步完整实现，`AwakeningCommand` / `AwakeningResult` 类型定义 |
| **Module Export** | `src/service/domain/runtime/mod.rs` | 统一导出所有类型和函数 |

**已落地的关键设计决策：**
- ✅ **输入/输出 Trace 共用同一 ID**：形成请求-响应关联对
- ✅ **Prompt 开头显示关联 Trace IDs**：Agent 可看到并引用历史 trace
- ✅ **Builder 模式组装 Prompt**：按需扩展，不破坏现有接口
- ✅ **完整 Message 支持**：解析消息 ID、发送者角色、消息类型、回复关联、任务/项目关联
- ✅ **双模式消息传入**：`current_message(&Message)` 完整模式 + `current_message_content(&str)` 便捷模式
- ✅ **第一阶段纯文本**：不引入工具/技能复杂度
- ✅ **向后兼容**：trace_ids 为空时不显示

**当前临时实现（待下一阶段接入真实 DAL）：**
1. 模型推理：直接返回固定字符串（避免依赖完整 Brain/Cortex 链路）

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

### 17.2 关键设计决策落地

#### 决策 1：消息链 ID 暴露给 Agent，自主决定是否读取
**❌ 旧方案：** Runtime 自动读取消息链完整内容，强制塞给 Agent

**✅ 新方案：**
- 只在 Prompt 中暴露 `reply_to_id`（已在 `current_message()` 中实现）
- Agent 看到 ID 后，自主决定是否需要回溯消息链
- 实际读取操作由 Agent 通过工具调用完成

**效果：** 降低 Runtime 复杂度，Agent 拥有完全的上下文控制权。

#### 决策 2：角色分工 - 只有客服 Agent 才拼接用户喜好
**❌ 旧方案：** 所有 Agent 都加载用户画像

**✅ 新方案：**
- 仅当 Agent 的 `roles` 字段包含 `customer_service` 或 `客服` 时
- 才在 Prompt 中添加【用户画像】区块
- 用户画像数据**由上层 Domain 传入**，Runtime 不负责拉取（分层原则）

**实现位置：** `awakening.rs` Step 3 拼装 Prompt

```rust
// 【角色分工】只有客服类 Agent 才需要拼接用户喜好等信息
let agent_roles = agent.po.get_roles();
if agent_roles.contains(&"customer_service".to_string())
    || agent_roles.contains(&"客服".to_string())
{
    // builder = builder.user_profile(user_profile_str);
}
```

#### 决策 3：Prompt 结构调整（新增用户画像区块）

```
【关联 Trace IDs】xxx

【Agent 人设】
xxx

【用户画像】         ← 新增区块（仅客服 Agent 显示）
xxx

【历史对话】
xxx

【当前消息】
xxx
```

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
| **P1** | 统计模块驱动的外部唤醒轮次 | Agent 收到 ToolCallResult 后可被消息机制再次唤醒；是否继续唤醒、还能唤醒几轮、是否暂停等待用户反馈，统一由统计模块的 task / agent / conversation 运行数据决定；最终用户答复仍由 Agent 在 Rig 回合内调用 `send_message` 等工具发出 |
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

## 下一步讨论方向

1. **统计模块驱动的外部唤醒轮次**（推荐 P1：让 ToolCallResult 触发下一次 Agent 唤醒，但轮次预算、暂停/继续和页面状态统一来自统计模块；必要时通过强类型 trace_ref 查询完整 ToolCallEntry）
2. **ToolCallResult attachment / artifact 产物化引用策略**（P1，仅当结果需要用户下载或成为 Project Artifact 时接入）
3. **Trace ID 关联链实现**（P1，完善 `message.reply_to_id` 追溯能力）
4. **streamable HTTP MCP runtime**（P2，继承 HTTP Tool SSRF/header/redirect 安全策略后再做）
5. **技能动态注入策略**（P2，Agent 能力扩展）

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
| 2026-07-08 | v0.8 | 新增第十九章：Agent 运行时状态管理完整设计；纯内存状态机、DashMap 并发安全、消费时校验、分层注入机制 |
| 2026-05-28 | v0.7 | 新增第十八章：记忆 Trace 闭环架构 + PO 自格式化重构完成；BrainDal 统一思考入口；简化 Trace 写入避免二次 IO；统一 Runtime 域内 Trace 写入路径；整体进度 ~93% |
| 2026-05-26 | v0.6 | 新增第十七章：架构重构 + 角色分工完成；对齐标准 Domain trait 组织方式；新增用户画像功能（客服专用）；消息链 ID 暴露给 Agent 自主读取；整体进度 ~86% |
| 2026-05-25 | v0.5 | 新增第十六章：简易版实现完成；核心逻辑全部可编译：Runtime Memory + Context Assembly (Builder) + Awakening 主流程 |
| 2026-05-25 | v0.4 | 新增第十二章：Runtime Memory 子模块行为对齐 + 第十三章：统一分页设计（独立需求） + 第十四章：第一阶段最终确认 |
| 2026-05-25 | v0.3 | 新增第十一章：可执行落地方案（第一阶段最小可用唤醒），包含能力盘点、6 步流程、4 个模块修改清单、开发顺序 |
| 2026-05-25 | v0.2 | 新增第九章：唤醒 Agent 操作的具体执行流程（12 步）、神经工具执行机制、ContextAssembly 拼装逻辑、AwakenOutcome 设计哲学 |
| 2026-05-25 | v0.1 | 初版草案，覆盖设计总纲与待拍板点 |


