# 意图感知两阶段唤醒设计

> 📦 归档标记（2026-08-16）：归档冻结。保留原因：intent_aware_two_stage_awaken_design 设计文档归档冻结，设计决策已沉淀至 wiki 长文。生效方案：见源码和 wiki 长文。

> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 整体分层架构
> - [runtime_design.md](./runtime_design.md) — Runtime 领域总纲与唤醒机制基础
> - [message_interaction_design.md](./message_interaction_design.md) — 用户-Agent 消息交互与前台调度
> - [thinking_task_policy_engine_design.md](./thinking_task_policy_engine_design.md) — 策略引擎（IA 场景专用 policy_group）
> - 【② Plan 落地快照】
>   - [唤醒上下文与睡眠约束.md](docs/archive/plan-archive/唤醒上下文与睡眠约束.md) — ThinkingOptions 统一参数 + PromptBuilder 两阶段方法内聚
> - 【③ Wiki 长文 ≥3 篇（Batch11 精确对齐）】
>   - [运行时领域.md](docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md) — awaken / analyze_input_intent 接口契约 + 两阶段入口
>   - [Runtime 领域编排.md](docs/wiki/zh/content/架构设计/分层架构设计/Domain%20层编排/Runtime%20领域编排.md) — 两阶段流程编排 + trait 签名
>   - [Agent 状态管理.md](docs/wiki/zh/content/项目概述/核心功能特性/Agent%20全生命周期管理/Agent%20状态管理.md) — Busy 状态内两阶段流程状态转换
> - 【④ RAG 原子知识卡（Batch11 精确对应 1 张 + 横向关联 2 张）】
>   - [Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2 正式执行串联](docs/wiki/knowledge/zh/Intent%20感知两阶段唤醒：IntentAnalyze%20Phase1%20七字段意图分析%20+%206%20级%20JSON%20降级兜底%20+%20Awaken%20Phase2%20正式执行串联/Intent%20感知两阶段唤醒：IntentAnalyze%20Phase1%20七字段意图分析%20+%206%20级%20JSON%20降级兜底%20+%20Awaken%20Phase2%20正式执行串联.md) — Phase1/2 串联总卡（IntentAnalysis 7 字段 + 6 级 JSON 降级 + 工具白名单 + think_loop 复用 + Prompt 仅供参考原则）
>   - [Agent 思考运行时 AgentThinkRuntime：挂载清理取消与每轮快照上报](docs/wiki/knowledge/zh/Agent%20思考运行时%20AgentThinkRuntime：挂载清理取消与每轮快照上报/Agent%20思考运行时%20AgentThinkRuntime：挂载清理取消与每轮快照上报.md) — Phase1 独立 think_runtime 注册清理机制
>   - [策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法](docs/wiki/knowledge/zh/策略引擎：Policy%20trait%20+%20PolicyGroup%20嵌套组合%20+%20policy_set!%20宏声明式写法/策略引擎：Policy%20trait%20+%20PolicyGroup%20嵌套组合%20+%20policy_set!%20宏声明式写法.md) — policy_set!(IntentAnalyze) 场景策略组配置

> 范围：只覆盖**总纲与核心理念、接口定义、数据流边界、Prompt 设计原则**；具体落地代码细节由 `docs/superpowers/plans/2026-08-14-intent-analyze-two-stage-awaken.md` 承接。

---

## 一、动机：为什么需要"先理解，再执行"

### 1.1 当前 awaken 流程的短板

现有单阶段唤醒流程（`2026-07-31` 引入的上下文注入版）：

```
外部消息（用户/系统/其他 Agent）
  → 查最近 20 条短期记忆作为历史对话
  → 拼装 Prompt（人设+技能+用户画像/项目/任务上下文+20条历史+当前消息原文）
  → run_think_loop 进入正式干活（所有工具可用）
```

这条链路在面对**短消息 / 对答式 / 讨论式 / 指代密集型**用户输入时有三个系统性问题：

| 问题 | 真实表现示例 | 根因 |
|------|------------|------|
| **没有显式意图识别** | 用户一句"我不这么觉得"承接 30 条历史之前的讨论，Agent 可能读最近 20 条没命中上下文就直接按"不同意当前观点"回复，完全错位 | 当前 Prompt 里没有任何"你先停下来想想用户这句话意图是什么"的指令约束 |
| **Agent 是否做语义检索完全靠自觉** | 用户一句"上次那个方案结果呢"跨任务/跨天，Agent 凭印象直接回答，不去调 `search_memory`，导致答偏或重复造轮子 | ① 调 search_memory 占一轮 think loop，Agent 倾向节省轮次不搜；② Agent 没有"我的记忆里有没有这个知识"的元认知；③ 消息越短越觉得简单越不搜 |
| **无法稳定处理指代消歧** | "他说的 X""改一下那个页面""就按之前定的来"这类依赖外部语境/常识的指代，系统层没有任何帮助 | 系统层完全靠 LLM 自己在单轮 Prompt + 20 条历史的范围内理解，没有机会"回头再查一下前面的历史/已沉淀的长期知识"再理解 |

### 1.2 为什么不做"硬编码工程化预检索"（方案 A 的放弃理由）

在头脑风暴阶段曾考虑方案 A：在 awaken 中新增一个 Rust 工程化模块，做「规则意图分类 → 正则关键词抽取 → 固定一次 search_memory」的预处理。放弃原因：

- 意图识别/指代消歧/关键词抽取**本身就是语义理解问题**，LLM 就是擅长这个的工具，我们用硬编码去模拟 LLM 的能力是"拿着锤子找钥匙"
- 工程化代码是**死的**：每次新增一种意图分类/一种搜索策略/一种指代模式，都要改 Rust 代码 + 补测试 + 走发版，迭代速度慢
- 规则覆盖度永远不够：行业黑话、倒装句、省略句、讽刺语气、多轮混合意图，规则根本写不完
- Agent 是**活的**：只要给它一套受约束的 Prompt + 工具白名单，它能在 1~2 轮 think loop 内完成理解，而且越复杂的场景优势越明显

### 1.3 设计哲学对齐 runtime_design 的"少即是多"

本文档的设计**严格遵守 runtime_design.md 三条核心原则**：

- **原则一（Runtime 只做唤醒，不做调度）**：我们不在 Runtime 内部新增意图分类状态机/调度循环；新增的 `IntentAnalyze` 场景仍然是"给 Agent 一次推理机会 + 一组神经工具 + 让它自己决定"，只是**思考目标和工具范围收窄了**
- **原则二（神经 vs 外骨骼二分）**：IntentAnalyze 阶段只允许神经级/记忆/检索类工具，不允许任何外骨骼级（发消息、改状态、做外部调用），和 Settle 场景只允许记忆类工具一样，是二分法的进一步细化
- **原则三（上下文极薄）**：IntentAnalyze 阶段的结果是给 Agent 自己参考的【输入理解结果】区块，不是"塞给它必须照做的事实"，最终判断仍由 Agent 在正式干活阶段自己定

---

## 二、总体方案：第四种 ThinkingScene「IntentAnalyze」+ 通用复用函数 + 两阶段 awaken

### 2.1 核心思路

**类比现有的 sleep_and_settle 流程**：
- 我们已经有了 `ThinkingScene::Settle`：`RuntimeAwakening::sleep_and_settle()` 内部调用 `run_think_loop`，但 Prompt 是专用的 `build_sleep_prompt`（约束只能做沉淀）+ 工具白名单只有 neural/memory

**对应用户消息理解场景**：
- 新增 `ThinkingScene::IntentAnalyze`：`RuntimeAwakening::analyze_input_intent()` 内部调用 `run_think_loop`，但 Prompt 是专用的 `build_intent_analyze_prompt`（约束只能做理解）+ 工具白名单是 neural/memory/query/search
- 产出结构化结果 `IntentAnalysis`，不做任何业务动作

**最后两阶段拼装**：
- `awaken()` 先跑 analyze_input_intent（阶段 1 理解），拿到 `IntentAnalysis`
- 判断是否需要澄清（need_clarification 非空时可选择直接发澄清消息，短路不进正式干活阶段）
- 把 IntentAnalysis 渲染成【输入理解结果】区块，拼到正式 awaken 的 Prompt 里（阶段 2 执行）

### 2.2 数据流总览

```
外部消息 → MessageConsumer
              ↓
   ┌───────────────────────────────────────────┐
   │ 阶段 1：IntentAnalyze 小循环                 │
   │  ThinkingScene = IntentAnalyze             │
   │  工具白名单：neural / memory / query / search │
   │  Prompt：build_intent_analyze_prompt()     │
   │    （SOP 5步走 + 结构化 JSON 输出约束）       │
   │  输出：IntentAnalysis {                    │
   │          intent_type, confidence,          │
   │          key_terms, resolutions,           │
   │          retrieved_context,                │
   │          need_clarification, summary       │
   │        }                                    │
   └──────────────────────┬────────────────────┘
                          ↓
             need_clarification 非空？
                ↙ Yes           ↘ No
   短路：send_message 追问用户     ↓
   返回，不进入阶段 2           【阶段 1.5】可选：
                          PromptBuilder.intent_analysis()
                          渲染【输入理解结果】区块
                                 ↓
                  ┌─────────────────────────────────────────┐
                  │ 阶段 2：Awaken 正常干活                  │
                  │  ThinkingScene = Awaken（所有工具可用）    │
                  │  Prompt = build() 新增【输入理解结果】区块 │
                  │  进入正常 think loop                     │
                  └─────────────────────────────────────────┘
```

### 2.3 与现有三种 ThinkingScene 的关系对比

| ThinkingScene | 触发者 | 主要目标 | 工具白名单 | 输出 |
|---------------|--------|---------|-----------|------|
| `Awaken` | 用户消息 / ToolCallResult / Agent 间协作消息 | 执行任务 + 产生最终用户回复 | 全部 | Agent 主动结束（mark_done）或轮次超限 |
| `Settle` | `sleep_and_settle` consumer（定时 agent_rest） | 沉淀短期记忆 → 长期知识图谱 | neural + memory | 最终 Final 就是沉淀完成的说明（不返回业务数据，直接用 Agent 内部副作用完成沉淀） |
| `Summary` | 轮次超限兜底 | 总结本轮思考 + 可选生成结果 + 标记退出 | neural + memory + messaging + project_management | 结构化 Final，退出 think loop |
| **`IntentAnalyze` 🆕** | 新增 `analyze_input_intent()`（awaken 前置阶段 / 外部入站消息预处理器） | 理解用户输入，不做执行 | neural + memory + query + search（严格不允许 messaging/project_management 等执行类） | 结构化 `IntentAnalysis`（解析自 Final JSON） |

---

## 三、接口定义（只定义骨架，不写实现）

### 3.1 ThinkingScene 扩展

**定义位置**：`src/service/domain/runtime/awakening.rs`（在现有 3 个变体基础上追加，与 2026-07-31 设计保持一致）

```rust
pub enum ThinkingScene {
    Awaken,
    Settle,
    Summary,
    /// 新增：意图识别 + 上下文补充阶段
    ///
    /// 思考目标：理解用户输入，产出结构化理解结果
    /// 工具约束：禁止任何执行类工具（消息发送/状态修改/外部调用）
    /// 最终输出：IntentAnalysis JSON
    IntentAnalyze,
}

impl ThinkingScene {
    pub fn is_tool_allowed(&self, tags: &[String]) -> bool {
        match self {
            ThinkingScene::Awaken => true,
            ThinkingScene::Settle => tags.iter().any(|t| t == "neural" || t == "memory"),
            ThinkingScene::Summary => tags.iter().any(|t| {
                t == "neural" || t == "memory" || t == "messaging" || t == "project_management"
            }),
            // 新增 IntentAnalyze 场景：
            // - neural：基础能力
            // - memory：读写短期/长期记忆、search_memory
            // - query / search：检索类工具（若未来给工具新增了 tags 分类则生效）
            // - **严格不包含** messaging / project_management / external_agent 等执行类
            ThinkingScene::IntentAnalyze => tags.iter().any(|t| {
                t == "neural" || t == "memory" || t == "query" || t == "search"
            }),
        }
    }
}
```

### 3.2 IntentAnalysis 结构体

**定义位置**：建议放在 `src/service/domain/runtime/awakening.rs`（与 ThinkingOptions / ThinkingScene 同文件，保持领域内聚）

```rust
/// 结构化意图分析结果
///
/// 由 analyze_input_intent() 输出，供：
/// - awaken 正式阶段的 PromptBuilder 渲染【输入理解结果】区块
/// - 外部入站适配器（飞书/WS/HTTP 回调）在路由消息前做预判断
/// - 澄清短路判断（need_clarification 非空时，不进入执行阶段直接追问用户）
#[derive(Debug, Clone, Default)]
pub struct IntentAnalysis {
    /// 主意图类型（推荐取值：Question / TaskRequest / Confirm /
    /// FollowUp / ClarificationResponse / Chat / Mixed）
    /// —— Agent 自主判断，不做强枚举，便于未来自然扩展
    pub intent_type: String,
    /// 意图置信度 0.0~1.0（Agent 自己打分）
    pub confidence: f32,
    /// 关键词/关键实体抽取（直接可用于 search_memory 复用）
    pub key_terms: Vec<String>,
    /// 指代消歧结果（自由文本数组，每条 Agent 写清楚"X → Y"）
    /// 例如：["\"上次那个方案\" → project=proj_123, task=task_456"]
    pub resolutions: Vec<String>,
    /// 检索到的补充上下文摘要（search_memory / recommend_seed_nodes /
    /// traverse_knowledge_graph 的结果，Agent 自己概括为短文本）
    pub retrieved_context: Vec<String>,
    /// 需要进一步追问澄清的问题（空列表表示理解充分）
    pub need_clarification: Vec<String>,
    /// 一句话总结：Agent 最终确认自己理解用户想要什么
    pub summary: String,
}
```

### 3.3 RuntimeAwakening trait 新增通用复用方法

**定义位置**：`src/service/domain/runtime/mod.rs` 的 `RuntimeAwakening` trait 上

```rust
#[async_trait]
pub trait RuntimeAwakening: Send + Sync {
    // 现有方法：
    async fn awaken(...) -> Result<...>;
    async fn wake_agent_brain(...) -> Result<...>;
    async fn sleep_and_settle(...) -> Result<...>;
    async fn summary(...) -> Result<...>;

    // 新增：专用输入分析函数
    //
    // 设计定位：可独立复用的"先理解"通用能力；
    // 典型调用方是 awaken 前置阶段，但也可用于：
    // - 外部消息入站（MessageAdapter）在路由前的预分析
    // - 澄清追问的重新理解
    // - Agent 间协作消息分发前的路由判断
    // - 前端用户发送消息前的本地预览意图
    async fn analyze_input_intent(
        &self,
        ctx: RequestContext,
        agent: &Agent,
        message: &Message,
        options: &ThinkingOptions,
    ) -> Result<IntentAnalysis>;
}
```

### 3.4 PromptBuilder 新增能力

**定义位置**：`src/models/prompt_builder.rs` 的 `PromptBuilder` trait

```rust
pub trait PromptBuilder {
    // 现有方法：
    fn system_prompt(&mut self, s: &str) -> &mut Self;
    fn skills(&mut self, s: &str) -> &mut Self;
    fn tool_failures(&mut self, s: &str) -> &mut Self;
    fn history(&mut self, messages: Vec<ChatMessage>) -> &mut Self;
    fn tools(&mut self, tools: Vec<ToolSpec>) -> &mut Self;
    fn project_context(&mut self, p: &Project) -> &mut Self;
    fn task_context(&mut self, t: &Task) -> &mut Self;
    fn user_profile(&mut self, u: &User) -> &mut Self;
    fn build(&self) -> Result<String>;
    fn build_sleep_prompt(&self, pending_memories_summary: &str) -> Result<String>;

    // 新增 1：阶段 1 专用 Prompt
    // —— 复用已挂载的人设/技能/历史/上下文，
    //    再拼意图识别 SOP + 输出 JSON schema 约束
    fn build_intent_analyze_prompt(&self) -> Result<String>;

    // 新增 2：阶段 2 的【输入理解结果】注入
    // —— build() 时把 IntentAnalysis 渲染成"供你参考"的结构化区块，
    //    放在【当前消息】之前
    fn intent_analysis(&mut self, analysis: &IntentAnalysis) -> &mut Self;
}
```

---

## 四、Prompt 设计原则（最重要的"活的灵魂"）

### 4.1 阶段 1：build_intent_analyze_prompt 的约束结构

Prompt 结构必须**强约束 + 强引导**，确保 Agent 不跑偏去执行任务：

```
【系统人设】（复用 build() 里已经挂好的人设）

【技能装载】（neural + memory + query/search 标签范围内的技能，复用已装载）

【上下文】（用户画像 + 项目 + 任务，复用 ThinkingOptions 注入；外加最近 20 条历史）

===== IntentAnalyze 场景约束（本阶段专属指令块）=====

## 你的任务：只做理解，不做执行（非常重要！）
你当前处于【输入理解阶段】，这是正式干活前的"审题环节"。本阶段你：
✅ 必须做：
   1. 按 SOP 五步识别用户意图（见下节）
   2. 调用一次 search_memory（或 recommend_seed_nodes 空白场景）做上下文补充
   3. 输出严格的 JSON 结构化结果（最终 Final 必须是纯 JSON）
❌ 严格禁止做：
   - 禁止调用 send_message / send_task_assignment_message（不准给任何用户/Agent 发消息）
   - 禁止调用 create_task / update_task / create_project 等执行类工具
   - 禁止做任何外部调用、修改任何系统状态
   - 禁止直接回答用户问题（哪怕你 100% 知道答案，也必须输出 JSON 后等下一阶段再回答）

## 理解 SOP（五步走，严格按顺序在你的思考中执行）

### Step 1：意图识别
在思考中先把【当前消息】归类，写出你判断的依据：
- Question：提问型（要信息/问进度/问规则/请教）
- TaskRequest：任务型（提需求/安排工作/要产出）
- Confirm：确认型（同意/否定/选择/拍板）
- FollowUp：追问型（承接之前某条回答/产出的继续追问）
- ClarificationResponse：澄清响应型（针对前面追问给出的答复）
- Chat：闲聊型（打招呼/客套/社交礼貌）
- Mixed：混合型（同时含多个意图，在解析中拆分说明）
意图类型在 intent_type 字段填，置信度 0.0~1.0 自己打分。

### Step 2：指代与上下文消歧
1. 仔细读【历史对话】+【项目/任务上下文】+【用户画像】
2. 找【当前消息】中的"指代短语"：这/那/上次/那个/他/按之前定的来 等
3. 在思考中把每个指代对应到具体对象（project_id/task_id/message_id/某个人物…），写进 resolutions 数组
4. 如果读完所有上下文仍无法确定对应对象 → 写进 need_clarification（下一步不要硬猜）

### Step 3：关键词抽取
从【当前消息】+ 已消歧结果中，抽取 3~8 个关键词/关键实体
（项目名、任务名、产品名、人名、专有名词、核心动词短语），写进 key_terms 数组。

### Step 4：语义检索补充
- 必须调用一次 search_memory（用 Step 3 的关键词组合成 query），
  除非你 100% 确认这是完全无历史的全新话题。
- 首次进入这个 project/task 空场景时：先调用 recommend_seed_nodes 拿图谱起点，
  再按需 traverse_knowledge_graph 走 1~2 跳。
- 把检索命中的高相关内容，**自己概括为短摘要**（不要直接贴原始 JSON），
  每条一行写进 retrieved_context。

### Step 5：判断是否需要澄清 + 总结
- 如果 Step 2 消歧失败 / 混合型意图优先级不清 / 需求边界不明 →
  把要问用户的问题逐条写进 need_clarification。
- 如果理解充分 → need_clarification = []。
- 最后在思考中用一句话总结"我理解用户想要：XXX"，写进 summary 字段。

## 最终输出规范（必须遵守）
你输出的最终 Final 内容**只能是一个 JSON 对象**（不含任何 ```json ``` 包裹或其他文本），严格符合以下 schema：

{
  "intent_type": 字符串（必须是 Step 1 列表中的取值）,
  "confidence": 数字 0.0~1.0,
  "key_terms": [字符串数组],
  "resolutions": [字符串数组],
  "retrieved_context": [字符串数组],
  "need_clarification": [字符串数组],
  "summary": 字符串（一句话总结你理解的用户需求）
}

===== IntentAnalyze 场景指令块结束 =====

【当前消息】（原文直接贴）
{message.content}

现在开始：先在思考中走完 Step 1~5，再输出最终 JSON。
```

### 4.2 阶段 2：【输入理解结果】区块的渲染风格（"供参考"姿态）

`DefaultPromptBuilder.intent_analysis()` → `build()` 时渲染成如下格式：

```
【输入理解结果（由你在前一阶段 IntentAnalyze 得出，仅供你参考）】
说明：以下内容是你自己在上一阶段"审题阶段"得出的理解摘要，
供正式执行时参考。如果发现与你的重新判断不一致，请以你当下的理解为准。

1. 意图类型：TaskRequest（置信度 0.82）
2. 指代消歧：
   - "上次那个方案" → project=proj_123, task=task_456
3. 关键词：方案、项目X、排期、结论
4. 检索补充上下文（你上一步自己搜索概括的）：
   - [短期记忆 stm_abc] 2026-08-10 方案 A/B 比较结论：推荐方案 A（相似度 0.88）
   - [长期图谱 lkn_def] 项目 X 方案排期节点 → task_456 完成度 80%
5. 是否需要澄清：不需要（need_clarification 为空）
6. 一句话理解总结：用户想知道项目 X 中之前推荐的方案 A 当前的推进进度与结果

===== 以上理解仅供参考 =====

【历史对话】
（最近 20 条）

【当前消息】
上次那个方案结果呢？

请你根据以上上下文，正式开始执行：思考 → 调工具 → 回复/行动。
```

**设计原则**：
- 反复强调"这是你上一阶段自己得出的结论，不一致就可以推翻"，避免 Agent 被错误的前置理解带偏
- 只展示摘要，不展示原始检索 JSON（避免 Prompt 太长）
- `need_clarification` 非空时渲染成更醒目的"💡 上一阶段判断需要向用户澄清：..."，给 Agent 更明确的信号"先问清楚再干"

---

## 五、边界与非目标（非常重要）

### 5.1 明确做什么 / 不做什么

| 做 ✅ | 不做 ❌ |
|------|--------|
| 新增 `IntentAnalyze` 场景 + 工具白名单 | 不修改现有 Awaken/Settle/Summary 场景的工具白名单 |
| 新增 `IntentAnalysis` 结构体 + 通用复用函数 | 不改变现有 `ThinkingOptions` / `awaken()` / `sleep_and_settle()` 的对外签名除了必要的场景扩展 |
| 新增 `build_intent_analyze_prompt`（完全对称于 build_sleep_prompt） | 不在 build_sleep_prompt / build() 里加新的业务区块 |
| `awaken()` 改两阶段（新增 intent_analysis 区块注入） | 不实现硬编码意图分类器/关键词匹配器/共指消解算法 |
| Prompt 层强制「必须调一次 search_memory」约束 | 不强迫每次 100% 命中（没搜到就是 retrieved_context 为空，正常进入阶段 2）|
| need_clarification 非空时可选短路 | 不修改 Consumer/Producer 流程；短路 send_message 与正常 Agent 调 send_message 走同一通道 |

### 5.2 降级与兜底策略

| 故障场景 | 降级方案 |
|---------|---------|
| Final JSON 解析失败（Agent 输出格式不对） | IntentAnalysis 退化为默认空结构，只把 summary 字段填成 Agent 输出的原文本（不阻塞主流程），正式阶段无增强区块，等价于当前单阶段流程 |
| run_think_loop 在 IntentAnalyze 阶段超限/抛错 | Result<IntentAnalysis> 捕获错误，记 warn 日志，awaken 正常进入阶段 2（无理解增强） |
| retrieved_context 太长超出 token 预算 | 在 DefaultPromptBuilder.intent_analysis 渲染逻辑中截断（最多 10 条，每条最多 200 字），剩余省略 |
| need_clarification 判断过多（Agent 动不动就问用户） | 第一阶段不上短路机制，只渲染提示让 Agent 自己决定何时问，避免过度打扰用户；等验证稳定后再开短路 |

### 5.3 方案 B 与方案 A+ 的分工边界（避免重复）

用户提的方案 B（在 TEMPLATE_COMMUNICATION 技能里新增「理解用户消息 SOP」整章）和方案 A+（本文档的 IntentAnalyze 场景）是**互补关系**，职责分工明确：

| 方案 | 性质 | 作用域 | 强制性 |
|------|------|--------|--------|
| **方案 B（技能 SOP）** | 长期技能文档 | 每轮 Agent 思考时都能看到的元方法论 | 弱约束：Agent 可以看了不执行 |
| **方案 A+（IntentAnalyze 场景）** | 系统级思考场景 + Prompt 强约束 | 仅在阶段 1 生效，执行完就退出 | 强约束：工具白名单 + 输出 JSON schema，不按规定做 Final 就是错的，降级兜底 |

**重复内容的处理原则**：
- 方案 B 的 SOP 写得**更通用**（适用于任何场景下 Agent 理解用户消息）
- 方案 A+ 的 build_intent_analyze_prompt 把 SOP **重新组织得更短、更具操作性**（5 步走 + 必须调 search_memory + 必须输出 JSON），并强化"不准执行"的禁令
- 两者 SOP 的核心逻辑保持一致，不让 Agent 困惑

---

## 六、与其他模块的协同关系

| 模块 | 协同方式 |
|------|---------|
| 四层记忆系统 | 阶段 1 唯一允许真正"动"的模块：search_memory / recommend_seed_nodes / traverse_knowledge_graph 是主要调用对象 |
| 协作沟通技能（方案 B） | 技能层提供通用 SOP；IntentAnalyze Prompt 把 SOP 改造成强约束执行版 |
| 用户画像双源沉淀 | 阶段 1 拼入用户画像后，Agent 在意图识别时可以参考（例如已知用户偏好简短回复 → need_clarification 不要写太多问题）|
| 消息消费者 MessageConsumer | 是 awaken 两阶段的主要调用方；未来 MessageAdapter（外部入站）可在路由前直接调用 analyze_input_intent() 做消息预路由 |
| 用户身份凭证中枢 / 外部 Agent / 工具执行 | 全部严格不允许出现在阶段 1，避免理解阶段产生副作用 |

---

## 七、分阶段落地节奏建议

为控制风险，建议四步走，每一步都独立可验证、可回滚：

| 阶段 | 产物 | 验证方式 |
|------|------|---------|
| **P0 方案 B** | TEMPLATE_COMMUNICATION 技能新增「理解用户消息 SOP」一章 | 本地发几条短消息观察 Agent 是否开始做意图分类 + 主动 search_memory |
| **P1 A+ 骨架** | ThinkingScene 新增 IntentAnalyze 变体 + IntentAnalysis 结构体 + RuntimeAwakening trait 新增 analyze_input_intent 方法签名 + build_intent_analyze_prompt 方法签名 | clippy + 编译通过 |
| **P2 A+ 方法实现 + 独立可调用** | analyze_input_intent 内部跑通 run_think_loop；DefaultPromptBuilder 实现 build_intent_analyze_prompt；对外可用但 awaken 先不串起来 | 单元/集成测试：构造一条典型指代消息，调用 analyze_input_intent()，断言 output.resolutions / output.key_terms 非空 |
| **P3 awaken 两阶段串联** | awaken() 内部先跑 P1；DefaultPromptBuilder 新增 intent_analysis() + 渲染；need_clarification 先不上短路 | 人工观察：正式 Prompt 中出现【输入理解结果】区块，内容与上一阶段 JSON 一致 |
| **P4（可选）短路澄清** | need_clarification 非空时直接 send_message 给用户并退出 awaken | 人工验证：消歧失败场景下 Agent 先发追问消息，不进入正式干活阶段 |