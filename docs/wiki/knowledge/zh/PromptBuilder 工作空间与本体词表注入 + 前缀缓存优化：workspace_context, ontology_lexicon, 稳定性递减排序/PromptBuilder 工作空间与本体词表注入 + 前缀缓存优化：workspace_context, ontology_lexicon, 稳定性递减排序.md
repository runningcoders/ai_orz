---
kind: wiki_knowledge_card
name: PromptBuilder 工作空间与本体词表注入 + 前缀缓存优化：workspace_context, ontology_lexicon, 稳定性递减排序

category: Prompt 构建与上下文注入
scope:
- src/models/prompt_builder.rs
- src/models/cortex_types.rs
- src/service/dal/agent/**
- src/service/domain/runtime/compaction.rs
- src/service/domain/runtime/awakening.rs
- src/service/domain/runtime/intent_analyze.rs
- src/service/domain/runtime/think_loop.rs
- src/models/project.rs
- src/models/task.rs
source_files:
- src/models/prompt_builder.rs#L41-L108
- src/models/prompt_builder.rs#L135-L146
- src/models/cortex_types.rs#L135-L195
- src/service/dal/agent/builder/default.rs#L1-L60
- src/service/dal/agent/builder/default.rs#L145-L230
- src/service/dal/agent/builder/default.rs#L248-L335
- src/service/dal/agent/builder/default.rs#L1055-L1234
- src/service/domain/runtime/compaction.rs#L1-L50
- src/service/domain/runtime/awakening.rs#L104-L145
- src/service/domain/runtime/awakening.rs#L343-L393
- src/service/domain/runtime/intent_analyze.rs#L124-L157
- src/models/project.rs#L199-L245
- src/models/task.rs#L216-L245
- docs/wiki/zh/content/架构设计/分层架构设计/Domain 层编排/Runtime 领域编排.md
- docs/wiki/zh/content/基础设施/存储系统/本地文件安全与工作区隔离.md
- docs/wiki/knowledge/zh/本体词表漂移治理与 Prompt 注入：OntologyDal load_lexicon_summary + common 纯函数解析 + 写后认证 + DuckDB 漂移记账/本体词表漂移治理与 Prompt 注入：OntologyDal load_lexicon_summary + common 纯函数解析 + 写后认证 + DuckDB 漂移记账.md
- docs/wiki/knowledge/zh/用户维度工作区路径改造 + 工具调用安全边界检查：路径逃逸 跨用户访问 相对路径阻断/用户维度工作区路径改造 + 工具调用安全边界检查：路径逃逸 跨用户访问 相对路径阻断.md

---

# PromptBuilder 工作空间与本体词表注入 + 前缀缓存优化

## §1 整体方案

为 PromptBuilder trait 新增 workspace_context 方法，将 Agent 工作空间路径信息注入 Prompt 中，帮助 Agent 明确文件操作的路径边界。

**95a0b1bf 重构**：PromptBuilder trait 新增 3 个默认方法扩展 Agent 上下文注入能力：`compacted_context`（注入 compaction 压缩后的近期上下文摘要）、`settled_reference`（注入长期沉淀知识图谱的摘要引用）、`past_memories_reference`（注入短期/长期记忆引用索引）；同时新增 `build_initial_messages / build_sleep_initial_messages / build_summary_initial_messages / build_intent_analyze_initial_messages` 四个构建方法，输出 `Vec<ChatMessage>` 而非扁平字符串——第一次让 Prompt 按 System / User 消息角色分层传递给模型（Chat Completions API 规范），告别人设/指令/对话内容全挤 User 角色的混乱状态。

同一轮重构还在 `cortex_types.rs` 中新增 `ChatMessage::System { content }` 变体 + `system()` 构造器 + `to_summary_text` 摘要处理，以及 DefaultPromptBuilder 中 `build_final_response_guidance` 方法——把【回复规则 §0-§5】（先审题再回答 / 何时直接回复 / send_message 正确用途 / 闲聊豁免 / 检索空结果 / 禁止无意义工具调用）写入 System 消息尾部，彻底解决 Agent 误以为必须调用 send_message 才能结束任务的心理陷阱。

**本体词表注入 + 区块重排（commit 5b52c72c → 08c3e720）**：PromptBuilder trait 新增 `ontology_lexicon(&common::ontology::OntologyLexiconSummary)` 默认方法，DefaultPromptBuilder 实现为 `build_lexicon_section()` 渲染【本体词表】区块（预算 3000 chars，裁剪优先级：关系词 > 实体类 > 同义样例）。区块从原来的 8 块重排为 11 块，按**稳定性递减排序**：人设 → 神经技能 → 必加载技能 → **本体词表**（跨会话最稳定） → 用户画像 → 项目/任务上下文 → 工作空间路径约定 → 历史对话（追加式，前缀稳定） → 输入理解结果/消息链上下文 → 工具失败警告 → **trace_id + 当前消息**（每次变化）。

**Prompt 前缀缓存优化（commit 14a4b688 + 08c3e720）**：核心原则——"稳定内容不被易变内容切割"。三项关键修复：
1. **绝对时间替代相对时长**：Project/Task 的 `start_at` / `last_followup_at` 从 `relative_duration("5 小时前")` 改为 `format_datetime("2026-09-15 10:30")`——相对时长随当前时刻漂移（分钟级变化），导致区块字节级变化作废前缀缓存；绝对时间仅在数据本身变更时变化
2. **场景 prompt 静态指令块前置 + 易变数据收尾**：`build_sleep_prompt` / `build_summary_prompt` 原来把 Trace ID / 轮次数 / 摘要 / trace_ids 等易变数据嵌入指令块中间，导致指令块被切割后其后全部静态内容（约束/步骤/认知要点）都无法命中前缀缓存。重构后：指令块完全静态、易变数据（Trace/待沉淀记忆/trace_ids）统一收尾；指令用"见文末【XXX】"引用尾部数据
3. **initial-messages 缓存友好注释**：三个 `*_initial_messages` 变体（已是优序）补上防回退注释——System 消息跨次运行完全静态，后续新增区块必须保持"静态前、易变后"，禁止插入到 System 中间

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构 |
|------|------|----------|
| [src/models/prompt_builder.rs](src/models/prompt_builder.rs) | PromptBuilder trait | workspace_context 方法声明（L61-L90）；**ontology_lexicon() 新增**（L135-L146）默认实现为空操作；trait 不依赖 config/paths/RequestContext |
| [src/service/dal/agent/builder/default.rs#L1-L60](src/service/dal/agent/builder/default.rs#L1-L60) | 区块重排总览（build()） | **11 块稳定性递减排序**：人设 → 神经技能 → 必加载技能 → 本体词表 → 用户画像 → 项目/任务 → 工作空间 → 历史对话 → 输入理解/消息链 → 工具失败警告 → trace+当前消息 |
| [src/service/dal/agent/builder/default.rs#L145-L230](src/service/dal/agent/builder/default.rs#L145-L230) | build_lexicon_section | **本体词表区块渲染**：预算 3000 chars；裁剪优先级关系词 > 实体类 > 同义样例；None/空词表 → 跳过注入（可选增强优雅降级）|
| [src/service/dal/agent/builder/default.rs#L248-L335](src/service/dal/agent/builder/default.rs#L248-L335) | DefaultPromptBuilder 字段扩展 | 新增 `ontology_lexicon: Option<OntologyLexiconSummary>` 字段；6 个 workspace_* Option 字段保持不变 |
| [src/service/dal/agent/builder/default.rs#L1055-L1234](src/service/dal/agent/builder/default.rs#L1055-L1234) | 场景 prompt 重构 | **build_sleep_prompt / build_summary_prompt** 静态指令块前置 + 易变数据统一收尾；指令用"见文末【XXX】"引用；空 trace_ids 兜底渲染 `- （本次运行无依赖 trace，填 []）` |
| [src/models/cortex_types.rs#L135-L195](src/models/cortex_types.rs#L135-L195) | ChatMessage 枚举 | 【95a0b1bf 新增】`System { content: String }` 变体（+ `system()` 构造器 + `to_summary_text` 处理）；ChatMessage 从此有 System 角色，人设/规则/指令 → System，上下文/历史/当前消息 → User |
| [src/service/domain/runtime/compaction.rs#L1-L50](src/service/domain/runtime/compaction.rs#L1-L50) | 上下文压缩模块 | 【7ebf37d3 新增】summary → compaction 重构：compacted_context trait 方法注入压缩后的近期对话摘要，自适应上下文预算 |
| [src/service/domain/runtime/awakening.rs#L343-L393](src/service/domain/runtime/awakening.rs#L343-L393) | 唤醒主流程 | 词表注入调用：`OntologyDal::try_dal()` → `load_lexicon_summary` → `builder.ontology_lexicon()`（可选依赖降级）；同时调用 workspace_context、compacted_context 等 |
| [src/models/project.rs#L199-L245](src/models/project.rs#L199-L245) | Project Prompt 摘要 | **绝对时间替换相对时长**：`relative_duration("5 小时前")` → `format_datetime("2026-09-15 10:30")`；仅数据变更时变化，缓存友好 |
| [src/models/task.rs#L216-L245](src/models/task.rs#L216-L245) | Task Prompt 摘要 | **新增前置依赖任务 ID**：`deps.join(", ")` + 阅读提示 `get_task(前置ID, with_artifacts=true)` |
| 【Wiki 长文】本地文件安全与工作区隔离.md | 路径安全系统化上下文 | [本地文件安全与工作区隔离](docs/wiki/zh/content/基础设施/存储系统/本地文件安全与工作区隔离.md) |
| 【兄弟卡 1】本体词表漂移治理与 Prompt 注入 | 词表侧核心逻辑（三表 / DAL / common 纯函数 / 写后认证）| [本体词表卡](docs/wiki/knowledge/zh/本体词表漂移治理与%20Prompt%20注入：OntologyDal%20load_lexicon_summary%20+%20common%20纯函数解析%20+%20写后认证%20+%20DuckDB%20漂移记账/本体词表漂移治理与%20Prompt%20注入：OntologyDal%20load_lexicon_summary%20+%20common%20纯函数解析%20+%20写后认证%20+%20DuckDB%20漂移记账.md) |
| 【兄弟卡 2】用户维度工作区路径改造 | 路径 SSOT + 安全边界 | [用户维度工作区路径改造](docs/wiki/knowledge/zh/用户维度工作区路径改造%20+%20工具调用安全边界检查：路径逃逸%20跨用户访问%20相对路径阻断/用户维度工作区路径改造%20+%20工具调用安全边界检查：路径逃逸%20跨用户访问%20相对路径阻断.md) |

## §3 架构约定

1. workspace_context 在 build_common_context_sections 中渲染为【工作空间与路径约定】区块
2. 路径参数从 config::get().base_data_path() 获取 base，再通过 paths 模块计算
3. 覆盖场景：awaken 主流程 / sleep_and_settle / intent_analyze / awaken_for_summary
4. trait 默认实现为空操作（`let _ = (...)`），Remote/Cli Agent 不参与路径注入
5. 仅在有 workspace 上下文时渲染区块（None 跳过），避免污染 Prompt
6. **ontology_lexicon trait 方法同样默认空操作**；词表注入是可选增强，本体子系统未初始化时（OntologyDal::try_dal() 返回 None）自动跳过，不阻断主流程
7. **区块顺序固定为稳定性递减**（build() 11 块、场景 prompt 同构）：完全静态的在前，每轮变化的统一收尾；指令用"见文末【XXX】"引用尾部数据
8. **场景 prompt 的 initial-messages 变体**：System 消息跨次运行完全静态（人设 + 技能 + 词表 + 指令 SOP），User 中上下文/历史在前（追加式），Trace/轮次/摘要/trace 列表等易变数据收尾——禁止在 System 中间插入每轮变化的内容
9. **Prompt 前缀缓存核心原则**：稳定内容不被易变内容切割——指令块哪怕只被一个每轮变化的 Trace ID 插入中间，其后全部静态内容都会作废前缀缓存命中

## §4 约束清单

1. ✅ 所有唤醒流程（awaken / intent_analyze / summary）必须注入 workspace_context
2. ✅ 路径信息通过 paths 纯函数获取，禁止手写散串
3. ✅ 仅在有 workspace 上下文时渲染区块，避免污染 Prompt
4. ❌ trait 方法内禁止依赖 config / paths / RequestContext（保持纯抽象）
5. ✅ 其他 PromptBuilder 实现（Cli/Remote）默认空操作，不需要路径感知
6. ❌ send_message 严禁用于询问用户澄清/决策——必须用 Final 文本（会终止思考循环等待用户下一条）；send_message 发完不打断思考循环，仅用于当前对话关键进展同步或跨 Agent 异步通知。需要用户回复/澄清/决策 → Final 文本；信息充足直接回答 → Final 文本；Agent 间通知/汇报 → send_message（不打断循环）
7. ✅ ontology_lexicon 默认空操作；词表未初始化时自动跳过，不阻断主流程
8. ✅ 区块顺序稳定性递减排序（build() 11 块、场景 prompt 同构）——禁止在静态块中间插入每轮变化的内容
9. ✅ Prompt 中时间字段统一用绝对时间（format_datetime YYYY-MM-DD HH:MM），禁止相对时长（relative_duration）——相对时长随当前时刻漂移导致区块字节级变化
10. ✅ 场景 prompt（sleep/summary/intent）的静态指令块必须完全静态（不含 Trace/轮次/摘要等易变数据）；易变数据统一收尾
11. ✅ 指令块内引用易变数据必须用"见文末【XXX】"格式，禁止在指令行内嵌入动态值
12. ✅ 本体词表区块裁剪优先级固定：关系词 > 实体类 > 同义样例；同义样例优先级最低，任何段落触发裁剪即整体让位
