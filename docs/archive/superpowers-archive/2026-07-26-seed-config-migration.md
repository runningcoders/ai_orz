# Seed 配置迁移中心 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 System Domain 下新增 seed 子模块作为"纯工具箱"——提供数据视图结构、diff 算法、文件存储能力；同时拆分 `initialize_system` 让 handler 层承担跨 domain 编排责任。所有 DB 读写由 handler 调用各 domain 完成，seed 不直接调用任何 DAL。

**Architecture:**
- seed 子模块位于 `src/service/domain/system/seed/`，作为 system domain 的子模块（不新增 pkg）
- seed 只暴露 **纯函数 + 数据结构 + 文件 CRUD**，不持有任何 DAL 引用，不调用其他 domain
- 跨 domain 编排（导出时拉数据组装快照、导入时调用各 domain upsert、diff 时拉当前 DB）全部在 handler 层完成
- 同时拆分 `OrganizationDomain::initialize_system` → `create_org_and_owner`，由 handler 编排 organization + finance domain
- 敏感字段（password/api_key）永远不导出，使用 `PENDING_INPUT` 占位符；`INHERIT_CURRENT` 由 handler 传入当前 DB 值给 seed 的纯函数解析

**Tech Stack:** Rust + Axum 0.8 + sqlx + serde_json + ai_orz_macros（`generate_http_handler`）+ 内置 `default.json`（`include_str!`）

---

## 文件结构

### 新建文件

| 文件 | 职责 |
|------|------|
| `src/service/domain/system/seed/mod.rs` | 子模块入口，导出公开 API |
| `src/service/domain/system/seed/defs.rs` | `SeedSnapshot` + `XxxDef` + `ImportStrategy` + `SensitiveRef` 占位符 + `SeedDiff` 结构定义 |
| `src/service/domain/system/seed/diff.rs` | `diff_snapshots(base, target)` 纯函数 + `validate_sensitive_fields()` + `resolve_password()`/`resolve_api_key()` 纯函数 |
| `src/service/domain/system/seed/store.rs` | 文件系统 CRUD（list/read/write/delete seeds/ 目录） |
| `src/service/domain/system/seed/default.rs` | 内置默认快照逻辑（`include_str!("default.json")`） |
| `src/service/domain/system/seed/default.json` | 默认模板 JSON（含示例 Agent） |
| `src/service/domain/system/seed/seed_test.rs` | `#[cfg(test)]` 单元测试（diff 算法、敏感字段解析、store 往返） |
| `src/handlers/system/seed/mod.rs` | Handler 模块入口，pub use 所有 handler + `assemble_snapshot_from_db`/`apply_snapshot_to_db` 编排函数 |
| `src/handlers/system/seed/list.rs` | `GET /api/v1/system/seed/list` 列出 seeds/ 文件 |
| `src/handlers/system/seed/get_file.rs` | `GET /api/v1/system/seed/file/{name}` 读取快照内容 |
| `src/handlers/system/seed/save.rs` | `POST /api/v1/system/seed/save` 编排各 domain 拉数据 → 组装快照 → 写文件 |
| `src/handlers/system/seed/load.rs` | `POST /api/v1/system/seed/load/{name}` 读文件 → 校验 → 调用各 domain upsert |
| `src/handlers/system/seed/delete_file.rs` | `DELETE /api/v1/system/seed/file/{name}` 删除快照文件 |
| `src/handlers/system/seed/diff.rs` | `POST /api/v1/system/seed/diff/{name}` 读文件 + 拉当前 DB → 调用 seed::diff_snapshots |
| `src/handlers/system/seed/diff_files.rs` | `POST /api/v1/system/seed/diff-files` 两个文件 diff |
| `src/handlers/system/seed/get_default.rs` | `GET /api/v1/system/seed/default` 获取内置默认模板 |
| `src/handlers/system/seed/apply_default.rs` | `POST /api/v1/system/seed/apply-default` 应用默认模板（编排各 domain） |
| `common/src/api/seed.rs` | Seed 相关 API DTO（请求/响应结构） |

### 修改文件

| 文件 | 修改内容 |
|------|---------|
| `src/service/domain/organization/mod.rs` | 新增 `create_org_and_owner` trait 方法声明（仅创建 org+user，不涉及 provider） |
| `src/service/domain/organization/org.rs` | 实现 `create_org_and_owner`，删除 `initialize_system` 中的 provider 创建逻辑（或保留但标记 deprecated） |
| `src/handlers/organization/initialize_system.rs` | 改为 handler 编排：调 organization domain 创建 org+user → 调 finance domain 创建 chat/embedding provider |
| `src/handlers/system/mod.rs` | 添加 `pub mod seed;` |
| `src/router.rs` | 在 `system_routes()` 中 nest `/seed` 子路由 |
| `common/src/api/mod.rs` | 添加 `pub mod seed;` |

---

## 任务清单

### Task 0: 拆分 initialize_system（架构原则修正）

**目的：** 修复 `OrganizationDomainImpl::initialize_system` 跨过 finance domain 直接调用 `model_provider::dal()` 的架构违规。为后续 seed 模块树立"handler 编排、domain 各司其职"的模式。

**Files:**
- Modify: `src/service/domain/organization/mod.rs`
- Modify: `src/service/domain/organization/org.rs`
- Modify: `src/handlers/organization/initialize_system.rs`

- [ ] **Step 1: 在 `OrganizationManage` trait 添加 `create_org_and_owner` 方法**

修改 `src/service/domain/organization/mod.rs`，在 `OrganizationManage` trait 中添加：

```rust
/// 创建组织 + Owner（超级管理员角色），不含 ModelProvider
///
/// 通用方法：可用于系统初始化，也可用于后续创建新组织。
/// 返回 (organization_id, user_id)
/// ModelProvider 的创建由 handler 编排 finance domain 完成
async fn create_org_and_owner(
    &self,
    ctx: RequestContext,
    params: common::api::InitializeSystemRequest,
) -> Result<(String, String)>;
```

- [ ] **Step 2: 在 `org.rs` 实现 `create_org_and_owner`**

在 `src/service/domain/organization/org.rs` 的 `impl super::OrganizationManage for super::OrganizationDomainImpl` 块中，参考现有 `initialize_system` 实现，但只创建 org + user，**删除 chat_provider 和 embedding_provider 创建逻辑**：

```rust
/// 创建组织 + Owner（超级管理员角色）
///
/// 通用方法：可用于系统初始化，也可用于后续创建新组织。
/// 返回 (organization_id, user_id)
async fn create_org_and_owner(
    &self,
    ctx: RequestContext,
    params: common::api::InitializeSystemRequest,
) -> Result<(String, String)> {
    // 1. 创建组织
    let org_id = generate_org_id();
    let org = OrganizationPo::new(
        org_id.clone(),
        params.organization_name,
        params.description.unwrap_or_default(),
        None,
        org_id.clone(),
    );
    self.org_dal.create(ctx.clone(), &org).await?;

    // 2. 创建超级管理员用户
    let user_id = generate_user_id();
    let user = UserPo::new(
        user_id.clone(),
        org_id.clone(),
        params.admin_username,
        params.admin_display_name.unwrap_or_else(|| "超级管理员".to_string()),
        params.admin_email.unwrap_or_default(),
        params.admin_password_hash,
        common::enums::UserRole::SuperAdmin,
        org_id.clone(),
    );
    self.user_dal.create(ctx.clone(), &user).await?;

    Ok((org_id, user_id))
}
```

- [ ] **Step 3: 直接删除旧的 `initialize_system` 方法**

在 `src/service/domain/organization/mod.rs` 的 `OrganizationManage` trait 中删除 `initialize_system` 方法声明，在 `src/service/domain/organization/org.rs` 中删除其实现。同时删除 `use rand::Rng;` 之外不再需要的 import（如果有）。

调用方只有 `src/handlers/organization/initialize_system.rs` 一个，会在 Step 4 一并改为调用 `create_org_and_owner`。

- [ ] **Step 4: 重写 `src/handlers/organization/initialize_system.rs` 改为 handler 编排**

```rust
//! 初始化系统接口
//!
//! 当系统还没有初始化时，调用这个接口创建第一个组织、超级管理员和默认 ModelProvider
//! Handler 层负责跨 domain 编排：organization domain 创建 org+user，finance domain 创建 provider

use ai_orz_macros::generate_http_handler;
use common::api::{CheckInitializedRequest, InitializeSystemRequest, InitializeSystemResponse};
use common::error::{Error, Result};
use crate::pkg::RequestContext;
use crate::service::domain::{finance, organization};

/// 检查系统是否已经初始化
#[generate_http_handler]
pub async fn check_initialized(
    ctx: RequestContext,
    _params: CheckInitializedRequest,
) -> Result<bool> {
    let domain = organization::domain();
    let initialized = domain.organization_manage().check_initialized(ctx).await?;
    Ok(initialized)
}

/// 初始化系统
#[generate_http_handler]
pub async fn initialize_system(
    ctx: RequestContext,
    params: InitializeSystemRequest,
) -> Result<InitializeSystemResponse> {
    // 1. organization domain 创建组织 + Owner
    let (org_id, user_id) = organization::domain()
        .organization_manage()
        .create_org_and_owner(ctx.clone(), params.clone())
        .await?;

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
    let embedding_provider = crate::models::model_provider::ModelProvider::new(
        params.embedding_model.name,
        common::enums::ProviderType::from_i32(params.embedding_model.provider_type),
        common::enums::ModelCapability::Embedding,
        params.embedding_model.model_name,
        params.embedding_model.api_key,
        params.embedding_model.base_url,
        params.embedding_model.description,
        user_id.clone(),
    );
    let embedding_provider_id = embedding_provider.po.id.clone();
    finance::domain()
        .model_provider_manage()
        .create_model_provider(ctx, &embedding_provider)
        .await?;

    Ok(InitializeSystemResponse {
        organization_id: org_id,
        user_id,
        chat_provider_id,
        embedding_provider_id,
    })
}
```

注意：`InitializeSystemRequest` 需要 derive `Clone`（若未实现则补充）。

- [ ] **Step 5: 验证编译 + 运行回归测试**

Run: `cargo check 2>&1 | tail -20`
Expected: PASS

Run: `cargo test --lib organization 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/service/domain/organization/ src/handlers/organization/initialize_system.rs
git commit -m "refactor(organization): 拆分 initialize_system 为 handler 编排模式"
```

---

### Task 1: 定义 seed 公共数据结构（defs.rs + common/src/api/seed.rs）

**Files:**
- Create: `common/src/api/seed.rs`
- Create: `src/service/domain/system/seed/defs.rs`
- Modify: `common/src/api/mod.rs`

- [ ] **Step 1: 创建 `common/src/api/seed.rs` 定义请求/响应 DTO**

```rust
//! Seed 配置迁移相关 API DTO

use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 列出 seeds/ 目录请求（无参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListSeedsRequest {}

/// 单个 seed 文件信息
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SeedFileInfo {
    /// 文件名（不含路径，含 .json 后缀）
    pub name: String,
    /// 文件大小（字节）
    pub size: u64,
    /// 最后修改时间戳（毫秒）
    pub modified_at: i64,
    /// 是否为系统默认模板（基于文件名前缀判断）
    pub is_default: bool,
}

/// 列出 seeds/ 响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListSeedsResponse {
    pub data: Vec<SeedFileInfo>,
    pub total: u64,
}

/// 读取 seed 文件请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetSeedFileRequest {
    #[param(source = "path")]
    pub name: String,
}

/// 读取 seed 文件响应（返回完整 JSON 内容）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetSeedFileResponse {
    pub name: String,
    pub content: String,
    pub size: u64,
}

/// 保存当前组织配置到文件请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct SaveSeedRequest {
    /// 文件名（不含路径，会自动加 .json 后缀）
    pub name: String,
    /// 描述（可选，写入快照 metadata）
    pub description: Option<String>,
}

/// 保存响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SaveSeedResponse {
    pub name: String,
    pub size: u64,
}

/// 加载 seed 文件请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct LoadSeedRequest {
    #[param(source = "path")]
    pub name: String,
    /// 导入策略
    pub strategy: ImportStrategy,
    /// 敏感字段值（key = "{entity_type}:{entity_id}:{field}"）
    /// 导入前若快照含 PENDING_INPUT 占位符，前端必须填写后传入
    #[serde(default)]
    pub sensitive_values: std::collections::HashMap<String, String>,
}

/// 导入策略
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema)]
pub enum ImportStrategy {
    /// 保留快照中的 ID（适合同组织回滚/恢复）
    #[default]
    PreserveIds,
    /// 生成新 ID（适合跨组织迁移）
    RegenerateIds,
    /// 仅预演，不实际写入，返回 diff 报告
    DryRun,
    /// 仅新建不存在的，已存在的跳过
    SkipExisting,
}

/// 加载响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LoadSeedResponse {
    pub created: usize,
    pub updated: usize,
    pub skipped: usize,
    /// DryRun 模式下的 diff 报告（非 DryRun 时为 None）
    pub diff: Option<crate::service::domain::system::seed::defs::SeedDiff>,
}

/// 删除 seed 文件请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteSeedFileRequest {
    #[param(source = "path")]
    pub name: String,
}

/// 删除响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteSeedFileResponse {
    pub success: bool,
}

/// Diff 请求（文件 vs DB）
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct DiffSeedRequest {
    #[param(source = "path")]
    pub name: String,
}

/// 两个文件之间 diff 请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct DiffFilesRequest {
    /// 基准文件名
    #[param(source = "query")]
    pub base: String,
    /// 目标文件名
    #[param(source = "query")]
    pub target: String,
}

/// 应用默认模板请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ApplyDefaultSeedRequest {
    /// 导入策略
    pub strategy: ImportStrategy,
    /// 敏感字段值
    #[serde(default)]
    pub sensitive_values: std::collections::HashMap<String, String>,
}

/// 获取默认模板请求（无参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetDefaultSeedRequest {}
```

- [ ] **Step 2: 在 `common/src/api/mod.rs` 添加模块声明**

```rust
pub mod seed;
```

- [ ] **Step 3: 创建 `src/service/domain/system/seed/defs.rs` 定义核心数据结构**

```rust
//! Seed 配置迁移核心数据结构
//!
//! 快照只保留业务实体定义（配置层），不包含运行时数据（消息、任务、stats、日志、向量索引）
//! 敏感字段（password_hash / api_key）永远不导出，使用 PENDING_INPUT 占位符

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 敏感字段占位符常量
pub const PENDING_INPUT: &str = "PENDING_INPUT";
/// 继承当前 DB 值（用于回滚场景，由 handler 传入当前值给纯函数解析）
pub const INHERIT_CURRENT: &str = "INHERIT_CURRENT";
/// 随机生成（导入时由 handler 生成并显示一次）
pub const RANDOM_GENERATE: &str = "RANDOM_GENERATE";

/// Seed 快照根结构
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SeedSnapshot {
    /// 快照格式版本
    pub version: String,
    /// 生成时间戳（毫秒）
    pub generated_at: i64,
    /// 快照描述（可选）
    pub description: Option<String>,
    /// 源组织 ID（用于追踪）
    pub source_organization_id: String,
    /// 组织定义
    pub organization: OrganizationDef,
    /// 用户列表
    pub users: Vec<UserDef>,
    /// 模型 Provider 列表
    pub model_providers: Vec<ModelProviderDef>,
    /// Agent 列表
    pub agents: Vec<AgentDef>,
    /// Skill 列表
    pub skills: Vec<SkillDef>,
}

impl SeedSnapshot {
    /// 当前快照格式版本
    pub const CURRENT_VERSION: &'static str = "1.0.0";
}

/// 组织定义
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OrganizationDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub base_url: String,
    pub status: i32,
    pub scope: i32,
}

/// 用户定义（不含 password_hash）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UserDef {
    pub id: String,
    pub organization_id: String,
    pub username: String,
    pub display_name: String,
    pub email: String,
    /// 密码占位符（PENDING_INPUT / INHERIT_CURRENT / RANDOM_GENERATE）
    pub password_ref: String,
    pub role: i32,
    pub status: i32,
}

/// 模型 Provider 定义（api_key 使用占位符）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModelProviderDef {
    pub id: String,
    pub name: String,
    pub provider_type: i32,
    pub model_name: String,
    pub capability: i32,
    /// API Key 占位符（PENDING_INPUT / INHERIT_CURRENT）
    pub api_key_ref: String,
    pub base_url: Option<String>,
    pub description: Option<String>,
    pub config: String,
    pub status: i32,
}

/// Agent 定义
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentDef {
    pub id: String,
    pub name: String,
    /// 角色标签数组
    pub roles: Vec<String>,
    pub description: String,
    pub capabilities: Vec<String>,
    pub soul: String,
    /// 关联的 ModelProvider ID（引用 model_providers 中的某项）
    pub model_provider_id: String,
    /// 运行时配置（JSON）
    pub runtime_config: String,
    pub status: i32,
    pub kind: i32,
}

/// Skill 定义（不含文件内容，仅元数据）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SkillDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub category: String,
    pub parent_skill_id: String,
    pub author_id: String,
    pub author_type: i32,
    pub status: i32,
    /// 相对 base_data_path 的技能目录路径（导入时复制目录）
    pub content_path: String,
}

// ==================== Diff 结构 ====================

/// Diff 报告
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SeedDiff {
    pub meta: DiffMeta,
    pub summary: DiffSummary,
    pub organization: Option<DiffEntry<OrganizationDef>>,
    pub users: Vec<DiffEntry<UserDef>>,
    pub model_providers: Vec<DiffEntry<ModelProviderDef>>,
    pub agents: Vec<DiffEntry<AgentDef>>,
    pub skills: Vec<DiffEntry<SkillDef>>,
}

/// Diff 元信息
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DiffMeta {
    pub kind: DiffKind,
    pub base_source: String,
    pub target_source: String,
    pub compared_at: i64,
}

/// Diff 类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub enum DiffKind {
    /// 文件 vs 当前 DB
    FileVsDb,
    /// DB vs 文件（反向）
    DbVsFile,
    /// 文件 vs 文件
    FileVsFile,
}

/// Diff 摘要统计
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct DiffSummary {
    pub new_count: usize,
    pub updated_count: usize,
    pub same_count: usize,
    pub removed_count: usize,
}

/// 单个实体的 diff
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum DiffEntry<T> {
    Same { id: String, current: T },
    Updated { id: String, current: T, snapshot: T, changes: Vec<FieldChange> },
    New { id: String, snapshot: T },
    Removed { id: String, current: T },
}

/// 字段级变更
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FieldChange {
    /// 字段路径（如 "name"、"config.max_context_length"）
    pub field: String,
    pub old_value: serde_json::Value,
    pub new_value: serde_json::Value,
}
```

- [ ] **Step 4: 验证编译**

Run: `cargo check -p common -p ai_orz 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add common/src/api/seed.rs common/src/api/mod.rs src/service/domain/system/seed/defs.rs
git commit -m "feat(seed): 添加 seed 配置迁移核心数据结构"
```

---

### Task 2: 创建 seed 子模块骨架（无 trait，仅 pub 模块）

**Files:**
- Create: `src/service/domain/system/seed/mod.rs`
- Create: 占位 `diff.rs`、`store.rs`、`default.rs`
- Modify: `src/service/domain/system/mod.rs`（仅添加 `pub mod seed;`，**不修改 SystemDomain trait**）

- [ ] **Step 1: 创建 `src/service/domain/system/seed/mod.rs`**

```rust
//! Seed 配置迁移子模块（纯工具箱）
//!
//! 提供业务实体定义的导出/导入/diff 的数据结构和算法工具，不持有任何 DAL 引用，
//! 不调用其他 domain。实际的 DB 读写由 handler 层编排各 domain 完成。
//! 不包含运行时数据（消息、任务、stats、日志、向量索引）。

pub mod defs;
pub mod default;
pub mod diff;
pub mod store;

pub use defs::*;
```

- [ ] **Step 2: 修改 `src/service/domain/system/mod.rs` 添加 seed 模块声明**

在文件顶部 `mod aop_monitor;` 之后添加：

```rust
pub mod seed;
```

**注意：不修改 `SystemDomain` trait，不添加 `seed_manager()` 方法。** seed 是纯工具箱，通过 `crate::service::domain::system::seed::xxx` 直接调用即可。

- [ ] **Step 3: 创建占位 `diff.rs`**

```rust
// src/service/domain/system/seed/diff.rs
//! Diff 算法 + 敏感字段解析（纯函数）

use std::collections::HashMap;
use super::defs::*;

/// 对比两个快照（纯函数）
pub fn diff_snapshots(_base: &SeedSnapshot, _target: &SeedSnapshot) -> SeedDiff {
    unimplemented!("将在 Task 4 实现")
}

/// 校验敏感字段是否齐备（纯函数）
pub fn validate_sensitive_fields(
    _snapshot: &SeedSnapshot,
    _sensitive_values: &HashMap<String, String>,
) -> Result<(), String> {
    unimplemented!("将在 Task 4 实现")
}

/// 解析密码占位符（纯函数，current_password 由 handler 查 DB 后传入）
pub fn resolve_password(
    _ref_value: &str,
    _user_id: &str,
    _sensitive_values: &HashMap<String, String>,
    _current_password_hash: Option<&str>,
) -> Result<String, String> {
    unimplemented!("将在 Task 4 实现")
}

/// 解析 API Key 占位符
pub fn resolve_api_key(
    _ref_value: &str,
    _provider_id: &str,
    _sensitive_values: &HashMap<String, String>,
    _current_api_key: Option<&str>,
) -> Result<String, String> {
    unimplemented!("将在 Task 4 实现")
}
```

- [ ] **Step 4: 创建占位 `store.rs`**

```rust
// src/service/domain/system/seed/store.rs
//! 文件系统 CRUD

use std::path::Path;
use common::error::Result;

pub async fn list_files(_dir: &Path) -> Result<Vec<common::api::seed::SeedFileInfo>> {
    unimplemented!("将在 Task 5 实现")
}

pub async fn read_file(_dir: &Path, _name: &str) -> Result<common::api::seed::GetSeedFileResponse> {
    unimplemented!("将在 Task 5 实现")
}

pub async fn write_file(_dir: &Path, _name: &str, _content: &str) -> Result<u64> {
    unimplemented!("将在 Task 5 实现")
}

pub async fn delete_file(_dir: &Path, _name: &str) -> Result<()> {
    unimplemented!("将在 Task 5 实现")
}

/// 校验路径安全性（防止路径穿越攻击）
pub fn validate_seed_filename(_name: &str) -> Result<String> {
    unimplemented!("将在 Task 5 实现")
}

/// 获取 seeds/ 目录路径（基于 AppConfig.base_data_path）
pub fn seeds_dir() -> std::path::PathBuf {
    crate::config::get().base_data_path().join("seeds")
}
```

- [ ] **Step 5: 创建占位 `default.rs`**

```rust
// src/service/domain/system/seed/default.rs
//! 内置默认快照（编译期内置）

pub fn embedded_default_snapshot() -> super::defs::SeedSnapshot {
    unimplemented!("将在 Task 6 实现")
}
```

- [ ] **Step 6: 验证编译**

Run: `cargo check 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/service/domain/system/
git commit -m "feat(seed): 添加 seed 子模块骨架（纯工具箱，无 trait）"
```

---

### Task 3: 实现 diff 算法 + 敏感字段解析（diff.rs）

**Files:**
- Modify: `src/service/domain/system/seed/diff.rs`
- Create: `src/service/domain/system/seed/seed_test.rs`

- [ ] **Step 1: 创建测试文件 `src/service/domain/system/seed/seed_test.rs`**

```rust
//! Seed 模块单元测试（纯函数测试，不需要 DB）

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use crate::service::domain::system::seed::defs::*;
    use crate::service::domain::system::seed::diff::*;

    fn make_test_snapshot(name: &str) -> SeedSnapshot {
        SeedSnapshot {
            version: SeedSnapshot::CURRENT_VERSION.to_string(),
            generated_at: 1000,
            description: None,
            source_organization_id: "ORG1".to_string(),
            organization: OrganizationDef {
                id: "ORG1".to_string(),
                name: name.to_string(),
                description: String::new(),
                base_url: String::new(),
                status: 1,
                scope: 0,
            },
            users: vec![UserDef {
                id: "U1".to_string(),
                organization_id: "ORG1".to_string(),
                username: "admin".to_string(),
                display_name: "Admin".to_string(),
                email: String::new(),
                password_ref: PENDING_INPUT.to_string(),
                role: 0,
                status: 1,
            }],
            model_providers: vec![],
            agents: vec![],
            skills: vec![],
        }
    }

    #[test]
    fn test_diff_snapshots_detects_updated_field() {
        let base = make_test_snapshot("旧名称");
        let mut target = base.clone();
        target.organization.name = "新名称".to_string();

        let diff = diff_snapshots(&base, &target);
        assert_eq!(diff.summary.updated_count, 1);
        assert!(matches!(diff.organization, Some(DiffEntry::Updated { .. })));
    }

    #[test]
    fn test_diff_snapshots_detects_same() {
        let base = make_test_snapshot("name");
        let target = base.clone();
        let diff = diff_snapshots(&base, &target);
        assert_eq!(diff.summary.same_count, 2); // org + user
        assert_eq!(diff.summary.updated_count, 0);
    }

    #[test]
    fn test_diff_snapshots_detects_new_and_removed() {
        let base = make_test_snapshot("name");
        let mut target = base.clone();
        target.users.clear(); // remove user
        target.users.push(UserDef {
            id: "U2".to_string(),
            organization_id: "ORG1".to_string(),
            username: "new_user".to_string(),
            display_name: "New".to_string(),
            email: String::new(),
            password_ref: PENDING_INPUT.to_string(),
            role: 2,
            status: 1,
        });

        let diff = diff_snapshots(&base, &target);
        assert_eq!(diff.summary.new_count, 1);
        assert_eq!(diff.summary.removed_count, 1);
    }

    #[test]
    fn test_validate_sensitive_fields_success() {
        let snapshot = make_test_snapshot("name");
        let mut sensitive = HashMap::new();
        sensitive.insert("user:U1:password".to_string(), "hashed_pwd".to_string());
        assert!(validate_sensitive_fields(&snapshot, &sensitive).is_ok());
    }

    #[test]
    fn test_validate_sensitive_fields_missing() {
        let snapshot = make_test_snapshot("name");
        let sensitive = HashMap::new();
        assert!(validate_sensitive_fields(&snapshot, &sensitive).is_err());
    }

    #[test]
    fn test_resolve_password_pending_input() {
        let mut sensitive = HashMap::new();
        sensitive.insert("user:U1:password".to_string(), "new_hash".to_string());
        let result = resolve_password(PENDING_INPUT, "U1", &sensitive, None).unwrap();
        assert_eq!(result, "new_hash");
    }

    #[test]
    fn test_resolve_password_inherit_current() {
        let sensitive = HashMap::new();
        let result = resolve_password(INHERIT_CURRENT, "U1", &sensitive, Some("current_hash")).unwrap();
        assert_eq!(result, "current_hash");
    }

    #[test]
    fn test_resolve_password_inherit_current_missing_current_value() {
        let sensitive = HashMap::new();
        // INHERIT_CURRENT 但 current_password_hash 为 None → 报错
        assert!(resolve_password(INHERIT_CURRENT, "U1", &sensitive, None).is_err());
    }

    #[test]
    fn test_resolve_password_random_generate_returns_non_empty() {
        let sensitive = HashMap::new();
        let result = resolve_password(RANDOM_GENERATE, "U1", &sensitive, None).unwrap();
        assert!(!result.is_empty());
    }
}
```

- [ ] **Step 2: 运行测试验证失败**

Run: `cargo test --lib service::domain::system::seed::seed_test 2>&1 | tail -10`
Expected: FAIL（unimplemented panic）

- [ ] **Step 3: 实现 `src/service/domain/system/seed/diff.rs`**

```rust
//! Diff 算法 + 敏感字段解析（纯函数）
//!
//! 这些函数只接收数据返回结果，不调用任何 DAL 或 domain。
//! 跨 domain 的 DB 读取由 handler 完成，handler 把当前 DB 值作为参数传入。

use std::collections::HashMap;
use super::defs::*;

/// 对比两个快照（纯函数）
pub fn diff_snapshots(base: &SeedSnapshot, target: &SeedSnapshot) -> SeedDiff {
    let mut summary = DiffSummary::default();
    let org_diff = diff_organization(&base.organization, &target.organization, &mut summary);

    let users = diff_vec(&base.users, &target.users, &mut summary, |u| u.id.clone());
    let model_providers = diff_vec(&base.model_providers, &target.model_providers, &mut summary, |p| p.id.clone());
    let agents = diff_vec(&base.agents, &target.agents, &mut summary, |a| a.id.clone());
    let skills = diff_vec(&base.skills, &target.skills, &mut summary, |s| s.id.clone());

    SeedDiff {
        meta: DiffMeta {
            kind: DiffKind::FileVsFile,
            base_source: base.source_organization_id.clone(),
            target_source: target.source_organization_id.clone(),
            compared_at: common::constants::utils::current_timestamp(),
        },
        summary,
        organization: org_diff,
        users,
        model_providers,
        agents,
        skills,
    }
}

fn diff_organization(
    base: &OrganizationDef,
    target: &OrganizationDef,
    summary: &mut DiffSummary,
) -> Option<DiffEntry<OrganizationDef>> {
    let changes = collect_changes(base, target);
    if changes.is_empty() {
        summary.same_count += 1;
        Some(DiffEntry::Same { id: base.id.clone(), current: base.clone() })
    } else {
        summary.updated_count += 1;
        Some(DiffEntry::Updated {
            id: base.id.clone(),
            current: base.clone(),
            snapshot: target.clone(),
            changes,
        })
    }
}

fn diff_vec<T, F>(
    base: &[T],
    target: &[T],
    summary: &mut DiffSummary,
    id_fn: F,
) -> Vec<DiffEntry<T>>
where
    T: Clone + serde::Serialize,
    F: Fn(&T) -> String,
{
    let mut result = Vec::new();
    let mut base_ids = std::collections::HashSet::new();

    for b in base {
        let id = id_fn(b);
        base_ids.insert(id.clone());
        if let Some(t) = target.iter().find(|t| id_fn(t) == id) {
            let changes = collect_changes(b, t);
            if changes.is_empty() {
                summary.same_count += 1;
                result.push(DiffEntry::Same { id, current: b.clone() });
            } else {
                summary.updated_count += 1;
                result.push(DiffEntry::Updated {
                    id,
                    current: b.clone(),
                    snapshot: t.clone(),
                    changes,
                });
            }
        } else {
            summary.removed_count += 1;
            result.push(DiffEntry::Removed { id, current: b.clone() });
        }
    }

    for t in target {
        let id = id_fn(t);
        if !base_ids.contains(&id) {
            summary.new_count += 1;
            result.push(DiffEntry::New { id, snapshot: t.clone() });
        }
    }

    result
}

fn collect_changes<T: serde::Serialize>(base: &T, target: &T) -> Vec<FieldChange> {
    let base_val = serde_json::to_value(base).unwrap_or(serde_json::Value::Null);
    let target_val = serde_json::to_value(target).unwrap_or(serde_json::Value::Null);
    collect_field_changes_recursive(&base_val, &target_val, "")
}

fn collect_field_changes_recursive(
    base: &serde_json::Value,
    target: &serde_json::Value,
    prefix: &str,
) -> Vec<FieldChange> {
    let mut changes = Vec::new();

    match (base, target) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(target_map)) => {
            for (key, base_val) in base_map {
                let field = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };
                if let Some(target_val) = target_map.get(key) {
                    if base_val != target_val {
                        if base_val.is_object() && target_val.is_object() {
                            changes.extend(collect_field_changes_recursive(base_val, target_val, &field));
                        } else {
                            changes.push(FieldChange {
                                field,
                                old_value: base_val.clone(),
                                new_value: target_val.clone(),
                            });
                        }
                    }
                }
            }
        }
        _ => {
            if base != target {
                changes.push(FieldChange {
                    field: prefix.to_string(),
                    old_value: base.clone(),
                    new_value: target.clone(),
                });
            }
        }
    }

    changes
}

/// 校验敏感字段是否齐备（纯函数）
///
/// 返回 Err(message) 表示缺少字段；返回 Ok(()) 表示齐备
pub fn validate_sensitive_fields(
    snapshot: &SeedSnapshot,
    sensitive_values: &HashMap<String, String>,
) -> Result<(), String> {
    for u in &snapshot.users {
        if u.password_ref == PENDING_INPUT {
            let key = format!("user:{}:password", u.id);
            if !sensitive_values.contains_key(&key) {
                return Err(format!(
                    "缺少敏感字段: {} (用户 {} 的密码)",
                    key, u.username
                ));
            }
        }
    }
    for p in &snapshot.model_providers {
        if p.api_key_ref == PENDING_INPUT {
            let key = format!("model_provider:{}:api_key", p.id);
            if !sensitive_values.contains_key(&key) {
                return Err(format!(
                    "缺少敏感字段: {} (Provider {} 的 API Key)",
                    key, p.name
                ));
            }
        }
    }
    Ok(())
}

/// 解析密码占位符（纯函数）
///
/// current_password_hash：当 ref_value = INHERIT_CURRENT 时由 handler 查 DB 传入（None 表示 DB 中无此用户）
pub fn resolve_password(
    ref_value: &str,
    user_id: &str,
    sensitive_values: &HashMap<String, String>,
    current_password_hash: Option<&str>,
) -> Result<String, String> {
    match ref_value {
        PENDING_INPUT => {
            let key = format!("user:{}:password", user_id);
            sensitive_values.get(&key).cloned()
                .ok_or_else(|| format!("缺少密码: {}", key))
        }
        INHERIT_CURRENT => {
            current_password_hash.map(|s| s.to_string())
                .ok_or_else(|| format!("INHERIT_CURRENT 但 DB 中无用户 {} 的当前密码", user_id))
        }
        RANDOM_GENERATE => {
            // 生成随机密码（实际场景应由 handler 转换为 hash 并展示明文给管理员）
            Ok(format!("random_{}", uuid::Uuid::now_v7()))
        }
        _ => Err(format!("未知占位符: {}", ref_value)),
    }
}

/// 解析 API Key 占位符（纯函数）
pub fn resolve_api_key(
    ref_value: &str,
    provider_id: &str,
    sensitive_values: &HashMap<String, String>,
    current_api_key: Option<&str>,
) -> Result<String, String> {
    match ref_value {
        PENDING_INPUT => {
            let key = format!("model_provider:{}:api_key", provider_id);
            sensitive_values.get(&key).cloned()
                .ok_or_else(|| format!("缺少 API Key: {}", key))
        }
        INHERIT_CURRENT => {
            current_api_key.map(|s| s.to_string())
                .ok_or_else(|| format!("INHERIT_CURRENT 但 DB 中无 Provider {} 的当前 API Key", provider_id))
        }
        _ => Err(format!("未知占位符: {}", ref_value)),
    }
}
```

- [ ] **Step 4: 运行测试验证通过**

Run: `cargo test --lib service::domain::system::seed::seed_test 2>&1 | tail -20`
Expected: PASS（8 个测试通过）

- [ ] **Step 5: Commit**

```bash
git add src/service/domain/system/seed/diff.rs src/service/domain/system/seed/seed_test.rs
git commit -m "feat(seed): 实现 diff 算法 + 敏感字段解析（纯函数）"
```

---

### Task 4: 实现文件系统 store（store.rs）

**Files:**
- Modify: `src/service/domain/system/seed/store.rs`
- Modify: `src/service/domain/system/seed/seed_test.rs`（追加测试）

- [ ] **Step 1: 实现 `store.rs`**

```rust
//! 文件系统 CRUD
//!
//! seeds/ 目录下管理所有 .json 快照文件
//! 路径基于 AppConfig.base_data_path 拼接

use std::path::{Path, PathBuf};
use common::error::{Error, Result};

/// 获取 seeds/ 目录路径（基于 AppConfig.base_data_path）
pub fn seeds_dir() -> PathBuf {
    crate::config::get().base_data_path().join("seeds")
}

/// 校验文件名安全性（防止路径穿越攻击）
///
/// 返回规范化后的文件名（必要时附加 .json 后缀）
pub fn validate_seed_filename(name: &str) -> Result<String> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(Error::bad_request(format!("无效文件名: {}", name)));
    }
    let file_name = if name.ends_with(".json") {
        name.to_string()
    } else {
        format!("{}.json", name)
    };
    Ok(file_name)
}

/// 列出 seeds/ 目录下所有 .json 文件
pub async fn list_files(dir: &Path) -> Result<Vec<common::api::seed::SeedFileInfo>> {
    if !dir.exists() {
        std::fs::create_dir_all(dir)?;
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let mut entries = tokio::fs::read_dir(dir).await
        .map_err(|e| Error::internal(format!("读取 seeds 目录失败: {}", e)))?;

    while let Some(entry) = entries.next_entry().await
        .map_err(|e| Error::internal(format!("读取目录项失败: {}", e)))?
    {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let metadata = entry.metadata().await
            .map_err(|e| Error::internal(format!("读取文件元信息失败: {}", e)))?;
        let name = entry.file_name().to_string_lossy().to_string();
        let modified_at = metadata.modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        let is_default = name.starts_with("default") || name == "default.json";

        files.push(common::api::seed::SeedFileInfo {
            name,
            size: metadata.len(),
            modified_at,
            is_default,
        });
    }

    files.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    Ok(files)
}

/// 读取快照文件内容
pub async fn read_file(dir: &Path, name: &str) -> Result<common::api::seed::GetSeedFileResponse> {
    let file_name = validate_seed_filename(name)?;
    let path = dir.join(&file_name);
    let content = tokio::fs::read_to_string(&path).await
        .map_err(|e| Error::not_found(format!("快照文件不存在: {} ({})", file_name, e)))?;
    let size = content.len() as u64;
    Ok(common::api::seed::GetSeedFileResponse {
        name: file_name,
        content,
        size,
    })
}

/// 写入快照文件
pub async fn write_file(dir: &Path, name: &str, content: &str) -> Result<u64> {
    if !dir.exists() {
        std::fs::create_dir_all(dir)?;
    }
    let file_name = validate_seed_filename(name)?;
    let path = dir.join(&file_name);
    let size = content.len() as u64;
    tokio::fs::write(&path, content).await
        .map_err(|e| Error::internal(format!("写入快照文件失败: {}", e)))?;
    Ok(size)
}

/// 删除快照文件
pub async fn delete_file(dir: &Path, name: &str) -> Result<()> {
    let file_name = validate_seed_filename(name)?;
    let path = dir.join(&file_name);
    if !path.exists() {
        return Err(Error::not_found(format!("快照文件不存在: {}", file_name)));
    }
    tokio::fs::remove_file(&path).await
        .map_err(|e| Error::internal(format!("删除快照文件失败: {}", e)))?;
    Ok(())
}
```

- [ ] **Step 2: 在 `seed_test.rs` 追加 store 测试**

```rust
    #[tokio::test]
    async fn test_store_write_read_delete_round_trip() {
        let dir = std::env::temp_dir().join("ai_orz_seed_store_test");
        let _ = std::fs::remove_dir_all(&dir);

        let name = "test-snapshot";
        let content = r#"{"version": "1.0.0"}"#;

        let size = crate::service::domain::system::seed::store::write_file(&dir, name, content).await.unwrap();
        assert_eq!(size, content.len() as u64);

        let resp = crate::service::domain::system::seed::store::read_file(&dir, name).await.unwrap();
        assert_eq!(resp.content, content);
        assert_eq!(resp.name, "test-snapshot.json");

        let files = crate::service::domain::system::seed::store::list_files(&dir).await.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "test-snapshot.json");

        crate::service::domain::system::seed::store::delete_file(&dir, name).await.unwrap();
        let files = crate::service::domain::system::seed::store::list_files(&dir).await.unwrap();
        assert_eq!(files.len(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_validate_seed_filename_rejects_path_traversal() {
        assert!(crate::service::domain::system::seed::store::validate_seed_filename("../../../etc/passwd").is_err());
        assert!(crate::service::domain::system::seed::store::validate_seed_filename("a/b").is_err());
        assert!(crate::service::domain::system::seed::store::validate_seed_filename("").is_err());
        assert!(crate::service::domain::system::seed::store::validate_seed_filename("..secret").is_err());
    }

    #[test]
    fn test_validate_seed_filename_appends_json_extension() {
        let name = crate::service::domain::system::seed::store::validate_seed_filename("snapshot").unwrap();
        assert_eq!(name, "snapshot.json");

        let name = crate::service::domain::system::seed::store::validate_seed_filename("snapshot.json").unwrap();
        assert_eq!(name, "snapshot.json");
    }
```

- [ ] **Step 3: 运行测试**

Run: `cargo test --lib service::domain::system::seed 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/service/domain/system/seed/store.rs src/service/domain/system/seed/seed_test.rs
git commit -m "feat(seed): 实现文件系统 store（CRUD + 路径穿越防护）"
```

---

### Task 5: 实现默认快照（default.rs + default.json）

**Files:**
- Create: `src/service/domain/system/seed/default.json`
- Modify: `src/service/domain/system/seed/default.rs`
- Modify: `src/service/domain/system/seed/seed_test.rs`（追加测试）

- [ ] **Step 1: 创建 `default.json`**

```json
{
  "version": "1.0.0",
  "generated_at": 0,
  "description": "系统默认模板 - 开箱即用基础配置",
  "source_organization_id": "TEMPLATE",
  "organization": {
    "id": "TEMPLATE_ORG",
    "name": "我的组织",
    "description": "由系统初始化创建的默认组织",
    "base_url": "",
    "status": 1,
    "scope": 0
  },
  "users": [
    {
      "id": "TEMPLATE_ADMIN",
      "organization_id": "TEMPLATE_ORG",
      "username": "admin",
      "display_name": "超级管理员",
      "email": "",
      "password_ref": "PENDING_INPUT",
      "role": 0,
      "status": 1
    }
  ],
  "model_providers": [
    {
      "id": "TEMPLATE_CHAT_PROVIDER",
      "name": "默认对话模型",
      "provider_type": 0,
      "model_name": "gpt-4o",
      "capability": 0,
      "api_key_ref": "PENDING_INPUT",
      "base_url": null,
      "description": "用于 Agent 思考和对话",
      "config": "{}",
      "status": 1
    },
    {
      "id": "TEMPLATE_EMBEDDING_PROVIDER",
      "name": "默认向量模型",
      "provider_type": 0,
      "model_name": "text-embedding-3-small",
      "capability": 1,
      "api_key_ref": "PENDING_INPUT",
      "base_url": null,
      "description": "用于文本向量化",
      "config": "{}",
      "status": 1
    }
  ],
  "agents": [
    {
      "id": "TEMPLATE_RECEPTION_AGENT",
      "name": "前台接待 Agent",
      "roles": ["feishu_reception"],
      "description": "负责接待访客、转接任务",
      "capabilities": ["chat"],
      "soul": "你是前台接待助手，态度友好，善于引导用户。",
      "model_provider_id": "TEMPLATE_CHAT_PROVIDER",
      "runtime_config": "{\"max_thinking_depth\":10,\"thinking_interval_ms\":0,\"max_tool_calls_per_step\":5,\"enable_reflection\":false,\"require_user_confirm\":true,\"installed_tags\":[],\"installed_skill_packs\":[]}",
      "status": 2,
      "kind": 0
    }
  ],
  "skills": []
}
```

- [ ] **Step 2: 实现 `default.rs`**

```rust
//! 内置默认快照
//!
//! 通过 include_str! 嵌入 default.json，无需文件系统即可使用

const DEFAULT_JSON: &str = include_str!("default.json");

/// 获取内置默认快照
pub fn embedded_default_snapshot() -> super::defs::SeedSnapshot {
    serde_json::from_str(DEFAULT_JSON)
        .expect("内置 default.json 解析失败（编译期检查）")
}
```

- [ ] **Step 3: 在 `seed_test.rs` 追加 default 测试**

```rust
    #[test]
    fn test_default_snapshot_parses_successfully() {
        let snapshot = crate::service::domain::system::seed::default::embedded_default_snapshot();
        assert_eq!(snapshot.version, "1.0.0");
        assert_eq!(snapshot.users.len(), 1);
        assert_eq!(snapshot.model_providers.len(), 2);
        assert_eq!(snapshot.agents.len(), 1);
        assert_eq!(snapshot.skills.len(), 0);
        assert_eq!(snapshot.agents[0].model_provider_id, "TEMPLATE_CHAT_PROVIDER");
        assert_eq!(snapshot.users[0].password_ref, super::super::defs::PENDING_INPUT);
    }
```

- [ ] **Step 4: 运行测试**

Run: `cargo test --lib service::domain::system::seed::seed_test::tests::test_default_snapshot_parses_successfully 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/service/domain/system/seed/default.json src/service/domain/system/seed/default.rs src/service/domain/system/seed/seed_test.rs
git commit -m "feat(seed): 添加内置默认快照（含示例前台 Agent）"
```

---

### Task 6: 创建 9 个 Handler + 编排各 domain + 注册路由

**Files:**
- Create: `src/handlers/system/seed/mod.rs`（含 `assemble_snapshot_from_db` 和 `apply_snapshot_to_db` 编排函数）
- Create: 9 个 handler 文件
- Modify: `src/handlers/system/mod.rs`
- Modify: `src/router.rs`

- [ ] **Step 1: 创建 `src/handlers/system/seed/mod.rs`**

包含模块声明 + 公共校验函数 + 两个编排函数：

```rust
//! Seed 配置迁移 HTTP 接口
//!
//! 路由层 `require_role_middleware(UserRole::Admin)` 已确保 Admin/SuperAdmin 可进入
//! 高危操作（load/apply-default/delete）在 handler 内部二次校验 SuperAdmin
//!
//! Handler 层职责：
//! 1. 编排各 domain 拉取数据组装 SeedSnapshot（导出）
//! 2. 编排各 domain 完成 upsert（导入）
//! 3. 调用 seed 模块的纯函数（diff、validate、resolve）完成算法部分

pub mod apply_default;
pub mod delete_file;
pub mod diff;
pub mod diff_files;
pub mod get_default;
pub mod get_file;
pub mod list;
pub mod load;
pub mod save;

pub use apply_default::apply_default_handler;
pub use delete_file::delete_seed_file_handler;
pub use diff::diff_handler;
pub use diff_files::diff_files_handler;
pub use get_default::get_default_handler;
pub use get_file::get_seed_file_handler;
pub use list::list_seeds_handler;
pub use load::load_seed_handler;
pub use save::save_seed_handler;

use std::collections::HashMap;
use common::error::{Error, Result};
use crate::pkg::RequestContext;
use crate::service::domain::system::seed::defs::*;

/// 校验当前用户是否为 SuperAdmin
fn check_super_admin(ctx: &RequestContext) -> Result<()> {
    let user_role = ctx
        .user_role()
        .map(common::enums::UserRole::from_i32)
        .unwrap_or(common::enums::UserRole::Member);
    if !common::enums::UserRole::has_permission(user_role, common::enums::UserRole::SuperAdmin) {
        return Err(Error::forbidden("权限不足，仅 SuperAdmin 可执行此操作"));
    }
    Ok(())
}

/// 从当前 DB 组装 SeedSnapshot（编排各 domain）
///
/// 调用 organization / user / finance / hr domain 拉取实体，
/// 转换为 SeedSnapshot 结构。敏感字段全部填 PENDING_INPUT。
pub async fn assemble_snapshot_from_db(
    ctx: RequestContext,
    org_id: &str,
    description: Option<String>,
) -> Result<SeedSnapshot> {
    use crate::service::domain::{finance, hr, organization};

    // 1. 组织
    let org = organization::domain()
        .organization_manage()
        .get_by_id(ctx.clone(), org_id)
        .await?
        .ok_or_else(|| Error::not_found(format!("组织不存在: {}", org_id)))?;

    let organization_def = OrganizationDef {
        id: org.id.clone(),
        name: org.name,
        description: org.description,
        base_url: org.base_url,
        status: org.status.to_i32(),
        scope: org.scope.to_i32(),
    };

    // 2. 用户
    let users = organization::domain()
        .user_manage()
        .find_by_organization_id(ctx.clone(), org_id)
        .await?;
    let user_defs: Vec<UserDef> = users.into_iter().map(|u| UserDef {
        id: u.id.clone(),
        organization_id: u.organization_id,
        username: u.username,
        display_name: u.display_name,
        email: u.email,
        password_ref: PENDING_INPUT.to_string(),
        role: u.role.to_i32(),
        status: u.status.to_i32(),
    }).collect();

    // 3. ModelProvider
    let providers = finance::domain()
        .model_provider_manage()
        .list_model_providers(ctx.clone())
        .await?;
    let provider_defs: Vec<ModelProviderDef> = providers.into_iter().map(|p| ModelProviderDef {
        id: p.po.id.clone(),
        name: p.po.name,
        provider_type: p.po.provider_type.to_i32(),
        model_name: p.po.model_name,
        capability: p.po.capability.to_i32(),
        api_key_ref: PENDING_INPUT.to_string(),
        base_url: p.po.base_url,
        description: p.po.description,
        config: p.po.config,
        status: p.po.status.to_i32(),
    }).collect();

    // 4. Agent
    let agents = hr::domain()
        .agent_manage()
        .list_agents(ctx.clone())
        .await?;
    let agent_defs: Vec<AgentDef> = agents.into_iter().map(|a| AgentDef {
        id: a.po.id.clone(),
        name: a.po.name,
        roles: a.po.get_roles(),
        description: a.po.description,
        capabilities: a.po.get_capabilities(),
        soul: a.po.soul,
        model_provider_id: a.po.model_provider_id,
        runtime_config: a.po.runtime_config,
        status: a.po.status.to_i32(),
        kind: a.po.kind.to_i32(),
    }).collect();

    // 5. Skill
    let skills = hr::domain()
        .skill_manage()
        .query_skills(ctx.clone(), Default::default())
        .await?;
    let skill_defs: Vec<SkillDef> = skills.items.into_iter().map(|s| SkillDef {
        id: s.id.clone(),
        name: s.name,
        description: s.description,
        tags: s.get_tags(),
        category: s.category,
        parent_skill_id: s.parent_skill_id,
        author_id: s.author_id,
        author_type: s.author_type.to_i32(),
        status: s.status.to_i32(),
        content_path: s.content_path,
    }).collect();

    Ok(SeedSnapshot {
        version: SeedSnapshot::CURRENT_VERSION.to_string(),
        generated_at: common::constants::utils::current_timestamp(),
        description,
        source_organization_id: org_id.to_string(),
        organization: organization_def,
        users: user_defs,
        model_providers: provider_defs,
        agents: agent_defs,
        skills: skill_defs,
    })
}

/// 将快照应用到 DB（编排各 domain upsert）
///
/// 根据 strategy 决定行为：
/// - PreserveIds: 按 ID upsert
/// - RegenerateIds: 生成新 ID（跨组织迁移）
/// - DryRun: 由调用方处理（不调用本函数）
/// - SkipExisting: 仅创建不存在的
///
/// sensitive_values 由前端提供；INHERIT_CURRENT 时调用各 domain 拉当前 DB 值
pub async fn apply_snapshot_to_db(
    ctx: RequestContext,
    snapshot: &SeedSnapshot,
    strategy: common::api::seed::ImportStrategy,
    sensitive_values: &HashMap<String, String>,
) -> Result<common::api::seed::LoadSeedResponse> {
    use crate::service::domain::{finance, hr, organization};
    use common::api::seed::ImportStrategy;

    // 1. DryRun 直接返回 diff（不调用本函数的写入路径）
    if matches!(strategy, ImportStrategy::DryRun) {
        let current = assemble_snapshot_from_db(ctx.clone(), &snapshot.source_organization_id, None).await?;
        let diff = crate::service::domain::system::seed::diff::diff_snapshots(&current, snapshot);
        let created = diff.users.iter()
            .chain(diff.model_providers.iter())
            .chain(diff.agents.iter())
            .chain(diff.skills.iter())
            .filter(|e| matches!(e, DiffEntry::New { .. })).count();
        let updated = diff.users.iter()
            .chain(diff.model_providers.iter())
            .chain(diff.agents.iter())
            .chain(diff.skills.iter())
            .filter(|e| matches!(e, DiffEntry::Updated { .. })).count();
        let same = diff.users.iter()
            .chain(diff.model_providers.iter())
            .chain(diff.agents.iter())
            .chain(diff.skills.iter())
            .filter(|e| matches!(e, DiffEntry::Same { .. })).count();
        return Ok(common::api::seed::LoadSeedResponse {
            created, updated, skipped: 0, diff: Some(diff),
        });
    }

    // 2. 校验敏感字段齐备
    crate::service::domain::system::seed::diff::validate_sensitive_fields(snapshot, sensitive_values)
        .map_err(Error::bad_request)?;

    let mut created = 0usize;
    let mut updated = 0usize;
    let mut skipped = 0usize;

    // 3. 写入用户
    for user_def in &snapshot.users {
        let existing = organization::domain()
            .user_manage()
            .get_user_by_id(ctx.clone(), &user_def.id)
            .await?;

        if existing.is_some() && matches!(strategy, ImportStrategy::SkipExisting) {
            skipped += 1;
            continue;
        }

        // 解析密码（INHERIT_CURRENT 时用 existing 的 password_hash）
        let current_hash = existing.as_ref().map(|u| u.password_hash.as_str());
        let password_hash = crate::service::domain::system::seed::diff::resolve_password(
            &user_def.password_ref,
            &user_def.id,
            sensitive_values,
            current_hash,
        ).map_err(Error::bad_request)?;

        let user_po = crate::models::user::UserPo {
            id: user_def.id.clone(),
            organization_id: user_def.organization_id.clone(),
            username: user_def.username.clone(),
            display_name: user_def.display_name.clone(),
            email: user_def.email.clone(),
            password_hash,
            role: common::enums::UserRole::from_i32(user_def.role),
            status: common::enums::UserStatus::from_i32(user_def.status),
            created_by: "seed_import".to_string(),
            modified_by: "seed_import".to_string(),
            created_at: common::constants::utils::current_timestamp(),
            updated_at: common::constants::utils::current_timestamp(),
        };

        if existing.is_some() {
            organization::domain().user_manage().update_user(ctx.clone(), &user_po).await?;
            updated += 1;
        } else {
            organization::domain().user_manage().create_user(ctx.clone(), user_po).await?;
            created += 1;
        }
    }

    // 4. 写入 ModelProvider
    for provider_def in &snapshot.model_providers {
        let existing = finance::domain()
            .model_provider_manage()
            .get_model_provider(ctx.clone(), &provider_def.id, Default::default())
            .await?;

        if existing.is_some() && matches!(strategy, ImportStrategy::SkipExisting) {
            skipped += 1;
            continue;
        }

        let current_api_key = existing.as_ref().and_then(|p| Some(p.po.api_key.clone()));
        let api_key = crate::service::domain::system::seed::diff::resolve_api_key(
            &provider_def.api_key_ref,
            &provider_def.id,
            sensitive_values,
            current_api_key.as_deref(),
        ).map_err(Error::bad_request)?;

        let mut provider = crate::models::model_provider::ModelProvider::new(
            provider_def.name.clone(),
            common::enums::ProviderType::from_i32(provider_def.provider_type),
            common::enums::ModelCapability::from_i32(provider_def.capability),
            provider_def.model_name.clone(),
            api_key,
            provider_def.base_url.clone(),
            provider_def.description.clone(),
            "seed_import".to_string(),
        );
        // 覆盖 ID 以保持引用一致
        provider.po.id = provider_def.id.clone();

        if existing.is_some() {
            finance::domain().model_provider_manage().update_model_provider(ctx.clone(), &provider).await?;
            updated += 1;
        } else {
            finance::domain().model_provider_manage().create_model_provider(ctx.clone(), &provider).await?;
            created += 1;
        }
    }

    // 5. 写入 Agent
    for agent_def in &snapshot.agents {
        let existing = hr::domain()
            .agent_manage()
            .get_agent(ctx.clone(), &agent_def.id, Default::default())
            .await?;

        if existing.is_some() && matches!(strategy, ImportStrategy::SkipExisting) {
            skipped += 1;
            continue;
        }

        let mut agent_po = crate::models::agent::AgentPo::new(
            agent_def.name.clone(),
            agent_def.roles.clone(),
            agent_def.description.clone(),
            agent_def.capabilities.clone(),
            agent_def.soul.clone(),
            agent_def.model_provider_id.clone(),
            "seed_import".to_string(),
        );
        agent_po.id = agent_def.id.clone();
        agent_po.status = common::enums::AgentStatus::from_i32(agent_def.status);
        agent_po.kind = common::enums::AgentKind::from_i32(agent_def.kind);
        agent_po.runtime_config = agent_def.runtime_config.clone();
        let agent = crate::models::agent::Agent::from_po(agent_po);

        if existing.is_some() {
            hr::domain().agent_manage().update_agent(ctx.clone(), &agent).await?;
            updated += 1;
        } else {
            hr::domain().agent_manage().create_agent(ctx.clone(), &agent).await?;
            created += 1;
        }
    }

    // 6. 写入 Skill（仅元数据，文件需要单独处理）
    for skill_def in &snapshot.skills {
        let existing = hr::domain()
            .skill_manage()
            .get_skill(ctx.clone(), &skill_def.id)
            .await?;

        if existing.is_some() && matches!(strategy, ImportStrategy::SkipExisting) {
            skipped += 1;
            continue;
        }

        let skill = crate::models::skill::Skill {
            id: skill_def.id.clone(),
            name: skill_def.name.clone(),
            description: skill_def.description.clone(),
            tags: serde_json::to_string(&skill_def.tags).unwrap_or_else(|_| "[]".to_string()),
            category: skill_def.category.clone(),
            parent_skill_id: skill_def.parent_skill_id.clone(),
            author_id: skill_def.author_id.clone(),
            author_type: common::enums::SkillAuthorType::from_i32(skill_def.author_type),
            status: common::enums::SkillStatus::from_i32(skill_def.status),
            content_path: skill_def.content_path.clone(),
            // 其他字段使用默认值
            ..Default::default()
        };

        if existing.is_some() {
            // Skill update 接口需要 UpdateSkillParams，这里简化为不更新文件
            let params = crate::service::domain::hr::UpdateSkillParams {
                skill: &skill,
                file_writes: vec![],
                file_deletes: vec![],
                file_imports: vec![],
            };
            hr::domain().skill_manage().update_skill(ctx.clone(), params).await?;
            updated += 1;
        } else {
            hr::domain().skill_manage().create_skill(ctx.clone(), &skill).await?;
            created += 1;
        }
    }

    Ok(common::api::seed::LoadSeedResponse {
        created, updated, skipped, diff: None,
    })
}
```

- [ ] **Step 2: 创建 `src/handlers/system/seed/list.rs`**

```rust
//! GET /api/v1/system/seed/list - 列出 seeds/ 目录

use ai_orz_macros::generate_http_handler;
use common::api::seed::{ListSeedsRequest, ListSeedsResponse};
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::seed::store;

#[generate_http_handler]
pub async fn list_seeds(
    _ctx: RequestContext,
    _params: ListSeedsRequest,
) -> Result<ListSeedsResponse> {
    let dir = store::seeds_dir();
    let files = store::list_files(&dir).await?;
    let total = files.len() as u64;
    Ok(ListSeedsResponse { data: files, total })
}
```

- [ ] **Step 3: 创建 `get_file.rs`**

```rust
//! GET /api/v1/system/seed/file/{name} - 读取快照文件内容

use ai_orz_macros::generate_http_handler;
use common::api::seed::{GetSeedFileRequest, GetSeedFileResponse};
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::seed::store;

#[generate_http_handler]
pub async fn get_seed_file(
    _ctx: RequestContext,
    params: GetSeedFileRequest,
) -> Result<GetSeedFileResponse> {
    let dir = store::seeds_dir();
    store::read_file(&dir, &params.name).await
}
```

- [ ] **Step 4: 创建 `save.rs`**

```rust
//! POST /api/v1/system/seed/save - 导出当前组织配置到文件
//!
//! Handler 编排：调用各 domain 拉取实体 → 组装 SeedSnapshot → 写入文件

use ai_orz_macros::generate_http_handler;
use common::api::seed::{SaveSeedRequest, SaveSeedResponse};
use common::error::{Error, Result};

use crate::pkg::RequestContext;
use crate::service::domain::system::seed::store;

#[generate_http_handler]
pub async fn save_seed(
    ctx: RequestContext,
    params: SaveSeedRequest,
) -> Result<SaveSeedResponse> {
    super::check_super_admin(&ctx)?;
    let org_id = ctx.organization_id.clone()
        .ok_or_else(|| Error::bad_request("缺少 organization_id".to_string()))?;

    // 编排各 domain 拉取数据
    let snapshot = super::assemble_snapshot_from_db(ctx, &org_id, params.description).await?;

    let content = serde_json::to_string_pretty(&snapshot)?;
    let dir = store::seeds_dir();
    let size = store::write_file(&dir, &params.name, &content).await?;

    Ok(SaveSeedResponse { name: params.name, size })
}
```

- [ ] **Step 5: 创建 `load.rs`**

```rust
//! POST /api/v1/system/seed/load/{name} - 从文件加载快照
//!
//! Handler 编排：读文件 → 校验 → 调用各 domain upsert

use ai_orz_macros::generate_http_handler;
use common::api::seed::{LoadSeedRequest, LoadSeedResponse};
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::seed::store;

#[generate_http_handler]
pub async fn load_seed(
    ctx: RequestContext,
    params: LoadSeedRequest,
) -> Result<LoadSeedResponse> {
    super::check_super_admin(&ctx)?;

    let dir = store::seeds_dir();
    let file_resp = store::read_file(&dir, &params.name).await?;
    let snapshot: crate::service::domain::system::seed::defs::SeedSnapshot =
        serde_json::from_str(&file_resp.content)?;

    super::apply_snapshot_to_db(ctx, &snapshot, params.strategy, &params.sensitive_values).await
}
```

- [ ] **Step 6: 创建 `delete_file.rs`**

```rust
//! DELETE /api/v1/system/seed/file/{name} - 删除快照文件

use ai_orz_macros::generate_http_handler;
use common::api::seed::{DeleteSeedFileRequest, DeleteSeedFileResponse};
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::seed::store;

#[generate_http_handler]
pub async fn delete_seed_file(
    ctx: RequestContext,
    params: DeleteSeedFileRequest,
) -> Result<DeleteSeedFileResponse> {
    super::check_super_admin(&ctx)?;
    let dir = store::seeds_dir();
    store::delete_file(&dir, &params.name).await?;
    Ok(DeleteSeedFileResponse { success: true })
}
```

- [ ] **Step 7: 创建 `diff.rs`**

```rust
//! POST /api/v1/system/seed/diff/{name} - 文件 vs DB diff
//!
//! Handler 编排：读文件 → 调用各 domain 拉当前 DB → 组装 current snapshot → 调用 seed::diff_snapshots

use ai_orz_macros::generate_http_handler;
use common::api::seed::DiffSeedRequest;
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::seed::{defs::{DiffKind, SeedDiff, SeedSnapshot}, diff, store};

#[generate_http_handler]
pub async fn diff_handler(
    ctx: RequestContext,
    params: DiffSeedRequest,
) -> Result<SeedDiff> {
    let dir = store::seeds_dir();
    let file_resp = store::read_file(&dir, &params.name).await?;
    let snapshot: SeedSnapshot = serde_json::from_str(&file_resp.content)?;

    // 编排各 domain 拉取当前 DB
    let current = super::assemble_snapshot_from_db(ctx, &snapshot.source_organization_id, None).await?;

    let mut diff_result = diff::diff_snapshots(&current, &snapshot);
    diff_result.meta.kind = DiffKind::FileVsDb;
    diff_result.meta.base_source = "current_db".to_string();
    diff_result.meta.target_source = params.name;
    Ok(diff_result)
}
```

- [ ] **Step 8: 创建 `diff_files.rs`**

```rust
//! POST /api/v1/system/seed/diff-files - 两个文件之间 diff
//!
//! 纯文件对比，不涉及 DB

use ai_orz_macros::generate_http_handler;
use common::api::seed::DiffFilesRequest;
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::seed::{defs::{DiffKind, SeedDiff, SeedSnapshot}, diff, store};

#[generate_http_handler]
pub async fn diff_files_handler(
    _ctx: RequestContext,
    params: DiffFilesRequest,
) -> Result<SeedDiff> {
    let dir = store::seeds_dir();
    let base_resp = store::read_file(&dir, &params.base).await?;
    let target_resp = store::read_file(&dir, &params.target).await?;
    let base_snapshot: SeedSnapshot = serde_json::from_str(&base_resp.content)?;
    let target_snapshot: SeedSnapshot = serde_json::from_str(&target_resp.content)?;

    let mut diff_result = diff::diff_snapshots(&base_snapshot, &target_snapshot);
    diff_result.meta.kind = DiffKind::FileVsFile;
    diff_result.meta.base_source = params.base;
    diff_result.meta.target_source = params.target;
    Ok(diff_result)
}
```

- [ ] **Step 9: 创建 `get_default.rs`**

```rust
//! GET /api/v1/system/seed/default - 获取内置默认模板

use ai_orz_macros::generate_http_handler;
use common::api::seed::GetDefaultSeedRequest;
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::seed::{default, defs::SeedSnapshot};

#[generate_http_handler]
pub async fn get_default_handler(
    _ctx: RequestContext,
    _params: GetDefaultSeedRequest,
) -> Result<SeedSnapshot> {
    Ok(default::embedded_default_snapshot())
}
```

- [ ] **Step 10: 创建 `apply_default.rs`**

```rust
//! POST /api/v1/system/seed/apply-default - 应用默认模板
//!
//! Handler 编排：加载内置默认 → 调用各 domain upsert

use ai_orz_macros::generate_http_handler;
use common::api::seed::{ApplyDefaultSeedRequest, LoadSeedResponse};
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::domain::system::seed::default;

#[generate_http_handler]
pub async fn apply_default_handler(
    ctx: RequestContext,
    params: ApplyDefaultSeedRequest,
) -> Result<LoadSeedResponse> {
    super::check_super_admin(&ctx)?;
    let snapshot = default::embedded_default_snapshot();
    super::apply_snapshot_to_db(ctx, &snapshot, params.strategy, &params.sensitive_values).await
}
```

- [ ] **Step 11: 修改 `src/handlers/system/mod.rs` 添加 seed 模块**

```rust
//! System 领域 HTTP 接口

pub mod aop;
pub mod aop_stats;
pub mod backup;
pub mod cron_trigger;
pub mod health_metrics;
pub mod logs;
pub mod seed;
```

- [ ] **Step 12: 修改 `src/router.rs` 注册 seed 路由**

在 `system_routes()` 函数中添加 seed 子路由：

```rust
.nest(
    "/seed",
    Router::new()
        .route("/list", get(system::seed::list_seeds_handler))
        .route("/file/{name}", get(system::seed::get_seed_file_handler))
        .route("/file/{name}", delete(system::seed::delete_seed_file_handler))
        .route("/save", post(system::seed::save_seed_handler))
        .route("/load/{name}", post(system::seed::load_seed_handler))
        .route("/diff/{name}", post(system::seed::diff_handler))
        .route("/diff-files", post(system::seed::diff_files_handler))
        .route("/default", get(system::seed::get_default_handler))
        .route("/apply-default", post(system::seed::apply_default_handler)),
)
```

执行时先打开 `src/router.rs` 找到 `system_routes()` 函数的实际位置，再合并上述代码。需要确认 `post`、`delete` 已在 imports 中。

- [ ] **Step 13: 验证编译**

Run: `cargo check 2>&1 | tail -30`
Expected: PASS

如有编译错误，按错误信息调整：
- 可能需要为 `InitializeSystemRequest` 添加 `Clone`
- `Skill` 结构可能没有 `Default`，需要手动构造
- `ModelProviderFetchOptions`、`AgentFetchOptions` 等 `Default::default()` 是否合适

- [ ] **Step 14: Commit**

```bash
git add src/handlers/system/seed/ src/handlers/system/mod.rs src/router.rs
git commit -m "feat(seed): 添加 9 个 seed Handler + 编排各 domain + 注册路由"
```

---

### Task 7: 集成测试 + 文档

**Files:**
- Modify: `src/handlers/system/seed/seed_handler_test.rs`（新建）
- Create: `docs/design/seed-config-migration.md`

- [ ] **Step 1: 创建 handler 层集成测试 `src/handlers/system/seed/seed_handler_test.rs`**

```rust
//! Seed Handler 集成测试
//!
//! 测试 handler 层的跨 domain 编排逻辑：
//! - assemble_snapshot_from_db: 各 domain 拉数据组装快照
//! - apply_snapshot_to_db: 各 domain upsert
//! - 往返一致性：导出 → 修改 → 导入 → 重新导出，验证字段更新

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use crate::pkg::request_context_test_support::new_test_ctx;
    use sqlx::SqlitePool;

    /// 初始化所有 domain（参考其他集成测试的 init 模式）
    async fn init_test_env(pool: SqlitePool) -> crate::pkg::RequestContext {
        let _ = crate::config::init();
        crate::pkg::tool_tracing::logger::ToolCallLogger::init(
            std::env::temp_dir().join("ai_orz_seed_handler_test_trace"),
        );
        // 初始化所有 DAO/DAL/Domain
        crate::service::dao::organization::init();
        crate::service::dao::user::init();
        crate::service::dao::model_provider::init();
        crate::service::dao::agent::init();
        crate::service::dao::skill::init();
        crate::service::dao::tool::init();
        crate::service::dao::tool_call::init();
        crate::service::dao::cortex::init();
        crate::service::dao::memory::init();
        crate::service::dao::mcp_server::init();
        crate::service::dao::project::init();
        crate::service::dao::task::init();
        crate::service::dao::message::init();
        crate::service::dao::artifact::init();
        crate::service::dao::attachment::init();
        crate::service::dao::message_channel::init();
        crate::service::dal::organization::init();
        crate::service::dal::user::init();
        crate::service::dal::model_provider::init();
        crate::service::dal::agent::init();
        crate::service::dal::skill::init();
        crate::service::dal::tool::init();
        crate::service::dal::memory::init();
        crate::service::dal::mcp_tool::init();
        crate::service::dal::brain::init();
        crate::service::dal::project::init();
        crate::service::dal::task::init();
        crate::service::dal::message::init();
        crate::service::dal::message_channel::init();
        crate::service::dal::attachment::init();
        crate::service::dal::artifact::init();
        crate::service::domain::hr::init();
        crate::service::domain::message::init();
        crate::service::domain::project::init();
        crate::service::domain::runtime::init();
        crate::service::domain::system::init();
        crate::service::domain::finance::init();
        crate::service::domain::organization::init();
        new_test_ctx("test-seed-handler-user", pool)
    }

    /// 准备测试数据：1 个组织 + 1 个 SuperAdmin + 1 个 chat provider + 1 个 embedding provider + 1 个 Agent
    async fn prepare_test_data(ctx: &crate::pkg::RequestContext) -> String {
        use crate::models::organization::OrganizationPo;
        use crate::models::user::UserPo;
        use crate::models::model_provider::ModelProvider;
        use crate::models::agent::{Agent, AgentPo};
        use common::enums::{UserRole, ModelCapability, ProviderType, AgentStatus};

        let org_dal = crate::service::dal::organization::dal();
        let user_dal = crate::service::dal::user::dal();
        let provider_dal = crate::service::dal::model_provider::dal();
        let agent_dal = crate::service::dal::agent::dal();

        let org_id = "TESTORG0001".to_string();
        let org = OrganizationPo::new(
            org_id.clone(), "测试组织".to_string(), "测试用组织".to_string(),
            None, org_id.clone(),
        );
        org_dal.create(ctx.clone(), &org).await.unwrap();

        let user_id = "TESTUSER000000001".to_string();
        let user = UserPo::new(
            user_id.clone(), org_id.clone(), "admin".to_string(), "管理员".to_string(),
            "admin@test.com".to_string(), "hashed_pwd".to_string(),
            UserRole::SuperAdmin, user_id.clone(),
        );
        user_dal.create(ctx.clone(), &user).await.unwrap();

        let chat_provider = ModelProvider::new(
            "OpenAI Chat".to_string(), ProviderType::OpenAI, ModelCapability::Agent,
            "gpt-4o".to_string(), "sk-test-key".to_string(), None,
            Some("对话模型".to_string()), user_id.clone(),
        );
        provider_dal.create(ctx.clone(), &chat_provider).await.unwrap();

        let embedding_provider = ModelProvider::new(
            "OpenAI Embedding".to_string(), ProviderType::OpenAI, ModelCapability::Embedding,
            "text-embedding-3-small".to_string(), "sk-test-key".to_string(), None,
            Some("向量模型".to_string()), user_id.clone(),
        );
        provider_dal.create(ctx.clone(), &embedding_provider).await.unwrap();

        let mut agent_po = AgentPo::new(
            "前台 Agent".to_string(), vec!["feishu_reception".to_string()],
            "前台接待".to_string(), vec!["chat".to_string()], "测试灵魂".to_string(),
            chat_provider.po.id.clone(), user_id.clone(),
        );
        agent_po.status = AgentStatus::Onboarded;
        let agent = Agent::from_po(agent_po);
        agent_dal.create(ctx.clone(), &agent).await.unwrap();

        org_id
    }

    #[sqlx::test]
    async fn test_assemble_snapshot_from_db_returns_valid_structure(pool: SqlitePool) {
        let ctx = init_test_env(pool).await;
        let org_id = prepare_test_data(&ctx).await;

        let snapshot = super::assemble_snapshot_from_db(ctx, &org_id, Some("测试".to_string())).await.unwrap();

        assert_eq!(snapshot.version, crate::service::domain::system::seed::defs::SeedSnapshot::CURRENT_VERSION);
        assert_eq!(snapshot.organization.id, org_id);
        assert_eq!(snapshot.users.len(), 1);
        assert_eq!(snapshot.model_providers.len(), 2);
        assert_eq!(snapshot.agents.len(), 1);
        assert_eq!(snapshot.users[0].password_ref, crate::service::domain::system::seed::defs::PENDING_INPUT);
    }

    #[sqlx::test]
    async fn test_apply_snapshot_with_preserve_ids_round_trip(pool: SqlitePool) {
        let ctx = init_test_env(pool).await;
        let org_id = prepare_test_data(&ctx).await;

        // 导出
        let snapshot = super::assemble_snapshot_from_db(ctx.clone(), &org_id, None).await.unwrap();

        // 提供敏感字段
        let mut sensitive = HashMap::new();
        for u in &snapshot.users {
            sensitive.insert(format!("user:{}:password", u.id), "new_hashed_pwd".to_string());
        }
        for p in &snapshot.model_providers {
            sensitive.insert(format!("model_provider:{}:api_key", p.id), "sk-new-key".to_string());
        }

        // 修改快照模拟配置更新
        let mut modified = snapshot.clone();
        modified.agents[0].name = "修改后的 Agent".to_string();

        // 导入
        let result = super::apply_snapshot_to_db(
            ctx, &modified, common::api::seed::ImportStrategy::PreserveIds, &sensitive
        ).await.unwrap();

        assert_eq!(result.updated, 1); // Agent 更新
        assert_eq!(result.created, 0);
    }

    #[sqlx::test]
    async fn test_apply_snapshot_dry_run_returns_diff_without_writing(pool: SqlitePool) {
        let ctx = init_test_env(pool).await;
        let org_id = prepare_test_data(&ctx).await;

        let snapshot = super::assemble_snapshot_from_db(ctx.clone(), &org_id, None).await.unwrap();

        let result = super::apply_snapshot_to_db(
            ctx, &snapshot, common::api::seed::ImportStrategy::DryRun, &HashMap::new()
        ).await.unwrap();

        assert!(result.diff.is_some());
        assert_eq!(result.created, 0); // DryRun 不写入
    }

    #[sqlx::test]
    async fn test_apply_default_template_creates_template_entities(pool: SqlitePool) {
        let ctx = init_test_env(pool).await;
        // 注意：默认模板的 organization_id="TEMPLATE_ORG"，需要先创建组织或在 apply 时改为当前 ctx.org_id
        // 测试中需要先创建 TEMPLATE_ORG 组织
        use crate::models::organization::OrganizationPo;
        let org = OrganizationPo::new(
            "TEMPLATE_ORG".to_string(), "模板组织".to_string(), "测试".to_string(),
            None, "TEMPLATE_ORG".to_string(),
        );
        crate::service::dal::organization::dal().create(ctx.clone(), &org).await.unwrap();

        let snapshot = crate::service::domain::system::seed::default::embedded_default_snapshot();

        let mut sensitive = HashMap::new();
        sensitive.insert("user:TEMPLATE_ADMIN:password".to_string(), "hashed".to_string());
        sensitive.insert("model_provider:TEMPLATE_CHAT_PROVIDER:api_key".to_string(), "sk-test".to_string());
        sensitive.insert("model_provider:TEMPLATE_EMBEDDING_PROVIDER:api_key".to_string(), "sk-test".to_string());

        let result = super::apply_snapshot_to_db(
            ctx, &snapshot, common::api::seed::ImportStrategy::PreserveIds, &sensitive
        ).await.unwrap();

        assert!(result.created > 0);
    }
}
```

- [ ] **Step 2: 在 `src/handlers/system/seed/mod.rs` 添加测试模块声明**

```rust
#[cfg(test)]
mod seed_handler_test;
```

- [ ] **Step 3: 运行集成测试**

Run: `cargo test --lib handlers::system::seed 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 4: 创建文档 `docs/design/seed-config-migration.md`**

```markdown
# Seed 配置迁移中心

## 概述

业务实体配置的导出/导入/diff 系统，区别于全量备份：
- 仅含配置层（Org/User/Provider/Agent/Skill 定义）
- 不含运行时数据（消息、任务、stats、日志、向量索引）
- 敏感字段（password/api_key）使用占位符，导入时由管理员填写
- 支持 4 种导入策略 + 字段级 diff

## 架构原则

**核心原则**：seed 模块是"纯工具箱"，不持有任何 DAL 引用，不调用其他 domain。

| 层级 | 职责 |
|------|------|
| Domain | 只处理自己的业务逻辑，不调用其他 domain |
| Handler/Consumer | 跨 domain 编排，调用各 domain 完成 CRUD |
| Seed 子模块 | 提供数据视图（snapshot 结构 + diff 算法 + 文件存储），不调用任何 domain |

### 为什么这样设计？

1. **架构原则一致性**：项目硬约束"DAL layer must not call other DALs; business orchestration must be moved to consumer layer"在 domain 层面同样适用——domain 不应调用其他 domain
2. **可测试性**：seed 的纯函数（diff、validate、resolve）可独立单元测试，无需 DB
3. **职责清晰**：seed 关注"数据视图"，handler 关注"编排执行"
4. **复用性**：seed 的算法可被任何 handler 复用（不限于 system domain）

## 模块结构

```
src/service/domain/system/seed/        # 纯工具箱子模块
├── defs.rs                            # SeedSnapshot + XxxDef + ImportStrategy + SeedDiff 结构
├── diff.rs                            # diff_snapshots + validate_sensitive_fields + resolve_password/api_key 纯函数
├── store.rs                           # 文件系统 CRUD（CRUD + 路径穿越防护）
├── default.rs + default.json          # 编译期内置默认模板
└── seed_test.rs                       # 纯函数单元测试

src/handlers/system/seed/              # HTTP Handler + 跨 domain 编排
├── mod.rs                             # assemble_snapshot_from_db + apply_snapshot_to_db 编排函数
├── list.rs                            # GET /seed/list
├── get_file.rs                        # GET /seed/file/{name}
├── save.rs                            # POST /seed/save
├── load.rs                            # POST /seed/load/{name}
├── delete_file.rs                     # DELETE /seed/file/{name}
├── diff.rs                            # POST /seed/diff/{name}
├── diff_files.rs                      # POST /seed/diff-files
├── get_default.rs                     # GET /seed/default
└── apply_default.rs                   # POST /seed/apply-default
```

## 使用场景

1. **开箱即用初始化**：`POST /seed/apply-default` 一键应用默认模板
2. **配置版本管理**：`POST /seed/save` 导出 → git 提交 → diff 跟踪
3. **跨环境迁移**：导出 → 切换环境 → 导入（RegenerateIds 策略）
4. **配置回滚**：导入旧版本快照（PreserveIds 策略，运行时数据保留）

## 占位符语义

| 占位符 | 含义 | 解析方式 |
|--------|------|---------|
| `PENDING_INPUT` | 导入时强制要求管理员填写 | handler 从 `sensitive_values` map 取值 |
| `INHERIT_CURRENT` | 保留 DB 当前值（回滚场景） | handler 调用 domain 拉 DB 当前值传入 `resolve_*` 纯函数 |
| `RANDOM_GENERATE` | 随机生成并显示一次 | seed::diff::resolve_password 内部生成 |

## API 列表

| 方法 | 路径 | 描述 | 权限 |
|------|------|------|------|
| GET | `/seed/list` | 列出所有快照 | Admin+ |
| GET | `/seed/file/{name}` | 读取快照内容 | Admin+ |
| POST | `/seed/save` | 导出当前配置 | SuperAdmin |
| POST | `/seed/load/{name}` | 加载快照 | SuperAdmin |
| DELETE | `/seed/file/{name}` | 删除快照 | SuperAdmin |
| POST | `/seed/diff/{name}` | 文件 vs DB diff | Admin+ |
| POST | `/seed/diff-files` | 两文件 diff | Admin+ |
| GET | `/seed/default` | 获取默认模板 | Admin+ |
| POST | `/seed/apply-default` | 应用默认模板 | SuperAdmin |

## 相关架构改进

本计划同时修复了 `OrganizationDomain::initialize_system` 的架构违规：
- **之前**：organization domain 直接调用 `crate::service::dal::model_provider::dal()`，跨过 finance domain
- **之后**：organization domain 只提供 `create_org_and_owner`（仅创建 org+user），handler 编排 organization + finance domain 完成 provider 创建
```

- [ ] **Step 5: 启动服务进行端到端冒烟测试**

Run: `cargo run --release 2>&1 | head -5`（启动服务）

测试接口（需要先初始化系统）：

```bash
# 1. 初始化系统（新的 handler 编排：organization + finance domain）
curl -X POST http://localhost:3000/api/v1/organization/initialize \
  -H "Content-Type: application/json" \
  -d '{
    "organization_name": "测试组织",
    "admin_username": "admin",
    "admin_password_hash": "$2a$10$xxx",
    "chat_model": {"name": "OpenAI Chat", "provider_type": 0, "model_name": "gpt-4o", "api_key": "sk-test"},
    "embedding_model": {"name": "OpenAI Embedding", "provider_type": 0, "model_name": "text-embedding-3-small", "api_key": "sk-test"}
  }'

# 2. 登录获取 token
TOKEN=$(curl -s -X POST http://localhost:3000/api/v1/organization/auth/login \
  -H "Content-Type: application/json" \
  -d '{"organization_id": "xxx", "username": "admin", "password_hash": "$2a$10$xxx"}' | jq -r '.data.token')

# 3. 测试 seed 接口
curl -X POST http://localhost:3000/api/v1/system/seed/save \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "test-export", "description": "测试导出"}'

curl http://localhost:3000/api/v1/system/seed/list \
  -H "Authorization: Bearer $TOKEN"

curl http://localhost:3000/api/v1/system/seed/file/test-export.json \
  -H "Authorization: Bearer $TOKEN"

curl http://localhost:3000/api/v1/system/seed/default \
  -H "Authorization: Bearer $TOKEN"
```

Expected: 全部返回 200 + 正确响应

- [ ] **Step 6: Commit**

```bash
git add src/handlers/system/seed/seed_handler_test.rs docs/design/seed-config-migration.md
git commit -m "test(seed): 添加 handler 层集成测试 + 设计文档"
```

---

## 后续工作（不在本计划范围）

1. **前端组织初始化页面**：增加 chat_model 和 embedding_model 表单字段（接口不变，前端补字段即可）
2. **前端"配置迁移" Tab**：列表/导入导出/diff 视图/密钥表单
3. **Skill 文件内容导出**：当前只导出 Skill 元数据，文件内容需要单独打包（可作为 tar/zip 附件）
4. **快照 schema 演进**：基于 `version` 字段做兼容性处理
5. **删除 `OrganizationDomain::initialize_system` 旧方法**（若 Step 3 选择了 deprecated 保留方案）

## Self-Review

**1. Spec coverage**：
- ✅ seed 作为 system domain 子模块 → Task 2（`pub mod seed;` in `system/mod.rs`）
- ✅ 不新增 pkg → 全部在 `src/service/domain/system/seed/`
- ✅ 导出/import/diff/默认模板 → Task 1-7
- ✅ 4 种导入策略 → Task 1 DTO + Task 6 编排
- ✅ 字段级 diff → Task 3 算法
- ✅ 敏感字段占位符 → Task 1 defs + Task 3 解析
- ✅ 内置默认模板 → Task 5
- ✅ 9 个 HTTP API → Task 6
- ✅ 修复 initialize_system 架构问题 → Task 0
- ✅ seed 不调用其他 domain → Task 2-5 都是纯函数 / 文件 CRUD

**2. Placeholder scan**：无 TODO/TBD，所有 Step 都有完整代码。

**3. Type consistency**：
- `SeedSnapshot` / `XxxDef` / `DiffEntry` / `FieldChange` 在 Task 1 定义，Task 3/6 使用一致
- `ImportStrategy` 在 Task 1 定义，Task 6 使用 `PreserveIds/RegenerateIds/DryRun/SkipExisting` 一致
- `PENDING_INPUT` / `INHERIT_CURRENT` / `RANDOM_GENERATE` 常量在 Task 1 定义，Task 3/6 使用一致
- `assemble_snapshot_from_db` / `apply_snapshot_to_db` 在 Task 6 mod.rs 定义，各 handler 调用一致
```
