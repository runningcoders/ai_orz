# Runtime Domain 设计

> 🎯 **本文档定位**：Runtime Domain（运行时领域）的整体设计大纲与逻辑思路
>
> 范围：只覆盖**总纲与核心理念**，不下沉到具体工具实现与代码细节
> 状态：草案 v0.1（2026-05-25）
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

每项都是 rig 原生 `Tool`，模型推理回合内可直接同步调用：

| 工具名 | 作用 | 委托给 |
|--------|------|--------|
| `search_skill` | "想起"有哪些相关技能 | `SkillDal` |
| `read_skill` | 取出某个技能的具体内容 | `SkillDal` |
| `search_memory` | 混合搜索长期/短期记忆 | `RuntimeMemory.search` |
| `write_memory` | 沉淀新记忆 | `RuntimeMemory.write` |
| `list_tools` | "想起"有哪些外骨骼工具可用（仅返回名字+一句话） | `ToolDal` |
| `read_tool_spec` | 展开某个工具的完整 schema | `ToolDal` |
| `send_message` | 给 user / agent / channel 发消息 | `MessageDomain.delivery` |
| `request_tool_call` | 异步发起一次外骨骼工具调用 | `tool_execution` |
| `mark_done` | 显式标记本任务完成 | Runtime 内部 |

**设计要点**：
- `list_*` 系列只返回**摘要**（名字+一句话），不含完整 schema，避免 prompt 膨胀
- 真正要用某项能力时，模型主动调 `read_*` 系列展开
- 这正是"想起来"的过程——模型先看到目录，再决定翻哪一页

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

## 五、待拍板的设计决策

以下是本设计中**尚未拍板**的关键问题，需要决策后再进入实现。每项给出我的倾向，但等用户最终确认。

### Q1：单回合 vs 多回合循环

- **倾向**：Runtime 内部只跑**单回合**，多回合靠"消息触发再次唤醒"
- **理由**：贴近"无复杂循环"原则；rig auto 已支持回合内多次工具调用，足够覆盖大多数场景
- **风险**：极端长链路任务需要更多消息往返，但换来的是可控性和可观测性

### Q2：rig 回合内多步 tool calling 算不算"循环"

- **倾向**：算"模型自主行为"，不算我们写的循环，**允许**
- **理由**：符合"模型自己决定"的原则，框架不应剥夺这个能力

### Q3：基础元能力放神经工具还是种子技能

| 选项 | 描述 | 倾向 |
|------|------|------|
| A | 作为**神经工具**硬编码在 `neural_tools.rs`，每个 Agent 默认都有 | ⭐ 倾向 |
| B | 作为**内置技能**写入 skill 库，Agent 入职时自动 install | |

- **理由**：神经工具是 Agent 的**天生能力**（就像人天生会说话、会回忆），不该用"技能"这层抽象套
- **避免循环依赖**：若 `find_skill` 本身是技能，会出现"得查技能才知道怎么查技能"的悖论
- B 选项留给"在 X 情况下发什么消息"这类**程序性知识**，那才是真正的 skill

### Q4：ContextAssembly 放 Runtime 还是单独抽 pkg

- **倾向**：放 `runtime/context_assembly.rs`
- **理由**：上下文拼装本身就是 Runtime 的职责；如果未来别的 Domain 也要拼，再下沉到 pkg

### Q5：Trigger 类型枚举

- **倾向**：`UserMessage / AgentMessage / ToolResult / Scheduled / Manual` 五类
- **理由**：覆盖目前可预见的所有唤醒场景，每种 trigger 都能在 system_prompt 里加一段定制开场

### Q6：finished 信号怎么定

| 选项 | 描述 | 倾向 |
|------|------|------|
| A | 约定一个特殊工具 `mark_done()`，模型显式调即结束 | ⭐ 倾向 |
| B | 模型本回合不再调任何"对外消息工具"就视为结束 | |

- **理由**：显式 > 隐式，便于追踪和审计；用户能清楚看到 Agent 自己判定"任务完成"的时点

---

## 六、实现路线图

按"小步推进、每步可编译可测"的原则，分六步落地：

| 步骤 | 内容 | 产出 | 验收 |
|------|------|------|------|
| **1** | 定义 `Awakening` trait + `AwakenCommand/Outcome` 数据结构 + 空实现（返回 `Err(Unimplemented)`） | `awakening.rs` 骨架、mod.rs 单例注册 | `cargo check` 通过 |
| **2** | 实现 `ContextAssembly` 纯函数：拼 system_prompt + recent_traces（先不接神经工具） | `context_assembly.rs` + 单测 | 纯函数单测通过 |
| **3** | 实现 `NeuralTools` 最小集：`search_memory` / `write_memory` / `search_skill`，rig Tool trait 适配 | `neural_tools.rs` + 单测 | 工具单独 invoke 通过 |
| **4** | 接入 Cortex（模型推理）到 `Awakening`，跑通"用户消息进 → 模型回 → 落 trace"最小闭环 | 端到端最小可用 | 集成测试通过 |
| **5** | 加入 `send_message` + `request_tool_call` 神经工具，打通消息通道和外骨骼通道 | 完整双通道 | Agent 可对话 + 调外骨骼工具 |
| **6** | 补齐展开式工具：`list_tools` / `read_skill` / `read_tool_spec` / `mark_done` | 完整神经工具集 | Agent 行为完整 |

**每步交付物**：
- 代码 + 内联单测（项目惯例）
- `cargo check --workspace` 通过
- `cargo test --workspace` 通过
- 涉及架构变化时同步更新本文档

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

## 八、变更记录

| 日期 | 版本 | 变更 |
|------|------|------|
| 2026-05-25 | v0.1 | 初版草案，覆盖设计总纲与待拍板点 |

