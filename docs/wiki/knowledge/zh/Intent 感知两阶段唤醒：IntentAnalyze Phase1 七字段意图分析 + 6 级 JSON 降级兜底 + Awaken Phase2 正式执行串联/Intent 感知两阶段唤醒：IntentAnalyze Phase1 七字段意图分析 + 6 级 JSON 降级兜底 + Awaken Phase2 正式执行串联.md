---
kind: rag_card
name: Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2 正式执行串联
category: 领域建模
scope:
  - "src/service/domain/runtime/**/*.rs"
  - "src/service/domain/runtime/*.rs"
  - "src/models/prompt_builder.rs"
  - "src/service/dal/agent.rs"
  - "common/src/enums/*.rs"
  - "common/src/api/runtime.rs"
  - "src/consumer/message.rs"
source_files:
  - "src/service/domain/runtime/types.rs#L178-L195"
  - "src/service/domain/runtime/intent_analyze.rs#L1-L90"
  - "src/service/domain/runtime/intent_analyze.rs#L27-L80"
  - "src/service/domain/runtime/awakening.rs#L150-L180"
  - "src/service/domain/runtime/awakening.rs#L230-L280"
  - "src/service/domain/runtime/awakening.rs#L862-L908"
  - "src/service/domain/runtime/awakening.rs#L1322-L1355"
  - "src/service/domain/runtime/mod.rs#L203-L225"
  - "src/service/domain/runtime/think_loop.rs#L117-L160"
  - "src/service/dal/agent.rs#L1022-L1080"
  - "src/service/dal/agent.rs#L1667-L1715"
  - "src/models/prompt_builder.rs#L104-L123"
  - "src/service/domain/system/seed/skills/TEMPLATE_COMMUNICATION/skill.md"
  - "docs/archive/design-archive/intent_aware_two_stage_awaken_design.md"
  - "docs/design/runtime_design.md"
  - "docs/archive/plan-archive/唤醒上下文与睡眠约束.md"
  - "docs/archive/plan-archive/运行时问题修复.md"
  - "docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md"
  - "docs/wiki/zh/content/架构设计/分层架构设计/Domain%20层编排/Runtime%20领域编排.md"
  - "docs/wiki/zh/content/基础设施/AOP%20事件系统/事件消费者/Agent%20循环消费者.md"
  - "docs/wiki/zh/content/项目概述/核心功能特性/Agent%20全生命周期管理/Agent%20状态管理.md"
  - "docs/wiki/zh/content/核心模块/处理器层/HR模块处理器/Agent处理器.md"
---

# §1 概述（一句话定位 + 解决什么问题）

**定位**：Agent 唤醒流程的「先理解再执行」两阶段串联——Phase1 `IntentAnalyze` 专用子循环（神经+记忆+检索工具白名单，最多 2 轮 think loop）产出结构化 7 字段 `IntentAnalysis`，Phase2 `Awaken` 把理解结果渲染成 Prompt 【输入理解结果】区块后再进入正式干活；含 6 级 JSON 解析降级兜底绝不阻塞主流程。

**解决三类存量痛点**（对应 Design §1.1）：
1. 短消息/指代密集型输入无显式意图识别 → 消歧失败直接答偏
2. Agent 是否调用 `search_memory` 全凭自觉 → 跨任务跨天上下文失焦
3. 系统层无指代消歧辅助链路 → "上次那个方案" 类语句完全依赖 20 条历史窗口运气

---

# §2 关键文件与核心锚点速查表

| 文件锚点（点击跳转） | 角色 | 核心契约 / 红线 |
|---------------------|------|-----------------|
| [IntentAnalysis 7 字段定义](src/service/domain/runtime/types.rs#L178-L195) | **结构体契约（SSOT）** | intent_type / confidence / key_terms / resolutions / retrieved_context / need_clarification / summary；Agent 自由填值不做强枚举，解析失败退化为 `Default::default()` |
| [Phase1 analyze_input_intent_inner 实现](src/service/domain/runtime/intent_analyze.rs#L27-L80) | Phase1 子循环入口 | ThinkingScene=IntentAnalyze；工具白名单 neural/memory/query/search；最多 2 轮；Final 解析为 JSON |
| [RuntimeDomain trait 方法签名](src/service/domain/runtime/mod.rs#L205-L225) | 领域层对外契约 | `analyze_input_intent(ctx, agent, message, options) -> Result<IntentAnalysis>`；可独立复用于：消息路由预分析 / 澄清追问 / Agent 协作消息分发 |
| [Phase2 awaken 两阶段串联](src/service/domain/runtime/awakening.rs#L150-L180) | 正式执行入口 | awaken 内部先调 analyze_input_intent → need_clarification 先渲染 Prompt → 正式 think loop；【输入理解结果】区块明确标注「仅供参考，不一致可推翻」 |
| [共享 run_think_loop 引擎](src/service/domain/runtime/think_loop.rs#L117-L160) | 两阶段复用的思考内核 | 超时控制 / 多轮迭代 / 策略评估 / 工具调用分发；Phase1 与 Phase2 走同一内核仅 scene 参数不同 |
| [两阶段唤醒 Design 总纲](docs/archive/design-archive/intent_aware_two_stage_awaken_design.md) | 为什么 / 决策表 | §关键决策表 §4.2 降级兜底表 §5 非目标边界 |
| [唤醒上下文 Plan 落地快照](docs/archive/plan-archive/唤醒上下文与睡眠约束.md) | 怎么做 + 结果 | ThinkingOptions 统一参数 / PromptBuilder 公共方法复用 |
| [运行时领域 Wiki 长文](docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md) | 人类百科 | §5 两阶段唤醒流程详细说明 §8 故障排查 |

---

# §3 架构约定与数据流（业务语义层面，不贴实现代码）

**数据流总览**：
```
外部消息 → MessageConsumer
  → Phase1 IntentAnalyze 子循环
     ThinkingScene=IntentAnalyze
     工具白名单：neural/memory/query/search（严格禁止 messaging/project_management/external_agent）
     Prompt：build_intent_analyze_prompt（SOP 五步走 + JSON Schema 约束）
     输出：IntentAnalysis {7 字段}
  → need_clarification 非空？→ 未来可短路澄清（当前仅渲染提示，Agent 自行决定何时问）
  → Phase1.5：PromptBuilder.intent_analysis() 渲染【输入理解结果】区块
  → Phase2 Awaken 正式干活
     ThinkingScene=Awaken（所有工具可用）
     Prompt 新增【输入理解结果】区块（明确写「仅供参考，不一致可推翻」）
     进入正常 think loop
```

**6 级 JSON 降级兜底策略**（对应 Design §5.2）：
| 级别 | 故障场景 | 降级行为 |
|------|---------|---------|
| Level 1 | 完美路径 | Final 纯 JSON → 直接 serde_json 反序列化 |
| Level 2 | JSON 包裹 ```json``` 代码块 | 正则剥离三引号后反序列化 |
| Level 3 | JSON 前后有多余文本 | 提取首个 { ... } 完整块反序列化 |
| Level 4 | 单字段解析失败 | 对应字段用 Default，其余保留 |
| Level 5 | 整体反序列化失败 | summary 字段填原文本全文，其余 6 字段 Default |
| Level 6 | think loop 超限/抛错 | Result 捕获 warn，awaken 正常进入 Phase2（无增强区块），等价于单阶段流程 |

**三条关键行为红线**（§4 必守，回归必保）：
1. **IntentAnalyze 严格不允许执行类工具**：`is_tool_allowed` 匹配 neural/memory/query/search 任一 tag 才放行；禁止任何 messaging/project_management/external_agent 类工具
2. **理解结果仅供参考原则**：Phase2 Prompt 渲染必须写入「以下是你上一阶段自己得出的结论，如与当前判断不一致可以推翻」，避免 Agent 被错误前置理解带偏
3. **降级绝不阻塞主流程**：任何 Level 4-6 失败场景，只打 warn 日志 + 退化为 Default/单阶段，绝不返回 Err 中断 awaken 链路

---

# §4 硬约束 / 必守红线 / 扩展入口

**§4.1 必守红线（8 条，违反 = FAIL）**

| # | 红线 | 验证方式 | 代码锚点 |
|---|------|---------|---------|
| 1 | IntentAnalysis 字段**绝对不强枚举** intent_type，允许 Agent 自由字符串扩展；解析失败用 Default | grep `enum IntentType` 应不存在；结构体 derive(Default) | [types.rs#L178-L195](src/service/domain/runtime/types.rs#L178-L195) |
| 2 | Phase1 think loop 轮次**最多 2 轮**，超过强制截断 Final；不得与 Phase2 共享同一 think_runtime | options.max_rounds 上限；AgentThinkRuntime 独立注册清理 | [intent_analyze.rs#L40-L60](src/service/domain/runtime/intent_analyze.rs#L40-L60) |
| 3 | `need_clarification` 字段**当前阶段绝不做短路 send_message**，仅渲染为 Phase2 Prompt 提示；短路机制为 P4 可选未来路径 | grep `send_message.*clarification` 在 awaken.rs 应为零 | [awakening.rs#L200-L240](src/service/domain/runtime/awakening.rs#L200-L240) |
| 4 | retrieved_context 字段渲染到 Prompt 时**最多 10 条，每条最多 200 字**；Token 安全网硬编码截断 | DefaultPromptBuilder.intent_analysis() 实现 | [dal/agent.rs](src/service/dal/agent.rs) intent_analysis 方法 |
| 5 | 降级 Level 5/6 必须打 `log_warn!(&ctx, "intent_analyze_degrade", ...)` 含降级级别与原始 snippet 前 100 字 | grep `log_warn.*intent_analyze_degrade` | [intent_analyze.rs#L70-L80](src/service/domain/runtime/intent_analyze.rs#L70-L80) |
| 6 | 两阶段唤醒的 Phase1 / Phase2 **必须复用同一 run_think_loop 引擎**，仅 scene 参数不同；禁止复制粘贴两套 think loop 逻辑 | run_think_loop 仅一处定义 | [think_loop.rs#L117-L160](src/service/domain/runtime/think_loop.rs#L117-L160) |
| 7 | analyze_input_intent 作为通用方法**必须可独立被外部调用**（消息预路由 / 澄清重理解等），不能与 awaken 内部私有耦合；trait 定义在 RuntimeDomain 总接口 | `pub async fn analyze_input_intent` 在 trait 层公开 | [mod.rs#L205-L225](src/service/domain/runtime/mod.rs#L205-L225) |
| 8 | 方案 B（技能 SOP 版理解流程）与方案 A+（本卡 IntentAnalyze）**SOP 核心逻辑一致但强度不同**：Prompt 约束描述不得前后矛盾；Agent 读不到两个互相打架的方法论 | 对照 Design §5.3 分工边界表 | [intent_aware_two_stage_awaken_design.md §5.3](docs/archive/design-archive/intent_aware_two_stage_awaken_design.md#L410-L422) |
| 9 | **禁止 awaken 对 Phase1 analyze 结果 `?` 冒泡**：错误吞掉降级 None；用 `?` 会把意图分析服务不可用升级为整次唤醒失败，P0 故障 | 集成测试模拟 cortex panick → awaken 仍返回 200 且正常响应 | [awakening.rs awaken Phase1 串联 match 分支](src/service/domain/runtime/awakening.rs#L230-L280) |
| 10 | **禁止 IntentAnalyze 的 is_tool_allowed 白名单包含执行类 tag**（send_message / lark_push / shell_exec / task_create / file_write）| thinking_scene_tool_whitelist 单元测试：IntentAnalyze 允许 neural/search tags、禁止 messaging/shell tags | [awakening.rs#L1322-L1355](src/service/domain/runtime/awakening.rs#L1322-L1355) |
| 11 | **PromptBuilder 默认实现不写 Phase1 专用指令**：外部 Agent（RemoteAgent/CodexCli）默认回退 build()，内部两阶段仅 DefaultPromptBuilder 完整实现 build_intent_analyze_prompt() | grep 其他 PromptBuilder 实现，不应出现 Phase1 专用指令字符串 | [models/prompt_builder.rs#L104-L123](src/models/prompt_builder.rs#L104-L123) trait 默认实现 |
| 12 | **INTENT_ANALYSIS_START 锚点必须存在于 Prompt 末尾**：6 级 JSON 解析 Level 4 靠此锚点定位后提取 JSON，删除则误匹配 Prompt 示例代码 {} 的概率极高 | build_intent_analyze_prompt 单元测试：含 INTENT_ANALYSIS_START 字符串 | [dal/agent.rs#L1667-L1715](src/service/dal/agent.rs#L1667-L1715) |
| 13 | **need_clarification=true 只做参考、当前不短路 Phase2**：短路是下一迭代功能；提前启用会导致每次都先问一句澄清再执行，响应翻倍 | awaken 入口 grep 不应有 send_message.*clarification 的短路 if 分支 | [awakening.rs awaken 入口 if need_clarification 检查位置](src/service/domain/runtime/awakening.rs#L200-L240) |
| 14 | **Template Communication SOP 章节（方案 B）与代码级 Phase1（方案 A+）并行双保险**：不要删除 skill.md 末尾的「理解用户消息 SOP」章节；即便 Phase1 流程被禁用，沟通技能也能驱动 Agent 在普通 think loop 中自行按五步理解 | TEMPLATE_COMMUNICATION/skill.md grep 「SOP」命中 | [TEMPLATE_COMMUNICATION skill.md 末尾章节](src/service/domain/system/seed/skills/TEMPLATE_COMMUNICATION/skill.md) |

**§4.2 扩展入口速查（按 4 步模板）**

| 扩展需求 | 改动位置（N 处同步） | 参考锚点 |
|---------|---------------------|---------|
| 新增 IntentAnalysis 字段（如 `ambiguity_level`） | ① types.rs struct 追加字段 → ② build_intent_analyze_prompt JSON Schema 追加 → ③ intent_analyze.rs serde 反序列化（兼容旧字段 Default）→ ④ PromptBuilder.intent_analysis() 渲染追加 → ⑤ 降级兜底 Default 兼容 | [types.rs#L178-L195](src/service/domain/runtime/types.rs#L178-L195) |
| Phase1 强制搜索策略更激进（例如必须调 recommend_seed_nodes + traverse 1 跳） | build_intent_analyze_prompt 的 Step 4 SOP 文本修改；降级兜底保持不变（Agent 不执行只是 retrieved_context 为空） | intent_analyze Prompt SOP Step 4 位置 |
| 短路澄清机制上线（need_clarification 非空直接发消息，不进入 Phase2） | awaken() 入口追加 if 分支；send_message 走与正常 Agent 调 send_message 同一通道；状态机正确回到 Idle | [awakening.rs awaken 方法入口](src/service/domain/runtime/awakening.rs#L150-L180) |
| 新增 ThinkingScene（如 CodeReview 专用前置分析场景） | ① ThinkingScene 枚举追加变体 → ② is_tool_allowed 追加分支 → ③ 对应专用 build_xxx_prompt → ④ RuntimeAwakening trait 新增通用复用方法 → ⑤ awaken 内部按需串联前置阶段 | common/src/enums/thinking_scene.rs + awakening.rs is_tool_allowed match |
