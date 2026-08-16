> 📦 归档标记（2026-08-16）：归档冻结。保留原因：两阶段唤醒：IntentAnalyze+Awaken正式执行 功能已完成并通过验收，文档转为历史快照。生效方案：见源码和 wiki 长文。

🎯定位：Agent 唤醒流程改造为「两阶段」：先 IntentAnalyze 意图识别+消歧+检索，再 Awaken 正式执行；同时新增协作沟通技能「理解用户消息 SOP」整章
状态：v1.0（2026-08-16，落地快照）
触发场景：用户消息含大量指代消歧、跨任务上下文、短消息语义不清导致 Agent 误执行；需要前置理解阶段兜底
关联文档段：
- 对应 design 文档：[docs/archive/design-archive/intent_aware_two_stage_awaken_design.md](docs/archive/design-archive/intent_aware_two_stage_awaken_design.md)
- Wiki 长文真实路径：[docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md](docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md)（两阶段唤醒章节）
- RAG 卡真实路径：[docs/wiki/knowledge/zh/Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2 正式执行串联/Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2 正式执行串联.md](docs/wiki/knowledge/zh/Intent%20感知两阶段唤醒：IntentAnalyze%20Phase1%20七字段意图分析%20+%206%20级%20JSON%20降级兜底%20+%20Awaken%20Phase2%20正式执行串联/Intent%20感知两阶段唤醒：IntentAnalyze%20Phase1%20七字段意图分析%20+%206%20级%20JSON%20降级兜底%20+%20Awaken%20Phase2%20正式执行串联.md)

---

## 一、目标

| 问题 | 方式 |
|------|------|
| 用户消息指代密集（这/那/上次/那个），Agent 直接执行易误解 | 新增 IntentAnalyze 场景，专门做意图识别+消歧+检索，不执行业务动作 |
| Prompt 层缺少统一的理解方法论 SOP | 方案 B：协作沟通技能（TEMPLATE_COMMUNICATION）末尾追加「理解用户消息 SOP」整章 |
| 理解结果无法传递给正式执行阶段 | 新增 IntentAnalysis 7 字段结构体，analyze_input_intent() 输出后注入 PromptBuilder |
| 解析失败会阻塞主流程 | 6 级 JSON 降级兜底：整段 JSON→代码块包裹→括号匹配提取→Value 宽容转换→原文 summary |
| 工具越权：理解阶段误发消息/写状态 | ThinkingScene::IntentAnalyze 工具白名单只允许 neural/memory/query/search，禁止 messaging/project_management/external 等 |

收敛一句话：方案 B 技能 SOP + 方案 A+ 两阶段唤醒（IntentAnalyze 前置 + Awaken 正式）+ 6 级 JSON 降级兜底，短消息指代场景理解准确率提升，执行阶段零阻塞。

---

## 二、架构思路

```
awaken() 流程：
┌─────────────────────────────────────────────────────────────────┐
│ 原流程：查最近 20 条历史 → 构造 PromptBuilder → build() → 思考  │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                     新增：阶段 1
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│ analyze_input_intent(ctx, agent, message, options_cloned)        │
│   ├─ options.scene 强制覆盖为 IntentAnalyze                      │
│   ├─ build_base_prompt_builder（与正式阶段同背景知识）            │
│   ├─ build_intent_analyze_prompt（五步走 SOP + JSON 约束）       │
│   ├─ wake_agent_brain(scene=IntentAnalyze, 工具白名单生效)       │
│   └─ parse_intent_analysis_json() 6 级降级 → IntentAnalysis     │
│            ↓                                                     │
│   失败降级：log_warn + IntentAnalysis::default()，不阻塞         │
└──────────────────────────┬──────────────────────────────────────┘
                           │ （P4 澄清短路：验证稳定后解锁，当前只留注释入口）
                           ▼
                     builder.intent_analysis(&ia)
                           │
                     原下一步：build()
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│ DefaultPromptBuilder.build()                                     │
│   └─ 在【当前消息】区块之前渲染【输入理解结果】结构化区块         │
│      （姿态："供你参考，若与你判断不一致以你为准"）               │
│      （截断：每条最多 5 条 / 单条 200 字，防 token 溢出）         │
└─────────────────────────────────────────────────────────────────┘
```

技能 SOP 独立生效：即使系统未触发 IntentAnalyze 阶段，Agent 人设中也有「理解用户消息 SOP」方法论作为兜底。

---

## 三、涉及文件清单

| 层次 | 文件路径 | 职责 |
|------|----------|------|
| 技能 seed | [src/service/domain/system/seed/skills/TEMPLATE_COMMUNICATION/skill.md](src/service/domain/system/seed/skills/TEMPLATE_COMMUNICATION/skill.md) | 方案 B：追加「理解用户消息 SOP」整章（五步走 + 澄清判断 + 与系统阶段关系说明） |
| domain 层枚举 | [src/service/domain/runtime/awakening.rs](src/service/domain/runtime/awakening.rs) | ThinkingScene 新增 IntentAnalyze 变体 + is_tool_allowed 白名单（neural/memory/query/search） |
| domain 层结构体 | [src/service/domain/runtime/awakening.rs](src/service/domain/runtime/awakening.rs) | IntentAnalysis 7 字段（intent_type/confidence/key_terms/resolutions/retrieved_context/need_clarification/summary） |
| domain trait | [src/service/domain/runtime/mod.rs](src/service/domain/runtime/mod.rs) | RuntimeAwakening trait 新增 analyze_input_intent 方法签名 |
| model trait | [src/models/prompt_builder.rs](src/models/prompt_builder.rs) | PromptBuilder trait 新增 build_intent_analyze_prompt() + intent_analysis() 注入方法 |
| domain 实现 | [src/service/domain/runtime/awakening.rs](src/service/domain/runtime/awakening.rs) | analyze_input_intent() 实现 + parse_intent_analysis_json() 6 级降级 + extract_first_json_object() 括号匹配 |
| dal 实现 | [src/service/dal/agent.rs](src/service/dal/agent.rs) | DefaultPromptBuilder 实现 build_intent_analyze_prompt（SOP 指令块 + JSON schema 约束）+ render_intent_analysis_section + build() 渲染区块 |
| domain 串联 | [src/service/domain/runtime/awakening.rs](src/service/domain/runtime/awakening.rs) | awaken() 在构造 builder 之后、build() 之前插入 analyze_input_intent + builder.intent_analysis 注入 |
| 集成测试 | [tests/intent_analyze_two_stage.rs](tests/intent_analyze_two_stage.rs) | analyze_input_intent 集成测试 + PromptBuilder 渲染字符串断言 |

⭐ **落地索引（四类互引）**
- Wiki 长文：[docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md](docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md)
- RAG 卡：[docs/wiki/knowledge/zh/Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2 正式执行串联/Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2 正式执行串联.md](docs/wiki/knowledge/zh/Intent%20感知两阶段唤醒：IntentAnalyze%20Phase1%20七字段意图分析%20+%206%20级%20JSON%20降级兜底%20+%20Awaken%20Phase2%20正式执行串联/Intent%20感知两阶段唤醒：IntentAnalyze%20Phase1%20七字段意图分析%20+%206%20级%20JSON%20降级兜底%20+%20Awaken%20Phase2%20正式执行串联.md)

---

## 四、分发点速查表

| 分发场景 | 入口 | IntentAnalyze 是否参与 |
|----------|------|----------------------|
| 用户消息正常唤醒 | RuntimeAwakeningImpl::awaken() | 是，前置阶段 1 |
| 沉淀（sleep_and_settle） | RuntimeAwakeningImpl::sleep_and_settle() | 否，Settle 场景专注记忆沉淀 |
| 超时总结（awaken_for_summary） | RuntimeAwakeningImpl::awaken_for_summary() | 否，Summary 场景专注退出 |
| Agent 人设 SOP 生效（纯 Prompt 层） | 任何调用 DEFAULT_COMMUNICATION skill 的场景 | 方案 B 独立生效，不依赖代码路径 |
| 外部消息入站预分析（未来） | 入站适配器路由之前 | 可复用 analyze_input_intent 做前置路由判断（当前未启用） |
| P4 澄清短路（未来） | awaken() need_clarification 非空分支 | 验证稳定后解锁，当前仅注释入口 |

---

## 五、验收清单

| 验收项 | 结果 |
|--------|------|
| TEMPLATE_COMMUNICATION skill.md 末尾追加「理解用户消息 SOP」整章（五步走 + 澄清判断 + 与系统阶段关系）渲染正确 | ✓ 已落地 |
| ThinkingScene::IntentAnalyze 变体 + is_tool_allowed 工具白名单（严格禁止 messaging/project_management/external） | ✓ 已落地 |
| IntentAnalysis 7 字段结构体 + JSON roundtrip 单元测试（含部分缺省反序列化） | ✓ 已落地 |
| RuntimeAwakening trait 新增 analyze_input_intent 签名 + DefaultPromptBuilder 对称新增 Prompt 方法 | ✓ 已落地 |
| analyze_input_intent 完整实现：构造专用 options + Prompt + wake_agent_brain + 6 级 JSON 降级解析，任何失败降级为空结构不阻塞 | ✓ 已落地 |
| awaken() 两阶段串联：阶段 1 analyze → 注入 builder → 阶段 2 build 正式思考 | ✓ 已落地 |
| build_intent_analyze_prompt 完整 Prompt：SOP 五步走 + 执行禁令 + JSON schema 输出约束 | ✓ 已落地 |
| build() 【输入理解结果】区块渲染：姿态"供你参考" + 5 条/200 字截断规则 + intent_analysis 为空不渲染 | ✓ 已落地 |
| 单元测试：白名单测试 + JSON roundtrip + 部分缺省 + parse/extract 辅助 UT 全部 PASS | ✓ 已落地 |
| 配套增强（计划外落地）：轮次/超时配置化（Agent runtime_config > ai_orz.toml > 硬编码）、AgentRuntimeConfigInfo 嵌套结构 | ✓ 已落地 |
| 四类互引占位路径已写入 | ✓ |

---

## 六、执行结果摘要

| 指标 | 值 |
|------|----|
| 修改文件数 | 8 个（skill md 1 / awakening.rs 2 次+ / mod.rs / prompt_builder.rs / agent.rs / 集成测试 1） |
| 新增代码行（约） | 1200 行（SOP md 约 150 行 / 骨架代码约 200 行 / analyze 实现约 350 行 / Prompt 构建约 300 行 / 渲染约 100 行 / 测试约 100 行） |
| 降级保障层级 | 6 级（a 整段 JSON → b 代码块提取 → c 括号匹配完整对象 → d Value 宽容类型转换 → e 仅 summary=原文 → f default 空结构） |
| 单元测试新增 | 5 个 UT + 集成测试若干 |
| clippy 零警告 | backend + frontend 均通过 |
| 全 workspace 测试通过 | 1101+ 测试 100% PASS |
| 四类互引覆盖率 | design 1/1 + wiki 1/1 + RAG 1/1 = 100% |

---

## 七、后续扩展路径

1. **common 层**：P4 澄清短路正式解锁——need_clarification 非空时 awaken 不进入正式阶段，直接通过 message_domain 发澄清消息给用户并短路返回，节省一轮正式思考 token。
2. **domain 层**：IntentAnalysis 结果持久化到短期记忆（Working Memory），同类 FollowUp 消息可直接复用上次消歧结果，避免重复分析。
3. **handler 层**：新增 /analyze 调试接口，前端发送消息前可先手动触发 analyze_input_intent 预览理解结果，开发者调试意图分析准确率。
4. **前端**：聊天页消息气泡右上角新增「AI 理解标签」微图标（根据 intent_type 显示 Question/TaskRequest/Chat 等彩色徽标），点击展开 IntentAnalysis 详情抽屉。