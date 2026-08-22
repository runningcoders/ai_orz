# 共享工具凭据增强器落地执行计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** MCP Server / HTTP 工具的凭据消费从「配置内嵌原文（env/headers）」迁移为「类型级需求声明 + 凭据增强器」，运行时编排注入（domain 生产路由取数 → pkg 加工 → check 注入单次实例）；内置工具 gh_cli/tavily/lark_cli 统一工厂化，per-tool resolver trait 三清零。

**Architecture:** 配置层只存 `(kind, platform, field?) + enhancer? + binding` 非敏感声明；运行时编排注入（D17/D22）：domain 编排读需求 → **生产路由取数**（LarkApp → lark dal 渠道路径附 attributes / 其余 → user dal `find_default`）→ pkg 纯函数加工（解密/增强器/canonical，零数据访问）→ 工厂 create 实例 + `check(resolved)` 注入（实例单次使用）→ call 只做 binding 纯放置；stdio 连接按 (server, user) 隔离（D23）；内置工具 gh_cli/tavily/lark_cli 三员统一工厂化，per-tool resolver trait 三清零。

**Tech Stack:** Rust + sqlx 0.8（SQLite STRICT）+ reqwest + base64 0.22；前端 Dioxus 0.7。

**设计文档（决策 SSOT）：** [docs/design/tool_credential_requirement_design.md](../../../docs/design/tool_credential_requirement_design.md) v1.5——执行中遇到本计划未覆盖的决策点，以设计文档为准；仍无答案时停下来问用户，不要自行发明。

---

## 〇、执行者前置信息（每个 Task 开工前必读）

### 0.1 构建与验证命令

| 场景 | 命令 |
|------|------|
| 后端单测（当前 crate lib） | `cargo test --lib`（可加 `-- 模块名` 过滤） |
| common 包单测 | `cd common && cargo test` |
| 后端 clippy 零容忍 | `cargo clippy --all-targets -- -D warnings` |
| 前端 clippy | `cd frontend && cargo clippy --target wasm32-unknown-unknown --all-targets -- -D warnings` |
| 前端测试 | `cd frontend && cargo test` |
| sqlx 离线缓存刷新 | 先删旧库 `rm -f .ai_orz/test.db` → `sqlx database create --database-url sqlite://./.ai_orz/test.db` → `sqlx database setup`（跑全部迁移）→ `cargo sqlx prepare --database-url sqlite://./.ai_orz/test.db` → 确认 `.sqlx/` 变更随代码一起提交 |
| 全量测试入口 | `make test` / `make clippy`（Makefile 为统一入口） |

注意：任何改动 DAO SQL 的 Task，`cargo test` 必须在 `SQLX_OFFLINE=true`（默认读 `.sqlx` 缓存）下通过；修改 SQL 后未 prepare 会导致宏编译失败，这是预期信号而非事故。

### 0.2 关键现状锚点（已核实，2026-08-21）

| 锚点 | 位置 | 现状要点 |
|------|------|---------|
| 凭据模型 SSOT | common/src/models/identity_credentials.rs | `CredentialKind`（LarkApp/GithubToken/TavilyKey 三变体）、`CredentialDetail`/`Patch` serde 内部 tag `snake_case`；`encrypt_sensitive(闭包)` 加密、apply_patch 打补丁 |
| 凭据 DAO | src/service/dao/user_credential/{mod,sqlite}.rs | `find_default(ctx, user_id, kind)` 解析链单点：`ORDER BY (uc.user_id=u.id) DESC, is_default DESC, created_at ASC LIMIT 1`；`set_default` 同事务清旧立新（作用域由 visibility 派生） |
| 既有 Gh/Tavily resolver | src/service/dal/user.rs#L301-L358 | 实现各自 pkg trait，`dal().find_default_credential` + `decrypt_channel_secret`；**本计划随 per-tool trait 整体删除**（取数上移 domain 编排，D17） |
| 既有 Lark resolver | src/service/dal/lark.rs#L47-L63 | `LarkDalCredentialResolver` → `resolve_credentials_for_user`（渠道路径）；**本计划删除 trait 段**（v1.5 统一），生产方法保留为合法生产端（D17） |
| resolver 注册点 | src/service/mod.rs `init()` | OnceLock setter 模式：`gh_cli::set_credential_resolver(...)` 等 3 个（**三行全删且无新增注册**，v1.5） |
| 工具注册表 | src/pkg/tool_registry/mod.rs#L45-L108 | **现状已是工厂 + per-request create**（`create_tool(po)` 每次从 PO 创建实例）——D22 实例化模型的基础设施零新增 |
| CoreTool trait | src/models/tool.rs | `call(ctx, args)`；**本计划增** `credential_requirements()` + `check(&mut self, resolved)` 生命周期方法（D22） |
| 工具调用编排 | src/service/dal/mcp_tool.rs#L236-L288 | `call_tool` → `assemble_executable_tool`（每次调用重组装）→ `mcp_tool_call_dao.execute`——check 注入点在 assemble 之后 |
| MCP 配置 | src/models/mcp_server.rs#L101-L167 | `McpServerConfig` 含 `env`/`headers`（**整体移除**）；`redacted_for_management` 现对 env/headers 值打码 |
| MCP 运行时 | src/pkg/tool_registry/mcp.rs | `call_tool(server, tool_name, args)` 无 ctx；`connect_stdio_client` L214-216 静态 env 注入循环（删除）；`McpCoreTool::call(_ctx, args)` ctx 被丢弃 |
| MCP DTO | common/src/api/mcp_server.rs#L12-L32 | `McpServerConfigDto` 含 env/headers（同步删除）；转换在 src/handlers/finance/mcp_server/response.rs |
| HTTP 工具 | src/pkg/tool_registry/http.rs | `HttpToolConfig` headers/query 模板；`validate_config` L369 起；`execute_http_call` 无 ctx；`_ctx` 被丢弃 |
| 敏感名判定 | src/pkg/tool_registry/tool_security.rs#L173-L181 | `is_sensitive_header`（authorization/cookie/token/secret/api-key/password） |
| SSRF 校验 | src/pkg/tool_registry/tool_security.rs `validate_target_url` | 返回 pinned addresses；OAuth 刷新复用 |
| 缺凭据引导 | src/pkg/tool_registry/tool_readiness.rs#L226-L233 | `api_key_missing_json(error, guidance)` 结构化 JSON |
| DB schema | migrations/20260420000000_initial.sql#L405-L419, L751-L757 | `user_credentials` 无 platform 列；两个默认唯一部分索引 `(user_id,kind)` / `(org_id,kind)` |
| 前端 MCP 表单 | frontend/src/pages/finance/mcp_servers.rs#L48-L112 | 仅 name/transport/command|url 三字段，`McpServerConfigDto::default()` 构造 |
| 前端 HTTP 工具表单 | frontend/src/components/create_http_tool.rs + frontend/src/pages/finance/tools.rs | method/url/headers/query 静态构造 |
| base64 依赖 | 根 Cargo.toml（已有 `base64 = "0.22"`） | BasicAuth 增强器直接用，无需新增依赖 |

### 0.3 全局约束（每个 Task 都生效）

1. **分层红线**：pkg 不 import `crate::service::*`——数据访问一律经本计划新增的 `CredentialDataProvider` trait + `service::init()` 注入；handler 只调 domain，PO 不出 DAL 层之上。
2. **凭据安全红线**（设计 §四）：落库 config 零 credential_id / 零原文；注入值禁入日志/错误/工具入参 schema；错误文案不得包含 platform 之外的凭据内容；`redacted_for_management` 语义保持。
3. **命名规范**：DAO 查询方法 `find_`/`query_`；枚举 snake_case serde；新测试文件用 `#[path]` 内嵌模式（参照 mcp_tests.rs）。
4. **sqlx 宏**：所有改动的 `sqlx::query!`/`query_as!` 完成后必须 `cargo sqlx prepare` 并提交 `.sqlx` 增量。
5. **commit 粒度**：每 Task 一次提交，message 格式 `feat(credential)/fix(mcp)/...`；不要 `git add -A`，按文件加。
6. **每 Phase 收口**：`cargo test --lib && cd common && cargo test && cargo clippy --all-targets -- -D warnings` 全绿才可进入下一 Phase。

### 0.4 范围外（明确不做）

- 新 kind（generic_token/oauth/user_password）的**创建/管理 API 与前端凭据区块**——设计 §三未包含；测试用例直接 DB 插行构造。落地后另起 plan。
- streamable_http MCP 运行时实现（保持 not implemented 错误，仅 DTO/校验层支持 Header binding）。
- 非敏感 env 白名单（D21）、凭据健康度告警（设计 §五.7）。

---

## Phase 1：契约与存储基座（common 模型 + DAO platform 维度 + 迁移）

> 交付物：新凭据类型/需求声明契约可通过编译与单测；`user_credentials` 表有 platform 维度；不改变任何既有运行时行为。

### Task 1.1：CredentialKind / CredentialDetail 扩展三变体

**Files:**
- Modify: common/src/models/identity_credentials.rs
- Test: 同文件内嵌 `mod tests`

- [x] **Step 1：写失败测试**——在文件内嵌 tests 追加：

```rust
// ==================== GenericToken / OAuth / UserPassword ====================

#[test]
fn test_new_kinds_serde_snake_case() {
    assert_eq!(
        serde_json::to_value(CredentialKind::GenericToken).unwrap(),
        "generic_token"
    );
    assert_eq!(serde_json::to_value(CredentialKind::OAuth).unwrap(), "oauth");
    assert_eq!(
        serde_json::to_value(CredentialKind::UserPassword).unwrap(),
        "user_password"
    );
}

#[test]
fn test_generic_token_detail_lifecycle() {
    let plain = CredentialDetail::GenericToken { token: " ntn_xxx ".to_string() }.normalized();
    assert!(matches!(&plain, CredentialDetail::GenericToken { token } if token == "ntn_xxx"));
    assert!(plain.validate().is_ok());
    let enc = CredentialDetail::GenericToken { token: "plain".to_string() }
        .encrypt_sensitive(|s| Ok(format!("enc:{}", s))).unwrap();
    assert!(matches!(&enc, CredentialDetail::GenericToken { token } if token == "enc:plain"));
    let mut detail = CredentialDetail::GenericToken { token: "enc:v1:old".to_string() };
    detail.apply_patch(
        CredentialDetailPatch::GenericToken { token: Some("new".to_string()) },
        |s| Ok(format!("enc:{}", s)),
    ).unwrap();
    assert!(matches!(&detail, CredentialDetail::GenericToken { token } if token == "enc:new"));
}

#[test]
fn test_oauth_detail_validate_and_secret() {
    let detail = CredentialDetail::OAuth {
        token_endpoint: "https://example.invalid/oauth/token".to_string(),
        client_id: "cid".to_string(),
        client_secret: "csec".to_string(),
        refresh_token: "rt".to_string(),
        scope: Some("read".to_string()),
    };
    assert!(detail.clone().normalized().validate().is_ok());
    // 缺 token_endpoint → 校验失败
    let bad = CredentialDetail::OAuth { token_endpoint: String::new(), client_id: "c".into(), client_secret: "s".into(), refresh_token: "r".into(), scope: None };
    assert!(bad.normalized().validate().is_err());
    assert_eq!(detail.primary_secret(), "rt");
    // client_secret / refresh_token 加密，client_id / token_endpoint 不加密
    let enc = detail.encrypt_sensitive(|s| Ok(format!("enc:{}", s))).unwrap();
    let CredentialDetail::OAuth { client_id, client_secret, refresh_token, .. } = enc else { panic!() };
    assert_eq!(client_id, "cid");
    assert_eq!(client_secret, "enc:csec");
    assert_eq!(refresh_token, "enc:rt");
}

#[test]
fn test_user_password_detail() {
    let detail = CredentialDetail::UserPassword { username: "alice".to_string(), password: " pw ".to_string() };
    let normalized = detail.normalized();
    assert!(matches!(&normalized, CredentialDetail::UserPassword { username, password } if username == "alice" && password == "pw"));
    assert!(normalized.validate().is_ok());
    let enc = CredentialDetail::UserPassword { username: "alice".into(), password: "p".into() }
        .encrypt_sensitive(|s| Ok(format!("enc:{}", s))).unwrap();
    // password 加密、username 不加密
    assert!(matches!(&enc, CredentialDetail::UserPassword { username, password } if username == "alice" && password == "enc:p"));
}
```

- [x] **Step 2：跑测试确认编译失败**（`cd common && cargo test identity_credentials`——变体不存在）。

- [x] **Step 3：实现**——按失败项补齐：
  1. `CredentialKind` 增 `GenericToken` / `OAuth` / `UserPassword` 三变体（doc 注释说明 generic 类 platform 必填）；derive 保持，另加 `schemars::JsonSchema`（`common/src/api` 已直接用 `#[derive(JsonSchema)]`，照抄模式；若 common 的 models 模块编译报 schemars 缺失，在 common/Cargo.toml 确认依赖后加 derive）。
  2. `CredentialDetail` 增三变体（serde tag 自动 `generic_token`/`oauth`/`user_password`）：

```rust
/// 通用平台令牌（Notion/Linear PAT 等；platform 必填，token 落库前加密）
GenericToken {
    /// 平台令牌（落库前经 encrypt_channel_secret 加密）
    token: String,
},
/// OAuth 刷新凭据（platform 必填；client_secret / refresh_token 落库前加密）
OAuth {
    /// 刷新端点（https，刷新前过 SSRF 校验）
    token_endpoint: String,
    client_id: String,
    /// 落库前加密
    client_secret: String,
    /// 落库前加密
    refresh_token: String,
    scope: Option<String>,
},
/// 用户名密码对（platform 必填；password 落库前加密）
UserPassword {
    username: String,
    /// 落库前加密
    password: String,
},
```

  3. `CredentialDetailPatch` 增对应三变体（字段全 Option；敏感字段提供时明文传入内部加密）。
  4. `impl CredentialDetail` 五个方法补 match 臂：`kind()` / `primary_id()`（三者均 None）/ `normalized()`（trim；scope 空白清除）/ `validate()`（OAuth 必填 token_endpoint+client_id+client_secret+refresh_token；UserPassword 两者必填；GenericToken token 必填）/ `encrypt_sensitive()`（OAuth 加密 client_secret+refresh_token；UserPassword 只加密 password；GenericToken 加密 token）/ `apply_patch()`（模式照 GithubToken 臂，跨类型 mismatch 报错）。
  5. 新增 `primary_secret()`（返回主密钥字段引用：LarkApp→app_secret、GithubToken/GenericToken→token、TavilyKey→api_key、OAuth→refresh_token、UserPassword→password；doc 注释「调用前须已解密」）。
  6. `impl CredentialKind` 新增：

```rust
/// generic 类 kind：匹配键含 platform 维度（D3）
pub fn requires_platform(&self) -> bool {
    matches!(self, Self::GenericToken | Self::OAuth | Self::UserPassword)
}
```

- [x] **Step 4：测试通过** `cd common && cargo test`。
- [x] **Step 5：提交** `git add common/src/models/identity_credentials.rs && git commit -m "feat(credential): add generic_token/oauth/user_password credential kinds"`。

### Task 1.2：CredentialRequirement 契约 + supports 矩阵单点

**Files:**
- Modify: common/src/models/identity_credentials.rs（文件尾追加）

- [x] **Step 1：写失败测试**：

```rust
// ==================== CredentialRequirement 契约 ====================

#[test]
fn test_requirement_serde_roundtrip() {
    let req = CredentialRequirement {
        kind: CredentialKind::OAuth,
        platform: Some("linear".to_string()),
        field: None,
        enhancer: Some(CredentialEnhancerKind::BearerToken),
        binding: CredentialBinding::Header { name: "authorization".to_string() },
    };
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json["kind"], "oauth");
    assert_eq!(json["platform"], "linear");
    assert_eq!(json["enhancer"], "bearer_token");
    assert_eq!(json["binding"]["type"], "header");
    assert_eq!(json["binding"]["name"], "authorization");
    let parsed: CredentialRequirement = serde_json::from_value(json).unwrap();
    assert_eq!(parsed, req);
}

#[test]
fn test_binding_serde_snake_case() {
    let env = CredentialBinding::Env { name: "API_TOKEN".to_string() };
    assert_eq!(serde_json::to_value(&env).unwrap()["type"], "env");
    let query = CredentialBinding::Query { name: "api_key".to_string() };
    assert_eq!(serde_json::to_value(&query).unwrap()["type"], "query");
}

#[test]
fn test_enhancer_supports_matrix() {
    use CredentialEnhancerKind as E;
    // D12 精确表
    assert!(enhancer_supports(CredentialKind::GenericToken, E::BearerToken));
    assert!(enhancer_supports(CredentialKind::OAuth, E::BearerToken));
    assert!(enhancer_supports(CredentialKind::OAuth, E::AccessToken));
    assert!(enhancer_supports(CredentialKind::UserPassword, E::BasicAuth));
    // 专用 kind 零支持
    for kind in [CredentialKind::LarkApp, CredentialKind::GithubToken, CredentialKind::TavilyKey] {
        assert!(!enhancer_supports(kind, E::BearerToken));
        assert!(!enhancer_supports(kind, E::BasicAuth));
        assert!(!enhancer_supports(kind, E::AccessToken));
    }
    // 反向组合拒绝
    assert!(!enhancer_supports(CredentialKind::UserPassword, E::BearerToken));
    assert!(!enhancer_supports(CredentialKind::GenericToken, E::AccessToken));
    assert!(!enhancer_supports(CredentialKind::GenericToken, E::BasicAuth));
}

#[test]
fn test_default_enhancer_assembly() {
    use CredentialEnhancerKind as E;
    assert_eq!(default_enhancer(CredentialKind::OAuth), Some(E::AccessToken));
    assert_eq!(default_enhancer(CredentialKind::UserPassword), Some(E::BasicAuth));
    assert_eq!(default_enhancer(CredentialKind::GenericToken), None);
    assert_eq!(default_enhancer(CredentialKind::LarkApp), None);
}
```

- [x] **Step 2：跑测试确认失败**。
- [x] **Step 3：实现**（文件尾追加，全部 derive `Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema`）：

```rust
/// 共享工具的凭据需求声明（类型级声明，非实例级引用；全部字段非敏感）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CredentialRequirement {
    /// 需要的凭据类型
    pub kind: CredentialKind,
    /// 平台标识（generic 类 kind 必填，专用 kind 必空；匹配键二元组第二维）
    pub platform: Option<String>,
    /// 提取字段：None = 规范可用值；Some = detail 指定字段；与 enhancer 互斥
    pub field: Option<String>,
    /// 增强器类型：None = 规范可用值；显式选择默认增强器幂等等价于 None（D11）；与 field 互斥
    pub enhancer: Option<CredentialEnhancerKind>,
    /// 注入点（纯放置，零变换）
    pub binding: CredentialBinding,
}

/// 凭据增强器类型（配置声明与 fetch options 共用同一值域）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CredentialEnhancerKind {
    /// "Bearer " + 规范可用值
    BearerToken,
    /// "Basic " + base64(username:password)
    BasicAuth,
    /// OAuth refresh → access_token（oauth 默认装配）
    AccessToken,
}

/// 注入点（纯放置，零变换）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CredentialBinding {
    /// 注入子进程环境变量（stdio MCP）
    Env { name: String },
    /// 注入 HTTP 请求头（http MCP / HTTP 工具）
    Header { name: String },
    /// 注入 URL 查询参数（HTTP 工具）
    Query { name: String },
    /// 存工具实例字段（内置工具消费形态，D25；field 为实例字段名）
    Internal { field: String },
}

/// 需求声明的作用域（binding ↔ 协议匹配校验用；前端预校验与后端共用）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CredentialRequirementScope {
    /// stdio MCP Server：仅 Env binding
    McpStdio,
    /// streamable HTTP MCP Server：仅 Header binding
    McpHttp,
    /// HTTP 工具：Header / Query binding
    HttpTool,
    /// 内置工具（DB 不持久化，静态声明自校验用）：仅 Internal binding（D25）
    Builtin,
}

/// 增强器 ↔ 凭据类型可用性矩阵（D12 单点；pkg supports 与前端下拉过滤共用）
pub fn enhancer_supports(kind: CredentialKind, enhancer: CredentialEnhancerKind) -> bool {
    matches!(
        (kind, enhancer),
        (CredentialKind::GenericToken, CredentialEnhancerKind::BearerToken)
            | (CredentialKind::OAuth, CredentialEnhancerKind::BearerToken)
            | (CredentialKind::OAuth, CredentialEnhancerKind::AccessToken)
            | (CredentialKind::UserPassword, CredentialEnhancerKind::BasicAuth)
    )
}

/// 复合形态凭据的默认增强器（D7：oauth→AccessToken、user_password→BasicAuth）
pub fn default_enhancer(kind: CredentialKind) -> Option<CredentialEnhancerKind> {
    match kind {
        CredentialKind::OAuth => Some(CredentialEnhancerKind::AccessToken),
        CredentialKind::UserPassword => Some(CredentialEnhancerKind::BasicAuth),
        _ => None,
    }
}
```

- [x] **Step 4：测试通过**。
- [x] **Step 5：提交** `git commit -m "feat(credential): add CredentialRequirement contract with supports matrix"`。

### Task 1.3：DB 迁移 + DAO platform 维度

**Files:**
- Create: migrations/20260821000000_add_credential_platform.sql
- Modify: src/models/user_credential.rs（PO + 实体加 platform）
- Modify: src/service/dao/user_credential/{mod.rs,sqlite.rs}
- Modify: src/service/dal/user.rs（trait 签名）
- Test: sqlite.rs 内嵌测试 / 既有 DAO 测试改造

- [x] **Step 1：写迁移**：

```sql
-- 用户凭证表增加 platform 维度（generic 类凭据按 (kind, platform) 匹配）
ALTER TABLE user_credentials ADD COLUMN platform TEXT NULL;

-- 默认唯一索引升级：加 platform 维度（存量专用 kind 行 platform 为 NULL，语义不变）
DROP INDEX IF EXISTS uq_user_credentials_default_private;
CREATE UNIQUE INDEX uq_user_credentials_default_private
ON user_credentials(user_id, kind, platform)
WHERE is_default = 1 AND visibility = 'private' AND status = 1;

DROP INDEX IF EXISTS uq_user_credentials_default_public;
CREATE UNIQUE INDEX uq_user_credentials_default_public
ON user_credentials(org_id, kind, platform)
WHERE is_default = 1 AND visibility = 'public' AND status = 1;

CREATE INDEX IF NOT EXISTS idx_user_credentials_platform ON user_credentials(platform);
```

> 执行时先读 migrations/20260420000000_initial.sql#L751-L764 确认原索引 WHERE 子句，新索引条件与其完全一致只加列。

- [x] **Step 2：PO/实体加字段**——`UserCredentialPo` 加 `pub platform: Option<String>`（FromRow 自动）；`src/models/user_credential.rs` 的 `UserCredential` 实体同步加；`sqlite.rs` 所有 `SELECT` 列清单加 `uc.platform`；`insert`/`update` 加列写入。
- [x] **Step 3：DAO 接口扩展**（mod.rs）：
  - `UserCredentialQuery` 加 `pub platform: Option<String>`（列表过滤：Some 精确匹配、None 不限——与 find_default 语义区分，注释写明）；`push_query_filters` 加 `Some(p) => builder.push(" AND uc.platform = ")...`。
  - `find_default` 签名改 `(ctx, user_id: &str, kind: CredentialKind, platform: Option<&str>)`，SQL 加 `AND uc.platform IS ?`（SQLite `IS ?` 同时匹配 NULL 与值；None 即要求 platform 为 NULL——专用 kind 语义）。doc 注释更新为「匹配键 (kind, platform) 二元组」。
  - `set_default` 内部两段清旧 UPDATE 的 WHERE 各加 `AND platform IS ?`（取 `target.platform.as_deref()`）。
  - `clear_default` 签名加 `platform: Option<&str>`，WHERE 加 `AND platform IS ?`。
- [x] **Step 4：调用点适配**——`src/service/dal/user.rs`：trait `UserDal` 的 `find_default_credential` / `clear_default_credential` 加 `platform: Option<&str>` 参数；`src/service/domain/finance/identity_credential.rs` 调用处专用 kind 一律传 `None`；`create_credential` 构造 PO 时 `platform: None`（编译器逐个指出调用点，全部补齐）。
- [x] **Step 5：新增 DAO 测试**（sqlite.rs 既有测试模式，sqlx::test 或内存池 + 迁移）：
  - `find_default_matches_platform_exact`：插两条 generic_token（platform=linear / notion），`find_default(kind, Some("linear"))` 命中 linear 行。
  - `find_default_none_platform_excludes_valued`：`find_default(kind, None)` 不匹配 platform 非空行。
  - `set_default_scoped_by_platform`：同 kind 不同 platform 可各自持有默认（不再互斥）。
- [x] **Step 6：刷新 .sqlx 并全绿**：

```bash
rm -f .ai_orz/test.db
sqlx database create --database-url sqlite://./.ai_orz/test.db
sqlx database setup 2>/dev/null || sqlx migrate run --database-url sqlite://./.ai_orz/test.db
cargo sqlx prepare --database-url sqlite://./.ai_orz/test.db
cargo test --lib
```

- [x] **Step 7：提交**（含 `.sqlx/` 变更）`git commit -m "feat(credential): add platform dimension to user_credentials"`。

**Phase 1 收口检查：**
- [x] `cd common && cargo test` / `cargo test --lib` / `cargo clippy --all-targets -- -D warnings` 全绿。
- [x] 既有 gh_cli / tavily_search 测试行为不变（find_default 调用链新增 platform=None 参数）。

---

## Phase 2：pkg 凭据纯值加工模块（增强器 + 解密 + resolve_requirements + 校验）

> 交付物：新建 `src/pkg/credential/`（门面 mod.rs + 子模块 enhancer.rs）——**凭据域纯值加工模块，零数据访问**（无 ctx / 无端口 trait / 无 OnceLock 注入注册，D17），不隶属 tool_registry（依赖方向 tool_registry → credential 单向）；全部逻辑可纯函数构造测试（无 DB / 无 mock provider）。gh_cli / tavily 的 per-tool resolver 删除与工厂化改造在 Phase 3 统一执行。

### Task 2.1：模块骨架 + decrypt_detail 解密单点

**Files:**
- Create: src/pkg/credential/mod.rs
- Modify: src/pkg/mod.rs（`pub mod credential;`）
- Test: src/pkg/credential/mod.rs 内嵌 `mod tests`

- [x] **Step 1：创建模块**——纯值加工定位，骨架（本 Task 只含 doc 头 + 解密函数，后续 Task 追加增强器与编排）：

```rust
//! 共享工具凭据需求运行时（设计：docs/design/tool_credential_requirement_design.md）
//!
//! 纯值加工模块（D17）：解密 / 增强器 / canonical / OAuth 刷新 / 配置校验。
//! 零数据访问——凭据由 service 编排层（domain resolve_tool_credentials）
//! 从 user dal 取回后传入；本模块不持有 ctx、不定义数据端口、无注入注册。

use common::error::Result;
use common::models::CredentialDetail;
```

- [x] **Step 2：解密单点**（同文件追加）：

```rust
// ==================== 解密单点 ====================

/// 按 kind 解密 detail 敏感字段（与 encrypt_sensitive 规则对称）。
/// 入参为 DAL 取回的加密态 detail；解密结果仅存于当次调用栈。
pub(crate) fn decrypt_detail(detail: CredentialDetail) -> Result<CredentialDetail> {
    let decrypt = crate::pkg::crypto::decrypt_channel_secret;
    Ok(match detail {
        CredentialDetail::LarkApp { app_id, app_secret, encrypt_key, verification_token } => {
            CredentialDetail::LarkApp {
                app_id,
                app_secret: decrypt(app_secret.as_str())?,
                encrypt_key: match encrypt_key {
                    Some(v) => Some(decrypt(v.as_str())?),
                    None => None,
                },
                verification_token,
            }
        }
        CredentialDetail::GithubToken { token } => CredentialDetail::GithubToken { token: decrypt(token.as_str())? },
        CredentialDetail::TavilyKey { api_key } => CredentialDetail::TavilyKey { api_key: decrypt(api_key.as_str())? },
        CredentialDetail::GenericToken { token } => CredentialDetail::GenericToken { token: decrypt(token.as_str())? },
        CredentialDetail::OAuth { token_endpoint, client_id, client_secret, refresh_token, scope } => CredentialDetail::OAuth {
            token_endpoint, client_id,
            client_secret: decrypt(client_secret.as_str())?,
            refresh_token: decrypt(refresh_token.as_str())?,
            scope,
        },
        CredentialDetail::UserPassword { username, password } => CredentialDetail::UserPassword { username, password: decrypt(password.as_str())? },
    })
}
```

- [x] **Step 3：解密单测**——测试用「与 encrypt_sensitive 对称」断言（`encrypt_sensitive(|s| Ok(format!("enc:{}", s)))` 加密 → `decrypt_channel_secret` 真实解密需密钥；单测改用注入闭包思路不可行时，直接构造 `decrypt_channel_secret` 可解的样本或仅测非敏感字段透传 + 结构 match；参照 pkg::crypto 既有测试模式）：

```rust
#[test]
fn decrypt_detail_passes_non_sensitive_fields() {
    // github_token 全字段敏感：验证结构重组与透传（加解密对称性依赖 crypto 既有测试）
    let detail = CredentialDetail::UserPassword { username: "alice".into(), password: "pw".into() };
    // 解密需要真实密钥环境——本用例验证 username 不经解密函数（原文透传）：
    // 在 decrypt_detail 中 username 直传，故无密钥环境下该字段不变。
    // （若 pkg::crypto 测试模式支持无密钥直通，按其模式补全字段断言）
    assert_eq!(detail.kind(), decrypt_safe_kind(&detail));
}
```

> 上述测试写法以 pkg::crypto 现有测试基建为准：若已有 encrypt/decrypt 测试辅助（固定测试密钥），直接用它构造加密态样本做全字段断言；没有则本 Task 测试降级为「非敏感字段透传」+ 依赖 Phase 3 集成测试覆盖解密链路，注释说明。

- [x] **Step 4：`cargo test --lib pkg::credential` 通过 + 提交** `git commit -m "feat(credential): credential module skeleton with decrypt"`。

### Task 2.2：增强器 trait + 三个内置增强器 + OAuthTokenManager

**Files:**
- Create: src/pkg/credential/enhancer.rs
- Modify: src/pkg/credential/mod.rs（`mod enhancer; pub use enhancer::*;`）

- [x] **Step 1：实现**（核心代码，enhancer.rs）：

```rust
// ==================== 凭据增强器 ====================

/// 增强结果（不同增强器返回形态不同，枚举拓展；v1 单值）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialEnhancedValue {
    Value(String),
}

/// 凭据增强器：原始凭据衍生行为的封装（pkg 实现、自包含）
#[async_trait]
pub trait CredentialEnhancer: Send + Sync {
    fn kind(&self) -> CredentialEnhancerKind;
    /// 可用性判定（矩阵单点 common::enhancer_supports，D12）
    fn supports(&self, kind: CredentialKind) -> bool {
        common::models::enhancer_supports(kind, self.kind())
    }
    /// 执行增强：入参凭据为解密后明文态（多字段拼接上下文即凭据自身）
    async fn enhance(&self, credential: &ResolvedCredential) -> Result<CredentialEnhancedValue>;
}

/// "Bearer " + 规范可用值（包裹规则 D10）
pub struct BearerTokenEnhancer;

#[async_trait]
impl CredentialEnhancer for BearerTokenEnhancer {
    fn kind(&self) -> CredentialEnhancerKind { CredentialEnhancerKind::BearerToken }
    async fn enhance(&self, credential: &ResolvedCredential) -> Result<CredentialEnhancedValue> {
        let canonical = credential.canonical_value(None).await?;
        Ok(CredentialEnhancedValue::Value(format!("Bearer {}", canonical)))
    }
}

/// "Basic " + base64(username:password)
pub struct BasicAuthEnhancer;

#[async_trait]
impl CredentialEnhancer for BasicAuthEnhancer {
    fn kind(&self) -> CredentialEnhancerKind { CredentialEnhancerKind::BasicAuth }
    async fn enhance(&self, credential: &ResolvedCredential) -> Result<CredentialEnhancedValue> {
        let CredentialDetail::UserPassword { username, password } = &credential.detail else {
            return Err(bail_err!(InvalidRequest, "basic_auth enhancer requires user_password credential"));
        };
        let encoded = base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{}", username, password));
        Ok(CredentialEnhancedValue::Value(format!("Basic {}", encoded)))
    }
}

/// OAuth refresh → access_token（§2.5.1 生命周期内聚）
pub struct AccessTokenEnhancer;

#[async_trait]
impl CredentialEnhancer for AccessTokenEnhancer {
    fn kind(&self) -> CredentialEnhancerKind { CredentialEnhancerKind::AccessToken }
    async fn enhance(&self, credential: &ResolvedCredential) -> Result<CredentialEnhancedValue> {
        let CredentialDetail::OAuth { .. } = &credential.detail else {
            return Err(bail_err!(InvalidRequest, "access_token enhancer requires oauth credential"));
        };
        let token = oauth_token_manager()
            .get_access_token(&credential.credential_id, &credential.detail).await?;
        Ok(CredentialEnhancedValue::Value(token))
    }
}

/// 增强器注册表（OnceLock；本模块内建三增强器，未来扩展在此注册）
fn enhancer_for(kind: CredentialEnhancerKind) -> Result<&'static dyn CredentialEnhancer> {
    static REGISTRY: OnceLock<Vec<&'static dyn CredentialEnhancer>> = OnceLock::new();
    let registry = REGISTRY.get_or_init(|| {
        vec![
            &BearerTokenEnhancer as &'static dyn CredentialEnhancer,
            &BasicAuthEnhancer,
            &AccessTokenEnhancer,
        ]
    });
    registry
        .iter()
        .find(|e| e.kind() == kind)
        .copied()
        .ok_or_else(|| bail_err!(InvalidRequest, "unknown credential enhancer kind"))
}
```

- [x] **Step 2：OAuthTokenManager**（同文件追加；错误不含 token 值）：

```rust
// ==================== OAuth refresh → access_token（TTL 缓存，D13） ====================

use std::collections::HashMap;
use std::time::{Duration, Instant};

struct CachedToken { token: String, expires_at: Instant }

/// OAuthTokenManager：AccessToken 增强器内部引擎
/// 命中且剩余 > 60s 直返；miss/将过期则 SSRF 校验 → POST refresh → 缓存（提前 60s 过期）
pub struct OAuthTokenManager {
    cache: std::sync::Mutex<HashMap<String, CachedToken>>,
}

fn oauth_token_manager() -> &'static OAuthTokenManager {
    static MANAGER: OnceLock<OAuthTokenManager> = OnceLock::new();
    MANAGER.get_or_init(|| OAuthTokenManager { cache: std::sync::Mutex::new(HashMap::new()) })
}

const TOKEN_CACHE_SAFETY_MARGIN: Duration = Duration::from_secs(60);
const TOKEN_REFRESH_TIMEOUT: Duration = Duration::from_secs(30);

impl OAuthTokenManager {
    pub async fn get_access_token(
        &self,
        credential_id: &str,
        detail: &CredentialDetail,
    ) -> Result<String> {
        let CredentialDetail::OAuth { token_endpoint, client_id, client_secret, refresh_token, scope } =
            detail
        else {
            return Err(bail_err!(InvalidRequest, "oauth token refresh requires oauth credential"));
        };

        // 缓存命中且剩余 > 60s 直返
        {
            let cache = self.cache.lock().unwrap();
            if let Some(cached) = cache.get(credential_id)
                && cached.expires_at.duration_since(Instant::now()) > TOKEN_CACHE_SAFETY_MARGIN
            {
                return Ok(cached.token.clone());
            }
        }

        // SSRF 校验 + DNS pin（复用 http.rs 同款模式；拒绝内网/环路）
        let url = reqwest::Url::parse(token_endpoint)
            .map_err(|_| bail_err!(InvalidRequest, "invalid oauth token_endpoint"))?;
        let pinned = crate::pkg::tool_registry::tool_security::validate_target_url(
            None, None, None, &url,
        ).await?;

        let mut form = vec![
            ("grant_type", "refresh_token"),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("refresh_token", refresh_token.as_str()),
        ];
        if let Some(scope) = scope { form.push(("scope", scope.as_str())); }

        let host = url.host_str().unwrap_or_default().to_string();
        let client = reqwest::Client::builder()
            .timeout(TOKEN_REFRESH_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .resolve_to_addrs(&host, &pinned)
            .build()
            .map_err(|_| bail_err!(InternalError, "failed to build oauth refresh client"))?;

        let response = client.post(url).form(&form).send().await
            .map_err(|_| bail_err!(InternalError, "oauth token refresh request failed"))?;
        if !response.status().is_success() {
            // 刷新失败不缓存失败结果（D13）
            return Err(bail_err!(InternalError, "oauth token refresh returned non-success status"));
        }
        let body: serde_json::Value = response.json().await
            .map_err(|_| bail_err!(InternalError, "oauth token refresh response parse failed"))?;
        let token = body.get("access_token").and_then(|v| v.as_str()).map(String::from)
            .ok_or_else(|| bail_err!(InternalError, "oauth token refresh response missing access_token"))?;
        let expires_in = body.get("expires_in").and_then(|v| v.as_u64()).unwrap_or(0);

        // 提前 60s 过期写缓存
        let expires_at = Instant::now()
            + Duration::from_secs(expires_in)
            .saturating_sub(TOKEN_CACHE_SAFETY_MARGIN);
        self.cache.lock().unwrap().insert(
            credential_id.to_string(),
            CachedToken { token: token.clone(), expires_at },
        );
        Ok(token)
    }
}
```

> `resolve_to_addrs` 需要 host:port 形态——照 http.rs L149-L157 现网写法对齐（含 port 提取）；错误码变体以 common::error 实际为准。

- [x] **Step 3：测试**（内嵌 mod tests，mock endpoint 用 `TcpListener` 本地起 HTTP 线程——模式照 http_tests.rs；SSRF 用例注意：validate_target_url 默认拒绝内网 → mock endpoint 是 127.0.0.1 会被拒！**测试策略**：TokenManager 单测直接测缓存逻辑（预填缓存命中/过期刷新分支经注入 form 的方式难以绕过 SSRF）——改为：缓存命中/提前过期用「预填 cache」测；刷新链路（SSRF 拒绝）用内网地址断言 Err。真实刷新链路放集成路径（allow_local_network 分支不在本 API 暴露，属可接受覆盖缺口，注释说明）：

```rust
#[tokio::test]
async fn token_manager_cache_hit_returns_without_refresh() {
    let mgr = OAuthTokenManager { cache: std::sync::Mutex::new(HashMap::new()) };
    mgr.cache.lock().unwrap().insert("c1".into(), CachedToken {
        token: "at_x".into(),
        expires_at: Instant::now() + Duration::from_secs(120),
    });
    let detail = CredentialDetail::OAuth {
        token_endpoint: "https://example.invalid/token".into(),
        client_id: "cid".into(), client_secret: "cs".into(),
        refresh_token: "rt".into(), scope: None,
    };
    // 命中：token_endpoint 不可达也不会发起请求
    assert_eq!(mgr.get_access_token("c1", &detail).await.unwrap(), "at_x");
}

#[tokio::test]
async fn token_manager_expired_cache_triggers_refresh() {
    // 预填已过期缓存 → 走刷新 → token_endpoint 为不可达外网地址 → Err（且失败不缓存）
    ...
}

#[tokio::test]
async fn token_manager_rejects_local_network_endpoint() {
    // token_endpoint = http://127.0.0.1:1/token → SSRF 拒绝 Err
    ...
}
```

- [x] **Step 4：测试通过 + 提交** `git commit -m "feat(credential): credential enhancers with oauth token manager"`。

### Task 2.3：ResolvedCredential + resolve_requirements 纯函数 + validate_requirements

**Files:**
- Modify: src/pkg/credential/mod.rs（追加）

- [x] **Step 1：实现**：

```rust
// ==================== 凭据对象（代理增强） ====================

/// 解析后的凭据对象：detail 明文态 + 派生属性 + 默认增强器装配（D7/D24）
pub struct ResolvedCredential {
    credential_id: String,
    detail: CredentialDetail,
    attributes: std::collections::BTreeMap<String, String>,
}

impl ResolvedCredential {
    /// detail 明文态（供增强器取多字段上下文）
    pub fn detail(&self) -> &CredentialDetail { &self.detail }

    /// 获取指定增强器的结果（代理执行；supports 不匹配 → 错误）
    pub async fn enhance(&self, kind: CredentialEnhancerKind) -> Result<CredentialEnhancedValue> {
        let enhancer = enhancer_for(kind)?;
        if !enhancer.supports(self.detail.kind()) {
            return Err(bail_err!(InvalidRequest, "credential enhancer not supported for this credential kind"));
        }
        enhancer.enhance(self).await
    }

    /// 规范可用值（D6）：复合形态走默认增强器；单值 kind 查找链（D24）：
    /// detail 字段 → attributes 派生属性 → primary_secret
    pub async fn canonical_value(&self, field: Option<&str>) -> Result<String> {
        match (self.detail.kind(), field) {
            // 显式选择默认增强器幂等等价 None（D11）由取值侧归一，此处只按 kind 分派
            (CredentialKind::OAuth, None) => {
                Ok(match self.enhance(CredentialEnhancerKind::AccessToken).await? {
                    CredentialEnhancedValue::Value(v) => v,
                })
            }
            (CredentialKind::UserPassword, None) => {
                Ok(match self.enhance(CredentialEnhancerKind::BasicAuth).await? {
                    CredentialEnhancedValue::Value(v) => v,
                })
            }
            (_, Some(field_name)) => self.extract_field(field_name),
            _ => Ok(self.detail.primary_secret().to_string()),
        }
    }

    /// 字段提取（serde JSON 泛化取 detail 字段，miss 则查 attributes，D24 查找链）
    fn extract_field(&self, field: &str) -> Result<String> {
        let value = serde_json::to_value(&self.detail)
            .map_err(|_| bail_err!(InternalError, "failed to serialize credential detail"))?;
        if let Some(v) = value.get(field).and_then(|v| v.as_str()) {
            return Ok(v.to_string());
        }
        self.attributes.get(field).cloned()
            .ok_or_else(|| bail_err!(InvalidRequest, "credential field not found: {}", field))
    }
}

// ==================== 纯函数加工入口（D17：凭据由编排层传入） ====================

/// 单条 requirement 的最终注入值
pub struct ResolvedRequirement {
    pub requirement: CredentialRequirement,
    /// 注入值（增强器包裹 canonical 的结果，D10）
    pub value: String,
}

/// 编排层传入的凭据条目（dal 生产：credential_id + detail + 派生属性）
pub struct FetchedCredential {
    pub credential_id: String,
    /// lark dal 生产路径为明文（already_decrypted=true）；
    /// user dal 为 DB 加密态（false）；无其他生产路径（tavily 兜底已随 D27 废除）
    pub detail: CredentialDetail,
    pub already_decrypted: bool,
    /// dal 派生属性（D24：lark 的 identity_mode 等；user dal 生产路径为空集）
    pub attributes: std::collections::BTreeMap<String, String>,
}

/// 纯函数加工：requirements 与编排层生产的凭据按序配对 → 逐条（按需）解密 + 取注入值。
/// 零数据访问——条目由 domain resolve_tool_credentials 传入；
/// 长度不匹配（编排层已保证逐条对应）→ 防御性错误。
pub async fn resolve_requirements(
    requirements: &[CredentialRequirement],
    fetched: &[FetchedCredential],
) -> Result<Vec<ResolvedRequirement>> {
    if requirements.len() != fetched.len() {
        return Err(bail_err!(InternalError, "credential requirements/fetched length mismatch"));
    }
    let mut resolved = Vec::with_capacity(requirements.len());
    for (requirement, fc) in requirements.iter().zip(fetched) {
        let detail = if fc.already_decrypted {
            fc.detail.clone()
        } else {
            decrypt_detail(fc.detail.clone())?
        };
        let credential = ResolvedCredential {
            credential_id: fc.credential_id.clone(),
            detail,
            attributes: fc.attributes.clone(),
        };
        let value = match requirement.enhancer {
            Some(kind) => match credential.enhance(kind).await? {
                CredentialEnhancedValue::Value(v) => v,
            },
            None => credential.canonical_value(requirement.field.as_deref()).await?,
        };
        resolved.push(ResolvedRequirement { requirement: requirement.clone(), value });
    }
    Ok(resolved)
}

/// 缺凭据结构化引导（复用 tavily 引导形态，D19）
pub fn credential_missing_json(requirement: &CredentialRequirement) -> serde_json::Value {
    let kind_desc = match &requirement.platform {
        Some(platform) => format!("{:?}/{}", requirement.kind, platform),
        None => format!("{:?}", requirement.kind),
    };
    crate::pkg::tool_registry::tool_readiness::api_key_missing_json(
        &format!("工具需要 {} 凭据，但当前用户与组织均无可解析凭据", kind_desc),
        "绑定个人凭据（设置 → 身份凭证）并设为默认，或由管理员配置组织共享默认凭据",
    )
}
```

> `format!("{:?}")` 的 kind 展示改用更友好的 `kind.as_str()`——若 `CredentialKind` 无 `as_str`，在 common 加一个（六值 match，snake_case）。

- [x] **Step 2：validate_requirements**（同文件）：

```rust
// ==================== 配置期校验（§2.1 校验清单单点） ====================

use common::models::{CredentialBinding, CredentialRequirementScope};

/// binding ↔ 作用域匹配
fn binding_allowed(binding: &CredentialBinding, scope: CredentialRequirementScope) -> bool {
    matches!(
        (binding, scope),
        (CredentialBinding::Env { .. }, CredentialRequirementScope::McpStdio)
            | (CredentialBinding::Header { .. }, CredentialRequirementScope::McpHttp)
            | (CredentialBinding::Header { .. }, CredentialRequirementScope::HttpTool)
            | (CredentialBinding::Query { .. }, CredentialRequirementScope::HttpTool)
            | (CredentialBinding::Internal { .. }, CredentialRequirementScope::Builtin)
    )
}

/// requirements 配置期校验（创建/更新 handler 与前端预校验同一套规则）
pub fn validate_requirements(
    requirements: &[CredentialRequirement],
    scope: CredentialRequirementScope,
) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for req in requirements {
        // 1. binding ↔ 协议
        if !binding_allowed(&req.binding, scope) {
            return Err(bail_err!(InvalidRequest, "credential binding does not match tool protocol"));
        }
        // 2. platform ↔ kind（generic 类必填、专用必空，D3）
        if req.kind.requires_platform() != req.platform.is_some() {
            return Err(bail_err!(InvalidRequest, "credential platform must be set exactly for generic credential kinds"));
        }
        // 3. field ↔ enhancer 互斥（D8）
        if req.field.is_some() && req.enhancer.is_some() {
            return Err(bail_err!(InvalidRequest, "credential field and enhancer are mutually exclusive"));
        }
        // 4. enhancer ↔ kind supports 矩阵（D12；专用 kind 零支持）
        if let Some(enhancer) = req.enhancer
            && !common::models::enhancer_supports(req.kind, enhancer)
        {
            return Err(bail_err!(InvalidRequest, "credential enhancer not supported for this credential kind"));
        }
        // 5. (kind, platform, 注入点名) 三元组去重（D10）
        let key = (
            req.kind,
            req.platform.clone(),
            binding_name(&req.binding).to_string(),
        );
        if !seen.insert(key) {
            return Err(bail_err!(InvalidRequest, "duplicate credential requirement for same injection point"));
        }
        // 注入名非空
        if binding_name(&req.binding).trim().is_empty() {
            return Err(bail_err!(InvalidRequest, "credential binding name must not be empty"));
        }
    }
    Ok(()) // 显式选择默认增强器幂等允许（D11），无校验分支
}

fn binding_name(binding: &CredentialBinding) -> &str {
    match binding {
        CredentialBinding::Env { name }
        | CredentialBinding::Header { name }
        | CredentialBinding::Query { name } => name,
        CredentialBinding::Internal { field } => field,
    }
}
```

- [x] **Step 3：测试**（内嵌 mod tests，纯逻辑无需 DB）：

```rust
fn req(kind: CredentialKind, platform: Option<&str>, field: Option<&str>,
       enhancer: Option<CredentialEnhancerKind>, binding: CredentialBinding) -> CredentialRequirement { ... }

#[test]
fn validate_rejects_env_binding_outside_stdio() { ... }        // Env + HttpTool → Err
#[test]
fn validate_rejects_query_binding_for_mcp() { ... }            // Query + McpStdio/McpHttp → Err
#[test]
fn validate_internal_binding_only_for_builtin() { ... }        // Internal + Builtin → Ok；Internal + HttpTool/McpStdio → Err（D25）
#[test]
fn validate_rejects_platform_mismatch() { ... }                // generic_token 无 platform / lark_app 带 platform → Err
#[test]
fn validate_rejects_field_and_enhancer_both_set() { ... }      // D8
#[test]
fn validate_rejects_enhancer_kind_mismatch() { ... }           // github_token + bearer_token → Err（专用 kind 零支持）
#[test]
fn validate_accepts_explicit_default_enhancer() { ... }        // oauth+access_token / user_password+basic_auth → Ok（D11 幂等）
#[test]
fn validate_rejects_duplicate_injection_point() { ... }        // 同 (kind,platform,name) 两条 → Err
#[test]
fn validate_allows_same_credential_different_bindings() { ... } // 同 key 不同注入名 → Ok（access_token env + Bearer header 场景）
```

增强/取值测试（构造 ResolvedCredential 需要暴露 test 构造器 `#[cfg(test)] fn new_for_test(detail) -> Self`）：

```rust
#[tokio::test]
async fn canonical_oauth_returns_access_token() { ... }        // mock：预填 TokenManager 缓存 → canonical = 缓存 token
#[tokio::test]
async fn canonical_user_password_returns_basic_string() { ... } // "Basic " + base64("alice:pw")
#[tokio::test]
async fn canonical_generic_token_returns_raw_token() { ... }
#[tokio::test]
async fn canonical_field_extraction() { ... }                   // lark_app + field=app_id → app_id 值
#[tokio::test]
async fn canonical_field_falls_back_to_attributes() { ... }     // attributes{identity_mode} + field=identity_mode → 属性值（D24 查找链）
#[tokio::test]
async fn bearer_wraps_canonical() { ... }                       // oauth + BearerToken = "Bearer " + access_token（D10 包裹）
#[tokio::test]
async fn bearer_wraps_generic_token() { ... }                   // "Bearer " + ntn_xxx
#[tokio::test]
async fn enhance_rejects_unsupported_kind() { ... }             // generic_token + BasicAuth → Err

// resolve_requirements 纯函数（配合 pkg::crypto 测试密钥基建构造加密态样本；
// 无密钥基建时以 GenericToken 单字段为最小验证路径）
#[tokio::test]
async fn resolve_requirements_pairs_and_derives_values() { ... } // generic_token + user_password 两条 → token 原文 + Basic 串
#[tokio::test]
async fn resolve_requirements_skips_decrypt_for_plaintext() { ... } // already_decrypted=true 明文直取（lark 生产路径）
#[tokio::test]
async fn resolve_requirements_rejects_length_mismatch() { ... }  // requirements 2 条 / fetched 1 条 → Err
```

- [x] **Step 4：全绿 + 提交** `git commit -m "feat(credential): resolved credential with pure resolve_requirements/validation"`。

**Phase 2 收口检查：**
- [x] `cargo test --lib pkg::credential` + clippy 全绿（全纯函数，无 DB / 无 OnceLock 注入）。
- [x] 模块 grep 确认：无 `RequestContext` / 无 `CredentialDataProvider` / 无 `set_` 注入注册（OAuthTokenManager 内部 OnceLock 缓存除外）。

---

## Phase 3：MCP 集成（config 删 env/headers + CoreTool 生命周期 + 编排注入 + 连接隔离 + 调用入口统一 D26）

### Task 3.1：McpServerConfig / DTO 字段替换

**Files:**
- Modify: src/models/mcp_server.rs（L101-L167）
- Modify: common/src/api/mcp_server.rs（L12-L32）
- Modify: src/handlers/finance/mcp_server/response.rs
- Test: src/models/mcp_server_test.rs、src/handlers/finance/mcp_server/response_test.rs、list_mcp_servers_test.rs

- [x] **Step 1：改测试先行**——三个测试文件中删除 env/headers 构造与断言，新增 `credential_requirements` 字段断言（`#[serde(default)]` 空数组序列化省略：`skip_serializing_if = "Vec::is_empty"`；反序列化旧数据无该字段 → 空数组）。
- [x] **Step 2：Model 改造**：

```rust
pub struct McpServerConfig {
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// streamable HTTP URL（保留 URL 脱敏）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// 凭据需求声明（唯一注入来源；env/headers 字段已移除，D14）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credential_requirements: Vec<CredentialRequirement>,
    pub timeout_ms: u64,
    pub connect_timeout_ms: u64,
    pub response_max_bytes: u64,
}
```

  - `default_stdio()` / `default_streamable_http()`：`credential_requirements: Vec::new()`，删 env/headers。
  - `redacted_for_management()`：删 env/headers 打码段；requirements 非敏感直接保留；URL 脱敏逻辑不动。
  - 删除 `use std::collections::BTreeMap;`（若无他用）。
- [x] **Step 3：DTO 同步**——`McpServerConfigDto` 删 `env`/`headers`（含 `BTreeMap` import），增：

```rust
/// Credential requirements (type-level declarations, non-sensitive).
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub credential_requirements: Vec<common::models::CredentialRequirement>,
```

  注意：`CredentialRequirement` 已有 `JsonSchema`（Task 1.2），DTO derive 不受影响。
- [x] **Step 4：response.rs 转换**——`to_model_config` / `to_config_dto` 删 env/headers 映射，补 `credential_requirements: config.credential_requirements.clone()`。
- [x] **Step 5：handler 校验接入**——create_mcp_server.rs / update_mcp_server.rs 在 `to_model_config` 后：

```rust
let model_config = to_model_config(params.config);
crate::pkg::credential::validate_requirements(
    &model_config.credential_requirements,
    match transport {
        McpTransport::Stdio => common::models::CredentialRequirementScope::McpStdio,
        McpTransport::StreamableHttp => common::models::CredentialRequirementScope::McpHttp,
    },
)?;
```

  （update 的 transport 分支：取更新后生效的 transport——`params.transport.map(to_model_transport).unwrap_or(server.po.transport)`。）
- [x] **Step 6：`cargo test --lib mcp_server` + 修复所有编译波及（前端 mcp_servers.rs 构造 `McpServerConfigDto` 用 `..Default::default()` 不受字段删除影响，若显式构造则同步删）+ 提交** `git commit -m "feat(mcp): replace env/headers with credential_requirements"`（commit 4e9255f6）。

### Task 3.2：CoreTool 生命周期方法 + MCP 运行时注入 + 连接隔离

**Files:**
- Modify: src/models/tool.rs（CoreTool trait）
- Modify: src/pkg/tool_registry/mcp.rs
- Modify: src/service/dao/tool_call/mcp.rs（`list_mcp_tools(ctx, server)` trait + impl）
- Test: src/pkg/tool_registry/mcp_tests.rs

- [x] **Step 1：CoreTool trait 增生命周期方法（D22，默认实现让 fs_read/shell_exec 等零改动）**：

```rust
/// 工具凭据需求声明（共享工具从实例 config 读；内置工具静态声明；默认空）
fn credential_requirements(&self) -> Vec<common::models::CredentialRequirement> {
    Vec::new()
}

/// 凭据注入生命周期：编排层在 call 前调用——校验声明与注入匹配 + 存实例字段。
/// 实例单次使用（create → check → call），凭据是对象状态（D22 红线）。
fn check(
    &mut self,
    resolved: &[crate::pkg::credential::ResolvedRequirement],
) -> common::error::Result<()> {
    Ok(())
}
```

- [x] **Step 2：McpCoreTool 实现生命周期 + env 注入重构**：
  - 新增实例字段：`credential_injections: Vec<(String, String)>`（check 写入的 Env 注入值）+ `user_scope: Option<String>`（连接隔离键的用户维度，check 时从注入条目附带的 user 信息写入——编排层 resolved 里无 user，改由 check 签名追加 `user_id: Option<&str>` 参数或实例创建时注入；**取实例创建时由工厂/dal 写入**——assemble 处持有 ctx）。
  - `credential_requirements()`：`self.server_config.credential_requirements.clone()`。
  - `check(resolved)`：逐条 match binding（Env → 收集 `(name, value)`；非 Env → 防御性 Err，配置期已校验）；存 `credential_injections`。
  - `call` 内删除取数逻辑；`connect_stdio_client(server, config, env_injections)`：删除 L214-216 静态 env 循环，改遍历 `credential_injections`；`process.env_clear()` 保持（红线 6）。
- [x] **Step 3：连接隔离（D23）**——`McpClientRuntime` 连接缓存键：`config.credential_requirements` 非空 → `(server_id, user_scope)`（实例维度）；空 → `server_id`（现状全局共享）。`invalidate_mcp_server(server_id)` 同步适配：清除该 server 前缀匹配的全部键。
- [x] **Step 4：tools 同步路径（D18）**——`dao/tool_call/mcp.rs` trait + impl `list_mcp_tools(&self, ctx, server)`：domain sync 编排先 `resolve_tool_credentials`（Task 3.3）→ 命中则构造注入值调用 `list_stdio_tools(server, env_injections)`；未命中 → `Err(InvalidRequest, 引导文案)`（同步是管理动作，可读错误优于静默空列表）。
- [x] **Step 5：测试**：
  - 改写存量 env 用例（`mcp_server_with_command` 等）：config 无 env 字段后构造调整。
  - 新增 `stdio_tool_injects_resolved_env`：本地 echo 脚本 MCP（既有 `write_echo_mcp_server_script` 模式）+ requirements 声明（generic_token/Env）+ **实例 check 注入**（测试直接构造 resolved 传入，不经 DB——取数链路在 Task 3.3 domain 测试覆盖）→ 子进程回显 env 值断言。
  - 新增 `missing_credential_returns_guidance_json`（编排层 None → 引导；本 Task 测 pkg 侧 check 前 requirements 非空且 resolved 空 → 防御 Err 路径，端到端引导在 Task 3.3）。
  - 新增 `connection_key_isolated_per_user`：同 server 两实例不同 user_scope → runtime 连接键不同（单测连接键生成函数，不起真实子进程）。
- [x] **Step 6：全绿 + 提交** `git commit -m "feat(mcp): core tool lifecycle with per-user connection isolation"`（commit 07dd7cf4）。

### Task 3.3：domain 编排 resolve_tool_credentials + 调用入口统一（D26）+ 内置三员工厂化 + Dal resolver 三删

**Files:**
- Modify: src/service/dal/lark.rs → 拆分为 src/service/dal/lark/ 文件夹（Step 0：trait 体系 + 改名，见下）
- Modify: common/src/config.rs（Step 0b D27：删 TavilyConfig 段；Step 1b D28：删 BrowserConfig 段——全局 config 零工具参数不变式达成）
- Modify: src/pkg/tool_registry/tavily_search.rs（Step 0b D27：timeout 挪 PO config + 单轨化）
- Modify: src/pkg/tool_registry/browser.rs / gh_cli.rs / lark_cli.rs（Step 1b D28：CLI 命令与行为参数挪 PO config，spawn 读实例 PO）
- Modify: src/pkg/tool_registry/tool_readiness.rs（Step 1b D28：probe 注册表机制消亡，仅留纯函数）
- Modify: src/handlers/finance/tool/response.rs（Step 1b D28：readiness 调用点切 domain）
- Modify: src/service/domain/finance/identity_credential.rs + common/src/api/tavily_integration.rs + frontend/src/pages/finance/identity_tavily.rs + tests/integration/web_search_tool_test.rs（Step 0b D27：shared_key_configured 全链删除）
- Modify: src/service/domain/runtime/tool_execution.rs（新增 `resolve_tool_credentials`——落位 runtime 而非 finance domain：唯一消费方是 `call_tool` 编排，内部只向下调注入的 DAL 端口 / pkg 纯函数，零 finance domain 依赖，规避 domain 同层互调；`call_tool` 增凭据编排 + 新增 `dispatch_manual_tool`）
- Modify: src/models/tool.rs（新增 `ToolExecutionRequest`，D26 统一传参结构）
- Modify: src/service/domain/runtime/mod.rs（RuntimeToolExecution trait 增 `dispatch_manual_tool` 声明；RuntimeDomainImpl 增 `user_dal` / `lark_credentials` 字段——依赖注入，见 Step 1）
- Modify: src/service/domain/runtime/think_loop.rs（control_mode 分发切换 domain，D26 入口统一）
- Modify: src/service/dal/tool.rs（`call_tool` 改 ToolExecutionRequest 签名 + Builtin per-call 重组装 + check；删 `call_tool_by_id` / `execute_auto` / `execute_manual`）
- Modify: src/service/dal/mcp_tool.rs（`call_tool` 改 ToolExecutionRequest 签名 + assemble 后 check；删 `call_tool_by_id`）
- Modify: src/pkg/tool_registry/gh_cli.rs / tavily_search.rs / lark_cli.rs（删 trait + 工厂化，v1.5 Lark 统一）
- Modify: src/pkg/tool_registry/mod.rs（`ToolRegistry::credential_requirements(&self, po)` 辅助：Builtin 查 factory 声明 / Mcp·Http 从 config 解析；**Step 1b 建**（readiness 先用），Step 2 call_tool 编排复用）
- Modify: src/service/dal/user.rs（删 Gh/Tavily Dal resolver）
- Modify: src/service/mod.rs（init 删 Gh/Tavily/Lark 三行注册）
- Modify: src/handlers/finance/tool/request_tool_call.rs / send_tool_call_message.rs（doc 注释 execute_manual → dispatch_manual_tool，逻辑零改动）
- Test: gh_cli / tavily / lark_cli 既有测试核对 + domain 内嵌测试 + tool_execution_test / mcp_tool_test / think_loop 相关测试签名迁移 + lark_test.rs 迁移

- [x] **Step 0：lark DAL trait 体系化 + 改名（前置重构，独立提交）**（commit a22eede3：`find_channel_by_lark_identity` 归 credentials（渠道定位对称）、`base()` 归 listener（运行面透传）；consumer/adapter 注册处维持具体类型 `Arc<LarkDalImpl>`（`adapt_lark` 固有方法需要），finance domain 持 `Arc<dyn LarkDal>`；`LarkDalCredentialResolver` 落位 credentials.rs）——现状 `LarkMessageChannelDal` 是无 trait 具体类型，且名不副实（不止 message channel）：
  - **文件夹化** [src/service/dal/lark/](../../../src/service/dal/lark/)（原单文件 850 行拆分）：
    - `mod.rs`：总 trait `LarkDal: LarkCredentialDal + LarkListenerDal`（组合子 trait）+ 单例 `dal()` / `init()` / `new*()` 构造；
    - `credentials.rs`：子 trait `LarkCredentialDal`——`resolve_credentials_for_user` / `resolve_channel_app_id` / `find_channels_by_credential_id`（消费方：runtime domain 凭据编排、finance domain 凭证删除联动）；
    - `listener.rs`：子 trait `LarkListenerDal`——`ensure_listener_for` / `release_listener_if_unused` / `sync_listener_for_channel` / `release_listener_for_channel` / `handover_listeners_after_credential_change` / `listener_stats`（消费方：finance domain 渠道联动、凭证变更联动）；
    - `impl.rs`：`LarkDalImpl`（原 `LarkMessageChannelDal` **改名**）实现全部子 trait；`adapt_lark` / `MessageInboundAdapter` / `LarkAdapterHandler` 入站适配段留在 impl（已通过 pkg `MessageInboundAdapter` trait 暴露，无需重复建口）。
  - **消费方类型切换**：finance domain 字段 `Option<Arc<LarkMessageChannelDal>>` → `Option<Arc<dyn LarkDal>>`（凭据 + 监听器两块都用，取总 trait）；consumer / adapter 注册处取 `Arc<dyn LarkDal>`（或维持 `LarkDalImpl` 具体类型——`LarkAdapterHandler` 内部组合非对外接口，两可，实现时以改动最小为准）；lark_test.rs 引用同步。
  - 纯结构重构零行为变化：`cargo test --lib` + `make test-be` 全绿后独立提交 `git commit -m "refactor(lark): split lark dal into folder with credential/listener sub-traits"`。

- [x] **Step 0b：tavily 共享 config 兜底废除（D27，前置独立提交）**（commit 12ebdbd1：附带删除共享兜底配套的 `ApiKeySource` 枚举与响应 `key_source` 字段——等价处理；集成测试文件已改未跑，留待最后统一验证）——全局 config `[tavily]` 是双轨授权时期遗留（[web_search_and_browser_tools_design.md](../design/web_search_and_browser_tools_design.md)），与「凭据统一走凭证库」相悖；共享 key 挪 tool PO config 同样违反 D1（凭据原文落组织共享配置 = 全员可见）：
  - **[common/src/config.rs](../../../common/src/config.rs#L629-L655)**：`TavilyConfig` 段整体删除（api_key + timeout_ms），`Config` struct `tavily` 字段移除，`ai_orz.toml` 模板/示例 `[tavily]` 段删除（有则删）。
  - **[tavily_search.rs](../../../src/pkg/tool_registry/tavily_search.rs)**：`resolve_api_key` 双轨改单轨（仅 `TavilyCredentialResolver`，共享分支 L90-L93 删除；本 Step 先保留双轨形态中用户凭证一支，完整 check 注入化在 Step 3 lark_cli/tavily 段落地）；timeout `get().tavily.timeout_ms`（L229）→ 读工具 PO config `config.timeout_ms` 缺省 15_000 常量（存量 PO config=Null 零迁移）；`API_KEY_MISSING_ERROR` / `API_KEY_MISSING_GUIDANCE` 文案改单路径（删「或由管理员…配置共享 key」）；L393 测试条件与断言同步。
  - **[tool_readiness.rs](../../../src/pkg/tool_registry/tool_readiness.rs#L180-L210)**：TavilyKeyProbe 删共享 config 探测分支（L193-L195），暂保留 resolver 分支（探测机制整体重构在 Step 1b，届时 TavilyKeyProbe 消亡——resolver 删除前的过渡态）；probe hint 文案改单路径。
  - **[identity_credential.rs](../../../src/service/domain/finance/identity_credential.rs#L387)**：`tavily_integration_status` 删 `shared_key_configured` 字段；[common/src/api/tavily_integration.rs](../../../common/src/api/tavily_integration.rs#L92) DTO 字段删除；[frontend identity_tavily.rs](../../../frontend/src/pages/finance/identity_tavily.rs#L150)（L150-L152 / L235-L241 两处分支）同步删（提示语单路径）；[web_search_tool_test.rs](../../../tests/integration/web_search_tool_test.rs) 断言删该字段。
  - 行为变化：未绑个人 key 的用户 tavily_search 从「共享兜底可用」变「api_key_missing 引导」——预期内，与 GithubToken 行为对齐。`cargo test --lib` + `make test-be` + 前端构建全绿后独立提交 `git commit -m "refactor(tavily): remove shared config key fallback, single-track user credential only (D27)"`。

- [x] **Step 1：domain `resolve_tool_credentials`（D17 编排链 ①②③ 单点，生产端二元路由；依赖注入，落位 runtime/tool_execution.rs）**（commit 518b955f：方法 pub(super) 可见性（模块树测试需要）；匿名 ctx（user_id None）提前返回 Ok(None)；FetchedCredential 补 already_decrypted 字段 + resolve_requirements 密文解密分支；a2a integration_test 补 user_credential/lark dal init）：
  - **依赖注入（禁止方法内取全局单例）**：RuntimeDomainImpl 增两个字段（与既有 tool_dal / mcp_tool_dal 同模式）：
    - `user_dal: Arc<dyn UserDal + Send + Sync>`——既有 trait，`find_default_credential(ctx, &user_id, kind, platform)`；
    - `lark_credentials: Arc<dyn LarkCredentialDal + Send + Sync>`——**Step 0 建的凭据子 trait**（[dal/lark/credentials.rs](../../../src/service/dal/lark/credentials.rs)，`LarkDalImpl` 实现；runtime 只消费凭据面，按需取子 trait 不持总 trait）。
    - 构造注入：生产 `new()` 取 `user::dal()` / `lark::dal()` 单例注入（与 tool_dal 现状一致）；测试 `new_with_tool_dals` / `new_with_all` 增参注入 stub；`Clone` / `Debug` impl 同步补字段。

```rust
impl RuntimeDomainImpl {
    /// 工具调用编排取数：生产路由 → pkg 纯函数加工；任一未命中 → None（调用方出引导）
    async fn resolve_tool_credentials(
        &self,
        ctx: &RequestContext,
        requirements: &[CredentialRequirement],
    ) -> Result<Option<Vec<ResolvedRequirement>>> {
        if requirements.is_empty() {
            return Ok(Some(Vec::new()));
        }
        let mut fetched = Vec::with_capacity(requirements.len());
        for requirement in requirements {
            let fetched_credential = match requirement.kind {
                // 生产路由（D17 v1.5）：LarkApp 走渠道路径，附派生属性（D24）
                CredentialKind::LarkApp => {
                    self.lark_credentials.resolve_credentials_for_user(ctx).await?
                        .map(|(creds, mode)| {
                            FetchedCredential {
                                credential_id: creds.app_id.clone(), // 生产端无独立 id，以 app_id 代
                                detail: CredentialDetail::LarkApp {
                                    app_id: creds.app_id,
                                    app_secret: creds.app_secret,
                                    encrypt_key: None,
                                    verification_token: None,
                                },
                                attributes: BTreeMap::from([("identity_mode".to_string(), mode)]),
                            }
                        })
                }
                // 其余 kind 统一走 user dal find_default（tavily 纯单轨 D27，无兜底）
                _ => {
                    // self.user_dal.find_default_credential(ctx, &user_id, requirement.kind,
                    //     requirement.platform.as_deref()).await?
                    None
                }
            };
            let Some(credential) = fetched_credential else {
                return Ok(None);
            };
            fetched.push(credential);
        }
        Ok(Some(pkg::credential::resolve_requirements(requirements, &fetched).await?))
    }
}
```

  > **明文/密文标记**：lark dal 的 `resolve_credentials_for_user` 内部已解密（返回明文），user dal 返回 DB 加密态——`FetchedCredential` 增 `already_decrypted: bool` 字段（lark 分支 true / user dal false），`resolve_requirements` 内 `if !fc.already_decrypted { decrypt_detail(...)? }`。Task 2.3 的结构体定义同步补此字段。
  > **落位说明**：函数落 runtime 而非 finance domain——唯一消费方是同模块 `call_tool` 编排；依赖全部经构造注入（user_dal / lark_credentials 字段）+ pkg 纯函数 + config 读取，零 finance domain 依赖，规避 domain 同层互调红线。私有方法不进 RuntimeToolExecution trait（内部编排细节，测试经 `call_tool` 公开入口覆盖；LarkCredentialDal 子 trait 本身可独立 stub 单测）。

- [x] **Step 1b：CLI 命令与工具参数 PO config 闭环 + readiness 数据驱动重构（D28，独立提交）**（commit b6641713：browser PO config 含 install_hint 通道；browser_tool_test 集成测试从 DB tools 表 PO config 读命令验证 D28 端到端；10 个 domain readiness 单测 + 三工厂 CLI config 不变式断言）——工具是「我们包装的内置工具」，其使用的二进制名/路径与行为参数是工具自身属性，应在工具管理页可改（用户决策）；readiness 探测与凭据解析同构（需 PO config + 用户凭据两类数据），按 D17 同一哲学重构：取数上移 domain，pkg 只留纯函数。依赖 Step 1（复用 `resolve_tool_credentials`），先于 Step 4（赶在 resolver 删除前接管 tavily 探测）：
  - **三工具命令进 PO config（统一「CLI 型 = po.config.command」不变式）**：
    - [browser.rs](../../../src/pkg/tool_registry/browser.rs)：PO config 默认 `{ command: "agent-browser", timeout_ms: 60000, max_output_bytes: 262144 }`（工厂 create_po 写入默认值）；spawn 预检与命令构造（L258）读 `self.po` 的 config；timeout 缺省（L271，params.timeout_ms 优先）与 max_output_bytes（L272）同源改读；引导文案「ai_orz.toml 的 [browser].command」→「在工具配置中修改命令路径」；parameters_schema L153 描述同步。
    - [gh_cli.rs](../../../src/pkg/tool_registry/gh_cli.rs)：PO config 默认 `{ command: "gh" }`；spawn 处 `GH_CLI_BIN` 常量 → 读 `self.po` config（常量保留为缺省值来源）。
    - [lark_cli.rs](../../../src/pkg/tool_registry/lark_cli.rs)：PO config 默认 `{ command: "lark-cli" }`（实际值以 `LARK_CLI_BIN` 常量为准）；同上。
    - [ToolPo](../../../src/models/tool.rs) 增 helper：`cli_command(&self) -> Option<String>`（读 config.command，CLI 型工具判定 + 命令读取单点）；browser 行为参数读取 helper（缺省常量兜底）。存量 DB PO config=Null → 读取时缺省兜底，**零迁移**。
    - **[common/src/config.rs](../../../common/src/config.rs#L657-L692)**：`BrowserConfig` 段整体删除，`Config.browser` 字段移除——全局 config 至此零工具参数（tavily D27 / browser D28），不变式落定。
  - **readiness 数据驱动重构**：
    - **删除**（[tool_readiness.rs](../../../src/pkg/tool_registry/tool_readiness.rs)）：`ToolReadinessProbe` trait + `PROBES` 注册表 + `register_default_probes` + `BrowserCliProbe` / `FixedCliProbe` / `TavilyKeyProbe` + `user_scoped`——探测不再需要注册机制，readiness 成为 Tool 数据的纯派生。pkg 保留纯函数：`command_available` / `cli_binary_readiness` / `cli_not_installed_json` / `credential_missing_json`（引导文案常量与各工具 spawn 内文案统一来源，避免两处漂移）。
    - **新增 domain 方法**（runtime/tool_execution.rs，进 `RuntimeToolExecution` trait）：`tool_readiness(&self, ctx, tool: &Tool) -> RuntimeReady`——① `tool.po.cli_command()` 存在 → CLI 型：pkg `command_available` 判定（NotReady 附 cli_not_installed + 工具对应安装引导）；② `ToolRegistry::credential_requirements(&po)` 非空 → key 型：复用 Step 1 `resolve_tool_credentials`（`Ok(Some(_))` → Ready / `Ok(None)` → NotReady{api_key_missing} 单路径引导 / `Err` → Unknown）；③ 两者皆无 → Ready。TTL 缓存（30s，key 型按 tool|user、CLI 型按 tool）迁 domain impl 内部，语义与现状等价（生产无主动失效调用方，仅测试用 invalidate——一并迁移）。
    - **调用点切换**：[response.rs](../../../src/handlers/finance/tool/response.rs#L23) `tool_readiness::probe(&id, ctx)` → runtime domain `tool_readiness(&ctx, &tool)`（handler 已持有 Tool 实体，直接传入；domain 获取方式对照同文件其他 domain 调用先例）；[service/mod.rs](../../../src/service/mod.rs#L33) init 的 `register_default_probes()` 调用行删除（与 Step 5 的 resolver 三行删除合并后 init 凭据/探测注册清零）。
  - 测试：browser_tool_test.rs L97/L183 `get().browser.command` → PO config 缺省读取；tool_readiness.rs 内嵌测试（cache-hit / user-scoped / ttl / tavily probe）迁移改写为 domain `tool_readiness` 测试（CLI 型纯函数 + key 型经 `new_with_all` 注入 StubUserDal/StubLarkCredentialDal，无需 DB）；gh_cli / lark_cli spawn 读 PO config 既有测试核对。`cargo test --lib` + `make test-be` 全绿后独立提交 `git commit -m "refactor(tools): cli command and tool params into po config, data-driven readiness (D28)"`。

- [x] **Step 2：统一传参 ToolExecutionRequest + 调用链改造（D22 create → check → call + D26）**（commit 68ed836f：`tool` 字段落位 `ToolPo` 而非 Tool——Tool 的 Clone 是 unreachable!()，PO 载体正是 plan 注释本意；mcp_tool_test 四处 call_tool_by_id 调用迁移 call_tool 直传；execute_auto/manual 内部暂适配空 resolved request，Step 3 随 think_loop 切换删除）：
  1. [models/tool.rs](../../../src/models/tool.rs) 新增结构体（内部调用结构非 HTTP DTO 不进 common；命名规避 cortex_types 既有 `ToolCallRequest`（LLM 调用描述符），与既有 `ToolExecutionResult` 成对）：

```rust
/// domain → DAL 统一工具执行传参（D26）：tool 为 PO 载体（实例由 DAL per-call 重组装）
pub struct ToolExecutionRequest {
    pub tool: Tool,
    pub args: Value,
    pub resolved: Vec<crate::pkg::credential::ResolvedRequirement>,
}
```

  2. domain `call_tool` 编排（[tool_execution.rs](../../../src/service/domain/runtime/tool_execution.rs)）：`requirements = registry.credential_requirements(&tool.po)` → `resolve_tool_credentials(ctx, &requirements)`（Step 1）→ `None` → 直接返回 `credential_missing_json(&requirements[0])` 包装结果（不构造实例）；`Some(resolved)` → 按协议路由 `dal.call_tool(ctx, ToolExecutionRequest { tool, args, resolved })`（Mcp → mcp_tool_dal；Builtin/Http → tool_dal，两个 DAL 签名一次定形）。
  3. `ToolDal::call_tool`（Builtin/Http）：**per-call 经 registry 重组装新实例**（`tool_call_dao.assemble_core_tool(&request.tool.po)`，不复用传入 `tool.our_tool` / agent.tools() 预装配实例，D22 单次实例）→ 局部 mut 实例 `check(&request.resolved)?` → 构造 Tool → `tool_call_dao.execute(ctx, &tool, args)`（execute 签名不动，幂等防重/trace/AOP 全保留）。
  4. `McpToolDal::call_tool`：既有 `assemble_executable_tool` 已是 per-call 重组装（连接隔离 D23）→ 组装后对 `our_tool` 实例 `check(&request.resolved)?` → `mcp_tool_call_dao.execute`。
  5. **双删（D26）**：`ToolDal::call_tool_by_id` / `McpToolDal::call_tool_by_id`（trait 声明 + impl）删除——全仓无生产调用方（domain 自有 `call_tool_by_id` 保留，debug handler 在用；mcp_tool_test 调用迁移见 Step 6）。
  - `Tool` 结构中 `our_tool: Box<dyn CoreTool>` 的可变性：check 发生在实例装入 Tool 前（局部 mut / `as_mut()`），`Tool` 构造时已 check 完毕，结构无需 `mut`。
- [x] **Step 3：think_loop 切换 + dispatch_manual_tool（D26 入口统一）**（commit 4a610c0d：dispatch_manual_tool 返回 ToolExecutionResult 而非 (Value, ToolCallEntry)——与 trait 其余方法统一返回类型，语义等价；顺带删除 RuntimeDomainImpl::tool_dal() 死代码辅助方法；tool_call_test 两处集成测试迁移 domain 入口）：
  - [think_loop.rs](../../../src/service/domain/runtime/think_loop.rs) control_mode 分发（L256-L271 区域）改走 domain `tool_execution()`：Auto → `call_tool(ctx, tool, tc.arguments)`（协议路由，**顺带修复 Auto-MCP 主循环不可执行**——现状 `assemble_core_tool` 对 Mcp 返回 None 走 ManagementOnlyTool 占位）；Manual → `dispatch_manual_tool(ctx, tool, tc.arguments)`。传入 `tool` 仅作 PO 载体（think_loop 从 agent.tools() 按 name 命中即可），实例由 DAL 重组装。
  - RuntimeToolExecution trait（[runtime/mod.rs](../../../src/service/domain/runtime/mod.rs)）新增 `dispatch_manual_tool(ctx, tool, args)`：特殊 tool 转发逻辑从 `ToolDalImpl::execute_manual`（dal/tool.rs L764-L824）**原样上移**——parse_dispatch_mode 选 request_tool_call（sync）/ send_tool_call_message（async）→ registry 创建转发器实例 → 包装 tool_id/tool_name/params/project_id/task_id 参数 → call → 占位 entry。转发器本身无凭据需求（registry 创建即 call，不走 check 注入）；**真实执行兜回** request_tool_call handler → `call_manual_tool_for_agent` → `call_tool`，凭据在真实执行时编排（D26）。
  - `ToolDal::execute_auto` / `execute_manual`（trait 声明 + 默认实现 + ToolDalImpl 重写）删除——think_loop 是唯一调用方。
  - request_tool_call.rs / send_tool_call_message.rs doc 注释「由 ToolDal::execute_manual 内部转发」→「由 domain dispatch_manual_tool 内部转发」（handler 逻辑零改动）。
- [x] **Step 4：内置三员工厂化（删 per-tool trait，D17 v1.5）**（commit be1575dc：每工具模块级 credential_requirements() 自由函数作单点声明——工厂覆写与 CoreTool 实例方法同源调用，声明零漂移 + 一致性测试锁定；lark check 三字段全量到齐才置位（部分注入保持 None 出引导）；**Step 5 内容连带提前完成**——pkg 定义删除后 service/mod.rs 注册行编译必失败，一并与 dal/user.rs、dal/lark/credentials.rs 的 resolver 实现段删除；web_search_tool_test 改写为 D17 编排链等价组合；call 内保留字段 None 单分支防御引导（credential_missing_json 现状为通用文案，不支持 per-requirement hint））：
  - **gh_cli.rs**：删除 `GhCredentialResolver` trait + `RESOLVER` OnceLock + setter/getter（L50-L64）；CoreTool impl 增 `credential_requirements() -> vec![CredentialRequirement { kind: GithubToken, ..默认 }]` 与 `check`（存 `token: Option<String>` 实例字段）；`call` 内取数段（L350-L362）删除，改用 `self.token`（None → 绑定引导 JSON，文案不变；「解析器未就绪」分支一并消失）。
  - **tavily_search.rs**：删除 `TavilyCredentialResolver` trait + OnceLock + setter/getter（L48-L62）；`resolve_api_key` 函数整体移除（用户凭证取数上移 Step 1 编排层，共享兜底已随 D27 Step 0b 废除）；CoreTool impl 增 requirements `[TavilyKey]` + `check`（存 `api_key: Option<String>`）；`call` 用实例字段。
  - **lark_cli.rs**（v1.5 统一）：删除 `LarkCredentialResolver` trait + `RESOLVER` OnceLock + setter/getter（L47-L62）；CoreTool impl 增 requirements——同凭据三条（D4 多字段模式）：

```rust
fn credential_requirements(&self) -> Vec<CredentialRequirement> {
    fn internal(field: &str) -> CredentialRequirement {
        CredentialRequirement {
            kind: CredentialKind::LarkApp,
            platform: None,
            field: Some(field.to_string()),
            enhancer: None,
            binding: CredentialBinding::Internal { field: field.to_string() },
        }
    }
    vec![internal("app_id"), internal("app_secret"), internal("identity_mode")]
}
```

    `check` 存三元组实例字段（`credentials: Option<(String, String, String)>`）；`call` 内取数段（L296-L309）删除——「解析器未就绪」与「未绑定」两分支均消失（未绑定引导统一编排层，文案沿用「请先在个人设置的飞书集成中绑定应用，并创建引用该凭证的 Lark 渠道」，经 `credential_missing_json` 定制 hint 传入）。
  - 内置工具编排统一（D26 收益）：Builtin 与 MCP/HTTP 同经 domain `call_tool` 单点编排（Step 2 第 3 步 registry 重组装已覆盖），无需单独核对调用点。
- [x] **Step 5：service 层清理**（随 Step 4 commit be1575dc 连带完成：pkg trait 定义删除后注册行编译必失败，实际执行顺序内聚；grep 收口核对——六 resolver 类型 + set/get_credential_resolver + CredentialDataProvider + register_default_probes 全仓零残留）——`dal/user.rs` 删 `GhDalCredentialResolver` / `TavilyDalCredentialResolver`（L301-L358）；`dal/lark/credentials.rs`（Step 0 实际落位）删 `LarkDalCredentialResolver` 及 pkg trait impl 段（原 L47-L63，`resolve_credentials_for_user` 保留为 `LarkCredentialDal` trait 方法）；`service/mod.rs` init 删对应**三行**注册（**无任何新增注册项**，凭据相关注册清零）。
- [x] **Step 6：测试**（零修复一次全绿：lib 1026 passed；集成 23 target 102 passed / 0 failed / 32 ignored（均需真实 API key）；clippy --all-targets 零警告）：
  - gh_cli / tavily / lark_cli 既有测试改造：「未注册 resolver → 引导」路径同构变更为「未 check 实例（字段 None）→ 引导」，断言文案核对更新；check 注入后 call → 正常路径断言。lark_cli 既有 `call_without_resolver_returns_error_json`（L453）改写为 `call_without_check_returns_guidance`。
  - domain `resolve_tool_credentials`（经 `new_with_all` 注入 stub：StubLarkCredentialDal / StubUserDal，无需 DB）：生产路由（LarkApp → 子 trait 返回附 attributes{identity_mode}）、find_default 逐条命中 / 任一未命中 None。
  - `new_with_all` / `new_with_tool_dals` 既有调用点（tool_execution_test 六处 + 其他模块引用处 grep 核对）同步补两个新参数（stub 默认返回 None / 空实现即可，不影响既有测试语义）。
  - domain `call_tool` 编排：requirements 空工具直通执行；requirements 命中 → DAL 收到含 resolved 的 ToolExecutionRequest（mock DAL 断言）；未命中 → credential_missing_json 包装。
  - think_loop 分发：Auto 工具经 domain `call_tool`（MCP 工具路由 mcp_tool_dal，修复主循环不可执行）；Manual 经 `dispatch_manual_tool`（特殊 tool 参数包装断言，复用既有 execute_manual 测试模式迁移）。
  - 迁移：mcp_tool_test.rs 四处 `call_tool_by_id` 调用（L258/L665/L687/L709）→ `get_by_id` + `call_tool`（新签名）；tool_execution_test.rs mock ToolDal/McpToolDal `call_tool` 签名适配 ToolExecutionRequest（L389/L516 mock call_tool_by_id 保留——domain 方法不动）。
  - 端到端：MCP requirements + DB 凭据 → call 注入成功；无凭据 → 引导 JSON。
- [x] **Step 7：收口 grep**（全部通过）——`GhCredentialResolver` / `TavilyCredentialResolver` / `LarkCredentialResolver` / `set_credential_resolver` / `get_credential_resolver` / `CredentialDataProvider` 全仓零残留（含 doc 注释与测试）；`execute_auto` / `execute_manual` 全仓零残留、`dispatch_manual_tool` 仅剩 domain + handler doc 注释；`call_tool_by_id` 全仓仅剩 domain trait/impl/test + debug handler（DAL 零残留）。
- [x] **Step 8：全绿 + 提交**（各 Step 独立提交，提交链：a22eede3 → 12ebdbd1 → 518b955f → b6641713 → 68ed836f → 4a610c0d → be1575dc；全量测试 + clippy 三门禁全绿，无额外收口提交需要）。

### Task 3.4：工具表单统一改造（config 可编辑 + 凭据需求展示/编辑）

> **范围扩展（用户决策，2026-08-21）**：Task 3.3 的 D28 改动（三员工具 CLI 参数进 PO config）+ 凭据增强器落地后，工具表单需统一体现「行为参数（PO config 可编辑）+ 凭据需求声明（类型级，指向某一类凭据而非实例）」——工具详情页与 MCP server 表单是同一类问题，一并改造。Builtin 三员凭据需求是工厂静态声明（表单只读展示）；MCP server 是 requirements 主编辑战场；编辑组件做成协议无关，Task 3.5 HTTP 工具后端就绪后直接复用。

**Files:**
- Modify: common/src/api/tool.rs（GetToolResponse 增 credential_requirements 字段）
- Modify: src/handlers/finance/tool/response.rs（聚合透出 + Builtin config 轻量校验辅助）
- Modify: src/handlers/finance/tool/update_tool.rs（config 轻量校验接入）
- Modify: frontend/src/pages/finance/tool_detail.rs（工具配置编辑卡片 + 凭据需求只读卡片）
- Modify: frontend/src/pages/finance/mcp_servers.rs（创建 modal 增凭据需求动态列表）
- Modify: frontend/src/pages/finance/mcp_server_detail.rs（展示 requirements 列表，只读卡片）

- [x] **Step 1：后端 DTO + 聚合透出**（commit 1c0f788a + Step 1b commit 6c9af46d：`#[serde(default)]` 保证前端反序列化兼容；ToolListItem 未加（列表 badge 由 runtime_ready 覆盖）；config 脱敏通道中 credential_requirements 键命中敏感词被 REDACTED——独立顶层字段正是为此；**Step 1b 打通 Builtin 更新管道**：DAO guard 从全字段挡改 diff 式工厂字段保护（name/description/protocol/control_mode/parameters_schema/tags 不可改，config/status 放行），顺带修复 Builtin 启停被挡现状 bug；Builtin config 变化不触发向量重索引（vectorize_text 不含 config，语义自洽））——`GetToolResponse` 增 `credential_requirements: Vec<common::models::CredentialRequirement>`；handler response.rs 构造时从 `ToolRegistry::credential_requirements(&tool.po)` 聚合（Builtin 工厂声明 / Mcp·Http config 解析统一入口）；GetTool / UpdateTool / ListTools（ToolListItem 若需要）响应同步。`update_tool` config 轻量校验：Builtin 已知字段类型校验（command 非空 string / timeout_ms 正整数 / max_output_bytes 正整数），未知字段宽松保留（不做白名单封闭，保持 config 扩展性）。
- [x] **Step 2：前端工具详情页两张新卡片**（commit 983a843a：GetToolResponse 无 runtime_ready 字段——经 query_tools(ids) 列表通道取 ToolListItem.runtime_ready 同源探测；CLI 型与 key 型 NotReady 文案区分；防丢合并纯函数 merge_builtin_config + 10 个单测；CredentialRequirementsTable 组件落位 components/credential_requirements.rs 供 Step 4 复用）——
  - 「工具配置」编辑卡片：结构化字段按协议渲染（browser: command/timeout_ms/max_output_bytes/install_hint；gh/lark: command；tavily: timeout_ms；MCP/Http 工具 config 为 JSON 编辑区或继承 server 不展示），保存走既有 `update_tool`（config 整体替换，管道已通）；Builtin 未展示字段保留在提交 JSON 中（先读 detail config 再覆盖展示字段，避免整体替换丢字段）。
  - 「凭据需求」只读卡片：表格（kind/platform/field/enhancer/binding 注入点），Builtin 工厂静态声明只读；runtime_ready NotReady 时展示绑定引导（与列表 badge 同源）。
- [x] **Step 3：MCP server 创建 modal 凭据需求动态列表**（commit aae2ecb1：transport 变更时已有条目 binding 重包装 Env↔Header 保留注入名（binding↔transport 结构性不可能失配）；enhancer 下拉排除 default_enhancer（D11）；预校验五规则与后端对齐 + 12 个单测；核对确认无编辑 modal（仅创建+启停+删除），未做回填）——`use_signal(Vec<CredentialRequirement>)` add/remove；单条表单字段联动：
  - kind 下拉（六值，`CredentialKind` serde 值）。
  - platform 输入框：仅 `kind.requires_platform()` 显示（前端用 `common::models::CredentialKind::requires_platform`）。
  - field 输入框 / enhancer 下拉互斥（选了 field 禁用 enhancer 下拉，反之亦然）；enhancer 下拉选项按 `enhancer_supports(kind, e)` 过滤，专用 kind 时禁用并提示「该凭据类型不适用增强器」；**不提供默认增强器选项**（oauth 下拉无 access_token、user_password 无 basic_auth，D11 前端不暴露）。
  - binding：transport=stdio → 仅 Env（name 输入）；streamable_http → 仅 Header。
  - 前端预校验（common 矩阵函数简化版：binding↔transport、platform↔kind、field/enhancer 互斥、注入名非空、三元组去重）；后端 create/update handler 已有 `validate_requirements` 双保险。
- [x] **Step 4：MCP server detail 页 requirements 只读展示**（commit aae2ecb1：复用 CredentialRequirementsTable +12 行）——requirements 表格（kind/platform/field/enhancer/binding type/name），非敏感直接展示。
- [x] **Step 5：`cargo test --lib` + 后端 handler 测试（requirements 聚合透出断言 + Builtin config 校验用例）+ `cd frontend && cargo test && cargo clippy --target wasm32-unknown-unknown --all-targets -- -D warnings` + 提交**（后端 1c0f788a + 6c9af46d：lib 1038 passed 全绿 + 集成测试 exit 0；前端 983a843a + aae2ecb1：109 passed + clippy 零警告；提交链按 Step 拆分）。

**Phase 3 收口检查：**
- [x] `cargo test --lib`（1038 passed）、集成测试（23 target 全绿 exit 0）、前端双命令（109 passed + clippy 零警告）全绿。
- [x] `GhCredentialResolver` / `TavilyCredentialResolver` / `LarkCredentialResolver` / `set_credential_resolver` / `get_credential_resolver` / `CredentialDataProvider` 全仓 grep 零残留；`service::init` 凭据注册三行清零。
- [x] D26 入口统一收口：think_loop 无 `tool_dal().execute_auto/execute_manual` 直连；`ToolDal::call_tool_by_id` / `McpToolDal::call_tool_by_id` / `ToolDal::execute_auto` / `execute_manual` 删除；DAL `call_tool` 统一 ToolExecutionRequest 签名；Auto 模式 MCP 工具主循环可执行（协议路由到 mcp_tool_dal）。
- [ ] 手工冒烟（可选）：启动后创建带 requirements 的 stdio server，DB config 无 env/headers 字段；同一 server 两用户调用各自注入；lark_cli 调用链与改造前行为一致。

---

## Phase 4：HTTP 工具集成

### Task 4.1：HttpToolConfig + 校验 + 运行时注入

**Files:**
- Modify: src/pkg/tool_registry/http.rs
- Test: src/pkg/tool_registry/http_tests.rs

- [ ] **Step 1：测试先行**：

```rust
#[test]
fn validate_rejects_sensitive_static_header() { ... }      // headers: {"authorization": "Bearer x"} → Err
#[test]
fn validate_rejects_sensitive_templated_header() { ... }   // headers: {"x-api-key": "{{args.k}}"} → Err
#[test]
fn validate_rejects_sensitive_query() { ... }              // query: {"token": "v"} → Err（is_sensitive_header 同判 query 键）
#[test]
fn validate_accepts_normal_headers() { ... }               // Content-Type/Accept → Ok
#[test]
fn validate_rejects_env_binding_for_http_tool() { ... }    // requirements 带 Env binding → Err
```

- [ ] **Step 2：config 字段**——`HttpToolConfig` 增：

```rust
/// Credential requirements (type-level; sensitive header/query injection
/// is only allowed through these bindings, D15).
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub credential_requirements: Vec<common::models::CredentialRequirement>,
```

- [ ] **Step 3：validate_config 增两段**（在 `validate_scalar_template_object` 调用后）：
  - 敏感名拒绝：headers/query 的每个 key 过 `is_sensitive_header` → 命中即 Err（`validate_scalar_template_object` 内 field_name=="headers"/"query" 分支各加一条，或在 validate_config 独立遍历——取独立遍历，语义清晰）。
  - `pkg::credential::validate_requirements(&config.credential_requirements, CredentialRequirementScope::HttpTool)?`。
- [ ] **Step 4：运行时注入（D22 生命周期，与 McpCoreTool 同构）**——`HttpCoreTool` 增 `credential_requirements()`（从 config 读）与 `check`（存 `header_injections: Vec<(String, String)>` + `query_injections: Vec<(String, String)>` 实例字段）；`execute_http_call` 不取数，模板 headers/query 渲染**之后**叠加实例注入值：

```rust
for (name, value) in &self.header_injections {
    request = request.header(name, value);
}
for (name, value) in &self.query_injections {
    // reqwest query 追加：照 http.rs 现网 request builder 模式实现
}
```

  编排层取数与缺凭据引导复用 Task 3.3 的 domain `resolve_tool_credentials` + `credential_missing_json`（HTTP 工具调用编排与 MCP 同一函数）。
- [ ] **Step 5：新增运行时测试**（http_tests.rs TcpListener mock 模式）：
  - `http_tool_injects_basic_auth_header`：requirements user_password/Header{authorization} + DB 凭据行 → mock server 断言收到 `authorization: Basic base64(...)`。
  - `http_tool_missing_credential_returns_guidance`。
  - `http_tool_sensitive_header_rejected_at_config_time`（覆盖 Step 1）。
- [ ] **Step 6：全绿 + 提交** `git commit -m "feat(http-tool): credential requirements with header/query injection"`。

### Task 4.2：前端 HTTP 工具表单凭据需求区

**Files:**
- Modify: frontend/src/components/create_http_tool.rs
- Modify: frontend/src/pages/finance/tools.rs

- [ ] **Step 1**：同 Task 3.3 模式；binding 仅 Header/Query；transport 联动改为无 transport（HTTP 工具恒 HttpTool scope）。
- [ ] **Step 2**：headers/query 静态编辑区增加敏感名前端预检（输入敏感名即时提示「敏感头只能通过凭据需求注入」）。
- [ ] **Step 3**：前端测试 + clippy + 提交。

**Phase 4 收口检查：**
- [ ] 全量 `make test` + 双端 clippy 绿。

---

## Phase 5：全量验证 + 文档收尾

- [ ] **Step 1**：`make clippy && make clippy-fe && make test`；如有集成测试目录 `cargo test --test '*'` 一并跑。
- [ ] **Step 2**：覆盖率门槛核对（`cargo llvm-cov` 若配置了 CI 门槛 38%/45%，确认不回退——只跑受影响模块对比即可）。
- [ ] **Step 3**：文档三件套（用对应 skill 执行，非手写）：
  1. `ai-orz-wiki-maintainer`：新增 RAG 知识卡「共享工具凭据增强器」+ wiki 长文引用（过 5 级查重）。
  2. `ai-orz-doc-maintainer`：本执行文档去 checkbox 化 → 归档为 `docs/plan/共享工具凭据增强器落地.md`（7 节模板）。
  3. 设计文档「暂无对应 plan 文档」行更新为归档 plan 链接。
- [ ] **Step 4**：`docs/superpowers/plans/2026-08-21-tool-credential-enhancer.md` 处置（7 天内删除，由 doc-maintainer 流程带走）。

---

## 自查记录（writing-plans self-review）

1. **Spec 覆盖**：设计 v1.5 §三 清单逐项映射——common 契约（T1.1/T1.2）、迁移（T1.3）、models MCP（T3.1）、CoreTool 生命周期（T3.2）、pkg 纯值加工（T2.1-2.3）、MCP 注入+连接隔离（T3.2）、domain 编排+内置三员工厂化（T3.3）、HTTP（T4.1）、handler（T3.1）、前端（T3.4/T4.2）、测试（各 Task 内嵌）。D1-D25 决策全部有落点；D17/D22 编排与实例化在 T3.2/T3.3（v1.5 生产路由 + lark_cli 工厂化）；D18（list 同步）在 T3.2 Step 4；D19 引导在 T2.3/T3.3；D23 连接隔离在 T3.2 Step 3；D24 attributes（T2.3 查找链 + T3.3 LarkApp 生产路由）；D25 Internal binding（T1.1 契约 + T2.3 校验 + T3.3 lark_cli 声明）。
2. **Placeholder 扫描**：测试代码中 `...` 处为「测试体按注释断言展开」的显式标记；Task 3.3 Step 1 伪代码中 `/* user_dal... */ None` 为「实现时按注释落位」的显式标记，语义已在注释中定义，非决策空洞。
3. **类型一致性**：`CredentialRequirement`/`CredentialEnhancerKind`/`CredentialBinding`（含 Internal{field}）字段名与设计 §2.3 逐字对齐；`find_default(ctx, user_id, kind, platform)` 贯穿 DAO/DAL；`FetchedCredential{credential_id, detail, already_decrypted, attributes}` 贯穿 domain/pkg；`resolve_requirements(requirements, fetched) -> Vec<ResolvedRequirement>` 纯函数签名统一；`check(&mut self, resolved)` 生命周期签名贯穿 CoreTool 各实现。
4. **已知风险前置**：OAuthTokenManager 真实刷新链路因 SSRF 默认拒内网无法本地 mock 全覆盖（T2.2 Step 3 已注明覆盖策略）；OnceLock 全局单测注册一次的限制沿用 gh_cli 既有测试模式；lark_cli 改造后既有「未注册 resolver」测试需同构改写为「未 check」路径（T3.3 Step 5 已列）。
