# 项目测试规范

> 🎯 **本文档定位**：ai_orz 统一测试编写规范——公共初始化抽取、风格对齐、测试分层（单元/集成/E2E）与可维护性约定
> 状态：v1.0（2026-08-15 整理）
> 查阅场景：新增单元/集成测试、审查测试代码是否符合初始化抽取红线、排查测试脚手架重复代码时打开；具体 fixture 位置直接看 tests/ 目录
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 整体分层架构
> - [browser_e2e_test_design.md](./browser_e2e_test_design.md) — 浏览器端 E2E/操作验证用例 Playbook 体系
> - [sqlx_guide.md](./sqlx_guide.md) — SQLite + SQLx 0.8 规范与 #[sqlx::test] 隔离
> - 【② Plan 落地】[Agent管理集成测试.md](../plan/Agent管理集成测试.md) — 19 集成测试 target 清单 + onboard_agent 回滚断言矩阵
> - 【② Plan 落地】[AOP生产消费事件中心重构.md](../plan/AOP生产消费事件中心重构.md) — event_delivery 集成测试链路：publish→消费者 ack→断言表记录
> - 【② Plan 落地】[身份凭证Domain统一CRUD重构.md](../plan/身份凭证Domain统一CRUD重构.md) — credential_crud 集成测试 AES256-GCM 加解密 roundtrip 断言
> - 【③ Wiki 百科】[测试指南.md](docs/wiki/zh/content/测试指南/测试指南.md) — 1124 测试分布总览：984 后端(897单元+87集成) + 82 前端 + 58 common
> - 【③ Wiki 百科】[端到端测试基础设施.md](docs/wiki/zh/content/测试指南/端到端测试基础设施.md) — Playwright E2E 本地 `just e2e` + 失败视频自动保存
> - 【③ Wiki 百科】[持续集成与发布工作流.md](docs/wiki/zh/content/基础设施/持续集成与发布工作流.md) — CI 四阶段闸门 check/test/coverage/build + cargo-llvm-cov 38%/45% 阈值
> - 【④ RAG 知识卡】[测试与质量工程](docs/wiki/knowledge/zh/%E6%B5%8B%E8%AF%95%E4%B8%8E%E8%B4%A8%E9%87%8F%E5%B7%A5%E7%A8%8B%EF%BC%9A1124%E6%B5%8B%E8%AF%95100%%E9%80%9A%E8%BF%87%20+%20984%E5%90%8E%E7%AB%AF82%E5%89%8D%E7%AB%AF%20+%2087%E9%9B%86%E6%88%90%E6%B5%8B%E8%AF%9519targets%20+%20cargo-llvm-cov%2038%25/45%25%E9%97%A8%E6%A7%9B%20+%20clippy%E9%9B%B6%E5%AE%B9%E5%BF%8D+Playwright%20E2E/%E6%B5%8B%E8%AF%95%E4%B8%8E%E8%B4%A8%E9%87%8F%E5%B7%A5%E7%A8%8B%EF%BC%9A1124%E6%B5%8B%E8%AF%95100%%E9%80%9A%E8%BF%87%20+%20984%E5%90%8E%E7%AB%AF82%E5%89%8D%E7%AB%AF%20+%2087%E9%9B%86%E6%88%90%E6%B5%8B%E8%AF%9519targets%20+%20cargo-llvm-cov%2038%25/45%25%E9%97%A8%E6%A7%9B%20+%20clippy%E9%9B%B6%E5%AE%B9%E5%BF%8D+Playwright%20E2E.md) — §4 阶段 CI 闸门 §init_full_test_env 启动顺序 §8 条红线

## 概述

本文档定义了 ai_orz 项目的测试编写规范，旨在统一测试代码风格，减少重复代码，提高测试可维护性。

## 核心原则

### ✅ 原则 1：初始化代码必须抽取公共函数

**所有测试共用的初始化逻辑，必须抽取到独立的公共函数中，禁止每个测试重复编写。**

#### 反模式（禁止）

```rust
#[sqlx::test]
async fn test_a(pool: SqlitePool) {
    // ❌ 重复初始化代码
    crate::service::dao::agent::init();
    crate::service::dal::agent::init();
    super::init();
    let domain = domain();
    // ...
}

#[sqlx::test]
async fn test_b(pool: SqlitePool) {
    // ❌ 同样的初始化代码重复写一遍
    crate::service::dao::agent::init();
    crate::service::dal::agent::init();
    super::init();
    let domain = domain();
    // ...
}
```

#### 正确模式（必须）

```rust
/// ✅ 抽取公共初始化函数
fn init_test_env() {
    // 所有 DAO 初始化
    crate::service::dao::agent::init();
    crate::service::dao::tool::init();
    crate::service::dao::skill::init();
    
    // 所有 DAL 初始化
    crate::service::dal::agent::init();
    crate::service::dal::tool::init();
    crate::service::dal::skill::init();
    
    // Domain 初始化
    super::init();
}

#[sqlx::test]
async fn test_a(pool: SqlitePool) {
    init_test_env();  // ✅ 一行调用
    let domain = domain();
    // ...
}

#[sqlx::test]
async fn test_b(pool: SqlitePool) {
    init_test_env();  // ✅ 一行调用
    let domain = domain();
    // ...
}
```

### ✅ 原则 2：初始化函数命名规范

初始化函数命名统一使用 `init_xxx_test_env` 格式：

| 范围 | 命名示例 | 说明 |
|------|----------|------|
| 单个模块 | `init_hr_test_env()` | HR Domain 测试初始化 |
| 多个模块 | `init_tool_domain_test_env()` | Tool Domain 测试初始化 |
| 全局集成测试 | `init_integration_test_env()` | 集成测试全局初始化 |

### ✅ 原则 3：初始化函数放置位置

- **单元测试**：放在对应 `*_test.rs` 文件顶部，所有测试函数之前
- **集成测试**：放在 `tests/common/mod.rs` 等公共模块中，所有测试可导入
- **跨模块复用**：考虑提升到更上层的公共模块，避免跨测试文件重复定义

### ✅ 原则 4：初始化函数内容边界

初始化函数**只包含**：
- DAO 单例初始化
- DAL 单例初始化
- Domain 单例初始化
- 全局配置加载
- 其他所有测试共用的前置操作

初始化函数**不包含**：
- 特定测试用例的测试数据构造
- 特定测试用例的业务操作调用
- 任何与测试用例逻辑相关的代码

## 进阶模式

### 模式 A：带返回值的初始化函数

如果多个测试需要共同的对象，可在初始化函数中返回：

```rust
fn init_hr_test_env() -> (Arc<dyn HrDomain>, RequestContext) {
    // 初始化所有依赖
    crate::service::dao::agent::init();
    crate::service::dal::agent::init();
    super::init();
    
    // 返回公共对象
    let ctx = RequestContext::new_simple("admin", pool);
    (domain(), ctx)
}

// 测试中使用：
#[sqlx::test]
async fn test_create(pool: SqlitePool) {
    let (domain, ctx) = init_hr_test_env(pool);  // ✅ 一行搞定
    // ...
}
```

### 模式 B：分层初始化（按依赖层级）

对于复杂依赖场景，可分层初始化：

```rust
/// 底层：仅初始化 DAO
fn init_daos() {
    crate::service::dao::agent::init();
    crate::service::dao::tool::init();
}

/// 中层：初始化 DAL（依赖 DAO）
fn init_dals() {
    init_daos();
    crate::service::dal::agent::init();
    crate::service::dal::tool::init();
}

/// 上层：初始化 Domain（依赖 DAL）
fn init_hr_test_env() {
    init_dals();
    super::init();
}
```

### 模式 C：测试数据工厂函数

除了初始化环境，测试数据构造也应抽取工厂函数：

```rust
/// ✅ Agent 测试数据工厂
fn create_test_agent(name: &str) -> Agent {
    let po = AgentPo::new(
        name.to_string(),
        vec!["worker".to_string()],
        "Test agent".to_string(),
        vec![],
        "".to_string(),
        "provider-id-1".to_string(),
        "admin".to_string(),
    );
    Agent::from_po(po)
}

// 测试中使用：
#[sqlx::test]
async fn test_create(pool: SqlitePool) {
    init_hr_test_env();
    let agent = create_test_agent("TestAgent");  // ✅ 一行创建测试数据
    // ...
}
```

## 现有最佳实践参考

### 示例 1：HR Domain 测试（已实现）

**文件**：`src/service/domain/hr/agent_test.rs`

```rust
/// ✅ 优秀示例：HR Domain 测试初始化
fn init_hr_test_env() {
    // 初始化所有 DAO
    crate::service::dao::agent::init();
    crate::service::dao::tool::init();
    crate::service::dao::skill::init();
    crate::service::dao::tool_call::init();
    
    // 初始化所有 DAL
    crate::service::dal::agent::init();
    crate::service::dal::tool::init();
    crate::service::dal::skill::init();
    
    // 初始化 HR Domain
    super::init();
}

// 4个测试全部复用：
// test_create_and_find_by_id → init_hr_test_env()
// test_list_agents → init_hr_test_env()
// test_update_agent → init_hr_test_env()
// test_delete_agent → init_hr_test_env()
```

**收益**：
- 消除了 4 份重复的初始化代码（每份 3 行 × 4 = 12 行）
- 后续新增依赖只需修改 1 处，无需修改所有测试
- 初始化逻辑统一维护，不易遗漏

### 示例 2：Message Channel 测试（已实现）

**文件**：`src/service/domain/message_channel_test.rs`

```rust
/// ✅ 优秀示例：Message Channel 测试初始化
fn init_message_channel_test_env() {
    crate::service::dao::message_channel::init();
    crate::service::dao::lark::init();
    crate::service::dao::wechat::init();
    crate::service::dao::slack::init();
    crate::service::dao::email::init();
    crate::service::dao::webhook::init();
    
    crate::service::dal::message_channel::init();
    super::init();
}

// 14 个测试全部复用此初始化函数
```

**收益**：14 个测试 × 7 行初始化 = 消除 98 行重复代码

## 代码审查检查项（提交测试代码时必过）

| 序号 | 检查点 | 判定标准 |
|------|--------|---------|
| 1 | 没有重复的初始化代码 | 所有测试共用的初始化逻辑已抽取到公共函数 |
| 2 | 初始化函数命名规范 | 统一使用 `init_xxx_test_env` 格式 |
| 3 | 初始化函数职责单一 | 仅包含环境初始化，不包含测试业务逻辑 |
| 4 | 测试数据工厂化 | 重复的测试数据构造已抽取工厂函数 |
| 5 | 注释清晰 | 公共函数有清晰的文档注释说明用途 |

## 重构指南

如果你遇到以下情况，请考虑重构：

1. **同一初始化代码重复出现 2 次以上** → 立即抽取
2. **初始化代码超过 3 行** → 考虑抽取
3. **新增依赖需要修改 N 个测试的初始化** → 说明应该抽取公共函数了

### 重构步骤

1. 找到所有测试中重复的初始化代码
2. 在测试文件顶部创建 `init_xxx_test_env()` 函数
3. 将重复代码移动到该函数中
4. 将所有测试中的初始化代码替换为函数调用
5. 运行测试验证重构正确

## 收益总结

| 收益 | 说明 |
|------|------|
| **减少重复代码** | 消除 70%+ 的测试样板代码 |
| **降低维护成本** | 新增依赖只需修改 1 处，无需修改所有测试 |
| **统一初始化逻辑** | 避免部分测试遗漏初始化步骤导致的偶发失败 |
| **提高可读性** | 测试用例聚焦业务逻辑，而非环境准备 |
| **降低出错概率** | 初始化逻辑只需编写和验证 1 次 |

## 相关文档

- [分层架构实践指南](./LAYERED_ARCHITECTURE_PRACTICE.md)
- [代码风格规范]（待补充）

---

## 集成测试规范（2026-07-27 新增）

### 概述

集成测试位于 `tests/integration/` 目录，通过 HTTP 端到端验证 Adapter → Domain → DAL → DAO 全链路。与单元测试（`#[sqlx::test]` 独立内存 DB）不同，集成测试共享全局 DB（通过 `OnceCell` 串行化初始化），靠唯一 ID 隔离。

### 测试脚手架（tests/common/）

```
tests/common/
├── mod.rs              # 模块导出
├── env.rs              # init_full_test_env（OnceCell 串行化初始化）
├── app.rs              # TestApp + HTTP helpers（get/post/get_with_jwt 等）
├── assertions.rs       # assert_api_ok / assert_api_error
└── factories/
    ├── user_factory.rs     # bootstrap_system + bootstrap_and_login
    ├── agent_factory.rs    # create_test_agent
    └── project_factory.rs  # create_test_project
```

### init_full_test_env 设计要点

**用 `OnceCell` 串行化初始化**，避免并行测试时多个测试同时初始化全局 DAO/DAL/Domain 单例导致竞争。

```rust
static TEST_ENV: OnceCell<()> = OnceCell::const_new();

pub async fn init_full_test_env(pool: SqlitePool) -> RequestContext {
    TEST_ENV.get_or_try_init(|| async {
        // 1. 加载全局 AppConfig
        // 2. pkg::storage 初始化（临时目录 + InMemory 向量库）
        // 3. pkg::jwt / tool_tracing / tool_registry 初始化
        // 4. service::init() 一行替代 30+ 个手动 DAO/DAL/Domain init
    }).await.unwrap();
    new_test_ctx("test-integration-user", pool)
}
```

**设计原则**：调用 `service::init()` 而不是手动一个个 init DAO/DAL/Domain，与 main.rs 启动流程对齐。新增 DAO/DAL 时无需改测试代码。

> **关于第二阶段 `init_base_data`**：`init_full_test_env` 内部**已经按真实启动顺序**调用了 `producer::init() → consumer::init() → service::init_base_data().await`，所以**测试内部不需要再手动调**。这样做的好处：
> - 对齐真实 `ai_orz::run()` 的启动顺序（AOP 基础设施就绪 → 基础数据注入）
> - system cron triggers（agent_rest / project_followup）的 baseline 在所有共享 DB 的测试里都一致
> - `service::init_base_data()` 本身是幂等的，多次调用安全；测试如果在自己函数里再次调也没问题
> - 目前只有 system domain 的 2 条默认 triggers 会被注入，对其它不关心 cron 的集成测试无副作用

### bootstrap_system 设计要点

`bootstrap_system` 通过 HTTP 调用 `POST /api/v1/organization/initialize` 创建组织 + 管理员 + chat provider。

**关键决策：`embedding_model` 传 `None`**

```rust
let req = InitializeSystemRequest {
    // ...
    chat_model: ModelProviderInitConfig { ... },
    embedding_model: None,  // ✅ 跳过 embedding provider 创建
};
```

**原因**：
1. **测试速度**：DB 里永远没有 embedding provider，所有实体创建走 `Ok(None)` 降级路径，永不触发 FastEmbed 模型加载（75s/测试 → 0.3s/测试）
2. **降级契约验证**：`vector_degradation_test` 显式验证"无 embedding provider 时主流程仍可用"
3. **避免并行竞争**：多个测试同时创建 embedding provider 会互相干扰

### 向量降级契约（强制执行）

**核心契约**：所有 `embed_entity` 调用失败时必须降级为 `Ok(None)`，主流程永远返回 `Ok(())`，不能因为向量索引失败阻塞业务。

**守护测试**：`tests/integration/vector_degradation_test.rs` 包含 3 个测试：
1. `test_agent_create_succeeds_without_embedding_provider` — 创建 Agent 验证降级
2. `test_project_create_succeeds_without_embedding_provider` — 创建 Project 验证降级
3. `test_full_crud_loop_without_embedding_provider` — Agent → Project → Task → Message 全链路冒烟

**禁止破坏降级机制**：任何重构导致这 3 个测试失败都视为破坏性变更，必须修复或回滚。

### 集成测试编写规范

#### ✅ 必须使用 bootstrap_and_login

```rust
#[sqlx::test]
async fn test_xxx(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 用 jwt 调用受保护的 API
    let (status, body) = app.get_with_jwt("/api/v1/hr/agents", &jwt).await;
    // ...
}
```

#### ✅ 用唯一 ID 隔离测试数据

```rust
let agent_name = format!("TestAgent-{}", uuid::Uuid::now_v7());
```

#### ✅ 用 assert_api_ok / assert_api_error 验证响应

```rust
let data = crate::common::assert_api_ok(status, &body);
assert_eq!(data.get("id").and_then(|v| v.as_str()), Some(agent_id.as_str()));
```

### CI 质量门槛（2026-07-27 新增）

| 门槛 | 配置 | 说明 |
|------|------|------|
| **clippy** | `cargo clippy --all-targets -- -D warnings` | 零容忍，任何 warning 都会阻断 CI |
| **覆盖率** | `cargo llvm-cov --fail-under-lines 35` | 纯 LLVM source-based coverage，门槛 35% |
| **集成测试** | `cargo test --test '*'`（并行） | 29 个集成测试，3.7s 跑完 |

**覆盖率说明**：CI 用 `cargo llvm-cov --lib` 只统计 ai_orz 主 crate 的单元测试覆盖率（不含集成测试）。`--ignore-filename-regex` 过滤依赖库、测试脚手架、build script，只统计 workspace 代码。如果加上集成测试（`--tests`），实际覆盖率会更高。

### 集成测试套件清单

| 套件 | 文件 | 测试数 | 覆盖范围 |
|------|------|--------|----------|
| Auth & SysInit | `auth_sysinit_test.rs` | 4 | JWT Cookie 认证、系统初始化、401/200 protected route |
| Core CRUD | `core_crud_test.rs` | 3 | Agent/Project/Task CRUD 闭环 + 状态流转 |
| Message Delivery | `message_delivery_test.rs` | 7 | send_message 持久化 + SSE 连接冒烟 + **Agent→User 消息角色/双向列表校验** + **端到端 SSE 推送内容验证** + **Webhook 渠道失败聚合不抛错** + **无渠道无 SSE 边界 OK** + **不可达 URL Webhook 不 panic** |
| Vector Degradation | `vector_degradation_test.rs` | 3 | 无 embedding provider 时主流程仍可用（降级契约守护） |
| A2A Flow | `a2a_flow_test.rs` | 2 | agent card 发现 + JSON-RPC tasks/send→get |
| 宏集成 | `macro_test.rs` | 15 | generate_http_handler 宏各参数组合边界场景 |
| Agent Management | `agent_management_test.rs` | 12+3 | Agent CRUD + 向量搜索（3 个 ignored 真实向量测试） |
| Tool/Skill Vector | `tool_skill_vector_test.rs` | 2+5 | FTS5 搜索 + 向量语义搜索/索引维护/混合排序（5 个 ignored） |
| Message Vector | `message_vector_test.rs` | 2+4 | Message FTS5 + 向量搜索/索引维护/混合排序（4 个 ignored） |
| Project/Task Vector | `project_task_vector_test.rs` | 4+4 | Project/Task FTS5 + 向量搜索/索引维护/混合排序（4 个 ignored） |
| Agent Awaken | `agent_awaken_test.rs` | 6+1 | Consumer 编排 + awakening 循环按 control_mode 分发（execute_auto/execute_manual）+ 真实 LLM（1 个 ignored） |
| Tool Call | `tool_call_test.rs` | 5+1 | debug_call_tool（Builtin/HTTP/SSRF）+ execute_manual→internal 工具→call_tool 异步链 + 真实 LLM execute_auto 工具（1 个 ignored） |
| Memory Cognition | `memory_test.rs` | 4+3 | 短期记忆 query/task_id 过滤 + FTS5 搜索 + 知识节点搜索 + 真实向量语义搜索/索引维护/混合排序（3 个 ignored） |

### 记忆认知测试模式（2026-08-04 新增）

记忆认知集成测试覆盖**短期记忆**和**图谱记忆（知识节点）**的查询与搜索能力：

| 场景 | 测试内容 | 是否需 LLM | 测试策略 |
|------|----------|-----------|----------|
| **query 基础查询** | 创建短期记忆 → query_memory 验证 | 否 | CI 默认 |
| **task_id 注意力过滤** | 创建不同 task_id 记忆 → 验证过滤生效 | 否 | CI 默认 |
| **FTS5 短期记忆搜索** | 创建含特定关键词的记忆 → search_memory FTS5 召回 | 否 | CI 默认 |
| **FTS5 知识节点搜索** | 创建知识节点 → search_memory FTS5 召回（trigram 分词） | 否 | CI 默认 |
| **真实向量语义搜索** | 创建语义相关但无关键词重叠的记忆 → 向量召回 | 是 | `#[ignore]` 真实 Embedding |
| **向量索引维护** | 创建 → 搜索验证 → 删除 → 搜索验证已删除 | 是 | `#[ignore]` 真实 Embedding |
| **混合排序** | Hybrid（FTS5+向量）vs Vector-only 排序验证 | 是 | `#[ignore]` 真实 Embedding |

**关键设计**：
- **数据准备**：直接调用 domain layer（`runtime_domain().memory().create()`）创建记忆，绕过 HTTP handler 的 `ctx.agent_id()` 限制
- **查询验证**：通过 HTTP 端点（`query_memory`/`search_memory`）验证端到端能力
- **agent_id 参数**：测试中传入 `agent_id` 指定查询目标 Agent（handler 优先使用参数 agent_id，兜底 ctx.agent_id()）
- **全局搜索修复**：DAL 层 `search_short_term`/`search_knowledge_nodes` 已修复空 agent_id 时的过滤逻辑——短期记忆空 agent_id 不过滤（全局搜索），知识节点空 agent_id 只返回 published 节点

### 工具调用测试模式（2026-08-04 新增）

工具调用集成测试覆盖**三条工具调用路径**，采用 **CI 默认 + 真实 LLM ignore** 双层模式：

| 路径 | 触发方式 | 是否需 LLM | 测试策略 |
|------|----------|-----------|----------|
| **debug_call_tool** | HTTP 端点同步调用 | 否 | CI 默认（Builtin/HTTP/SSRF） |
| **Manual 异步消息链** | Consumer 处理 ToolCallRequest → ToolCallResult | 否 | CI 默认（Mock HTTP Server） |
| **Auto awaken 工具执行** | awaken 时通过 execute_auto 调用 | 是 | `#[ignore]` 真实 LLM |

**关键设计**：
- **Mock HTTP Server**：测试内启动绑定随机端口的 TCP listener，返回固定响应，用于验证 HTTP 工具调用与 SSRF 防护
- **SSRF 防护**：通过创建 `allow_local_network=false` 的 HTTP 工具并断言调用失败，验证多层防护（scheme/域名/本地网络/DNS pinning）
- **ToolCallLogger 验证**：Manual 路径通过 execute_manual → 特殊 internal 工具转发 → call_manual_tool_for_agent → call_tool 触发并查询 trace 记录，验证 call_id/tool_id/input/output/status 字段

```rust
// 默认测试：无 LLM，验证工具调用编排与 trace
#[sqlx::test]
async fn test_consumer_tool_call_request_chain(pool: SqlitePool) { ... }

// ignored 测试：需 TEST_LLM_API_KEY，验证真实 LLM 自动调用工具
#[sqlx::test]
#[ignore = "requires real LLM API key in .env (TEST_LLM_API_KEY)"]
async fn test_real_llm_auto_tool_call(pool: SqlitePool) { ... }
```

### 向量搜索测试模式（2026-08-04 新增）

向量搜索集成测试统一采用 **CI 默认 + 真实 API ignore** 双层模式：

```rust
// 默认测试：无 embedding provider，走 FTS5 路径，CI 安全
#[sqlx::test]
async fn test_xxx_fts5_search(pool: SqlitePool) { ... }

// ignored 测试：需 TEST_EMBEDDING_API_KEY，验证 LanceDB + Embedding API
#[sqlx::test]
#[ignore = "requires real Embedding API key in .env (TEST_EMBEDDING_API_KEY)"]
async fn test_real_xxx_vector_search(pool: SqlitePool) { ... }
```

**ignored 测试覆盖场景**：语义搜索（"神经网络"→"深度学习"）、向量索引维护（创建→更新→删除）、混合排序（FTS5 > Vector > Keyword）。

### 消息投递与 SSE/Webhook 推送测试模式（2026-08-05 新增）

消息投递集成测试覆盖**两条主路径**（Agent→User 定向发送 + Consumer 触发的 deliver_message）与**三种推送方式**（列表拉取 / SSE 主动推 / 外部渠道 Webhook），采用「域层直接调用」+「HTTP 端到端」混合策略：

| 场景 | 测试函数 | 验证重点 | 触发方式 |
|------|----------|----------|----------|
| Agent→Agent 消息持久化 | `test_send_message_persists_record` | `POST /messages/agents` → 记录入库 → `GET /messages?to_id=...` 返回 | HTTP handler |
| SSE 端点连通性 | `test_sse_endpoint_returns_event_stream` | `/messages/sse` 连接返回 200，流式响应 | HTTP handler |
| **Agent→User 消息 + 角色校验 + 双向列表** | `test_send_message_to_user_via_tool_persists_and_listable` | `from_role=Agent`、`to_role=User` 正确；`to_id=<user>` 和 `from_id=<agent>` 列表都能命中；`send_message` neural 工具已注册 | `message_domain.send_to_user`（绕开 debug-call 缺失 agent 身份） |
| **端到端 SSE 推送内容验证** | `test_sse_push_delivers_message_payload_to_subscriber` | 后台任务订阅 SSE → 主线程 `deliver_message` → 校验收到的 event JSON 含正确 `message_id` + `content` | `TestApp::get_with_jwt_collect_sse_events` + domain 层 `deliver_message` |
| **Webhook 渠道投递 + 失败聚合** | `test_webhook_channel_delivers_message_to_mock_server` | 创建真实 TCP mock 服务器 + Webhook Channel；`total=1`、`failed=1`；错误信息落入 `ChannelDeliveryDetail.error`；**deliver_message 仍返回 Ok**，不向上抛（保证 SSE/其他渠道继续） | HTTP 建 Channel + domain 层 `deliver_message` |
| **无渠道无 SSE 边界** | `test_deliver_message_no_channels_and_no_sse_still_returns_ok` | total/success/failed 全 0；`deliver_message` 返回 Ok | domain 层 `deliver_message` |
| **不可达 URL Webhook 不 panic** | `test_webhook_channel_invalid_url_reports_failed_without_panicking` | 不可达 URL → 错误进 `details.error`；函数返回 Ok | domain 层 `deliver_message` |

**关键设计：**

- **`TestApp` 新增 `#[derive(Clone)]` + `get_with_jwt_collect_sse_events()`**：axum 0.8 的 `Body` 不直接实现 `StreamExt::next()`，必须通过 `http_body_util::BodyExt::into_data_stream()` 转 Stream；SSE 按 `\n\n` 拆包，自动过滤 `data: keep-alive` ping 行。
- **`extern crate common as common_ext`**：`tests/integration/*` 文件顶部 `#[path = "../common/mod.rs"] mod common;` 会遮蔽外部依赖 `common` crate，导入 `common::enums::*` 必须重命名 extern（否则报 `could not find enums in common`）。
- **`generate_http_handler` 方法推断规则**：Params struct 字段**未**标 `#[param(source = "query")]` 时，宏会把它当成 body 字段，生成的 handler 是 `POST + Json<T>`（不是 GET query）。列表接口优先用 `POST /xxx/query`，不要假设 GET 可用。
- **`ChannelType` JSON 序列化**：enum 未带 `#[serde(rename = ...)]`，默认序列化为 Rust 变体名大写开头（`"Webhook"` 而不是 `4` 或 `"webhook"`），建渠道请求传 `channel_type: "Webhook"` 字符串。
- **通用 Webhook 推送当前未实现**：`message_channel_dal.deliver_message` 对 `ChannelType::Webhook` 返回 `unsupported_operation 通用 Webhook 推送功能尚未实现`。测试已显式捕获这个错误，等实现后只需把 `assert_eq!(delivery.failed, 1)` 改回 `assert_eq!(delivery.success, 1)` + mock 服务器收包校验即可。

### `TestApp` HTTP 辅助方法清单（`tests/common/app.rs`）

| 方法 | 说明 |
|------|------|
| `new(pool)` | 创建 `axum::Router` 包装器 |
| `get/post` | 匿名请求，自动加 `Content-Type: application/json` |
| `get_with_jwt/post_with_jwt/put_with_jwt/delete_with_jwt` | 带 JWT Cookie（`ai_orz_jwt=<token>`） |
| `get_with_jwt_status_only` | 只返回 StatusCode，用于 SSE 等流式场景不读完 body |
| **`get_with_jwt_collect_sse_events(path, jwt, max_events, max_wait)`** | ✨ 连接 SSE，收集最多 `max_events` 个 JSON data 事件或超时返回，自动过滤 keep-alive |

---

**最后更新**：2026-08-05
**维护者**：AI Orz 开发团队
