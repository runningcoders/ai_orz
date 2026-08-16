---
kind: RAG 原子知识卡
name: 测试与质量工程：1124测试100%通过 + 984后端82前端58common + 87集成测试19targets + cargo-llvm-cov PR38%/main45%门槛 + clippy -D warnings零容忍 + Playwright E2E
category: 基础设施 / 质量工程
scope:
  - "tests/common/**"
  - "tests/integration/**"
  - "tests/http_handler_macro_test.rs"
  - "src/**/*test_support.rs"
  - "src/**/tests/*.rs"
  - ".github/workflows/**"
  - "frontend/tests/**"
  - "common/**"
source_files:
  - tests/common/env.rs#L1-L120 (init_full_test_env：严格对齐真实启动顺序 = pkg::init_all → service::init → producer::init → consumer::init → service::init_base_data().await；顺序错会导致默认 cron trigger 没写入，集成测试断言失败)
  - tests/common/factories/agent_factory.rs (测试工厂：AgentFactory + create_test_agent(ctx) → 随机 id + 默认 status=Active + 默认 ModelProvider InMemoryMock；所有 Agent 集成测试用工厂造数据，不手写 UserPo/AgentPo)
  - tests/integration/ (87 集成测试 19 targets：a2a_flow agent_awaken agent_management auth_sysinit core_crud cron github lark memory message_channel message_delivery message_vector preset_skills project_owner project_task real_model system_cron tool_call tool_vector vector_degradation 共 20 个文件；87 指文件×模块组合)
  - tests/integration/agent_management_test.rs#L1-L100 (Agent CRUD + 入职五步集成测试：模拟 onboard_agent → 断言 agents.status=Active + installed_skill_packs COUNT=3 + agent_tool_bindings COUNT≥5；步骤 3 失败模拟 → 回滚 agent_id COUNT=0)
  - src/service/dao/cron_trigger/sqlite_test.rs (DAO 层单元测试：#[sqlx::test] 独立内存 SQLite 数据库；每次 test 全新 DB，不依赖全局状态；test_create_cron_trigger + test_list_due_triggers + test_query_count_reuse_where 三条独立)
  - .github/workflows/ci.yml (GitHub CI 流水线：4 Stage = check(clippy --all-targets -D warnings) + test(cargo test --workspace --exclude frontend 后端 + cargo test -p frontend wasm32) + coverage(cargo-llvm-cov --threshold 38% PR / 45% main) + build(dist release))
  - docs/archive/design-archive/testing_guidelines.md#L1-L50 (§测试分层金字塔：DAO/DAL/Domain/Handler 单元 → 集成测试 target → E2E Playwright；§#[sqlx::test] 隔离约定；§common 测试 init_full_test_env 启动顺序)
  - docs/design/sqlx_guide.md#L1-L60 (§SQLite STRICT 强制所有表；§枚举字段 status as "status: TaskStatus" 显式标注；§.sqlx 目录必须纳入版本控制（query! 离线元数据）；§软删除 status=0 默认过滤)
  - docs/archive/design-archive/browser_e2e_test_design.md（§Playwright 本地 E2E 约定：仅本地跑不进 CI（因为要 LLM Key 成本）；§login → 创建 Agent → 发消息 完整 happy path 用例；§Video 录制失败自动上传 Artifact）
  - docs/archive/plan-archive/AOP生产消费事件中心重构.md（§集成测试 target=event_delivery：AOP publish → 消费者 ack/nack → message_delivery_attempts 查询断言链路）
  - docs/archive/plan-archive/身份凭证Domain统一CRUD重构.md（§集成测试 target=credential_crud：8 个 Handler 接口调用断言 AES256-GCM 加解密 roundtrip）
  - docs/archive/plan-archive/Agent管理集成测试.md（§19 集成测试 target 清单与依赖顺序 §onboard_agent 回滚断言 §UserRole 权限组合三角色 × 三资源矩阵）
  - docs/wiki/zh/content/测试指南/测试指南.md（测试入口总览：1124 测试分布 + 984 后端细分 897 单元 87 集成 + 82 前端 + 58 common + 运行命令 `cargo test --workspace`）
  - docs/wiki/zh/content/测试指南/端到端测试基础设施.md（Playwright E2E：本地命令 `just e2e` + 环境变量 AI_ORZ_ADMIN_PASSWORD + 失败视频自动保存到 e2e-artifacts/）
  - docs/wiki/zh/content/基础设施/持续集成与发布工作流.md（CI 四阶段：check/test/coverage/build + PR coverage threshold 38% + main branch 45% + cargo-llvm-cov 运行 `--fail-under-lines` 严格模式）
  - 【平行卡 1】docs/wiki/knowledge/zh/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册.md（8 类消费者注册顺序测试：a2a_flow 集成测试断言 CronTriggerConsumer 必须在 AgentLoopConsumer 之后 register）
  - 【平行卡 2】docs/wiki/knowledge/zh/三位一体混合搜索：FTS5 关键词 + 向量语义 + 合并排序（6 DAO 统一 search 模式 + 向量失败降级）/三位一体混合搜索：FTS5 关键词 + 向量语义 + 合并排序（6 DAO 统一 search 模式 + 向量失败降级）.md（vector_degradation 集成测试 target：向量存储 Mock 失败 → 搜索自动降级 FTS5-only 断言结果非空 score>0）
---

## §1 概述

**本卡角色**：1124 测试 100% 通过率 + 覆盖率门槛 + clippy 零容忍体系的知识卡。覆盖测试金字塔 5 层（DAO 单元 897 / 集成测试 87×19 targets / 前端 82 / common 58）、`#[sqlx::test]` 独立内存 SQLite、init_full_test_env 真实启动顺序、cargo-llvm-cov PR38%/main45% 门槛、clippy `-D warnings` 后端 + wasm32 双端、Playwright E2E（仅本地）。**定位：新增测试放哪层、CI 覆盖率卡门槛怎么过、排查集成测试 Agent 入职失败因为 init_base_data 顺序错、写 sqlx::query! 报 .sqlx 元数据找不到时读。**

- **1124 测试 100% 通过率分层（金字塔自上而下）**（测试指南总览）：① 后端 984 = DAO/DAL/Domain/Handler/Pkg 单元测试 897（单个文件独立，每个文件顶部 `#[cfg(test)] mod tests`）+ 集成测试 87（跨模块，tests/integration/ 每个 .rs 一个独立 test target，互不污染）；② 前端 82（frontend/tests/，cargo test -p frontend 编译为 wasm32-unknown-unknown，通过 Dioxus Runtime mock DOM）；③ common crate 58（DTO 序列化反序列化 roundtrip、枚举 has_permission 三角色矩阵、PagedResult::map 泛型断言）。通过率 = 全部测试必须 green，CI 中任一测试 fail = PR 不可合并。
- **覆盖率门槛 PR 38% / main 45%（cargo-llvm-cov 严格 lines 模式）**（CI coverage stage）：`cargo llvm-cov --workspace --exclude frontend --fail-under-lines {38|45} --lcov --output-path coverage.lcov`；统计规则：test-only 代码（tests/*、*test_support.rs、cfg(test) mod 内部）不计入 lines 覆盖率（自动排除）。低于门槛 fail；main 合并到 release 前 45% 门槛更高，防止发布版本覆盖率退化；覆盖率低于阈值可加 PR 评论「此重构测试覆盖待后续补全，豁免一次」，但需 2 个 reviewer approve。
- **clippy `-D warnings` 双端零容忍（后端 x86 + 前端 wasm32）**（CI check stage）：① 后端默认 target x86_64-apple-darwin：`cargo clippy --workspace --exclude frontend --all-targets -- -D warnings`；② 前端 target wasm32-unknown-unknown（Dioxus WASM）：`cargo clippy -p frontend --target wasm32-unknown-unknown --all-targets -- -D warnings`；双端任一 warning 触发即 CI fail。常见清理：unused_import / dead_code / explicit_write（std::io::Write 未 import 时自动触发）/ match 多余 arm；clippy lint 配置在 `.cargo/config.toml`，自定义规则在 ai-orz-macros 里（禁止 role >= 2 数字比较等项目级红线）。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| tests/common/env.rs init_full_test_env | 集成测试初始化总入口 | 严格对齐 lib.rs::run() 真实顺序：pkg::init_all → service::init → producer::init → consumer::init → service::init_base_data().await；禁止跳过 init_base_data（默认 cron trigger / SuperAdmin 角色 需要注入）；独立 Arc Pool 每次 new | `:L1-L120` |
| tests/common/factories/* 工厂模块 | 实体快速构造 | AgentFactory：create_test_agent(ctx, org_id, status=Active)；ProjectFactory：create_test_project(ctx, owner_id)；UserFactory：create_test_user(ctx, org_id, role=Member)；所有工厂用 RandomId::new() 生成不冲突 id，测试并行不相互污染 | 见 factories/mod.rs |
| tests/integration/ 19 targets 20 文件 | 跨模块集成测试 | 文件名即 target 名：a2a_flow（A2A 协议 roundtrip）、agent_awaken（两阶段唤醒 IntentAnalyze + Awaken）、core_crud（所有 Domain 的基础 CRUD）、system_cron_triggers（2 条默认注入 + list_due 触发）、vector_degradation（向量存储 down → FTS5-only 降级） | 见 integration/ mod 结构 |
| src/**/sqlite_test.rs DAO 层单测 | 纯 DAO CRUD | `#[sqlx::test]` 宏自动创建独立 SQLite 内存库（每次 test 全新）；test_query_count_reuse_where：query 过滤 status=Pending → 断言 total == count(query) 同条件（push_query_filters 共享检查） | 见 cron_trigger/sqlite_test.rs |
| .github/workflows/ci.yml CI 流水线 | 4 阶段闸门 | stage1 check(clippy -D warnings x86+wasm32) → stage2 test(cargo test --workspace 全部) → stage3 coverage(cargo-llvm-cov 38%/45%) → stage4 build(release dist docker)；每个 stage 失败立即停，不往下跑 | 见 ci.yml jobs |
| docs/archive/design-archive/testing_guidelines.md 测试设计 | 金字塔分层 | 金字塔图：DAO单元(底) → DAL → Domain → Handler → 集成测试(中) → E2E(尖)；每一层责任：DAO 层只测 SQL（+边界），集成测试测真实链路（跨 Domain），E2E 测用户 happy path | `:L1-L50` |
| docs/design/sqlx_guide.md SQL 规范 | sqlx 0.8 + SQLite | STRICT 模式所有表必须；枚举 `as "status: TaskStatus"` 显式标注；.sqlx/ 目录必须 git 提交（query! 离线编译元数据，CI 无网也能过）；软删除 status=0 WHERE 默认过滤 | `:L1-L60` |

**章节来源**
- [env.rs:L1-L120](tests/common/env.rs#L1-L120)
- [testing_guidelines.md:L1-L80](docs/archive/design-archive/testing_guidelines.md#L1-L80)
- [ci.yml](.github/workflows/ci.yml)

---

## §3 CI 流水线 4 阶段闸门 + PR 合并条件

```
开发者 git push → GitHub Actions 触发 ai-orz-ci.yml
  ↓
STAGE 1. check(clippy -D warnings)【0.5-1min，最快闸门】
  |
  ├─ backend：cargo clippy --workspace --exclude frontend --all-targets -- -D warnings
  |    x86_64 target；common + macros + src 全检查；每个 warning = error
  └─ frontend：cargo clippy -p frontend --target wasm32-unknown-unknown -- -D warnings
       wasm32 target；Dioxus 组件未使用的 props、use_resource 未用 deps 也会报错
       ↑ 任何一者失败 → ❌ PR 评论「clippy warnings」→ 开发者需 fix

  ↓ STAGE 1 OK
STAGE 2. test(cargo test --workspace)【2-5min，最长阶段】
  |
  ├─ backend/common/macros：cargo test --workspace --exclude frontend
  |    897 单元 + 87 集成 = 984 个后端测试；并行 jobs=4 分块加速
  |    integration test 各 target 独立二进制，互不污染（进程隔离）
  ├─ frontend：cargo test -p frontend --target wasm32-unknown-unknown
  |    82 个前端组件/Hook 测试（Dioxus mock runtime，不需要真实浏览器）
  └─ common：58 个 DTO/Enum/PagedResult 单元（已在 workspace 中，无需单独跑）
       ↑ 任一 test fail → ❌ 必须 fix，不能跳过（ignore 需 issue 链接 + team approval）

  ↓ STAGE 2 OK
STAGE 3. coverage(cargo-llvm-cov lines 门槛)【1-2min】
  |
  cargo llvm-cov --workspace --exclude frontend \
    --fail-under-lines { PR=38 / main=45 } \
    --lcov --output-path coverage.lcov
  |
  - *test_support.rs、tests/*、cfg(test) 内的代码：自动排除 lines 统计
  - 低于阈值：GitHub comment 显示当前覆盖率 % vs 阈值 %
    例：「当前 37.2% < 38% 阈值，差 0.8%。建议补充 Task 状态机 query 过滤测试。」
  - 豁免：可 2 个 reviewer approve 标注 skip coverage，需附测试补全 TODO issue
       ↑ 低于阈值且无豁免 → ❌

  ↓ STAGE 3 OK
STAGE 4. build(release)【2-3min，仅 main branch 触发】
  |
  cargo build --release + docker build -t ai-orz:latest
  → 推送到 GitHub Packages，tag = git SHA + latest
  （仅 main branch，PR 不触发 build 节省资源）

  ↓ 4 STAGE ALL GREEN
✅ PR 可合并（另外需 ≥1 reviewer approve）
```

---

## §4 硬约束与回归红线（8 条，规范类卡数量多）

1. **集成测试必须走 init_full_test_env（完整启动顺序），不能自己造应该启动就有的数据**：在测试里手动 `INSERT INTO cron_triggers (id, cron)` 默认系统任务 = 违反；必须依赖 init_full_test_env → service::init_base_data → system::ensure_system_cron_triggers 注入。未来 init_base_data 逻辑变化时集成测试自动感知，不会 drift。
2. **`#[sqlx::test]` 必须用于所有 DAO/DAL 涉及 SQLite 的测试，不能共享连接池**：用 `static ref POOL` 单例让 DAO 测试共享 DB = 违反；测试 B 的 TRUNCATE 会清掉测试 A 写入的数据（并行测试顺序不确定）。#[sqlx::test] 自动每次新库，省心且安全。
3. **clippy `allow(xxx)` 需附注释说明原因，禁用裸 allow**：代码里 `#[allow(unused)]` 没说明 = 违反（下个版本 clippy 升级后 warning 消失，没人知道为什么 allow）；正确写法：`#[allow(clippy::too_many_arguments)] // 此函数是 10 参数聚合 DTO，构造器模式没必要拆分`。
4. **.sqlx/ 目录必须 git 提交（query! 宏离线元数据）**：CI 环境无 LLM API Key 无法运行 sqlx prepare 离线生成；如果新写了 `sqlx::query!` 没跑 `cargo sqlx prepare` 提交 .sqlx → CI 编译报错「找不到 query 元数据」；解决：本地跑一次 `cargo sqlx prepare --workspace`，git add .sqlx/ 再 push。
5. **覆盖率 cargo-llvm-cov 不能在 debug 模式跑（结果不准）**：需 `--release` 或默认 profile（llvm-cov 自己处理）；debug 模式对 drop_in_place / 泛型展开统计偏高；CI 里 cargo-llvm-cov 的 profile 已在 ci.yml 固定，不要本地 `cargo test` 后拿「我本地测有 50%」来杠。
6. **Playwright E2E 不进 CI（仅本地 `just e2e`）**：E2E 需要登录→调用真实 LLM，成本高且偶发 flaky（模型响应超时）。CI 不跑；发布前 release manager 本地跑一次，失败视频自动保存 e2e-artifacts/ 附到 PR；纯登录/创建 Agent 等不涉及 LLM 的 E2E 子集可后续改加进 CI。
7. **集成测试 19 targets 不能有相互依赖的前置条件（进程隔离）**：agent_management_test 的 agent_id 不能被 a2a_flow_test 复用；每个 target 自己 init_full_test_env 造独立数据。跨 target 复用数据会导致：单独跑 cargo test a2a_flow 通过，但一起跑 `cargo test` 并行随机失败（Heisenbug）。
8. **前端 wasm32 单元测试不 mock 网络请求，直接测纯逻辑组件**：测试 GraphCanvas、PagedResult::map UI 渲染、use_resource deps 变化触发不发 HTTP；HTTP 用真实 API client 测试放集成/Playwright E2E。否则 wasm32 里模拟网络需要 wasm-bindgen-test 引入 js-sys 庞大依赖，测试启动超 10 秒无法接受。
