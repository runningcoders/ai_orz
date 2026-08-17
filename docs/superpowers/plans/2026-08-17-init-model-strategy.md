# 系统初始化模型配置策略调整 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 系统初始化时对话模型改为可选（Agent 创建时引导补配），向量模型保持可选但强化「跳过后果」提示，并补齐「初始化后补配/更换向量模型」的完整闭环（前端入口 + 唯一性守卫 + 自动重建）。

**Architecture:** 三段式改动：① 后端协议与初始化流程（`chat_model` 改 `Option`，条件化创建 provider，动态步骤数，handler 入口边界校验）；② 后端补配闭环（domain 层 embedding 唯一性守卫，handler 层创建/更新 embedding 后自动注册向量重建任务）；③ 前端三处（初始化向导重构为 2 步表单：基础信息→模型配置，逐步校验+最终统一提交，模型均可选带跳过提示；Agent 创建按 capability 过滤+空态引导；模型管理创建 Modal 支持 capability 选择）。

**Tech Stack:** Rust (Axum + sqlx)、Dioxus 0.7 (WASM)、common crate DTO 单一事实源。

---

## 背景与决策（为什么）

- **对话模型改可选**：初始化后浏览/组织/任务管理不依赖 LLM，只有 Agent 思考依赖；创建 Agent 时绑定 provider 是更自然的时机。配套：Agent 创建流程做 capability 过滤 + 空态引导。
- **向量模型保持可选但强推荐**：向量索引是全局共享知识基础设施，必须同一向量空间（组织维度，禁止普通用户随意切换——现状正确）。跳过的代价：后补时存量实体需全量重建（`RebuildVectorsTask`，切换场景已有）。故向导中不阻塞但明确提示后果。
- **向量模型「创建不阻塞 + 启用时切换」**（用户决策修正）：已有使用中的向量模型时，**允许创建**新的 Embedding provider，但默认落库为**未启用**状态（`Disabled`）；用户后续在列表点「启用」时复用现有切换链路（update 守卫 409 `embedding_provider_switch_required` → 前端 switch modal 提示后果 → 确认 → `switch_embedding` 软删旧+启用新+全量重建）。允许预先配置备用模型，切换时机与后果确认由用户掌握。
- **补配闭环（本计划发现的缺口，必须修）**：
  - 前端「添加提供商」Modal 硬编码 `ModelCapability::Agent`，初始化后**无任何 UI 入口**新建 Embedding provider；
  - 后补向量模型后存量实体无向量，现有重建只在 switch（切换）路径触发，首次补配不触发。

## 关键现状锚点（实现者必读）

| 事实 | 位置 |
|------|------|
| `chat_model` 当前必填（非 Option） | `common/src/api/organization.rs:23` |
| 初始化步骤链 org→chat→embedding(可选)→tools→skills | `src/handlers/organization/initialize_system.rs:138-224` |
| `ModelProviderStatus` 仅 `Deleted=0 / Normal=1`，**无 Disabled**（本计划新增 `Disabled=2`）；i32 存储，无 migration 需要 | `common/src/enums/agent.rs:118-124` |
| DAO 过滤：`find_by_id` 用 `status != 0`、`find_all` 用 `exclude Deleted` → **`Disabled=2` 天然在列表/详情可见** | `src/service/dao/model_provider/sqlite.rs:66-82,126-133` |
| `ModelProviderPo::new` 硬编码 `status: Normal`（L170）；domain create 需按场景改写 | `src/models/model_provider.rs:146-175` |
| 前端列表 `is_enabled = p.status == 1`，非 1 显示「禁用」badge +「启用」按钮 → **Disabled=2 前端天然兼容，启用链路零新增** | `frontend/src/pages/finance/model_providers.rs:201,220-273` |
| update 已有 embedding 唯一性守卫（409 `embedding_provider_switch_required`） | `src/service/domain/finance/model_provider.rs:53-81` |
| switch 流程：软删旧 + 启用新 + handler 注册 `RebuildVectorsTask`；首次启用（current=None）也注册重建 | `src/service/domain/finance/model_provider.rs:103-147`、`src/handlers/finance/model_provider/switch_embedding.rs:37-44` |
| 前端初始化向导单页表单；embedding 已有 checkbox 样板 | `frontend/src/pages/reception.rs:539-551` |
| Agent 创建 provider 下拉不过滤 capability；空态退化为手输 input | `frontend/src/pages/hr/agents.rs:457-474` |
| 模型管理创建 Modal 硬编码 `capability: ModelCapability::Agent` | `frontend/src/pages/finance/model_providers.rs:74` |
| 集成测试工厂 `bootstrap_system`（chat 必填、embedding None） | `tests/common/factories/user_factory.rs:75-139` |
| `ModelCapability` 有 `is_agent()/is_embedding()/from_i32()` | `common/src/enums/provider.rs:112-132` |

**分层纪律**：唯一性校验属业务规则 → domain 层；重建任务注册属编排 → handler 层（与 switch_embedding 同模式）；DTO 只动 common。

---

### Task 1: common DTO — `chat_model` 改 Option，响应字段同步

**Files:**
- Modify: `common/src/api/organization.rs:22-26`（请求字段）
- Modify: `common/src/api/organization.rs:50-61`（响应字段）

- [ ] **Step 1: 修改请求字段**

`common/src/api/organization.rs` 中将：

```rust
    /// 对话模型配置（用于 Agent 思考和对话）
    pub chat_model: ModelProviderInitConfig,
```

改为：

```rust
    /// 对话模型配置（用于 Agent 思考和对话，可选 — 不传时跳过，创建 Agent 前需在模型管理中补配）
    #[serde(default)]
    pub chat_model: Option<ModelProviderInitConfig>,
```

注意：`embedding_model` 字段保持不变（原本就是 Option + `#[serde(default)]`）。

- [ ] **Step 2: 修改响应字段**

同文件 `InitializeSystemResponse` 中将：

```rust
    /// 对话模型 Provider ID
    pub chat_provider_id: String,
```

改为：

```rust
    /// 对话模型 Provider ID（None 表示初始化时未配置对话模型）
    pub chat_provider_id: Option<String>,
```

- [ ] **Step 3: 编译验证 common**

Run: `cargo check -p common 2>&1 | tail -20`
Expected: common 本身通过；此时后端主 crate（ai_orz）与 frontend 必然报错（Task 2/6 修复），这是预期中间态。

- [ ] **Step 4: Commit**

```bash
git add common/src/api/organization.rs
git commit -m "feat(common): InitializeSystemRequest.chat_model 改为可选，响应 chat_provider_id 可空"
```

---

### Task 2: 初始化 handler — 条件创建 chat provider + 动态步骤数

**Files:**
- Modify: `src/handlers/organization/initialize_system.rs:43-64`（total_steps 计算）
- Modify: `src/handlers/organization/initialize_system.rs:138-224`（run_steps 重写）

- [ ] **Step 1: 修改 total_steps 计算**

`InitializeSystemTask::new` 中将：

```rust
        let total_steps = if params.embedding_model.is_some() {
            5
        } else {
            4
        };
```

改为：

```rust
        // 基础 3 步（组织 + 内置工具 + 预置技能）+ 对话模型(0/1) + 向量模型(0/1)
        let total_steps =
            3 + usize::from(params.chat_model.is_some()) + usize::from(params.embedding_model.is_some());
```

- [ ] **Step 2: 重写 run_steps 的 provider 创建段**

将 `run_steps` 中 Step 2（chat，L149-L165）与 Step 3（embedding，L167-L190）两段整体替换为下面的动态步进版本（Step 1 org 与 Step 4/5 tools/skills 保持不动，仅步骤号改用变量）：

```rust
        // Step 1: 创建组织 + Owner
        self.set_step(1, "正在创建组织和超级管理员");
        let (org_id, user_id) = organization::domain()
            .organization_manage()
            .create_org_and_owner(ctx.clone(), params.clone())
            .await?;

        let mut step = 2;

        // Step（可选）: 创建 chat provider — 未配置时跳过，后续在模型管理中补配
        let chat_provider_id = if let Some(chat_config) = params.chat_model.clone() {
            self.set_step(step, "正在配置对话模型");
            let chat_provider = crate::models::model_provider::ModelProvider::new(
                chat_config.name,
                common::enums::ProviderType::from_i32(chat_config.provider_type),
                common::enums::ModelCapability::Agent,
                chat_config.model_name,
                chat_config.api_key,
                chat_config.base_url,
                chat_config.description,
                user_id.clone(),
            );
            let provider_id = chat_provider.po.id.clone();
            finance::domain()
                .model_provider_manage()
                .create_model_provider(ctx.clone(), &chat_provider)
                .await?;
            step += 1;
            Some(provider_id)
        } else {
            None
        };

        // Step（可选）: 创建 embedding provider — 未配置时跳过向量索引
        let embedding_provider_id = if let Some(embedding_config) = params.embedding_model {
            self.set_step(step, "正在配置向量模型");
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
                .create_model_provider(ctx.clone(), &embedding_provider)
                .await?;
            step += 1;
            Some(provider_id)
        } else {
            None
        };

        // Step: 同步内置工具到 DB
        self.set_step(step, "正在同步内置工具");
        let tool_count = finance::domain()
            .tool_provider_manage()
            .sync_builtin_tools(ctx.clone())
            .await?;
        sys_info!("initialize_system: 同步 {} 个内置工具到 DB", tool_count);

        // Step: 导入预置技能
        self.set_step(step + 1, "正在导入预置技能");
```

（后续 skill 导入与 `Ok(InitializeSystemResponse {...})` 段保持原样；`chat_provider_id` 现在是 `Option<String>`，与 Task 1 的响应类型一致。）

- [ ] **Step 3: handler 入口加模型配置边界校验**

在 `src/handlers/organization/initialize_system.rs` 的 handler 入口函数（创建/注册 `InitializeSystemTask` 之前）加校验——直调 API 绕过前端分步校验时，立即返回 400 而非经异步进度报 Failed：

```rust
    // 边界校验：配置了模型则字段必须完整（前端分步校验只覆盖正常路径）
    if let Some(chat) = params.chat_model.as_ref() {
        if chat.name.trim().is_empty()
            || chat.model_name.trim().is_empty()
            || chat.api_key.trim().is_empty()
        {
            return Err(common::error::Error::bad_request(
                "chat_model provided but name / model_name / api_key is empty",
            ));
        }
    }
    if let Some(emb) = params.embedding_model.as_ref() {
        if emb.name.trim().is_empty() || emb.model_name.trim().is_empty() {
            return Err(common::error::Error::bad_request(
                "embedding_model provided but name / model_name is empty",
            ));
        }
    }
```

（`Error` 若已在 imports 中则用短名；handler 函数与任务注册的确切位置以该文件实际代码为准，保持「校验先于任务注册」的顺序即可。）

- [ ] **Step 4: 编译验证**

Run: `cargo check -p ai-orz 2>&1 | tail -20`
Expected: 通过（crate 名以实际为准，若报错 `unrecognized` 则用 `cargo check --workspace --exclude frontend`）。

- [ ] **Step 5: Commit**

```bash
git add src/handlers/organization/initialize_system.rs
git commit -m "feat(org): 初始化支持跳过对话模型，入口补模型字段边界校验"
```

---

### Task 3: 集成测试 — 工厂暴露最小初始化 + 无对话模型用例

**Files:**
- Modify: `tests/common/factories/user_factory.rs`（`poll_initialize_progress` 改 pub + 新增最小变体）
- Modify: `tests/integration/auth_sysinit_test.rs`（追加用例）

- [ ] **Step 1: 工厂函数复用改造**

`tests/common/factories/user_factory.rs` 中 `async fn poll_initialize_progress`（L34）改为 pub：

```rust
/// 轮询初始化进度直到完成或失败（供各初始化变体复用）
pub async fn poll_initialize_progress(app: &TestApp, task_id: &str) -> serde_json::Value {
```

- [ ] **Step 2: 新增最小初始化工厂变体**

在 `bootstrap_system` 函数之后追加：

```rust
/// 最小初始化变体：不配置任何模型 provider（chat/embedding 均为 None）。
/// 用于验证「跳过对话模型」的初始化路径。
pub async fn bootstrap_system_minimal(app: &TestApp) -> serde_json::Value {
    let _guard = BOOTSTRAP_MUTEX.lock().await;

    let req = InitializeSystemRequest {
        organization_name: format!("MinOrg-{}", uuid::Uuid::now_v7()),
        admin_username: format!("min-admin-{}", uuid::Uuid::now_v7()),
        admin_password_hash: format!("hash-{}", uuid::Uuid::now_v7()),
        description: None,
        admin_display_name: None,
        admin_email: None,
        chat_model: None,
        embedding_model: None,
    };

    let (status, body) = app.post("/api/v1/organization/initialize", &req).await;
    let data = crate::common::assert_api_ok(status, &body);
    let task_id = data
        .get("task_id")
        .and_then(|v| v.as_str())
        .expect("missing task_id in initialize response")
        .to_string();
    poll_initialize_progress(app, &task_id).await
}
```

（注意：原 `bootstrap_system` 完全不动 —— 它继续传 `Some(chat)`，`chat_provider_id` 仍非空，15+ 处既有引用不受影响。）

- [ ] **Step 3: 追加集成用例**

`tests/integration/auth_sysinit_test.rs` 末尾追加：

```rust
/// 最小初始化：跳过对话模型与向量模型，系统仍应完成初始化，
/// 且响应中 chat_provider_id / embedding_provider_id 均为 null。
#[sqlx::test]
async fn test_initialize_system_without_any_model(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let result = crate::common::factories::bootstrap_system_minimal(&app).await;

    assert!(
        result.get("organization_id").and_then(|v| v.as_str()).is_some(),
        "organization_id should exist"
    );
    assert!(
        result.get("user_id").and_then(|v| v.as_str()).is_some(),
        "user_id should exist"
    );
    assert!(
        result.get("chat_provider_id").map(|v| v.is_null()).unwrap_or(false),
        "chat_provider_id should be null when chat model skipped"
    );
    assert!(
        result
            .get("embedding_provider_id")
            .map(|v| v.is_null())
            .unwrap_or(false),
        "embedding_provider_id should be null when embedding skipped"
    );

    let (status, body) = app.get("/api/v1/organization/initialize/check").await;
    let data = crate::common::assert_api_ok(status, &body);
    let initialized = data
        .get("initialized")
        .and_then(|v| v.as_bool())
        .expect("expected initialized field");
    assert!(initialized, "system should be initialized after minimal bootstrap");
}
```

- [ ] **Step 4: 运行测试验证**

Run: `cargo test --test auth_sysinit_test 2>&1 | tail -15`
Expected: 全部 PASS（含既有 4 个用例 + 新增 1 个）。

- [ ] **Step 5: Commit**

```bash
git add tests/common/factories/user_factory.rs tests/integration/auth_sysinit_test.rs
git commit -m "test(org): 新增跳过模型的最小初始化集成用例"
```

---

### Task 4: 状态枚举 + domain 层 — Embedding 创建降级为未启用

**Files:**
- Modify: `common/src/enums/agent.rs:118-124`（`ModelProviderStatus` 加 `Disabled`）
- Modify: `src/service/domain/finance/model_provider.rs:14-21`（create_model_provider）
- Test: `tests/integration/model_provider_embedding_test.rs`（新建）

- [ ] **Step 1: 写失败的集成测试**

新建 `tests/integration/model_provider_embedding_test.rs`：

```rust
//! Embedding provider 生命周期集成测试。
//!
//! 策略（创建不阻塞 + 启用时切换）：
//! - 首个 Embedding 创建 → 直接启用（Normal）
//! - 已有启用的 Embedding 时再创建 → 成功但落库为未启用（Disabled=2）
//! - 启用 Disabled 的 Embedding → 409 switch_required → 走 switch 完成切换
//! - 补配/更换触发的向量重建（Task 5 实现后补充断言）

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use common::api::CreateModelProviderRequest;
use sqlx::SqlitePool;

fn embedding_req(name: &str) -> CreateModelProviderRequest {
    CreateModelProviderRequest {
        name: name.to_string(),
        provider_type: 6, // FastEmbed
        capability: common::enums::ModelCapability::Embedding,
        model_name: "BAAI/bge-small-en-v1.5".to_string(),
        api_key: String::new(),
        base_url: None,
        description: None,
        max_context_length: None,
        recommended_context_length: None,
    }
}

/// 已有启用的 Embedding 时，创建第二个成功但为未启用状态（Disabled）
#[sqlx::test]
async fn test_create_second_embedding_lands_disabled(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let bs = crate::common::factories::bootstrap_system(&app).await;
    let jwt = crate::common::factories::login(&app, &bs.username, &bs.password_hash).await;

    // 首个 → 启用
    let (status, _) = app
        .post_with_jwt(&jwt, "/api/v1/finance/model-providers", &embedding_req("Embedding-A"))
        .await;
    assert_eq!(status, 200, "first embedding provider should be created enabled");

    // 第二个 → 成功但 Disabled
    let (status, body) = app
        .post_with_jwt(&jwt, "/api/v1/finance/model-providers", &embedding_req("Embedding-B"))
        .await;
    assert_eq!(status, 200, "second embedding provider should be created (not rejected)");
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(
        data.get("status").and_then(|v| v.as_i64()),
        Some(2),
        "second embedding provider should land as Disabled(2), got: {:?}",
        data.get("status")
    );

    // 列表中 A 仍唯一启用
    let (status, body) = app.get_with_jwt(&jwt, "/api/v1/finance/model-providers").await;
    let data = crate::common::assert_api_ok(status, &body);
    let enabled_count = data["providers"]
        .as_array()
        .expect("providers array")
        .iter()
        .filter(|p| p["capability"].as_i64() == Some(1) && p["status"].as_i64() == Some(1))
        .count();
    assert_eq!(enabled_count, 1, "exactly one enabled embedding provider");
}

/// 启用 Disabled 的 Embedding → 409 switch_required（切换确认由既有 switch 链路完成）
#[sqlx::test]
async fn test_enable_disabled_embedding_requires_switch_confirm(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let bs = crate::common::factories::bootstrap_system(&app).await;
    let jwt = crate::common::factories::login(&app, &bs.username, &bs.password_hash).await;

    let _ = app
        .post_with_jwt(&jwt, "/api/v1/finance/model-providers", &embedding_req("Embedding-A"))
        .await;
    let (status, body) = app
        .post_with_jwt(&jwt, "/api/v1/finance/model-providers", &embedding_req("Embedding-B"))
        .await;
    let b_id = crate::common::assert_api_ok(status, &body)
        .get("id")
        .and_then(|v| v.as_str())
        .expect("created id")
        .to_string();

    // 直接启用 B → 409 + switch_required（前端据此弹确认 modal）
    let update = serde_json::json!({ "id": b_id, "status": 1 });
    let (status, body) = app
        .put_with_jwt(&jwt, &format!("/api/v1/finance/model-providers/{}", b_id), &update)
        .await;
    assert_eq!(status, 409);
    assert!(
        body.contains("embedding_provider_switch_required"),
        "expected switch_required error, got: {}",
        body
    );
}
```

注意：`post_with_jwt` / `get_with_jwt` / `put_with_jwt` / `login` 辅助函数名以 `tests/common/` 实际导出为准（先读 `tests/common/mod.rs` 与既有集成测试开头，套用同款调用方式；若名称不同按既有测试改写，断言语义不变）。另：`CreateModelProviderResponse` 加 `status` 字段在 Task 5 Step 1——本任务先在响应 DTO 中一并加上（否则第一个测试断言 `data.get("status")` 编译不过），两任务联动，执行时可合并提交。

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --test model_provider_embedding_test 2>&1 | tail -15`
Expected: FAIL —— 第二个创建返回的 status 是 1（Normal，降级逻辑尚不存在）。

- [ ] **Step 3: 枚举加 Disabled**

`common/src/enums/agent.rs` 的 `ModelProviderStatus` 改为：

```rust
pub enum ModelProviderStatus {
    /// Deleted (soft deleted)
    Deleted = 0,
    /// Normal (available / enabled)
    #[default]
    Normal = 1,
    /// Disabled (created but not enabled; embedding providers pending switch)
    Disabled = 2,
}
```

同文件 `impl ModelProviderStatus` 的 `from_i32` / `match` 分支若为穷举式，补 `2 => ModelProviderStatus::Disabled`。全仓 `cargo check` 检查是否有非穷举 match 报错并补齐（多数逻辑以 `== Normal` / `!= Deleted` 判断，天然兼容）。

- [ ] **Step 4: domain create 降级逻辑**

`src/service/domain/finance/model_provider.rs` 的 `create_model_provider` 改为：

```rust
    async fn create_model_provider(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
    ) -> Result<()> {
        // Embedding「创建不阻塞 + 启用时切换」：已有启用的 Embedding Provider 时，
        // 新的允许创建但降级为 Disabled（未启用）；启用时走 update 守卫 409
        // → 前端 switch modal 确认 → switch_embedding（软删旧+启用新+全量重建）。
        // 首个 Embedding 直接 Normal 启用（当前无任何启用者）。
        let mut provider = provider.clone();
        if provider.po.capability.is_embedding()
            && provider.po.status == ModelProviderStatus::Normal
            && self.model_provider_dal
                .find_enabled_embedding_provider(ctx.clone())
                .await?
                .is_some()
        {
            provider.po.status = ModelProviderStatus::Disabled;
        }

        let ctx = enrich_ctx!(&ctx, &provider);
        self.model_provider_dal.create(ctx, &provider).await
    }
```

（`ModelProvider` 需 `Clone`——已有 derive，见 `src/models/model_provider.rs:70` 附近；如无则加。）

- [ ] **Step 5: 运行确认通过 + 回归**

Run: `cargo test --test model_provider_embedding_test --test auth_sysinit_test 2>&1 | tail -10`
Expected: 全 PASS（初始化 embedding 是首个创建，不受降级影响）。

- [ ] **Step 6: Commit**

```bash
git add common/src/enums/agent.rs common/src/api/model_provider.rs src/service/domain/finance/model_provider.rs tests/integration/model_provider_embedding_test.rs
git commit -m "feat(finance): Embedding 创建不阻塞策略 — 已有启用者时降级为未启用，启用走切换确认"
```

---

### Task 5: handler 层 — 补配/更换 embedding 自动注册向量重建

**Files:**
- Modify: `common/src/api/model_provider.rs`（Create Response 加 `status`（Task 4 联动）；Create/Update Response 加 `rebuild_task_id`）
- Modify: `src/handlers/finance/model_provider/create_model_provider.rs`
- Modify: `src/handlers/finance/model_provider/update_model_provider.rs`
- Test: `tests/integration/model_provider_embedding_test.rs`（追加用例）

**重建触发矩阵（本任务实现的核心语义）：**

| 场景 | 是否注册重建 | 原因 |
|------|-------------|------|
| create 首个 embedding（落库 Normal） | ✅ | 后补场景：存量实体无向量，需全量补建 |
| create 第二个 embedding（降级 Disabled） | ❌ | 未生效；重建推迟到启用切换时（switch handler 已有） |
| update 编辑 **Normal** embedding 的 model_name/api_key/base_url | ✅ | 向量空间变化，使用中的索引需重建 |
| update 编辑 **Disabled** embedding | ❌ | 未生效，启用时 switch 全量重建兜底 |

- [ ] **Step 1: DTO 加字段**

`common/src/api/model_provider.rs`：

`CreateModelProviderResponse` 追加（末尾）：

```rust
    /// 创建后的状态（0=已删除 1=启用 2=未启用 — embedding 已有启用者时创建为 2）
    pub status: i32,
    /// 本次操作触发的向量重建任务 ID（仅 Embedding 且即时生效时返回，None 表示未触发）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rebuild_task_id: Option<String>,
```

`UpdateModelProviderResponse` 追加（末尾）：

```rust
    /// 本次操作触发的向量重建任务 ID（仅 Embedding 且配置变化时返回，None 表示未触发）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rebuild_task_id: Option<String>,
```

- [ ] **Step 2: 写失败的测试**

`tests/integration/model_provider_embedding_test.rs` 追加：

```rust
/// 后补场景：初始化未配向量模型，事后创建首个 Embedding（落库 Normal）→ 携带 rebuild_task_id
#[sqlx::test]
async fn test_create_first_embedding_provider_triggers_rebuild(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let bs = crate::common::factories::bootstrap_system(&app).await;
    let jwt = crate::common::factories::login(&app, &bs.username, &bs.password_hash).await;

    let (status, body) = app
        .post_with_jwt(&jwt, "/api/v1/finance/model-providers", &embedding_req("Embedding-Late"))
        .await;
    assert_eq!(status, 200);
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(data.get("status").and_then(|v| v.as_i64()), Some(1), "first embedding lands Normal");
    assert!(
        data.get("rebuild_task_id").and_then(|v| v.as_str()).is_some(),
        "first embedding creation should register rebuild task"
    );
}

/// 已有启用者时创建第二个（Disabled）→ 不携带 rebuild_task_id（重建推迟到切换时）
#[sqlx::test]
async fn test_create_disabled_embedding_no_rebuild(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let bs = crate::common::factories::bootstrap_system(&app).await;
    let jwt = crate::common::factories::login(&app, &bs.username, &bs.password_hash).await;

    let _ = app
        .post_with_jwt(&jwt, "/api/v1/finance/model-providers", &embedding_req("Embedding-A"))
        .await;
    let (status, body) = app
        .post_with_jwt(&jwt, "/api/v1/finance/model-providers", &embedding_req("Embedding-B"))
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    assert_eq!(data.get("status").and_then(|v| v.as_i64()), Some(2), "second embedding lands Disabled");
    assert!(
        data.get("rebuild_task_id").is_none(),
        "disabled embedding creation must NOT register rebuild (deferred to switch)"
    );
}

/// 编辑使用中（Normal）embedding 的 model_name → 触发重建
#[sqlx::test]
async fn test_update_enabled_embedding_model_triggers_rebuild(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let bs = crate::common::factories::bootstrap_system(&app).await;
    let jwt = crate::common::factories::login(&app, &bs.username, &bs.password_hash).await;

    let (status, body) = app
        .post_with_jwt(&jwt, "/api/v1/finance/model-providers", &embedding_req("Embedding-Edit"))
        .await;
    let id = crate::common::assert_api_ok(status, &body)
        .get("id").and_then(|v| v.as_str()).expect("id").to_string();

    let update = serde_json::json!({ "id": id, "model_name": "BAAI/bge-m3" });
    let (status, body) = app
        .put_with_jwt(&jwt, &format!("/api/v1/finance/model-providers/{}", id), &update)
        .await;
    assert_eq!(status, 200);
    assert!(
        crate::common::assert_api_ok(status, &body)
            .get("rebuild_task_id").and_then(|v| v.as_str()).is_some(),
        "model_name change on enabled embedding should trigger rebuild"
    );
}

/// 编辑未启用（Disabled）embedding 的 model_name → 不触发重建
#[sqlx::test]
async fn test_update_disabled_embedding_no_rebuild(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let bs = crate::common::factories::bootstrap_system(&app).await;
    let jwt = crate::common::factories::login(&app, &bs.username, &bs.password_hash).await;

    let _ = app
        .post_with_jwt(&jwt, "/api/v1/finance/model-providers", &embedding_req("Embedding-A"))
        .await;
    let (status, body) = app
        .post_with_jwt(&jwt, "/api/v1/finance/model-providers", &embedding_req("Embedding-B"))
        .await;
    let b_id = crate::common::assert_api_ok(status, &body)
        .get("id").and_then(|v| v.as_str()).expect("id").to_string();

    let update = serde_json::json!({ "id": b_id, "model_name": "BAAI/bge-m3" });
    let (status, body) = app
        .put_with_jwt(&jwt, &format!("/api/v1/finance/model-providers/{}", b_id), &update)
        .await;
    assert_eq!(status, 200);
    assert!(
        crate::common::assert_api_ok(status, &body).get("rebuild_task_id").is_none(),
        "editing disabled embedding must NOT trigger rebuild"
    );
}
```

- [ ] **Step 3: 运行确认失败**

Run: `cargo test --test model_provider_embedding_test 2>&1 | tail -15`
Expected: 新增四用例 FAIL（`rebuild_task_id`/`status` 缺失或逻辑未实现）。

- [ ] **Step 4: create handler — 条件注册重建 + 响应 status**

`src/handlers/finance/model_provider/create_model_provider.rs`：

1. `create_model_provider(...).await?;` 成功后、构造响应前——注意 domain 可能已将落库状态降级，需**回读**（domain create 无返回值，最简单是 `get_model_provider` 回查一次）：

```rust
    // 回读落库状态（domain 对已有启用者的 embedding 创建降级为 Disabled）
    let created = domain()
        .model_provider_manage()
        .get_model_provider(ctx.clone(), &provider.po.id)
        .await?
        .ok_or_else(|| common::error::Error::internal("created provider not found"))?;

    // 首个补配（即时启用）→ 存量实体无向量索引，注册全量重建；
    // 降级 Disabled 的创建不重建（推迟到启用切换，switch handler 已注册）
    let rebuild_task_id = if created.po.capability.is_embedding()
        && created.po.status == common::enums::ModelProviderStatus::Normal
    {
        let task = Arc::new(
            crate::handlers::finance::model_provider::rebuild_vectors_task::RebuildVectorsTask::new(ctx.clone()),
        );
        Some(crate::pkg::background_task::registry().register(task).await)
    } else {
        None
    };
```

（`Error::internal` 若不存在用实际等价构造器；`registry` 用法参照 `switch_embedding.rs:3-10,40-42`。）

2. 文件头加 `use std::sync::Arc;`；响应构造中 `id: provider.po.id.clone()` 等字段改为用 `created`（或保持 `provider` 均可，id 一致），末尾追加：

```rust
        status: created.po.status as i32,
        rebuild_task_id,
```

- [ ] **Step 5: update handler — 仅使用中配置变化才重建**

`src/handlers/finance/model_provider/update_model_provider.rs`：

1. `get_model_provider` 之后、字段更新之前记录（**关键：status 判断用更新前的值，且须在 `params.status` 应用前**）：

```rust
    // Embedding 配置变化检测：仅「使用中(Normal)」的配置变化需要重建向量索引；
    // Disabled 的编辑不重建（启用切换时 switch 全量重建兜底）
    let was_enabled_embedding = provider.po.capability.is_embedding()
        && provider.po.status == ModelProviderStatus::Normal;
    let embedding_config_changed = was_enabled_embedding
        && (params.model_name.as_deref().is_some_and(|v| v != provider.po.model_name)
            || params.api_key.as_deref().is_some_and(|v| v != provider.po.api_key)
            || params
                .base_url
                .as_deref()
                .is_some_and(|v| provider.po.base_url.as_deref() != Some(v)));
```

2. `update_model_provider(...).await?;` 成功后插入：

```rust
    let rebuild_task_id = if embedding_config_changed {
        let task = Arc::new(
            crate::handlers::finance::model_provider::rebuild_vectors_task::RebuildVectorsTask::new(ctx.clone()),
        );
        Some(crate::pkg::background_task::registry().register(task).await)
    } else {
        None
    };
```

注意：若 `update_model_provider(ctx, ...)` 已 move ctx，在调用前 `let ctx = ctx.clone();` 预留。另外本路径触发的重建不软删旧 provider（同模型原地改配置），与 switch 语义不同，属预期。

3. 文件头加 `use std::sync::Arc;`；响应末尾加 `rebuild_task_id,`。

- [ ] **Step 6: 运行确认通过 + 回归**

Run: `cargo test --test model_provider_embedding_test --test auth_sysinit_test 2>&1 | tail -10`
Expected: 全 PASS。

- [ ] **Step 7: Commit**

```bash
git add common/src/api/model_provider.rs src/handlers/finance/model_provider/create_model_provider.rs src/handlers/finance/model_provider/update_model_provider.rs tests/integration/model_provider_embedding_test.rs
git commit -m "feat(finance): 补配/更换 Embedding 按生效状态条件触发向量重建"
```

---

### Task 6: 前端 — 初始化向导重构为 2 步表单（基础信息 → 模型配置）

**Files:**
- Modify: `frontend/src/pages/reception.rs`（信号区 L36-52、提交处理 L135-205、表单 UI L405-632）

**目标交互结构：**

```
[steps 指示器]  ① 基础信息 ──── 模型配置
Step 1: 组织/管理员 6 字段 + [下一步]     ← 仅校验基础信息，通过进 Step 2
Step 2: 对话模型开关 + 字段/跳过提示
        向量模型开关 + 字段/跳过提示
        [上一步] [完成初始化]            ← 校验模型区后一次性提交
提交 → 异步进度条轮询（现有逻辑保留不动）
```

- [ ] **Step 1: 加表单状态信号**

`let mut init_progress = ...`（L37）之后加：

```rust
    // 两步表单状态
    let mut init_step = use_signal(|| 1u8); // 1=基础信息 2=模型配置
```

`// 对话模型配置` 注释（L39）后、`chat_provider_name` 之前加：

```rust
    // 对话模型配置（可选 — 可跳过，创建 Agent 前补配）
    let mut enable_chat = use_signal(|| true); // 默认启用
```

- [ ] **Step 2: 加向导导航处理**

`// 初始化提交` 注释（L135）之前插入：

```rust
    // 向导导航：Step 1「下一步」仅校验基础信息区
    let on_next_step = move |_| {
        if org_name().is_empty() || init_username().is_empty() || init_password().is_empty() {
            toast.error("组织名称、用户名、密码不能为空");
            return;
        }
        init_step.set(2);
    };

    let on_prev_step = move |_| {
        init_step.set(1);
    };
```

- [ ] **Step 3: 精简提交处理（去基础信息校验，chat 条件化）**

`on_submit_init` 三处改动：

1. 删除 L138-141 基础信息校验（Step 1「下一步」已把关）。
2. 对话模型校验（原 L142-148）改为：

```rust
            // 基础信息已在 Step 1 校验，这里仅校验模型区
            if enable_chat()
                && (chat_provider_name().is_empty()
                    || chat_model_name().is_empty()
                    || chat_api_key().is_empty())
            {
                toast.error("对话模型的 Provider 名称、模型名称、API Key 不能为空");
                return;
            }
```

3. `chat_model:` 字段构造（原 L176-187）改为条件化：

```rust
                chat_model: if enable_chat() {
                    Some(common::api::ModelProviderInitConfig {
                        name: chat_provider_name(),
                        provider_type: chat_provider_type(),
                        model_name: chat_model_name(),
                        api_key: chat_api_key(),
                        base_url: if chat_base_url().is_empty() { None } else { Some(chat_base_url()) },
                        description: None,
                    })
                } else {
                    None
                },
```

（`embedding_model` 构造与后续提交/进度轮询逻辑保持不动。）

- [ ] **Step 4: 表单重构为两步条件渲染**

`<form>`（L405）内部整体调整为「步骤指示器 + 两步条件渲染」。原基础信息 6 字段（组织名称/组织描述/用户名/密码/显示名/邮箱）原样移入 Step 1 容器；原对话模型 5 字段（L472-537）与向量模型区块（L539-623）原样移入 Step 2 容器；原表单尾部独立提交按钮删除，由 Step 2 按钮组取代：

```rust
                                // ===== 步骤指示器 =====
                                ul { class: "steps steps-primary w-full mb-2",
                                    "data-testid": "init-wizard-steps",
                                    li { class: "step step-primary", "基础信息" }
                                    li {
                                        class: if init_step() >= 2 { "step step-primary" } else { "step" },
                                        "模型配置"
                                    }
                                }

                                if init_step() == 1 {
                                    // ===== Step 1: 基础信息 =====
                                    div { "data-testid": "init-wizard-step-1",
                                        // [原组织名称/组织描述/用户名/密码/显示名/邮箱 6 个 form-control 原样移入]

                                        div { class: "form-control mt-4",
                                            button {
                                                r#type: "button",
                                                class: "btn btn-primary w-full",
                                                "data-testid": "init-next-step",
                                                onclick: on_next_step,
                                                "下一步"
                                            }
                                        }
                                    }
                                } else {
                                    // ===== Step 2: 模型配置 =====
                                    div { class: "flex flex-col gap-1", "data-testid": "init-wizard-step-2",

                                        // 对话模型开关（可选）
                                        div { class: "form-control w-full",
                                            label { class: "label cursor-pointer justify-start gap-2",
                                                input {
                                                    r#type: "checkbox",
                                                    class: "checkbox checkbox-primary",
                                                    "data-testid": "init-enable-chat",
                                                    checked: enable_chat(),
                                                    onchange: move |e| enable_chat.set(e.checked()),
                                                }
                                                span { class: "label-text", "配置对话模型（用于 Agent 思考与对话，推荐）" }
                                            }
                                        }

                                        if enable_chat() {
                                            div { class: "divider text-sm opacity-70", "对话模型配置" }
                                            // [原对话模型 5 字段原样移入：Provider 名称/服务商类型/模型名称/API Key/Base URL]
                                        } else {
                                            div {
                                                class: "alert alert-warning text-sm py-2 mb-1",
                                                "data-testid": "init-chat-skip-hint",
                                                "未配置对话模型：可稍后使用，但创建 Agent 前需先在「模型提供商管理」中配置"
                                            }
                                        }

                                        // 向量模型开关（推荐）
                                        div { class: "form-control w-full",
                                            label { class: "label cursor-pointer justify-start gap-2",
                                                input {
                                                    r#type: "checkbox",
                                                    class: "checkbox checkbox-primary",
                                                    "data-testid": "init-enable-embedding",
                                                    checked: enable_embedding(),
                                                    onchange: move |e| enable_embedding.set(e.checked()),
                                                }
                                                span { class: "label-text", "启用向量模型（推荐 — 用于语义搜索）" }
                                            }
                                        }

                                        if enable_embedding() {
                                            div { class: "divider text-sm opacity-70", "向量模型配置" }
                                            // [原向量模型 5 字段原样移入：Provider 名称/服务商类型/模型名称/API Key/Base URL]
                                        } else {
                                            div {
                                                class: "alert alert-warning text-sm py-2 mb-1",
                                                "data-testid": "init-embedding-skip-hint",
                                                "未启用向量模型：语义搜索不可用；后续补配时将自动全量重建向量索引，实体较多时耗时较长"
                                            }
                                        }

                                        // 按钮组
                                        div { class: "flex gap-2 mt-4",
                                            button {
                                                r#type: "button",
                                                class: "btn btn-ghost flex-1",
                                                onclick: on_prev_step,
                                                "上一步"
                                            }
                                            button {
                                                r#type: "submit",
                                                class: "btn btn-primary flex-1",
                                                disabled: init_submitting(),
                                                "完成初始化"
                                            }
                                        }
                                    }
                                }
```

要点：
- Step 1 的按钮必须 `r#type: "button"`；整个表单唯一 `type="submit"` 在 Step 2 按钮组——HTML 隐式提交（回车键）需要 submit 按钮存在才触发，故 Step 1 中回车不会误提交。
- `<form>` 的 `onsubmit`/`class` 等属性原样保留；所有信号定义保留（渲染是条件的，信号不销毁）。
- 原提交按钮若有其他样式/禁用逻辑，随 `disabled: init_submitting()` 一并保留。

- [ ] **Step 5: 编译验证**

Run: `cargo check -p frontend --target wasm32-unknown-unknown 2>&1 | tail -10`
Expected: 通过。

- [ ] **Step 6: Commit**

```bash
git add frontend/src/pages/reception.rs
git commit -m "feat(frontend): 初始化页重构为两步表单，对话模型可选，向量模型跳过后果提示"
```

---

### Task 7: 前端 — Agent 创建：capability 过滤 + 空态引导

**Files:**
- Modify: `frontend/src/pages/hr/agents.rs:457-474`（provider 选择控件）

- [ ] **Step 1: 过滤 + 空态引导**

将 L457-474 的模型提供商控件整体替换为：

```rust
                            div { class: "form-control w-full",
                                label { class: "label",
                                    span { class: "label-text font-medium", "模型提供商 *" }
                                }
                                if model_providers.read().iter().filter(|mp| mp.capability.is_agent()).count() == 0 {
                                    div { class: "flex flex-col gap-1",
                                        input {
                                            class: "input input-bordered w-full opacity-60",
                                            value: "{new_model_provider_id}",
                                            oninput: move |e| new_model_provider_id.set(e.value()),
                                            placeholder: "暂无可用对话模型，请先在「模型提供商管理」中配置"
                                        }
                                        a {
                                            class: "link link-primary link-hover text-xs",
                                            href: "/finance/model-providers",
                                            "前往模型提供商管理 →"
                                        }
                                    }
                                } else {
                                    select { class: "select select-bordered w-full", value: "{new_model_provider_id}",
                                        onchange: move |e| new_model_provider_id.set(e.value()),
                                        option { value: "", "-- 请选择 --" }
                                        for mp in model_providers.read().iter().filter(|mp| mp.capability.is_agent()) {
                                            option { value: "{mp.id}", "{mp.name} ({mp.model_name})" }
                                        }
                                    }
                                }
                            }
```

要点：两处 `.filter(|mp| mp.capability.is_agent())`（空态判断 + 下拉渲染）；Embedding provider 不再出现在下拉（后端仍兜底校验）。

- [ ] **Step 2: 编译验证**

Run: `cargo check -p frontend --target wasm32-unknown-unknown 2>&1 | tail -10`
Expected: 通过。

- [ ] **Step 3: Commit**

```bash
git add frontend/src/pages/hr/agents.rs
git commit -m "fix(frontend): Agent 创建仅列 Agent 能力 provider，空态引导去模型管理"
```

---

### Task 8: 前端 — 模型管理：创建 capability 选择 + 禁用/删除语义分离

**Files:**
- Modify: `frontend/src/pages/finance/model_providers.rs`（信号区、Modal 表单、handle_create、列表操作按钮、删除确认文案）

- [ ] **Step 1: 加信号**

信号定义区（L30-L52 附近）加：

```rust
    let mut new_capability = use_signal(|| 0i32); // 0=Agent(对话) 1=Embedding(向量)
```

- [ ] **Step 2: handle_create 使用动态 capability + 重建提示**

`handle_create` 中 `capability: ModelCapability::Agent,` 改为：

```rust
                capability: ModelCapability::from_i32(new_capability()),
```

创建成功的 `Ok(resp)` 分支（L91 起）在 toast 成功提示后按状态区分追加：

```rust
                    if resp.rebuild_task_id.is_some() {
                        toast.info("已触发向量索引全量重建，期间语义搜索可能不完整");
                    } else if resp.status == 2 {
                        toast.info("已创建为未启用状态；在列表点「启用」并确认切换后生效");
                    }
                    new_capability.set(0);
```

- [ ] **Step 3: Modal 表单加 capability 选择**

「添加提供商」Modal 中，服务商类型 select 之后插入：

```rust
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text font-medium", "能力类型 *" }
                                    }
                                    select {
                                        class: "select select-bordered w-full",
                                        "data-testid": "mp-create-capability",
                                        value: "{new_capability}",
                                        onchange: move |e| {
                                            if let Ok(v) = e.value().parse::<i32>() {
                                                new_capability.set(v);
                                            }
                                        },
                                        option { value: "0", "Agent（对话 / 思考）" }
                                        option { value: "1", "Embedding（向量化 / 语义搜索）" }
                                    }
                                }
```

API Key 输入的 label 由固定 `"API Key *"` 改为动态：

```rust
                                        span { class: "label-text font-medium",
                                            if new_capability() == 1 { "API Key（FastEmbed 无需填写）" } else { "API Key *" }
                                        }
```

- [ ] **Step 4: 禁用按钮语义修正（status 0 → 2）**

现状「禁用」按钮（L228-L243）发 `status: 0`（软删除，条目从列表消失），名为禁用实为删除。改为发 `status: 2`（Disabled，条目保留、显示「禁用」badge、可再启用）——与既有红色「删除」按钮（L303-L309，走 `delete_model_provider` API）形成真正的语义分离：

```rust
                                        if is_enabled {
                                            button { class: "btn btn-outline btn-sm",
                                                onclick: {
                                                    let id = id_toggle.clone();
                                                    move |_| {
                                                        let id = id.clone();
                                                        spawn(async move {
                                                            // Disabled=2：真禁用（条目保留可再启用）；软删除走「删除」按钮
                                                            if let Ok(()) = toggle_model_provider(UpdateModelProviderStatusRequest { id, status: 2 }).await {}
                                                            match list_model_providers().await {
                                                                Ok(list) => providers.set(list.providers),
                                                                Err(e) => toast.error(&e),
                                                            }
                                                        });
                                                    }
                                                },
                                                "禁用"
                                            }
                                        } else {
                                            // ... 启用分支保持不动（含 embedding 的 switch modal 处理）
                                        }
```

（仅改 `status: 0` 为 `status: 2` 一处数值；启用分支与删除按钮零改动。）

- [ ] **Step 5: 删除确认文案按能力区分警示**

`ConfirmDialog`（L476-L498）message 改为按 capability 提示——删除启用中的 Embedding 会让向量搜索静默失效，需明确警示：

```rust
        ConfirmDialog {
            show: show_delete_confirm(),
            title: "确认删除".to_string(),
            message: {
                let id = pending_delete_id();
                let is_embedding = providers.read().iter()
                    .any(|p| p.id == id && p.capability.is_embedding());
                if is_embedding {
                    "确定删除此模型提供商？若它是使用中的向量模型，删除后语义搜索将失效，需重新配置并全量重建向量索引。".to_string()
                } else {
                    "确定删除此模型提供商？此操作不可撤销。".to_string()
                }
            },
            // on_confirm / on_cancel 保持不动
```

- [ ] **Step 6: 编译验证**

Run: `cargo check -p frontend --target wasm32-unknown-unknown 2>&1 | tail -10`
Expected: 通过。

- [ ] **Step 7: Commit**

```bash
git add frontend/src/pages/finance/model_providers.rs
git commit -m "feat(frontend): 模型管理支持 Embedding 创建；禁用改真禁用，与删除语义分离"
```

---

### Task 9: 全量验证

**Files:** 无新改动（验证 + 修复回归）

- [ ] **Step 1: 格式化 + lint**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -10`
Expected: clippy 零警告（后端 host target）。

- [ ] **Step 2: 前端 wasm clippy**

Run: `cargo clippy -p frontend --target wasm32-unknown-unknown -- -D warnings 2>&1 | tail -10`
Expected: 零警告。

- [ ] **Step 3: 全量测试**

Run: `cargo test --workspace 2>&1 | tail -25`
Expected: 全部通过（1124+ 基线 + 本计划新增用例）。

- [ ] **Step 4: 手动冒烟（可选，需用户配合）**

`make dev` → 访问 `http://localhost:8080`：
1. 全新库（删 `.ai_orz/`）→ 向导 Step 1 填基础信息 →「下一步」（留空必填项应报错拦截）→ Step 2 取消「配置对话模型」→ 跳过提示出现 →「完成初始化」→ 进度完成 → 登录 → Agent 创建页显示「暂无可用对话模型」引导 → 模型管理创建 Agent provider → Agent 可创建。
2. 模型管理创建首个 Embedding provider → toast「已触发向量索引重建」；再创建第二个 → toast「已创建为未启用状态」→ 列表中第二条显示「禁用」badge → 点「启用」→ 弹切换确认 modal（含重建警示）→ 确认 → 切换完成。
3. 对启用中的 provider 点「禁用」→ 条目保留、badge 变「禁用」→ 可再「启用」；点「删除」→ 确认框（embedding 时含向量搜索失效警示）→ 条目消失。

- [ ] **Step 5: 最终提交（如有修复）**

```bash
git add -A && git commit -m "chore: 初始化模型策略调整收尾（fmt/clippy/test 全绿）"
```

---

## 后续扩展（本计划不做，记录防丢）

1. **switch 文案区分首次/切换**：首次补配（无旧 provider）时 switch modal 文案「禁用当前的 Embedding Provider」不准确，可按 `previous_provider_id == null` 区分文案。
2. **删除启用中 embedding 的后端守卫**：前端确认文案已警示（T8），但直调 API 仍可删除唯一启用的 embedding 导致向量搜索静默失效；可在 domain `delete_model_provider` 加守卫拒绝。
3. **重建进度可视化**：`rebuild_task_id` 已返回，前端可接 rebuild progress 接口做进度条。
4. **reception.rs 拆分组件**：两步表单落地后文件仍 ~700 行（4 种页面状态混合），可拆 `InitWizardForm` / `LoginForm` 子组件（纯结构重构，与本计划解耦）。

## Self-Review 记录

- 需求覆盖：对话模型可选（T1/T2/T3/T6）、向量模型强推荐+后果提示（T6）、初始化向导 2 步分步+逐步校验+统一提交（T6，用户确认并入）、后端边界校验（T2）、Agent 创建引导（T7）、补配闭环+创建不阻塞+启用时切换确认（T4/T5/T8，用户决策修正）、禁用/删除语义分离（T8，用户决策追加）✓
- 类型一致性：`chat_model: Option<ModelProviderInitConfig>`（T1 ↔ T2 ↔ T3 ↔ T6）；`rebuild_task_id: Option<String>` + `status: i32`（T4/T5 定义 ↔ T8 消费）✓
- Embedding 生命周期闭环自洽：首个创建 Normal+重建 → 第二个创建 Disabled 不重建 → 启用时 409 → switch modal 确认 → 软删旧+启用新+重建（既有链路）；编辑 Normal 配置变化重建、编辑 Disabled 不重建（switch 兜底）；禁用（Disabled）与删除（软删）前端语义分离 ✓
- 分步交互边界：Step 1 按钮全 `type=button`、唯一 submit 在 Step 2 → 回车误提交被 HTML 隐式提交规则天然阻止；两步均只做前端体验校验，最终以后端 T2 边界校验为准 ✓
- 已知不确定点（执行时先核实再写码）：Task 3/4/5 中 `tests/common` 的 jwt 辅助函数名、list 响应字段名；Task 4 枚举加变体后全仓穷举 match 的补齐点；Task 7 的 `link` 样式类是否与项目 DaisyUI 用法一致；Task 6 表单尾部原提交按钮的确切位置（L624-632）与样式；Task 8 Step 5 中 `providers.read()` 在 rsx 闭包内读取的借用方式（如报借用错误，改用 signal clone 后读取）
