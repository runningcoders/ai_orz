# 项目测试规范

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

## 代码审查检查清单

提交测试代码时，必须检查以下事项：

- [ ] **没有重复的初始化代码**：所有测试共用的初始化逻辑已抽取到公共函数
- [ ] **初始化函数命名规范**：使用 `init_xxx_test_env` 格式
- [ ] **初始化函数职责单一**：仅包含环境初始化，不包含测试业务逻辑
- [ ] **测试数据工厂化**：重复的测试数据构造已抽取工厂函数
- [ ] **注释清晰**：公共函数有清晰的文档注释说明用途

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
| **覆盖率** | `cargo tarpaulin --fail-under 35` | baseline 40.79%，门槛 35% 留 ~5% 缓冲 |
| **集成测试** | `cargo test --test '*'`（并行） | 29 个集成测试，3.7s 跑完 |

**覆盖率说明**：CI 命令 `cargo tarpaulin --lib --engine llvm` 只统计 ai_orz 主 crate 的单元测试覆盖率（baseline 40.79%），不包含 common/ai_orz_macros 等 workspace 其他 crate，也不包含集成测试覆盖。如果加上集成测试，实际覆盖率会更高。未来可考虑改用 `--tests` 标志统计包含集成测试的覆盖率。

### 集成测试套件清单

| 套件 | 文件 | 测试数 | 覆盖范围 |
|------|------|--------|----------|
| Auth & SysInit | `auth_sysinit_test.rs` | 4 | JWT Cookie 认证、系统初始化、401/200 protected route |
| Core CRUD | `core_crud_test.rs` | 3 | Agent/Project/Task CRUD 闭环 + 状态流转 |
| Message Delivery | `message_delivery_test.rs` | 2 | send_message 持久化 + SSE 连接冒烟 |
| Vector Degradation | `vector_degradation_test.rs` | 3 | 无 embedding provider 时主流程仍可用（降级契约守护） |
| A2A Flow | `a2a_flow_test.rs` | 2 | agent card 发现 + JSON-RPC tasks/send→get |
| 宏集成 | `macro_test.rs` | 15 | generate_http_handler 宏各参数组合边界场景 |

### 性能优化历史

| 阶段 | 集成测试耗时 | 说明 |
|------|-------------|------|
| 优化前（并行） | 238s | 多个 bootstrap 同时创建 embedding provider 互相干扰触发 FastEmbed |
| 短期方案（串行） | 107s | CI 加 `--test-threads=1` |
| 长期方案（可选 embedding） | 3.7s | `embedding_model: Option<>` + bootstrap 传 None |

---

**最后更新**：2026-07-27
**维护者**：AI Orz 开发团队
