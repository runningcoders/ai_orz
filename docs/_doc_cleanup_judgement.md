# 文档整理判定标准辅助（临时）

> 所有 Task 执行时必须引用本文件的判定规则，禁止主观判断。
> Task 9 完成后删除。

---

## 判定表 A：代码块性质（§2.1.2 铁律）

用于处理 docs/design/* 和 docs/memory_design.md 中的 ``` 代码块：

| 代码块内容特征 | 性质判定 | 处理动作 |
|--------------|---------|---------|
| 内容仅为：`trait X {` + 方法签名列表（无 `{` 实现体，仅 `fn name(...` 行后 `;` 或空行） | **契约表达型 ✅** | 保留代码块；代码块紧后追加一行：<br>`> 当前实现：[相对路径::trait 名](file:///绝对路径#LLine-Line)` |
| 内容仅为：`struct X {` + 字段列表 + `}`，无 `impl` 块 | **契约表达型 ✅** | 保留；同上附路径 |
| 内容仅为：`enum X {` + 变体列表 + `}`，无 match 逻辑 | **契约表达型 ✅** | 保留；同上附路径 |
| 内容仅为：SQL `CREATE TABLE / CREATE INDEX / CREATE VIRTUAL TABLE`（不含 INSERT/UPDATE/触发器逻辑） | **契约表达型 ✅** | 保留；紧后追加一行：<br>`> 对应迁移文件：[migrations/XXX.sql](file:///绝对路径/migrations/XXX.sql)` |
| 内容为：目录树结构 ASCII 图（`frontend/` `├── Cargo.toml` 类）或数据流 ASCII 图 | **契约表达型 ✅** | 保留；无需附路径（纯图示） |
| 内容为：`fn foo() {` + 内部逻辑 + `}`，或 `match x {` + 分支体，或 `#[test]` 测试，或 macro_rules | **实现快照型 ❌** | 整段删除；替换为一句话：<br>`> 相关实现细节见：[文件名::函数/宏/测试名](file:///绝对路径#LLine-Line)` |
| 内容为：`cargo test/build/check ...`、`git add/commit/push ...`、`cargo run ...`、bash for/while 循环等命令 | **实现快照型 ❌** | 整段删除，无需替代（AGENTS.md §2.1.3 明确禁止） |
| 无法判断 | 保守判定为实现快照型 | 删，换路径链接 |

---

## 判定表 B：superpowers/plans 文件分类（P0 处置）

逐个打开文件，看文件头下面的第一个 H2/H3 是否是「Task 1」「Step 1」类执行清单，或 `Goal` 段落中写了「补页面 / 写测试 / 实现接口」类具体实现措辞：

| 判定条件 | 分类 | 处置 |
|---------|------|------|
| 文件内容 > 80% 是 Task/Step checkbox + 实现代码块 + cargo/git 命令，且 Goal 描述的功能已在 git log 中存在对应提交（功能已落地） | **B 类：纯执行期蓝图** | 直接移动到 `docs/archive/superpowers-archive/YYYY-MM-DD-原文件名.md`（按原提交最接近的日期打前缀），不做任何内容修改 |
| 文件内容含明显架构决策段落（如「设计哲学」「关键决策表」「行为红线」「扩展模式」），且不是纯 checkbox 驱动 | **A 类：有长期参考价值** | 留在原地，等 Task 2 按 plan 模板 B 精简后迁移到 `docs/plan/`，原文件删除 |
| 对应功能明确未完成（文档中验收清单全未勾选，且 git log 无相关提交） | **C 类：进行中** | 不处理，保留 |
## 文档规模基线（整理前，2026-08-15）
```
  119016 total
    3945 docs/superpowers/plans/2026-07-12-frontend-refactor.md
    2684 docs/superpowers/plans/2026-08-04-remove-rig-dependency.md
    2507 docs/superpowers/plans/2026-07-26-seed-config-migration.md
    2359 docs/design/runtime_design.md
    2319 docs/superpowers/plans/2026-07-19-a2a-server.md
    2170 docs/superpowers/plans/2026-07-18-external-agent-integration.md
    2156 docs/superpowers/plans/2026-07-25-canvas-expansion.md
    1982 docs/superpowers/plans/2026-07-27-integration-testing-foundation.md
    1858 docs/superpowers/plans/2026-07-25-stats-charts-phase3.md
    1831 docs/superpowers/plans/2026-08-03-agent-integration-tests.md
    1769 docs/superpowers/plans/2026-08-05-agent-loop-aop-hooks.md
    1761 docs/design/tool_design.md
    1603 docs/superpowers/plans/2026-07-30-tool-skill-search-install.md
    1595 docs/superpowers/plans/2026-07-23-runtime-issues-fix.md
    1520 docs/superpowers/plans/2026-07-28-project-task-enhancement.md
    1485 docs/superpowers/plans/2026-07-17-feishu-p2p-message-integration.md
    1480 docs/superpowers/plans/2026-07-15-entity-stats-injection.md
    1453 docs/superpowers/plans/2026-07-12-chat-mvp.md
    1437 docs/superpowers/plans/2026-07-20-aop-event-center.md
    1420 docs/superpowers/plans/2026-07-23-p2-detail-pages-and-tool-calls.md
    1414 docs/superpowers/plans/2026-07-13-frontend-page-completion.md
    1413 docs/LAYERED_ARCHITECTURE_PRACTICE.md
    1402 docs/superpowers/plans/2026-07-24-canvas-particle-systems.md
    1390 docs/design/mcp_tool_design.md
    1384 docs/superpowers/plans/2026-07-30-preinstall-basic-skills.md
    1329 docs/design/thinking_task_policy_engine_design.md
    1263 docs/superpowers/plans/2026-07-24-query-pagination.md
    1244 docs/superpowers/plans/2026-07-26-macro-path-param-fix.md
    1227 docs/superpowers/plans/2026-08-04-agent-intelligence-integration-tests.md
    1217 docs/superpowers/plans/2026-08-14-intent-analyze-two-stage-awaken.md
    1216 docs/superpowers/plans/2026-07-15-task-management-core.md
    1209 docs/superpowers/plans/2026-07-13-search-and-knowledge-graph.md
    1208 docs/superpowers/plans/2026-08-04-knowledge-graph-recommend-seed.md
    1196 docs/superpowers/plans/2026-08-14-policy-engine-and-think-runtime.md
    1147 docs/superpowers/plans/2026-07-30-background-task-module.md
    1121 docs/superpowers/plans/2026-07-28-agent-artifact-creation.md
    1119 docs/superpowers/plans/2026-08-01-unify-entity-search-query.md
    1116 docs/memory_design.md
    1063 docs/plan/agent_loop_engine_plan.md
    1061 docs/superpowers/plans/2026-07-24-canvas-dynamic-rendering.md
    1042 docs/superpowers/plans/2026-07-17-system-admin-tools.md
    1033 docs/superpowers/plans/2026-07-26-macro-path-query-branch-fix.md
    1023 docs/superpowers/plans/2026-07-24-knowledge-graph-tags-display-and-filter.md
    1018 docs/superpowers/plans/2026-08-04-rig-upgrade.md
    1013 docs/superpowers/plans/2026-07-24-batch-query-by-ids.md
     998 docs/superpowers/plans/2026-07-16-hnsw-persistence-and-async-rebuild.md
     991 docs/superpowers/plans/2026-07-24-workspace-dashboard.md
     989 docs/superpowers/plans/2026-07-25-stats-charts-phase1.md
     958 docs/superpowers/plans/2026-07-28-artifact-editing.md
     933 docs/superpowers/plans/2026-07-31-hive-knowledge-sharing.md
     893 docs/superpowers/plans/2026-07-21-a2a-async-callback-polling.md
     878 docs/superpowers/plans/2026-07-31-unify-agent-search-query.md
     864 docs/superpowers/plans/2026-07-30-background-task-admin-page.md
     858 docs/design/stats_module_design.md
     835 docs/superpowers/plans/2026-07-15-knowledge-graph-enhancement.md
     821 docs/design/skill_design.md
     818 docs/superpowers/plans/2026-07-27-frontend-api-protocol-struct-refactor.md
     818 docs/superpowers/plans/2026-07-15-task-management-advanced.md
     812 docs/superpowers/plans/2026-08-14-runtime-api-and-exit-reason.md
     808 docs/superpowers/plans/2026-07-24-detail-pages-relation-graph-tab.md
     793 docs/superpowers/plans/2026-07-23-p1-crud-edit-modals.md
     780 docs/superpowers/plans/2026-07-21-aop-monitoring.md
     765 docs/superpowers/plans/2026-07-22-tailwind-daisyui-migration.md
     763 docs/superpowers/plans/2026-07-23-p0-skill-detail-and-task-edit.md
     753 docs/superpowers/plans/2026-08-01-caller-type-context.md
     744 docs/design/agent_loop_engine_design.md
     720 docs/superpowers/plans/2026-07-14-sse-message-push.md
     711 docs/superpowers/plans/2026-07-27-clippy-warning-cleanup.md
     707 docs/superpowers/plans/2026-08-14-frontend-runtime-panel.md
     694 docs/superpowers/plans/2026-07-15-frontend-stats-integration.md
     693 docs/superpowers/plans/2026-08-01-unify-project-search.md
     692 docs/superpowers/plans/2026-08-05-unify-tool-execution.md
     691 docs/superpowers/plans/2026-07-25-stats-charts-phase2.md
     689 docs/superpowers/plans/2026-07-15-agent-memory-chat-ux.md
     683 docs/superpowers/plans/2026-07-31-awaken-context-and-sleep-constraint.md
     678 docs/design/project_management_design.md
     667 docs/archive/handler_management_api_plan.md
     649 docs/design/message_channel_design.md
     640 docs/design/stats_query_design.md
     607 docs/superpowers/plans/2026-07-31-code-smell-and-sleep.md
     595 docs/archive/runtime-domain-roadmap.md
     592 docs/design/attachment_storage.md
     591 docs/superpowers/plans/2026-07-15-toast-notification-system.md
     590 docs/ARCHITECTURE.md
     576 docs/superpowers/plans/2026-08-01-unify-task-search.md
     576 docs/design/handler-tool-registration-macro.md
     571 docs/superpowers/plans/2026-07-24-canvas-rendering-infrastructure.md
     559 docs/superpowers/plans/2026-07-27-test-speed-optimization.md
     540 docs/superpowers/plans/2026-08-15-docs-cleanup.md
     531 docs/superpowers/plans/2026-07-13-management-pages.md
     528 docs/design/vector_search_architecture.md
     527 docs/superpowers/plans/2026-08-14-agent-runtime-context-extend.md
     525 docs/design/generic_builtin_tools_design.md
     525 docs/archive/a2a_server_design.md
     514 docs/design/testing_guidelines.md
     484 docs/design/request_context_design.md
     484 docs/design/builtins_http_tool_design.md
     461 docs/superpowers/specs/mobile-adaptation/tasks.md
     457 docs/design/task_scheduler_design.md
     448 docs/design/entity_list_query_search_design.md
     446 docs/design/message_interaction_design.md
     438 docs/superpowers/specs/2026-07-19-a2a-server/tasks.md
     438 docs/design/intent_aware_two_stage_awaken_design.md
     423 docs/superpowers/plans/2026-07-16-cron-trigger-frontend-ux.md
     403 docs/design/unified-idl-http-handler.md
     397 docs/design/ui_design_system.md
     389 docs/design/sqlx_guide.md
     370 docs/design/consumer_architecture.md
     366 docs/design/logging_design.md
     355 docs/design/agent_onboarding_design.md
     338 docs/design/common-error-type.md
     335 docs/design/frontend_architecture.md
     318 docs/superpowers/plans/2026-07-28-agent-artifact-creation-proposal.md
     311 docs/superpowers/plans/2026-07-16-vector-search-enhancement.md
     309 docs/design/canvas_rendering_playbook.md
     295 docs/archive/frontend_roadmap.md
     286 docs/superpowers/specs/mobile-adaptation/spec.md
     272 docs/plan/architecture_status_20260725.md
     261 docs/design/pagination_and_count_convention.md
     255 docs/design/external_agent_design.md
     238 docs/superpowers/specs/2026-07-19-a2a-server/checklist.md
     234 docs/superpowers/specs/enhance-entity-search/spec.md
     217 docs/design/organization_design.md
     212 docs/plan/lark-cli_集成二期.md
     197 docs/design/lark_cli_integration.md
     194 docs/design/event_design.md
     191 docs/superpowers/specs/enhance-skill-system/spec.md
     186 docs/superpowers/specs/2026-07-19-a2a-server/spec.md
     182 docs/superpowers/specs/enhance-memory-system/spec.md
     182 docs/superpowers/specs/enhance-memory-search/spec.md
     181 docs/superpowers/specs/mobile-adaptation/checklist.md
     174 docs/archive/test_supplement_plan_20260514.md
     158 docs/superpowers/specs/enhance-memory-system/tasks.md
     150 docs/plan/身份凭证Domain统一CRUD重构.md
     144 docs/superpowers/plans/2026-07-31-knowledge-graph-published-sharing.md
     125 docs/design/browser_e2e_test_design.md
     123 docs/superpowers/plans/2026-07-16-finance-stats-panels.md
     112 docs/design/project_design.md
     108 docs/superpowers/specs/enhance-entity-search/checklist.md
     106 docs/design/task_design.md
     103 docs/plan/聊天页项目信息侧栏.md
     103 docs/plan/todo.md
     102 docs/plan/前端_Markdown_渲染全覆盖.md
      89 docs/plan/进程管理与shell_exec修复.md
      84 docs/design/seed-config-migration.md
      83 docs/superpowers/specs/enhance-entity-search/tasks.md
      80 docs/superpowers/specs/enhance-skill-system/checklist.md
      76 docs/superpowers/specs/enhance-skill-system/tasks.md
      73 docs/plan/用户偏好双源设计.md
      70 docs/design/api_protocol_convention.md
      60 docs/CODE_WIKI.md
      58 docs/superpowers/specs/enhance-memory-system/checklist.md
      58 docs/superpowers/specs/enhance-memory-search/checklist.md
      56 docs/plan/图谱遍历查询优化.md
      55 docs/plan/前端工具与进程管理.md
      48 docs/superpowers/specs/enhance-memory-search/tasks.md
      47 docs/superpowers/plans/2026-07-23-frontend-completion-index.md
      35 docs/_doc_cleanup_judgement.md
      25 docs/superpowers/plans/2026-07-24-workspace-task-dag-layout.md
      23 docs/README.md
```
