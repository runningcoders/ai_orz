# Agent 管理集成测试落地

> 📦 归档标记（2026-08-16）：归档冻结。保留原因：Agent管理集成测试 功能已完成并通过验收，文档转为历史快照。生效方案：见源码和 wiki 长文。

> 状态：完成（2026-08-03）
> 查阅场景：新增 Agent/Tool/Skill 或其他实体集成测试时回看改动清单和扩展模式；向量搜索测试模式复用；集成测试模式规范对齐时打开。
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 项目架构总纲 §测试隔离原则
> - [集成测试基础设施.md](./集成测试基础设施.md) — 6 层测试基础设施架构定义与扩展模式
> - [向量存储架构设计](../archive/design-archive/vector_search_architecture.md) — FTS5+向量混合搜索原理
> - 【① Design 设计总纲（Batch11 精确对应 1 篇）】
>   - [skill_system_enhancement_design.md](../archive/design-archive/skill_system_enhancement_design.md) — 技能系统增强（决策 4 安装幂等 + 决策 6 唤醒注入 Token 熔断粒度）
> - 【③ Wiki 长文 ≥3 篇（Batch11 精确对齐主题④）】
>   - [技能系统.md](docs/wiki/zh/content/功能模块/技能系统.md) — Skill 6 字段结构 + Prompt 四层熔断分层测试断言
>   - [技能包管理.md](docs/wiki/zh/content/功能模块/AI%20Agent%20管理/技能包管理.md) — 技能包幂等安装测试流程（第二次 installed=0）
>   - [HR 领域编排.md](docs/wiki/zh/content/架构设计/分层架构设计/Domain%20层编排/HR%20领域编排.md) — HRDomain.skill / HRDomain.agent 入职绑定集成测试目标
> - 【④ RAG 原子知识卡（Batch11 精确对应 1 张）】
>   - [Skill 系统增强：5 套 TEMPLATE 预置包 + install_skill_pack 幂等 Tag 分发 + Agent 入职绑定 + Prompt Token 熔断](docs/wiki/knowledge/zh/Skill%20系统增强：5%20套%20TEMPLATE%20预置包%20+%20install_skill_pack%20幂等%20Tag%20分发%20+%20Agent%20入职绑定%20+%20Prompt%20Token%20熔断/Skill%20系统增强：5%20套%20TEMPLATE%20预置包%20+%20install_skill_pack%20幂等%20Tag%20分发%20+%20Agent%20入职绑定%20+%20Prompt%20Token%20熔断.md) — §4.1 必守红线 9 条（install 幂等/入职不阻断/熔断永不抛错）对应集成测试断言清单

---

## 一、目标（为什么做）

原状态：Agent 管理 16 个 HTTP 端点无集成测试覆盖；向量索引构建/更新/删除全流程未用真实模型验证；Tool/Skill 向量搜索路径无回归安全网；DoubaoVision ProviderType 匹配依赖 `model_name.contains("vision")` 不严谨。

| 问题维度 | 解决方式 |
|---------|---------|
| Agent 管理 16 个 HTTP 端点零集成测试 | Part A：12 个端点测试（生命周期流转+非法跳转+Cli/Remote外部Agent+搜索+查询+工具包+技能包+统计+前台+边界） |
| 向量索引与语义搜索无真实模型验证 | Part B：3 个 `#[ignore]` 真实 Doubao embedding 测试（语义搜索+索引维护+混合排序） |
| Tool/Skill 向量搜索路径无覆盖 | Follow-up：新增 `tool_skill_vector_test.rs` 9 个测试（4 CI-safe FTS5 + 5 真实向量） |
| DoubaoVision 匹配依赖字符串包含判断 | ProviderType 枚举显式新增 `DoubaoVision = 7`，所有匹配点改走 enum 分发 |
| Skill 删除时向量索引泄漏未清理 | Skill DAL `delete` 补齐 `skill_vector_dao.delete_vector()` 调用（与 Tool 对称） |

**收敛后效果**：Agent/Tool/Skill 三大域集成测试全覆盖（24 个测试，15 CI-safe + 9 真实向量），测试基础设施模式统一，向量索引生命周期三阶段（创建→更新→删除）均有真实模型验证。

---

## 二、架构思路（怎么做的）

集成测试分三波落地，模式逐层复用：

```
HTTP 端点层（Part A，Task 1-12，CI-safe）
  ├─ 测试骨架 + Cargo.toml 注册 + 冒烟（Task 1）
  ├─ Agent 生命周期：合法流转 + 非法拒绝（Task 2-3）
  ├─ 外部 Agent 创建：Cli + Remote（Task 4-5）
  ├─ 搜索端点 + 查询端点（Task 6-7）
  ├─ 工具包 + 技能包生命周期（Task 8-9）
  ├─ 统计查询参数 + 前台路由 + 边界场景（Task 10-12）
  │
  ▼ 复用同一套 init_full_test_env + TestApp + bootstrap_and_login
真实向量层（Part B，Task 13-15，#[ignore]）
  ├─ 真实向量索引构建 + 语义搜索（Task 13）
  ├─ 向量索引自动维护：更新+删除（Task 14）
  └─ 混合搜索排序：FTS5 关键词 > 向量语义（Task 15）
  │
  ▼ 复用 RealModelConfig + create_embedding_provider 辅助
延伸实体层（Follow-up，Tool+Skill+架构修复）
  ├─ Tool/Skill CRUD + FTS5 + 向量 + 混合排序（9 测试）
  ├─ ProviderType::DoubaoVision 显式枚举化
  └─ Skill DAL delete 向量索引清理补洞
```

**关键边界（行为红线，回归必保）**：
1. Part A 测试必须完全 CI-safe：不依赖任何外部 API Key，`init_full_test_env` 中 `embedding_model=None` 走向量降级路径
2. Part B 及 Follow-up 真实向量测试必须用 `#[ignore]` 标记；函数体首行用 `RealModelConfig::from_env()` 守卫，无 Key 时 eprintln 后直接 return（不 panic）
3. 每个测试独立内存 SQLite：`#[sqlx::test]` 宏 + `init_full_test_env(pool.clone())` 串行化全局 OnceLock
4. 向量索引断言需留足异步索引时间窗口（`tokio::time::sleep(Duration::from_secs(2))`），禁止因竞态导致的 flaky test
5. 混合搜索排序断言：FTS5 字面匹配实体位置必须**严格小于**纯向量语义匹配实体位置（FTS5 score 权重 > vector）

---

## 三、涉及文件清单（读代码直接跳）

按 AGENTS.md §3.2 目录结构索引：

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| **集成测试文件（新增）** | | |
| [tests/integration/agent_management_test.rs](../../tests/integration/agent_management_test.rs) | Part A+B 测试主体 | 15 个测试（12 HTTP端点+3真实向量）；含 RealModelConfig 结构体 + create_embedding_provider 辅助 |
| [tests/integration/tool_skill_vector_test.rs](../../tests/integration/tool_skill_vector_test.rs) | Follow-up Tool/Skill 测试 | 9 个测试（4 FTS5 CRUD+搜索 + 5 真实向量语义/维护/混合） |
| [Cargo.toml](../../Cargo.toml) | test target 注册 | 追加 `[[test]]` 块注册 agent_management_test + tool_skill_vector_test 两个 target |
| **基础设施复用（零改动）** | | |
| [tests/common/env.rs](../../tests/common/env.rs) | 测试环境初始化 | 零改动；复用 init_full_test_env（service→producer→consumer→init_base_data 严格顺序） |
| [tests/common/mod.rs](../../tests/common/mod.rs) | TestApp + factories | 零改动；复用 TestApp（HTTP 请求封装）、bootstrap_and_login、create_test_agent、assert_api_ok/error |
| **DoubaoVision 枚举重构（修复）** | | |
| [common/src/enums/provider.rs](../../common/src/enums/provider.rs) | ProviderType 枚举 | 新增 `DoubaoVision = 7` 变体；更新 `From<i32>` + `Display` 实现 |
| [src/service/dao/cortex/rig.rs](../../src/service/dao/cortex/rig.rs) | CortexDao 分发 | Embedding 分支匹配改用 `ProviderType::DoubaoVision`；Agent 分支显式报错 |
| [src/service/dao/cortex/rig/doubao_vision.rs](../../src/service/dao/cortex/rig/doubao_vision.rs) | DoubaoVision 辅助 | 删除 `is_doubao_vision_model` 字符串匹配函数及对应测试 |
| **Skill 向量索引泄漏修复** | | |
| [src/service/dao/skill/mod.rs](../../src/service/dao/skill/mod.rs) | SkillVectorDao trait | 新增 `delete_vector(ctx, skill_id)` 方法签名 |
| [src/service/dao/skill/vector.rs](../../src/service/dao/skill/vector.rs) | SkillVectorDao 实现 | 实现 `delete_vector`，SQL 删除 vss 对应行 |
| [src/service/dal/skill.rs](../../src/service/dal/skill.rs) | Skill DAL delete | `delete` 方法尾部追加 `skill_vector_dao.delete_vector(ctx, &po.id).await` 调用（与 Tool DAL 对称） |
| **前端（零逻辑改动，枚举选项补充）** | | |
| 前端 3 个 Provider 选择器页面 | UI 选项 | 下拉菜单追加 DoubaoVision 选项 |
| **零改动面** | | |
| Domain 层核心业务逻辑 / 路由定义 / common DTO 契约 / 前端 API 调用方式 | 对外不变 | 无修改；集成测试作为外部观察者验证行为契约 |

---

## 四、分发速查表（新增同类集成测试时改 N 处）

新增某实体（如 Task/Project/Memory）的集成测试时，改动入口仅 3 处：

### 4.1 Cargo.toml 注册 test target
| 改动位置 | 操作 | 参考 |
|---------|------|------|
| Cargo.toml 尾部 `[[test]]` 块 | 复制末尾块，改 name + path 为新 target | 参考 [Cargo.toml 现有 agent_management_test 块](../../Cargo.toml) |

> 代码入口：[Cargo.toml 末尾 `[[test]]` 段](../../Cargo.toml)

### 4.2 Part A（CI-safe）HTTP 端点测试模板
| 场景 | 首行模式 | 断言工具 |
|------|---------|---------|
| CRUD 基本流程 | `#[sqlx::test]` + `init_full_test_env` + `bootstrap_and_login` | `assert_api_ok` 取 data 字段断言 |
| 非法操作/边界 | 同上 | `assert_api_error` 或 status 断言 + 二次 GET 验证状态未变 |
| 列表/查询/搜索 | 同上 | 断言 PagedResult `items` 数组内容 + `total` 范围 |

> 代码入口：[agent_management_test.rs::test_agent_smoke 头部模式](../../tests/integration/agent_management_test.rs)

### 4.3 Part B（真实向量）`#[ignore]` 测试模板
| 场景 | 首行守卫 | 环境变量 |
|------|---------|---------|
| 语义搜索/索引维护/混合排序 | `let Some(cfg) = RealModelConfig::from_env() else { eprintln!("SKIP..."); return; };` | TEST_EMBEDDING_API_KEY / TEST_EMBEDDING_MODEL_NAME / TEST_EMBEDDING_PROVIDER_TYPE / TEST_EMBEDDING_BASE_URL |

> 代码入口：[agent_management_test.rs::test_real_vector_semantic_search 首段](../../tests/integration/agent_management_test.rs)

---

## 五、验收清单（2026-08-03 全部达成 ✅）

见 Plan 文档对应 Git 提交记录 / 对应执行任务。

---

## 六、执行结果摘要

| 模块 | 验证结果 |
|------|---------|
| agent_management_test（Part A，非 ignored） | 12 passed（生命周期 2 + 外部Agent 2 + 搜索查询 2 + 工具技能 2 + 统计前台边界 3 + 冒烟 1） |
| agent_management_test（Part B，ignored） | 3 passed（语义搜索 1 + 索引维护 1 + 混合排序 1） |
| tool_skill_vector_test（CI-safe） | 4 passed（Tool CRUD + Skill CRUD + 双方 FTS5 搜索） |
| tool_skill_vector_test（ignored） | 5 passed（Tool 向量 2 + Skill 向量 2 + 混合排序 1） |
| 后端 lib 全量测试 | 全量 PASS（无回归） |
| Clippy（后端 + 前端 wasm32 + 集成测试 targets） | 双端零错误 + 三集成测试 target 零告警 |

### 与计划的 2 处延伸（非偏离，原计划外扩展，业务和质量正向收益）
1. 追加 Tool/Skill 向量集成测试（原计划仅覆盖 Agent 域）：实际落地时发现 Tool DAL 已有向量清理但 Skill DAL 缺失，顺手补洞并补齐对称测试
2. DoubaoVision 枚举化重构：原计划无此项，真实向量测试中暴露 `model_name.contains("vision")` 误判风险（含 vision 子串但非视觉模型），升级为显式枚举消除隐式耦合

---

## 七、后续扩展路径（新增实体集成测试 4 步模板）

> **核心不变量**：测试基础设施（init_full_test_env / TestApp / factories / assert_api_*）零改动；向量降级与真实模型守卫模式零改动。

1. **Cargo.toml 注册 target**：[Cargo.toml](../../Cargo.toml)
   - 复制末尾 `[[test]]` 块，改 `name` + `path` 为 `xxx_test`
   - 保证 `path = "tests/integration/xxx_test.rs"` 与实际文件一致

2. **Part A CI-safe 测试**：[tests/integration/agent_management_test.rs](../../tests/integration/agent_management_test.rs)
   - 文件头部 `#[path = "../common/mod.rs"] mod common;` 引入基础设施
   - 每个测试首行：`#[sqlx::test] async fn test_xxx(pool: SqlitePool) { let _ = init_full_test_env(pool.clone()).await; let app = TestApp::new(pool).await; let (bs, jwt) = bootstrap_and_login(&app).await; ... }`
   - 断言统一用 `assert_api_ok(status, &body)` / `assert_api_error(status, &body, expected_http_status)`

3. **Part B 真实向量测试（如实体支持 Vectorizable）**：[agent_management_test.rs::RealModelConfig + create_embedding_provider](../../tests/integration/agent_management_test.rs)
   - 如已有 agent/tool/skill 的辅助代码直接复用；新实体需要时可跨文件 use 或复制（集成测试 target 间不共享）
   - 测试前两行固定：`#[sqlx::test]` + `#[ignore = "requires real Embedding API key in .env (TEST_EMBEDDING_API_KEY)"]`
   - 函数体首行守卫：`let Some(cfg) = RealModelConfig::from_env() else { eprintln!("SKIP..."); return; };`
   - 向量断言前固定：`tokio::time::sleep(Duration::from_secs(2)).await;`（给异步索引用足够时间窗口）

4. **收尾质量检查**（每次修改集成测试后必跑）：
   - 执行模块集成测试（xxx_test，Part A 全绿验证）（Part A 全绿）
   - 如写了 ignored 测试：执行模块真实向量集成测试（xxx_test，本地 API Key 环境）（本地有 API Key 时跑）
   - 格式化规范检查（全工程零差异） + Clippy 静态检查（xxx_test target 零告警才提交）（零告警才提交）