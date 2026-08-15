# 系统管理工具（备份 + 日志查询 + 角色权限）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 构建系统管理工具箱：数据备份与恢复、日志查询、基于角色并查集的权限中间件

**Architecture:** 
- 权限中间件：基于并查集的角色继承体系，`require_role` 中间件指定最低角色要求，用户角色在最低角色的祖先链上即可访问
- 备份功能：System Domain 封装，tar.gz 压缩整个数据目录，排除 backups/logs，备份目录维护 `_index.json` 总索引文件，版本号自动递增，恢复返回可执行脚本
- 日志查询：直接读取 JSONL 日志文件，内存过滤，支持关键词/log_id/级别/时间范围

**Tech Stack:** Rust + Axum + tar + flate2 + chrono + serde_json

---

## 文件结构总览

| 操作 | 文件 | 职责 |
|------|------|------|
| 修改 | `common/src/enums/user.rs` | UserRole 新增 parent()、has_permission() 方法（并查集角色继承） |
| 新建 | `src/middleware/require_role.rs` | 角色权限中间件实现 |
| 修改 | `src/middleware/mod.rs` | 导出 require_role 中间件 |
| 修改 | `src/pkg/request_context.rs` | 新增 `user_role` 字段 + builder 方法 + from_headers 提取 |
| 修改 | `src/middleware/jwt_auth.rs` | JWT 解码后注入 user_role 到请求头 |
| 修改 | `src/pkg/jwt.rs` | JWT claims 增加 role 字段 |
| 修改 | `common/src/constants/http_header.rs` | 新增 USER_ROLE header 常量 |
| 新建 | `src/service/dal/backup.rs` | 备份 DAL：压缩/列表/删除/恢复脚本 + _index.json 索引 |
| 修改 | `src/service/dal/mod.rs` | 导出 backup DAL |
| 修改 | `src/service/domain/system/mod.rs` | 新增 BackupManager trait + 实现 |
| 新建 | `src/handlers/system/backup/create_backup.rs` | 创建备份 Handler |
| 新建 | `src/handlers/system/backup/list_backups.rs` | 列出备份 Handler |
| 新建 | `src/handlers/system/backup/delete_backup.rs` | 删除备份 Handler |
| 新建 | `src/handlers/system/backup/restore_backup.rs` | 获取恢复脚本 Handler |
| 新建 | `src/handlers/system/backup/mod.rs` | backup 模块导出 |
| 新建 | `src/service/dal/log_query.rs` | 日志查询 DAL：读文件 + 解析 + 过滤 + 分页 |
| 修改 | `src/service/dal/mod.rs` | 导出 log_query DAL |
| 修改 | `src/service/domain/system/mod.rs` | 新增 LogQuery trait + 实现 |
| 新建 | `src/handlers/system/logs/query_logs.rs` | 日志查询 Handler |
| 新建 | `src/handlers/system/logs/mod.rs` | logs 模块导出 |
| 修改 | `src/handlers/system/mod.rs` | 整合 system handlers |
| 修改 | `src/router.rs` | system 路由挂载 require_role 中间件 |
| 新建 | `frontend/src/pages/system/logs.rs` | 日志查询前端页面 |
| 新建 | `frontend/src/pages/system/backup.rs` | 备份管理前端页面 |
| 修改 | `frontend/src/main.rs` | 新增 system 路由（仅管理员可见） |
| 修改 | `frontend/src/api/system.rs` | 新增 system API 客户端 |

---

## Phase 1: 角色权限中间件（并查集模式）

### Task 1: UserRole 新增并查集角色继承方法

**Files:**
- Modify: `common/src/enums/user.rs`

- [ ] **Step 1: 在 UserRole 中新增 parent() 方法**

```rust
impl UserRole {
    // ... 现有方法保持不变 ...

    /// 获取上级角色（权限更高一级）
    ///
    /// 并查集角色继承体系：
    /// Member → Admin → SuperAdmin（根）
    ///
    /// 上级角色拥有下级角色的所有权限
    pub fn parent(&self) -> Option<UserRole> {
        match self {
            UserRole::SuperAdmin => None,    // 根节点，没有上级
            UserRole::Admin => Some(UserRole::SuperAdmin),
            UserRole::Member => Some(UserRole::Admin),
        }
    }

    /// 查找权限根（并查集 find 操作，带路径压缩语义）
    ///
    /// 最终都会回到 SuperAdmin
    pub fn find_root(&self) -> UserRole {
        match self.parent() {
            Some(parent) => parent.find_root(),
            None => *self,
        }
    }

    /// 判断当前用户角色是否满足要求的最低角色权限
    ///
    /// 核心逻辑：从 min_role 向上遍历祖先链，如果路径上包含 user_role，则满足。
    /// 因为上级角色 = 下级角色权限 + 额外权限，所以上级总是满足下级的要求。
    ///
    /// # 示例
    /// ```
    /// // user=Admin, min_role=Member → Member→Admin ✅ 满足
    /// // user=SuperAdmin, min_role=Member → Member→Admin→SuperAdmin ✅ 满足
    /// // user=Member, min_role=Admin → Admin→SuperAdmin ❌ 不满足
    /// // user=Member, min_role=SuperAdmin → SuperAdmin ❌ 不满足
    /// ```
    pub fn has_permission(user_role: UserRole, min_role: UserRole) -> bool {
        let mut current = min_role;
        loop {
            if current == user_role {
                return true;
            }
            match current.parent() {
                Some(parent) => current = parent,
                None => return false,
            }
        }
    }
}
```

- [ ] **Step 2: 编译验证**

```bash
cargo check -p common
```

Expected: 编译通过

- [ ] **Step 3: 提交**

```bash
git add common/src/enums/user.rs
git commit -m "feat: UserRole 新增并查集角色继承方法（parent/has_permission）"
```

---

### Task 2: JWT Claims 增加 user_role 字段

**Files:**
- Modify: `src/pkg/jwt.rs`

- [ ] **Step 1: 查看当前 jwt.rs 的 Claims 结构和 encode_token 函数**

- [ ] **Step 2: 在 Claims 中新增 role 字段**

```rust
// Claims 结构体中增加:
pub role: Option<i32>,  // UserRole 数值，None 表示未设置（兼容旧 token）
```

- [ ] **Step 3: 在 encode_token 函数中加入 role 参数**

修改 `encode_token` 签名，增加 `role: Option<UserRole>` 参数，在构建 Claims 时写入。

- [ ] **Step 4: 更新所有调用 encode_token 的地方（登录 handler 等）**

登录时从用户数据读取 role，传入 encode_token。

- [ ] **Step 5: 编译验证**

```bash
cargo check --lib
```

---

### Task 3: JWT 中间件注入 user_role 到请求头

**Files:**
- Modify: `common/src/constants/http_header.rs`
- Modify: `src/middleware/jwt_auth.rs`

- [ ] **Step 1: 新增 HTTP header 常量**

在 `common/src/constants/http_header.rs` 中增加：
```rust
pub const USER_ROLE: HeaderName = HeaderName::from_static("x-user-role");
```

- [ ] **Step 2: jwt_auth_middleware 解码后注入 role header**

在 JWT 验证通过后（现有 user_id/username/organization_id 注入逻辑之后），增加：
```rust
if let Some(role) = claims.role {
    if let Ok(header_value) = HeaderValue::from_str(&role.to_string()) {
        req.headers_mut().insert(http_header::USER_ROLE, header_value);
    }
}
```

- [ ] **Step 3: 编译验证**

```bash
cargo check --lib
```

---

### Task 4: RequestContext 增加 user_role 字段

**Files:**
- Modify: `src/pkg/request_context.rs`

- [ ] **Step 1: 在 RequestContext 结构体中新增 user_role 字段**

在 `model_provider_id` / `model_name` 之后增加：
```rust
/// 当前用户角色（数值，对应 UserRole 枚举）
#[log_field]
pub user_role: Option<i32>,
```

- [ ] **Step 2: RequestContextBuilder 新增 user_role 字段 + builder 方法**

```rust
// 在 RequestContextBuilder struct 中增加
user_role: Option<i32>,

// 在 impl RequestContextBuilder 中增加
pub fn user_role(mut self, role: i32) -> Self {
    self.user_role = Some(role);
    self
}
```

- [ ] **Step 3: from_headers 中提取 x-user-role**

在 `from_headers` 方法中，现有 user_id/username 提取逻辑之后增加：
```rust
let user_role = headers
    .get(http_header::USER_ROLE)
    .and_then(|v| v.to_str().ok())
    .and_then(|s| s.parse::<i32>().ok());
```

并在构建 RequestContext 时传入 `user_role`。

- [ ] **Step 4: to_builder 中保留 user_role**

- [ ] **Step 5: 同步更新 test_support 中的 ctx 构建函数**

- [ ] **Step 6: 编译验证**

```bash
cargo check --lib
```

---

### Task 5: require_role 中间件实现

**Files:**
- Create: `src/middleware/require_role.rs`
- Modify: `src/middleware/mod.rs`

- [ ] **Step 1: 创建 require_role 中间件**

```rust
//! 角色权限中间件
//!
//! 基于并查集角色继承体系，检查当前用户角色是否满足最低角色要求。
//! 用户角色在最低角色的祖先链上即可访问（上级角色满足下级要求）。

use crate::pkg::RequestContext;
use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use common::api::ApiResponse;
use common::enums::UserRole;

/// 角色权限中间件
///
/// 检查当前用户角色是否满足 `min_role` 要求。
/// 用户角色在 min_role 的祖先链上（含自身）则通过，否则返回 403。
///
/// # 示例
/// ```
/// // 要求至少 Admin 权限（Admin 和 SuperAdmin 可访问）
/// require_role_middleware(UserRole::Admin, req, next)
///
/// // 要求仅 SuperAdmin 可访问
/// require_role_middleware(UserRole::SuperAdmin, req, next)
/// ```
pub async fn require_role_middleware(
    min_role: UserRole,
    req: Request,
    next: Next,
) -> Response {
    let ctx = req
        .extensions()
        .get::<RequestContext>()
        .cloned();

    let user_role = ctx
        .as_ref()
        .and_then(|c| c.user_role)
        .map(UserRole::from_i32)
        .unwrap_or(UserRole::Member);

    if !UserRole::has_permission(user_role, min_role) {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiResponse::<()>::error(403, "权限不足".to_string())),
        )
            .into_response();
    }

    next.run(req).await
}
```

- [ ] **Step 2: 在 middleware/mod.rs 中导出**

```rust
pub mod require_role;
pub use require_role::require_role_middleware;
```

- [ ] **Step 3: 编译验证**

```bash
cargo check --lib
```

---

### Task 6: 路由挂载 + 编译验证

**Files:**
- Modify: `src/router.rs`

- [ ] **Step 1: system 路由挂载 require_role 中间件**

在 `protected_routes()` 中，将 system_routes 用 require_role 中间件包裹：

```rust
// 在 protected_routes 中，替换原来的 .nest("/system", system_routes())
.nest(
    "/system",
    system_routes().layer(axum::middleware::from_fn(|req, next| {
        require_role_middleware(UserRole::Admin, req, next)
    })),
)
```

含义：system 路由至少需要 Admin 权限（Admin 和 SuperAdmin 可访问）。
备份相关接口需要 SuperAdmin，在 handler 内部二次校验。

- [ ] **Step 2: 编译验证**

```bash
cargo check --lib
```

Expected: 编译通过

- [ ] **Step 3: 提交**

```bash
git add -A
git commit -m "feat: 角色权限中间件（并查集模式）+ JWT role 注入 + RequestContext user_role"
```

---

## Phase 2: 备份功能

### Task 7: 备份 DAL 实现

**Files:**
- Create: `src/service/dal/backup.rs`
- Modify: `src/service/dal/mod.rs`

前置依赖：Cargo.toml 中需要 `tar` 和 `flate2` 依赖。先检查是否已有：
- 如果没有，添加到根 Cargo.toml 的 [dependencies]
  - `tar = "0.4"`
  - `flate2 = "1.0"`
  - `md-5 = "0.10"`

- [ ] **Step 1: 定义备份相关结构体和 trait**

```rust
use common::error::Result;
use crate::pkg::RequestContext;
use std::sync::Arc;

/// 单个备份的元信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackupInfo {
    pub version: u64,
    pub timestamp: String,  // ISO8601 格式
    pub file_name: String,  // v1_20260717_153000.tar.gz
    pub size_bytes: u64,
    pub md5: String,
}

/// 备份索引文件（_index.json）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackupIndex {
    pub schema_version: u32,
    pub backups: Vec<BackupInfo>,
}

impl Default for BackupIndex {
    fn default() -> Self {
        Self {
            schema_version: 1,
            backups: Vec::new(),
        }
    }
}

#[async_trait::async_trait]
pub trait BackupDal: Send + Sync {
    /// 创建备份
    async fn create_backup(&self, ctx: RequestContext) -> Result<BackupInfo>;
    
    /// 列出所有备份（从 _index.json 读取，按版本号降序）
    async fn list_backups(&self, ctx: RequestContext) -> Result<Vec<BackupInfo>>;
    
    /// 删除指定版本的备份
    async fn delete_backup(&self, ctx: RequestContext, version: u64) -> Result<()>;
    
    /// 生成恢复脚本（返回脚本内容）
    async fn generate_restore_script(&self, ctx: RequestContext, version: u64) -> Result<String>;
}
```

- [ ] **Step 2: 实现 BackupDalFsImpl**

实现要点：

1. `create_backup`:
   - 从 config 获取 base_data_path
   - 读取 `_index.json`，计算下一个版本号（当前最大版本 + 1，空则 1）
   - 生成备份文件名：`v{version}_{YYYYMMDD_HHMMSS}.tar.gz`
   - 备份目录：`base_data_path/backups/`
   - 排除目录：`backups/`、`logs/`
   - 用 `tar::Builder` + `flate2::write::GzEncoder` 压缩
   - 计算压缩包 MD5
   - 更新 `_index.json`（追加新 BackupInfo）
   - 返回 BackupInfo

2. `list_backups`:
   - 读取 `_index.json`
   - 如果文件不存在，扫描目录重建索引（防御性）
   - 按 version 降序返回

3. `delete_backup`:
   - 删除指定版本的备份文件
   - 更新 `_index.json`（移除对应记录）

4. `generate_restore_script`:
   - 从 `_index.json` 查找指定版本
   - 返回一段 bash 脚本：
     ```bash
     #!/bin/bash
     # ai_orz 数据恢复脚本 - 恢复到版本 v{version}
     # ⚠️ 警告：此操作将覆盖当前所有数据！
     
     BACKUP_FILE="{backup_file_path}"
     DATA_DIR="{base_data_path}"
     
     # 1. 停止服务（需手动执行或通过 systemd）
     echo "请先停止 ai_orz 服务..."
     
     # 2. 备份当前数据（防止恢复失败）
     if [ -d "$DATA_DIR" ]; then
         mv "$DATA_DIR" "${DATA_DIR}.bak.$(date +%Y%m%d%H%M%S)"
     fi
     
     # 3. 创建数据目录并解压
     mkdir -p "$DATA_DIR"
     tar -xzf "$BACKUP_FILE" -C "$DATA_DIR"
     
     # 4. 恢复 backups 和 logs 目录（从备份的旧目录复制）
     # ...
     
     echo "恢复完成，请重启 ai_orz 服务"
     ```

- [ ] **Step 3: 添加单例 + init 函数**

```rust
static BACKUP_DAL: OnceLock<Arc<dyn BackupDal>> = OnceLock::new();

pub fn dal() -> Arc<dyn BackupDal> {
    BACKUP_DAL.get().cloned().unwrap()
}

pub fn init() {
    let _ = BACKUP_DAL.set(Arc::new(BackupDalFsImpl));
}
```

- [ ] **Step 4: 在 dal/mod.rs 中导出**

```rust
pub mod backup;
```

- [ ] **Step 5: 编译验证**

```bash
cargo check --lib
```

---

### Task 8: System Domain 增加 BackupManager

**Files:**
- Modify: `src/service/domain/system/mod.rs`

- [ ] **Step 1: SystemDomain trait 增加 backup_manager 方法**

```rust
pub trait SystemDomain: Send + Sync {
    fn cron_manager(&self) -> &dyn CronManager;
    fn backup_manager(&self) -> &dyn BackupManager;
}
```

- [ ] **Step 2: 定义 BackupManager trait**

```rust
use crate::service::dal::backup::BackupInfo;

#[async_trait::async_trait]
pub trait BackupManager: Send + Sync {
    async fn create_backup(&self, ctx: RequestContext) -> Result<BackupInfo>;
    async fn list_backups(&self, ctx: RequestContext) -> Result<Vec<BackupInfo>>;
    async fn delete_backup(&self, ctx: RequestContext, version: u64) -> Result<()>;
    async fn generate_restore_script(&self, ctx: RequestContext, version: u64) -> Result<String>;
}
```

- [ ] **Step 3: SystemDomainImpl 增加 backup_dal 字段**

```rust
struct SystemDomainImpl {
    cron_trigger_dal: Arc<dyn CronTriggerDal>,
    backup_dal: Arc<dyn BackupDal>,
}
```

更新 `new` 和 `init` 函数，注入 backup_dal。

- [ ] **Step 4: 实现 BackupManager trait**

直接委托给 backup_dal。

- [ ] **Step 5: 编译验证**

```bash
cargo check --lib
```

---

### Task 9: 备份 API Handler

**Files:**
- Create: `src/handlers/system/backup/mod.rs`
- Create: `src/handlers/system/backup/create_backup.rs`
- Create: `src/handlers/system/backup/list_backups.rs`
- Create: `src/handlers/system/backup/delete_backup.rs`
- Create: `src/handlers/system/backup/restore_backup.rs`
- Modify: `src/handlers/system/mod.rs`
- Modify: `src/router.rs`

权限说明：
- 创建/删除/恢复：仅 SuperAdmin（handler 内部二次校验 `UserRole::has_permission(user_role, UserRole::SuperAdmin)`）
- 列表：SuperAdmin + Admin（路由层 require_role_middleware(UserRole::Admin) 已覆盖）

- [ ] **Step 1: create_backup handler**

POST `/api/v1/system/backups`
- 检查角色是否为 SuperAdmin
- 调用 domain.backup_manager().create_backup()
- 返回 BackupInfo

- [ ] **Step 2: list_backups handler**

GET `/api/v1/system/backups`
- 调用 domain.backup_manager().list_backups()
- 返回 Vec<BackupInfo>

- [ ] **Step 3: delete_backup handler**

DELETE `/api/v1/system/backups/{version}`
- 检查 SuperAdmin
- 调用 domain.backup_manager().delete_backup(version)
- 返回成功

- [ ] **Step 4: restore_backup handler**

POST `/api/v1/system/backups/{version}/restore`
- 检查 SuperAdmin
- 调用 domain.backup_manager().generate_restore_script(version)
- 返回脚本内容（纯文本 content-type）

- [ ] **Step 5: 整合到 system mod + router**

- [ ] **Step 6: 编译验证**

```bash
cargo check --lib
```

- [ ] **Step 7: 提交**

```bash
git add -A
git commit -m "feat: 数据备份与恢复功能（_index.json 索引 + tar.gz 压缩 + 恢复脚本）"
```

---

## Phase 3: 日志查询

### Task 10: 日志查询 DAL

**Files:**
- Create: `src/service/dal/log_query.rs`
- Modify: `src/service/dal/mod.rs`

- [ ] **Step 1: 定义日志查询结构体和 trait**

```rust
use common::error::Result;
use crate::pkg::RequestContext;
use std::sync::Arc;

/// 单条日志条目
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
    pub log_id: Option<String>,
    pub user_id: Option<String>,
    pub operation: Option<String>,
    pub raw: serde_json::Value,
}

/// 日志查询参数
pub struct LogQuery {
    pub keyword: Option<String>,
    pub log_id: Option<String>,
    pub level: Option<String>,     // "INFO" / "WARN" / "ERROR" / "DEBUG"
    pub start_time: Option<i64>,   // unix timestamp ms
    pub end_time: Option<i64>,
    pub page: usize,
    pub page_size: usize,
}

/// 分页结果
pub struct LogPageResult {
    pub total: usize,
    pub entries: Vec<LogEntry>,
    pub page: usize,
    pub page_size: usize,
}

#[async_trait::async_trait]
pub trait LogQueryDal: Send + Sync {
    async fn query_logs(&self, ctx: RequestContext, query: LogQuery) -> Result<LogPageResult>;
}
```

- [ ] **Step 2: 实现 LogQueryDalFsImpl**

实现要点：
1. 从 config 获取 log_dir
2. 确定需要扫描的日志文件（按日期范围，最多最近 30 天）
3. 从最新文件开始倒序读取
4. 逐行解析 JSON
5. 过滤匹配：
   - keyword: message 字段包含关键词（不区分大小写）
   - log_id: log_id 精确匹配
   - level: level 匹配
   - start_time/end_time: timestamp 时间范围过滤
6. 分页（skip + take）
7. 单次查询最多扫描 10000 条记录

- [ ] **Step 3: 添加单例 + init 函数**

```rust
static LOG_QUERY_DAL: OnceLock<Arc<dyn LogQueryDal>> = OnceLock::new();

pub fn dal() -> Arc<dyn LogQueryDal> {
    LOG_QUERY_DAL.get().cloned().unwrap()
}

pub fn init() {
    let _ = LOG_QUERY_DAL.set(Arc::new(LogQueryDalFsImpl));
}
```

- [ ] **Step 4: 在 dal/mod.rs 中导出**

- [ ] **Step 5: 编译验证**

```bash
cargo check --lib
```

---

### Task 11: System Domain 增加 LogQuery 能力

**Files:**
- Modify: `src/service/domain/system/mod.rs`

- [ ] **Step 1: SystemDomain trait 增加 log_query 方法**

```rust
fn log_query(&self) -> &dyn LogQuery;
```

- [ ] **Step 2: 定义 LogQuery trait**

```rust
use crate::service::dal::log_query::{LogQuery, LogPageResult};

#[async_trait::async_trait]
pub trait LogQuery: Send + Sync {
    async fn query_logs(&self, ctx: RequestContext, query: LogQuery) -> Result<LogPageResult>;
}
```

- [ ] **Step 3: SystemDomainImpl 增加 log_query_dal 字段并实现**

- [ ] **Step 4: 编译验证**

```bash
cargo check --lib
```

---

### Task 12: 日志查询 API Handler

**Files:**
- Create: `src/handlers/system/logs/mod.rs`
- Create: `src/handlers/system/logs/query_logs.rs`
- Modify: `src/handlers/system/mod.rs`
- Modify: `src/router.rs`

- [ ] **Step 1: query_logs handler**

GET `/api/v1/system/logs`
Query 参数：
- keyword: 可选，关键词
- log_id: 可选，调用链 ID
- level: 可选，日志级别
- start_time: 可选，开始时间（ms）
- end_time: 可选，结束时间（ms）
- page: 默认 1
- page_size: 默认 20

权限：SuperAdmin + Admin（路由层 require_role_middleware(UserRole::Admin) 已覆盖）

- [ ] **Step 2: 整合到 system mod + router**

- [ ] **Step 3: 编译验证**

```bash
cargo check --lib
```

- [ ] **Step 4: 提交**

```bash
git add -A
git commit -m "feat: 日志查询功能（关键词 + log_id 调用链 + 级别 + 时间范围过滤）"
```

---

## Phase 4: 前端页面

### Task 13: 前端 System API 客户端

**Files:**
- Create/Modify: `frontend/src/api/system.rs`

- [ ] **Step 1: 新增备份相关 API 函数**

```rust
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    pub version: u64,
    pub timestamp: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub md5: String,
}

pub async fn list_backups() -> Result<Vec<BackupInfo>> {
    api_get::<Vec<BackupInfo>>("/system/backups").await
}

pub async fn create_backup() -> Result<BackupInfo> {
    api_post::<(), BackupInfo>("/system/backups", &()).await
}

pub async fn delete_backup(version: u64) -> Result<()> {
    api_delete::<()>(&format!("/system/backups/{}", version)).await
}

pub async fn get_restore_script(version: u64) -> Result<String> {
    // 返回纯文本脚本，需特殊处理
    // 使用底层 fetch 获取 text 响应
}
```

- [ ] **Step 2: 新增日志查询相关 API 函数**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
    pub log_id: Option<String>,
    pub user_id: Option<String>,
    pub operation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogPageResult {
    pub total: usize,
    pub entries: Vec<LogEntry>,
    pub page: usize,
    pub page_size: usize,
}

#[derive(Debug, Clone, Default)]
pub struct LogQueryParams {
    pub keyword: Option<String>,
    pub log_id: Option<String>,
    pub level: Option<String>,
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    pub page: usize,
    pub page_size: usize,
}

pub async fn query_logs(params: LogQueryParams) -> Result<LogPageResult> {
    let mut url = "/system/logs?".to_string();
    // 拼接 query 参数
    if let Some(kw) = &params.keyword { url.push_str(&format!("keyword={}&", kw)); }
    if let Some(id) = &params.log_id { url.push_str(&format!("log_id={}&", id)); }
    // ... 其他参数
    url.push_str(&format!("page={}&page_size={}", params.page, params.page_size));
    api_get::<LogPageResult>(&url).await
}
```

---

### Task 14: 日志查询页面

**Files:**
- Create: `frontend/src/pages/system/logs.rs`

- [ ] **Step 1: 实现日志查询页面组件**

页面结构：
- 查询表单：关键词输入、log_id 输入、级别下拉（INFO/WARN/ERROR/DEBUG）、时间范围选择
- 结果表格：时间、级别（带颜色徽章）、log_id、operation、message
- 分页：上一页/下一页
- 点击 log_id 可过滤同 log_id 的所有日志（调用链追踪）
- 点击单条日志可展开查看完整 JSON

样式：沿用现有设计系统 CSS 变量

---

### Task 15: 备份管理页面

**Files:**
- Create: `frontend/src/pages/system/backup.rs`

- [ ] **Step 1: 实现备份管理页面组件**

页面结构：
- 顶部：备份总数、最新版本号
- "创建备份"按钮（点击后显示 loading + 结果 toast）
- 备份列表表格：版本号、时间、文件名、大小、MD5、操作（删除/恢复）
- 恢复按钮：点击后弹出确认框，显示恢复脚本供用户复制执行

---

### Task 16: 前端路由整合

**Files:**
- Modify: `frontend/src/main.rs`

- [ ] **Step 1: 新增 system 路由组**

```rust
// /system/logs → 日志查询页面
// /system/backup → 备份管理页面
```

- [ ] **Step 2: 导航栏增加"系统管理"入口（仅管理员可见）**

需要从用户信息中判断角色（user_role <= 1 即 Admin 或 SuperAdmin），决定是否显示入口。

- [ ] **Step 3: 编译验证**

```bash
cd frontend && cargo check --target wasm32-unknown-unknown
```

- [ ] **Step 4: 提交**

```bash
git add -A
git commit -m "feat: 前端系统管理页面（日志查询 + 备份管理）"
```

---

## Phase 5: 测试与文档

### Task 17: 单元测试

- [ ] **Step 1: UserRole 权限判断测试**

```rust
#[test]
fn test_user_role_has_permission() {
    // SuperAdmin 满足所有要求
    assert!(UserRole::has_permission(UserRole::SuperAdmin, UserRole::SuperAdmin));
    assert!(UserRole::has_permission(UserRole::SuperAdmin, UserRole::Admin));
    assert!(UserRole::has_permission(UserRole::SuperAdmin, UserRole::Member));
    
    // Admin 满足 Admin 和 Member 要求，不满足 SuperAdmin
    assert!(!UserRole::has_permission(UserRole::Admin, UserRole::SuperAdmin));
    assert!(UserRole::has_permission(UserRole::Admin, UserRole::Admin));
    assert!(UserRole::has_permission(UserRole::Admin, UserRole::Member));
    
    // Member 只满足自身
    assert!(!UserRole::has_permission(UserRole::Member, UserRole::SuperAdmin));
    assert!(!UserRole::has_permission(UserRole::Member, UserRole::Admin));
    assert!(UserRole::has_permission(UserRole::Member, UserRole::Member));
}

#[test]
fn test_user_role_parent() {
    assert_eq!(UserRole::SuperAdmin.parent(), None);
    assert_eq!(UserRole::Admin.parent(), Some(UserRole::SuperAdmin));
    assert_eq!(UserRole::Member.parent(), Some(UserRole::Admin));
}

#[test]
fn test_user_role_find_root() {
    assert_eq!(UserRole::SuperAdmin.find_root(), UserRole::SuperAdmin);
    assert_eq!(UserRole::Admin.find_root(), UserRole::SuperAdmin);
    assert_eq!(UserRole::Member.find_root(), UserRole::SuperAdmin);
}
```

- [ ] **Step 2: 备份 DAL 单元测试**
  - 创建临时目录模拟数据
  - 测试 create_backup 生成压缩包 + _index.json 更新
  - 测试 list_backups 从 _index.json 读取
  - 测试 delete_backup 删除文件 + 更新 _index.json
  - 测试 _index.json 不存在时防御性重建

- [ ] **Step 3: 日志查询 DAL 单元测试**
  - 生成测试 JSONL 日志文件
  - 测试关键词过滤
  - 测试 log_id 过滤
  - 测试级别过滤
  - 测试时间范围过滤
  - 测试分页

- [ ] **Step 4: 运行全部测试**

```bash
cargo test
```

Expected: 所有测试通过

---

### Task 18: 文档更新

- [ ] **Step 1: 更新 AGENTS.md 的功能列表**

在已实现功能中增加：
- 数据备份与恢复（_index.json 索引 + tar.gz 压缩 + 恢复脚本）
- 日志在线查询（关键词 + log_id 调用链 + 级别 + 时间范围）
- 基于角色并查集的权限中间件

- [ ] **Step 2: 更新 README.md**

在功能列表中简要提及系统管理能力。

- [ ] **Step 3: 提交**

```bash
git add -A
git commit -m "docs: 更新文档，新增系统管理工具说明"
```

---

## 自检清单

### Spec Coverage
- ✅ 版本信息与 config 解耦，使用 _index.json 总索引 — Task 7
- ✅ 备份压缩包（tar.gz，排除 backups/logs）— Task 7
- ✅ 版本号自动递增（扫描 _index.json 最大版本 + 1）— Task 7
- ✅ 恢复 API 返回可执行脚本 — Task 9
- ✅ 日志查询（关键词 + log_id + 级别 + 时间）— Task 10/11/12
- ✅ 角色权限中间件（并查集模式）— Task 1-6
- ✅ SuperAdmin 控制备份操作 — Task 9
- ✅ 前端隐藏管理页面 — Task 13-16

### 角色权限并查集设计
- Member → Admin → SuperAdmin（根）
- `has_permission(user_role, min_role)`: 从 min_role 向上遍历，user_role 在祖先链上则满足
- `require_role_middleware(UserRole::Admin)`: Admin 和 SuperAdmin 可访问
- 未来新增角色只需在 `parent()` 里加一条配置

### 类型一致性
- BackupInfo/BackupIndex 在 dal 层定义，domain 层 re-export，handler 直接使用
- LogEntry/LogQuery/LogPageResult 同上
- UserRole 枚举在 common 层，前后端共用

### 分层合规性
- Handler → Domain → DAL 单向调用 ✅
- 备份 DAL 操作文件系统，不访问数据库 ✅
- 日志查询 DAL 操作文件系统，不访问数据库 ✅
- 备份属于 System Domain ✅
- 日志查询属于 System Domain ✅
