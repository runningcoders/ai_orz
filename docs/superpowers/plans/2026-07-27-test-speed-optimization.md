# Test Speed Optimization Implementation Plan

> **Status: ✅ COMPLETED (2026-07-27)**
>
> 5 个 Task + 1 前端修复全部完成。集成测试从 238s 降到 3.7s（63 倍提升）。
>
> 关键成果：
> - 集成测试并行：238s → 3.7s（63x 提升）
> - 集成测试串行：107s → 3.7s（28x 提升）
> - `initialize_system` 的 `embedding_model` 改为可选（`Option<>` + `#[serde(default)]`）
> - 删除 `disable_embedding_provider` / `bootstrap_login_and_disable_embedding` 两个测试 helper
> - 修复前端 reception.rs 初始化表单（加模型配置字段，预先存在的 bug）
>
> Commits: f8dc339 → 48fcbb4 → 7a4b72f → f872a98 → 1662914（前端修复）
>
> **根因**：并行测试时 embedding provider 互相干扰触发 FastEmbed 模型加载（75s/测试）。方案：bootstrap_system 传 None 跳过创建，DB 里永远没有 embedding provider，走 `Ok(None)` 降级路径。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 解决集成测试并行运行时因 embedding provider 互相干扰触发 FastEmbed 模型加载（75s）的问题，让集成测试从 238s 降到 < 30s。

**Architecture:** 两步走：短期方案在 CI 中分开跑单元测试（并行）和集成测试（串行）立即生效；长期方案修改 `initialize_system` 接口让 `embedding_model` 可选，`bootstrap_system` 传 None 跳过创建 embedding provider，从根本上消除并行干扰，让集成测试可以真正并行。

**Tech Stack:** Rust, axum, sqlx, cargo test。PROTOC=/opt/homebrew/bin/protoc 前缀。

**问题根因分析（实测数据）：**

| 运行方式 | 耗时 | 原因 |
|---------|------|------|
| 单文件 `--test-threads=3` | 0.30s | 无跨文件竞争 |
| 全部 `--test-threads=4` | 238s | 跨文件并行触发 FastEmbed 模型加载 |
| 全部 `--test-threads=1` | 107s | 串行避免 FastEmbed |

并行运行时：
1. 测试 A `bootstrap_system` 创建 embedding provider X
2. 测试 B `bootstrap_system` 创建 embedding provider Y（DB 里有 X 和 Y）
3. 测试 A `disable_embedding_provider` 删除 X（DB 里还有 Y）
4. 测试 A `create_agent` → `get_default_embedding_provider` 返回 Y → 触发 FastEmbed 加载（75s）

---

## File Structure

**短期方案（Task 1）：**
- Modify: `.github/workflows/rust.yml` —— 分离单元测试和集成测试的线程数

**长期方案（Task 2-4）：**
- Modify: `common/src/api/organization.rs` —— `embedding_model` 改为 `Option<ModelProviderInitConfig>`
- Modify: `src/handlers/organization/initialize_system.rs` —— 跳过 None 的 embedding provider 创建
- Modify: `tests/common/factories/user_factory.rs` —— `bootstrap_system` 传 None + 删除 `disable_embedding_provider`
- Modify: `tests/integration/*.rs` —— 删除 `bootstrap_login_and_disable_embedding` 调用，改用 `bootstrap_and_login`

---

## Phase 1: 短期方案 — CI 分离单元/集成测试线程数

### Task 1: 修改 CI workflow 分离测试并行度

**Files:**
- Modify: `.github/workflows/rust.yml`

- [ ] **Step 1: 读现有 CI workflow**

读 `.github/workflows/rust.yml`，找到 "Run unit tests" 和 "Run integration tests" 步骤。

- [ ] **Step 2: 单元测试保持并行，集成测试改为串行**

修改 `.github/workflows/rust.yml`：

```yaml
    - name: Run unit tests (parallel)
      run: cargo test --lib --verbose

    - name: Run integration tests (serial)
      run: cargo test --test '*' --verbose -- --test-threads=1
```

**为什么集成测试用 `--test-threads=1`？**
因为集成测试共享全局 DB，并行运行时 embedding provider 互相干扰触发 FastEmbed 模型加载（75s/测试）。串行运行时每个测试的 bootstrap → disable → create 窗口不被干扰，永远不会触发 FastEmbed。实测从 238s 降到 107s。

- [ ] **Step 3: 验证本地等价命令通过**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo test --lib 2>&1 | tail -5
PROTOC=/opt/homebrew/bin/protoc cargo test --test '*' -- --test-threads=1 2>&1 | tail -20
```

预期：全部 PASS，总时间约 107s（单元 ~80s + 集成 ~27s）。

- [ ] **Step 4: 提交**

```bash
git add .github/workflows/rust.yml
git commit -m "ci: 集成测试改用 --test-threads=1 避免跨文件并行竞争

并行运行时多个测试同时 bootstrap 创建 embedding provider，互相干扰
触发 FastEmbed 模型加载（75s/测试）。串行运行避免此问题，总时间
从 238s 降到 107s。

长期方案: 修改 initialize_system 让 embedding_model 可选（Task 2-4）"
```

---

## Phase 2: 长期方案 — `embedding_model` 可选

### Task 2: 修改 `InitializeSystemRequest` 让 `embedding_model` 可选

**Files:**
- Modify: `common/src/api/organization.rs:9-26` — `InitializeSystemRequest`
- Modify: `common/src/api/organization.rs:50-60` — `InitializeSystemResponse`

- [ ] **Step 1: 修改 `InitializeSystemRequest`**

在 `common/src/api/organization.rs` 中，把 `embedding_model` 从 `ModelProviderInitConfig` 改为 `Option<ModelProviderInitConfig>`：

```rust
/// 系统初始化请求 - 创建第一个组织和超级管理员
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct InitializeSystemRequest {
    /// 组织名称
    pub organization_name: String,
    /// 超级管理员用户名
    pub admin_username: String,
    /// 超级管理员密码（前端已哈希）
    pub admin_password_hash: String,
    /// 组织描述（可选）
    pub description: Option<String>,
    /// 超级管理员显示名称（可选）
    pub admin_display_name: Option<String>,
    /// 超级管理员邮箱（可选）
    pub admin_email: Option<String>,
    /// 对话模型配置（用于 Agent 思考和对话）
    pub chat_model: ModelProviderInitConfig,
    /// 向量模型配置（用于 Embedding 向量化，可选 — 不传时跳过向量索引）
    #[serde(default)]
    pub embedding_model: Option<ModelProviderInitConfig>,
}
```

注意 `#[serde(default)]` —— 让前端不传 `embedding_model` 字段时反序列化为 `None`，保持向后兼容。

- [ ] **Step 2: 修改 `InitializeSystemResponse`**

把 `embedding_provider_id` 改为 `Option<String>`：

```rust
/// 系统初始化响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InitializeSystemResponse {
    /// 组织 ID
    pub organization_id: String,
    /// 超级管理员用户 ID
    pub user_id: String,
    /// 对话模型 Provider ID
    pub chat_provider_id: String,
    /// 向量模型 Provider ID（None 表示未创建向量模型）
    pub embedding_provider_id: Option<String>,
}
```

- [ ] **Step 3: 验证编译**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo build --all-targets 2>&1 | tail -10
```

预期：编译失败，因为 `initialize_system.rs` 还在用 `params.embedding_model.name` 等非 Option 访问。这是预期的，Task 3 会修复。

- [ ] **Step 4: 提交（暂时不提交，等 Task 3 一起）**

暂不提交，等 Task 3 修改 handler 后一起编译验证。

---

### Task 3: 修改 `initialize_system` handler 跳过 None 的 embedding provider

**Files:**
- Modify: `src/handlers/organization/initialize_system.rs:52-67`

- [ ] **Step 1: 修改 handler 跳过 None 的 embedding provider**

读 `src/handlers/organization/initialize_system.rs`，把第 52-67 行的 embedding provider 创建逻辑改为：

```rust
    // 2. finance domain 创建 chat provider（Agent 思考用）
    let chat_provider = crate::models::model_provider::ModelProvider::new(
        params.chat_model.name,
        common::enums::ProviderType::from_i32(params.chat_model.provider_type),
        common::enums::ModelCapability::Agent,
        params.chat_model.model_name,
        params.chat_model.api_key,
        params.chat_model.base_url,
        params.chat_model.description,
        user_id.clone(),
    );
    let chat_provider_id = chat_provider.po.id.clone();
    finance::domain()
        .model_provider_manage()
        .create_model_provider(ctx.clone(), &chat_provider)
        .await?;

    // 3. finance domain 创建 embedding provider（向量索引用）
    //    embedding_model 为 None 时跳过 — 调用方不配置向量索引
    let embedding_provider_id = if let Some(embedding_config) = params.embedding_model {
        let embedding_provider = crate::models::model_provider::ModelProvider::new(
            embedding_config.name,
            common::enums::ProviderType::from_i32(embedding_config.provider_type),
            common::enums::ModelCapability::Embedding,
            embedding_config.model_name,
            embedding_config.api_key,
            embedding_config.base_url,
            embedding_config.description,
            user_id.clone(),
        );
        let provider_id = embedding_provider.po.id.clone();
        finance::domain()
            .model_provider_manage()
            .create_model_provider(ctx, &embedding_provider)
            .await?;
        Some(provider_id)
    } else {
        None
    };

    Ok(InitializeSystemResponse {
        organization_id: org_id,
        user_id,
        chat_provider_id,
        embedding_provider_id,
    })
```

注意：原代码第 67 行 `create_model_provider(ctx, &embedding_provider)` 消耗 `ctx`，现在在 if let 分支内消耗。else 分支不消耗 ctx，但 ctx 也不再使用（函数返回），所以 OK。

- [ ] **Step 2: 验证编译**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo build --all-targets 2>&1 | tail -10
```

预期：编译通过。如果失败，检查 `InitializeSystemResponse` 的 `embedding_provider_id` 字段类型是否已改为 `Option<String>`（Task 2 Step 2）。

- [ ] **Step 3: 跑现有测试验证无回归**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo test --lib 2>&1 | tail -5
PROTOC=/opt/homebrew/bin/protoc cargo test --test auth_sysinit_test -- --test-threads=1 2>&1 | tail -10
```

预期：现有测试 PASS。`auth_sysinit_test::test_initialize_system_creates_org_and_providers` 仍传 `embedding_model: Some(...)`（因为现有测试代码还没改），所以 embedding_provider_id 仍然是 `Some(id)`，测试断言需要更新（下一 Task）。

实际上 `auth_sysinit_test` 当前断言是 `assert!(!bs.embedding_provider_id.is_empty())`，但 `embedding_provider_id` 现在是 `Option<String>`，编译会失败。需要 Task 4 一起修改测试代码。

- [ ] **Step 4: 提交（与 Task 2 一起）**

```bash
git add common/src/api/organization.rs src/handlers/organization/initialize_system.rs
git commit -m "refactor: initialize_system 的 embedding_model 改为可选

- InitializeSystemRequest.embedding_model: ModelProviderInitConfig → Option<ModelProviderInitConfig>
- InitializeSystemResponse.embedding_provider_id: String → Option<String>
- handler 跳过 None 的 embedding provider 创建
- #[serde(default)] 保持前端不传字段时向后兼容

为集成测试优化铺路: 测试环境传 None 跳过 embedding provider 创建，
避免并行测试时 embedding provider 互相干扰触发 FastEmbed 模型加载。"
```

---

### Task 4: 修改测试代码用可选 embedding_model

**Files:**
- Modify: `tests/common/factories/user_factory.rs` — `bootstrap_system` 传 None + 删除 `disable_embedding_provider`
- Modify: `tests/common/factories/user_factory.rs` — `BootstrappedSystem.embedding_provider_id` 改为 `Option<String>`
- Modify: `tests/common/factories/user_factory.rs` — 删除 `bootstrap_login_and_disable_embedding`（不再需要）
- Modify: `tests/integration/*.rs` — 所有 `bootstrap_login_and_disable_embedding` 改为 `bootstrap_and_login`
- Modify: `tests/integration/auth_sysinit_test.rs` — `embedding_provider_id` 断言改为 `Option<String>`

- [ ] **Step 1: 修改 `bootstrap_system` 传 None**

读 `tests/common/factories/user_factory.rs`，修改 `bootstrap_system` 函数：

```rust
pub async fn bootstrap_system(app: &TestApp) -> BootstrappedSystem {
    let username = format!("admin-{}", uuid::Uuid::now_v7());
    let password_hash = format!("hash-{}", uuid::Uuid::now_v7());
    let org_name = format!("TestOrg-{}", uuid::Uuid::now_v7());

    let req = InitializeSystemRequest {
        organization_name: org_name,
        admin_username: username.clone(),
        admin_password_hash: password_hash.clone(),
        description: Some("Integration test org".to_string()),
        admin_display_name: Some("Test Admin".to_string()),
        admin_email: Some("admin@test.local".to_string()),
        chat_model: ModelProviderInitConfig {
            name: "Test Chat Provider".to_string(),
            provider_type: 0, // OpenAI
            model_name: "gpt-4o-mini".to_string(),
            api_key: "test-key".to_string(),
            base_url: None,
            description: Some("test chat model".to_string()),
        },
        // 不传 embedding_model — 测试环境不需要向量索引
        // DB 里永远不会有 embedding provider，get_default_embedding_provider 永远返回 Ok(None)
        // 所有实体创建走降级路径，永远不会触发 FastEmbed 模型加载
        embedding_model: None,
    };

    let (status, body) = app.post("/api/v1/organization/initialize", &req).await;
    let data = crate::common::assert_api_ok(status, &body);
    let org_id = data
        .get("organization_id")
        .and_then(|v| v.as_str())
        .expect("missing organization_id in response")
        .to_string();
    let user_id = data
        .get("user_id")
        .and_then(|v| v.as_str())
        .expect("missing user_id in response")
        .to_string();
    let chat_provider_id = data
        .get("chat_provider_id")
        .and_then(|v| v.as_str())
        .expect("missing chat_provider_id in response")
        .to_string();
    // embedding_provider_id 现在是 Option<String>，可能为 null
    let embedding_provider_id = data
        .get("embedding_provider_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    BootstrappedSystem {
        organization_id: org_id,
        user_id,
        username,
        password_hash,
        chat_provider_id,
        embedding_provider_id,
    }
}
```

- [ ] **Step 2: 修改 `BootstrappedSystem` 结构体**

```rust
#[derive(Debug, Clone)]
pub struct BootstrappedSystem {
    pub organization_id: String,
    pub user_id: String,
    pub username: String,
    pub password_hash: String,
    pub chat_provider_id: String,
    /// None 表示测试环境未配置 embedding provider（默认情况）
    pub embedding_provider_id: Option<String>,
}
```

- [ ] **Step 3: 删除 `disable_embedding_provider` 和 `bootstrap_login_and_disable_embedding`**

这两个函数不再需要，因为 `bootstrap_system` 传 None 时根本不创建 embedding provider。删除：

```rust
// 删除整个 disable_embedding_provider 函数
// 删除整个 bootstrap_login_and_disable_embedding 函数
```

- [ ] **Step 4: 更新 `factories/mod.rs` 的 re-exports**

删除 `disable_embedding_provider` 和 `bootstrap_login_and_disable_embedding` 的 re-export：

```rust
pub use user_factory::{
    bootstrap_and_login, bootstrap_system, login_and_get_jwt, BootstrappedSystem,
};
```

- [ ] **Step 5: 修改所有集成测试文件**

对 `tests/integration/*.rs` 中的所有 `bootstrap_login_and_disable_embedding` 调用，改为 `bootstrap_and_login`：

```bash
# 用 Grep 找到所有调用位置
```

需要修改的文件：
- `tests/integration/core_crud_test.rs`
- `tests/integration/message_delivery_test.rs`
- `tests/integration/vector_degradation_test.rs`
- `tests/integration/a2a_flow_test.rs`

每个文件中：
```rust
// 旧代码
let (_bs, jwt) = crate::common::factories::bootstrap_login_and_disable_embedding(&app).await;
// 新代码
let (_bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;
```

- [ ] **Step 6: 修改 `auth_sysinit_test.rs` 的断言**

`test_initialize_system_creates_org_and_providers` 当前断言 `embedding_provider_id` 非空，现在改为断言为 None：

```rust
assert!(bs.embedding_provider_id.is_none(), "embedding_provider_id should be None when not configured");
```

- [ ] **Step 7: 修改 `vector_degradation_test.rs` 的 `test_agent_create_succeeds_without_embedding_provider`**

这个测试当前验证 "删除 embedding provider 后创建 agent 仍成功"。现在 embedding provider 从一开始就不存在，测试逻辑简化：

```rust
/// When no embedding provider is configured, agent creation should still succeed
/// and the agent record should be retrievable.
///
/// Validates the `Ok(None)` degradation path in agent DAL.
#[sqlx::test]
async fn test_agent_create_succeeds_without_embedding_provider(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // bootstrap_system 传 embedding_model: None，DB 里没有任何 embedding provider
    assert!(bs.embedding_provider_id.is_none());

    // Create an agent — should succeed via Ok(None) degradation path
    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("NoVecAgent-{}", uuid::Uuid::now_v7()),
    )
    .await;

    // Re-fetch — main table record must be persisted
    let (status, body) = app
        .get_with_jwt(&format!("/api/v1/hr/agents/{}", agent_id), &jwt)
        .await;
    let agent_data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        agent_data.get("id").and_then(|v| v.as_str()),
        Some(agent_id.as_str()),
        "agent should be retrievable after create-without-embedding"
    );
}
```

- [ ] **Step 8: 验证编译**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo build --all-targets 2>&1 | tail -10
```

预期：编译通过。如果有编译错误，根据错误信息修改对应文件。

- [ ] **Step 9: 跑全部测试验证无回归（串行）**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo test --lib 2>&1 | tail -5
PROTOC=/opt/homebrew/bin/protoc cargo test --test '*' -- --test-threads=1 2>&1 | tail -20
```

预期：全部 PASS。耗时约 107s（与 Task 1 相同，因为还是串行）。

- [ ] **Step 10: 跑并行测试验证性能提升**

```bash
time PROTOC=/opt/homebrew/bin/protoc cargo test --tests -- --test-threads=4 2>&1 | tail -10
```

预期：全部 PASS，耗时 **< 30s**（从 238s 降到 < 30s）。因为 DB 里永远没有 embedding provider，永远不会触发 FastEmbed 模型加载。

- [ ] **Step 11: 提交**

```bash
git add tests/
git commit -m "test: bootstrap_system 传 None 跳过 embedding provider 创建

- bootstrap_system: embedding_model: None
- 删除 disable_embedding_provider 和 bootstrap_login_and_disable_embedding
- 所有集成测试改用 bootstrap_and_login
- BootstrappedSystem.embedding_provider_id 改为 Option<String>
- auth_sysinit 断言 embedding_provider_id 为 None
- vector_degradation 测试简化（不再需要验证 embedding provider 删除）

性能提升: 集成测试并行运行从 238s 降到 < 30s
（DB 里永远没有 embedding provider，不触发 FastEmbed 模型加载）"
```

---

### Task 5: CI 恢复并行集成测试

**Files:**
- Modify: `.github/workflows/rust.yml`

- [ ] **Step 1: 修改 CI workflow 集成测试恢复并行**

```yaml
    - name: Run unit tests (parallel)
      run: cargo test --lib --verbose

    - name: Run integration tests (parallel)
      run: cargo test --test '*' --verbose
```

删除 `-- --test-threads=1`，因为 embedding provider 不再被创建，并行运行不会互相干扰。

- [ ] **Step 2: 验证本地并行测试通过**

```bash
time PROTOC=/opt/homebrew/bin/protoc cargo test --tests -- --test-threads=4 2>&1 | tail -10
```

预期：全部 PASS，耗时 < 30s。

- [ ] **Step 3: 提交**

```bash
git add .github/workflows/rust.yml
git commit -m "ci: 集成测试恢复并行运行

Task 4 已让 bootstrap_system 不创建 embedding provider，并行运行
不再互相干扰触发 FastEmbed 模型加载。

性能: 集成测试从串行 107s 恢复到并行 < 30s"
```

---

## Self-Review Checklist

**Spec coverage:**
- ✅ 短期方案（Task 1）：CI 集成测试用 `--test-threads=1` 立即缓解
- ✅ 长期方案 Task 2：`InitializeSystemRequest.embedding_model` 改为 `Option`
- ✅ 长期方案 Task 3：handler 跳过 None 的 embedding provider
- ✅ 长期方案 Task 4：测试代码用可选 embedding_model
- ✅ 长期方案 Task 5：CI 恢复并行

**Placeholder scan:** 所有 Step 都有具体代码和命令。无占位符。

**Type consistency:**
- `InitializeSystemRequest.embedding_model`: `Option<ModelProviderInitConfig>`（Task 2 定义，Task 3 使用）
- `InitializeSystemResponse.embedding_provider_id`: `Option<String>`（Task 2 定义，Task 3 + Task 4 使用）
- `BootstrappedSystem.embedding_provider_id`: `Option<String>`（Task 4 定义，与 InitializeSystemResponse 一致）

**风险点：**
1. **前端兼容性**：`#[serde(default)]` 让前端不传 `embedding_model` 字段时反序列化为 `None`，保持向后兼容。前端如果之前必传该字段，现在可以不传。
2. **`initialize_system` 的 ctx 消耗**：原代码 `create_model_provider(ctx, ...)` 消耗 ctx，改为 if let 后在分支内消耗。else 分支不消耗 ctx 但 ctx 也不再使用（函数返回），所以 OK。
3. **`vector_degradation_test` 测试语义变化**：原测试验证 "删除 embedding provider 后" 的降级，现在改为 "从未配置 embedding provider" 的降级。语义略有不同但都验证同一个降级路径（`Ok(None)`），仍然有效。
4. **`auth_sysinit_test` 断言变化**：从 `assert!(!bs.embedding_provider_id.is_empty())` 改为 `assert!(bs.embedding_provider_id.is_none())`，与新的 `bootstrap_system` 行为一致。
5. **CI 恢复并行的时机**：Task 5 必须在 Task 4 完成后才能做，否则并行测试仍会触发 FastEmbed。

**性能预期：**
- Task 1 完成后：集成测试从 238s 降到 107s（串行避免 FastEmbed）
- Task 4 完成后：集成测试并行运行从 238s 降到 < 30s（不创建 embedding provider）
- Task 5 完成后：CI 恢复并行，从 107s 降到 < 30s
