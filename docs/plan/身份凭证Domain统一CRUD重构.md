# 身份凭证 Domain 层统一 CRUD 重构实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 `IdentityCredentialManage` trait 中按凭证类型复制的 8 个 CRUD 方法（lark 4 + github 4）收敛为 5 个类型无关的统一方法，类型差异下沉到模型（`CredentialDetail` 行为）+ Domain 内 `match kind` 分发。

**Architecture:** 三层收敛——(1) 字段知识下沉 common 模型：`CredentialDetail` 获得 `normalize/validate/kind/primary_id/encrypt_sensitive/apply_patch` 行为（信息专家原则，与 `Vectorizable` 同模式），加密原语以闭包注入（common 不依赖后端 crypto）；(2) Domain trait 换统一签名（Command 化入参），实现内部 `match kind` 分发前置检查与后置副作用；(3) Handler 保持类型专属不动（用户明确：handler 是具体行为，不抽象），只改调用方式。**不引入策略模式**（仅 2 种类型，match 足够）。

**Tech Stack:** Rust / async_trait / serde 内部 tag 枚举 / 现有 `pkg::crypto::encrypt_channel_secret` 作为加密原语注入。

---

## 背景与现状（执行者必读）

### 现状问题

`src/service/domain/finance/mod.rs` 的 `IdentityCredentialManage` trait 有 17 个方法，其中 8 个是按类型复制的 CRUD（`create/update/delete/set_default_lark_credential` + `_github_credential` 各 4 个）。每加一种凭证类型（微信/Slack）trait 就 +4~5 个方法。

两套实现的骨架完全同构：`load_credential_library → 变更 → save → 联动`。真正的类型差异只有三类：
- **(a) detail 结构与必填校验** → 下沉到 `CredentialDetail::validate()/normalized()`
- **(b) 敏感字段加密**（哪些字段算敏感）→ 下沉到 `CredentialDetail::encrypt_sensitive()/apply_patch()`
- **(c) 生命周期副作用** → 保留在 Domain，用 `match kind` 分发：
  - lark update 后：清 lark-cli HOME config + WS 监听移交（`handover_listeners_after_credential_change(old_app_id, new_app_id, secret_changed)`，失败仅告警）
  - lark delete 前：渠道引用检查（被引用报 `Conflict` 拒删）
  - github delete 后：删的是生效凭证时 `clear_gh_auth` 清 HOME 登录态（失败仅告警）

### 涉及文件全景

| 文件 | 角色 | 本计划动作 |
|------|------|-----------|
| `common/src/models/identity_credentials.rs` | 凭证模型 | Task 1-3：新增行为方法 + 补丁枚举 + 测试 |
| `src/service/domain/finance/mod.rs` | trait + Commands | Task 4：删 8 个类型方法、加 5 个统一方法、定义 2 个 Command |
| `src/service/domain/finance/identity_credential.rs` | trait 实现 | Task 4：重写 CRUD 为统一实现 + match 分发 |
| `src/handlers/finance/lark_integration/{create,update,delete,set_default}_credential.rs` | 4 个 lark handler | Task 4：改调统一方法（构造 Command/Patch） |
| `src/handlers/finance/github_integration/{create,update,delete,set_default}_credential.rs` | 4 个 github handler | Task 4：同上 |
| `tests/integration/lark_integration_test.rs` / `github_integration_test.rs` | 行为安全网 | **不改**，必须原样通过 |
| 前端 / API DTO / 路由 | — | **零改动**（handler 对外契约不变） |

### 统一后的 trait 形态（目标）

```rust
#[async_trait]
pub trait IdentityCredentialManage: Send + Sync {
    // ===== 统一凭证 CRUD（封顶 5 个，不随类型增长） =====
    async fn get_identity_credentials(&self, ctx, user_id) -> Result<Option<UserIdentityCredentials>>;
    async fn create_credential(&self, ctx, user_id, cmd: CreateCredentialCmd) -> Result<String>;
    async fn update_credential(&self, ctx, user_id, cmd: UpdateCredentialCmd) -> Result<()>;
    async fn delete_credential(&self, ctx, user_id, credential_id: &str) -> Result<()>;
    async fn set_default_credential(&self, ctx, user_id, kind: CredentialKind, credential_id: Option<&str>) -> Result<()>;

    // ===== 类型专属集成能力（保留，性质不是 CRUD） =====
    async fn github_integration_status(...) -> Result<GithubIntegrationStatusResponse>;
    async fn lark_auth_start/complete/status/logout(...);   // device flow，不动
    async fn lark_bind_start/status/cancel(...);            // 绑定会话，不动
}
```

### 行为保持要点（回归红线）

1. lark update 的联动链（清 cli config + WS handover）语义不变；`secret_changed = app_secret 或 encrypt_key 任一非空提供`
2. lark delete 的渠道引用 `Conflict` 拦截不变
3. github delete 的 `was_active → clear_gh_auth` 联动不变
4. `verification_token` 更新语义不变：`Some("")` 清除、`None` 保持
5. **统一微调（可接受）**：明文字段统一 trim 后加密/落库（原 lark app_secret 不 trim、github token trim，统一为都 trim——粘贴凭证尾随空白是常见场景，属修复而非破坏）
6. set_default 对不存在凭证从 lark 的 `InvalidRequest` / github 的 `NotFound` 统一为 `NotFound`（集成测试断言 4xx，两者皆 4xx，不受影响）

---

### Task 1: common — `CredentialDetail` 行为知识（normalize/validate/kind/primary_id/encrypt_sensitive）

**Files:**
- Modify: `common/src/models/identity_credentials.rs`（`CredentialDetail` impl 块 + 测试模块）

- [ ] **Step 1.1: 写失败测试**

在 `common/src/models/identity_credentials.rs` 的 `mod tests` 末尾追加：

```rust
    // ==================== CredentialDetail 行为 ====================

    #[test]
    fn test_detail_kind_and_primary_id() {
        let lark = lark_credential("c1", "A").detail;
        assert_eq!(lark.kind(), CredentialKind::LarkApp);
        assert_eq!(lark.primary_id(), Some("cli_a1b2c3"));

        let gh = github_credential("g1", "B").detail;
        assert_eq!(gh.kind(), CredentialKind::GithubToken);
        assert_eq!(gh.primary_id(), None);
    }

    #[test]
    fn test_detail_normalized_trims_and_drops_empty_optionals() {
        let plain = CredentialDetail::LarkApp {
            app_id: "  cli_x  ".to_string(),
            app_secret: " s1 ".to_string(),
            encrypt_key: Some("   ".to_string()),
            verification_token: Some(" vt ".to_string()),
        };
        let normalized = plain.normalized();
        let CredentialDetail::LarkApp {
            app_id,
            app_secret,
            encrypt_key,
            verification_token,
        } = normalized
        else {
            panic!("kind 不变");
        };
        assert_eq!(app_id, "cli_x");
        assert_eq!(app_secret, "s1");
        assert_eq!(encrypt_key, None, "空白可选字段视为未提供");
        assert_eq!(verification_token.as_deref(), Some("vt"));

        let gh = CredentialDetail::GithubToken {
            token: " ghp_x\n ".to_string(),
        }
        .normalized();
        assert!(matches!(gh, CredentialDetail::GithubToken { ref token } if token == "ghp_x"));
    }

    #[test]
    fn test_detail_validate_required_fields() {
        // 合法
        assert!(lark_credential("c1", "A").detail.validate().is_ok());
        assert!(github_credential("g1", "B").detail.validate().is_ok());
        // lark 缺 app_id
        let bad = CredentialDetail::LarkApp {
            app_id: " ".to_string(),
            app_secret: "s".to_string(),
            encrypt_key: None,
            verification_token: None,
        }
        .normalized();
        assert!(bad.validate().is_err());
        // github 缺 token
        let bad_gh = CredentialDetail::GithubToken {
            token: String::new(),
        };
        assert!(bad_gh.validate().is_err());
    }

    #[test]
    fn test_encrypt_sensitive_only_secret_fields() {
        let plain = CredentialDetail::LarkApp {
            app_id: "cli_x".to_string(),
            app_secret: "s1".to_string(),
            encrypt_key: Some("k1".to_string()),
            verification_token: Some("vt".to_string()),
        };
        let enc = plain
            .clone()
            .encrypt_sensitive(|s| Ok(format!("enc:{}", s)))
            .unwrap();
        let CredentialDetail::LarkApp {
            app_id,
            app_secret,
            encrypt_key,
            verification_token,
        } = enc
        else {
            panic!("kind 不变");
        };
        assert_eq!(app_id, "cli_x", "app_id 非敏感原样保留");
        assert_eq!(app_secret, "enc:s1");
        assert_eq!(encrypt_key.as_deref(), Some("enc:k1"));
        assert_eq!(verification_token.as_deref(), Some("vt"), "verification_token 非加密字段");

        // encrypt_key=None 不调用加密器
        let no_key = CredentialDetail::LarkApp {
            app_id: "a".to_string(),
            app_secret: "s".to_string(),
            encrypt_key: None,
            verification_token: None,
        };
        assert!(no_key.encrypt_sensitive(|_| Err(bail_err_test())).is_err());

        // github token 加密
        let gh_enc = CredentialDetail::GithubToken {
            token: "ghp_x".to_string(),
        }
        .encrypt_sensitive(|s| Ok(format!("enc:{}", s)))
        .unwrap();
        assert!(matches!(gh_enc, CredentialDetail::GithubToken { ref token } if token == "enc:ghp_x"));
    }

    /// 测试用错误构造（避免测试依赖具体 error 变体）
    fn bail_err_test() -> crate::error::Error {
        crate::error::Error::Internal("test".to_string())
    }
```

> 注意：`bail_err_test` 依赖 `common::error::Error` 的变体形态，若 `Error` 无 `Internal(String)` 变体，改用该文件中已存在的错误构造方式（查看 `common/src/error.rs` 后调整，测试意图只是"加密器失败时传播错误"）。

- [ ] **Step 1.2: 跑测试确认失败**

```bash
cargo test -p common --lib models::identity_credentials
```

预期：编译失败（`kind/normalized/validate/encrypt_sensitive` 方法不存在）。

- [ ] **Step 1.3: 实现**

在 `CredentialDetail` 枚举定义后新增 impl 块：

```rust
/// 凭证详情补丁（Domain 更新命令组件，明文输入；非 API DTO，无需 serde）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CredentialDetailPatch {
    /// 不变更 detail
    #[default]
    Unchanged,
    /// 飞书应用字段补丁（None 保持不变；verification_token 的 Some("") 表示清除）
    LarkApp {
        app_id: Option<String>,
        app_secret: Option<String>,
        encrypt_key: Option<String>,
        verification_token: Option<String>,
    },
    /// GitHub token 补丁
    GithubToken {
        token: Option<String>,
    },
}
```

（补丁枚举本 Task 先定义占位，Task 2 实现行为——避免 Task 1 测试无关编译噪音，也可直接放到 Task 2 定义，执行者二选一，保持一次编译通过即可。）

```rust
impl CredentialDetail {
    /// 凭证类型
    pub fn kind(&self) -> CredentialKind {
        match self {
            Self::LarkApp { .. } => CredentialKind::LarkApp,
            Self::GithubToken { .. } => CredentialKind::GithubToken,
        }
    }

    /// 外部主标识（lark app_id；无概念的类型返回 None，供渠道移交对比）
    pub fn primary_id(&self) -> Option<&str> {
        match self {
            Self::LarkApp { app_id, .. } => Some(app_id.as_str()),
            Self::GithubToken { .. } => None,
        }
    }

    /// 规范化明文字段（trim；空白的可选字段视为未提供）
    pub fn normalized(self) -> Self {
        match self {
            Self::LarkApp {
                app_id,
                app_secret,
                encrypt_key,
                verification_token,
            } => Self::LarkApp {
                app_id: app_id.trim().to_string(),
                app_secret: app_secret.trim().to_string(),
                encrypt_key: encrypt_key
                    .map(|v| v.trim().to_string())
                    .filter(|s| !s.is_empty()),
                verification_token: verification_token
                    .map(|v| v.trim().to_string())
                    .filter(|s| !s.is_empty()),
            },
            Self::GithubToken { token } => Self::GithubToken {
                token: token.trim().to_string(),
            },
        }
    }

    /// 类型必填校验（规范化后调用）
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::LarkApp {
                app_id, app_secret, ..
            } => {
                if app_id.is_empty() || app_secret.is_empty() {
                    bail_err!(InvalidRequest, "飞书应用 App ID / App Secret 不能为空");
                }
            }
            Self::GithubToken { token } => {
                if token.is_empty() {
                    bail_err!(InvalidRequest, "GitHub Token 不能为空");
                }
            }
        }
        Ok(())
    }

    /// 对敏感字段应用加密（哪些字段敏感由模型自身决定；加密原语以闭包注入，
    /// common 不依赖后端 crypto 实现）
    pub fn encrypt_sensitive<F>(self, encrypt: F) -> Result<Self>
    where
        F: Fn(&str) -> Result<String>,
    {
        match self {
            Self::LarkApp {
                app_id,
                app_secret,
                encrypt_key,
                verification_token,
            } => Ok(Self::LarkApp {
                app_id,
                app_secret: encrypt(&app_secret)?,
                encrypt_key: match encrypt_key {
                    Some(v) => Some(encrypt(&v)?),
                    None => None,
                },
                verification_token,
            }),
            Self::GithubToken { token } => Ok(Self::GithubToken {
                token: encrypt(&token)?,
            }),
        }
    }
}
```

> **闭包签名注意**：`crate::pkg::crypto::encrypt_channel_secret` 的实际签名若不是 `fn(&str) -> common::error::Result<String>`（例如参数收 `&str` 返回 `Result<String>` 即匹配；若收 `String` 则调用处包一层 `|s| crypto::encrypt_channel_secret(s)`），在 Task 4 接入时以编译器提示为准微调（模型方法泛型 bound 不动）。

- [ ] **Step 1.4: 跑测试确认通过**

```bash
cargo test -p common --lib models::identity_credentials
```

预期：全部 PASS（含既有测试）。

---

### Task 2: common — `CredentialDetailPatch` + `apply_patch`（补丁应用与影响摘要）

**Files:**
- Modify: `common/src/models/identity_credentials.rs`

- [ ] **Step 2.1: 写失败测试**

`mod tests` 追加：

```rust
    // ==================== 补丁应用 ====================

    fn plain_lark_detail() -> CredentialDetail {
        CredentialDetail::LarkApp {
            app_id: "cli_old".to_string(),
            app_secret: "enc:v1:old-secret".to_string(),
            encrypt_key: None,
            verification_token: Some("vt-old".to_string()),
        }
    }

    #[test]
    fn test_apply_patch_unchanged_is_noop() {
        let mut detail = plain_lark_detail();
        let before = detail.clone();
        let impact = detail
            .apply_patch(CredentialDetailPatch::Unchanged, |s| Ok(format!("enc:{}", s)))
            .unwrap();
        assert_eq!(detail, before);
        assert!(!impact.secret_changed);
    }

    #[test]
    fn test_apply_patch_lark_fields() {
        let mut detail = plain_lark_detail();
        let impact = detail
            .apply_patch(
                CredentialDetailPatch::LarkApp {
                    app_id: Some("cli_new".to_string()),
                    app_secret: Some("new-secret".to_string()),
                    encrypt_key: Some("k1".to_string()),
                    verification_token: Some("  ".to_string()), // 空白清除
                },
                |s| Ok(format!("enc:{}", s)),
            )
            .unwrap();
        let CredentialDetail::LarkApp {
            app_id,
            app_secret,
            encrypt_key,
            verification_token,
        } = &detail
        else {
            panic!("kind 不变");
        };
        assert_eq!(app_id, "cli_new");
        assert_eq!(app_secret, "enc:new-secret");
        assert_eq!(encrypt_key.as_deref(), Some("enc:k1"));
        assert_eq!(*verification_token, None, "空白 verification_token 清除");
        assert!(impact.secret_changed, "secret/encrypt_key 任一变更即轮换");
    }

    #[test]
    fn test_apply_patch_empty_optional_keeps_value() {
        // 全 None（含空串）→ detail 不变、无 secret 轮换
        let mut detail = plain_lark_detail();
        let before = detail.clone();
        let impact = detail
            .apply_patch(
                CredentialDetailPatch::LarkApp {
                    app_id: None,
                    app_secret: Some("   ".to_string()),
                    encrypt_key: None,
                    verification_token: None,
                },
                |s| Ok(format!("enc:{}", s)),
            )
            .unwrap();
        assert_eq!(detail, before);
        assert!(!impact.secret_changed);
    }

    #[test]
    fn test_apply_patch_kind_mismatch_rejected() {
        // github 补丁打到 lark 凭证 → 报错
        let mut detail = plain_lark_detail();
        assert!(detail
            .apply_patch(
                CredentialDetailPatch::GithubToken {
                    token: Some("t".to_string()),
                },
                |s| Ok(s.to_string()),
            )
            .is_err());
        // lark 补丁打到 github 凭证 → 报错
        let mut gh = CredentialDetail::GithubToken {
            token: "enc:v1:t".to_string(),
        };
        assert!(gh
            .apply_patch(
                CredentialDetailPatch::LarkApp {
                    app_id: None,
                    app_secret: None,
                    encrypt_key: None,
                    verification_token: None,
                },
                |s| Ok(s.to_string()),
            )
            .is_err());
    }

    #[test]
    fn test_apply_patch_github_token() {
        let mut gh = CredentialDetail::GithubToken {
            token: "enc:v1:old".to_string(),
        };
        let impact = gh
            .apply_patch(
                CredentialDetailPatch::GithubToken {
                    token: Some("ghp_new".to_string()),
                },
                |s| Ok(format!("enc:{}", s)),
            )
            .unwrap();
        assert!(matches!(&gh, CredentialDetail::GithubToken { token } if token == "enc:ghp_new"));
        assert!(impact.secret_changed);
    }
```

- [ ] **Step 2.2: 跑测试确认失败**

```bash
cargo test -p common --lib models::identity_credentials
```

预期：编译失败（`apply_patch` 不存在）。

- [ ] **Step 2.3: 实现**

`CredentialDetail` impl 块追加（若 Task 1 未定义 `CredentialDetailPatch`，在此定义）：

```rust
/// detail 变更影响摘要（Domain 据此决定联动动作，无需感知字段细节）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CredentialUpdateImpact {
    /// 敏感字段轮换（app_secret/encrypt_key/token 任一实际写入）
    pub secret_changed: bool,
}
```

```rust
    /// 应用明文补丁：新敏感字段在内部加密后写入，返回变更影响摘要
    ///
    /// - 补丁变体与凭证类型不匹配时报错（防御跨类型误用）
    /// - 可选字段 None / 空白保持原值不变；`verification_token` 的显式空串清除
    pub fn apply_patch<F>(
        &mut self,
        patch: CredentialDetailPatch,
        encrypt: F,
    ) -> Result<CredentialUpdateImpact>
    where
        F: Fn(&str) -> Result<String>,
    {
        let mut impact = CredentialUpdateImpact::default();
        match patch {
            CredentialDetailPatch::Unchanged => {}
            CredentialDetailPatch::LarkApp {
                app_id,
                app_secret,
                encrypt_key,
                verification_token,
            } => {
                let Self::LarkApp {
                    app_id: app_id_slot,
                    app_secret: secret_slot,
                    encrypt_key: encrypt_slot,
                    verification_token: token_slot,
                } = self
                else {
                    bail_err!(InvalidRequest, "补丁类型与凭证类型不匹配，无法应用飞书凭证补丁");
                };
                if let Some(v) = app_id
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    *app_id_slot = v;
                }
                if let Some(v) = app_secret
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    *secret_slot = encrypt(&v)?;
                    impact.secret_changed = true;
                }
                if let Some(v) = encrypt_key
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    *encrypt_slot = Some(encrypt(&v)?);
                    impact.secret_changed = true;
                }
                if let Some(v) = verification_token {
                    // 空白视为清除，非空覆盖
                    *token_slot = Some(v.trim().to_string()).filter(|s| !s.is_empty());
                }
            }
            CredentialDetailPatch::GithubToken { token } => {
                let Self::GithubToken { token: token_slot } = self else {
                    bail_err!(InvalidRequest, "补丁类型与凭证类型不匹配，无法应用 GitHub 凭证补丁");
                };
                if let Some(v) = token
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    *token_slot = encrypt(&v)?;
                    impact.secret_changed = true;
                }
            }
        }
        Ok(impact)
    }
```

- [ ] **Step 2.4: 跑测试确认通过**

```bash
cargo test -p common --lib models::identity_credentials
```

预期：全部 PASS。

---

### Task 3: common — `UserIdentityCredentials` 默认凭证统一操作

**Files:**
- Modify: `common/src/models/identity_credentials.rs`

- [ ] **Step 3.1: 写失败测试**

`mod tests` 追加：

```rust
    // ==================== 默认凭证统一操作 ====================

    #[test]
    fn test_set_default_for_each_kind() {
        let mut creds = UserIdentityCredentials {
            items: vec![
                lark_credential("cred-1", "A"),
                github_credential("gh-1", "工作号"),
            ],
            ..Default::default()
        };
        // 设置 lark 默认
        creds
            .set_default_for(CredentialKind::LarkApp, Some("cred-1".to_string()))
            .unwrap();
        assert_eq!(creds.default_credential_id.as_deref(), Some("cred-1"));
        assert_eq!(creds.default_github_credential_id, None, "两类型默认互不影响");
        // 设置 github 默认
        creds
            .set_default_for(CredentialKind::GithubToken, Some("gh-1".to_string()))
            .unwrap();
        assert_eq!(creds.default_github_credential_id.as_deref(), Some("gh-1"));
        assert_eq!(creds.default_credential_id.as_deref(), Some("cred-1"));
        // 空串等价取消
        creds
            .set_default_for(CredentialKind::GithubToken, Some("  ".to_string()))
            .unwrap();
        assert_eq!(creds.default_github_credential_id, None);
        // None 清除
        creds
            .set_default_for(CredentialKind::LarkApp, None)
            .unwrap();
        assert_eq!(creds.default_credential_id, None);
    }

    #[test]
    fn test_set_default_for_validates() {
        let mut creds = UserIdentityCredentials {
            items: vec![github_credential("gh-1", "工作号")],
            ..Default::default()
        };
        // 不存在 → NotFound
        assert!(creds
            .set_default_for(CredentialKind::GithubToken, Some("no-such".to_string()))
            .is_err());
        // 类型不匹配（拿 lark kind 设 github 凭证）→ InvalidRequest
        assert!(creds
            .set_default_for(CredentialKind::LarkApp, Some("gh-1".to_string()))
            .is_err());
    }

    #[test]
    fn test_clear_default_for_on_delete() {
        let mut creds = UserIdentityCredentials {
            items: vec![github_credential("gh-1", "工作号")],
            default_github_credential_id: Some("gh-1".to_string()),
            default_credential_id: Some("gh-1".to_string()), // 异常态也一并清对位字段
        };
        creds.clear_default_for(CredentialKind::GithubToken, "gh-1");
        assert_eq!(creds.default_github_credential_id, None);
        assert_eq!(
            creds.default_credential_id.as_deref(),
            Some("gh-1"),
            "只清对应类型的默认槽位"
        );
        // 非命中 ID 不动
        creds.default_github_credential_id = Some("gh-2".to_string());
        creds.clear_default_for(CredentialKind::GithubToken, "gh-1");
        assert_eq!(creds.default_github_credential_id.as_deref(), Some("gh-2"));
    }
```

- [ ] **Step 3.2: 跑测试确认失败**

```bash
cargo test -p common --lib models::identity_credentials
```

预期：编译失败（`set_default_for/clear_default_for` 不存在）。

- [ ] **Step 3.3: 实现**

`UserIdentityCredentials` impl 块追加（文件顶部 import 需补 `err`：`use crate::error::{Result, bail_err, err};`）：

```rust
    /// 按类型设置默认凭证（Some 时校验存在 + 类型匹配；None/空白清除）
    ///
    /// 各类型默认槽位独立（lark 与 github 互不影响）。
    pub fn set_default_for(
        &mut self,
        kind: CredentialKind,
        credential_id: Option<String>,
    ) -> Result<()> {
        let target = credential_id
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let Some(id) = target else {
            *self.default_slot_mut(kind) = None;
            return Ok(());
        };
        let credential = self
            .find_by_id(&id)
            .ok_or_else(|| err!(NotFound, "凭证不存在 credential_id={}", id))?;
        if credential.kind != kind {
            bail_err!(InvalidRequest, "所选凭证类型不匹配，无法设为该类型默认凭证");
        }
        *self.default_slot_mut(kind) = Some(id);
        Ok(())
    }

    /// 按类型清除指向该凭证的默认标记（删除凭证时联动）
    pub fn clear_default_for(&mut self, kind: CredentialKind, credential_id: &str) {
        let slot = self.default_slot_mut(kind);
        if slot.as_deref() == Some(credential_id) {
            *slot = None;
        }
    }

    /// 类型 → 默认凭证槽位
    fn default_slot_mut(&mut self, kind: CredentialKind) -> &mut Option<String> {
        match kind {
            CredentialKind::LarkApp => &mut self.default_credential_id,
            CredentialKind::GithubToken => &mut self.default_github_credential_id,
        }
    }
```

> 若 `common::error` 无 `err!` 宏，改用 `bail_err!(NotFound, ...)`（确认 `NotFound` 变体存在于 common error；当前 domain 层正是从 `common::error` 导入 `err!`，应当可用）。

- [ ] **Step 3.4: 跑 common 全量测试确认通过**

```bash
cargo test -p common --lib
```

预期：全部 PASS。

---

### Task 4: Domain trait 收敛 + 实现重写 + 8 个 handler 迁移（原子编译单元）

> 本 Task 是一个原子重构：trait 删旧加新后 handler 立即编译失败，必须连同 8 个 handler 一起改完才恢复绿。**先改 trait/实现，再逐个 handler，最后整体编译。**

**Files:**
- Modify: `src/service/domain/finance/mod.rs`（trait + Commands）
- Modify: `src/service/domain/finance/identity_credential.rs`（实现重写）
- Modify: `src/handlers/finance/lark_integration/create_credential.rs` / `update_credential.rs` / `delete_credential.rs` / `set_default_credential.rs`
- Modify: `src/handlers/finance/github_integration/create_credential.rs` / `update_credential.rs` / `delete_credential.rs` / `set_default_credential.rs`

- [ ] **Step 4.1: mod.rs — 定义 Commands 并替换 trait 方法**

在 `IdentityCredentialManage` trait 定义**之前**加 Commands（AGENTS.md 4.2：Command 定义在 domain mod.rs）：

```rust
/// 创建凭证命令（detail 为明文，Domain 内规范化 + 校验 + 加密落库）
#[derive(Debug, Clone)]
pub struct CreateCredentialCmd {
    /// 用户自命名
    pub name: String,
    /// 明文凭证详情（类型由变体决定）
    pub detail: common::models::CredentialDetail,
}

/// 更新凭证命令（patch 为明文；None 字段保持不变）
#[derive(Debug, Clone)]
pub struct UpdateCredentialCmd {
    /// 目标凭证 ID
    pub credential_id: String,
    /// 新名称（None/空白不变）
    pub name: Option<String>,
    /// 详情补丁（`Unchanged` 表示不动 detail）
    pub patch: common::models::CredentialDetailPatch,
}
```

trait 中**删除**以下 8 个方法声明（连同 doc 注释与 `#[allow(clippy::too_many_arguments)]`）：
`create_lark_credential` / `update_lark_credential` / `delete_lark_credential` / `set_default_lark_credential` / `create_github_credential` / `update_github_credential` / `delete_github_credential` / `set_default_github_credential`

同位置**新增**（保留 `get_identity_credentials` 与其余方法不动）：

```rust
    /// 创建凭证（类型由 cmd.detail 变体决定；敏感字段加密落库），返回凭证唯一 ID
    async fn create_credential(
        &self,
        ctx: RequestContext,
        user_id: &str,
        cmd: CreateCredentialCmd,
    ) -> Result<String>;

    /// 更新凭证（name/detail 按补丁语义；类型不匹配报错）
    ///
    /// 类型分发联动（失败仅告警）：
    /// - LarkApp：清该用户 HOME 的 lark-cli config + WS 监听移交
    ///   （secret 轮换强制断连重建）
    async fn update_credential(
        &self,
        ctx: RequestContext,
        user_id: &str,
        cmd: UpdateCredentialCmd,
    ) -> Result<()>;

    /// 删除凭证
    ///
    /// 类型分发前置检查：LarkApp 被渠道引用报 Conflict；
    /// 类型分发后置联动：GithubToken 删除的是生效凭证时清 HOME 登录态。
    async fn delete_credential(
        &self,
        ctx: RequestContext,
        user_id: &str,
        credential_id: &str,
    ) -> Result<()>;

    /// 设置默认凭证（各类型默认槽位独立）
    ///
    /// None/空白表示取消该类型默认；Some 校验凭证存在且类型匹配。
    async fn set_default_credential(
        &self,
        ctx: RequestContext,
        user_id: &str,
        kind: common::models::CredentialKind,
        credential_id: Option<&str>,
    ) -> Result<()>;
```

- [ ] **Step 4.2: identity_credential.rs — 重写实现**

删除 8 个旧实现（`create_lark_credential` 到 `set_default_github_credential`，即原文件 L49-L395 中对应方法；`get_identity_credentials`、`github_integration_status`、lark auth/bind 6 个保留），替换为：

```rust
    // ==================== 统一凭证 CRUD（类型差异经 detail 行为 + match kind 分发） ====================

    /// 创建凭证（明文 detail → 规范化/校验/加密落库）
    async fn create_credential(
        &self,
        ctx: RequestContext,
        user_id: &str,
        cmd: super::CreateCredentialCmd,
    ) -> Result<String> {
        let name = cmd.name.trim().to_string();
        if name.is_empty() {
            bail_err!(InvalidRequest, "凭证名称不能为空");
        }
        let detail = cmd.detail.normalized();
        detail.validate()?;
        let kind = detail.kind();
        let detail =
            detail.encrypt_sensitive(crate::pkg::crypto::encrypt_channel_secret)?;

        let user_dal = self.user_dal()?.clone();
        let mut library = self.load_credential_library(ctx.clone(), user_id).await?;
        let now = chrono::Utc::now().to_rfc3339();
        let credential_id = uuid::Uuid::now_v7().to_string();
        library.items.push(UserIdentityCredential {
            id: credential_id.clone(),
            kind,
            name,
            created_at: now.clone(),
            updated_at: now,
            detail,
        });
        user_dal
            .save_identity_credentials(ctx, user_id, &library)
            .await?;
        Ok(credential_id)
    }

    /// 更新凭证（补丁语义 + 类型分发联动）
    async fn update_credential(
        &self,
        ctx: RequestContext,
        user_id: &str,
        cmd: super::UpdateCredentialCmd,
    ) -> Result<()> {
        let user_dal = self.user_dal()?.clone();
        let mut library = self.load_credential_library(ctx.clone(), user_id).await?;
        let credential = library
            .find_by_id_mut(&cmd.credential_id)
            .ok_or_else(|| err!(NotFound, "凭证不存在 credential_id={}", cmd.credential_id))?;
        if let Some(n) = cmd.name.as_deref().filter(|s| !s.trim().is_empty()) {
            credential.name = n.trim().to_string();
        }
        let old_primary_id = credential.detail.primary_id().unwrap_or_default().to_string();
        let kind = credential.kind;
        let impact = credential
            .detail
            .apply_patch(cmd.patch, crate::pkg::crypto::encrypt_channel_secret)?;
        credential.updated_at = chrono::Utc::now().to_rfc3339();
        let new_primary_id = credential.detail.primary_id().unwrap_or_default().to_string();

        user_dal
            .save_identity_credentials(ctx, user_id, &library)
            .await?;

        // 类型分发：更新后联动（失败仅告警）
        if kind == CredentialKind::LarkApp
            && let Some(lark_dal) = &self.lark_channel_dal
        {
            let home = crate::pkg::tool_registry::lark_cli::lark_home(
                &crate::config::get().base_data_path(),
                user_id,
            );
            if let Err(e) = crate::pkg::tool_registry::lark_cli::clear_cli_config(&home).await {
                log_warn!(
                    "lark credential update: clear cli config failed (ignored): user_id={} err={}",
                    user_id,
                    e
                );
            }
            lark_dal
                .handover_listeners_after_credential_change(
                    &old_primary_id,
                    &new_primary_id,
                    impact.secret_changed,
                )
                .await;
        }
        // GithubToken：token 轮换无需显式清登录态（gh_cli marker 指纹机制自动重登录）
        Ok(())
    }

    /// 删除凭证（前置检查 + 后置联动均按类型分发）
    async fn delete_credential(
        &self,
        ctx: RequestContext,
        user_id: &str,
        credential_id: &str,
    ) -> Result<()> {
        let user_dal = self.user_dal()?.clone();
        let mut library = self.load_credential_library(ctx.clone(), user_id).await?;
        let Some(credential) = library.find_by_id(credential_id).cloned() else {
            bail_err!(NotFound, "凭证不存在 credential_id={}", credential_id);
        };

        // 类型分发：前置检查（Lark 渠道引用 / GitHub 生效凭证快照）
        let github_was_active = match credential.kind {
            CredentialKind::LarkApp => {
                if let Some(lark_dal) = &self.lark_channel_dal {
                    let channels = lark_dal
                        .find_channels_by_credential_id(credential_id)
                        .await?;
                    if !channels.is_empty() {
                        bail_err!(
                            Conflict,
                            "凭证被 {} 个渠道引用，请先删除或更换引用渠道",
                            channels.len()
                        );
                    }
                }
                false
            }
            CredentialKind::GithubToken => library
                .resolve_github_credential()
                .is_some_and(|c| c.id == credential_id),
        };

        library.remove_by_id(credential_id);
        // 删掉的凭证若恰为该类型默认，联动清除对应默认槽位
        library.clear_default_for(credential.kind, credential_id);
        user_dal
            .save_identity_credentials(ctx, user_id, &library)
            .await?;

        // 类型分发：后置联动（失败仅告警；剩余凭证存在时下次调用自动重建）
        if credential.kind == CredentialKind::GithubToken && github_was_active {
            let home = crate::pkg::tool_registry::gh_cli::gh_home(
                &crate::config::get().base_data_path(),
                user_id,
            );
            if let Err(e) = crate::pkg::tool_registry::gh_cli::clear_gh_auth(&home).await {
                log_warn!(
                    "github credential delete: clear gh auth failed (ignored): user_id={} err={}",
                    user_id,
                    e
                );
            }
        }
        // LarkApp：不联动删 HOME config（保留用户授权 token）
        Ok(())
    }

    /// 设置默认凭证（各类型默认槽位独立）
    async fn set_default_credential(
        &self,
        ctx: RequestContext,
        user_id: &str,
        kind: CredentialKind,
        credential_id: Option<&str>,
    ) -> Result<()> {
        let user_dal = self.user_dal()?.clone();
        let mut library = self.load_credential_library(ctx.clone(), user_id).await?;
        library.set_default_for(kind, credential_id.map(|s| s.to_string()))?;
        user_dal
            .save_identity_credentials(ctx, user_id, &library)
            .await
    }
```

> 编译注意：`encrypt_channel_secret` 作为函数项传入泛型 `F: Fn(&str) -> Result<String>` 时，若签名不完全匹配（如生命周期），包一层闭包 `|s: &str| crate::pkg::crypto::encrypt_channel_secret(s)`。

- [ ] **Step 4.3: 迁移 4 个 lark handler**

`src/handlers/finance/lark_integration/create_credential.rs` — 调用处替换为：

```rust
use crate::service::domain::finance::{CreateCredentialCmd, domain};
use common::models::CredentialDetail;

    let credential_id = domain()
        .identity_credential_manage()
        .create_credential(
            ctx,
            &user_id,
            CreateCredentialCmd {
                name: params.name,
                detail: CredentialDetail::LarkApp {
                    app_id: params.app_id,
                    app_secret: params.app_secret,
                    encrypt_key: params.encrypt_key,
                    verification_token: params.verification_token,
                },
            },
        )
        .await?;
```

> 以该文件现有 `params` 字段所有权为准：DTO 字段多为 `Option<String>` / `String`，直接移动；handler 内若有额外 trim/校验保留原样。

`update_credential.rs` — 调用处替换为：

```rust
use crate::service::domain::finance::{UpdateCredentialCmd, domain};
use common::models::CredentialDetailPatch;

    domain()
        .identity_credential_manage()
        .update_credential(
            ctx,
            &user_id,
            UpdateCredentialCmd {
                credential_id: params.id.clone(),
                name: params.name.clone(),
                patch: CredentialDetailPatch::LarkApp {
                    app_id: params.app_id.clone(),
                    app_secret: params.app_secret.clone(),
                    encrypt_key: params.encrypt_key.clone(),
                    verification_token: params.verification_token.clone(),
                },
            },
        )
        .await?;
```

> 保留该文件既有的 body/path id 一致性校验（若有）。字段名以实际 DTO 为准（`params.app_id` 等）。

`delete_credential.rs` — 调用处替换为：

```rust
    domain()
        .identity_credential_manage()
        .delete_credential(ctx, &user_id, &params.id)
        .await?;
```

`set_default_credential.rs` — 调用处替换为：

```rust
use common::models::CredentialKind;

    let trimmed = params.credential_id.trim().to_string();
    let target = if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    };
    domain()
        .identity_credential_manage()
        .set_default_credential(ctx, &user_id, CredentialKind::LarkApp, target.as_deref())
        .await?;
```

- [ ] **Step 4.4: 迁移 4 个 github handler**

`github_integration/create_credential.rs`：

```rust
use crate::service::domain::finance::{CreateCredentialCmd, domain};
use common::models::CredentialDetail;

    let credential_id = domain()
        .identity_credential_manage()
        .create_credential(
            ctx,
            &user_id,
            CreateCredentialCmd {
                name: params.name,
                detail: CredentialDetail::GithubToken {
                    token: params.token,
                },
            },
        )
        .await?;
```

`github_integration/update_credential.rs`：

```rust
use crate::service::domain::finance::{UpdateCredentialCmd, domain};
use common::models::CredentialDetailPatch;

    domain()
        .identity_credential_manage()
        .update_credential(
            ctx,
            &user_id,
            UpdateCredentialCmd {
                credential_id: params.id.clone(),
                name: params.name.clone(),
                patch: CredentialDetailPatch::GithubToken {
                    token: params.token.clone(),
                },
            },
        )
        .await?;
```

`github_integration/delete_credential.rs`：

```rust
    domain()
        .identity_credential_manage()
        .delete_credential(ctx, &user_id, &params.id)
        .await?;
```

`github_integration/set_default_credential.rs`：

```rust
use common::models::CredentialKind;

    let trimmed = params.credential_id.trim().to_string();
    let target = if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    };
    domain()
        .identity_credential_manage()
        .set_default_credential(ctx, &user_id, CredentialKind::GithubToken, target.as_deref())
        .await?;
```

- [ ] **Step 4.5: 整体编译**

```bash
cargo build 2>&1 | grep -E "^error" -A 5
```

预期：无 error（Trait/实现/handler 一次到位）。常见残留：未使用的 import（旧 `crypto` 引用等）按编译器提示清理。

- [ ] **Step 4.6: 跑 domain 层与模块测试**

```bash
cargo test --lib service::domain::finance
```

预期：全部 PASS。

---

### Task 5: 全量回归验证

**Files:** 无新改动（只验证；若集成测试断言了被统一掉的错误消息文案，按"行为保持要点 6"微调测试断言，**不改业务代码**）

- [ ] **Step 5.1: 后端全量 lib 测试**

```bash
cargo test --lib 2>&1 | grep "test result"
```

预期：0 failed（已知既有 flaky `test_sync_builtin_tools_to_db` 若偶发，单独复跑确认）。

- [ ] **Step 5.2: 集成测试（行为安全网，不改断言优先）**

```bash
cargo test --test lark_integration_test
cargo test --test github_integration_test
```

预期：全部 PASS。lark 侧重点观察：凭证 CRUD、set_default 4xx 断言、渠道引用 Conflict。

- [ ] **Step 5.3: clippy 零警告**

```bash
cargo clippy --lib --tests -- -D warnings
```

预期：通过（顺带确认旧 `#[allow(clippy::too_many_arguments)]` 删除后无新告警）。

- [ ] **Step 5.4: 前端回归（DTO/路由零改动，冒烟即可）**

```bash
cd frontend && cargo clippy --target wasm32-unknown-unknown -- -D warnings && cargo test
```

预期：通过（前端不依赖被改动的 domain 方法）。

- [ ] **Step 5.5: 提交（需用户确认）**

```bash
git add common/src/models/identity_credentials.rs \
  src/service/domain/finance/mod.rs \
  src/service/domain/finance/identity_credential.rs \
  src/handlers/finance/lark_integration/ \
  src/handlers/finance/github_integration/
git commit  # 消息示例：refactor(identity): 统一凭证 CRUD 为类型无关 domain 接口，字段知识下沉 CredentialDetail
```

---

## 验收清单（2026-08-15 全部达成 ✅）

- [x] trait `IdentityCredentialManage` 不再有 `*_lark_credential` / `*_github_credential` 命名的方法
- [x] 新增凭证类型（如微信）时：common 扩 `CredentialDetail`/`CredentialDetailPatch` 变体 + domain 两处 match 分支 + 1 个 handler 目录，**trait 零改动**
- [x] domain `identity_credential.rs` 不再直接出现 `encrypt_channel_secret` 字段级调用（仅作为原语传入模型方法）
- [x] 集成测试 lark/github 原样通过（行为安全网）
- [x] 前端零改动

---

## 执行结果（2026-08-15，子代理驱动）

| Task | 内容 | 验收 |
|------|------|------|
| Task 1 | `CredentialDetail::kind/primary_id/normalized/validate/encrypt_sensitive` + 4 测试 + `CredentialDetailPatch` 枚举 | common 模块 21 passed |
| Task 2 | `CredentialUpdateImpact` + `CredentialDetail::apply_patch` + 5 测试 | 同上 |
| Task 3 | `UserIdentityCredentials::set_default_for/clear_default_for/default_slot_mut` + 3 测试 | common 全量 72 passed；clippy 0 警告 |
| Task 4 | 删 8 个类型方法，换 4 个统一 CRUD（Command 化）+ 8 个 handler 迁移；lark WS 移交/渠道引用 Conflict 与 github clear_gh_auth 联动经 `match kind` 分发保持 | cargo build 一次过；`service::domain::finance` 22 passed |
| Task 5 | 全量回归：后端 lib 946 passed / 0 failed；集成 lark 3 + github 3；clippy 双端零错误；前端 82 passed | ✅ |

### 关键偏离记录（计划与落地 2 处，计划已预案）
1. `common::error::Error` 无 `Internal(String)` 变体，测试辅助改用现成 `Error::internal("test")` 构造器
2. `CredentialDetailPatch` 字段级补 `///` 注释（common crate 强制 `missing_docs`，原计划代码块缺失）

### 后续新增凭证类型的扩展路径（模式已验证）
1. `common::models::CredentialDetail` / `CredentialDetailPatch` 各加一个变体（字段知识全部在此描述）
2. `src/service/domain/finance/identity_credential.rs` 的 `delete_credential`（前置检查 + 后置副作用）与 `update_credential`（类型分发尾段）各加一条 `match` 分支
3. 新建 `src/handlers/finance/<name>_integration/`（参照 github_integration 5 文件模板）+ 路由挂载
4. 前端 api + 区块组件（参照 `identity_github.rs`）

**trait / DTO / 路由机制不变**。
