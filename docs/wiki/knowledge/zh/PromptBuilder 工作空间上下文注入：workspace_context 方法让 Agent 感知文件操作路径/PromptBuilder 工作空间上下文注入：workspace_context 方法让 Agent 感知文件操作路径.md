---
kind: wiki_knowledge_card
name: PromptBuilder 工作空间上下文注入（workspace_context 方法让 Agent 感知文件操作路径）
category: Prompt 构建与上下文注入
scope:
- src/models/prompt_builder.rs
- src/models/cortex_types.rs
- src/service/dal/agent/**
- src/service/domain/runtime/compaction.rs
- src/service/domain/runtime/awakening.rs
- src/service/domain/runtime/intent_analyze.rs
source_files:
- src/models/prompt_builder.rs#L41-L108
- src/models/prompt_builder.rs#L155-L209
- src/models/cortex_types.rs#L135-L195
- src/service/dal/agent/builder/default.rs#L1-L40
- src/service/domain/runtime/compaction.rs#L1-L50
- src/service/domain/runtime/awakening.rs#L104-L145
- src/service/domain/runtime/awakening.rs#L343-L393
- src/service/domain/runtime/intent_analyze.rs#L124-L157
- docs/wiki/zh/content/架构设计/分层架构设计/Domain 层编排/Runtime 领域编排.md
- docs/wiki/zh/content/基础设施/存储系统/本地文件安全与工作区隔离.md
- docs/wiki/knowledge/zh/用户维度工作区路径改造 + 工具调用安全边界检查：路径逃逸 跨用户访问 相对路径阻断/用户维度工作区路径改造 + 工具调用安全边界检查：路径逃逸 跨用户访问 相对路径阻断.md

---

# PromptBuilder 工作空间上下文注入

## §1 整体方案

为 PromptBuilder trait 新增 workspace_context 方法，将 Agent 工作空间路径信息注入 Prompt 中，帮助 Agent 明确文件操作的路径边界。

**95a0b1bf 重构**：PromptBuilder trait 新增 3 个默认方法扩展 Agent 上下文注入能力：`compacted_context`（注入 compaction 压缩后的近期上下文摘要）、`settled_reference`（注入长期沉淀知识图谱的摘要引用）、`past_memories_reference`（注入短期/长期记忆引用索引）；同时新增 `build_initial_messages / build_sleep_initial_messages / build_summary_initial_messages / build_intent_analyze_initial_messages` 四个构建方法，输出 `Vec<ChatMessage>` 而非扁平字符串——第一次让 Prompt 按 System / User 消息角色分层传递给模型（Chat Completions API 规范），告别人设/指令/对话内容全挤 User 角色的混乱状态。

同一轮重构还在 `cortex_types.rs` 中新增 `ChatMessage::System { content }` 变体 + `system()` 构造器 + `to_summary_text` 摘要处理，以及 DefaultPromptBuilder 中 `build_final_response_guidance` 方法——把【回复规则 §0-§5】（先审题再回答 / 何时直接回复 / send_message 正确用途 / 闲聊豁免 / 检索空结果 / 禁止无意义工具调用）写入 System 消息尾部，彻底解决 Agent 误以为必须调用 send_message 才能结束任务的心理陷阱。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构 |
|------|------|----------|
| [src/models/prompt_builder.rs](src/models/prompt_builder.rs) | PromptBuilder trait | workspace_context 方法声明（L61-L90），默认实现为空操作（`let _ = (...)`），trait 不依赖 config/paths/RequestContext |
| [src/service/dal/agent/builder/default.rs](src/service/dal/agent/builder/default.rs) | DefaultPromptBuilder 实现 | workspace_context 实现 + 渲染【工作空间与路径约定】区块；6 个 Option 字段：workspace_default / workspace_user_home / workspace_user_shared / workspace_user_agent / workspace_agent / workspace_project |
| [src/service/domain/runtime/awakening.rs](src/service/domain/runtime/awakening.rs) | 唤醒主流程 | 两处注入：awaken + sleep_and_settle 通过 paths:: 模块计算路径后调用 builder.workspace_context(...) |
| [src/service/domain/runtime/intent_analyze.rs](src/service/domain/runtime/intent_analyze.rs) | 意图分析流程 | 注入 workspace_context，意图分析阶段 Agent 也感知路径边界 |
| [src/models/cortex_types.rs](src/models/cortex_types.rs#L135-L195) | ChatMessage 枚举 | 【95a0b1bf 新增】`System { content: String }` 变体（+ `system()` 构造器 + `to_summary_text` 处理）；ChatMessage 从此有 System 角色，人设/规则/指令 → System，上下文/历史/当前消息 → User |
| [src/service/domain/runtime/compaction.rs](src/service/domain/runtime/compaction.rs) | 上下文压缩模块 | 【7ebf37d3 新增】summary → compaction 重构：compacted_context trait 方法注入压缩后的近期对话摘要，自适应上下文预算，按轮次分桶 + PENDING_BUDGET_RATIO 常量控制压缩比例 |
| 【Wiki 长文】本地文件安全与工作区隔离.md | 路径安全系统化上下文 | [本地文件安全与工作区隔离](docs/wiki/zh/content/基础设施/存储系统/本地文件安全与工作区隔离.md) |
| 【兄弟卡】用户维度工作区路径改造 | 路径 SSOT + 安全边界 | [用户维度工作区路径改造](docs/wiki/knowledge/zh/用户维度工作区路径改造 + 工具调用安全边界检查：路径逃逸 跨用户访问 相对路径阻断/用户维度工作区路径改造 + 工具调用安全边界检查：路径逃逸 跨用户访问 相对路径阻断.md) |

## §3 架构约定

1. workspace_context 在 build_common_context_sections 中渲染为【工作空间与路径约定】区块
2. 路径参数从 config::get().base_data_path() 获取 base，再通过 paths 模块计算
3. 覆盖场景：awaken 主流程 / sleep_and_settle / intent_analyze / awaken_for_summary
4. trait 默认实现为空操作（`let _ = (...)`），Remote/Cli Agent 不参与路径注入
5. 仅在有 workspace 上下文时渲染区块（None 跳过），避免污染 Prompt

## §4 约束清单

1. ✅ 所有唤醒流程（awaken / intent_analyze / summary）必须注入 workspace_context
2. ✅ 路径信息通过 paths 纯函数获取，禁止手写散串
3. ✅ 仅在有 workspace 上下文时渲染区块，避免污染 Prompt
4. ❌ trait 方法内禁止依赖 config / paths / RequestContext（保持纯抽象）
5. ✅ 其他 PromptBuilder 实现（Cli/Remote）默认空操作，不需要路径感知
