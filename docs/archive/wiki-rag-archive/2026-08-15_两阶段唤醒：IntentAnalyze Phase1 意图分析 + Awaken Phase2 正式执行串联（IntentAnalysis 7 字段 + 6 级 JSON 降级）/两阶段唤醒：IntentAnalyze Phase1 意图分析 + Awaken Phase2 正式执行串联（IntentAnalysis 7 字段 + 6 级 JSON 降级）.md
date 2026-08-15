> 📦 归档标记（2026-08-15）：被 [Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2 正式执行串联](docs/wiki/knowledge/zh/Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2 正式执行串联/Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2 正式执行串联.md) 取代。保留原因：历史参考，主卡已吸收本卡独有源码锚点与硬约束。生效方案：主卡真实路径作为唯一 RAG 召回目标。
---
kind: wiki_knowledge_card
name: 两阶段唤醒：IntentAnalyze Phase1 意图分析 + Awaken Phase2 正式执行串联（IntentAnalysis 7 字段 + 6 级 JSON 降级）
category: runtime domain（awakening + intent_analyze）
scope:
  - "src/service/domain/runtime/intent_analyze.rs"
  - "src/service/domain/runtime/awakening.rs"
  - "src/service/domain/runtime/types.rs"
  - "src/service/domain/runtime/mod.rs"
  - "src/models/prompt_builder.rs"
  - "src/service/dal/agent.rs"
  - "common/src/enums/*.rs"
source_files:
  - src/service/domain/runtime/awakening.rs#L230-L280（awaken Phase1 串联：analyze_input_intent 调用 + 错误降级 Option<IntentAnalysis>，不阻塞主流程；注入 builder.intent_analysis()）
  - src/service/domain/runtime/intent_analyze.rs#L1-L90（analyze_input_intent_inner：构造 IntentAnalyze scene options + 20 条短期记忆 + scene 技能过滤 + build_intent_analyze_prompt + init_think_runtime_and_policy）
  - src/service/domain/runtime/awakening.rs#L862-L908（RuntimeAwakening::analyze_input_intent：stats 包一层 + AgentLoopEvent started/finished 发布，监控区分 Phase 1）
  - src/service/domain/runtime/mod.rs#L203-L214（RuntimeAwakening trait 第 4 入口方法签名 + 典型调用场景注释）
  - src/service/dal/agent.rs#L1022-L1080（DefaultPromptBuilder::build_intent_analyze_prompt：人设+技能+上下文+历史 1-8 区复用 + 追加「意图识别 SOP 五步走 + 执行禁令 + 7 字段 JSON schema 约束」指令块；--- INTENT_ANALYSIS_START --- 锚点输出 Final）
  - src/models/prompt_builder.rs#L104-L123（PromptBuilder trait：build_intent_analyze_prompt 默认实现回退 build()；intent_analysis() 默认注入空函数体；仅 DefaultPromptBuilder 完整渲染 7 字段区块）
  - common/src/enums.rs 或 common/src/enums/agent_thinking.rs:Ln-Lm（ThinkingScene::IntentAnalyze 变体 + is_tool_allowed() 白名单：允许 vector_search/query_memory/search/analyze tag；禁止 shell_exec/lark_push/send_message 等执行类）
  - src/service/domain/runtime/intent_analyze.rs:Ln-Lm（parse_intent_analysis_json 6 级降级：1）正常 JSON Schema 验证 → 2）字段容错（类型转换）→ 3）取 JSON 数组第一个对象 → 4）提取第一个 JSON 对象 → 5）基于正则抽取关键字段 → 6）最终降级空结构）
  - src/service/dal/agent.rs#L1667-L1715（单元测试 build_intent_analyze_prompt_contains_sop_and_schema：包含阶段一标题 + 7 字段名 + INTENT_ANALYSIS_START 锚点）
  - src/service/domain/runtime/awakening.rs#L1322-L1355（单元测试 thinking_scene_tool_whitelist：IntentAnalyze 允许 neural/search tags、禁止 messaging/shell tags）
  - src/service/domain/system/seed/skills/TEMPLATE_COMMUNICATION/skill.md:Ln-Lm（TEMPLATE_COMMUNICATION 默认沟通技能末尾追加「理解用户消息 SOP」完整一章：五步走 + 澄清判断原则）
  - docs/design/intent_aware_two_stage_awaken_design.md
  - docs/design/runtime_design.md
  - docs/plan/唤醒上下文与睡眠约束.md（ThinkingOptions 统一选项 + PromptBuilder 公共方法复用 + 场景工具白名单过滤）
  - docs/plan/运行时问题修复.md（阶段 1 可用性修复：AOP queue ack/nack + BusyGuard RAII + try_set_busy CAS 语义）
  - docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md
  - docs/wiki/zh/content/架构设计/分层架构设计/Domain 层编排/Runtime 领域编排.md
  - docs/wiki/zh/content/基础设施/AOP 事件系统/事件消费者/Agent 循环消费者.md
  - docs/wiki/zh/content/项目概述/核心功能特性/Agent 全生命周期管理/Agent 状态管理.md
  - docs/wiki/zh/content/核心模块/处理器层/HR模块处理器/Agent处理器.md
  - 【平行卡1】Agent 思考运行时 AgentThinkRuntime：挂载清理取消与每轮快照上报 docs/wiki/knowledge/zh/Agent%20思考运行时%20AgentThinkRuntime：挂载清理取消与每轮快照上报/Agent%20思考运行时%20AgentThinkRuntime：挂载清理取消与每轮快照上报.md
  - 【平行卡2】策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 docs/wiki/knowledge/zh/策略引擎：Policy%20trait%20+%20PolicyGroup%20嵌套组合%20+%20policy_set!%20宏声明式写法/策略引擎：Policy%20trait%20+%20PolicyGroup%20嵌套组合%20+%20policy_set!%20宏声明式写法.md
---

# 两阶段唤醒（IntentAnalyze Phase1 → Awaken Phase2）

## §1 整体方案
响应消息的 Agent 唤醒流程在原单一 think loop 之前插入 **Phase 1 意图分析**小循环，先做意图识别、指代消歧、语义检索，产出结构化 7 字段 `IntentAnalysis` 结果；Phase 2 正式执行时 Prompt 注入【输入理解结果】区块，Agent 无需再花轮次理解用户消息，可以直接进入执行。

**设计核心权衡（为什么两阶段而不是让 Agent 自己处理）**：
- **降低 Agent 首几轮"先理解后执行"的轮次浪费**：无两阶段时 Agent 通常先用 2-5 轮做"查记忆 + 澄清 + 理解语义"才开始真正干活；Phase1 结果注入后 Phase2 首轮就拿结构化结果直接启动执行，整体轮次减少 15%-30%。
- **任何失败降级不阻塞主流程**：IntentAnalyze 内部 JSON 解析 6 级降级，最后最差也返回空结构，awaken 拿到 Err 或空 Some 一律按 None 处理（当成单阶段原流程），不引入稳定性风险；**澄清短路（need_clarification=true 直接回消息问）暂不启用**，稳定后再评估要不要加。
- **复用现有运行时/策略/统计基础设施**：IntentAnalyze 虽然是子流程，但同样拥有独立 `AgentThinkRuntime`（可 cancel）、独立 `policy_set!(IntentAnalyze)` 策略组、独立 `AgentLoopEvent started/finished` 事件 → stats 可分别统计 Phase1/Phase2 轮次/耗时/token，便于调优。

**Phase 1 入口与关键链路（代码级）**：
1. 入口：`awakening.rs awaken()` Phase 1 头 → 克隆 options，把 scene 强制覆盖为 `ThinkingScene::IntentAnalyze`
2. 调用：`analyze_input_intent(ctx, agent, message, &cloned_options).await`（外面包 stats 一层，里层调用 inner 实现）
3. inner 做：构造 20 条短期记忆上下文（与 awaken 同窗口）→ scene 白名单过滤技能（严格排除 messaging/project_management 执行类）→ `builder.build_intent_analyze_prompt()` 专用 Prompt → 跑 think loop（轮次 intent_analyze_max_rounds 由 Agent runtime_config > ai_orz.toml > 硬编码，典型 1~2 轮）→ 解析 Final Output 为 `IntentAnalysis`
4. JSON 解析 6 级降级链路（parse_intent_analysis_json）：
   - Level 1：完整 serde_json::from_str 按 IntentAnalysis 结构化
   - Level 2：字段级宽容（string 自动转 number/bool）
   - Level 3：{...} 包了 [{},...] 数组取第一个对象
   - Level 4：extract_first_json_object(&raw_str) 正则找最外层 {}
   - Level 5：正则抽 7 字段名（summary/summary: xxx 模式）
   - Level 6：全部失败 → 构造 IntentAnalysis { intent_type="Unknown", confidence=0.0, key_terms=[], resolutions=[], retrieved_context=[], need_clarification=false, summary="" }
5. awaken 注入 Phase1 结果：`builder.intent_analysis(&ia_ok_or_default)`；DefaultPromptBuilder 在 build() 结果【当前消息】前渲染结构化"供你参考"区块。

**PromptBuilder 两阶段专用接口与默认实现的空回退**：
- `build_intent_analyze_prompt()` → DefaultPromptBuilder 完整实现「阶段一：输入理解专用指令」（人设+技能+上下文 1-8 块复用 + 「意图识别 SOP 五步走 + 执行禁令 + 7 字段 JSON schema」指令块 + --- INTENT_ANALYSIS_START --- 锚点输出 Final）。其余 Builder（RemoteAgent/CodexCli）默认回退 build()（那些 Agent 运行在外部不做内部两阶段）。
- `intent_analysis(&IntentAnalysis)` → DefaultPromptBuilder 存 Some；build() 时渲染「【输入理解结果】intent_type=…，summary=…，key_terms=…」结构化块。其余 Builder 默认空函数体忽略注入。

**ThinkingScene::IntentAnalyze 工具白名单（scene 层是第一道闸门，Prompt 层第二道）**：
- **允许 tags**（理解类，不产生副作用）：vector_search / query_memory / search_memory / analyze_text / read_memory / search_graph / semantic_search …
- **禁止 tags**（执行类，产生副作用或外呼）：shell_exec / lark_push / send_message / task_create / project_update / file_write / http_post …
- **双层防御**：① build_scene_tool_descriptors(agent, scene) 生成 function calling 列表时白名单过滤；② DefaultPromptBuilder build_intent_analyze_prompt 的「执行禁令」段再次明确文字化约束（Agent 有时会强行在 Final 之前想调用工具，模型级约束兜底）。

**Phase 2 执行注入的"供你参考"姿态（Prompt 渲染策略）**：
- **不强制** Agent 必须基于 IntentAnalysis 结果决定行为（姿态"供你参考"，不是"强制遵循"）。原因：Phase 1 可能解析错或用户消息就是超短命令（「重启」），强行遵循反而会出错。
- **token 预算截断**：IntentAnalysis 的 retrieved_context（Phase 1 检索出的记忆引用）超过 2k 字符 → 裁掉，保留 intent_type/summary/key_terms 3 个核心字段（Phase1 检索到引用 Agent Phase2 可以再自行重新检索，不做数据搬运）。
- **渲染位置**：位于【历史对话】区块之后、【当前消息】区块之前（语义上是"对当前消息的预分析"，不是系统人设的一部分）。

**Template Communication 技能「理解用户消息 SOP」（方案 B 零代码增强）**：
- 在 `TEMPLATE_COMMUNICATION/skill.md` 默认协作技能末尾追加完整一章「理解用户消息 SOP」：五步走（1. 识别消息意图类型 Query/Command/Clarify/Report/… 2. 提取 key terms 3. 指代消歧："它/那个/上次结果" → 从历史找实体 4. 语义检索补充上下文 5. 判断 need_clarification：缺关键参数 + 上下文也找不到 → true）。
- 澄清判断原则：若 Phase 1 IntentAnalysis.need_clarification=true 且未来开启短路流程，Agent 直接回复澄清问题（不进入 Phase2）；当前未启用短路，Agent 仍会在 Phase2 自行判断是否回消息询问（两套逻辑并行，不冲突）。
- 此为独立的零代码增强（不需要 IntentAnalyze Scene 就能生效），与 Phase1 代码级实现互为"双保险"：即便未来 Phase1 流程被禁用，沟通技能文档仍驱动 Agent 在普通 think loop 中自己做以上理解步骤。

## §2 关键文件路径表格

| 文件 | 角色 | 关键结构/入口 |
|------|------|-------------|
| [awakening.rs](/src/service/domain/runtime/awakening.rs) | awaken 主入口，Phase 1 串联 + Phase 2 注入 + stats 层 | awaken() Phase 1 ~L230；RuntimeAwakening::analyze_input_intent() ~L862（AgentLoopEvent started/finished）；thinking_scene_tool_whitelist 单元测试 ~L1322 |
| [intent_analyze.rs](/src/service/domain/runtime/intent_analyze.rs) | Phase1 核心 inner 实现 + 6 级 JSON 解析降级 | analyze_input_intent_inner()；parse_intent_analysis_json()；extract_first_json_object() |
| [dal/agent.rs PromptBuilder 实现](/src/service/dal/agent.rs) | DefaultPromptBuilder build_intent_analyze_prompt() + intent_analysis() 注入 build() | build_intent_analyze_prompt() ~L1022（五步 + 禁令 + schema）；build() 渲染【输入理解结果】区块 ~L1150 左右；单元测试 ~L1667 |
| [models/prompt_builder.rs](/src/models/prompt_builder.rs) | PromptBuilder trait 声明（默认空实现） | fn build_intent_analyze_prompt() 默认回退 build()；fn intent_analysis() 默认空函数体 |
| [common enums ThinkingScene](/common/src/enums)（对应文件）| ThinkingScene::IntentAnalyze 变体 + is_tool_allowed() 白名单 | ThinkingScene enum；is_tool_allowed(&[tag]) 白名单判定 |
| [runtime/mod.rs](/src/service/domain/runtime/mod.rs) | RuntimeAwakening trait 第 4 方法签名 | analyze_input_intent 方法 + 典型调用方注释 |
| [TEMPLATE_COMMUNICATION/skill.md](/src/service/domain/system/seed/skills/TEMPLATE_COMMUNICATION/skill.md) | 方案 B 沟通技能末尾追加章节 | 末尾「理解用户消息 SOP」完整一章（五步 + 澄清判断） |
| 【① Design 1】intent_aware_two_stage_awaken_design.md | 为什么方案 A+（两阶段串联+7 字段 JSON）而非方案 B-only；澄清短路为什么暂不启用 | docs/design/intent_aware_two_stage_awaken_design.md |
| 【① Design 2】runtime_design.md v3.8 节 | awaken 整体流程架构：wake_brain → Phase1 IA → Phase2 执行 → 压缩循环 → Summary 沉淀 | docs/design/runtime_design.md |
| 【③ Wiki 长文 1】运行时领域.md | awaken 子流程说明 | docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md |
| 【③ Wiki 长文 2】Runtime 领域编排.md | Domain 层四方法编排入口（wake/awaken/sleep/analyze）| docs/wiki/zh/content/架构设计/分层架构设计/Domain%20层编排/Runtime%20领域编排.md |
| 【③ Wiki 长文 3】Agent 循环消费者.md | consumer 中如何串联 awaken 两阶段 + 消息入站场景 | docs/wiki/zh/content/基础设施/AOP%20事件系统/事件消费者/Agent%20循环消费者.md |
| 【③ Wiki 长文 4】Agent 状态管理.md | Idle→Busy 触发 awaken 的状态转换说明 | docs/wiki/zh/content/项目概述/核心功能特性/Agent%20全生命周期管理/Agent%20状态管理.md |
| 【平行卡】AgentThinkRuntime 挂载清理取消 | IA Phase1 独立 think_runtime | 见本卡 source_files 尾平行卡1 绝对路径 |
| 【平行卡】策略引擎 Policy trait + policy_set! | IA 场景专用 policy_group | 见本卡 source_files 尾平行卡2 绝对路径 |

## §3 架构约定

1. **IntentAnalysis 永远不阻塞主流程**：任何错误（Prompt 构建失败、LLM 无响应、JSON 全 6 级都解析不出来）awaken 端都用 `match result { Ok(ia) => Some(ia), Err(_) => None }` 降级为 None，原单阶段流程照常工作。这是本设计第一红线。
2. **Phase 1 绝不允许任何执行类工具**：ThinkingScene 白名单 + Prompt 禁令段双层防御。哪怕 PromptBuilder 被改漏，白名单层也会阻止 function calling 暴露 shell_exec 给 LLM。绝对不允许"意图分析阶段不小心调了发送消息/执行命令"。
3. **IntentAnalysis 7 字段是跨版本契约**：字段名（intent_type/confidence/key_terms/resolutions/retrieved_context/need_clarification/summary）= 前后端 + PromptBuilder + 6 级解析函数四方共识；新增字段必须同时改这四方。不得把字段重命名（例如 confidence→certainty 直接破坏解析与注入 4 方联动）。
4. **Phase 1 的"检索结果搬运"不跨阶段传大文本**：IntentAnalysis.retrieved_context Phase2 渲染超 2k 截断；Phase2 需要上下文请自行调用记忆工具重新检索（避免 Phase1 拉了大段内容 Phase2 不需要反而超 token）。
5. **Template Communication SOP 章节（方案 B）与代码级 Phase1（方案 A+）并行双保险**：不要删除 skill.md 中的理解 SOP 章。即便 Phase1 未来要重构或降级，沟通技能文档仍能驱动 Agent 在普通 think loop 中自己按五步理解消息。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止 awaken 中对 Phase1 analyze_input_intent 结果 ? 向上冒泡**。错误直接吞掉降级 None；用 ? 会把"意图分析服务不可用"升级为"整次唤醒失败"，用户得不到任何响应，P0 故障。
2. ❌ **禁止 ThinkingScene::IntentAnalyze 的 is_tool_allowed 白名单包含执行类 tag**（send_message、lark_push、shell_exec、task_create、file_write 等）。反例：把 send_message 误标为"理解类"→ IA 阶段 Agent 以为自己在澄清就真发出去了 → 用户反复收到两次"澄清消息"（IA + Awaken 各一次），严重用户干扰。
3. ❌ **禁止 PromptBuilder 的 build_intent_analyze_prompt() 默认实现写 Phase1 专用指令**。DefaultPromptBuilder 之外的实现（RemoteAgent/CodexCli 等外部 Agent）默认回退 build()，不支持内部两阶段（他们的 think 在外部）。内部两阶段只能 DefaultPromptBuilder 启用。
4. ✅ **INTENT_ANALYSIS_START 锚点必须存在于 Prompt 末尾**：6 级 JSON 解析的 Level 4（extract_first_json_object）靠定位此锚点后提取 JSON，若删除锚点会退化为扫全文找 {}（容易误匹配到 Prompt 示例代码的 JSON）。
5. ✅ **IntentAnalysis.need_clarification=true 必须只做参考、当前不短路 Phase2**：短路是下一轮迭代功能（需要用户交互确认机制），提前启用会导致"Agent 每次都先问一句澄清再执行"= 响应变慢一倍。
6. ✅ **四类互引闭环**：本卡 source_files[] 含 4 篇 wiki 长文（运行时领域/Runtime 编排/Agent 循环消费者/Agent 状态管理）+ 2 Design + Plan 占位 + 2 平行卡（AgentThinkRuntime/策略引擎）；对应 Wiki 长文 cite 段回链本卡 + 2 份 Design + 平行卡。
