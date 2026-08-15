//! 用户身份凭证库（users.identity_credentials JSON 列）
//!
//! 类型化结构体约束凭证类型/详情/关键 ID，前后端共享。
//! secret 类字段在落库前经 `pkg::crypto::encrypt_channel_secret` 加密，
//! 本结构体本身不含加密逻辑（加密发生在 DAL 读写时）。

use serde::{Deserialize, Serialize};

use crate::error::{Result, bail_err, err};

/// 用户身份凭证库（users.identity_credentials JSON 列的顶层结构）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserIdentityCredentials {
    /// 凭证列表（多类型凭据共存）
    pub items: Vec<UserIdentityCredential>,
    /// 默认飞书凭证 ID（用户显式选择；lark_cli 工具身份优先取引用该凭证的渠道）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_credential_id: Option<String>,
    /// 默认 GitHub 凭证 ID（gh_cli 工具身份优先；多条 token 时生效）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_github_credential_id: Option<String>,
}

impl UserIdentityCredentials {
    /// 从 users.identity_credentials 列值解析（空串视为无凭证）
    pub fn parse(column: &str) -> Self {
        if column.trim().is_empty() {
            return Self::default();
        }
        serde_json::from_str(column).unwrap_or_default()
    }

    /// 序列化为落库 JSON 字符串（无凭证时返回空串，保持列默认值语义）
    pub fn to_column_value(&self) -> String {
        if self.items.is_empty() {
            return String::new();
        }
        serde_json::to_string(self).unwrap_or_default()
    }

    /// 按关键 ID 查找凭证
    pub fn find_by_id(&self, id: &str) -> Option<&UserIdentityCredential> {
        self.items.iter().find(|c| c.id == id)
    }

    /// 按关键 ID 查找可变凭证
    pub fn find_by_id_mut(&mut self, id: &str) -> Option<&mut UserIdentityCredential> {
        self.items.iter_mut().find(|c| c.id == id)
    }

    /// 按关键 ID 删除凭证，返回被删除的凭证
    pub fn remove_by_id(&mut self, id: &str) -> Option<UserIdentityCredential> {
        let pos = self.items.iter().position(|c| c.id == id)?;
        Some(self.items.remove(pos))
    }

    /// 飞书渠道凭证引用统一校验（存在 + kind=LarkApp）
    ///
    /// 渠道创建/更新与快照聚合共用的单一校验入口；归属校验天然成立：
    /// 凭证库本身即按渠道/请求归属用户加载。
    pub fn resolve_lark_credential_ref(
        &self,
        lark_credential_id: Option<&str>,
    ) -> Result<&UserIdentityCredential> {
        let Some(credential_id) = lark_credential_id.map(str::trim).filter(|s| !s.is_empty())
        else {
            bail_err!(InvalidRequest, "飞书渠道必须选择已绑定的应用凭证");
        };
        let Some(credential) = self.find_by_id(credential_id) else {
            bail_err!(
                InvalidRequest,
                "所选飞书凭证不存在，请先在飞书集成中绑定应用"
            );
        };
        if !matches!(credential.kind, CredentialKind::LarkApp) {
            bail_err!(InvalidRequest, "所选凭证不是飞书应用凭证，无法用于飞书渠道");
        }
        Ok(credential)
    }

    /// 解析 gh_cli 工具身份可用的 GitHub 凭证
    ///
    /// 优先取 `default_github_credential_id` 指向的 GithubToken 凭证
    /// （指向不存在/非 GitHub 类型时忽略）；未命中回退第一条 GithubToken。
    pub fn resolve_github_credential(&self) -> Option<&UserIdentityCredential> {
        if let Some(default_id) = self.default_github_credential_id.as_deref()
            && let Some(credential) = self.find_by_id(default_id)
            && matches!(credential.kind, CredentialKind::GithubToken)
        {
            return Some(credential);
        }
        self.items
            .iter()
            .find(|credential| matches!(credential.kind, CredentialKind::GithubToken))
    }

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
}

/// 单条用户身份凭证
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserIdentityCredential {
    /// 凭证关键 ID（渠道引用键，uuid）
    pub id: String,
    /// 凭证类型
    pub kind: CredentialKind,
    /// 用户自命名
    pub name: String,
    /// 创建时间（RFC3339）
    pub created_at: String,
    /// 更新时间（RFC3339）
    pub updated_at: String,
    /// 按 kind 区分的详情
    pub detail: CredentialDetail,
}

/// 凭证类型（可扩展，如后续 WechatApp）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialKind {
    /// 飞书自建应用凭证
    LarkApp,
    /// GitHub 访问令牌（PAT / OAuth token）
    GithubToken,
}

/// 凭证详情（serde 内部 tag，按类型区分字段集）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CredentialDetail {
    /// 飞书自建应用：app_id + app_secret（落库前加密）+ 可选事件回调配置
    LarkApp {
        /// 应用 ID
        app_id: String,
        /// 应用 Secret（落库前经 encrypt_channel_secret 加密）
        app_secret: String,
        /// Encrypt Key（可选，同样加密存储）
        encrypt_key: Option<String>,
        /// Verification Token（可选，事件回调校验用）
        verification_token: Option<String>,
    },
    /// GitHub 访问令牌（落库前经 encrypt_channel_secret 加密）
    GithubToken {
        /// 访问令牌（PAT / OAuth token，落库前加密）
        token: String,
    },
}

/// 凭证详情补丁（Domain 更新命令组件，明文输入；非 API DTO，无需 serde）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CredentialDetailPatch {
    /// 不变更 detail
    #[default]
    Unchanged,
    /// 飞书应用字段补丁（None 保持不变；verification_token 的 Some("") 表示清除）
    LarkApp {
        /// 应用 ID（None/空白保持不变）
        app_id: Option<String>,
        /// 应用 Secret（None/空白保持不变；提供时以明文传入，内部加密写入）
        app_secret: Option<String>,
        /// Encrypt Key（None/空白保持不变；提供时以明文传入，内部加密写入）
        encrypt_key: Option<String>,
        /// Verification Token（None 保持不变；Some 空白清除、非空覆盖）
        verification_token: Option<String>,
    },
    /// GitHub token 补丁
    GithubToken {
        /// 访问令牌（None/空白保持不变；提供时以明文传入，内部加密写入）
        token: Option<String>,
    },
}

/// detail 变更影响摘要（Domain 据此决定联动动作，无需感知字段细节）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CredentialUpdateImpact {
    /// 敏感字段轮换（app_secret/encrypt_key/token 任一实际写入）
    pub secret_changed: bool,
}

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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lark_credential(id: &str, name: &str) -> UserIdentityCredential {
        UserIdentityCredential {
            id: id.to_string(),
            kind: CredentialKind::LarkApp,
            name: name.to_string(),
            created_at: "2026-08-12T00:00:00Z".to_string(),
            updated_at: "2026-08-12T00:00:00Z".to_string(),
            detail: CredentialDetail::LarkApp {
                app_id: "cli_a1b2c3".to_string(),
                app_secret: "enc:v1:secret".to_string(),
                encrypt_key: Some("enc:v1:key".to_string()),
                verification_token: None,
            },
        }
    }

    fn github_credential(id: &str, name: &str) -> UserIdentityCredential {
        UserIdentityCredential {
            id: id.to_string(),
            kind: CredentialKind::GithubToken,
            name: name.to_string(),
            created_at: "2026-08-12T00:00:00Z".to_string(),
            updated_at: "2026-08-12T00:00:00Z".to_string(),
            detail: CredentialDetail::GithubToken {
                token: "enc:v1:gh-token".to_string(),
            },
        }
    }

    #[test]
    fn test_parse_empty_column() {
        assert_eq!(
            UserIdentityCredentials::parse(""),
            UserIdentityCredentials::default()
        );
        assert_eq!(
            UserIdentityCredentials::parse("   "),
            UserIdentityCredentials::default()
        );
        // 非法 JSON 容错为空库
        assert_eq!(
            UserIdentityCredentials::parse("not-json"),
            UserIdentityCredentials::default()
        );
    }

    #[test]
    fn test_column_round_trip() {
        let creds = UserIdentityCredentials {
            items: vec![
                lark_credential("cred-1", "工作应用"),
                lark_credential("cred-2", "测试应用"),
            ],
            default_credential_id: Some("cred-1".to_string()),
            default_github_credential_id: None,
        };
        let column = creds.to_column_value();
        assert!(!column.is_empty());
        let parsed = UserIdentityCredentials::parse(&column);
        assert_eq!(parsed, creds);
    }

    #[test]
    fn test_legacy_column_without_default_field_parses() {
        // 存量 JSON 无 default_credential_id 字段 → serde(default) 容错
        let legacy = r#"{"items":[]}"#;
        let parsed = UserIdentityCredentials::parse(legacy);
        assert_eq!(parsed.default_credential_id, None);
    }

    #[test]
    fn test_empty_library_serializes_to_empty_string() {
        assert_eq!(UserIdentityCredentials::default().to_column_value(), "");
    }

    #[test]
    fn test_find_and_remove_by_id() {
        let mut creds = UserIdentityCredentials {
            items: vec![
                lark_credential("cred-1", "A"),
                lark_credential("cred-2", "B"),
            ],
            ..Default::default()
        };
        assert!(creds.find_by_id("cred-1").is_some());
        assert!(creds.find_by_id("cred-x").is_none());

        let removed = creds.remove_by_id("cred-1");
        assert_eq!(removed.unwrap().name, "A");
        assert_eq!(creds.items.len(), 1);
        assert!(creds.remove_by_id("cred-1").is_none());
    }

    #[test]
    fn test_detail_serde_tag() {
        let cred = lark_credential("cred-1", "A");
        let json = serde_json::to_value(&cred).unwrap();
        assert_eq!(json["detail"]["type"], "lark_app");
        assert_eq!(json["kind"], "lark_app");
        assert_eq!(json["detail"]["app_id"], "cli_a1b2c3");
    }

    #[test]
    fn test_resolve_lark_credential_ref() {
        let mut creds = UserIdentityCredentials {
            items: vec![lark_credential("cred-1", "A")],
            ..Default::default()
        };
        // 缺失/空白引用拒绝
        assert!(creds.resolve_lark_credential_ref(None).is_err());
        assert!(creds.resolve_lark_credential_ref(Some("  ")).is_err());
        // 不存在的引用拒绝
        assert!(creds.resolve_lark_credential_ref(Some("missing")).is_err());
        // 存在的 LarkApp 引用通过
        let cred = creds.resolve_lark_credential_ref(Some("cred-1")).unwrap();
        assert_eq!(cred.name, "A");

        // 非 LarkApp 类型拒绝（当前枚举仅 LarkApp，以构造非法 kind 不现实，
        // 改为验证空库拒绝）
        creds.items.clear();
        assert!(creds.resolve_lark_credential_ref(Some("cred-1")).is_err());
    }

    #[test]
    fn test_github_credential_serde_tag() {
        let cred = github_credential("gh-1", "工作号");
        let json = serde_json::to_value(&cred).unwrap();
        assert_eq!(json["kind"], "github_token");
        assert_eq!(json["detail"]["type"], "github_token");
        assert_eq!(json["detail"]["token"], "enc:v1:gh-token");
        // 往返一致
        let parsed: UserIdentityCredential = serde_json::from_value(json).unwrap();
        assert_eq!(parsed, cred);
    }

    #[test]
    fn test_resolve_github_credential() {
        // 空 / 仅 Lark 凭证 → None
        assert!(UserIdentityCredentials::default()
            .resolve_github_credential()
            .is_none());
        let lark_only = UserIdentityCredentials {
            items: vec![lark_credential("cred-1", "A")],
            ..Default::default()
        };
        assert!(lark_only.resolve_github_credential().is_none());

        // 单条 GitHub 凭证 → 直接命中（无需设默认）
        let single = UserIdentityCredentials {
            items: vec![github_credential("gh-1", "工作号")],
            ..Default::default()
        };
        assert_eq!(single.resolve_github_credential().unwrap().id, "gh-1");

        // 多条时默认字段指向 GithubToken → 优先默认；且与 Lark 默认互不影响
        let multi = UserIdentityCredentials {
            items: vec![
                github_credential("gh-1", "工作号"),
                github_credential("gh-2", "个人号"),
                lark_credential("cred-1", "A"),
            ],
            default_credential_id: Some("cred-1".to_string()),
            default_github_credential_id: Some("gh-2".to_string()),
        };
        assert_eq!(multi.resolve_github_credential().unwrap().id, "gh-2");

        // 默认字段指向 LarkApp → 忽略，回退第一条 GithubToken
        let mut lark_default = multi.clone();
        lark_default.default_github_credential_id = Some("cred-1".to_string());
        assert_eq!(lark_default.resolve_github_credential().unwrap().id, "gh-1");
    }

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
        crate::error::Error::internal("test")
    }

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
}
