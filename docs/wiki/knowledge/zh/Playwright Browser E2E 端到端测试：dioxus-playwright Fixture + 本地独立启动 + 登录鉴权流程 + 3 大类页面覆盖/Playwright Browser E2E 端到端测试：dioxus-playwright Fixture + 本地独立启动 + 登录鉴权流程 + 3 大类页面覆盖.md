---
kind: knowledge_card
name: Playwright Browser E2E 端到端测试：dioxus-playwright Fixture + 本地独立启动 + 登录鉴权流程 + 3 大类页面覆盖
category: 工程规范
scope:
  - "tests/**/*.rs"
  - "tests/common/**/*.rs"
  - "frontend/src/pages/**/*.rs"
  - "src/handlers/organization/auth/*.rs"
  - "common/src/enums/**/*.rs"
  - "src/models/**/*.rs"
source_files:
  - "tests/common/env.rs#L1-L87"
  - "tests/integration/agent_management_test.rs"
  - "Cargo.toml#L86-L172"
  - "common/src/enums"
  - "src/models"
  - "src/handlers/organization/auth/login.rs"
  - "frontend/src/pages"
  - "docs/archive/design-archive/browser_e2e_test_design.md"
  - "docs/archive/plan-archive/Agent管理集成测试.md"
  - "docs/wiki/zh/content/测试指南/测试指南.md"
  - "docs/wiki/zh/content/基础设施/持续集成与发布工作流.md"
  - "docs/wiki/zh/content/项目概述/技术栈概览.md"
---

# §1 概述与定位

本知识卡沉淀 AI Orz 面向浏览器的端到端（E2E）测试体系设计，覆盖 **Playbook Markdown 双模式执行契约**、**测试环境初始化对齐生产启动顺序**、**登录鉴权流程 Fixture 抽取**、**三大核心页面域覆盖规划**四大支柱。当前代码库中集成测试已落地（tests/integration 19 targets），Playwright E2E 层处于设计约定阶段，代码实现复用同一套测试基础设施（`init_full_test_env` + `data-testid` 选择器契约 + 登录态 Cookie 注入）。

# §2 关键文件表

| 角色 | 路径 | 关键锚点 |
|------|------|----------|
| init_full_test_env 启动顺序对齐 | tests/common/env.rs | L1-L87 8 步初始化：config→storage→jwt→tool_tracing→tool_registry→service.init→producer+consumer→service.init_base_data；OnceCell 串行化，测试间唯一 ID 隔离 |
| 集成测试目标清单 | Cargo.toml | L86-L172 `[[test]]` 块注册：agent_management/tool_skill_vector/message_vector/a2a_flow/preset_skills/core_crud 等 19 targets |
| Agent 集成测试样板（可复用模式） | tests/integration/agent_management_test.rs | 24 个测试分层：12 HTTP CI-safe + 5 真实向量 + 4 FTS5；含 RealModelConfig 守卫 + create_embedding_provider 辅助 |
| Design 规范 | docs/archive/design-archive/browser_e2e_test_design.md | Playbook 双模式（Playwright解析驱动 / Agent驱动）+ Action 枚举规范 + data-testid 一等公民 + 冷却层机制 |
| Plan 落地计划 | docs/archive/plan-archive/Agent管理集成测试.md | 集成测试 3 波落地骨架 + Part A/B/Follow-up 分层 + 5 条红线（CI-safe/#[ignore]/OnceCell/异步窗口/混合排序） |
| 测试金字塔总览 | docs/wiki/zh/content/测试指南/测试指南.md | 1124 测试分布：984 后端 + 82 前端 + 87 集成 + Playwright E2E 预留位 |
| CI 闸门阶段 | docs/wiki/zh/content/基础设施/持续集成与发布工作流.md | CI 四阶段：clippy→单元测试→集成测试→Playwright E2E（最后闸门） |
| 技术栈总览 | docs/wiki/zh/content/项目概述/技术栈概览.md | 后端 Rust + Axum；前端 Dioxus 0.7 + WASM；E2E dioxus-playwright 选型 |
| 枚举与 PO 定义 | common/src/enums / src/models | 测试断言的状态值来源：AgentStatus/ToolStatus/SkillStatus 枚举 + 各实体 PO 结构 |
| 登录 Handler | src/handlers/organization/auth/login.rs | E2E 登录流程的后端入口，HttpOnly Cookie JWT 发放逻辑 |
| 前端页面组件 | frontend/src/pages/**/*.rs | E2E Playwright 页面域覆盖目标文件路径 |

# §3 架构与约定

## 3.1 Playbook Markdown 双模式执行契约

E2E 用例以 Markdown 文件（Playbook）作为「人读 / 解析器读 / Agent 读」三方共同契约，支持两种执行模式共享同一份剧本：

| 模式 | 驱动方式 | Runner 目标文件 | 适用场景 | 精度 | 速度 |
|------|---------|-----------------|---------|------|------|
| A. 解析驱动 | Playbook → AST → Playwright API | tests/e2e/runners/playwright_runner.rs | CI 冒烟、回归测试、PR 闸门 | 100% 稳定 | 快 |
| B. Agent 驱动 | Playbook 原文当 Prompt → LLM + Browser 工具 | tests/e2e/runners/agent_runner.rs | 探索性 QA、复杂多步业务路径、UI 微变容错 | 偶发漂移 | 慢 |

Playbook 格式 = YAML Front Matter（id/tags/roles/setup）+ Steps 表格（Step/Action/Target/Data/Assert）。Action 枚举统一使用：navigate / click / type / select / toggle / wait / assert / screenshot。

## 3.2 本地独立启动 Fixture 对齐生产顺序

E2E 启动顺序与 `ai_orz::run()`（src/lib.rs:L114-L154）严格一一对应：

```
pkg::init_all → service::init → producer::init → consumer::init → service::init_base_data → aop::init_all
```

测试环境在 `init_full_test_env`（tests/common/env.rs:L25-L78）中封装为 8 步，关键差异：
- Storage 隔离到 tempdir，使用 `VectorStoreType::InMemory`（避免 LanceDB multi-thread 要求）
- JWT Secret 为测试专用「test-jwt-secret-do-not-use-in-prod」，1 小时过期
- 全局 OnceLock + `tokio::sync::OnceCell` 串行化初始化（测试间复用单例，靠 UUID 隔离数据）
- init_base_data 步骤不可省略：注入 2 条系统级 cron triggers + 内置工具同步

## 3.3 登录鉴权流程 Fixture 抽取

**禁止**每个 Playbook 都写「注册→登录→创建组织」前置步骤。抽为 setup 标签：
- `setup: bootstrap_admin_login`：自动完成 System Initialization → 创建组织+Owner 登录 → 注入 JWT Cookie → 返回上下文 `{jwt, org_id, user_id}`
- `setup: bootstrap_admin_and_agent`：admin 登录基础上，再创建 1 个 Local Agent + 1 个 HTTP Tool，返回 `{agent_id, tool_id}`

登录态使用 HttpOnly Cookie（非前端 localStorage 持 token），Playwright 通过 `context.storage_state(path)` 持久化登录态供后续测试复用。

## 3.4 三大类页面域覆盖

| 页面域 | 典型 Playbook 数 | 覆盖的核心路由 |
|--------|-----------------|---------------|
| 人力资源域 | 工具 4 篇 + 技能 3 篇 + Agent 5 篇 | `/hr/tools`、`/hr/skills`、`/hr/agents` + 详情页 Tabs |
| 项目域 | 项目 3 篇 + 任务 4 篇 | `/projects`、`/projects/{id}/tasks` + 看板 |
| 系统域 | 种子 2 篇 + 日志 1 篇 + Cron 2 篇 | `/system/seed`、`/system/logs`、`/system/cron-triggers` |

每个页面域必须覆盖：CRUD 表单校验 / 列表筛选 search 三场景切换 / 跨实体关联操作 / 权限边界访问 4 类 Playbook。

# §4 硬约束与红线

1. **data-testid 一等公民红线**：所有交互 DOM 节点必须添加 `data-testid="xxx"` 属性，Playbook Target 字段**禁止**使用文字匹配、XPath、层级选择器（`div > div:nth-child(3)`）；Playwright Runner 默认走 data-testid，找不到直接报错而非降级
2. **模式 B 冷却层红线**：Agent 驱动模式（模式 B）连续 3 步 DOM 快照无变化时，必须自动降级到模式 A 的选择器兜底执行；**禁止** Agent 无限循环尝试同一操作超过 5 次
3. **断言不是可选红线**：Playbook 每个 Step 必须有 Assert 字段；Playbook 结尾必须有独立的「总结断言」块；模式 B 执行完成后**强制**走一遍模式 A 断言校验 + 截图归档
4. **Setup 独立红线**：Playbook YAML Front Matter 中 `setup:` 字段**禁止**省略；执行器在跑 Steps 前必须先执行对应 Fixture，把注入字段写入上下文供 Steps 变量引用（如 `{{user_id}}`）
5. **Playbook 三用红线**：同一份 Playbook MD 必须同时满足产品验收单 / PR QA 清单 / 回归 Case 三种用途；标题必须可读（非 E2E-001 无意义编号）；步骤说明用业务语言而非技术语言
6. **E2E 独立于集成测试红线**：Playwright E2E 与 tests/integration 集成测试互不替代；集成测试验证后端 HTTP API，E2E 验证真实浏览器交互；CI 中集成测试通过后才启动 Playwright E2E 闸门
7. **向量与真实模型隔离红线**：E2E 默认走 InMemory 向量存储降级路径；需要真实 Doubao Embedding 的 Playbook 必须显式标记 `tags: ["requires-real-model"]`，CI 中默认跳过（仅 nightly 批次执行）
