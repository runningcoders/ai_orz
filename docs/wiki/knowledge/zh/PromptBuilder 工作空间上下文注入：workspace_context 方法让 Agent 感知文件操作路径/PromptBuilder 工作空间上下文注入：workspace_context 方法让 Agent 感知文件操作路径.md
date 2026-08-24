---
kind: wiki_knowledge_card
name: PromptBuilder 工作空间上下文注入（workspace_context 方法让 Agent 感知文件操作路径）
category: Prompt 构建与上下文注入
scope:
- src/models/prompt_builder.rs
- src/service/dal/agent.rs
- src/service/domain/runtime/awakening.rs
- src/service/domain/runtime/intent_analyze.rs
- src/service/domain/runtime/summary.rs
source_files:
- src/models/prompt_builder.rs#L61-L90
- src/service/dal/agent.rs#L1428-L1465
- src/service/domain/runtime/awakening.rs#L343-L348
- src/service/domain/runtime/intent_analyze.rs#L103-L108
- src/service/domain/runtime/summary.rs#L137-L142
- docs/wiki/zh/content/架构设计/分层架构设计/Domain 层编排/Runtime 领域编排.md
- docs/wiki/zh/content/基础设施/存储系统/本地文件安全与工作区隔离.md
- docs/wiki/knowledge/zh/用户维度工作区路径改造 + 工具调用安全边界检查：路径逃逸 跨用户访问 相对路径阻断/用户维度工作区路径改造 + 工具调用安全边界检查：路径逃逸 跨用户访问 相对路径阻断.md

---

# PromptBuilder 工作空间上下文注入

## §1 整体方案

为 PromptBuilder trait 新增 workspace_context 方法，将 Agent 工作空间路径信息注入 Prompt 中，帮助 Agent 明确文件操作的路径边界。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构 |
|------|------|----------|
| [src/models/prompt_builder.rs](src/models/prompt_builder.rs) | PromptBuilder trait | workspace_context 方法声明（L61-L90），默认实现为空操作（`let _ = (...)`），trait 不依赖 config/paths/RequestContext |
| [src/service/dal/agent.rs](src/service/dal/agent.rs) | DefaultPromptBuilder 实现 | workspace_context 实现（L1428-L1465）+ 渲染【工作空间与路径约定】区块（L1032-L1048）；6 个 Option 字段：workspace_default / workspace_user_home / workspace_user_shared / workspace_user_agent / workspace_agent / workspace_project |
| [src/service/domain/runtime/awakening.rs](src/service/domain/runtime/awakening.rs) | 唤醒主流程 | 两处注入：awaken（L343）+ sleep_and_settle（L809）通过 paths:: 模块计算路径后调用 builder.workspace_context(...) |
| [src/service/domain/runtime/intent_analyze.rs](src/service/domain/runtime/intent_analyze.rs) | 意图分析流程 | 注入 workspace_context（L103），意图分析阶段 Agent 也感知路径边界 |
| [src/service/domain/runtime/summary.rs](src/service/domain/runtime/summary.rs) | 摘要唤醒流程 | 注入 workspace_context（L137），总结退出时 Agent 仍能正确引用路径 |
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
