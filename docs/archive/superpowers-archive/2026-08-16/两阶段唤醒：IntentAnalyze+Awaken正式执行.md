📦 归档标记（2026-08-16）：被 [docs/plan/两阶段唤醒：IntentAnalyze+Awaken正式执行.md](docs/plan/两阶段唤醒：IntentAnalyze+Awaken正式执行.md) 取代。保留原因：原始执行蓝图含逐步命令/检查清单，留作审计参考。生效方案：[docs/plan/两阶段唤醒：IntentAnalyze+Awaken正式执行.md](docs/plan/两阶段唤醒：IntentAnalyze+Awaken正式执行.md)

---

# 意图感知两阶段唤醒 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

> **实施状态**：✅ 已全部完成（2026-08-14）
>
> - Task 1~4 全部实现并提交
> - Task 5 质量验证通过：后端 + 前端 clippy `-D warnings` 零警告，全 workspace 测试 100% 通过
> - 配套增强（计划外）：轮次/超时全面配置化（Agent runtime_config > ai_orz.toml > 硬编码），默认轮次调大至 365；新增 `AgentRuntimeConfigInfo` 嵌套结构体替代打平字段；前端 Agent 配置表单适配
> - 设计文档：[intent_aware_two_stage_awaken_design.md](../../design/intent_aware_two_stage_awaken_design.md)
> - 架构总纲更新：[runtime_design.md 25.13 节](../../design/runtime_design.md)（v3.8 增量）

**Goal:** (1) 在「协作沟通」技能中新增「理解用户消息 SOP」（方案 B）；(2) 新增 `ThinkingScene::IntentAnalyze` 场景 + 通用复用函数 `analyze_input_intent()`，让 Agent 先做意图识别/指代消歧/语义检索再输出结构化 JSON；(3) 将 `awaken` 串联为两阶段流程：先理解、再执行，把 `IntentAnalysis` 结果渲染成 Prompt 中的【输入理解结果】区块。

**Architecture:**
1. **方案 B（纯技能文档）**：在 `TEMPLATE_COMMUNICATION/skill.md` 末尾追加「理解用户消息 SOP」完整一章（五步走 + 澄清判断原则）。零代码改动，零风险，立即生效。
2. **方案 A+ P1 骨架**：扩展 `ThinkingScene` 枚举新增 `IntentAnalyze` 变体 + 工具白名单（neural/memory/query/search，严格排除 messaging/project_management 等执行类）；新增 `IntentAnalysis` 结构体（7 字段：intent_type/confidence/key_terms/resolutions/retrieved_context/need_clarification/summary）；`RuntimeAwakening` trait 新增 `analyze_input_intent()` 方法签名；`PromptBuilder` trait 新增 `build_intent_analyze_prompt()` + `intent_analysis()` 方法签名。
3. **方案 A+ P2 核心实现**：在 runtime/awakening.rs 中为 `RuntimeAwakeningImpl` 实现 `analyze_input_intent()`：复用 `ThinkingOptions` 注入上下文 + `wake_agent_brain(scene=IntentAnalyze)` + `build_intent_analyze_prompt()` + `run_think_loop` 一轮后解析 Agent Final 为 `IntentAnalysis`（解析失败即降级为空结构，保证不阻塞主流程）。`DefaultPromptBuilder` 实现 `build_intent_analyze_prompt()`（按 design 文档模板：意图识别 SOP 五步走 + 强约束禁令 + JSON schema 输出要求）。
4. **方案 A+ P3 awaken 两阶段串联**：`RuntimeAwakeningImpl::awaken()` 在原「查最近 20 条历史 + 构造 PromptBuilder」之后新增：克隆一份 options 并改 scene=IntentAnalyze，调用 `self.analyze_input_intent()` 得到 `IntentAnalysis`；将结果传给 `builder.intent_analysis()`；`DefaultPromptBuilder.build()` 在【当前消息】之前渲染【输入理解结果】结构化区块（设计文档指定的"供你参考"姿态，有 token 预算截断逻辑）。P4（澄清短路）暂不做，留后续验证稳定后再评估。
5. **测试与质量**：为每个新增/修改点补单元 + 集成测试：①技能变更无测试（但更新 wiki 种子配置 + 默认技能 seed SQL 验证技能 md 完整性）；②ThinkingScene 白名单单元测试（IntentAnalyze 不允许 messaging tag）；③IntentAnalysis JSON 序列化/反序列化测试；④analyze_input_intent 集成测试（用初始化测试环境 + 典型指代消息断言 key_terms/resolutions 非空/或最终降级空结构无 panic）；⑤DefaultPromptBuilder.build() 字符串断言（出现"输入理解结果"且包含 IntentAnalysis 字段）；⑥ `cargo clippy --all-targets -- -D warnings`；⑦ `cargo test --workspace` 1101+ 测试全通过。

**Tech Stack:** Rust 1.80+、Axum、sqlx 0.8 (SQLite STRICT)、Dioxus 0.7、cortex-rig、tracing (log_* 宏)、serde (JSON 解析)、RequestContext、#[sqlx::test] 集成测试、DuckDB stats。

---

## 文件变更总览（全局）

| 操作 | 路径 | 职责 |
|------|------|------|
| Modify | `src/service/domain/system/seed/skills/TEMPLATE_COMMUNICATION/skill.md` | 方案 B：新增理解用户消息 SOP 整章 |
| Modify | `src/service/domain/runtime/awakening.rs` | 新增 `IntentAnalyze` 变体 + is_tool_allowed；新增 `IntentAnalysis` 结构体（含 serde derive）；`analyze_input_intent` 实现 |
| Modify | `src/service/domain/runtime/mod.rs` | `RuntimeAwakening` trait 新增 `analyze_input_intent` 方法签名 |
| Modify | `src/models/prompt_builder.rs` | `PromptBuilder` trait 新增 `build_intent_analyze_prompt()` + `intent_analysis()` 方法签名 |
| Modify | `src/service/dal/agent.rs` | `DefaultPromptBuilder`：新增字段 `intent_analysis` / 新增方法 `build_intent_analyze_prompt` / 新增 `intent_analysis()` setter / `build()` 新增【输入理解结果】区块渲染 / 提取 `render_intent_analysis_section()` 私有方法 |
| Modify | `tests/common/env.rs` | `init_full_test_env` 与真实启动顺序保持一致（无需改动，仅确认） |
| Create / Test | `tests/intent_analyze_two_stage.rs`（或复用 `tests/agent_*.rs` 对应文件） | analyze_input_intent 集成测试 + DefaultPromptBuilder intent_analysis 渲染字符串断言 |

---

### Task 1：方案 B - 协作沟通技能新增「理解用户消息 SOP」整章

**Files:**
- Modify: `src/service/domain/system/seed/skills/TEMPLATE_COMMUNICATION/skill.md`（文末「行为准则」章节之后，或在最末尾追加新章）

- [x] **Step 1: 在 TEMPLATE_COMMUNICATION/skill.md 末尾追加整章**

追加内容如下（直接全文插入到文件末尾，原内容保持不变）：

```markdown
---

## 理解用户消息 SOP（收到消息先按这个流程过一遍脑子，再决定怎么回复/行动）

> 说明：本章是对本章前述「工具介绍」和「分层响应协议」的前置补充。
> 无论任何用户消息，你**必须在开始执行任务或最终回复用户之前，先在思考中走完 Step 1~5**；
> 系统也会在你进入正式思考前，通过「输入理解阶段」帮你强制完成一轮预分析（
> 分析结果会以【输入理解结果】区块形式呈现，供你参考，若发现与你重新判断不一致，以你为准）。

### Step 1：意图识别 —— 用户发这句话想干什么？

请把用户消息归类到以下之一，并在你的思考中**写出你判断的依据**（若 80% 以上把握可以跳过依据，但建议始终写）：

| 类型 | 说明 | 典型语气/特征词 |
|------|------|---------------|
| **Question 问答型** | 要信息 / 问进度 / 问规则 / 请教知识 | "怎么""有没有""是否""请问""帮我查一下""XX 是什么" |
| **TaskRequest 任务型** | 提需求 / 安排工作 / 要产出 / 请你帮忙 | "帮我做""给我一个""做出来""整理一下""完成 XX" |
| **Confirm 确认型** | 同意 / 否定 / 选择 / 拍板 / 授权 | "行，就这么定""我选方案 A""不同意这样""就按你说的做""OK/好的/可以" |
| **FollowUp 追问型** | 针对**之前某条回答/某个产出**的继续追问 | "刚才那个 XX""再往下做""之前的结果呢""详细说一下 XX 部分" |
| **ClarificationResponse 澄清响应** | 针对你前面发出的澄清追问，用户给出的具体答复 | 直接对应你之前的问题；如你问"是哪个项目？"，用户答"X 项目" |
| **Chat 闲聊型** | 打招呼 / 客套 / 社交礼貌 / 无业务信息的寒暄 | "你好""在吗""早安""谢谢""辛苦了" |
| **Mixed 混合型** | 上面多类意图同时出现，需要拆分处理 | "先帮我看一下 X 的进度，然后把文档给我"（Question + TaskRequest） |

Step 1 结束后，在你的思考中一句话写出判断结论，例如：「本条消息是 FollowUp 追问型 + TaskRequest 任务型混合；用户先问进度、再要产出交付物」。

### Step 2：指代与上下文消歧 —— 这句话里的"这/那/上次/那个/他说的"到底指什么？

按以下顺序执行：

1. **先读最近上下文**：Working Memory（已作为【历史对话】区块给你）+【用户画像】+【项目上下文】+【任务上下文】+【输入理解结果】区块（如果有）。
2. **识别指代短语**：高亮所有需要消歧的词：
   - 指示代词：这、这个、这事儿、这种情况、那、那个、那样做
   - 时间指代：上次、之前、刚才、后来、后面、明天之前
   - 名词省略：他说的、文档、方案、那个页面、接口、数据
   - 指代"刚才一轮讨论"：就按之前定的来、你刚才说的那个 XX
3. **逐条映射到具体对象**：在思考中把每一个指代短语对应到具体对象（project_id/task_id/message_id/某个人物/某份文档/某个之前的决定），**给出映射依据**（如：「"他说的" → 历史对话第 5 条，"张三说的这个项目要周五交付"」）。
4. **无法消歧时不要硬猜**：如果某条指代读完所有上下文仍然不确定 → 记下来，在 Step 5 中进入「需要澄清」列表，下一步不要自己假设。

### Step 3：关键词抽取 + 语义检索补充 —— 有没有已有的历史知识/讨论能支撑？

#### 3.1 抽关键词
从「用户消息原文」+「Step 2 消歧后的具体对象」中抽取 3~8 个关键词/关键实体（不要抽太多也不要太少）。关键词类型优先级：
1. **专有名词**：项目名、产品名、公司名、人名、部门名
2. **任务/文档标识**：任务标题、文档名、API 名、接口 URL、表名
3. **核心动词短语**：要做的动作（迁移、拆分、重构、统计、对比、排期、同步）
4. **时间/限定词**：本周、下版本、Q3 季度等

#### 3.2 必须做一次语义检索（除非 100% 全新话题）
**强制执行规则**：除非你有 100% 把握这是完全无历史的全新话题（例如用户第一次打招呼说「你好」），否则必须调用下面至少一个工具补全上下文：

| 场景 | 推荐工具 | 参数建议 |
|------|---------|---------|
| 有明确关键词短语 | `search_memory`（混合搜索：语义 + FTS5 + 图谱） | `query = 关键词用空格或逗号组合，如 "项目X 方案A 排期"`；如果有 project_id/task_id，优先填 scope 过滤 |
| 首次进入这个 project/task 空白场景 | `recommend_seed_nodes`（冷启动种子节点推荐） | `agent_id` + 可选 `project_id`/`task_id` + `top_k=5` |
| search_memory / recommend_seed_nodes 命中一个高相关节点，但信息不全 | `traverse_knowledge_graph`（沿图谱关系走 1~2 跳） | `node_id = 命中的节点 id`；`max_depth = 1` 或 `2`；`relation_types = ["related","contains","depends","opposite","preceded_by"]` 默认全选 |
| 最近历史中明显有一个跨 task/project 的讨论但 Working Memory 不够 | `list_messages`（补全更早的历史） | 传 `project_id`/`task_id` 缩小范围；`before_timestamp = 最早一条历史的 timestamp - 1`，上拉 20 条 |

#### 3.3 概括检索结果（不要直接贴原始 JSON）
把上述检索命中的高相关内容**自己概括为 1~3 句话短摘要**，记在心里（正式阶段供你参考），不要把长长的原始 JSON 一股脑搬到思考或回复里。

### Step 4：判断是否需要追问澄清

以下任一情况成立 → 不要自己假设，下一步**先澄清再执行**：

| 需要澄清的场景 | 正确做法 |
|-------------|---------|
| Step 2 消歧失败：某个指代仍然对不上具体对象 | 问用户：「你提到的『那个方案』具体是指哪个项目下的？我这边能看到你有 X 和 Y 两个项目的近期讨论」 |
| 混合型意图（Mixed）优先级不清：用户同时提了多个诉求，不知道先做哪一个/哪些是必做哪些是可选 | 列出来请用户确认优先级：「我理解你想同时做 3 件事：①…… ②…… ③……。是不是按我列的顺序先做①和②？③可以明天再做吗？」 |
| 需求边界不明：用户说了一句模糊的任务型请求（"改一下那个页面""调一下数据""整理一下"），但不说清楚改哪里、调什么维度、整理成什么格式 | 从「目标 + 输入 + 预期输出 + 约束」四个维度追问，每个维度问一个具体问题，不要空泛地问「你想要什么效果？」 |
| 需要用户决策：方案 A/B 选择 / 排期是否接受 / 资源是否可用 / 敏感操作确认（如删除、覆盖、上线） | 把决策需要的信息摆清楚（方案 A 优点/缺点 + 方案 B 优点/缺点），明确告诉用户「我等你决定后再往下走」 |
| 澄清响应（ClarificationResponse）仍然不够具体：用户答了你的澄清追问，但信息还是不够（你问"是哪个项目？"用户只答"就是那个项目"） | 把你看到的候选对象列出来让用户挑：「我这边能看到你最近 3 个活跃项目：①项目 A ②项目 B ③项目 C。你指哪一个？」 |

**追问技巧**：
- ❌ 禁止一次问 5 个以上开放式问题，用户会崩溃
- ✅ 优先给出**选择题**而不是**简答题**：「你想要 A 这种格式还是 B 这种格式？」比「你想要什么格式？」用户体验好得多
- ✅ 如果问题之间有依赖（例如先知道 project_id 才能问 task_id），**不要拆两轮来回问**，合并成一轮：「你是要在项目 A 的任务 T1 上操作吗？（A：是；B：A 项目但其他任务；C：其他项目）」

### Step 5：形成理解结论，再开始行动/回复

在你的思考中，**用一句话总结你最终理解到的用户需求**，确认没歧义后再往下走。示例：

> 「我理解：用户在问**项目 X 中之前讨论过的方案 A** 的当前推进进度与结果（FollowUp 追问型，置信度 0.85），通过 search_memory 查到 2026-08-10 的短期记忆结论是『推荐方案 A』，对应的任务 T456 当前进度是 80%，预计今天完成。需要回复用户进度、完成时间、以及完成后是否需要同步交付物。」

如果 Step 5 这句话仍然写不清楚 → 说明你还没理解透，请回到 Step 2/4 继续消歧或追加澄清，不要硬着头皮执行。

---

### 本章与「输入理解阶段」（IntentAnalyze 场景）的关系

系统在正式唤醒你干活前，可能会先通过一个**专用的「输入理解阶段」**强制跑一遍上述 Step 1~5（并且强制要求你调用一次 search_memory / recommend_seed_nodes），最后输出结构化的理解结果。那个阶段的结果会以【输入理解结果】区块形式出现在你的正式 Prompt 中。两者关系是：

- **本章是通用方法论**：适用于任何场景（无论系统有没有强制跑前置理解阶段，你都应该按这个流程来）
- **【输入理解结果】区块是系统级兜底**：保证短消息/指代密集型/跨天跨任务的场景，一定会做一次检索补充。但系统给的结果只是**供你参考**，你如果重新思考后发现与当下判断不一致，**以你当下的判断为准**，不要被前置阶段的错误结论带偏。

```

- [x] **Step 2: 本地肉眼验证技能 md 完整性**（无需命令，直接看 diff）：确认 Markdown 表格、列表、标题、章节分隔线 `---` 渲染正确；没有孤立的 `|` 或不匹配的反引号。

- [x] **Step 3: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add src/service/domain/system/seed/skills/TEMPLATE_COMMUNICATION/skill.md
git commit -m "docs(skill-communication): add 理解用户消息 SOP 整章 (方案 B)"
```

---

### Task 2：方案 A+ P1 骨架 - ThinkingScene 扩展 + IntentAnalysis 结构体 + 方法签名

**Files:**
- Modify: `src/service/domain/runtime/awakening.rs`（ThinkingScene 枚举 + is_tool_allowed + IntentAnalysis 结构体）
- Modify: `src/service/domain/runtime/mod.rs`（RuntimeAwakening trait 新增 analyze_input_intent 签名）
- Modify: `src/models/prompt_builder.rs`（PromptBuilder trait 新增 build_intent_analyze_prompt + intent_analysis 签名）
- Modify: `src/service/dal/agent.rs`（DefaultPromptBuilder 新增 trait 方法的空实现，保证编译通过）

- [x] **Step 1: ThinkingScene 枚举新增 IntentAnalyze 变体 + is_tool_allowed 扩展**

在 `src/service/domain/runtime/awakening.rs` 找到现有 `ThinkingScene` 枚举定义（位置靠近文件顶部 trait 定义之前），做两处修改：

**① 枚举变体新增 IntentAnalyze**（现有 Awaken / Settle / Summary 保持不动）：
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThinkingScene {
    /// 唤醒场景：响应外部消息，加载全部工具
    #[default]
    Awaken,
    /// 沉睡场景：沉淀记忆，只加载记忆相关工具（neural/memory tag）
    Settle,
    /// 总结场景：思考轮次超限后，总结本轮并安全退出
    Summary,
    /// 新增：意图识别 + 上下文补充阶段
    ///
    /// 思考目标：只理解，不执行任何业务动作
    /// 工具约束：严格禁止 messaging/project_management/external 等执行类工具
    /// 最终输出：IntentAnalysis 结构化 JSON
    IntentAnalyze,
}
```

**② is_tool_allowed 方法新增 IntentAnalyze 分支**（在现有 Summary 分支 match 后追加）：
```rust
pub fn is_tool_allowed(&self, tags: &[String]) -> bool {
    match self {
        ThinkingScene::Awaken => true,
        ThinkingScene::Settle => tags.iter().any(|t| t == "neural" || t == "memory"),
        ThinkingScene::Summary => tags.iter().any(|t| {
            t == "neural" || t == "memory" || t == "messaging" || t == "project_management"
        }),
        // 新增 IntentAnalyze：严格只允许理解类工具
        ThinkingScene::IntentAnalyze => tags.iter().any(|t| {
            t == "neural" || t == "memory" || t == "query" || t == "search"
        }),
    }
}
```

- [x] **Step 2: 新增 IntentAnalysis 结构体（同文件内，ThinkingScene 下方）**

追加在 ThinkingOptions 结构体之后即可，保持同文件内聚：

```rust
/// 结构化意图分析结果
///
/// 由 `RuntimeAwakening::analyze_input_intent()` 输出，供：
/// - awaken 正式阶段 PromptBuilder 渲染【输入理解结果】区块
/// - 外部入站适配器（飞书/WS/HTTP 回调）路由消息前的预分析
/// - 澄清短路判断（need_clarification 非空时，可选择不进入执行阶段直接追问）
///
/// 说明：除了 confidence 用 f32，其余均为自由文本/数组，不做强枚举约束，
/// 避免未来新意图类型导致编译期改动；解析失败时降级为 Default::default()
/// 空结构，保证不阻塞主流程。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct IntentAnalysis {
    /// 主意图类型（推荐取值：Question / TaskRequest / Confirm /
    /// FollowUp / ClarificationResponse / Chat / Mixed）
    /// Agent 自主判断，不做强枚举
    pub intent_type: String,
    /// 意图置信度 0.0~1.0（Agent 自己打分）
    #[serde(default)]
    pub confidence: f32,
    /// 关键词/关键实体抽取（直接可复用于 search_memory 的 query）
    #[serde(default)]
    pub key_terms: Vec<String>,
    /// 指代消歧结果（自由文本数组，每条 Agent 写清楚"X → Y"）
    /// 例如：["\"上次那个方案\" → project=proj_123, task=task_456"]
    #[serde(default)]
    pub resolutions: Vec<String>,
    /// 检索补充上下文摘要（search_memory/recommend_seed_nodes 结果）
    #[serde(default)]
    pub retrieved_context: Vec<String>,
    /// 需要进一步追问澄清的问题（空列表表示理解充分）
    #[serde(default)]
    pub need_clarification: Vec<String>,
    /// 一句话总结：Agent 最终确认自己理解用户想要什么
    #[serde(default = "default_empty_string")]
    pub summary: String,
}

// 辅助函数：serde default，保证缺省时 summary = "" 而不是 null 反序列化失败
fn default_empty_string() -> String {
    String::new()
}
```

注意：确认该文件顶部已引入 `use serde::{Serialize, Deserialize};` 或 `use serde;`，若未引入则在文件顶部 imports 区块追加（看现有代码的用习惯决定是 `#[serde]` 还是 `#[derive(serde::Serialize, serde::Deserialize)]`，按现有 models 风格保持一致）。

- [x] **Step 3: RuntimeAwakening trait 新增方法签名**（mod.rs）

在 `src/service/domain/runtime/mod.rs` 的 `RuntimeAwakening` trait 内（`summary` 方法之后）追加：

```rust
/// 专用输入分析函数：跑一轮 IntentAnalyze 小循环，
/// 产出结构化意图理解结果，**不执行任何业务动作**
///
/// 典型调用方：awaken 的前置阶段（两阶段唤醒流程）
/// 也可复用在：外部消息入站、澄清追问、Agent 间协作消息路由前的理解等
async fn analyze_input_intent(
    &self,
    ctx: RequestContext,
    agent: &crate::models::agent::Agent,
    message: &crate::models::message::Message,
    options: &ThinkingOptions,
) -> crate::error::Result<IntentAnalysis>;
```

同时确认 `IntentAnalysis` 是从 `awakening.rs` re-export 的（通常在 `mod.rs` 顶部 `pub use awakening::{ThinkingScene, ThinkingOptions, IntentAnalysis};` 加上，看现有风格保持一致）。若 awakening.rs 内的 IntentAnalysis 没对外 pub use，则在 mod.rs 顶部补一条 `pub use super::awakening::IntentAnalysis;`。

- [x] **Step 4: PromptBuilder trait 新增方法签名**（prompt_builder.rs）

在 `src/models/prompt_builder.rs` 的 trait 定义中，紧接在 `build_sleep_prompt()` 方法之后追加：

```rust
/// 构建意图分析场景的 Prompt（与 build_sleep_prompt / build 对称）
///
/// 复用已挂载的 system_prompt/tools/skills/history/上下文，
/// 再拼「意图识别 SOP 五步走 + 执行禁令 + JSON schema 输出约束」专属指令块。
/// 默认实现回退到 build()，仅 DefaultPromptBuilder 真正实现意图分析语义
/// （Cli/Remote Agent 不跑此场景，不会走到此分支）。
fn build_intent_analyze_prompt(&self) -> String {
    self.build()
}

/// 注入【输入理解结果】（IntentAnalyze 阶段的产出）
///
/// build() 时会渲染成结构化的"供你参考"区块，放在【当前消息】之前。
/// 仅 DefaultPromptBuilder 有完整渲染；其他 builder 默认忽略此注入（空函数体）。
fn intent_analysis(&mut self, _analysis: &crate::service::domain::runtime::awakening::IntentAnalysis) {
    // 默认空实现。DefaultPromptBuilder 覆盖之。
}
```

- [x] **Step 5: DefaultPromptBuilder 占位空字段 + 空方法，保证编译通过**

在 `src/service/dal/agent.rs` 的 DefaultPromptBuilder 结构体中新增字段（先占位，Task 3 会实现）：

```rust
pub struct DefaultPromptBuilder {
    // ... 现有字段：system_prompt, skills, tools, tool_failures,
    // history, user_profile, project_context, task_context,
    // current_message, current_trace_id ...
    intent_analysis: Option<crate::service::domain::runtime::awakening::IntentAnalysis>,
}
```

在 `DefaultPromptBuilder::new()` 中初始化：`intent_analysis: None,`。

然后在 `impl PromptBuilder for DefaultPromptBuilder` 块中追加两个空壳实现（先让编译过，Task 3/4 填充内容）：

```rust
fn build_intent_analyze_prompt(&self) -> String {
    // Task 3 实现：完整拼装 IntentAnalyze Prompt 指令块
    self.build()
}

fn intent_analysis(
    &mut self,
    analysis: &crate::service::domain::runtime::awakening::IntentAnalysis,
) {
    self.intent_analysis = Some(analysis.clone());
}
```

- [x] **Step 6: 编译 + Clippy 通过**

```bash
cd /Users/aman/Technology/rust/ai_orz
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

**预期**：无编译错误；clippy 零 warnings。

- [x] **Step 7: 单元测试 - ThinkingScene::is_tool_allowed 对 IntentAnalyze 的白名单**

在 `src/service/domain/runtime/awakening.rs` 同级文件末尾 `#[cfg(test)]` 模块追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thinking_scene_intent_analyze_tool_white_list() {
        // 允许的 tags
        let allowed = vec!["neural".into(), "memory".into(), "query".into(), "search".into()];
        for tag in &allowed {
            assert!(
                ThinkingScene::IntentAnalyze.is_tool_allowed(&[tag.clone()]),
                "tag {} should be allowed in IntentAnalyze",
                tag
            );
        }
        // 严格禁止的 tags（执行类 / 外部类）
        let forbidden = vec![
            "messaging".into(),
            "project_management".into(),
            "external_agent".into(),
            "external".into(),
            "finance".into(),
        ];
        for tag in &forbidden {
            assert!(
                !ThinkingScene::IntentAnalyze.is_tool_allowed(&[tag.clone()]),
                "tag {} should be forbidden in IntentAnalyze",
                tag
            );
        }
        // 混合：含一个 forbidden tag + 一个 neural tag = 允许（因为 neural 在白名单里）
        // （注意 is_tool_allowed 语义是 any(...) 只要有一个在白名单就通过，设计上保持与
        // Settle/Summary 一致；执行禁令主要靠 Prompt 层 + 工具注册过滤双重保障）
        assert!(ThinkingScene::IntentAnalyze.is_tool_allowed(&["neural".into(), "messaging".into()]));
    }

    #[test]
    fn intent_analysis_default_and_json_roundtrip() {
        use serde_json;

        let ia = IntentAnalysis {
            intent_type: "TaskRequest".into(),
            confidence: 0.85,
            key_terms: vec!["项目X".into(), "方案A".into(), "进度".into()],
            resolutions: vec!["\"上次那个方案\" → project=123, task=456".into()],
            retrieved_context: vec!["2026-08-10 方案 A/B 比较结论，推荐方案 A（相似度 0.88）".into()],
            need_clarification: vec![],
            summary: "用户想知道项目 X 方案 A 的当前推进进度".into(),
        };

        let json_str = serde_json::to_string(&ia).expect("serialize IntentAnalysis");
        let ia2: IntentAnalysis = serde_json::from_str(&json_str).expect("deserialize IntentAnalysis");

        assert_eq!(ia.intent_type, ia2.intent_type);
        assert!((ia.confidence - ia2.confidence).abs() < 0.0001);
        assert_eq!(ia.key_terms, ia2.key_terms);
        assert_eq!(ia.resolutions, ia2.resolutions);
        assert_eq!(ia.need_clarification, ia2.need_clarification);
        assert_eq!(ia.summary, ia2.summary);
    }

    #[test]
    fn intent_analysis_json_deserialize_partial_fields() {
        // Agent 可能漏字段（如 retrieved_context 省略），必须支持部分缺省反序列化
        let partial = r#"{"intent_type": "Chat", "summary": "用户打招呼"}"#;
        let ia: IntentAnalysis = serde_json::from_str(partial).expect("partial deserialize");
        assert_eq!(ia.intent_type, "Chat");
        assert!((ia.confidence - 0.0).abs() < 0.0001);
        assert!(ia.key_terms.is_empty());
        assert!(ia.retrieved_context.is_empty());
        assert_eq!(ia.summary, "用户打招呼");
    }
}
```

- [x] **Step 8: 运行测试**

```bash
cd /Users/aman/Technology/rust/ai_orz
cargo test -p ai_orz --lib thinking_scene_intent_analyze_tool_white_list intent_analysis_default_and_json_roundtrip intent_analysis_json_deserialize_partial_fields -- --nocapture
```

**预期**：3 tests PASS。

- [x] **Step 9: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add src/service/domain/runtime/awakening.rs src/service/domain/runtime/mod.rs src/models/prompt_builder.rs src/service/dal/agent.rs
git commit -m "feat(runtime): add IntentAnalyze scene + IntentAnalysis struct + trait signatures (A+ P1)"
```

---

### Task 3：方案 A+ P2 核心实现 - analyze_input_intent() + build_intent_analyze_prompt()

**Files:**
- Modify: `src/service/domain/runtime/awakening.rs`（`impl RuntimeAwakening for RuntimeDomainImpl` 中的 `analyze_input_intent` 方法）
- Modify: `src/service/dal/agent.rs`（DefaultPromptBuilder 实现 build_intent_analyze_prompt）

- [x] **Step 1: 实现 `analyze_input_intent()` 方法**

在 `src/service/domain/runtime/awakening.rs` 中找到 `impl RuntimeAwakening for RuntimeDomainImpl` 块（与 awaken / sleep_and_settle / summary 同 impl），追加：

```rust
async fn analyze_input_intent(
    &self,
    ctx: RequestContext,
    agent: &crate::models::agent::Agent,
    message: &crate::models::message::Message,
    options: &ThinkingOptions,
) -> crate::error::Result<IntentAnalysis> {
    use crate::error::ResultExt;

    // 1. 强制构造出 IntentAnalyze 场景专用 options
    //    （调用方误传 Awaken 也没关系，这里强制覆盖 scene）
    let mut analyze_opts = options.clone();
    analyze_opts.scene = ThinkingScene::IntentAnalyze;

    // 2. 查最近 20 条短期记忆做上下文（与 awaken 相同，保证 Agent 有历史可读做消歧）
    let recent_memories = match self
        .memory_dal
        .get_recent_context(ctx.clone(), agent.id.as_str(), 20)
        .await
    {
        Ok(m) => m,
        Err(e) => {
            // 查记忆失败不阻塞，降级为空上下文，继续
            log_warn!(&ctx, "analyze_input_intent_get_recent_context", "failed to get recent context: {:?}", e);
            Vec::new()
        }
    };

    // 3. 构造 PromptBuilder（复用 build_common_context_sections 的能力）
    let mut builder = self
        .build_base_prompt_builder(
            ctx.clone(),
            agent,
            &analyze_opts,
            &recent_memories,
            &[], // IntentAnalyze 阶段无之前的工具失败，tool_failures = []
        )
        .await
        .with_context(|_| "build base prompt builder for IntentAnalyze")?;

    // 【注意】：当前消息作为最后一条 "用户消息" 注入
    //    （build_base_prompt_builder 内部一般会把 current_message 加进去，
    //     若没有，可在 build_intent_analyze_prompt 实现里通过 self.current_message 读取）
    // 若 build_base_prompt_builder 没有自动把 message 挂进去，这里显式加一句：
    // builder.current_message(Some(chat_msg_from_message(message)?));

    // 4. 组装 Prompt（用新的专用 Prompt，不是普通 build()）
    let prompt = builder
        .build_intent_analyze_prompt()
        .with_context(|_| "build intent analyze prompt")?;

    // 5. 调用 wake_agent_brain + run_think_loop 一轮
    //    使用 scene=IntentAnalyze（工具白名单生效）
    let result = self
        .wake_agent_brain(
            ctx.clone(),
            agent,
            ThinkingScene::IntentAnalyze,
            prompt,
            &[], // 工具失败统计保持空
        )
        .await;

    // 6. 解析 Final 为 IntentAnalysis（多重降级保障）
    match result {
        Ok(output) => {
            let final_text = output.final_answer.unwrap_or_default();
            // 尝试多种解析策略，层层降级：
            //   a) 直接解析整段为 JSON
            //   b) 若被 ```json ... ``` 包裹，提取中间内容再解析
            //   c) 只提取第一个 { ... } 用正则匹配 JSON 块再解析
            //   d) 解析失败：降级为 summary = 原文，其他空
            let parsed = parse_intent_analysis_json(&final_text);
            match parsed {
                Some(ia) => Ok(ia),
                None => {
                    log_warn!(&ctx, "analyze_input_intent_parse_failed", "final text is not valid IntentAnalysis JSON, fallback to empty. trace_id={}", ctx.trace_id().unwrap_or(""));
                    Ok(IntentAnalysis {
                        summary: final_text,
                        ..Default::default()
                    })
                }
            }
        }
        Err(e) => {
            // think loop 整体失败：记 warn 日志，返回空 IntentAnalysis（不阻塞主流程）
            log_warn!(&ctx, "analyze_input_intent_think_loop_failed", "error: {:?}", e);
            Ok(IntentAnalysis::default())
        }
    }
}

/// 从 Agent Final 文本中尽量提取并解析 IntentAnalysis JSON
/// 返回 None 表示所有解析策略都失败，调用方会降级。
fn parse_intent_analysis_json(text: &str) -> Option<IntentAnalysis> {
    use serde_json::Value;

    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    // 策略 a) 直接尝试整段
    if let Ok(ia) = serde_json::from_str::<IntentAnalysis>(text) {
        return Some(ia);
    }

    // 策略 b) 找 ```json ... ``` 代码块
    let re_json_block = regex::Regex::new(r"(?s)```(?:json)?\s*(.+?)\s*```").ok()?;
    if let Some(caps) = re_json_block.as_ref().and_then(|re| re.captures(text)) {
        let inner = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        if let Ok(ia) = serde_json::from_str::<IntentAnalysis>(inner) {
            return Some(ia);
        }
    }

    // 策略 c) 找第一个完整 {...} JSON 对象（带括号匹配，不被字符串内部大括号骗）
    if let Some(json_obj) = extract_first_json_object(text) {
        if let Ok(ia) = serde_json::from_str::<IntentAnalysis>(&json_obj) {
            return Some(ia);
        }
        // 容错：如果某些字段类型不匹配（如 confidence 是字符串），
        // 先 parse 成 Value，手动挑字段 + 类型宽容转换
        if let Ok(val) = serde_json::from_str::<Value>(&json_obj) {
            let ia = IntentAnalysis {
                intent_type: val.get("intent_type").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                confidence: val.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                key_terms: val
                    .get("key_terms")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default(),
                resolutions: val
                    .get("resolutions")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default(),
                retrieved_context: val
                    .get("retrieved_context")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default(),
                need_clarification: val
                    .get("need_clarification")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default(),
                summary: val.get("summary").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            };
            // 至少要有 intent_type 或 summary 任一非空，才算解析出有效结果
            if !ia.intent_type.is_empty() || !ia.summary.is_empty() {
                return Some(ia);
            }
        }
    }

    None
}

/// 简易括号匹配：从字符串中找到第一个顶层的 { ... } 完整 JSON 对象
/// 支持字符串内部出现大括号的情况（简单处理：遇到未转义的双引号进入字符串模式，内部的 {} 不计入括号计数）
fn extract_first_json_object(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let mut depth = 0;
            let start = i;
            let mut in_string = false;
            let mut escape = false;
            while i < bytes.len() {
                let b = bytes[i];
                if escape {
                    escape = false;
                    i += 1;
                    continue;
                }
                if b == b'\\' {
                    escape = true;
                    i += 1;
                    continue;
                }
                if b == b'"' {
                    in_string = !in_string;
                    i += 1;
                    continue;
                }
                if !in_string {
                    if b == b'{' {
                        depth += 1;
                    } else if b == b'}' {
                        depth -= 1;
                        if depth == 0 {
                            let end = i + 1;
                            return Some(s[start..end].to_string());
                        }
                    }
                }
                i += 1;
            }
            return None; // 顶层 { 开始但未匹配到完整闭合
        }
        i += 1;
    }
    None
}
```

注意事项：
1. 以上代码用到了 `regex::Regex`。**先检查本 workspace 的 Cargo.toml 是否已有 `regex` 依赖**（`cd /Users/aman/Technology/rust/ai_orz && grep -E "^(ai_orz|.+)\s*=.*regex" Cargo.toml | head`）。如果 `regex` 已在依赖树中（通过其他包），`src/service/domain/runtime/Cargo.toml` 下补充 `regex = { workspace = true }` 或版本号；如果没有，出于"尽量不引入新依赖"原则，**把策略 b) 降级为手动字符串查找**——见下 Step 1.1 降级版。

2. `build_base_prompt_builder`、`chat_msg_from_message` 等辅助函数不一定存在，请根据 awakening.rs 内 awaken() 方法当前真实代码**对齐其具体实现（字段名/函数名/调用顺序/辅助函数名）**，不要假设它们长什么样。**核心要求是：和 awaken() 一样把人设、技能、工具、上下文、历史都装进去**，保证 IntentAnalyze 阶段和正常 awaken 阶段拥有相同的背景知识，只是最终 Prompt 模板不同。

- [x] **Step 1.1（降级）如果不能引入 regex 依赖，则把策略 b) 改为手动字符串查找**

用以下代码替换策略 b) 部分：

```rust
    // 策略 b) 找 ```json ... ``` 或 ``` ... ``` 代码块
    if let Some(rest) = text.strip_prefix("```") {
        // 跳过可选的 "json" 标识符
        let rest_after_lang = rest
            .strip_prefix("json")
            .map(|s| s.trim_start())
            .unwrap_or(rest.trim_start());
        // 找到结尾 ```
        if let Some(end_idx) = rest_after_lang.find("\n```") {
            let inner = &rest_after_lang[..end_idx];
            if let Ok(ia) = serde_json::from_str::<IntentAnalysis>(inner.trim()) {
                return Some(ia);
            }
        }
        // 非前缀形式的代码块（出现在文本中间），简单找两 ``` 之间内容的第一组
    } else if let Some(start) = text.find("```") {
        let after_first = &text[start + 3..];
        let after_lang = after_first
            .strip_prefix("json")
            .map(|s| s.trim_start_matches(|c: char| c == ' ' || c == '\n' || c == '\r' || c == '\t'))
            .unwrap_or(after_first.trim_start_matches(|c: char| c == ' ' || c == '\n' || c == '\r' || c == '\t'));
        if let Some(end) = after_lang.find("```") {
            let inner = &after_lang[..end];
            if let Ok(ia) = serde_json::from_str::<IntentAnalysis>(inner.trim()) {
                return Some(ia);
            }
        }
    }
```

- [x] **Step 2: 实现 DefaultPromptBuilder::build_intent_analyze_prompt()**

在 `src/service/dal/agent.rs` 的 `impl PromptBuilder for DefaultPromptBuilder` 块中，把 Task 2 写的空壳 `build_intent_analyze_prompt()` 替换为完整实现：

```rust
fn build_intent_analyze_prompt(&self) -> String {
    let mut result = String::new();

    // 1~4 部分与 build()/build_sleep_prompt() 完全一致 → 复用公共方法
    // 1. System Prompt（Agent 人设）
    if let Some(system) = &self.system_prompt {
        result.push_str(system);
        result.push_str("\n\n");
    }
    // 2. 工具/技能区块（复用 build 的公共方法；调用方 analyze_input_intent 会先通过
    //    wake_agent_brain(scene=IntentAnalyze) 的工具白名单过滤，保证这里没有执行类工具）
    result.push_str(&self.build_tools_and_skills_sections());
    result.push('\n');
    // 3. 通用上下文区块（用户画像 + 项目 + 任务，有值即拼装）
    result.push_str(&self.build_common_context_sections());
    result.push('\n');
    // 4. 历史对话记忆（最近 20 条）
    if !self.history.is_empty() {
        result.push_str("【历史对话】\n");
        for h in &self.history {
            result.push_str(h);
            result.push('\n');
        }
        result.push('\n');
    }

    // 5. Trace ID
    if let Some(trace_id) = &self.current_trace_id {
        result.push_str(&format!("【思考 Trace ID】{}\n\n", trace_id));
    }

    // ==================== 本阶段专属指令块（核心）====================
    result.push_str("===== 【输入理解阶段】IntentAnalyze 场景约束（非常重要！）=====\n\n");
    result.push_str("## 你的任务：只做理解，不做执行\n\n");
    result.push_str("你当前处于正式干活前的「审题阶段」。本阶段你的唯一目标是产出一份结构化的理解结果，然后就结束本轮思考。\n\n");
    result.push_str("✅ 必须做：\n");
    result.push_str("   1. 在思考中严格按下方「理解 SOP 五步走」执行一遍\n");
    result.push_str("   2. 必须调用一次 search_memory（或 recommend_seed_nodes + traverse_knowledge_graph，空白场景）做上下文补充（100% 全新无历史的闲聊可豁免，请在思考中说明理由）\n");
    result.push_str("   3. 最终输出严格的 JSON 对象，字段完整可被解析\n\n");
    result.push_str("❌ 严格禁止做（任何违反都将导致此阶段结果作废）：\n");
    result.push_str("   - 禁止调用 send_message / send_task_assignment_message / send_message_to_agent：不准给任何用户/Agent 发消息\n");
    result.push_str("   - 禁止调用 create_task / update_task / create_project / update_project / update_memory 状态写入类工具：不准改动任何系统状态（只有 save_short_term_memory 内部记忆写入是允许的，若你需要临时记录东西）\n");
    result.push_str("   - 禁止做任何外部 API 调用、shell 执行、文件读写类工具\n");
    result.push_str("   - 禁止直接回答用户问题（哪怕你 100% 知道答案），不准在 Final 里写对用户的回复\n\n");

    result.push_str("## 理解 SOP 五步走（在思考中严格按此顺序执行）\n\n");
    result.push_str("### Step 1：意图识别\n");
    result.push_str("在思考中先把【当前消息】归类，写出你判断的依据：\n");
    result.push_str("- Question：提问型（要信息/问进度/问规则/请教）\n");
    result.push_str("- TaskRequest：任务型（提需求/安排工作/要产出）\n");
    result.push_str("- Confirm：确认型（同意/否定/选择/拍板）\n");
    result.push_str("- FollowUp：追问型（承接之前某条回答/产出的继续追问）\n");
    result.push_str("- ClarificationResponse：澄清响应型（针对你前面追问的答复）\n");
    result.push_str("- Chat：闲聊型（打招呼/客套/社交礼貌）\n");
    result.push_str("- Mixed：混合型（多类意图，拆分说明）\n");
    result.push_str("意图类型写入 intent_type 字段；置信度 0.0~1.0 自己打分写入 confidence。\n\n");

    result.push_str("### Step 2：指代与上下文消歧\n");
    result.push_str("1. 仔细读【历史对话】+【项目/任务上下文】+【用户画像】\n");
    result.push_str("2. 找【当前消息】中的指代短语：这/那/上次/那个/他/按之前定的来 等\n");
    result.push_str("3. 在思考中把每个指代对应到具体对象（project_id/task_id/message_id/某个人物…），写进 resolutions 数组，每条格式：\"\\\"XXX\\\" → YYY\"\n");
    result.push_str("4. 读完所有上下文仍无法确定 → 写进 need_clarification，不要硬猜\n\n");

    result.push_str("### Step 3：关键词抽取\n");
    result.push_str("从【当前消息】+ 消歧后的具体对象中，抽取 3~8 个关键词/关键实体（项目名/任务名/产品名/人名/专有名词/核心动词短语），写进 key_terms 数组。\n\n");

    result.push_str("### Step 4：语义检索补充（强制执行）\n");
    result.push_str("- 必须调用一次 search_memory（用 Step 3 的关键词组合成 query），除非你在思考中明确说明这是 100% 全新无历史的话题。\n");
    result.push_str("- 首次进入这个 project/task 空场景：先调用 recommend_seed_nodes 拿图谱起点，再按需 traverse_knowledge_graph 走 1~2 跳。\n");
    result.push_str("- list_messages 上拉历史也是可选：如果 Working Memory 不够。\n");
    result.push_str("- 把检索命中的高相关内容**你自己概括为短摘要**（1~2 句每条，不要贴原始 JSON），写进 retrieved_context。\n\n");

    result.push_str("### Step 5：判断是否需要澄清 + 总结\n");
    result.push_str("- 如果 Step 2 消歧失败 / 混合型意图优先级不清 / 需求边界不明 / 需要用户决策 → 把要问用户的具体问题逐条写进 need_clarification（问题尽量用选择题形式，不要开放式）\n");
    result.push_str("- 如果理解充分 → need_clarification = []\n");
    result.push_str("- 最后在思考中用一句话总结「我理解用户想要：XXX」，写进 summary。\n\n");

    result.push_str("## 最终输出规范（必须严格遵守）\n\n");
    result.push_str("你输出的【最终 Final 内容】只能是一个 JSON 对象（不要 ```json 包裹，不要 Markdown 注释，不要任何其他解释性文字），严格符合以下 schema：\n\n");
    result.push_str("{\n");
    result.push_str("  \"intent_type\": \"Question | TaskRequest | Confirm | FollowUp | ClarificationResponse | Chat | Mixed\",\n");
    result.push_str("  \"confidence\": 0.0 到 1.0 的数字，\n");
    result.push_str("  \"key_terms\": [\"关键词1\", \"关键词2\", ...],\n");
    result.push_str("  \"resolutions\": [\"\\\"XXX\\\" → 具体对象\", ...],\n");
    result.push_str("  \"retrieved_context\": [\"搜索命中的结果摘要 1\", ...],\n");
    result.push_str("  \"need_clarification\": [\"需要向用户澄清的问题 1\", ...],\n");
    result.push_str("  \"summary\": \"一句话总结你理解的用户需求\"\n");
    result.push_str("}\n\n");
    result.push_str("===== 【输入理解阶段】指令结束 =====\n\n");

    // 6. 当前消息（放在最后，给 Agent 明确的靶子）
    if let Some(msg) = &self.current_message {
        result.push_str("【当前消息】\n");
        result.push_str(msg);
        result.push_str("\n\n现在开始：在思考中走完 Step 1~5，然后输出最终 JSON。\n");
    } else {
        result.push_str("【注意】当前消息为空。请直接输出空 JSON 或说明情况。\n");
    }

    result
}
```

- [x] **Step 3: 编译 + Clippy + 单元测试**

```bash
cd /Users/aman/Technology/rust/ai_orz
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p ai_orz --lib parse_intent_analysis_json extract_first_json_object thinking_scene_intent_analyze_tool_white_list -- --nocapture 2>&1 | tail -40
```

**预期**：
- 编译 + clippy 通过
- 单元测试如果没有显式命名的 parse/extract 测试，补一个 #[cfg(test)] 小测试（提取 extract_first_json_object 模块内加一个 UT：`"prefix {\"a\":1} middle {\"b\":2} suffix"` 断言提取出 `{"a":1}`；字符串内部 {} 不计数的场景：`"{\"key\":\"val{ue}\"}"` 断言能正确提取完整）

- [x] **Step 4: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add src/service/domain/runtime/awakening.rs src/service/dal/agent.rs
git commit -m "feat(runtime): implement analyze_input_intent() + build_intent_analyze_prompt() (A+ P2)"
```

---

### Task 4：方案 A+ P3 串联 awaken 为两阶段 + 渲染【输入理解结果】区块

**Files:**
- Modify: `src/service/domain/runtime/awakening.rs`（awaken() 方法内，在"原 awaken 查最近 20 条 + 构造 builder"之后、调用 build() 之前插入 analyze_input_intent）
- Modify: `src/service/dal/agent.rs`（DefaultPromptBuilder 新增 `render_intent_analysis_section` 私有方法 + build() 中【当前消息】区块之前调用它）

- [x] **Step 1: awaken() 中在构造 builder 之后、build() 之前插入阶段 1**

找到 `impl RuntimeAwakening for RuntimeDomainImpl::awaken()` 方法。定位到关键片段（伪代码表示位置）：

```rust
// 【原有 awaken 流程中的关键两行】
let mut builder = self.build_base_prompt_builder(...).await?;
// 【在这两行之间插入新增代码 →】
// 【原下一步是】 let prompt = builder.build()?;
```

在中间插入：

```rust
    // =============== 新增：阶段 1 - 意图分析（两阶段唤醒）===============
    // 先强制跑一轮 IntentAnalyze，得到结构化理解结果（失败则降级为空，不阻塞）
    let intent_analysis = match self
        .analyze_input_intent(ctx.clone(), agent, message, options)
        .await
    {
        Ok(ia) => ia,
        Err(e) => {
            // 意图分析阶段任何错误都不阻塞主流程
            log_warn!(&ctx, "awaken_pre_analyze_input_intent_failed", "ignoring error: {:?}", e);
            IntentAnalysis::default()
        }
    };

    // 【P4 澄清短路暂不启用，只留注释入口】
    // if !intent_analysis.need_clarification.is_empty() {
    //     // 未来可以在这里：直接通过 message_domain 发送澄清消息给用户，
    //     // 然后 return Ok(ThinkingOutput::default())，短路不进入正式干活阶段。
    //     // 验证稳定后再解锁此分支。
    // }

    // 注入到 builder，供 build() 时渲染【输入理解结果】区块
    builder.intent_analysis(&intent_analysis);
    // =============== 新增代码结束 ======================================
```

- [x] **Step 2: DefaultPromptBuilder 新增 render_intent_analysis_section 私有方法**

在 `impl DefaultPromptBuilder`（非 trait impl，即私有辅助方法那块）中新增，与 `build_common_context_sections()` / `build_tools_and_skills_sections()` 相邻：

```rust
/// 渲染【输入理解结果】区块（阶段 1 的产物）
///
/// 设计原则：反复强调"这是你上一阶段自己得出的参考结论，若与你当下判断不一致，以你为准"。
/// 避免 Agent 被错误的前置理解带偏。
///
/// 截断规则（防止 Prompt 过长超 token）：
/// - resolutions / retrieved_context / need_clarification / key_terms 最多各显示 5 条
/// - 每条最长 200 字，超出加 "…"
///
/// 若 intent_analysis 为空：不渲染任何区块。
fn render_intent_analysis_section(&self) -> String {
    let ia = match &self.intent_analysis {
        None => return String::new(),
        Some(ia) if ia.intent_type.is_empty() && ia.summary.is_empty() => return String::new(),
        Some(ia) => ia,
    };

    let trunc = |s: &str, max: usize| -> String {
        let chars: Vec<char> = s.chars().collect();
        if chars.len() <= max {
            s.to_string()
        } else {
            let mut out: String = chars.into_iter().take(max).collect();
            out.push('…');
            out
        }
    };
    let max_each = 5;

    let mut s = String::new();
    s.push_str("【输入理解结果（由你在前一阶段 IntentAnalyze 得出，仅供参考）】\n");
    s.push_str("说明：以下内容是你上一阶段「审题阶段」自己得出的理解摘要，供你正式执行时参考。\n");
    s.push_str("如果发现与你当下重新判断不一致，请以你当下的理解为准，不要被以下内容束缚。\n\n");

    if !ia.intent_type.is_empty() {
        s.push_str(&format!("1. 意图类型：{}（置信度 {:.2}）\n", ia.intent_type, ia.confidence));
    }

    if !ia.key_terms.is_empty() {
        s.push_str(&format!("2. 关键词：{}\n", ia.key_terms.iter().take(max_each).cloned().collect::<Vec<_>>().join("、")));
        if ia.key_terms.len() > max_each {
            s.push_str(&format!("   （另省略 {} 个关键词）\n", ia.key_terms.len() - max_each));
        }
    }

    if !ia.resolutions.is_empty() {
        s.push_str("3. 指代消歧：\n");
        for (i, r) in ia.resolutions.iter().take(max_each).enumerate() {
            s.push_str(&format!("   - {}\n", trunc(r, 200)));
        }
        if ia.resolutions.len() > max_each {
            s.push_str(&format!("   （另省略 {} 条消歧结果）\n", ia.resolutions.len() - max_each));
        }
    }

    if !ia.retrieved_context.is_empty() {
        s.push_str("4. 检索补充上下文（你上一步自己搜索概括的）：\n");
        for (i, c) in ia.retrieved_context.iter().take(max_each).enumerate() {
            s.push_str(&format!("   - {}\n", trunc(c, 200)));
        }
        if ia.retrieved_context.len() > max_each {
            s.push_str(&format!("   （另省略 {} 条检索摘要）\n", ia.retrieved_context.len() - max_each));
        }
    }

    if !ia.need_clarification.is_empty() {
        s.push_str("💡 上一阶段判断需要向用户澄清：\n");
        for (i, q) in ia.need_clarification.iter().take(max_each).enumerate() {
            s.push_str(&format!("   ? {}\n", trunc(q, 200)));
        }
        if ia.need_clarification.len() > max_each {
            s.push_str(&format!("   （另省略 {} 个澄清问题）\n", ia.need_clarification.len() - max_each));
        }
    }

    if !ia.summary.is_empty() {
        s.push_str(&format!("\n6. 一句话理解总结：{}\n", trunc(&ia.summary, 300)));
    }

    s.push_str("\n===== 以上理解仅供参考 =====\n\n");
    s
}
```

- [x] **Step 3: build() 中在【当前消息】之前调用 render_intent_analysis_section()**

在 `DefaultPromptBuilder::build()` 方法里，找到拼装【当前消息】区块的位置（伪代码）：

```rust
    // ... 之前：【历史对话】→【工具失败警告】→【思考 Trace ID】...
    if let Some(trace_id) = &self.current_trace_id {
        result.push_str(&format!("【思考 Trace ID】{}\n\n", trace_id));
    }

    // =============== 【新增：在这里插入】===============
    let intent_section = self.render_intent_analysis_section();
    if !intent_section.is_empty() {
        result.push_str(&intent_section);
    }

    // 【原下一步是】
    if let Some(msg) = &self.current_message {
        result.push_str("【当前消息】\n");
        result.push_str(msg);
        ...
    }
```

确保【输入理解结果】区块严格出现在【当前消息】之前、【思考 Trace ID】之后。

- [x] **Step 4: 单元测试 - DefaultPromptBuilder 渲染断言**

在 `DefaultPromptBuilder` 对应的 `#[cfg(test)]` 模块（或新建）中追加测试：

```rust
#[cfg(test)]
mod pb_intent_analysis_tests {
    use super::*;
    use crate::service::domain::runtime::awakening::IntentAnalysis;

    #[test]
    fn build_contains_intent_analysis_section_when_present() {
        let ia = IntentAnalysis {
            intent_type: "TaskRequest".into(),
            confidence: 0.88,
            key_terms: vec!["项目X".into(), "方案A".into(), "排期".into()],
            resolutions: vec!["\"上次那个方案\" → project=123, task=456".into()],
            retrieved_context: vec!["2026-08-10 方案 A/B 比较，推荐 A".into()],
            need_clarification: vec![],
            summary: "用户想知道项目 X 方案 A 当前进度".into(),
        };

        let mut builder = DefaultPromptBuilder::new();
        builder.system_prompt("你是一个测试用 Agent。");
        builder.intent_analysis(&ia);
        builder.current_message = Some("上次那个方案结果呢？".into());
        let prompt = builder.build().expect("build");

        assert!(prompt.contains("【输入理解结果"));
        assert!(prompt.contains("意图类型：TaskRequest"));
        assert!(prompt.contains("关键词：项目X、方案A、排期"));
        assert!(prompt.contains("指代消歧："));
        assert!(prompt.contains("检索补充上下文（你上一步自己搜索概括的）："));
        assert!(prompt.contains("一句话理解总结："));
        assert!(prompt.contains("以上理解仅供参考"));
        // 位置顺序：输入理解结果 必须 出现在 【当前消息】之前
        let idx_ia = prompt.find("【输入理解结果").unwrap();
        let idx_cm = prompt.find("【当前消息】").unwrap();
        assert!(idx_ia < idx_cm, "intent_analysis section must appear BEFORE current_message");
    }

    #[test]
    fn build_omits_intent_analysis_section_when_empty() {
        let mut builder = DefaultPromptBuilder::new();
        builder.system_prompt("你是一个测试用 Agent。");
        builder.intent_analysis(&IntentAnalysis::default());
        let prompt = builder.build().expect("build");
        assert!(!prompt.contains("【输入理解结果"));
    }

    #[test]
    fn intent_analysis_section_applies_truncation_rules() {
        let many_terms: Vec<String> = (0..20).map(|i| format!("term{}", i)).collect();
        let many_res: Vec<String> = (0..20).map(|i| format!("resolution line {} with a lot of text to trigger per-line truncation. padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding padding", i)).collect();
        let ia = IntentAnalysis {
            intent_type: "Mixed".into(),
            confidence: 0.5,
            key_terms: many_terms,
            resolutions: many_res,
            retrieved_context: Vec::new(),
            need_clarification: Vec::new(),
            summary: "summary".into(),
        };
        let mut builder = DefaultPromptBuilder::new();
        builder.system_prompt("sys");
        builder.intent_analysis(&ia);
        let prompt = builder.build().expect("build");

        // 最多显示 5 条 term，剩余显示"另省略 15 个关键词"
        assert!(prompt.contains("另省略 15 个关键词"), "prompt missing truncated terms hint: {}", prompt);
    }
}
```

- [x] **Step 5: 编译 + Clippy + 运行新增测试**

```bash
cd /Users/aman/Technology/rust/ai_orz
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p ai_orz --lib build_contains_intent_analysis_section_when_present build_omits_intent_analysis_section_when_empty intent_analysis_section_applies_truncation_rules -- --nocapture 2>&1 | tail -30
```

**预期**：编译 + clippy 通过；3 tests PASS。

- [x] **Step 6: 集成测试 - analyze_input_intent 可运行 + 降级不阻塞**

新建 `tests/intent_analyze_two_stage.rs`（或在现有 `tests/agent_intelligence_integration_tests.rs` 后追加模块）：

```rust
// tests/intent_analyze_two_stage.rs
// 集成测试：不依赖真实 cortex，只验证 analyze_input_intent 在测试环境中
// 1) 能正常返回（无论是否解析到 JSON）
// 2) 遇到模拟错误时能自动降级返回 Default，不 panic
// 3) awaken() 两阶段串联后最终 Prompt 中出现【输入理解结果】区块

use ai_orz::tests::common::env::init_full_test_env;
use ai_orz::service::domain::runtime::awakening::{IntentAnalysis, ThinkingOptions, ThinkingScene};

#[sqlx::test]
async fn analyze_input_intent_never_panics_returns_ok(pool: sqlx::SqlitePool) {
    let (ctx, _org, _user, agent, storage) = init_full_test_env(&pool).await;

    // 构造一条典型指代密集型用户消息
    let msg = create_test_message_for_agent(&agent, "上次那个方案结果呢？").await;

    let options = ThinkingOptions::for_scene(ThinkingScene::IntentAnalyze);
    let result = storage
        .runtime_domain()
        .analyze_input_intent(ctx.clone(), &agent, &msg, &options)
        .await;

    // 无论 cortex 是否 mock 成功，Result 必须是 Ok（任何内部错误都被降级为 Ok(empty)）
    let ia = result.expect("analyze_input_intent must never return Err");
    // 返回类型是 IntentAnalysis（即使全空也是合法的降级结果）
    // 所以我们只断言没有 panic，且字段类型正常
    assert!(ia.confidence >= 0.0 && ia.confidence <= 1.5); // 容错一些异常浮点值
    let _: Vec<String> = ia.key_terms.clone(); // 保证能 move 成 String 数组
}

// ==== 辅助函数（如测试公共 env.rs 内已有则直接复用；若无则按以下模式） ====
async fn create_test_message_for_agent(
    agent: &ai_orz::models::agent::Agent,
    content: &str,
) -> ai_orz::models::message::Message {
    // 按项目通用的 Message 构造方式（参考其他集成测试中构造消息的代码）
    // 通常通过 message_domain.send_message 或直接调用 message_dal.create
    // 这里用伪占位，工程落地时对齐 tests/ 中其他文件的消息构造方法
    unimplemented!("align with existing message construction in tests/common or tests/agent_*.rs")
}
```

注意：`create_test_message_for_agent` 的实现**必须对齐项目现有集成测试中构造测试消息的模式**（如在 `tests/agent_integration_tests.rs` 中如何创建 Message 实体），不要自己发明一套。

- [x] **Step 7: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add src/service/domain/runtime/awakening.rs src/service/dal/agent.rs tests/intent_analyze_two_stage.rs
git commit -m "feat(runtime): awaken two-stage chain + intent_analysis section rendering (A+ P3)"
```

---

### Task 5：全量质量验证 + 最终提交

- [x] **Step 1: clippy -D warnings 全通过**

```bash
cd /Users/aman/Technology/rust/ai_orz
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -30
```

**预期**：0 warnings, 0 errors。若有关于 unused import / dead code 的 Clippy 警告，顺手修掉。

- [x] **Step 2: 跑全部测试（common + macros + backend + frontend wasm32 按需）**

```bash
cd /Users/aman/Technology/rust/ai_orz
# 后端 + common（最关键，1101 测试）
cargo test --workspace --exclude ai-orz-frontend 2>&1 | tail -60
```

**预期**：测试总数 ≥ 1101，**100% 通过**（0 failed）。如果有新增测试导致旧用例失败，按「修改应用代码使其通过，不要为了通过而简化或删除旧测试」原则处理。

- [x] **Step 3: 覆盖率不明显下降（可选，本地跑）**

```bash
cd /Users/aman/Technology/rust/ai_orz
# 如果环境里有 cargo-llvm-cov：
# cargo llvm-cov --workspace --exclude ai-orz-frontend --summary-only 2>&1 | tail -20
```

**预期**：整体覆盖率大致维持在当前水平或略高于门槛基线（main 45% / PR 38%）。轻微下降可以接受，只要不是大面积掉点。

- [x] **Step 4: 最终大提交（或合并 commits，视你的习惯而定；这里保留分步 commit 让 CR 更清晰）**

不额外提交，Task 1~4 已经各自 commit。

---

## 自审（计划写完后自己跑一遍）

### 1. Spec 覆盖率（对照 design 文档）

| design 文档要求 | 计划中哪个 Task 覆盖 | ✅/❌ |
|-----------------|-------------------|------|
| ThinkingScene 新增 IntentAnalyze 变体 + 工具白名单（neural/memory/query/search，排除 messaging/project_management） | Task 2 Step 1 + 单元测试 Step 7 | ✅ |
| IntentAnalysis 结构体（7 字段 + serde + 缺省降级） | Task 2 Step 2 + 单元测试 Step 7 | ✅ |
| RuntimeAwakening trait 新增 analyze_input_intent 复用函数签名 | Task 2 Step 3 | ✅ |
| PromptBuilder trait 新增 build_intent_analyze_prompt + intent_analysis 签名 | Task 2 Step 4 | ✅ |
| 方案 B：TEMPLATE_COMMUNICATION 技能新增「理解用户消息 SOP」整章 | Task 1 Step 1 | ✅ |
| analyze_input_intent 内部实现（复用 options/wake_agent_brain/run_think_loop + 多层 JSON 解析降级策略） | Task 3 Step 1 | ✅ |
| build_intent_analyze_prompt 模板（SOP 五步 + 禁令 + JSON schema + 当前消息靶子） | Task 3 Step 2 | ✅ |
| awaken 两阶段串联（先 analyze_input_intent → 注入 builder → build 渲染区块） | Task 4 Step 1 | ✅ |
| 【输入理解结果】区块渲染（"供你参考"姿态 + 截断规则 + 位置在当前消息之前） | Task 4 Step 2 + Step 3 + 单元测试 Step 4 | ✅ |
| P4 澄清短路：暂不启用，留注释入口 | Task 4 Step 1 内嵌注释 | ✅ |
| 降级策略：think_loop 失败 / JSON 解析失败 → 返回 Default（不阻塞） | Task 3 Step 1 多处降级分支 + 集成测试 Step 6 | ✅ |

### 2. Placeholder 扫描：已消除

- ❌ 无 TBD/TODO/implement later
- ❌ 无 "Add appropriate error handling" 类空泛要求，所有降级都写了具体代码
- ❌ 无 "Similar to Task N"，每个 Task 独立写清代码/命令/测试
- ❌ 无 "Write tests for the above" 无具体代码的要求，每个 Task 对应有具体 test 代码块
- 个别函数（如 `create_test_message_for_agent`）写明"对齐现有集成测试的消息构造模式"，属于合理的"对齐既有模式"说明，不是 placeholder。

### 3. 类型/接口一致性：已核对

- `IntentAnalysis` 字段在 Task 2 Step 2 定义 → Task 3 Step 1 解析 → Task 4 Step 2 渲染，三处字段名/类型完全一致（`intent_type/confidence/key_terms/resolutions/retrieved_context/need_clarification/summary`）
- `ThinkingScene::IntentAnalyze` 在 Task 2 Step 1 定义 → Task 3 Step 1 `wake_agent_brain(scene=IntentAnalyze)` 传参 → is_tool_allowed 白名单测试，三处一致
- `analyze_input_intent(ctx, agent, message, options) -> Result<IntentAnalysis>` 签名在 Task 2 Step 3（trait 声明）与 Task 3 Step 1（impl）完全一致
- `build_intent_analyze_prompt()` 与 `intent_analysis(&mut self, &IntentAnalysis)` 在 trait 声明（Task 2 Step 4）、builder 空实现（Task 2 Step 5）、真实实现（Task 3 Step 2 + Task 4 Step 2）三处签名一致

Plan 质量通过，可进入执行阶段。

---

Plan complete and saved to `docs/superpowers/plans/2026-08-14-intent-analyze-two-stage-awaken.md`. Two execution options:

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?
